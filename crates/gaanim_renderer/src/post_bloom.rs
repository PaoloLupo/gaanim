//! The mip chain behind bloom post-process passes.
//!
//! A bloom pass first extracts the bright part of its input into a
//! half-resolution texture, then halves it again level by level with a
//! 13-tap filter and adds the levels back up with a tent filter, the way
//! Call of Duty's and Blender's bloom do. The pass's own shader then reads
//! the result with `gaanim_bloom(uv)`. Every level is sized from the camera
//! frame, so the glow spreads over the same fraction of the frame at any
//! export resolution.

use std::borrow::Cow;
use vello::wgpu;

/// Declarations that a bloom composite pass sees before its source.
pub(crate) const BLOOM_COMPOSITE_PREAMBLE: &str = r#"
@group(0) @binding(6)
var gaanim_bloom_texture: texture_2d<f32>;

// Bloom of the chain's input at `uv`, in linear light and premultiplied.
fn gaanim_bloom(uv: vec2<f32>) -> vec3<f32> {
    let frame = gaanim_post_params.frame;
    let region = vec4<f32>(gaanim_post_params.region);
    let local = (frame.xy + uv * frame.zw - region.xy) / region.zw;
    return textureSampleLevel(gaanim_bloom_texture, gaanim_post_sampler, local, 0.0).rgb;
}
"#;

const STAGES: &str = r#"
struct GaanimBloomStage {
    // Sampled source region: origin (xy) and size (zw) in source texels.
    source: vec4<f32>,
    // Source texture size (xy), threshold (z) and scatter (w).
    extent: vec4<f32>,
}

@group(0) @binding(0)
var bloom_source: texture_2d<f32>;

@group(0) @binding(1)
var bloom_sampler: sampler;

@group(0) @binding(2)
var bloom_output: texture_storage_2d<rgba16float, write>;

@group(0) @binding(3)
var<uniform> bloom_stage: GaanimBloomStage;

// The finer level that an upsample adds the coarser one to.
@group(0) @binding(4)
var bloom_base: texture_2d<f32>;

fn bloom_linear(color: vec3<f32>) -> vec3<f32> {
    return select(
        pow((color + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        color / 12.92,
        color <= vec3<f32>(0.04045),
    );
}

// Premultiplied linear light above the threshold, with a soft knee.
fn bloom_bright(color: vec4<f32>) -> vec3<f32> {
    let threshold = bloom_stage.extent.z;
    let brightness = max(color.r, max(color.g, color.b));
    let knee = max(threshold * 0.5, 1e-4);
    var soft = clamp(brightness - threshold + knee, 0.0, 2.0 * knee);
    soft = soft * soft / (4.0 * knee);
    let contribution = max(soft, brightness - threshold) / max(brightness, 1e-4);
    return bloom_linear(color.rgb) * color.a * max(contribution, 0.0);
}

fn bloom_tap(center: vec2<f32>, offset: vec2<f32>, prefilter: bool) -> vec3<f32> {
    let source = bloom_stage.source;
    let texel = clamp(
        center + offset,
        source.xy + vec2<f32>(0.5),
        source.xy + source.zw - vec2<f32>(0.5),
    );
    let color = textureSampleLevel(bloom_source, bloom_sampler, texel / bloom_stage.extent.xy, 0.0);
    if (prefilter) {
        return bloom_bright(color);
    }
    return color.rgb;
}

fn bloom_downsample(id: vec2<u32>, prefilter: bool) {
    let size = textureDimensions(bloom_output);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let source = bloom_stage.source;
    let center = source.xy + (vec2<f32>(id) + vec2<f32>(0.5)) / vec2<f32>(size) * source.zw;
    let a = bloom_tap(center, vec2<f32>(-2.0, -2.0), prefilter);
    let b = bloom_tap(center, vec2<f32>(0.0, -2.0), prefilter);
    let c = bloom_tap(center, vec2<f32>(2.0, -2.0), prefilter);
    let d = bloom_tap(center, vec2<f32>(-2.0, 0.0), prefilter);
    let e = bloom_tap(center, vec2<f32>(0.0, 0.0), prefilter);
    let f = bloom_tap(center, vec2<f32>(2.0, 0.0), prefilter);
    let g = bloom_tap(center, vec2<f32>(-2.0, 2.0), prefilter);
    let h = bloom_tap(center, vec2<f32>(0.0, 2.0), prefilter);
    let i = bloom_tap(center, vec2<f32>(2.0, 2.0), prefilter);
    let j = bloom_tap(center, vec2<f32>(-1.0, -1.0), prefilter);
    let k = bloom_tap(center, vec2<f32>(1.0, -1.0), prefilter);
    let l = bloom_tap(center, vec2<f32>(-1.0, 1.0), prefilter);
    let m = bloom_tap(center, vec2<f32>(1.0, 1.0), prefilter);
    let color = e * 0.125
        + (a + c + g + i) * 0.03125
        + (b + d + f + h) * 0.0625
        + (j + k + l + m) * 0.125;
    textureStore(bloom_output, id, vec4<f32>(color, 1.0));
}

@compute @workgroup_size(8, 8, 1)
fn bloom_prefilter(@builtin(global_invocation_id) id: vec3<u32>) {
    bloom_downsample(id.xy, true);
}

@compute @workgroup_size(8, 8, 1)
fn bloom_down(@builtin(global_invocation_id) id: vec3<u32>) {
    bloom_downsample(id.xy, false);
}

@compute @workgroup_size(8, 8, 1)
fn bloom_up(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(bloom_output);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let uv = (vec2<f32>(id.xy) + vec2<f32>(0.5)) / vec2<f32>(size);
    let texel_step = 1.0 / bloom_stage.extent.xy;
    var blurred = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let weight = f32((2 - abs(x)) * (2 - abs(y))) / 16.0;
            let at = uv + vec2<f32>(f32(x), f32(y)) * texel_step;
            blurred += textureSampleLevel(bloom_source, bloom_sampler, at, 0.0).rgb * weight;
        }
    }
    let base = textureLoad(bloom_base, vec2<i32>(id.xy), 0).rgb;
    textureStore(bloom_output, id.xy, vec4<f32>(mix(base, blurred, bloom_stage.extent.w), 1.0));
}
"#;

const STAGE_SIZE: u64 = 32;

/// Most levels in a chain; the coarsest is 1/512 of the frame.
const MAX_LEVELS: u32 = 9;

/// The three bloom stage pipelines of one device.
pub(crate) struct BloomPipelines {
    down_layout: wgpu::BindGroupLayout,
    up_layout: wgpu::BindGroupLayout,
    prefilter: wgpu::ComputePipeline,
    down: wgpu::ComputePipeline,
    up: wgpu::ComputePipeline,
}

impl BloomPipelines {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-bloom-stages"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(STAGES)),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let texture = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let down_entries = [
            entry(0, texture),
            entry(
                1,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
            entry(
                2,
                wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba16Float,
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
        ];
        let down_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-bloom-down-layout"),
            entries: &down_entries,
        });
        let mut up_entries = down_entries.to_vec();
        up_entries.push(entry(4, texture));
        let up_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-bloom-up-layout"),
            entries: &up_entries,
        });
        let pipeline = |layout: &wgpu::BindGroupLayout, entry_point: &str| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gaanim-bloom-pipeline-layout"),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("gaanim-bloom-pipeline"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            prefilter: pipeline(&down_layout, "bloom_prefilter"),
            down: pipeline(&down_layout, "bloom_down"),
            up: pipeline(&up_layout, "bloom_up"),
            down_layout,
            up_layout,
        }
    }
}

/// Textures and stage uniforms of one bloom pass, reused while the processed
/// region keeps its size.
pub(crate) struct BloomChain {
    region: (u32, u32),
    /// Successively halved levels; `down[0]` is half the region.
    down: Vec<wgpu::Texture>,
    /// `up[k]` is `down[k]` plus the coarser levels, for `k < down.len() - 1`.
    up: Vec<wgpu::Texture>,
    /// Uniforms of each down stage, then of each up stage.
    stages: Vec<wgpu::Buffer>,
    /// View of the finest finished level, which the composite pass binds.
    result_view: wgpu::TextureView,
    /// The stages' bind groups, kept while the chain reads the same source.
    bound: Option<BoundChain>,
}

/// Bind groups of a chain's stages and the source and sampler they bind.
struct BoundChain {
    source: wgpu::Texture,
    sampler: wgpu::Sampler,
    dispatches: Vec<BloomDispatch>,
}

/// One recorded dispatch of the chain.
#[derive(Clone)]
pub(crate) struct BloomDispatch {
    kind: StageKind,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
}

#[derive(Clone, Copy)]
enum StageKind {
    Prefilter,
    Down,
    Up,
}

impl BloomChain {
    /// Levels for a `width`x`height` region: halve until the coarsest level
    /// is about 1/256 of the frame height, so the glow scales with the frame.
    fn level_sizes(width: u32, height: u32) -> Vec<(u32, u32)> {
        let shortest = width.min(height).max(1);
        let levels = (shortest.ilog2().saturating_sub(3)).clamp(1, MAX_LEVELS);
        let mut sizes = Vec::with_capacity(levels as usize);
        let (mut w, mut h) = (width, height);
        for _ in 0..levels {
            w = w.div_ceil(2).max(1);
            h = h.div_ceil(2).max(1);
            sizes.push((w, h));
        }
        sizes
    }

    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let sizes = Self::level_sizes(width, height);
        let texture = |(w, h): (u32, u32)| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("gaanim-bloom-level"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let down: Vec<_> = sizes.iter().copied().map(texture).collect();
        let up: Vec<_> = sizes[..sizes.len() - 1]
            .iter()
            .copied()
            .map(texture)
            .collect();
        let stages = (0..down.len() + up.len())
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("gaanim-bloom-stage"),
                    size: STAGE_SIZE,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let result_view = up
            .first()
            .unwrap_or(&down[0])
            .create_view(&Default::default());
        Self {
            region: (width, height),
            down,
            up,
            stages,
            result_view,
            bound: None,
        }
    }

    pub(crate) fn fits(&self, width: u32, height: u32) -> bool {
        self.region == (width, height)
    }

    /// A view of the finest finished level, which the composite pass
    /// samples.
    pub(crate) fn result_view(&self) -> &wgpu::TextureView {
        &self.result_view
    }

    /// Forget the bind groups, and with them the source they read.
    pub(crate) fn unbind(&mut self) {
        self.bound = None;
    }

    /// Write this frame's stage uniforms and return the dispatches of a
    /// chain that reads `region` of `source`. Bind groups are rebuilt only
    /// when `source` or `sampler` changes.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipelines: &BloomPipelines,
        sampler: &wgpu::Sampler,
        source: &wgpu::Texture,
        region: [u32; 4],
        threshold: f32,
        radius: f32,
    ) -> Vec<BloomDispatch> {
        let threshold = if threshold.is_finite() {
            threshold.clamp(0.0, 1.0)
        } else {
            0.8
        };
        let radius = if radius.is_finite() {
            radius.clamp(0.0, 1.0)
        } else {
            0.5
        };
        let scatter = 0.35 + 0.6 * radius;
        let size = |texture: &wgpu::Texture| (texture.width(), texture.height());
        let mut stages = self.stages.iter();

        let mut input = (
            [
                region[0] as f32,
                region[1] as f32,
                region[2] as f32,
                region[3] as f32,
            ],
            size(source),
        );
        for output in &self.down {
            let stage = stages.next().expect("one stage per level");
            let (rect, extent) = input;
            queue.write_buffer(stage, 0, &stage_bytes(rect, extent, threshold, scatter));
            let (w, h) = size(output);
            input = ([0.0, 0.0, w as f32, h as f32], (w, h));
        }
        for index in (0..self.up.len()).rev() {
            let stage = stages.next().expect("one stage per level");
            let coarser = self.up.get(index + 1).unwrap_or(&self.down[index + 1]);
            let extent = size(coarser);
            queue.write_buffer(
                stage,
                0,
                &stage_bytes(
                    [0.0, 0.0, extent.0 as f32, extent.1 as f32],
                    extent,
                    threshold,
                    scatter,
                ),
            );
        }

        if let Some(bound) = &self.bound
            && bound.source == *source
            && bound.sampler == *sampler
        {
            return bound.dispatches.clone();
        }
        let dispatches = self.bind(device, pipelines, sampler, source);
        self.bound = Some(BoundChain {
            source: source.clone(),
            sampler: sampler.clone(),
            dispatches: dispatches.clone(),
        });
        dispatches
    }

    /// Build the stages' bind groups, in the order of [`Self::stages`].
    fn bind(
        &self,
        device: &wgpu::Device,
        pipelines: &BloomPipelines,
        sampler: &wgpu::Sampler,
        source: &wgpu::Texture,
    ) -> Vec<BloomDispatch> {
        let size = |texture: &wgpu::Texture| (texture.width(), texture.height());
        let mut dispatches = Vec::with_capacity(self.stages.len());
        let mut stages = self.stages.iter();

        let mut input = source.create_view(&Default::default());
        for (level, output) in self.down.iter().enumerate() {
            let stage = stages.next().expect("one stage per level");
            let output_view = output.create_view(&Default::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gaanim-bloom-down"),
                layout: &pipelines.down_layout,
                entries: &[
                    texture_entry(0, &input),
                    sampler_entry(sampler),
                    texture_entry(2, &output_view),
                    buffer_entry(3, stage),
                ],
            });
            dispatches.push(BloomDispatch {
                kind: if level == 0 {
                    StageKind::Prefilter
                } else {
                    StageKind::Down
                },
                bind_group,
                size: size(output),
            });
            input = output_view;
        }

        for (index, output) in self.up.iter().enumerate().rev() {
            let stage = stages.next().expect("one stage per level");
            let coarser = self.up.get(index + 1).unwrap_or(&self.down[index + 1]);
            let coarser_view = coarser.create_view(&Default::default());
            let base_view = self.down[index].create_view(&Default::default());
            let output_view = output.create_view(&Default::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gaanim-bloom-up"),
                layout: &pipelines.up_layout,
                entries: &[
                    texture_entry(0, &coarser_view),
                    sampler_entry(sampler),
                    texture_entry(2, &output_view),
                    buffer_entry(3, stage),
                    texture_entry(4, &base_view),
                ],
            });
            dispatches.push(BloomDispatch {
                kind: StageKind::Up,
                bind_group,
                size: size(output),
            });
        }
        dispatches
    }
}

/// Record `dispatches` in order.
pub(crate) fn encode(
    encoder: &mut wgpu::CommandEncoder,
    pipelines: &BloomPipelines,
    dispatches: &[BloomDispatch],
) {
    for dispatch in dispatches {
        let mut compute = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("gaanim-bloom-stage"),
            timestamp_writes: None,
        });
        compute.set_pipeline(match dispatch.kind {
            StageKind::Prefilter => &pipelines.prefilter,
            StageKind::Down => &pipelines.down,
            StageKind::Up => &pipelines.up,
        });
        compute.set_bind_group(0, &dispatch.bind_group, &[]);
        compute.dispatch_workgroups(dispatch.size.0.div_ceil(8), dispatch.size.1.div_ceil(8), 1);
    }
}

fn stage_bytes(
    rect: [f32; 4],
    extent: (u32, u32),
    threshold: f32,
    scatter: f32,
) -> [u8; STAGE_SIZE as usize] {
    let floats = [
        rect[0],
        rect[1],
        rect[2],
        rect[3],
        extent.0 as f32,
        extent.1 as f32,
        threshold,
        scatter,
    ];
    let mut bytes = [0_u8; STAGE_SIZE as usize];
    for (chunk, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(floats) {
        chunk.copy_from_slice(&value.to_ne_bytes());
    }
    bytes
}

fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

fn sampler_entry(sampler: &wgpu::Sampler) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding: 1,
        resource: wgpu::BindingResource::Sampler(sampler),
    }
}

fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_scale_with_the_frame() {
        let hd = BloomChain::level_sizes(1920, 1080);
        assert_eq!(hd.first(), Some(&(960, 540)));
        assert_eq!(hd.len(), 7);
        assert_eq!(BloomChain::level_sizes(3840, 2160).len(), 8);
        assert_eq!(BloomChain::level_sizes(16, 9), vec![(8, 5)]);
        assert_eq!(BloomChain::level_sizes(1, 1), vec![(1, 1)]);
    }

    #[test]
    fn stage_shaders_are_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(STAGES).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
