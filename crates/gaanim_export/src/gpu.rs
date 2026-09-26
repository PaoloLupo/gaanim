use bevy_vello::vello::RendererOptions;
use bevy_vello::vello::wgpu::{
    Backends, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, Instance,
    InstanceDescriptor, Limits, MapMode, Origin3d, PowerPreference, RequestAdapterOptions,
    TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use gaanim_renderer::post_process::{GpuPostProcess, PostProcessRequest};
use std::sync::{Arc, Mutex, mpsc};
use thiserror::Error;

/// Written to the probe pixels before each render. Vello writes every pixel
/// of its target unless a stage overflowed its fixed-size buffers, in which
/// case it writes none; probes still holding this value mark such a frame.
const PROBE_SENTINEL: [u8; 4] = [0x5a, 0xc3, 0x17, 0x01];
/// Byte stride of the probe readback buffer (texture copy row alignment).
const PROBE_STRIDE: u64 = 256;
/// Largest multiplier of Vello's buffers tried before giving up on a frame.
const MAX_BUMP_SCALE: u32 = 8;
/// Largest storage binding requested from the adapter for Vello's buffers.
const MAX_STORAGE_BINDING: u64 = 512 << 20;

/// A recoverable GPU failure reported while running a direct Vello export.
///
/// A headless export cannot safely recreate its renderer midway through a frame
/// sequence. The caller receives this error, retains the project/session, and
/// can retry the export explicitly with a fresh context.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GpuContextError {
    #[error("no suitable GPU adapter found: {0}")]
    Adapter(String),
    #[error("failed to create GPU device: {0}")]
    Device(String),
    #[error("failed to create Vello renderer: {0}")]
    Renderer(String),
    #[error("GPU device was lost: {0}")]
    DeviceLost(String),
    #[error("GPU ran out of memory")]
    OutOfMemory,
    #[error("GPU validation error: {0}")]
    Validation(String),
    #[error("internal GPU error: {0}")]
    Internal(String),
    #[error("Vello render error: {0}")]
    Render(String),
    #[error("GPU readback failed: {0}")]
    Readback(String),
    #[error(
        "the frame needs more GPU memory than the renderer can use, even with {scale}× its \
         buffers; reduce the number of large overlapping or translucent objects, or export at a \
         lower resolution"
    )]
    SceneTooComplex { scale: u32 },
}

impl GpuContextError {
    /// Whether retrying requires a fresh GPU context rather than another frame.
    pub const fn requires_new_context(&self) -> bool {
        matches!(
            self,
            Self::DeviceLost(_) | Self::OutOfMemory | Self::Internal(_)
        )
    }
}

pub struct GpuContext {
    device: bevy_vello::vello::wgpu::Device,
    queue: bevy_vello::vello::wgpu::Queue,
    renderer: bevy_vello::vello::Renderer,
    texture: bevy_vello::vello::wgpu::Texture,
    texture_view: bevy_vello::vello::wgpu::TextureView,
    staging: bevy_vello::vello::wgpu::Buffer,
    /// One probe pixel per `PROBE_STRIDE` bytes, read before post-processing.
    probe: bevy_vello::vello::wgpu::Buffer,
    width: u32,
    height: u32,
    padded_width: u32,
    pending_error: Arc<Mutex<Option<GpuContextError>>>,
    post: GpuPostProcess,
    /// Multiplier of Vello's bump buffers; it only grows during an export.
    bump_scale: u32,
    /// Largest Vello buffer the device can bind.
    max_bump_bytes: u64,
}

impl GpuContext {
    pub fn new(width: u32, height: u32) -> Result<Self, GpuContextError> {
        // `WGPU_BACKEND` (vulkan, dx12, metal, gl) narrows the search like it
        // does for the preview window; CI uses it to skip broken adapters.
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all().with_env(),
            ..InstanceDescriptor::new_without_display_handle()
        });

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| GpuContextError::Adapter(e.to_string()))?;

        // Complex scenes need larger Vello buffers than wgpu's default
        // 128 MiB binding; request what the adapter allows, up to a bound.
        let supported = adapter.limits();
        let defaults = Limits::default();
        let max_buffer_size = supported
            .max_buffer_size
            .min(MAX_STORAGE_BINDING)
            .max(defaults.max_buffer_size);
        let max_storage = supported
            .max_storage_buffer_binding_size
            .min(max_buffer_size)
            .max(defaults.max_storage_buffer_binding_size);
        let (device, queue) = pollster::block_on(adapter.request_device(
            &bevy_vello::vello::wgpu::DeviceDescriptor {
                label: Some("gaanim-export-gpu"),
                required_features: bevy_vello::vello::wgpu::Features::empty(),
                required_limits: Limits {
                    max_buffer_size,
                    max_storage_buffer_binding_size: max_storage,
                    ..defaults
                },
                ..Default::default()
            },
        ))
        .map_err(|e| GpuContextError::Device(e.to_string()))?;

        let pending_error = Arc::new(Mutex::new(None));
        {
            let pending_error = pending_error.clone();
            device.set_device_lost_callback(move |reason, description| {
                let message = format!("{reason:?}: {description}");
                pending_error
                    .lock()
                    .expect("export GPU error state poisoned")
                    .get_or_insert(GpuContextError::DeviceLost(message));
            });
        }
        {
            let pending_error = pending_error.clone();
            device.on_uncaptured_error(Arc::new(move |error| {
                let captured = match error {
                    bevy_vello::vello::wgpu::Error::OutOfMemory { .. } => {
                        GpuContextError::OutOfMemory
                    }
                    bevy_vello::vello::wgpu::Error::Validation { description, .. } => {
                        GpuContextError::Validation(description)
                    }
                    bevy_vello::vello::wgpu::Error::Internal { description, .. } => {
                        GpuContextError::Internal(description)
                    }
                };
                pending_error
                    .lock()
                    .expect("export GPU error state poisoned")
                    .get_or_insert(captured);
            }));
        }

        let renderer = bevy_vello::vello::Renderer::new(
            &device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: bevy_vello::vello::AaSupport::all(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map_err(|e| GpuContextError::Renderer(e.to_string()))?;

        let format = TextureFormat::Rgba8Unorm;
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("gaanim-export-target"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::STORAGE_BINDING
                | TextureUsages::COPY_SRC
                | TextureUsages::COPY_DST
                | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&Default::default());

        let padded_width = (width + 63) & !63;
        let buffer_size = (padded_width as u64) * (height as u64) * 4;
        let staging = device.create_buffer(&BufferDescriptor {
            label: Some("gaanim-export-staging"),
            size: buffer_size,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let probe = device.create_buffer(&BufferDescriptor {
            label: Some("gaanim-export-probe"),
            size: PROBE_STRIDE * 5,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            renderer,
            texture,
            texture_view,
            staging,
            probe,
            width,
            height,
            padded_width,
            pending_error,
            post: GpuPostProcess::default(),
            bump_scale: 1,
            max_bump_bytes: max_storage,
        })
    }

    /// Corners and center of the target.
    fn probe_points(&self) -> [(u32, u32); 5] {
        let (right, bottom) = (self.width - 1, self.height - 1);
        [
            (0, 0),
            (right, 0),
            (0, bottom),
            (right, bottom),
            (self.width / 2, self.height / 2),
        ]
    }

    /// Block until `buffer` is mapped for reading.
    fn map_read(&self, buffer: &bevy_vello::vello::wgpu::Buffer) -> Result<(), GpuContextError> {
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        buffer.slice(..).map_async(MapMode::Read, move |result| {
            let _ = tx.send(result.map_err(|e| format!("Buffer map failed: {e}")));
        });
        loop {
            let _ = self.device.poll(bevy_vello::vello::wgpu::PollType::Poll);
            self.check_error()?;
            match rx.try_recv() {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(e)) => return Err(GpuContextError::Readback(e)),
                Err(mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(GpuContextError::Readback(
                        "buffer map channel disconnected".to_string(),
                    ));
                }
            }
        }
    }

    fn check_error(&self) -> Result<(), GpuContextError> {
        match self
            .pending_error
            .lock()
            .expect("export GPU error state poisoned")
            .take()
        {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Render `scene`, apply `post` inside its camera frame, and read the
    /// frame back as tightly packed RGBA8 rows.
    ///
    /// A scene too complex for Vello's buffers renders no pixels at all; the
    /// frame is then rendered again with larger buffers, which later frames
    /// keep, until it fits or the device limit is reached.
    pub fn render_frame(
        &mut self,
        scene: &bevy_vello::vello::Scene,
        base_color: bevy_vello::vello::peniko::Color,
        post: Option<&PostProcessRequest>,
    ) -> Result<Vec<u8>, GpuContextError> {
        loop {
            if let Some(pixels) = self.render_attempt(scene, base_color, post)? {
                return Ok(pixels);
            }
            if self.bump_scale >= MAX_BUMP_SCALE {
                return Err(GpuContextError::SceneTooComplex {
                    scale: self.bump_scale,
                });
            }
            self.bump_scale *= 2;
        }
    }

    /// One render and readback; `None` when Vello skipped the frame.
    fn render_attempt(
        &mut self,
        scene: &bevy_vello::vello::Scene,
        base_color: bevy_vello::vello::peniko::Color,
        post: Option<&PostProcessRequest>,
    ) -> Result<Option<Vec<u8>>, GpuContextError> {
        self.check_error()?;
        // Vello reads these on the thread that renders.
        vello_encoding::set_bump_buffer_scale(self.bump_scale);
        vello_encoding::set_max_bump_buffer_bytes(self.max_bump_bytes);
        let probe_points = self.probe_points();
        for (x, y) in probe_points {
            self.queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: Origin3d { x, y, z: 0 },
                    aspect: TextureAspect::All,
                },
                &PROBE_SENTINEL,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: None,
                    rows_per_image: None,
                },
                Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                scene,
                &self.texture_view,
                &bevy_vello::vello::RenderParams {
                    base_color,
                    width: self.width,
                    height: self.height,
                    antialiasing_method: bevy_vello::vello::AaConfig::Msaa16,
                },
            )
            .map_err(|e| GpuContextError::Render(e.to_string()))?;
        self.check_error()?;

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("gaanim-export-copy"),
            });
        // Probe before post-processing, which rewrites the frame.
        for (index, (x, y)) in probe_points.into_iter().enumerate() {
            encoder.copy_texture_to_buffer(
                TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: Origin3d { x, y, z: 0 },
                    aspect: TextureAspect::All,
                },
                TexelCopyBufferInfo {
                    buffer: &self.probe,
                    layout: TexelCopyBufferLayout {
                        offset: index as u64 * PROBE_STRIDE,
                        bytes_per_row: None,
                        rows_per_image: None,
                    },
                },
                Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
        if self
            .post
            .prepare(&self.device, &self.queue, &self.texture, post)
        {
            self.post.encode(&mut encoder);
        }

        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &self.staging,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_width * 4),
                    rows_per_image: None,
                },
            },
            Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(Some(encoder.finish()));
        self.check_error()?;

        self.map_read(&self.probe)?;
        let skipped = {
            let probes = self.probe.slice(..).get_mapped_range();
            let skipped = probes
                .chunks(PROBE_STRIDE as usize)
                .all(|probe| probe[..4] == PROBE_SENTINEL);
            drop(probes);
            skipped
        };
        self.probe.unmap();
        if skipped {
            return Ok(None);
        }

        self.map_read(&self.staging)?;
        let slice = self.staging.slice(..);
        let pixels = {
            let data = slice.get_mapped_range();
            let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
            for row in 0..self.height {
                let start = (row * self.padded_width * 4) as usize;
                let end = start + (self.width * 4) as usize;
                pixels.extend_from_slice(&data[start..end]);
            }
            drop(data);
            pixels
        };

        self.staging.unmap();

        Ok(Some(pixels))
    }
}

#[cfg(test)]
mod tests {
    use super::{GpuContext, GpuContextError};

    #[test]
    #[ignore = "requires a GPU adapter; run explicitly for raster replay validation"]
    fn raster_images_survive_vector_only_frames_and_replay() {
        use bevy_vello::vello::{Scene, kurbo::Affine, peniko};

        let mut gpu = GpuContext::new(32, 32).expect("GPU context");
        let image = peniko::ImageData {
            data: peniko::Blob::new(std::sync::Arc::new([255, 0, 0, 255].repeat(4))),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: 2,
            height: 2,
        };
        let brush = peniko::ImageBrush::new(image);
        let mut image_scene = Scene::new();
        image_scene.draw_image(&brush, Affine::scale(16.0));
        let mut vector_scene = Scene::new();
        vector_scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::from_rgb8(0, 0, 255),
            None,
            &bevy_vello::vello::kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
        );

        let first = gpu
            .render_frame(&image_scene, peniko::Color::BLACK, None)
            .unwrap();
        assert_eq!(&first[(16 * 32 + 16) * 4..][..4], &[255, 0, 0, 255]);
        for pass in 1..=3 {
            let vectors = gpu
                .render_frame(&vector_scene, peniko::Color::BLACK, None)
                .unwrap();
            assert_eq!(&vectors[(16 * 32 + 16) * 4..][..4], &[0, 0, 255, 255]);
            let replay = gpu
                .render_frame(&image_scene, peniko::Color::BLACK, None)
                .unwrap();
            assert_eq!(
                &replay[(16 * 32 + 16) * 4..][..4],
                &first[(16 * 32 + 16) * 4..][..4],
                "image pixels must survive returning from a vector-only slide (pass {pass})"
            );
            assert_eq!(replay, first, "replay must reproduce the complete image");
        }
    }

    #[test]
    fn post_process_changes_only_the_camera_frame() {
        use bevy_vello::vello::{Scene, kurbo, peniko};
        use gaanim_renderer::post_process::{CanvasPostProcess, PostProcessShader};

        let Ok(mut gpu) = GpuContext::new(32, 16) else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let mut scene = Scene::new();
        scene.fill(
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            peniko::Color::from_rgb8(255, 0, 0),
            None,
            &kurbo::Rect::new(0.0, 0.0, 32.0, 16.0),
        );
        let shader = PostProcessShader::new(
            "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             let color = gaanim_scene(uv);\n\
             return vec4<f32>(color.g, color.r, color.b, color.a);\n}",
        )
        .unwrap();
        let request = CanvasPostProcess {
            shader: Some(shader),
            segments: Vec::new(),
        }
        .request(0.0, kurbo::Rect::new(8.0, 4.0, 24.0, 12.0))
        .unwrap();

        let plain = gpu
            .render_frame(&scene, peniko::Color::BLACK, None)
            .unwrap();
        let processed = gpu
            .render_frame(&scene, peniko::Color::BLACK, Some(&request))
            .unwrap();
        let at = |pixels: &[u8], x: usize, y: usize| pixels[(y * 32 + x) * 4..][..4].to_vec();
        assert_eq!(at(&plain, 16, 8), [255, 0, 0, 255]);
        assert_eq!(at(&processed, 16, 8), [0, 255, 0, 255], "inside the frame");
        assert_eq!(
            at(&processed, 8, 4),
            [0, 255, 0, 255],
            "top-left frame pixel"
        );
        assert_eq!(at(&processed, 2, 2), [255, 0, 0, 255], "outside the frame");
        assert_eq!(at(&processed, 24, 12), [255, 0, 0, 255], "past the frame");
        let again = gpu
            .render_frame(&scene, peniko::Color::BLACK, None)
            .unwrap();
        assert_eq!(again, plain, "a frame without post is untouched");
    }

    fn pixel(frame: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        [frame[at], frame[at + 1], frame[at + 2], frame[at + 3]]
    }

    #[test]
    fn frame_spanning_paths_render_without_a_retry() {
        use bevy_vello::vello::{Scene, kurbo, peniko};

        let Ok(mut gpu) = GpuContext::new(1920, 1080) else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        // Each diagonal takes every tile of its 1080p bounding box, which
        // overflowed Vello's fixed tile buffer at a few hundred paths.
        let mut scene = Scene::new();
        for index in 0..600 {
            let mut line = kurbo::BezPath::new();
            line.move_to((0.0, f64::from(index) * 1.8));
            line.line_to((1920.0, 1080.0 - f64::from(index) * 1.8));
            scene.stroke(
                &kurbo::Stroke::new(2.0),
                kurbo::Affine::IDENTITY,
                peniko::Color::WHITE,
                None,
                &line,
            );
        }
        let frame = gpu
            .render_frame(&scene, peniko::Color::from_rgb8(9, 11, 23), None)
            .unwrap();
        assert_eq!(gpu.bump_scale, 1, "the tile estimate sizes the buffer");
        assert_eq!(pixel(&frame, 1920, 1900, 2), [9, 11, 23, 255], "background");
        // Every diagonal crosses the center.
        assert_eq!(pixel(&frame, 1920, 960, 540), [255, 255, 255, 255]);
    }

    #[test]
    fn overflowing_frames_retry_with_larger_buffers() {
        use bevy_vello::vello::{Scene, kurbo, peniko};

        let Ok(mut gpu) = GpuContext::new(1920, 1080) else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        // Frame-sized translucent layers fill every tile's command list.
        let frame = kurbo::Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let mut scene = Scene::new();
        for index in 0..1000 {
            scene.push_layer(
                peniko::Fill::NonZero,
                peniko::BlendMode::default(),
                0.5,
                kurbo::Affine::IDENTITY,
                &frame,
            );
            let x = f64::from(index % 40) * 48.0;
            let y = f64::from(index / 40) * 43.0;
            scene.fill(
                peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                peniko::Color::WHITE,
                None,
                &kurbo::Rect::new(x, y, x + 4.0, y + 4.0),
            );
            scene.pop_layer();
        }
        let rendered = gpu
            .render_frame(&scene, peniko::Color::from_rgb8(9, 11, 23), None)
            .unwrap();
        assert!(gpu.bump_scale > 1, "the first attempt overflowed");
        assert_eq!(
            pixel(&rendered, 1920, 1900, 1070),
            [9, 11, 23, 255],
            "background"
        );
        assert_ne!(
            pixel(&rendered, 1920, 1, 1),
            [9, 11, 23, 255],
            "first square"
        );
    }

    #[test]
    fn only_terminal_gpu_failures_require_a_fresh_context() {
        assert!(GpuContextError::DeviceLost("driver reset".into()).requires_new_context());
        assert!(GpuContextError::OutOfMemory.requires_new_context());
        assert!(GpuContextError::Internal("backend".into()).requires_new_context());
        assert!(!GpuContextError::Validation("bad pipeline".into()).requires_new_context());
    }
}
