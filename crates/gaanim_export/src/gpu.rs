use bevy_vello::vello::RendererOptions;
use bevy_vello::vello::wgpu::{
    Backends, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, Instance,
    InstanceDescriptor, MapMode, PowerPreference, RequestAdapterOptions, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use std::sync::mpsc;

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
}

impl GpuContext {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| format!("No suitable GPU adapter found: {e}"))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &bevy_vello::vello::wgpu::DeviceDescriptor {
                label: Some("gaanim-export-gpu"),
                required_features: bevy_vello::vello::wgpu::Features::empty(),
                required_limits: bevy_vello::vello::wgpu::Limits::default(),
                ..Default::default()
            },
        ))
        .map_err(|e| format!("Failed to create wgpu device: {e}"))?;

        let renderer = bevy_vello::vello::Renderer::new(
            &device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: bevy_vello::vello::AaSupport::all(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map_err(|e| format!("Failed to create Vello renderer: {e}"))?;

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
        })
    }

    pub fn render_frame(
        &mut self,
        scene: &bevy_vello::vello::Scene,
        base_color: bevy_vello::vello::peniko::Color,
    ) -> Result<Vec<u8>, String> {
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
            .map_err(|e| format!("Vello render error: {e}"))?;

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

        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        let slice = self.staging.slice(..);
        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result.map_err(|e| format!("Buffer map failed: {e}")));
        });

        loop {
            let _ = self.device.poll(bevy_vello::vello::wgpu::PollType::Poll);
            match rx.try_recv() {
                Ok(Ok(())) => break,
                Ok(Err(e)) => return Err(e),
                Err(mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("Buffer map channel disconnected".to_string());
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
