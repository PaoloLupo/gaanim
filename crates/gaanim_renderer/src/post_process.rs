//! Custom WGSL post-processing of everything Vello draws inside the camera frame.

use bevy::prelude::Resource;
use gaanim_core::kurbo;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use vello::wgpu;

const SHADER_PREAMBLE: &str = r#"
struct GaanimPostParams {
    // Camera frame origin (xy) and size (zw) in target pixels.
    frame: vec4<f32>,
    // Target size (xy) and timeline seconds (z).
    canvas: vec4<f32>,
    // Processed pixel region: origin (xy) and size (zw).
    region: vec4<u32>,
}

@group(0) @binding(0)
var gaanim_post_source: texture_2d<f32>;

@group(0) @binding(1)
var gaanim_post_sampler: sampler;

@group(0) @binding(2)
var gaanim_post_output: texture_storage_2d<rgba8unorm, write>;

@group(0) @binding(3)
var<uniform> gaanim_post_params: GaanimPostParams;

fn gaanim_scene(uv: vec2<f32>) -> vec4<f32> {
    let frame = gaanim_post_params.frame;
    let pixel = clamp(
        frame.xy + uv * frame.zw,
        frame.xy + vec2<f32>(0.5),
        frame.xy + frame.zw - vec2<f32>(0.5),
    );
    return textureSampleLevel(
        gaanim_post_source,
        gaanim_post_sampler,
        pixel / gaanim_post_params.canvas.xy,
        0.0,
    );
}
"#;

const SHADER_ENTRY_POINT: &str = r#"
@compute @workgroup_size(8, 8, 1)
fn gaanim_apply_post(@builtin(global_invocation_id) id: vec3<u32>) {
    let region = gaanim_post_params.region;
    if (id.x >= region.z || id.y >= region.w) {
        return;
    }
    let frame = gaanim_post_params.frame;
    let pixel = vec2<f32>(region.xy + id.xy) + vec2<f32>(0.5);
    let uv = (pixel - frame.xy) / frame.zw;
    let color = gaanim_post(uv, frame.zw, gaanim_post_params.canvas.z);
    textureStore(gaanim_post_output, id.xy, clamp(color, vec4<f32>(0.0), vec4<f32>(1.0)));
}
"#;

const PARAMS_SIZE: u64 = 48;

/// A WGSL function applied to the rendered 2D scene inside the camera frame.
///
/// `source` must define
/// `fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32>`
/// and may call `gaanim_scene(uv)` to sample the rendered scene. `uv` is
/// normalized with (0, 0) at the top-left corner of the camera frame,
/// `resolution` is the frame size in pixels and `time` is absolute timeline
/// seconds. Colors are straight-alpha sRGB values as stored in the target.
#[derive(Clone)]
pub struct PostProcessShader {
    source: Arc<str>,
}

impl fmt::Debug for PostProcessShader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("PostProcessShader");
        debug.field("source_len", &self.source.len());
        if gaanim_core::fingerprint::identity_debug() {
            debug.field("source", &self.source);
        }
        debug.finish_non_exhaustive()
    }
}

impl PostProcessShader {
    pub fn new(source: impl Into<Arc<str>>) -> Result<Self, PostProcessError> {
        let source = source.into();
        validate_post_source(&source)?;
        Ok(Self { source })
    }

    /// Load WGSL source from an asset file. Relative paths are resolved by the caller.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, PostProcessError> {
        let path = path.as_ref().to_path_buf();
        let source =
            std::fs::read_to_string(&path).map_err(|error| PostProcessError::ReadSource {
                path,
                message: error.to_string(),
            })?;
        Self::new(source)
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Clone, Debug, Error)]
pub enum PostProcessError {
    #[error("could not read post-process WGSL asset '{path}': {message}")]
    ReadSource { path: PathBuf, message: String },
    #[error("invalid post-process WGSL: {0}")]
    InvalidWgsl(String),
}

/// Post-processing selected by one authored segment.
#[derive(Clone, Debug, Default)]
pub enum PostProcessOverride {
    /// Use the scene post-process.
    #[default]
    Inherit,
    /// Draw this segment without post-processing.
    Disabled,
    /// Replace the scene post-process while this segment is active.
    Shader(PostProcessShader),
}

/// Post-process override and time range of one authored segment.
#[derive(Clone, Debug)]
pub struct SegmentPostProcess {
    pub start_time: f64,
    pub end_time: f64,
    /// A terminal stop keeps the outgoing segment active at its shared boundary.
    pub hold_at_end: bool,
    pub post: PostProcessOverride,
}

/// Scene post-process and per-segment overrides, inserted by scene compilation.
#[derive(Resource, Clone, Debug, Default)]
pub struct CanvasPostProcess {
    pub shader: Option<PostProcessShader>,
    pub segments: Vec<SegmentPostProcess>,
}

impl CanvasPostProcess {
    /// Post-process active at an exact timeline position.
    pub fn shader_at(&self, time_seconds: f64) -> Option<&PostProcessShader> {
        let segment = crate::pipeline::active_segment(&self.segments, time_seconds, |segment| {
            (segment.start_time, segment.end_time, segment.hold_at_end)
        });
        match segment.map(|segment| &segment.post) {
            None | Some(PostProcessOverride::Inherit) => self.shader.as_ref(),
            Some(PostProcessOverride::Disabled) => None,
            Some(PostProcessOverride::Shader(shader)) => Some(shader),
        }
    }

    /// Frame to post-process at `time_seconds`, with the camera frame in
    /// target pixels. `None` when nothing applies.
    pub fn request(&self, time_seconds: f64, frame: kurbo::Rect) -> Option<PostProcessRequest> {
        let time = time_seconds as f32;
        if !time.is_finite() || !(frame.width() > 0.0 && frame.height() > 0.0) {
            return None;
        }
        self.shader_at(time_seconds)
            .map(|shader| PostProcessRequest {
                shader: shader.clone(),
                frame,
                time,
            })
    }
}

/// One frame of post-processing for [`GpuPostProcess`].
#[derive(Clone, Debug)]
pub struct PostProcessRequest {
    pub shader: PostProcessShader,
    /// Camera frame in target pixels (top-left origin).
    pub frame: kurbo::Rect,
    /// Timeline seconds.
    pub time: f32,
}

struct PostPipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

struct PreparedFrame {
    pipeline: Arc<PostPipeline>,
    bind_group: wgpu::BindGroup,
    target: wgpu::Texture,
    /// Origin and size of the processed region in target pixels.
    region: [u32; 4],
}

/// Applies a [`PostProcessRequest`] to a render target on the GPU.
///
/// The shader writes a scratch texture the size of the camera frame, which is
/// then copied back over the frame; pixels outside the frame are untouched.
/// Used by both the interactive render world and the direct export.
#[derive(Default)]
pub struct GpuPostProcess {
    device: Option<wgpu::Device>,
    /// Pipelines by shader source; `None` records a failed build.
    pipelines: HashMap<Arc<str>, Option<Arc<PostPipeline>>>,
    sampler: Option<wgpu::Sampler>,
    params: Option<wgpu::Buffer>,
    scratch: Option<wgpu::Texture>,
    frame: Option<PreparedFrame>,
}

impl GpuPostProcess {
    /// Prepare `request` for `target`, which must be an `Rgba8Unorm` texture
    /// with `TEXTURE_BINDING` and `COPY_DST` usage. Returns whether
    /// [`Self::encode`] will draw a pass.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::Texture,
        request: Option<&PostProcessRequest>,
    ) -> bool {
        self.frame = None;
        if self.device.as_ref() != Some(device) {
            *self = Self {
                device: Some(device.clone()),
                ..Self::default()
            };
        }
        let Some(request) = request else {
            return false;
        };
        let Some(region) = frame_region(request.frame, target.width(), target.height()) else {
            return false;
        };
        let Some(pipeline) = self.pipeline(device, &request.shader.source) else {
            return false;
        };

        let scratch = match &self.scratch {
            Some(scratch) if scratch.width() == region[2] && scratch.height() == region[3] => {
                scratch.clone()
            }
            _ => {
                let scratch = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("gaanim-post-process-scratch"),
                    size: wgpu::Extent3d {
                        width: region[2],
                        height: region[3],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                self.scratch = Some(scratch.clone());
                scratch
            }
        };
        let params = self
            .params
            .get_or_insert_with(|| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("gaanim-post-process-params"),
                    size: PARAMS_SIZE,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .clone();
        let sampler = self
            .sampler
            .get_or_insert_with(|| {
                device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("gaanim-post-process-sampler"),
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                })
            })
            .clone();
        queue.write_buffer(
            &params,
            0,
            &params_bytes(request, target.width(), target.height(), region),
        );

        let source_view = target.create_view(&Default::default());
        let output_view = scratch.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-post-process-bind-group"),
            layout: &pipeline.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        self.frame = Some(PreparedFrame {
            pipeline,
            bind_group,
            target: target.clone(),
            region,
        });
        true
    }

    /// Drop the pending frame, keeping device resources for later frames.
    pub fn clear(&mut self) {
        self.frame = None;
    }

    /// Record the pass prepared by [`Self::prepare`], if any.
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(frame), Some(scratch)) = (&self.frame, &self.scratch) else {
            return;
        };
        let [x, y, width, height] = frame.region;
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("gaanim-post-process-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&frame.pipeline.pipeline);
            pass.set_bind_group(0, &frame.bind_group, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        encoder.copy_texture_to_texture(
            scratch.as_image_copy(),
            wgpu::TexelCopyTextureInfo {
                texture: &frame.target,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn pipeline(&mut self, device: &wgpu::Device, source: &Arc<str>) -> Option<Arc<PostPipeline>> {
        if let Some(cached) = self.pipelines.get(source) {
            return cached.clone();
        }
        // Keep only the current shader; a hot reload replaces it.
        self.pipelines.clear();
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = PostPipeline::new(device, source);
        let pipeline = match pollster::block_on(error_scope.pop()) {
            None => Some(Arc::new(pipeline)),
            Some(error) => {
                bevy::log::error!("post-process shader failed; drawing without it: {error}");
                None
            }
        };
        self.pipelines.insert(source.clone(), pipeline.clone());
        pipeline
    }
}

impl PostPipeline {
    fn new(device: &wgpu::Device, source: &str) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-post-process-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(complete_shader(source))),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-post-process-layout"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    2,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                ),
                entry(
                    3,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-post-process-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gaanim-post-process-pipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("gaanim_apply_post"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self { layout, pipeline }
    }
}

/// Whole target pixels covered by `frame`, as origin and size.
fn frame_region(frame: kurbo::Rect, width: u32, height: u32) -> Option<[u32; 4]> {
    let x0 = frame.x0.round().clamp(0.0, f64::from(width));
    let y0 = frame.y0.round().clamp(0.0, f64::from(height));
    let x1 = frame.x1.round().clamp(0.0, f64::from(width));
    let y1 = frame.y1.round().clamp(0.0, f64::from(height));
    (x1 > x0 && y1 > y0).then_some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
}

fn params_bytes(
    request: &PostProcessRequest,
    width: u32,
    height: u32,
    region: [u32; 4],
) -> [u8; PARAMS_SIZE as usize] {
    let frame = request.frame;
    let floats = [
        frame.x0 as f32,
        frame.y0 as f32,
        frame.width() as f32,
        frame.height() as f32,
        width as f32,
        height as f32,
        request.time,
        0.0,
    ];
    let mut bytes = [0_u8; PARAMS_SIZE as usize];
    for (chunk, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(floats) {
        chunk.copy_from_slice(&value.to_ne_bytes());
    }
    for (chunk, value) in bytes[32..].as_chunks_mut::<4>().0.iter_mut().zip(region) {
        chunk.copy_from_slice(&value.to_ne_bytes());
    }
    bytes
}

fn complete_shader(source: &str) -> String {
    format!("{SHADER_PREAMBLE}\n{source}\n{SHADER_ENTRY_POINT}")
}

fn validate_post_source(source: &str) -> Result<(), PostProcessError> {
    if !source.contains("gaanim_post") {
        return Err(PostProcessError::InvalidWgsl(
            "source must define gaanim_post(uv, resolution, time)".to_string(),
        ));
    }
    let complete = complete_shader(source);
    let module = naga::front::wgsl::parse_str(&complete)
        .map_err(|error| PostProcessError::InvalidWgsl(error.emit_to_string(&complete)))?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| PostProcessError::InvalidWgsl(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::test_gpu::test_gpu;

    const INVERT: &str = "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
         let color = gaanim_scene(uv + vec2<f32>(1.0, 0.0) / resolution);\n\
         return vec4<f32>(vec3<f32>(1.0) - color.rgb, color.a) + time * 0.0;\n}";

    fn shader() -> PostProcessShader {
        PostProcessShader::new(INVERT).unwrap()
    }

    #[test]
    fn validation_accepts_the_documented_contract() {
        assert!(shader().source().contains("gaanim_scene"));
    }

    #[test]
    fn validation_rejects_missing_or_mistyped_entry_functions() {
        assert!(matches!(
            PostProcessShader::new("fn other() {}"),
            Err(PostProcessError::InvalidWgsl(_))
        ));
        let wrong = "fn gaanim_post(uv: vec2<f32>) -> vec4<f32> { return gaanim_scene(uv); }";
        assert!(matches!(
            PostProcessShader::new(wrong),
            Err(PostProcessError::InvalidWgsl(_))
        ));
    }

    #[test]
    fn source_can_be_loaded_from_an_asset_file() {
        let path =
            std::env::temp_dir().join(format!("gaanim_post_process_{}.wgsl", std::process::id()));
        std::fs::write(&path, INVERT).unwrap();
        assert_eq!(
            PostProcessShader::from_file(&path).unwrap().source(),
            INVERT
        );
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            PostProcessShader::from_file(&path),
            Err(PostProcessError::ReadSource { .. })
        ));
    }

    #[test]
    fn segments_inherit_disable_or_replace_the_scene_post_process() {
        let scene = shader();
        let other = PostProcessShader::new(
            "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return gaanim_scene(uv);\n}",
        )
        .unwrap();
        let segment = |start, end, post| SegmentPostProcess {
            start_time: start,
            end_time: end,
            hold_at_end: false,
            post,
        };
        let post = CanvasPostProcess {
            shader: Some(scene.clone()),
            segments: vec![
                segment(0.0, 1.0, PostProcessOverride::Inherit),
                segment(1.0, 2.0, PostProcessOverride::Disabled),
                segment(2.0, 3.0, PostProcessOverride::Shader(other.clone())),
            ],
        };
        assert_eq!(post.shader_at(0.5).unwrap().source(), scene.source());
        assert!(post.shader_at(1.5).is_none());
        assert_eq!(post.shader_at(2.5).unwrap().source(), other.source());
        assert_eq!(post.shader_at(9.0).unwrap().source(), scene.source());

        let frame = kurbo::Rect::new(0.0, 0.0, 16.0, 9.0);
        assert!(post.request(1.5, frame).is_none());
        assert!(post.request(0.5, kurbo::Rect::ZERO).is_none());
        assert!(post.request(f64::NAN, frame).is_none());
        assert_eq!(post.request(0.5, frame).unwrap().time, 0.5);
    }

    #[test]
    fn frame_regions_are_clipped_to_the_target() {
        assert_eq!(
            frame_region(kurbo::Rect::new(-4.0, 2.4, 70.0, 20.6), 64, 36),
            Some([0, 2, 64, 19])
        );
        assert_eq!(
            frame_region(kurbo::Rect::new(80.0, 0.0, 90.0, 10.0), 64, 36),
            None
        );
    }

    #[test]
    fn gpu_pass_processes_only_the_camera_frame() {
        let Some(gpu) = test_gpu() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let (width, height) = (64_u32, 36_u32);
        // Vertical stripes: the red channel encodes the column.
        let pixels: Vec<u8> = (0..height)
            .flat_map(|_| (0..width).flat_map(|x| [(x * 4) as u8, 64, 200, 255]))
            .collect();
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            target.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            target.size(),
        );

        let frame = kurbo::Rect::new(8.0, 4.0, 40.0, 28.0);
        let request = CanvasPostProcess {
            shader: Some(shader()),
            segments: Vec::new(),
        }
        .request(1.0, frame)
        .unwrap();
        let mut post = GpuPostProcess::default();
        assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request)));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        post.encode(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        let out = gpu.read(&target);

        let at = |x: u32, y: u32| &out[((y * width + x) * 4) as usize..][..4];
        // Outside the frame nothing changes.
        assert_eq!(at(2, 2), &[8, 64, 200, 255]);
        assert_eq!(at(50, 30), &[200, 64, 200, 255]);
        // The top-left frame pixel is uv ~ (0, 0) and reads its right-hand
        // neighbour (x = 9) through gaanim_scene, inverted.
        assert_eq!(at(8, 4), &[255 - 36, 255 - 64, 255 - 200, 255]);
        // The last column clamps its sample to the frame edge (x = 39).
        assert_eq!(at(39, 27), &[255 - 156, 255 - 64, 255 - 200, 255]);

        assert!(!post.prepare(&gpu.device, &gpu.queue, &target, None));
    }
}
