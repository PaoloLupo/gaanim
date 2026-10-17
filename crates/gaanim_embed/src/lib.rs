//! Gaanim's embed player: draws `.gaanim` files in other pages, such as a
//! blog post, without the editor.
//!
//! The web player (`gaanim_web`) is the whole playback application, Bevy and
//! egui included. This one only decodes a bundle (`gaanim_bundle`), composes
//! each recorded frame with the engine's own code
//! ([`compose_captured_at`]) and draws it with Vello on WebGPU, so it
//! downloads a fraction of the size. Its page (`web/`) keeps the clock, the
//! download, the controls and the audio; [`Player`] only answers "draw the
//! frame at this time".
//!
//! A frame draws as a preview of the scene does without its GPU passes:
//! shader backgrounds show their fallback color, a shader transition
//! cross-fades, drawables under a shader effect or behind glass draw plainly
//! and post-processing is left out. [`Player::open`] says when a file uses them,
//! so the page can hand it to the web player instead.
//!
//! [`compose_captured_at`]: gaanim_renderer::pipeline::compose_captured_at

#[cfg(target_arch = "wasm32")]
mod web {
    use std::ops::Range;

    use gaanim_bundle::{Bundle, BundleError, BundleSource, Frame};
    use gaanim_renderer::background::BackgroundPaint;
    use gaanim_renderer::fragment::FragmentStore;
    use gaanim_renderer::pipeline::{
        CanvasBackground, CapturedGpuThreeD, camera_to_output, compose_captured_at,
        output_pixels_per_unit,
    };
    use gaanim_renderer::three_d::{GpuMeshFrame, GpuThreeD};
    use vello::peniko::Color;
    use wasm_bindgen::prelude::*;
    use web_sys::HtmlCanvasElement;

    /// Chunks read ahead of the one on screen while a file downloads.
    const READ_AHEAD_CHUNKS: usize = 3;

    /// Draws the frames of one `.gaanim` file on a canvas.
    #[wasm_bindgen]
    pub struct Player {
        gpu: Gpu,
        file: Option<File>,
        /// A file downloading in pieces that has not opened yet.
        pending: Option<BundleSource>,
        /// Byte ranges to download, for [`Player::take_wanted`].
        wanted: Vec<Range<u64>>,
    }

    struct File {
        bundle: Bundle,
        store: FragmentStore,
        background: Option<CanvasBackground>,
        /// Color under the frame.
        base: Color,
        /// The frame on the canvas and the canvas size it was drawn at.
        shown: Option<(usize, u32, u32)>,
    }

    #[wasm_bindgen]
    impl Player {
        /// A player that draws on `canvas`; fails without WebGPU.
        pub async fn create(canvas: HtmlCanvasElement) -> Result<Player, JsError> {
            console_error_panic_hook::set_once();
            Ok(Self {
                gpu: Gpu::new(canvas).await?,
                file: None,
                pending: None,
                wanted: Vec::new(),
            })
        }

        /// Open a `.gaanim` file held whole. Returns what the page needs to
        /// know about it, as JSON (see [`describe_file`]).
        pub fn open(&mut self, bytes: Vec<u8>) -> Result<String, JsValue> {
            self.file = None;
            self.pending = None;
            self.open_source(BundleSource::whole(bytes.into()))?
                .ok_or_else(|| failure("damaged", "Al archivo le faltan partes."))
        }

        /// Start opening a file of `length` bytes that downloads in pieces,
        /// from `tail`, its bytes from `offset` to the end. Returns the file's
        /// JSON once it opens; until then [`Player::take_wanted`] names the
        /// bytes it waits for.
        #[wasm_bindgen(js_name = openRemote)]
        pub fn open_remote(
            &mut self,
            length: f64,
            offset: f64,
            tail: Vec<u8>,
        ) -> Result<Option<String>, JsValue> {
            self.file = None;
            let source = BundleSource::new(length as u64);
            source.insert(offset as u64, tail.into());
            self.open_source(source)
        }

        /// Bytes of the file downloading in pieces, from `offset`. Returns
        /// the file's JSON when they were the last it needed to open.
        #[wasm_bindgen(js_name = addBytes)]
        pub fn add_bytes(
            &mut self,
            offset: f64,
            bytes: Vec<u8>,
        ) -> Result<Option<String>, JsValue> {
            if let Some(file) = &self.file {
                file.bundle.source().insert(offset as u64, bytes.into());
                return Ok(None);
            }
            let Some(source) = self.pending.clone() else {
                return Ok(None);
            };
            source.insert(offset as u64, bytes.into());
            self.open_source(source)
        }

        /// Draw the frame shown at `time` at the canvas's size. Returns
        /// `false` while its bytes are still downloading.
        pub fn draw(&mut self, time: f64) -> Result<bool, JsValue> {
            let Some(file) = self.file.as_mut() else {
                return Ok(true);
            };
            let (width, height) = self.gpu.fit_canvas();
            let index = file.bundle.frame_index_at(time);
            if file.shown == Some((index, width, height)) {
                return Ok(true);
            }
            let frame = match file.bundle.frame(index) {
                Ok(frame) => frame,
                Err(BundleError::Incomplete { missing }) => {
                    self.wanted.extend(missing);
                    return Ok(false);
                }
                Err(error) => return Err(failure("error", &describe(&error))),
            };
            if !file.bundle.source().is_complete() {
                self.wanted
                    .extend(file.bundle.missing_around(index, READ_AHEAD_CHUNKS));
            }
            let (scene, three_d) = file.compose(&frame, width, height);
            if self
                .gpu
                .render(&scene, &three_d, file.base, width, height)?
            {
                file.shown = Some((index, width, height));
            }
            Ok(true)
        }

        /// Draw the frame shown at `time` even when it is the one on the
        /// canvas already: a video export captures every frame it asks for.
        #[wasm_bindgen(js_name = drawFrame)]
        pub fn draw_frame(&mut self, time: f64) -> Result<bool, JsValue> {
            if let Some(file) = self.file.as_mut() {
                file.shown = None;
            }
            self.draw(time)
        }

        /// The byte ranges to download, as start and end pairs, and forget
        /// them.
        #[wasm_bindgen(js_name = takeWanted)]
        pub fn take_wanted(&mut self) -> Vec<f64> {
            self.wanted
                .drain(..)
                .flat_map(|range| [range.start as f64, range.end as f64])
                .collect()
        }

        /// The bytes of embedded file `entry` (an audio track's), or
        /// `undefined` while they download.
        pub fn media(&mut self, entry: &str) -> Result<Option<Vec<u8>>, JsValue> {
            let Some(file) = self.file.as_mut() else {
                return Ok(None);
            };
            match file.bundle.media(entry) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(BundleError::Incomplete { missing }) => {
                    self.wanted.extend(missing);
                    Ok(None)
                }
                Err(error) => Err(failure("error", &describe(&error))),
            }
        }
    }

    impl Player {
        fn open_source(&mut self, source: BundleSource) -> Result<Option<String>, JsValue> {
            match Bundle::from_source(source.clone()) {
                Ok(bundle) => {
                    self.pending = None;
                    if bundle.frame_count() == 0 {
                        return Err(failure("damaged", "El archivo no tiene fotogramas."));
                    }
                    let json = describe_file(&bundle);
                    self.file = Some(File::new(bundle));
                    Ok(Some(json))
                }
                Err(BundleError::Incomplete { missing }) => {
                    self.pending = Some(source);
                    self.wanted.extend(missing);
                    Ok(None)
                }
                Err(error) => {
                    self.pending = None;
                    let kind = if matches!(error, BundleError::Corrupt(_)) {
                        "damaged"
                    } else {
                        "error"
                    };
                    Err(failure(kind, &describe(&error)))
                }
            }
        }
    }

    impl File {
        fn new(bundle: Bundle) -> Self {
            // Shader backgrounds draw on the GPU in a pass of their own,
            // which this player leaves out: they show their fallback color.
            let background = bundle.scene.background.clone().map(|mut background| {
                background.paint = without_shader(background.paint);
                for segment in &mut background.segment_paints {
                    segment.paint = segment.paint.take().map(without_shader);
                }
                background
            });
            let base = bundle
                .scene
                .clear_color
                .map(|[r, g, b, a]| Color::from_rgba8(r, g, b, a))
                .unwrap_or(Color::BLACK);
            Self {
                bundle,
                store: FragmentStore::default(),
                background,
                base,
                shown: None,
            }
        }

        /// The scene `frame` draws on a canvas `width` by `height` pixels,
        /// as an export at that size composes it, and the frames of its 3D
        /// content, which the GPU draws into images the scene shows.
        fn compose(
            &mut self,
            frame: &Frame,
            width: u32,
            height: u32,
        ) -> (vello::Scene, Vec<GpuMeshFrame>) {
            if let Some(background) = &mut self.background {
                background.pixel_size = (width, height);
            }
            let background = self.background.as_ref();
            let pixels_per_unit = background.and_then(|background| {
                output_pixels_per_unit(&frame.camera, background.pixel_size.0)
            });
            // Fitted inside the canvas, as an export with `fit="contain"`.
            let resolved = gaanim_math::ResolvedCamera::new(
                frame.camera,
                gaanim_math::CameraViewport::default(),
            );
            let placement = camera_to_output(Some(&resolved), width, height, false);
            let (scene, three_d) = compose_captured_at(
                &frame.capture,
                &mut self.store,
                background.map(|background| (background, (width, height))),
                pixels_per_unit,
                None,
                0.0,
                Some(CapturedGpuThreeD {
                    camera: &frame.camera,
                    size: (width, height),
                    placement,
                    slot: 0,
                }),
            );
            self.store.end_frame();
            let mut placed = vello::Scene::new();
            placed.append(&scene, Some(placement));
            (placed, three_d)
        }
    }

    /// What the page needs to know about a file, as JSON: its title,
    /// duration, frame rate and size, the color under it, the times of its
    /// stops and the ranges of their ambient loops, its audio tracks (as the
    /// web player hands them to its page), and `full`, which GPU passes it
    /// uses that this player leaves out.
    fn describe_file(bundle: &Bundle) -> String {
        let scene = &bundle.scene;
        let stops: Vec<f64> = scene
            .segments
            .iter()
            .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
            .collect();
        // `scene.stop(loop=…)`: the range after the stop that repeats while
        // the player rests there, as `Timeline::ambient_loop_at` has it.
        let loops: Vec<[f64; 2]> = scene
            .segments
            .iter()
            .flat_map(|segment| segment.stops.iter())
            .filter_map(|stop| {
                let length = stop.ambient.filter(|length| *length > 1e-5)?;
                Some([stop.time, stop.time + length])
            })
            .collect();
        let audio: Vec<serde_json::Value> = scene
            .audio
            .iter()
            .map(|track| {
                serde_json::json!({
                    "media": track.media,
                    "start": track.start_time,
                    "duration": track.duration,
                    "volume": track.volume,
                    "fadeIn": track.fade_in,
                    "fadeOut": track.fade_out,
                    "offset": track.source_offset,
                    "sourceDuration": track.source_duration,
                    "speed": track.speed,
                    "looping": track.looping,
                })
            })
            .collect();
        let shader_background = scene.background.as_ref().is_some_and(|background| {
            matches!(background.paint, BackgroundPaint::Shader(_))
                || background
                    .segment_paints
                    .iter()
                    .any(|segment| matches!(segment.paint, Some(BackgroundPaint::Shader(_))))
        });
        let mut full = Vec::new();
        if !scene.post_shaders.is_empty() {
            full.push("post");
        }
        if shader_background {
            full.push("background");
        }
        if bundle.has_shader_transitions() {
            full.push("transition");
        }
        if bundle.has_shader_effects() {
            full.push("effect");
        }
        if bundle.has_glass() {
            full.push("glass");
        }
        serde_json::json!({
            "title": scene.title,
            "duration": scene.duration,
            "fps": scene.fps,
            "width": scene.output_size.0,
            "height": scene.output_size.1,
            "background": scene.clear_color,
            "stops": stops,
            "loops": loops,
            "audio": audio,
            "full": full,
        })
        .to_string()
    }

    fn without_shader(paint: BackgroundPaint) -> BackgroundPaint {
        match paint {
            BackgroundPaint::Shader(shader) => BackgroundPaint::solid(shader.fallback()),
            paint => paint,
        }
    }

    /// What the viewer reads when a file cannot open or play, as the web
    /// player words it.
    fn describe(error: &BundleError) -> String {
        match error {
            BundleError::UnsupportedVersion { found, .. } if *found > gaanim_bundle::VERSION => {
                "Este archivo usa una versión del formato .gaanim más nueva que este reproductor. \
                 Recarga la página para usar la última versión."
                    .into()
            }
            BundleError::UnsupportedVersion { generator, .. } => format!(
                "Este archivo lo grabó {generator} en un formato que ya no se lee. Vuelve a \
                 grabarlo con una versión reciente de Gaanim."
            ),
            BundleError::Corrupt(detail) => {
                format!("No es un archivo .gaanim o está dañado ({detail}).")
            }
            error => format!("No se pudo abrir el archivo: {error}."),
        }
    }

    /// A JavaScript `Error` with `message` whose `name` is `kind`:
    /// `"damaged"` when the bytes are not a bundle (or not all of one), or
    /// `"error"`.
    fn failure(kind: &str, message: &str) -> JsValue {
        let error = js_sys::Error::new(message);
        error.set_name(kind);
        error.into()
    }

    /// The canvas, the device that draws on it, and Vello.
    struct Gpu {
        canvas: HtmlCanvasElement,
        surface: wgpu::Surface<'static>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        renderer: vello::Renderer,
        /// Copies Vello's output, which is a storage texture, to the canvas.
        blitter: wgpu::util::TextureBlitter,
        /// Draws the 3D content into the images the scene shows.
        three_d: GpuThreeD,
        /// Vello's output and its size.
        target: Option<(wgpu::TextureView, u32, u32)>,
    }

    impl Gpu {
        async fn new(canvas: HtmlCanvasElement) -> Result<Self, JsError> {
            let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;
            let instance = wgpu::Instance::new(descriptor);
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
                .map_err(|error| JsError::new(&error.to_string()))?;
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                })
                .await
                .map_err(|error| JsError::new(&error.to_string()))?;
            // Vello's gradient ramps are a texture row each and complex
            // scenes need large buffers: ask for what the adapter allows, as
            // the exporter does.
            let supported = adapter.limits();
            let defaults = wgpu::Limits::default();
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("gaanim-embed"),
                    required_limits: wgpu::Limits {
                        max_buffer_size: supported.max_buffer_size.max(defaults.max_buffer_size),
                        max_storage_buffer_binding_size: supported
                            .max_storage_buffer_binding_size
                            .max(defaults.max_storage_buffer_binding_size),
                        max_texture_dimension_2d: supported
                            .max_texture_dimension_2d
                            .max(defaults.max_texture_dimension_2d),
                        ..defaults
                    },
                    ..Default::default()
                })
                .await
                .map_err(|error| JsError::new(&error.to_string()))?;
            let (width, height) = (canvas.width().max(1), canvas.height().max(1));
            let mut config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| JsError::new("WebGPU cannot draw on this canvas"))?;
            // Vello writes sRGB-encoded values: a non-sRGB canvas shows them
            // as they are.
            let capabilities = surface.get_capabilities(&adapter);
            if let Some(format) = capabilities
                .formats
                .iter()
                .copied()
                .find(|format| !format.is_srgb())
            {
                config.format = format;
            }
            surface.configure(&device, &config);
            let renderer = vello::Renderer::new(
                &device,
                vello::RendererOptions {
                    use_cpu: false,
                    antialiasing_support: vello::AaSupport::area_only(),
                    num_init_threads: None,
                    pipeline_cache: None,
                },
            )
            .map_err(|error| JsError::new(&error.to_string()))?;
            let blitter = wgpu::util::TextureBlitter::new(&device, config.format);
            Ok(Self {
                canvas,
                surface,
                device,
                queue,
                config,
                renderer,
                blitter,
                three_d: GpuThreeD::default(),
                target: None,
            })
        }

        /// The canvas size in pixels, which the page sets; the surface
        /// follows it.
        fn fit_canvas(&mut self) -> (u32, u32) {
            let size = (self.canvas.width().max(1), self.canvas.height().max(1));
            if size != (self.config.width, self.config.height) {
                (self.config.width, self.config.height) = size;
                self.surface.configure(&self.device, &self.config);
            }
            size
        }

        /// Vello's output texture at `width` by `height`.
        fn target(&mut self, width: u32, height: u32) -> wgpu::TextureView {
            if let Some((view, w, h)) = &self.target
                && (*w, *h) == (width, height)
            {
                return view.clone();
            }
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("gaanim-embed-frame"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some((view.clone(), width, height));
            view
        }

        /// Draw `scene`, with the 3D content of `three_d`, over `base` and
        /// show it; `false` when the canvas could not take a frame now.
        fn render(
            &mut self,
            scene: &vello::Scene,
            three_d: &[GpuMeshFrame],
            base: Color,
            width: u32,
            height: u32,
        ) -> Result<bool, JsValue> {
            let target = self.target(width, height);
            self.three_d
                .prepare(&self.device, &self.queue, &mut self.renderer, three_d);
            self.renderer
                .render_to_texture(
                    &self.device,
                    &self.queue,
                    scene,
                    &target,
                    &vello::RenderParams {
                        base_color: base,
                        width,
                        height,
                        antialiasing_method: vello::AaConfig::Area,
                    },
                )
                .map_err(|error| failure("error", &error.to_string()))?;
            let output = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(output)
                | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    self.surface.configure(&self.device, &self.config);
                    return Ok(false);
                }
                _ => return Ok(false),
            };
            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gaanim-embed-present"),
                });
            self.blitter
                .copy(&self.device, &mut encoder, &target, &view);
            self.queue.submit([encoder.finish()]);
            output.present();
            Ok(true)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::Player;
