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

const SHADER_PREAMBLE: &str = r#"
@group(0) @binding(0)
var gaanim_output: texture_storage_2d<rgba8unorm, write>;

@group(0) @binding(1)
var<uniform> gaanim_background_params: vec4<f32>;
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

    /// Representative color used by the native 3D clear pass and text contrast.
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

    pub fn resolve_brush(
        &self,
        width: u32,
        height: u32,
        time_seconds: f64,
    ) -> Result<Brush, ShaderBackgroundError> {
        match self {
            Self::Brush(brush) => Ok(brush.clone()),
            Self::Shader(shader) => shader.resolve(width, height, time_seconds),
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
    compiled: Arc<Mutex<Option<Arc<CompiledShader>>>>,
    cache: Arc<Mutex<Option<CachedShaderRaster>>>,
    gpu_image: Arc<Mutex<Option<ImageData>>>,
}

type CachedShaderRaster = ((u32, u32, u32), Result<Brush, ShaderBackgroundError>);

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
        let source = source.into();
        let contract = validate_shader_source(&source)?;
        Ok(Self {
            source,
            fallback,
            contract,
            compiled: Arc::default(),
            cache: Arc::default(),
            gpu_image: Arc::default(),
        })
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
        if width == 0 || height == 0 {
            return Err(ShaderBackgroundError::InvalidSize { width, height });
        }
        let time = shader_time(time_seconds, self.contract)?;
        let key = (width, height, time.to_bits());
        let mut cache = self.cache.lock().expect("shader background cache poisoned");
        if let Some((cached_key, cached)) = &*cache
            && *cached_key == key
        {
            return cached.clone();
        }
        let rendered = rasterize_shader(self, width, height, time)
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
        })
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
        let compiled = Arc::new(CompiledShader::new(device, &self.source, self.contract));
        *cached = Some(compiled.clone());
        compiled
    }
}

impl CompiledShader {
    fn new(device: &wgpu::Device, source: &str, contract: ShaderContract) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-background-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(complete_shader(source, contract))),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gaanim-background-shader-layout"),
            entries: &[
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
            ],
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
    ) -> wgpu::BindGroup {
        let view = output.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gaanim-background-shader-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: time.as_entire_binding(),
                },
            ],
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

fn time_uniform_bytes(time: f32) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[..4].copy_from_slice(&time.to_ne_bytes());
    bytes
}

/// One shader background frame to draw with [`GpuShaderBackgrounds`].
#[derive(Clone)]
pub struct ShaderBackgroundRequest {
    shader: ShaderBackground,
    image: ImageData,
    time: f32,
}

impl ShaderBackgroundRequest {
    /// Placeholder image to draw with an image brush.
    pub fn image(&self) -> &ImageData {
        &self.image
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
    texture: wgpu::Texture,
    time: wgpu::Buffer,
    /// `None` when the pipeline failed to build; the texture holds the fallback.
    pipeline: Option<(Arc<CompiledShader>, wgpu::BindGroup)>,
    rendered_time: Option<u32>,
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

    /// Draw `request` for the next frame of `renderer` and release the
    /// textures of images that frame no longer draws.
    ///
    /// Call before the frame is submitted to `queue`.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        request: Option<&ShaderBackgroundRequest>,
    ) {
        self.adopt_device(device, renderer);
        let requested = request.map(|request| request.image.data.id());
        let new_target =
            request.filter(|request| !self.targets.contains_key(&request.image.data.id()));
        // A reload rebuilds the same shader; keep its pipeline.
        let reusable = new_target.and_then(|request| {
            self.targets
                .values()
                .find(|target| {
                    target.contract == request.shader.contract
                        && *target.source == *request.shader.source
                })
                .and_then(|target| target.pipeline.as_ref())
                .map(|(compiled, _)| compiled.clone())
        });
        self.targets.retain(|id, target| {
            let keep = Some(*id) == requested;
            if !keep {
                renderer.override_image(&target.image, None);
            }
            keep
        });
        let Some(request) = request else {
            return;
        };
        let target = self
            .targets
            .entry(request.image.data.id())
            .or_insert_with(|| {
                let target = GpuShaderTarget::new(device, queue, request, reusable);
                renderer.override_image(&target.image, Some(target.texture_copy()));
                target
            });
        let time = request.time.to_bits();
        if target.rendered_time == Some(time) {
            return;
        }
        if let Some((compiled, bind_group)) = &target.pipeline {
            queue.write_buffer(&target.time, 0, &time_uniform_bytes(request.time));
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
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let compiled = compiled.unwrap_or_else(|| {
            Arc::new(CompiledShader::new(device, &shader.source, shader.contract))
        });
        let bind_group = compiled.bind_group(device, &texture, &time);
        let pipeline = match pollster::block_on(error_scope.pop()) {
            None => Some((compiled, bind_group)),
            Some(error) => {
                bevy::log::error!("background shader failed; using its fallback color: {error}");
                let rgba = shader.fallback.to_rgba8().to_u8_array();
                let pixels = rgba.repeat(image.width as usize * image.height as usize);
                queue.write_texture(
                    texture.as_image_copy(),
                    &pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(image.width * 4),
                        rows_per_image: None,
                    },
                    texture.size(),
                );
                None
            }
        };
        Self {
            image: image.clone(),
            source: shader.source.clone(),
            contract: shader.contract,
            texture,
            time,
            pipeline,
            rendered_time: None,
        }
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

fn complete_shader(source: &str, contract: ShaderContract) -> String {
    let entry_point = match contract {
        ShaderContract::Animated => ANIMATED_SHADER_ENTRY_POINT,
        ShaderContract::StaticLegacy => STATIC_SHADER_ENTRY_POINT,
    };
    format!("{SHADER_PREAMBLE}\n{source}\n{entry_point}")
}

fn validate_complete_shader(source: &str, contract: ShaderContract) -> Result<(), String> {
    let complete = complete_shader(source, contract);
    let module =
        naga::front::wgsl::parse_str(&complete).map_err(|error| error.emit_to_string(&complete))?;
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn validate_shader_source(source: &str) -> Result<ShaderContract, ShaderBackgroundError> {
    if !source.contains("gaanim_background") {
        return Err(ShaderBackgroundError::InvalidWgsl(
            "source must define gaanim_background(uv, resolution, time)".to_string(),
        ));
    }
    match validate_complete_shader(source, ShaderContract::Animated) {
        Ok(()) => Ok(ShaderContract::Animated),
        Err(animated_error) => {
            if validate_complete_shader(source, ShaderContract::StaticLegacy).is_ok() {
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
        contents: &time_uniform_bytes(time),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind_group = compiled.bind_group(device, &texture, &uniform);

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
            backgrounds.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, Some(&request));
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
        backgrounds.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, Some(&resized));
        assert_eq!(
            backgrounds.texture_count(),
            1,
            "resizing releases the old texture"
        );
        backgrounds.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, None);
        assert_eq!(backgrounds.texture_count(), 0);
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
                    backgrounds.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, Some(&request));
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
