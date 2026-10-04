use gaanim_core::peniko::{Blob, Brush, Color, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use thiserror::Error;
use vello::wgpu;
use wgpu::util::DeviceExt;

use crate::gpu_scope::ScopeCheck;

const SHADER_PREAMBLE: &str = r#"
@group(0) @binding(0)
var gaanim_output: texture_storage_2d<rgba8unorm, write>;

@group(0) @binding(1)
var<uniform> gaanim_background_params: vec4<f32>;

// Scene frame size in world units, or the output aspect at unit height when
// the host did not report the frame.
fn gaanim_frame_size(resolution: vec2<f32>) -> vec2<f32> {
    let frame = gaanim_background_params.yz;
    if (frame.x > 0.0 && frame.y > 0.0) {
        return frame;
    }
    return vec2<f32>(resolution.x / resolution.y, 1.0);
}
"#;

const ANIMATED_SHADER_ENTRY_POINT: &str = r#"
@compute @workgroup_size(8, 8, 1)
fn gaanim_render_background(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(gaanim_output);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let resolution = vec2<f32>(size);
    let uv = (vec2<f32>(id.xy) + vec2<f32>(0.5)) / resolution;
    let time = gaanim_background_params.x;
    let color = clamp(gaanim_background(uv, resolution, time), vec4<f32>(0.0), vec4<f32>(1.0));
    textureStore(gaanim_output, id.xy, color);
}
"#;

const STATIC_SHADER_ENTRY_POINT: &str = r#"
@compute @workgroup_size(8, 8, 1)
fn gaanim_render_background(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(gaanim_output);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let resolution = vec2<f32>(size);
    let uv = (vec2<f32>(id.xy) + vec2<f32>(0.5)) / resolution;
    let color = clamp(gaanim_background(uv, resolution), vec4<f32>(0.0), vec4<f32>(1.0));
    textureStore(gaanim_output, id.xy, color);
}
"#;

/// Paint used inside the authored scene bounds.
#[derive(Clone, Debug)]
pub enum BackgroundPaint {
    /// A native Vello solid, gradient, or image brush in canvas coordinates.
    Brush(Brush),
    /// A WGSL function rasterized for the active output size and timeline time.
    Shader(ShaderBackground),
}

impl BackgroundPaint {
    pub fn solid(color: Color) -> Self {
        Self::Brush(Brush::Solid(color))
    }

    /// Representative color used by the window clear and text contrast.
    pub fn fallback_color(&self) -> Color {
        match self {
            Self::Brush(Brush::Solid(color)) => *color,
            Self::Brush(Brush::Gradient(gradient)) => gradient
                .stops
                .first()
                .map(|stop| stop.color.to_alpha_color())
                .unwrap_or(Color::BLACK),
            Self::Brush(Brush::Image(_)) => Color::BLACK,
            Self::Shader(shader) => shader.fallback,
        }
    }

    /// Resolve the paint for a `width`x`height` raster of a scene frame
    /// `frame` world units wide and tall.
    pub fn resolve_brush(
        &self,
        width: u32,
        height: u32,
        time_seconds: f64,
        frame: (f64, f64),
    ) -> Result<Brush, ShaderBackgroundError> {
        match self {
            Self::Brush(brush) => Ok(brush.clone()),
            Self::Shader(shader) => shader.resolve_in_frame(width, height, time_seconds, frame),
        }
    }

    pub fn is_shader(&self) -> bool {
        matches!(self, Self::Shader(_))
    }
}

/// A custom WGSL scene background driven by exact timeline time.
///
/// `source` must define
/// `fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32>`.
/// The engine supplies normalized top-left-origin UV coordinates and the output
/// resolution in pixels plus absolute timeline seconds. The legacy two-argument
/// static signature remains accepted.
#[derive(Clone)]
pub struct ShaderBackground {
    source: Arc<str>,
    fallback: Color,
    contract: ShaderContract,
    /// Names of the `f32` fields of `gaanim_uniforms`, in order.
    uniforms: Arc<[Arc<str>]>,
    /// What each uniform reads, evaluated at the scene time.
    sources: Arc<[gaanim_animation::ScalarSource]>,
    /// Storage data the shader reads as `gaanim_data`, e.g. an audio track.
    data: Option<Arc<[[f32; 4]]>>,
    /// Uniform values the scene evaluated at recent times, keyed by the
    /// time's bits: reading a `Parameter` needs the world, which drawing
    /// the background does not have.
    values: Arc<Mutex<Vec<RecordedValues>>>,
    compiled: Arc<Mutex<Option<Arc<CompiledShader>>>>,
    cache: Arc<Mutex<Option<CachedShaderRaster>>>,
    gpu_image: Arc<Mutex<Option<ImageData>>>,
}

/// Uniform values recorded for the time with these bits.
type RecordedValues = (u64, Arc<[f32]>);

/// Recent times whose uniform values a background keeps.
const RECORDED_VALUE_TIMES: usize = 16;

type CachedShaderRaster = (Vec<u32>, Result<Brush, ShaderBackgroundError>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShaderContract {
    Animated,
    StaticLegacy,
}

struct CompiledShader {
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

impl fmt::Debug for ShaderBackground {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("ShaderBackground");
        debug
            .field("source_len", &self.source.len())
            .field("fallback", &self.fallback);
        if gaanim_core::fingerprint::identity_debug() {
            debug
                .field("source", &self.source)
                .field("contract", &self.contract);
        }
        debug.finish_non_exhaustive()
    }
}

impl ShaderBackground {
    pub fn new(
        source: impl Into<Arc<str>>,
        fallback: Color,
    ) -> Result<Self, ShaderBackgroundError> {
        Self::with_uniforms(source, fallback, Vec::new(), None)
    }

    /// A shader that also reads `f32` uniforms from `gaanim_uniforms`, one
    /// per `(name, source)` in order, evaluated at the scene time, and
    /// optional storage `data` from `gaanim_data`.
    pub fn with_uniforms(
        source: impl Into<Arc<str>>,
        fallback: Color,
        uniforms: Vec<(String, gaanim_animation::ScalarSource)>,
        data: Option<Arc<[[f32; 4]]>>,
    ) -> Result<Self, ShaderBackgroundError> {
        let source = source.into();
        let (names, sources): (Vec<Arc<str>>, Vec<_>) = uniforms
            .into_iter()
            .map(|(name, source)| (Arc::<str>::from(name), source))
            .unzip();
        validate_uniform_names(&names)?;
        if data.as_ref().is_some_and(|data| data.is_empty()) {
            return Err(ShaderBackgroundError::InvalidWgsl(
                "background shader data must not be empty".to_string(),
            ));
        }
        let layout = ShaderLayout {
            uniforms: &names,
            data: data.is_some(),
        };
        let contract = validate_shader_source(&source, layout)?;
        Ok(Self {
            source,
            fallback,
            contract,
            uniforms: names.into(),
            sources: sources.into(),
            data,
            values: Arc::default(),
            compiled: Arc::default(),
            cache: Arc::default(),
            gpu_image: Arc::default(),
        })
    }

    fn layout(&self) -> ShaderLayout<'_> {
        ShaderLayout {
            uniforms: &self.uniforms,
            data: self.data.is_some(),
        }
    }

    /// Names of the uniforms, in declaration order.
    pub fn uniforms(&self) -> &[Arc<str>] {
        &self.uniforms
    }

    /// What each uniform reads.
    pub fn uniform_sources(&self) -> &[gaanim_animation::ScalarSource] {
        &self.sources
    }

    /// Storage data the shader reads as `gaanim_data`.
    pub fn data(&self) -> Option<&Arc<[[f32; 4]]>> {
        self.data.as_ref()
    }

    /// Keep `values` as the uniforms at scene second `time`, as the scene
    /// evaluated them (or a bundle recorded them).
    pub fn record_values(&self, time: f64, values: &[f32]) {
        if self.uniforms.is_empty() {
            return;
        }
        let mut recorded = self.values.lock().expect("background values poisoned");
        let key = time.to_bits();
        recorded.retain(|(time, _)| *time != key);
        if recorded.len() >= RECORDED_VALUE_TIMES {
            recorded.remove(0);
        }
        recorded.push((key, values.into()));
    }

    /// The uniforms at scene second `time`: the values recorded for that
    /// time, or else the sources that read only the time (numbers, audio
    /// signals), with 0 for those that need the world.
    pub fn values_at(&self, time: f64) -> Arc<[f32]> {
        if self.uniforms.is_empty() {
            return Arc::default();
        }
        let key = time.to_bits();
        if let Some((_, values)) = self
            .values
            .lock()
            .expect("background values poisoned")
            .iter()
            .find(|(recorded, _)| *recorded == key)
        {
            return values.clone();
        }
        self.sources
            .iter()
            .map(|source| {
                source
                    .evaluate(time, |_| None)
                    .map_or(0.0, |value| value as f32)
            })
            .collect()
    }

    /// Load WGSL source from an asset file.
    ///
    /// The contents are validated with the same entry-point contract as
    /// [`Self::new`]. Relative paths are resolved by the caller.
    pub fn from_file(
        path: impl AsRef<Path>,
        fallback: Color,
    ) -> Result<Self, ShaderBackgroundError> {
        let path = path.as_ref().to_path_buf();
        let source =
            std::fs::read_to_string(&path).map_err(|source| ShaderBackgroundError::ReadSource {
                path,
                message: source.to_string(),
            })?;
        Self::new(source, fallback)
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn fallback(&self) -> Color {
        self.fallback
    }

    pub fn is_animated(&self) -> bool {
        self.contract == ShaderContract::Animated
    }

    pub fn resolve(
        &self,
        width: u32,
        height: u32,
        time_seconds: f64,
    ) -> Result<Brush, ShaderBackgroundError> {
        self.resolve_in_frame(width, height, time_seconds, (0.0, 0.0))
    }

    /// [`Self::resolve`] for a scene frame `frame` world units wide and tall,
    /// which the shader reads with `gaanim_frame_size`.
    pub fn resolve_in_frame(
        &self,
        width: u32,
        height: u32,
        time_seconds: f64,
        frame: (f64, f64),
    ) -> Result<Brush, ShaderBackgroundError> {
        let values = self.values_at(time_seconds);
        self.resolve_with_values(width, height, time_seconds, frame, &values)
    }

    /// [`Self::resolve_in_frame`] with its uniforms set to `values`.
    pub fn resolve_with_values(
        &self,
        width: u32,
        height: u32,
        time_seconds: f64,
        frame: (f64, f64),
        values: &[f32],
    ) -> Result<Brush, ShaderBackgroundError> {
        if width == 0 || height == 0 {
            return Err(ShaderBackgroundError::InvalidSize { width, height });
        }
        let time = shader_time(time_seconds, self.contract)?;
        let frame = frame_size(frame);
        let mut key = vec![
            width,
            height,
            time.to_bits(),
            frame[0].to_bits(),
            frame[1].to_bits(),
        ];
        key.extend(values.iter().map(|value| value.to_bits()));
        let mut cache = self.cache.lock().expect("shader background cache poisoned");
        if let Some((cached_key, cached)) = &*cache
            && *cached_key == key
        {
            return cached.clone();
        }
        let rendered = rasterize_shader(self, width, height, time, frame, values)
            .map(|image| Brush::Image(ImageBrush::new(image)));
        if let Err(error) = &rendered {
            bevy::log::error!("background shader failed; using its fallback color: {error}");
        }
        *cache = Some((key, rendered.clone()));
        rendered
    }

    /// Describe this shader rendered at `width`x`height` on the GPU that
    /// draws the scene, for [`GpuShaderBackgrounds`].
    pub fn gpu_request(
        &self,
        width: u32,
        height: u32,
        time_seconds: f64,
    ) -> Result<ShaderBackgroundRequest, ShaderBackgroundError> {
        if width == 0 || height == 0 {
            return Err(ShaderBackgroundError::InvalidSize { width, height });
        }
        Ok(ShaderBackgroundRequest {
            shader: self.clone(),
            image: self.gpu_image(width, height),
            time: shader_time(time_seconds, self.contract)?,
            frame: [0.0; 2],
            values: self.values_at(time_seconds),
        })
    }

    /// Whether a frame of this shader was copied through the CPU.
    #[cfg(test)]
    pub(crate) fn has_cpu_copy(&self) -> bool {
        self.cache
            .lock()
            .expect("shader background cache poisoned")
            .is_some()
    }

    /// Placeholder image that Vello replaces with the GPU texture. The same
    /// size returns the same image, so Vello refreshes one atlas slot in place.
    fn gpu_image(&self, width: u32, height: u32) -> ImageData {
        let mut cached = self
            .gpu_image
            .lock()
            .expect("shader background image poisoned");
        if let Some(image) = &*cached
            && image.width == width
            && image.height == height
        {
            return image.clone();
        }
        // Vello copies the registered texture instead of reading these bytes.
        // Large zeroed allocations are usually not committed until read, and
        // a missing texture draws a transparent image instead of panicking on
        // empty data.
        let image = ImageData {
            data: Blob::from(vec![0_u8; width as usize * height as usize * 4]),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width,
            height,
        };
        *cached = Some(image.clone());
        image
    }

    fn compiled_shader(&self, device: &wgpu::Device) -> Arc<CompiledShader> {
        let mut cached = self
            .compiled
            .lock()
            .expect("compiled background shader cache poisoned");
        if let Some(compiled) = &*cached {
            return compiled.clone();
        }
        let compiled = Arc::new(CompiledShader::new(
            device,
            &self.source,
            self.contract,
            self.layout(),
        ));
        *cached = Some(compiled.clone());
        compiled
    }

    /// The uniform buffer bytes of `values`, padded to whole `vec4`s.
    fn uniform_bytes(&self, values: &[f32]) -> Vec<u8> {
        let mut bytes = vec![0_u8; self.uniforms.len().div_ceil(4).max(1) * 16];
        for (index, value) in values.iter().take(self.uniforms.len()).enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_ne_bytes());
        }
        bytes
    }

    fn data_bytes(&self) -> Option<Vec<u8>> {
        self.data.as_ref().map(|data| {
            data.iter()
                .flat_map(|texel| texel.iter().flat_map(|value| value.to_ne_bytes()))
                .collect()
        })
    }
}

/// What a background shader declares beyond its output and time.
#[derive(Clone, Copy)]
struct ShaderLayout<'a> {
    uniforms: &'a [Arc<str>],
    data: bool,
}

/// The extra buffers a background shader binds: its uniforms and data.
struct ShaderBuffers<'a> {
    uniforms: Option<&'a wgpu::Buffer>,
    data: Option<&'a wgpu::Buffer>,
}

/// Most named uniforms a background shader may declare.
pub const MAX_BACKGROUND_UNIFORMS: usize = 32;

fn validate_uniform_names(names: &[Arc<str>]) -> Result<(), ShaderBackgroundError> {
    if names.len() > MAX_BACKGROUND_UNIFORMS {
        return Err(ShaderBackgroundError::InvalidWgsl(format!(
            "at most {MAX_BACKGROUND_UNIFORMS} uniforms per background, got {}",
            names.len()
        )));
    }
    for (index, name) in names.iter().enumerate() {
        let mut chars = name.chars();
        let valid = chars
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && !name.starts_with("__")
            && name.as_ref() != "_";
        if !valid {
            return Err(ShaderBackgroundError::InvalidWgsl(format!(
                "{name:?} is not a WGSL identifier (letters, digits and '_', not starting with a digit)"
            )));
        }
        if names[..index].contains(name) {
            return Err(ShaderBackgroundError::InvalidWgsl(format!(
                "uniform {name:?} is declared twice"
            )));
        }
    }
    Ok(())
}

impl CompiledShader {
    fn new(
        device: &wgpu::Device,
        source: &str,
        contract: ShaderContract,
        layout: ShaderLayout<'_>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-background-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(complete_shader(source, contract, layout))),
        });
        let mut entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        if !layout.uniforms.is_empty() {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
        }
        if layout.data {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
        }
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-background-shader-layout"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim-background-shader-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gaanim-background-shader-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("gaanim_render_background"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            bind_group_layout,
            pipeline,
        }
    }

    fn bind_group(
        &self,
        device: &wgpu::Device,
        output: &wgpu::Texture,
        time: &wgpu::Buffer,
        buffers: ShaderBuffers<'_>,
    ) -> wgpu::BindGroup {
        let view = output.create_view(&Default::default());
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: time.as_entire_binding(),
            },
        ];
        if let Some(uniforms) = buffers.uniforms {
            entries.push(wgpu::BindGroupEntry {
                binding: 2,
                resource: uniforms.as_entire_binding(),
            });
        }
        if let Some(data) = buffers.data {
            entries.push(wgpu::BindGroupEntry {
                binding: 3,
                resource: data.as_entire_binding(),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-background-shader-bind-group"),
            layout: &self.bind_group_layout,
            entries: &entries,
        })
    }

    fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        width: u32,
        height: u32,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("gaanim-background-shader-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
}

fn shader_output_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gaanim-background-shader-output"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | usage,
        view_formats: &[],
    })
}

fn time_uniform_bytes(time: f32, frame: [f32; 2]) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[..4].copy_from_slice(&time.to_ne_bytes());
    bytes[4..8].copy_from_slice(&frame[0].to_ne_bytes());
    bytes[8..12].copy_from_slice(&frame[1].to_ne_bytes());
    bytes
}

/// A positive finite frame size in `f32`, or zeros for an unknown frame.
fn frame_size(frame: (f64, f64)) -> [f32; 2] {
    let size = [frame.0 as f32, frame.1 as f32];
    if size.iter().all(|value| value.is_finite() && *value > 0.0) {
        size
    } else {
        [0.0; 2]
    }
}

/// One shader background frame to draw with [`GpuShaderBackgrounds`].
#[derive(Clone)]
pub struct ShaderBackgroundRequest {
    shader: ShaderBackground,
    image: ImageData,
    time: f32,
    frame: [f32; 2],
    values: Arc<[f32]>,
}

impl ShaderBackgroundRequest {
    /// Placeholder image to draw with an image brush.
    pub fn image(&self) -> &ImageData {
        &self.image
    }

    /// Timeline seconds the shader draws.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Scene frame, in world units, reported to the shader.
    pub fn frame(&self) -> [f32; 2] {
        self.frame
    }

    /// Report a scene frame `frame` world units wide and tall to the shader.
    pub fn in_frame(mut self, frame: (f64, f64)) -> Self {
        self.frame = frame_size(frame);
        self
    }

    /// The shader's uniform values.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// Set the shader's uniform values.
    pub fn with_values(mut self, values: Arc<[f32]>) -> Self {
        self.values = values;
        self
    }

    /// Whether `other` draws the same pixels into the same image, so one
    /// texture serves both.
    pub fn draws_same(&self, other: &Self) -> bool {
        let bits = |values: &[f32]| {
            values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        };
        self.image.data.id() == other.image.data.id()
            && self.time.to_bits() == other.time.to_bits()
            && bits(&self.frame) == bits(&other.frame)
            && bits(&self.values) == bits(&other.values)
    }
}

/// Shader backgrounds rendered on the device of a Vello renderer.
///
/// Each placeholder image maps to a texture registered with
/// [`vello::Renderer::override_image`]. A new frame time only dispatches the
/// compute shader: Vello copies the texture into its image atlas on the GPU,
/// so nothing is read back to the CPU or uploaded again.
#[derive(Default)]
pub struct GpuShaderBackgrounds {
    device: Option<wgpu::Device>,
    targets: HashMap<u64, GpuShaderTarget>,
}

struct GpuShaderTarget {
    image: ImageData,
    source: Arc<str>,
    contract: ShaderContract,
    uniform_count: usize,
    has_data: bool,
    texture: wgpu::Texture,
    time: wgpu::Buffer,
    uniforms: Option<wgpu::Buffer>,
    /// `None` when the pipeline failed to build; the texture holds the fallback.
    pipeline: Option<(Arc<CompiledShader>, wgpu::BindGroup)>,
    /// Set while WebGPU has not validated `pipeline`; it is not run until then.
    checking: Option<crate::gpu_scope::PendingScope>,
    /// Time, frame size and uniform bits of the texture contents.
    rendered_time: Option<Vec<u32>>,
}

impl GpuShaderTarget {
    fn texture_copy(&self) -> wgpu::TexelCopyTextureInfoBase<wgpu::Texture> {
        wgpu::TexelCopyTextureInfoBase {
            texture: self.texture.clone(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        }
    }
}

impl GpuShaderBackgrounds {
    /// Register every texture again after the renderer was replaced.
    pub fn renderer_replaced(&mut self, device: &wgpu::Device, renderer: &mut vello::Renderer) {
        self.adopt_device(device, renderer);
        for target in self.targets.values() {
            renderer.override_image(&target.image, Some(target.texture_copy()));
        }
    }

    /// Draw `requests` for the next frame of `renderer`, such as the
    /// backgrounds of both segments of a transition, and release the
    /// textures of images that frame no longer draws.
    ///
    /// Call before the frame is submitted to `queue`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        requests: &[ShaderBackgroundRequest],
    ) {
        self.adopt_device(device, renderer);
        // A reload rebuilds the same shader; keep its pipeline.
        let reusable: Vec<Option<Arc<CompiledShader>>> = requests
            .iter()
            .map(|request| {
                if self.targets.contains_key(&request.image.data.id()) {
                    return None;
                }
                self.targets
                    .values()
                    .find(|target| {
                        target.contract == request.shader.contract
                            && *target.source == *request.shader.source
                            && target.uniform_count == request.shader.uniforms.len()
                            && target.has_data == request.shader.data.is_some()
                    })
                    .filter(|target| target.checking.is_none())
                    .and_then(|target| target.pipeline.as_ref())
                    .map(|(compiled, _)| compiled.clone())
            })
            .collect();
        self.targets.retain(|id, target| {
            let keep = requests
                .iter()
                .any(|request| request.image.data.id() == *id);
            if !keep {
                renderer.override_image(&target.image, None);
            }
            keep
        });
        for (request, reusable) in requests.iter().zip(reusable) {
            self.draw(device, queue, renderer, request, reusable);
        }
    }

    /// Render `request` into its image's texture unless it already holds it.
    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        request: &ShaderBackgroundRequest,
        reusable: Option<Arc<CompiledShader>>,
    ) {
        let target = self
            .targets
            .entry(request.image.data.id())
            .or_insert_with(|| {
                let target = GpuShaderTarget::new(device, queue, request, reusable);
                renderer.override_image(&target.image, Some(target.texture_copy()));
                target
            });
        let mut time = vec![
            request.time.to_bits(),
            request.frame[0].to_bits(),
            request.frame[1].to_bits(),
        ];
        time.extend(request.values.iter().map(|value| value.to_bits()));
        if let Some(scope) = &target.checking {
            let Some(outcome) = scope.poll() else {
                return;
            };
            target.checking = None;
            if let ScopeCheck::Invalid(error) = outcome {
                target.fail(queue, &request.shader, &error);
            }
        }
        if target.rendered_time.as_ref() == Some(&time) {
            return;
        }
        if let Some((compiled, bind_group)) = &target.pipeline {
            queue.write_buffer(
                &target.time,
                0,
                &time_uniform_bytes(request.time, request.frame),
            );
            if let Some(uniforms) = &target.uniforms {
                queue.write_buffer(uniforms, 0, &request.shader.uniform_bytes(&request.values));
            }
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gaanim-background-shader-commands"),
            });
            compiled.dispatch(
                &mut encoder,
                bind_group,
                target.image.width,
                target.image.height,
            );
            queue.submit(Some(encoder.finish()));
        }
        renderer.mark_override_image_dirty(&target.image);
        target.rendered_time = Some(time);
    }

    /// Drop the textures of a previous, e.g. lost, device.
    fn adopt_device(&mut self, device: &wgpu::Device, renderer: &mut vello::Renderer) {
        if self.device.as_ref() == Some(device) {
            return;
        }
        for target in self.targets.values() {
            renderer.override_image(&target.image, None);
        }
        self.targets.clear();
        self.device = Some(device.clone());
    }

    #[cfg(test)]
    fn texture_count(&self) -> usize {
        self.targets.len()
    }
}

impl GpuShaderTarget {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        request: &ShaderBackgroundRequest,
        compiled: Option<Arc<CompiledShader>>,
    ) -> Self {
        let ShaderBackgroundRequest { shader, image, .. } = request;
        let texture = shader_output_texture(
            device,
            image.width,
            image.height,
            wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        );
        let time = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gaanim-background-shader-time"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniforms = (!shader.uniforms.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("gaanim-background-shader-uniforms"),
                contents: &shader.uniform_bytes(&request.values),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });
        let data = shader.data_bytes().map(|bytes| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("gaanim-background-shader-data"),
                contents: &bytes,
                usage: wgpu::BufferUsages::STORAGE,
            })
        });
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let compiled = compiled.unwrap_or_else(|| {
            Arc::new(CompiledShader::new(
                device,
                &shader.source,
                shader.contract,
                shader.layout(),
            ))
        });
        let bind_group = compiled.bind_group(
            device,
            &texture,
            &time,
            ShaderBuffers {
                uniforms: uniforms.as_ref(),
                data: data.as_ref(),
            },
        );
        let mut target = Self {
            image: image.clone(),
            source: shader.source.clone(),
            contract: shader.contract,
            uniform_count: shader.uniforms.len(),
            has_data: shader.data.is_some(),
            texture,
            time,
            uniforms,
            pipeline: Some((compiled, bind_group)),
            checking: None,
            rendered_time: None,
        };
        match crate::gpu_scope::check(error_scope) {
            ScopeCheck::Valid => {}
            ScopeCheck::Invalid(error) => target.fail(queue, shader, &error),
            ScopeCheck::Pending(scope) => target.checking = Some(scope),
        }
        target
    }

    /// Drop the pipeline that failed to build and fill the texture with the
    /// shader's fallback color.
    fn fail(&mut self, queue: &wgpu::Queue, shader: &ShaderBackground, error: &wgpu::Error) {
        bevy::log::error!("background shader failed; using its fallback color: {error}");
        self.pipeline = None;
        let rgba = shader.fallback.to_rgba8().to_u8_array();
        let pixels = rgba.repeat(self.image.width as usize * self.image.height as usize);
        queue.write_texture(
            self.texture.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.image.width * 4),
                rows_per_image: None,
            },
            self.texture.size(),
        );
    }
}

#[derive(Clone, Debug, Error)]
pub enum ShaderBackgroundError {
    #[error("could not read background WGSL asset '{path}': {message}")]
    ReadSource { path: PathBuf, message: String },
    #[error("invalid background WGSL: {0}")]
    InvalidWgsl(String),
    #[error("background shader output size must be positive, got {width}x{height}")]
    InvalidSize { width: u32, height: u32 },
    #[error("background shader timeline time must be finite, got {0}")]
    InvalidTime(f64),
    #[error("no suitable GPU adapter is available for the background shader")]
    NoAdapter,
    #[error("background shader GPU initialization failed: {0}")]
    Device(String),
    #[error("background shader GPU validation failed: {0}")]
    GpuValidation(String),
    #[error("background shader readback failed: {0}")]
    Readback(String),
}

fn complete_shader(source: &str, contract: ShaderContract, layout: ShaderLayout<'_>) -> String {
    let entry_point = match contract {
        ShaderContract::Animated => ANIMATED_SHADER_ENTRY_POINT,
        ShaderContract::StaticLegacy => STATIC_SHADER_ENTRY_POINT,
    };
    let mut declarations = String::new();
    if !layout.uniforms.is_empty() {
        declarations.push_str("struct GaanimUniforms {\n");
        for name in layout.uniforms {
            declarations.push_str(&format!("    {name}: f32,\n"));
        }
        declarations.push_str(
            "}\n\n@group(0) @binding(2)\nvar<uniform> gaanim_uniforms: GaanimUniforms;\n",
        );
    }
    if layout.data {
        declarations.push_str(
            "\n@group(0) @binding(3)\nvar<storage, read> gaanim_data: array<vec4<f32>>;\n",
        );
    }
    format!("{SHADER_PREAMBLE}\n{declarations}\n{source}\n{entry_point}")
}

fn validate_complete_shader(
    source: &str,
    contract: ShaderContract,
    layout: ShaderLayout<'_>,
) -> Result<(), String> {
    let complete = complete_shader(source, contract, layout);
    let module =
        naga::front::wgsl::parse_str(&complete).map_err(|error| error.emit_to_string(&complete))?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn validate_shader_source(
    source: &str,
    layout: ShaderLayout<'_>,
) -> Result<ShaderContract, ShaderBackgroundError> {
    if !source.contains("gaanim_background") {
        return Err(ShaderBackgroundError::InvalidWgsl(
            "source must define gaanim_background(uv, resolution, time)".to_string(),
        ));
    }
    match validate_complete_shader(source, ShaderContract::Animated, layout) {
        Ok(()) => Ok(ShaderContract::Animated),
        Err(animated_error) => {
            if validate_complete_shader(source, ShaderContract::StaticLegacy, layout).is_ok() {
                Ok(ShaderContract::StaticLegacy)
            } else {
                Err(ShaderBackgroundError::InvalidWgsl(animated_error))
            }
        }
    }
}

fn shader_time(time_seconds: f64, contract: ShaderContract) -> Result<f32, ShaderBackgroundError> {
    if contract == ShaderContract::StaticLegacy {
        return Ok(0.0);
    }
    let time = time_seconds as f32;
    if !time_seconds.is_finite() || !time.is_finite() {
        return Err(ShaderBackgroundError::InvalidTime(time_seconds));
    }
    Ok(time)
}

struct ShaderGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    operation: Mutex<()>,
}

impl ShaderGpu {
    fn new() -> Result<Self, ShaderBackgroundError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all().with_env(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|_| ShaderBackgroundError::NoAdapter)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gaanim-background-shader-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|error| ShaderBackgroundError::Device(error.to_string()))?;
        Ok(Self {
            device,
            queue,
            operation: Mutex::new(()),
        })
    }
}

static SHADER_GPU: OnceLock<Result<ShaderGpu, ShaderBackgroundError>> = OnceLock::new();

fn shader_gpu() -> Result<&'static ShaderGpu, ShaderBackgroundError> {
    match SHADER_GPU.get_or_init(ShaderGpu::new) {
        Ok(gpu) => Ok(gpu),
        Err(error) => Err(error.clone()),
    }
}

fn rasterize_shader(
    shader: &ShaderBackground,
    width: u32,
    height: u32,
    time: f32,
    frame: [f32; 2],
    values: &[f32],
) -> Result<ImageData, ShaderBackgroundError> {
    let gpu = shader_gpu()?;
    let _operation = gpu
        .operation
        .lock()
        .map_err(|_| ShaderBackgroundError::Device("shader GPU lock poisoned".to_string()))?;
    let device = &gpu.device;
    let queue = &gpu.queue;

    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let compiled = shader.compiled_shader(device);
    let texture = shader_output_texture(device, width, height, wgpu::TextureUsages::COPY_SRC);
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gaanim-background-shader-time"),
        contents: &time_uniform_bytes(time, frame),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let uniforms = (!shader.uniforms.is_empty()).then(|| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gaanim-background-shader-uniforms"),
            contents: &shader.uniform_bytes(values),
            usage: wgpu::BufferUsages::UNIFORM,
        })
    });
    let data = shader.data_bytes().map(|bytes| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gaanim-background-shader-data"),
            contents: &bytes,
            usage: wgpu::BufferUsages::STORAGE,
        })
    });
    let bind_group = compiled.bind_group(
        device,
        &texture,
        &uniform,
        ShaderBuffers {
            uniforms: uniforms.as_ref(),
            data: data.as_ref(),
        },
    );

    let padded_width = (width + 63) & !63;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gaanim-background-shader-readback"),
        size: u64::from(padded_width) * u64::from(height) * 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("gaanim-background-shader-commands"),
    });
    compiled.dispatch(&mut encoder, &bind_group, width, height);
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_width * 4),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit(Some(encoder.finish()));
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: None,
    });
    if let Some(error) = pollster::block_on(error_scope.pop()) {
        return Err(ShaderBackgroundError::GpuValidation(error.to_string()));
    }

    let (sender, receiver) = std::sync::mpsc::channel();
    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result.map_err(|error| error.to_string()));
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    receiver
        .recv()
        .map_err(|error| ShaderBackgroundError::Readback(error.to_string()))?
        .map_err(ShaderBackgroundError::Readback)?;

    let mapped = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for row in 0..height {
        let start = (row * padded_width * 4) as usize;
        pixels.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
    }
    drop(mapped);
    staging.unmap();
    Ok(ImageData {
        data: Blob::from(pixels),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    })
}

/// A real GPU for tests; `None` when the machine has no adapter.
#[cfg(test)]
pub(crate) mod test_gpu {
    use super::*;

    pub(crate) struct TestGpu {
        pub(crate) device: wgpu::Device,
        pub(crate) queue: wgpu::Queue,
        pub(crate) renderer: vello::Renderer,
    }

    pub(crate) fn test_gpu() -> Option<TestGpu> {
        // CI sets `WGPU_BACKEND` to skip adapters that crash the test process.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all().with_env(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).ok()?;
        let renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::area_only(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .ok()?;
        Some(TestGpu {
            device,
            queue,
            renderer,
        })
    }

    impl TestGpu {
        pub(crate) fn target(&self, width: u32, height: u32) -> wgpu::Texture {
            shader_output_texture(&self.device, width, height, wgpu::TextureUsages::COPY_SRC)
        }

        pub(crate) fn render(&mut self, scene: &vello::Scene, target: &wgpu::Texture) {
            self.renderer
                .render_to_texture(
                    &self.device,
                    &self.queue,
                    scene,
                    &target.create_view(&Default::default()),
                    &vello::RenderParams {
                        base_color: Color::TRANSPARENT,
                        width: target.width(),
                        height: target.height(),
                        antialiasing_method: vello::AaConfig::Area,
                    },
                )
                .unwrap();
        }

        pub(crate) fn read(&self, texture: &wgpu::Texture) -> Vec<u8> {
            let (width, height) = (texture.width(), texture.height());
            let padded_width = (width + 63) & !63;
            let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(padded_width) * u64::from(height) * 4,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = self.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &staging,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_width * 4),
                        rows_per_image: None,
                    },
                },
                texture.size(),
            );
            self.queue.submit(Some(encoder.finish()));
            let slice = staging.slice(..);
            slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            let mapped = slice.get_mapped_range();
            (0..height)
                .flat_map(|row| {
                    let start = (row * padded_width * 4) as usize;
                    mapped[start..start + (width * 4) as usize].to_vec()
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_gpu::{TestGpu, test_gpu};
    use super::*;

    #[test]
    fn shader_validation_accepts_a_time_parameter_in_the_documented_contract() {
        let shader = ShaderBackground::new(
            "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return vec4<f32>(uv, sin(time) + resolution.x * 0.0, 1.0);\n}",
            Color::BLACK,
        )
        .unwrap();
        assert!(shader.source().contains("time: f32"));
        assert!(shader.is_animated());
    }

    #[test]
    fn shader_validation_keeps_the_static_two_argument_contract_compatible() {
        let shader = ShaderBackground::new(
            "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>) -> vec4<f32> {\n\
             return vec4<f32>(uv, resolution.x * 0.0, 1.0);\n}",
            Color::BLACK,
        )
        .unwrap();
        assert!(shader.source().contains("gaanim_background"));
        assert!(!shader.is_animated());
    }

    #[test]
    fn shader_source_can_be_loaded_from_an_asset_file() {
        let path = std::env::temp_dir().join(format!(
            "gaanim_shader_background_{}.wgsl",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return vec4<f32>(uv, time + resolution.x * 0.0, 1.0);\n}",
        )
        .unwrap();

        let shader = ShaderBackground::from_file(&path, Color::BLACK).unwrap();
        assert!(shader.is_animated());
        assert!(shader.source().contains("time: f32"));
    }

    #[test]
    fn animated_shader_cache_time_tracks_f32_timeline_seconds() {
        assert_eq!(
            shader_time(1.25, ShaderContract::Animated).unwrap(),
            1.25_f32
        );
        assert!(matches!(
            shader_time(f64::NAN, ShaderContract::Animated),
            Err(ShaderBackgroundError::InvalidTime(value)) if value.is_nan()
        ));
        assert_eq!(
            shader_time(f64::NAN, ShaderContract::StaticLegacy).unwrap(),
            0.0
        );
    }

    const ANIMATED_SOURCE: &str = "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
         return vec4<f32>(uv, fract(time * 0.37) + resolution.x * 0.0, 1.0);\n}";

    fn background_scene(brush: &Brush, width: u32, height: u32) -> vello::Scene {
        let mut scene = vello::Scene::new();
        scene.fill(
            gaanim_core::peniko::Fill::NonZero,
            gaanim_core::kurbo::Affine::IDENTITY,
            brush,
            None,
            &gaanim_core::kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
        );
        scene
    }

    #[test]
    fn gpu_resident_background_matches_the_cpu_copy() {
        let Some(mut gpu) = test_gpu() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let (width, height) = (96, 54);
        let shader = ShaderBackground::new(ANIMATED_SOURCE, Color::BLACK).unwrap();
        let mut backgrounds = GpuShaderBackgrounds::default();
        let target = gpu.target(width, height);
        let mut frames = Vec::new();
        // The repeated time checks a frame that skips the dispatch.
        for time in [0.0, 1.5, 1.5, 4.0] {
            let request = shader.gpu_request(width, height, time).unwrap();
            backgrounds.prepare(
                &gpu.device,
                &gpu.queue,
                &mut gpu.renderer,
                std::slice::from_ref(&request),
            );
            let brush = Brush::Image(ImageBrush::new(request.image().clone()));
            gpu.render(&background_scene(&brush, width, height), &target);
            let resident = gpu.read(&target);

            let copied = shader.resolve(width, height, time).unwrap();
            gpu.render(&background_scene(&copied, width, height), &target);
            // Hardware drivers may round the resident texture and the uploaded
            // copy one step apart; anything more is a real mismatch.
            let copy = gpu.read(&target);
            let worst = resident
                .iter()
                .zip(&copy)
                .map(|(a, b)| a.abs_diff(*b))
                .max();
            assert!(worst <= Some(1), "t = {time}: channels differ by {worst:?}");
            frames.push(resident);
        }
        assert_ne!(frames[0], frames[1], "the shader output follows time");
        assert_eq!(backgrounds.texture_count(), 1);

        let resized = shader.gpu_request(width * 2, height, 4.0).unwrap();
        backgrounds.prepare(
            &gpu.device,
            &gpu.queue,
            &mut gpu.renderer,
            std::slice::from_ref(&resized),
        );
        assert_eq!(
            backgrounds.texture_count(),
            1,
            "resizing releases the old texture"
        );
        backgrounds.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &[]);
        assert_eq!(backgrounds.texture_count(), 0);
    }

    #[test]
    fn two_backgrounds_draw_in_the_same_frame() {
        let Some(mut gpu) = test_gpu() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let (width, height) = (96, 54);
        let first = ShaderBackground::new(ANIMATED_SOURCE, Color::BLACK).unwrap();
        let second = ShaderBackground::new(
            "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
             return vec4<f32>(1.0 - uv.x, fract(time * 0.21) + resolution.x * 0.0, uv.y, 1.0);
}",
            Color::BLACK,
        )
        .unwrap();
        let mut backgrounds = GpuShaderBackgrounds::default();
        let target = gpu.target(width, height);
        // Both segments of a transition, each at its own time.
        let requests = [
            first.gpu_request(width, height, 1.5).unwrap(),
            second.gpu_request(width, height, 3.0).unwrap(),
        ];
        backgrounds.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &requests);
        assert_eq!(backgrounds.texture_count(), 2);
        for (shader, request, time) in [(&first, &requests[0], 1.5), (&second, &requests[1], 3.0)] {
            let brush = Brush::Image(ImageBrush::new(request.image().clone()));
            gpu.render(&background_scene(&brush, width, height), &target);
            let resident = gpu.read(&target);
            let copied = shader.resolve(width, height, time).unwrap();
            gpu.render(&background_scene(&copied, width, height), &target);
            let copy = gpu.read(&target);
            let worst = resident
                .iter()
                .zip(&copy)
                .map(|(a, b)| a.abs_diff(*b))
                .max();
            assert!(worst <= Some(1), "t = {time}: channels differ by {worst:?}");
        }
        // The next frame draws only the second; the first texture goes.
        backgrounds.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &requests[1..]);
        assert_eq!(backgrounds.texture_count(), 1);
    }

    #[test]
    fn gpu_requests_reuse_one_placeholder_per_size() {
        let shader = ShaderBackground::new(ANIMATED_SOURCE, Color::BLACK).unwrap();
        let first = shader.gpu_request(64, 32, 0.0).unwrap();
        let later = shader.gpu_request(64, 32, 2.0).unwrap();
        let resized = shader.gpu_request(32, 32, 2.0).unwrap();
        assert_eq!(first.image().data.id(), later.image().data.id());
        assert_ne!(first.image().data.id(), resized.image().data.id());
        assert!(matches!(
            shader.gpu_request(0, 32, 0.0),
            Err(ShaderBackgroundError::InvalidSize { .. })
        ));
    }

    /// Per-frame cost of an animated 1080p background, CPU copy vs GPU texture:
    /// `just test-package gaanim_renderer shader_background_frame_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark"]
    fn shader_background_frame_cost() {
        let Some(mut gpu) = test_gpu() else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let (width, height, frames) = (1920, 1080, 60);
        let shader = ShaderBackground::new(ANIMATED_SOURCE, Color::BLACK).unwrap();
        let target = gpu.target(width, height);
        let mut backgrounds = GpuShaderBackgrounds::default();
        let mut measure = |label: &str, gpu: &mut TestGpu, resident: bool| {
            let started = std::time::Instant::now();
            for frame in 0..frames {
                let time = f64::from(frame) / 60.0 + if resident { 100.0 } else { 0.0 };
                let brush = if resident {
                    let request = shader.gpu_request(width, height, time).unwrap();
                    backgrounds.prepare(
                        &gpu.device,
                        &gpu.queue,
                        &mut gpu.renderer,
                        std::slice::from_ref(&request),
                    );
                    Brush::Image(ImageBrush::new(request.image().clone()))
                } else {
                    shader.resolve(width, height, time).unwrap()
                };
                gpu.render(&background_scene(&brush, width, height), &target);
                gpu.device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
            }
            let per_frame = started.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
            println!("{label}: {per_frame:.2} ms/frame at {width}x{height}");
        };
        measure("cpu copy", &mut gpu, false);
        measure("gpu texture", &mut gpu, true);
    }

    #[test]
    fn shader_validation_rejects_a_missing_entry_function() {
        let error = ShaderBackground::new("fn other() {}", Color::BLACK).unwrap_err();
        assert!(matches!(error, ShaderBackgroundError::InvalidWgsl(_)));
    }
}
