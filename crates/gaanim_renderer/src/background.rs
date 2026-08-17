use gaanim_core::peniko::{Blob, Brush, Color, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use std::borrow::Cow;
use std::fmt;
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
        f.debug_struct("ShaderBackground")
            .field("source_len", &self.source.len())
            .field("fallback", &self.fallback)
            .finish_non_exhaustive()
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
        })
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

    fn compiled_shader(&self, device: &wgpu::Device) -> Arc<CompiledShader> {
        let mut cached = self
            .compiled
            .lock()
            .expect("compiled background shader cache poisoned");
        if let Some(compiled) = &*cached {
            return compiled.clone();
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaanim-background-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(complete_shader(
                &self.source,
                self.contract,
            ))),
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
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gaanim-background-shader-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("gaanim_render_background"),
            compilation_options: Default::default(),
            cache: None,
        });
        let compiled = Arc::new(CompiledShader {
            bind_group_layout,
            pipeline,
        });
        *cached = Some(compiled.clone());
        compiled
    }
}

#[derive(Clone, Debug, Error)]
pub enum ShaderBackgroundError {
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
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
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

    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let compiled = shader.compiled_shader(device);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
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
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut uniform_bytes = [0_u8; 16];
    uniform_bytes[..4].copy_from_slice(&time.to_ne_bytes());
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("gaanim-background-shader-time"),
        contents: &uniform_bytes,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gaanim-background-shader-bind-group"),
        layout: &compiled.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
        ],
    });

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
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("gaanim-background-shader-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&compiled.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
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
    if let Some(error) = pollster::block_on(device.pop_error_scope()) {
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

#[cfg(test)]
mod tests {
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

    #[test]
    fn shader_validation_rejects_a_missing_entry_function() {
        let error = ShaderBackground::new("fn other() {}", Color::BLACK).unwrap_err();
        assert!(matches!(error, ShaderBackgroundError::InvalidWgsl(_)));
    }
}
