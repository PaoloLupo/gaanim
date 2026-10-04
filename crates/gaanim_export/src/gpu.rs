use gaanim_renderer::pipeline::TransitionScenes;
use gaanim_renderer::post_process::{GpuPostProcess, PostProcessRequest, TransitionInputs};
use std::sync::{Arc, Mutex, mpsc};
use thiserror::Error;
use vello::RendererOptions;
use vello::wgpu::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, Limits, MapMode, Origin3d,
    TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

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
/// Bytes of staging buffers frames may hold while the encoder writes them.
const STAGING_BUDGET: u64 = 256 << 20;

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

/// Everything [`GpuContext::submit_frame`] renders for one frame; see
/// [`GpuContext::render_frame_layers`] for each part.
pub struct FrameJob {
    pub scene: vello::Scene,
    pub layers: Option<TransitionScenes>,
    pub effects: Vec<gaanim_renderer::object_effects::EffectLayer>,
    pub backgrounds: Vec<gaanim_renderer::background::ShaderBackgroundRequest>,
    pub base_color: vello::peniko::Color,
    pub post: Option<PostProcessRequest>,
}

impl FrameJob {
    /// Whether this frame renders the same pixels as `other`, so that
    /// `other`'s can be reused. Shader backgrounds and transitions always
    /// count as different.
    pub fn same_output(&self, other: &Self) -> bool {
        self.layers.is_none()
            && other.layers.is_none()
            && self.backgrounds.is_empty()
            && other.backgrounds.is_empty()
            && self.base_color == other.base_color
            && self.effects.len() == other.effects.len()
            && self
                .effects
                .iter()
                .zip(&other.effects)
                .all(|(effect, other)| effect.same_output(other))
            && match (&self.post, &other.post) {
                (None, None) => true,
                (Some(post), Some(other)) => post.same_output(other),
                _ => false,
            }
            && gaanim_renderer::canvas::draws_same(&self.scene, &other.scene)
    }
}

pub struct GpuContext {
    device: vello::wgpu::Device,
    queue: vello::wgpu::Queue,
    renderer: vello::Renderer,
    texture: vello::wgpu::Texture,
    texture_view: vello::wgpu::TextureView,
    /// The buffer the next frame is copied into.
    staging: Option<vello::wgpu::Buffer>,
    /// Staging buffers that [`MappedFrame`]s released.
    spare_staging: mpsc::Receiver<vello::wgpu::Buffer>,
    release_staging: mpsc::Sender<vello::wgpu::Buffer>,
    /// Staging buffers created so far, up to `max_staging`.
    staging_count: usize,
    max_staging: usize,
    /// One probe pixel per `PROBE_STRIDE` bytes, read before post-processing.
    probe: vello::wgpu::Buffer,
    width: u32,
    height: u32,
    padded_width: u32,
    pending_error: Arc<Mutex<Option<GpuContextError>>>,
    post: GpuPostProcess,
    /// Targets of a shader transition's incoming segment and of the layer
    /// above its blend, created on first use.
    transition_targets: Option<[(vello::wgpu::Texture, vello::wgpu::TextureView); 2]>,
    /// Textures of drawables drawn through a shader effect.
    effects: gaanim_renderer::object_effects::ObjectEffects,
    /// The textures of shader backgrounds, drawn on this device.
    backgrounds: gaanim_renderer::background::GpuShaderBackgrounds,
    /// Multiplier of Vello's bump buffers; it only grows during an export.
    bump_scale: u32,
    /// Largest Vello buffer the device can bind.
    max_bump_bytes: u64,
    /// The frame of [`Self::submit_frame`] whose pixels are not read yet.
    pending: Option<FrameJob>,
    /// Time spent blocked on readbacks, for the benchmark harness.
    readback_wait: std::cell::Cell<std::time::Duration>,
}

impl GpuContext {
    pub fn new(width: u32, height: u32) -> Result<Self, GpuContextError> {
        let adapter = gaanim_renderer::adapter::request_headless_adapter()
            .map_err(|e| GpuContextError::Adapter(e.to_string()))?;
        if std::env::var_os("GAANIM_BENCHMARK_SCENARIO").is_some() {
            let info = adapter.get_info();
            println!(
                "GAANIM_GPU_ADAPTER backend={:?} type={:?} name={}",
                info.backend, info.device_type, info.name
            );
        }

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
        // Vello's gradient ramps are one texture row each: a smooth-shaded 3D
        // mesh draws thousands, past wgpu's default 8192. The preview gets
        // the adapter's limit from Bevy; so does the exporter.
        let max_texture_dimension_2d = supported
            .max_texture_dimension_2d
            .max(defaults.max_texture_dimension_2d);
        let (device, queue) =
            pollster::block_on(adapter.request_device(&vello::wgpu::DeviceDescriptor {
                label: Some("gaanim-export-gpu"),
                required_features: vello::wgpu::Features::empty(),
                required_limits: Limits {
                    max_buffer_size,
                    max_storage_buffer_binding_size: max_storage,
                    max_texture_dimension_2d,
                    ..defaults
                },
                ..Default::default()
            }))
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
                    vello::wgpu::Error::OutOfMemory { .. } => GpuContextError::OutOfMemory,
                    vello::wgpu::Error::Validation { description, .. } => {
                        GpuContextError::Validation(description)
                    }
                    vello::wgpu::Error::Internal { description, .. } => {
                        GpuContextError::Internal(description)
                    }
                };
                pending_error
                    .lock()
                    .expect("export GPU error state poisoned")
                    .get_or_insert(captured);
            }));
        }

        let renderer = vello::Renderer::new(
            &device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::all(),
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
        let staging = create_staging(&device, padded_width, height);
        let (release_staging, spare_staging) = mpsc::channel();
        let frame_bytes = u64::from(padded_width) * u64::from(height) * 4;
        let max_staging = (STAGING_BUDGET / frame_bytes).clamp(3, 10) as usize;
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
            staging: Some(staging),
            spare_staging,
            release_staging,
            staging_count: 1,
            max_staging,
            probe,
            width,
            height,
            padded_width,
            pending_error,
            post: GpuPostProcess::default(),
            transition_targets: None,
            effects: Default::default(),
            backgrounds: Default::default(),
            bump_scale: 1,
            max_bump_bytes: max_storage,
            pending: None,
            readback_wait: Default::default(),
        })
    }

    /// Render the incoming segment over `base_color` and the layer above
    /// over transparency into their own targets.
    fn render_transition_layers(
        &mut self,
        layers: &TransitionScenes,
        base_color: vello::peniko::Color,
    ) -> Result<(), GpuContextError> {
        let (width, height) = (self.width, self.height);
        let device = &self.device;
        let targets = self.transition_targets.get_or_insert_with(|| {
            [
                "gaanim-export-transition-incoming",
                "gaanim-export-transition-above",
            ]
            .map(|label| {
                let texture = device.create_texture(&TextureDescriptor {
                    label: Some(label),
                    size: Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rgba8Unorm,
                    usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                (texture, view)
            })
        });
        for ((_, view), (scene, base)) in targets.iter().zip([
            (&layers.incoming, base_color),
            (&layers.above, vello::peniko::Color::TRANSPARENT),
        ]) {
            self.renderer
                .render_to_texture(
                    &self.device,
                    &self.queue,
                    scene,
                    view,
                    &vello::RenderParams {
                        base_color: base,
                        width,
                        height,
                        antialiasing_method: vello::AaConfig::Msaa16,
                    },
                )
                .map_err(|e| GpuContextError::Render(e.to_string()))?;
        }
        self.check_error()
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

    /// Time spent so far blocked on frame readbacks.
    pub fn readback_wait(&self) -> std::time::Duration {
        self.readback_wait.get()
    }

    /// Block until `buffer` is mapped for reading.
    fn map_read(&self, buffer: &vello::wgpu::Buffer) -> Result<(), GpuContextError> {
        let started = std::time::Instant::now();
        let result = self.map_read_blocking(buffer);
        self.readback_wait
            .set(self.readback_wait.get() + started.elapsed());
        result
    }

    fn map_read_blocking(&self, buffer: &vello::wgpu::Buffer) -> Result<(), GpuContextError> {
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        buffer.slice(..).map_async(MapMode::Read, move |result| {
            let _ = tx.send(result.map_err(|e| format!("Buffer map failed: {e}")));
        });
        loop {
            let _ = self.device.poll(vello::wgpu::PollType::Poll);
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
        scene: &vello::Scene,
        base_color: vello::peniko::Color,
        post: Option<&PostProcessRequest>,
    ) -> Result<Vec<u8>, GpuContextError> {
        self.render_frame_layers(scene, None, &[], &[], base_color, post)
    }

    /// [`Self::render_frame`] for a frame under a shader transition: `scene`
    /// is the outgoing segment, `layers` the incoming one and the layer
    /// above, which `post` (with its transition) blends. `effects` fill the
    /// images of drawables drawn through a shader effect, and `backgrounds`
    /// the images of shader backgrounds.
    pub fn render_frame_layers(
        &mut self,
        scene: &vello::Scene,
        layers: Option<&TransitionScenes>,
        effects: &[gaanim_renderer::object_effects::EffectLayer],
        backgrounds: &[gaanim_renderer::background::ShaderBackgroundRequest],
        base_color: vello::peniko::Color,
        post: Option<&PostProcessRequest>,
    ) -> Result<Vec<u8>, GpuContextError> {
        loop {
            if let Some(pixels) =
                self.render_attempt(scene, layers, effects, backgrounds, base_color, post)?
            {
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

    /// The frame given to [`Self::submit_frame`] and not finished yet.
    pub fn pending_frame(&self) -> Option<&FrameJob> {
        self.pending.as_ref()
    }

    /// Render `frame` and leave its readback pending, so that the caller can
    /// prepare the next frame while the GPU draws this one;
    /// [`Self::finish_frame`] returns its pixels. A frame already pending
    /// must be finished first.
    pub fn submit_frame(&mut self, frame: FrameJob) -> Result<(), GpuContextError> {
        assert!(
            self.pending.is_none(),
            "finish the pending frame before submitting another"
        );
        self.submit_attempt(
            &frame.scene,
            frame.layers.as_ref(),
            &frame.effects,
            &frame.backgrounds,
            frame.base_color,
            frame.post.as_ref(),
        )?;
        self.pending = Some(frame);
        Ok(())
    }

    /// The pixels of the frame given to [`Self::submit_frame`], as
    /// [`Self::render_frame_layers`] returns them: a frame Vello skipped is
    /// rendered again with larger buffers.
    pub fn finish_frame(&mut self) -> Result<FramePixels, GpuContextError> {
        let frame = self
            .pending
            .take()
            .expect("finish_frame needs a frame given to submit_frame");
        if self.read_probes()? {
            return self.read_mapped().map(FramePixels::Mapped);
        }
        if self.bump_scale >= MAX_BUMP_SCALE {
            return Err(GpuContextError::SceneTooComplex {
                scale: self.bump_scale,
            });
        }
        self.bump_scale *= 2;
        self.render_frame_layers(
            &frame.scene,
            frame.layers.as_ref(),
            &frame.effects,
            &frame.backgrounds,
            frame.base_color,
            frame.post.as_ref(),
        )
        .map(FramePixels::Owned)
    }

    /// One render and readback; `None` when Vello skipped the frame.
    fn render_attempt(
        &mut self,
        scene: &vello::Scene,
        layers: Option<&TransitionScenes>,
        effects: &[gaanim_renderer::object_effects::EffectLayer],
        backgrounds: &[gaanim_renderer::background::ShaderBackgroundRequest],
        base_color: vello::peniko::Color,
        post: Option<&PostProcessRequest>,
    ) -> Result<Option<Vec<u8>>, GpuContextError> {
        self.submit_attempt(scene, layers, effects, backgrounds, base_color, post)?;
        self.read_attempt()
    }

    /// Render a frame and queue the copies of its probes and pixels, without
    /// waiting for the GPU; [`Self::read_attempt`] reads them.
    fn submit_attempt(
        &mut self,
        scene: &vello::Scene,
        layers: Option<&TransitionScenes>,
        effects: &[gaanim_renderer::object_effects::EffectLayer],
        backgrounds: &[gaanim_renderer::background::ShaderBackgroundRequest],
        base_color: vello::peniko::Color,
        post: Option<&PostProcessRequest>,
    ) -> Result<(), GpuContextError> {
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
        // The shader backgrounds and the effects fill images the frame
        // draws, so they render first. The backgrounds stay on the GPU: no
        // readback, no upload of their pixels.
        self.backgrounds
            .prepare(&self.device, &self.queue, &mut self.renderer, backgrounds);
        // Effects fill the images the frame draws, so they render first.
        if !effects.is_empty() || self.effects.is_active() {
            self.effects
                .render(
                    &self.device,
                    &self.queue,
                    &mut self.renderer,
                    effects,
                    vello::AaConfig::Msaa16,
                )
                .map_err(|e| GpuContextError::Render(e.to_string()))?;
        }
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                scene,
                &self.texture_view,
                &vello::RenderParams {
                    base_color,
                    width: self.width,
                    height: self.height,
                    antialiasing_method: vello::AaConfig::Msaa16,
                },
            )
            .map_err(|e| GpuContextError::Render(e.to_string()))?;
        self.check_error()?;
        if let Some(layers) = layers {
            self.render_transition_layers(layers, base_color)?;
        }

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
        let inputs = layers
            .and(self.transition_targets.as_ref())
            .map(|[incoming, above]| TransitionInputs {
                incoming: &incoming.0,
                above: &above.0,
            });
        if self
            .post
            .prepare(&self.device, &self.queue, &self.texture, post, inputs)
        {
            self.post.encode(&mut encoder);
        }

        let staging = self.take_staging();
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &staging,
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

        self.staging = Some(staging);

        self.queue.submit(Some(encoder.finish()));
        self.check_error()
    }

    /// A staging buffer no frame holds: the current one, one a frame
    /// released, a new one, or the next one released once `max_staging`
    /// frames hold one.
    fn take_staging(&mut self) -> vello::wgpu::Buffer {
        if let Some(staging) = self.staging.take() {
            return staging;
        }
        if let Ok(staging) = self.spare_staging.try_recv() {
            return staging;
        }
        if self.staging_count < self.max_staging {
            self.staging_count += 1;
            return create_staging(&self.device, self.padded_width, self.height);
        }
        // This context keeps a sender, so the channel never disconnects; a
        // frame held by the encoder is released when it is written or dropped.
        self.spare_staging
            .recv()
            .expect("the GPU context keeps a sender of staging buffers")
    }

    /// Wait for the frame of the last [`Self::submit_attempt`] and read it
    /// back; `None` when Vello skipped it.
    fn read_attempt(&mut self) -> Result<Option<Vec<u8>>, GpuContextError> {
        if !self.read_probes()? {
            return Ok(None);
        }
        Ok(Some(self.read_mapped()?.into_pixels()))
    }

    /// Whether Vello drew the frame of the last [`Self::submit_attempt`],
    /// read from its probe pixels.
    fn read_probes(&mut self) -> Result<bool, GpuContextError> {
        self.map_read(&self.probe)?;
        let skipped = {
            let probes = self.probe.slice(..).get_mapped_range();
            probes
                .chunks(PROBE_STRIDE as usize)
                .all(|probe| probe[..4] == PROBE_SENTINEL)
        };
        self.probe.unmap();
        Ok(!skipped)
    }

    /// The pixels of the last [`Self::submit_attempt`], left in its mapped
    /// staging buffer.
    fn read_mapped(&mut self) -> Result<MappedFrame, GpuContextError> {
        let staging = self
            .staging
            .take()
            .expect("a submitted frame has a staging buffer");
        if let Err(error) = self.map_read(&staging) {
            self.staging = Some(staging);
            return Err(error);
        }
        Ok(MappedFrame {
            buffer: Some(staging),
            width: self.width,
            height: self.height,
            padded_width: self.padded_width,
            release: self.release_staging.clone(),
        })
    }
}

fn create_staging(
    device: &vello::wgpu::Device,
    padded_width: u32,
    height: u32,
) -> vello::wgpu::Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some("gaanim-export-staging"),
        size: u64::from(padded_width) * u64::from(height) * 4,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The pixels of a frame, as tightly packed RGBA8 rows.
pub enum FramePixels {
    Owned(Vec<u8>),
    /// The pixels of a frame repeated while the scene holds still.
    Shared(Arc<[u8]>),
    /// Still in the GPU's staging buffer: the thread that writes the frame
    /// reads them, so the render loop never copies them.
    Mapped(MappedFrame),
}

impl FramePixels {
    pub fn into_pixels(self) -> Vec<u8> {
        match self {
            Self::Owned(pixels) => pixels,
            Self::Shared(pixels) => pixels.to_vec(),
            Self::Mapped(frame) => frame.into_pixels(),
        }
    }

    /// Write the rows to `out`, without copying a mapped frame first.
    pub fn write_to(&self, out: &mut impl std::io::Write) -> std::io::Result<()> {
        match self {
            Self::Owned(pixels) => out.write_all(pixels),
            Self::Shared(pixels) => out.write_all(pixels),
            Self::Mapped(frame) => frame.write_to(out),
        }
    }
}

impl From<Vec<u8>> for FramePixels {
    fn from(pixels: Vec<u8>) -> Self {
        Self::Owned(pixels)
    }
}

/// A frame read back into a staging buffer that stays mapped until the
/// frame is dropped, which unmaps it and returns it to its GPU context.
pub struct MappedFrame {
    buffer: Option<vello::wgpu::Buffer>,
    width: u32,
    height: u32,
    /// Row stride of the buffer in pixels (rows are aligned for the copy).
    padded_width: u32,
    release: mpsc::Sender<vello::wgpu::Buffer>,
}

impl MappedFrame {
    pub fn into_pixels(self) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(self.width as usize * self.height as usize * 4);
        self.write_to(&mut pixels)
            .expect("writing to a Vec does not fail");
        pixels
    }

    pub fn write_to(&self, out: &mut impl std::io::Write) -> std::io::Result<()> {
        let buffer = self.buffer.as_ref().expect("the buffer is held until drop");
        let data = buffer.slice(..).get_mapped_range();
        let row = self.width as usize * 4;
        if self.padded_width == self.width {
            return out.write_all(&data[..row * self.height as usize]);
        }
        let stride = self.padded_width as usize * 4;
        for start in (0..self.height as usize).map(|y| y * stride) {
            out.write_all(&data[start..start + row])?;
        }
        Ok(())
    }
}

impl Drop for MappedFrame {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            buffer.unmap();
            // The context may be gone already; the buffer is then freed.
            let _ = self.release.send(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameJob, FramePixels, GpuContext, GpuContextError};

    #[test]
    #[ignore = "requires a GPU adapter; run explicitly for raster replay validation"]
    fn raster_images_survive_vector_only_frames_and_replay() {
        use vello::{Scene, kurbo::Affine, peniko};

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
            &vello::kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
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
    fn frames_with_more_gradients_than_wgpu_default_texture_rows_render() {
        use vello::{Scene, kurbo, peniko};

        let Ok(mut gpu) = GpuContext::new(64, 64) else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        // Each distinct gradient takes a row of Vello's ramp texture; a
        // smooth-shaded 3D mesh draws one per visible triangle.
        let gradients = 9000;
        if gpu.device.limits().max_texture_dimension_2d < gradients {
            eprintln!("skipped: the adapter allows fewer texture rows");
            return;
        }
        let mut scene = Scene::new();
        for index in 0..gradients {
            let shade = index as f32 / gradients as f32;
            let gradient = peniko::Gradient::new_linear((0.0, 0.0), (64.0, 0.0)).with_stops([
                peniko::Color::new([shade, 0.0, 1.0 - shade, 1.0]),
                peniko::Color::new([0.0, shade, 0.0, 1.0]),
            ]);
            scene.fill(
                peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                &peniko::Brush::Gradient(gradient),
                None,
                &kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
            );
        }
        let pixels = gpu
            .render_frame(&scene, peniko::Color::BLACK, None)
            .expect("a frame with 9000 gradients renders");
        // The last gradient covers the frame.
        assert_ne!(&pixels[..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn post_process_changes_only_the_camera_frame() {
        use gaanim_renderer::post_process::{CanvasPostProcess, PostProcessShader};
        use vello::{Scene, kurbo, peniko};

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
            passes: vec![shader.into()],
            ..Default::default()
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
        use vello::{Scene, kurbo, peniko};

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
        use vello::{Scene, kurbo, peniko};

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

    fn frame_job(scene: vello::Scene) -> FrameJob {
        FrameJob {
            scene,
            layers: None,
            effects: Vec::new(),
            backgrounds: Vec::new(),
            base_color: vello::peniko::Color::from_rgb8(9, 11, 23),
            post: None,
        }
    }

    #[test]
    fn submitted_frames_match_rendered_ones_and_recycle_their_buffers() {
        use vello::{Scene, kurbo, peniko};

        let Ok(mut gpu) = GpuContext::new(100, 30) else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
        let scene = |index: u8| {
            let mut scene = Scene::new();
            scene.fill(
                peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                peniko::Color::from_rgb8(index * 20, 200, 40),
                None,
                &kurbo::Circle::new((f64::from(index) * 9.0, 15.0), 12.0),
            );
            scene
        };
        let mut held = Vec::new();
        for index in 0..10 {
            let expected = gpu
                .render_frame(&scene(index), peniko::Color::from_rgb8(9, 11, 23), None)
                .unwrap();
            gpu.submit_frame(frame_job(scene(index))).unwrap();
            let frame = gpu.finish_frame().unwrap();
            assert!(matches!(frame, FramePixels::Mapped(_)));
            let mut written = Vec::new();
            frame.write_to(&mut written).unwrap();
            assert_eq!(written, expected, "frame {index}");
            // Frames the encoder still holds keep their buffers.
            held.push(frame);
            if held.len() > 2 {
                assert_eq!(held.remove(0).into_pixels().len(), 100 * 30 * 4);
            }
        }
        assert!(gpu.staging_count <= 4, "{} buffers", gpu.staging_count);
    }

    #[test]
    fn overflowing_submitted_frames_retry_with_larger_buffers() {
        use vello::{Scene, kurbo, peniko};

        let Ok(mut gpu) = GpuContext::new(1920, 1080) else {
            eprintln!("skipped: no GPU adapter");
            return;
        };
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
        gpu.submit_frame(frame_job(scene)).unwrap();
        let rendered = gpu.finish_frame().unwrap().into_pixels();
        assert!(gpu.bump_scale > 1, "the first attempt overflowed");
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
