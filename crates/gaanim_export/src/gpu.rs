use bevy_vello::vello::RendererOptions;
use bevy_vello::vello::wgpu::{
    Backends, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, Instance,
    InstanceDescriptor, MapMode, PowerPreference, RequestAdapterOptions, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use std::sync::{Arc, Mutex, mpsc};
use thiserror::Error;

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
    width: u32,
    height: u32,
    padded_width: u32,
    pending_error: Arc<Mutex<Option<GpuContextError>>>,
}

impl GpuContext {
    pub fn new(width: u32, height: u32) -> Result<Self, GpuContextError> {
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            ..InstanceDescriptor::new_without_display_handle()
        });

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| GpuContextError::Adapter(e.to_string()))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &bevy_vello::vello::wgpu::DeviceDescriptor {
                label: Some("gaanim-export-gpu"),
                required_features: bevy_vello::vello::wgpu::Features::empty(),
                required_limits: bevy_vello::vello::wgpu::Limits::default(),
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

        Ok(Self {
            device,
            queue,
            renderer,
            texture,
            texture_view,
            staging,
            width,
            height,
            padded_width,
            pending_error,
        })
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

    pub fn render_frame(
        &mut self,
        scene: &bevy_vello::vello::Scene,
        base_color: bevy_vello::vello::peniko::Color,
    ) -> Result<Vec<u8>, GpuContextError> {
        self.check_error()?;
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

        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        let slice = self.staging.slice(..);
        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result.map_err(|e| format!("Buffer map failed: {e}")));
        });

        loop {
            let _ = self.device.poll(bevy_vello::vello::wgpu::PollType::Poll);
            self.check_error()?;
            match rx.try_recv() {
                Ok(Ok(())) => break,
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

        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::GpuContextError;

    #[test]
    fn only_terminal_gpu_failures_require_a_fresh_context() {
        assert!(GpuContextError::DeviceLost("driver reset".into()).requires_new_context());
        assert!(GpuContextError::OutOfMemory.requires_new_context());
        assert!(GpuContextError::Internal("backend".into()).requires_new_context());
        assert!(!GpuContextError::Validation("bad pipeline".into()).requires_new_context());
    }
}
