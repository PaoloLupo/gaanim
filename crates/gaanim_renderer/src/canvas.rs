//! Draws the composed Vello scene into the interactive window.
//!
//! Each frame the render world rasterizes the [`MainVelloScene`] with Vello
//! into a storage texture sized to the [`VelloView`] camera's viewport, then a
//! full-screen pass composites that texture into the camera's main 2D pass.
//! Headless export bypasses this plugin and renders scenes directly.
//!
//! [`MainVelloScene`]: crate::pipeline::MainVelloScene

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use bevy::asset::RenderAssetUsages;
use bevy::camera::CameraUpdateSystems;
use bevy::core_pipeline::core_2d::main_transparent_pass_2d;
use bevy::core_pipeline::{Core2d, Core2dSystems};
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, BindingResource, BindingType,
    BlendState, Buffer, BufferBindingType, BufferDescriptor, BufferUsages, ColorTargetState,
    ColorWrites, Extent3d, LoadOp, MultisampleState, Operations, PrimitiveState, RawFragmentState,
    RawRenderPipelineDescriptor, RawVertexState, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, ShaderStages, StoreOp, TextureDimension, TextureFormat, TextureSampleType,
    TextureUsages, TextureViewDimension, TextureViewId,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery, render_system};
use bevy::render::texture::GpuImage;
use bevy::render::view::{ExtractedView, Msaa, ViewTarget};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::window::PrimaryWindow;
use vello::kurbo::Affine;
use vello::{AaConfig, AaSupport, RenderParams, Scene};

use crate::pipeline::MainVelloScene;

/// Marks the 2D camera whose viewport shows the Vello canvas.
#[derive(Component, Debug, Clone, Copy, ExtractComponent)]
#[require(Camera2d)]
pub struct VelloView;

/// A composed Vello scene in gaanim's Y-up world space.
///
/// The scene is shared so the render world can take it without copying the
/// encoding every frame.
#[derive(Component, Default, Clone, Deref)]
pub struct VelloScene2d(Arc<Scene>);

impl From<Scene> for VelloScene2d {
    fn from(scene: Scene) -> Self {
        Self(Arc::new(scene))
    }
}

// Compares scene encodings on the CPU; it lives with the fragments so the
// playground's Pyodide extension, which leaves this module out, has it too.
pub use crate::fragment::draws_same;

impl VelloScene2d {
    /// Show `scene` unless it draws the same frame as the current one. Keeping
    /// the shared scene lets the canvas skip rasterizing an unchanged frame.
    pub fn replace_if_different(&mut self, scene: Scene) -> bool {
        if draws_same(&self.0, &scene) {
            return false;
        }
        self.0 = Arc::new(scene);
        true
    }
}

/// The Vello renderer on the render device, shared with GPU shader backgrounds.
#[derive(Resource, Deref, DerefMut)]
pub struct VelloRenderer(Arc<Mutex<vello::Renderer>>);

impl VelloRenderer {
    fn try_new(device: &vello::wgpu::Device, use_cpu: bool) -> Result<Self, vello::Error> {
        vello::Renderer::new(
            device,
            vello::RendererOptions {
                use_cpu,
                // Vello cannot add antialiasing modes after initialization,
                // and building each one delays the first frame, so only the
                // canvas mode is built. Change both together.
                antialiasing_support: AaSupport::area_only(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map(|renderer| Self(Arc::new(Mutex::new(renderer))))
    }
}

/// Antialiasing of the interactive canvas, the only mode its renderer
/// supports; exports choose their own.
const CANVAS_ANTIALIASING: AaConfig = AaConfig::Area;

/// Environment variable that sets the preview resolution: `auto` (the
/// default), `full`, or a fixed fraction of the viewport such as `0.5`.
pub const PREVIEW_RESOLUTION_ENV: &str = "GAANIM_PREVIEW_RESOLUTION";

/// Smallest fraction of the viewport a fixed preview resolution may use.
pub const MIN_PREVIEW_SCALE: f32 = 0.25;

/// Scales playback steps through, from full resolution down, while frames
/// run slow.
const ADAPTIVE_PREVIEW_SCALES: [f32; 3] = [1.0, 0.75, 0.5];

/// Average frame time above which playback lowers the resolution: 20 % over
/// a 60 Hz frame. Faster displays that draw fewer frames still look fluid.
const SLOW_FRAME_SECONDS: f64 = 1.2 / 60.0;

/// Frame times are averaged over windows of this many seconds.
const ADAPT_WINDOW_SECONDS: f64 = 0.5;

/// Resolution of the interactive canvas relative to the physical pixels of
/// its viewport.
///
/// While `adaptive` is set, playback lowers `scale` when frames miss 60 fps
/// because of rasterization, and a pause restores full resolution, so a
/// paused or resting frame is always sharp. Without this resource the
/// canvas draws every pixel: windowed captures and exports of 3D scenes
/// also draw through the canvas, so only the interactive editor inserts it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct PreviewResolution {
    /// Whether playback may change `scale`.
    pub adaptive: bool,
    /// Fraction of the viewport's physical pixels that Vello rasterizes,
    /// between [`MIN_PREVIEW_SCALE`] and 1.
    pub scale: f32,
}

impl Default for PreviewResolution {
    fn default() -> Self {
        Self::FULL
    }
}

impl PreviewResolution {
    /// Every pixel, always.
    pub const FULL: Self = Self {
        adaptive: false,
        scale: 1.0,
    };

    /// Full resolution that playback lowers while it runs slow.
    pub const AUTO: Self = Self {
        adaptive: true,
        scale: 1.0,
    };

    /// Interactive resolution for a [`PREVIEW_RESOLUTION_ENV`] value; `None`
    /// or anything unrecognized is [`Self::AUTO`].
    pub fn from_setting(setting: Option<&str>) -> Self {
        let setting = setting.map(str::trim).unwrap_or("auto");
        if setting.eq_ignore_ascii_case("full") {
            return Self::FULL;
        }
        match setting.parse::<f32>() {
            Ok(fixed) if fixed.is_finite() && fixed > 0.0 => Self {
                adaptive: false,
                scale: fixed.clamp(MIN_PREVIEW_SCALE, 1.0),
            },
            _ => Self::AUTO,
        }
    }
}

/// Pixels of a `full`-sized canvas that a preview at `scale` rasterizes.
pub fn scaled_canvas_size(full: UVec2, scale: f32) -> UVec2 {
    let scale = if scale.is_finite() {
        scale.clamp(MIN_PREVIEW_SCALE, 1.0)
    } else {
        1.0
    };
    let full = full.max(UVec2::ONE);
    (full.as_vec2() * scale)
        .round()
        .as_uvec2()
        .clamp(UVec2::ONE, full)
}

/// Chooses the preview scale during playback from measured frame times.
#[derive(Debug, Default)]
struct ResolutionAdapter {
    playing: bool,
    elapsed: f64,
    frames: u32,
    /// Windows to ignore: the first of a playback and the one after a change.
    settle: u32,
    slow_windows: u32,
    /// Average frame time before the latest reduction.
    before_step: Option<f64>,
    /// Set once a reduction did not speed frames up: rasterizing is not what
    /// is slow, so a lower resolution would only blur the preview.
    locked: bool,
}

impl ResolutionAdapter {
    /// Account one frame of `dt` seconds at `scale` and return the scale for
    /// the next frame.
    fn update(&mut self, playing: bool, dt: f64, scale: f32) -> f32 {
        if !playing {
            *self = Self::default();
            return 1.0;
        }
        if !self.playing {
            *self = Self {
                playing: true,
                settle: 1,
                ..Self::default()
            };
        }
        if !(dt.is_finite() && dt > 0.0) {
            return scale;
        }
        self.elapsed += dt;
        self.frames += 1;
        if self.elapsed < ADAPT_WINDOW_SECONDS {
            return scale;
        }
        let average = self.elapsed / f64::from(self.frames);
        self.elapsed = 0.0;
        self.frames = 0;
        if self.settle > 0 {
            self.settle -= 1;
            return scale;
        }
        if self.locked {
            return scale;
        }
        if let Some(before) = self.before_step.take()
            && average > before * 0.9
        {
            self.locked = true;
            return ADAPTIVE_PREVIEW_SCALES
                .iter()
                .rev()
                .copied()
                .find(|larger| *larger > scale + 1e-3)
                .unwrap_or(1.0);
        }
        if average <= SLOW_FRAME_SECONDS {
            self.slow_windows = 0;
            return scale;
        }
        // One slow window can be a hitch; two in a row are the scene.
        self.slow_windows += 1;
        if self.slow_windows < 2 {
            return scale;
        }
        self.slow_windows = 0;
        let Some(smaller) = ADAPTIVE_PREVIEW_SCALES
            .iter()
            .copied()
            .find(|smaller| *smaller < scale - 1e-3)
        else {
            return scale;
        };
        self.before_step = Some(average);
        self.settle = 1;
        smaller
    }
}

/// Lowers the preview resolution while playback runs slow and restores it
/// when playback stops. Does nothing unless a host inserted an adaptive
/// [`PreviewResolution`].
fn adapt_preview_resolution(
    time: Res<Time<Real>>,
    playback: Option<Res<gaanim_animation::PlaybackState>>,
    preview: Option<ResMut<PreviewResolution>>,
    mut adapter: Local<ResolutionAdapter>,
) {
    let Some(mut preview) = preview.filter(|preview| preview.adaptive) else {
        *adapter = ResolutionAdapter::default();
        return;
    };
    let playing = playback.is_some_and(|state| state.is_playing);
    let scale = adapter.update(playing, time.delta_secs_f64(), preview.scale);
    if scale != preview.scale {
        preview.scale = scale;
    }
}

/// Texture the canvas is rasterized into, sized to the camera viewport.
#[derive(Resource, Clone, Default)]
pub struct VelloCanvas {
    pub image: Handle<Image>,
    size: UVec2,
}

impl VelloCanvas {
    /// Physical pixels of the canvas texture: those of the camera viewport.
    /// A reduced [`PreviewResolution`] rasterizes only part of them.
    pub fn size(&self) -> UVec2 {
        self.size
    }
}

/// A small image the [`VelloView`] camera's canvas is also drawn into,
/// scaled to fill it over `clear`: what that camera shows, for Presenter
/// View to keep in sight without rasterizing the scene again. Make the
/// image with [`CanvasMirror::target`]; the default handle draws nothing.
#[derive(Resource, Clone, Default)]
pub struct CanvasMirror {
    pub image: Handle<Image>,
    /// The scene's background, beneath the canvas's transparent pixels.
    pub clear: Color,
}

/// Format of a [`CanvasMirror`] image: sRGB, so its texels sample as linear
/// color as egui expects.
const MIRROR_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

impl CanvasMirror {
    /// An image of `size` pixels a mirror, or a camera, can draw into.
    pub fn target(size: UVec2) -> Image {
        let mut image = Image::new_uninit(
            Extent3d {
                width: size.x.max(1),
                height: size.y.max(1),
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            MIRROR_FORMAT,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage =
            TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
        image
    }
}

/// Scene complexity of the latest rendered frame, written by the render world.
#[derive(Resource, Clone, Default)]
pub struct VelloFrameStats(Arc<FrameStats>);

#[derive(Default)]
struct FrameStats {
    rendered: AtomicBool,
    scenes: AtomicU32,
    paths: AtomicU32,
    path_segments: AtomicU32,
    clips: AtomicU32,
    open_clips: AtomicU32,
}

impl VelloFrameStats {
    /// Counts of the latest rendered frame, or `None` before the first one.
    /// `[scenes, paths, path segments, clips, open clips]`.
    pub fn latest(&self) -> Option<[u32; 5]> {
        let stats = &self.0;
        stats.rendered.load(Ordering::Acquire).then(|| {
            [
                stats.scenes.load(Ordering::Relaxed),
                stats.paths.load(Ordering::Relaxed),
                stats.path_segments.load(Ordering::Relaxed),
                stats.clips.load(Ordering::Relaxed),
                stats.open_clips.load(Ordering::Relaxed),
            ]
        })
    }
}

/// The main-world state the render world draws this frame.
#[derive(Resource, Default)]
struct ExtractedCanvas {
    image: Option<AssetId<Image>>,
    scene: Option<(Arc<Scene>, Mat4)>,
    /// A shader transition's incoming segment and layer above, drawn with
    /// the scene's transform into their own targets.
    transition: Option<(Arc<Scene>, Arc<Scene>)>,
    /// Drawables drawn through their shader effect, into the images the
    /// scene draws; see [`crate::object_effects`].
    effects: Vec<crate::object_effects::EffectLayer>,
    /// Preview resolution scale; see [`PreviewResolution`].
    scale: f32,
}

/// The render world's textures for drawables with a shader effect.
#[derive(Resource, Default)]
struct CanvasObjectEffects(crate::object_effects::ObjectEffects);

/// The [`CanvasMirror`] the render world draws into this frame.
#[derive(Resource, Default)]
struct ExtractedMirror {
    image: Option<AssetId<Image>>,
    clear: LinearRgba,
}

/// The frame the canvas texture holds, so an identical frame is not
/// rasterized again (while paused, holding a pose, or resting on a stop).
#[derive(Resource, Default)]
struct RenderedCanvas(Option<CanvasFrame>);

struct CanvasFrame {
    /// The texture view drawn into; a recreated texture starts out blank.
    view: TextureViewId,
    /// Rasterized region at the top-left of the texture.
    size: UVec2,
    /// Holding the scene keeps its address from being reused by another one.
    scene: Option<(Arc<Scene>, Affine)>,
}

impl CanvasFrame {
    fn same(&self, other: &Self) -> bool {
        self.view == other.view
            && self.size == other.size
            && match (&self.scene, &other.scene) {
                (None, None) => true,
                (Some((scene, affine)), Some((other_scene, other_affine))) => {
                    Arc::ptr_eq(scene, other_scene) && affine == other_affine
                }
                _ => false,
            }
    }
}

/// Frames Vello skipped in the preview. Vello sizes its GPU buffers from an
/// estimate of the scene, and when a scene outgrows them it writes no pixel
/// and reports nothing: the canvas keeps its last frame, which the
/// post-process then processes again, so a bloom grows into a white blob.
/// Like the exporter, the canvas writes sentinel pixels before a render and
/// reads them back after it, here a frame later so the preview never waits;
/// a frame whose probes kept the sentinel doubles Vello's buffers and is
/// drawn again.
#[derive(Resource)]
struct CanvasOverflow {
    /// Multiplier of Vello's bump buffers, from 1 to [`MAX_BUMP_SCALE`].
    scale: u32,
    /// Where the probe pixels are copied, kept between frames.
    buffer: Option<vello::wgpu::Buffer>,
    /// The readback in flight: [`PROBE_PENDING`], [`PROBE_MAPPED`] or
    /// [`PROBE_FAILED`].
    pending: Option<Arc<AtomicU8>>,
}

/// Written to the probe pixels before a render; Vello overwrites every pixel
/// of the region it draws unless a stage ran out of buffer.
const PROBE_SENTINEL: [u8; 4] = [0x5a, 0xc3, 0x17, 0x01];
/// Byte stride of the probes in the readback buffer (copy row alignment).
const PROBE_STRIDE: u64 = 256;
const PROBE_COUNT: usize = 5;
const PROBE_PENDING: u8 = 0;
const PROBE_MAPPED: u8 = 1;
const PROBE_FAILED: u8 = 2;
/// Largest multiplier of Vello's buffers, as in the exporter.
const MAX_BUMP_SCALE: u32 = 8;

impl Default for CanvasOverflow {
    fn default() -> Self {
        Self {
            scale: 1,
            buffer: None,
            pending: None,
        }
    }
}

impl CanvasOverflow {
    /// Whether Vello skipped the last probed frame, once its readback is in.
    fn skipped(&mut self, device: &vello::wgpu::Device) -> Option<bool> {
        let state = self.pending.as_ref()?;
        let _ = device.poll(vello::wgpu::PollType::Poll);
        let skipped = match state.load(Ordering::Acquire) {
            PROBE_PENDING => return None,
            PROBE_MAPPED => self.buffer.as_ref().map(|buffer| {
                let skipped = buffer
                    .slice(..)
                    .get_mapped_range()
                    .chunks(PROBE_STRIDE as usize)
                    .all(|probe| probe[..4] == PROBE_SENTINEL);
                buffer.unmap();
                skipped
            }),
            _ => None,
        };
        self.pending = None;
        skipped
    }

    /// The pixels probed in a `size` region, or `None` while the last
    /// readback is in flight.
    fn probe_points(&self, size: UVec2) -> Option<[(u32, u32); PROBE_COUNT]> {
        if self.pending.is_some() || size.x == 0 || size.y == 0 {
            return None;
        }
        let (right, bottom) = (size.x - 1, size.y - 1);
        Some([
            (0, 0),
            (right, 0),
            (0, bottom),
            (right, bottom),
            (size.x / 2, size.y / 2),
        ])
    }

    /// Write the sentinel into `points` of `texture`.
    fn arm(queue: &vello::wgpu::Queue, texture: &vello::wgpu::Texture, points: &[(u32, u32)]) {
        for &(x, y) in points {
            queue.write_texture(
                vello::wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: vello::wgpu::Origin3d { x, y, z: 0 },
                    aspect: vello::wgpu::TextureAspect::All,
                },
                &PROBE_SENTINEL,
                vello::wgpu::TexelCopyBufferLayout::default(),
                Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Copy `points` of the rendered `texture` and start reading them back.
    fn read_back(
        &mut self,
        device: &vello::wgpu::Device,
        queue: &vello::wgpu::Queue,
        texture: &vello::wgpu::Texture,
        points: &[(u32, u32)],
    ) {
        let buffer = self.buffer.get_or_insert_with(|| {
            device.create_buffer(&vello::wgpu::BufferDescriptor {
                label: Some("gaanim-canvas-probes"),
                size: PROBE_STRIDE * PROBE_COUNT as u64,
                usage: vello::wgpu::BufferUsages::MAP_READ | vello::wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let mut encoder = device.create_command_encoder(&vello::wgpu::CommandEncoderDescriptor {
            label: Some("gaanim-canvas-probes"),
        });
        for (index, &(x, y)) in points.iter().enumerate() {
            encoder.copy_texture_to_buffer(
                vello::wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: vello::wgpu::Origin3d { x, y, z: 0 },
                    aspect: vello::wgpu::TextureAspect::All,
                },
                vello::wgpu::TexelCopyBufferInfo {
                    buffer,
                    layout: vello::wgpu::TexelCopyBufferLayout {
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
        queue.submit(Some(encoder.finish()));
        let state = Arc::new(AtomicU8::new(PROBE_PENDING));
        let mapped = Arc::clone(&state);
        buffer
            .slice(..)
            .map_async(vello::wgpu::MapMode::Read, move |result| {
                let outcome = if result.is_ok() {
                    PROBE_MAPPED
                } else {
                    PROBE_FAILED
                };
                mapped.store(outcome, Ordering::Release);
            });
        self.pending = Some(state);
    }
}

#[derive(Resource)]
struct CompositePipelines {
    layout: BindGroupLayout,
    shader: vello::wgpu::ShaderModule,
    pipelines: Vec<((TextureFormat, u32), RenderPipeline)>,
    /// Fraction of the canvas texture the rasterized region covers (`xy`).
    region: Buffer,
    region_value: [f32; 4],
    /// Bind group of the canvas texture view it was created for.
    bind_group: Option<(TextureViewId, BindGroup)>,
}

pub(crate) struct VelloCanvasPlugin;

impl Plugin for VelloCanvasPlugin {
    fn build(&self, app: &mut App) {
        let stats = VelloFrameStats::default();
        app.add_plugins(ExtractComponentPlugin::<VelloView>::default())
            .init_resource::<VelloCanvas>()
            .init_resource::<CanvasMirror>()
            .init_resource::<crate::pipeline::EffectLayers>()
            .insert_resource(stats.clone())
            .add_systems(PreUpdate, adapt_preview_resolution)
            .add_systems(PostUpdate, resize_canvas_target.after(CameraUpdateSystems));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .insert_resource(stats)
            .init_resource::<ExtractedCanvas>()
            .init_resource::<RenderedCanvas>()
            .init_resource::<CanvasOverflow>()
            .init_resource::<ExtractedMirror>()
            .init_resource::<CanvasObjectEffects>()
            .add_systems(ExtractSchedule, (extract_canvas, extract_mirror))
            .add_systems(
                Render,
                (
                    prepare_composite
                        .in_set(RenderSystems::PrepareBindGroups)
                        .run_if(resource_exists::<CompositePipelines>),
                    render_canvas
                        .in_set(RenderSystems::Render)
                        .before(render_system)
                        .run_if(resource_exists::<VelloRenderer>),
                ),
            )
            .add_systems(
                Core2d,
                (composite_canvas, mirror_canvas)
                    .chain()
                    .after(main_transparent_pass_2d)
                    .in_set(Core2dSystems::MainPass),
            );
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        let Some(device) = render_app.world().get_resource::<RenderDevice>().cloned() else {
            return;
        };
        let renderer = match VelloRenderer::try_new(device.wgpu_device(), false) {
            Ok(renderer) => renderer,
            Err(error) => {
                error!("Vello GPU renderer unavailable ({error}); falling back to CPU shaders");
                VelloRenderer::try_new(device.wgpu_device(), true)
                    .unwrap_or_else(|error| panic!("failed to start Vello: {error}"))
            }
        };
        render_app
            .insert_resource(renderer)
            .insert_resource(CompositePipelines::new(&device));
    }
}

/// Size of the `VelloView` camera's viewport, falling back to the primary window.
fn canvas_size(
    cameras: &Query<&Camera, With<VelloView>>,
    window: Option<&Window>,
) -> Option<UVec2> {
    if let Ok(camera) = cameras.single()
        && let Some(size) = camera.physical_viewport_size()
    {
        return Some(size);
    }
    window.map(|window| UVec2::new(window.physical_width(), window.physical_height()))
}

/// Keeps the canvas texture the same size as the camera viewport.
fn resize_canvas_target(
    cameras: Query<&Camera, With<VelloView>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut canvas: ResMut<VelloCanvas>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(size) = canvas_size(&cameras, window.as_deref().copied()) else {
        return;
    };
    if size.x == 0 || size.y == 0 || (size == canvas.size && canvas.image != Handle::default()) {
        return;
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    // COPY_SRC lets `CanvasOverflow` read back its probe pixels.
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST
        | TextureUsages::COPY_SRC
        | TextureUsages::STORAGE_BINDING;
    canvas.image = images.add(image);
    canvas.size = size;
}

fn extract_canvas(
    canvas: Extract<Res<VelloCanvas>>,
    preview: Extract<Option<Res<PreviewResolution>>>,
    scenes: Extract<Query<(&VelloScene2d, &Transform), With<MainVelloScene>>>,
    layers: Extract<Option<Res<crate::pipeline::TransitionLayers>>>,
    effects: Extract<Option<Res<crate::pipeline::EffectLayers>>>,
    mut extracted: ResMut<ExtractedCanvas>,
) {
    extracted.effects = effects
        .as_ref()
        .map(|effects| effects.0.clone())
        .unwrap_or_default();
    extracted.transition = layers
        .as_ref()
        .and_then(|layers| layers.0.as_ref())
        .map(|layers| (Arc::clone(&layers.incoming), Arc::clone(&layers.above)));
    extracted.image = (canvas.image != Handle::default()).then(|| canvas.image.id());
    extracted.scale = preview.as_ref().map_or(1.0, |preview| preview.scale);
    extracted.scene = scenes
        .iter()
        .next()
        .map(|(scene, transform)| (scene.0.clone(), transform.to_matrix()));
}

fn extract_mirror(mirror: Extract<Res<CanvasMirror>>, mut extracted: ResMut<ExtractedMirror>) {
    extracted.image = (mirror.image != Handle::default()).then(|| mirror.image.id());
    extracted.clear = mirror.clear.to_linear();
}

/// Maps the scene from world space to the canvas pixels of `view`, with the
/// viewport rasterized at `scale` of its pixels per axis.
fn scene_to_pixels(
    model: Mat4,
    camera: &ExtractedCamera,
    view: &ExtractedView,
    scale: Vec2,
) -> Option<Affine> {
    let size = camera.physical_viewport_size?;
    Some(scene_to_canvas(
        model,
        view.world_from_view.to_matrix(),
        view.clip_from_view,
        Vec2::new(size.x as f32 * scale.x, size.y as f32 * scale.y),
    ))
}

/// Maps a scene placed by `model` to a canvas `pixels` large, seen by a
/// camera at `world_from_view` through `clip_from_view`.
///
/// As in bevy_vello, a scene's units are Y-down and the model places them in
/// the Y-up world (the main scene's model flips Y back, since it is composed
/// Y-up); clip space is Y-up and the canvas Y-down. Built as one matrix, a
/// rotated camera turns the view about its own position, as exports do.
pub(crate) fn scene_to_canvas(
    model: Mat4,
    world_from_view: Mat4,
    clip_from_view: Mat4,
    pixels: Vec2,
) -> Affine {
    let ndc_to_pixels = Mat4::from_cols(
        Vec4::new(pixels.x / 2.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, -pixels.y / 2.0, 0.0, 0.0),
        Vec4::Z,
        Vec4::new(pixels.x / 2.0, pixels.y / 2.0, 0.0, 1.0),
    );
    let y_down = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
    let m = (ndc_to_pixels * clip_from_view * world_from_view.inverse() * model * y_down)
        .to_cols_array();
    Affine::new([
        m[0] as f64,
        m[1] as f64,
        m[4] as f64,
        m[5] as f64,
        m[12] as f64,
        m[13] as f64,
    ])
}

/// Render-world cameras that show the canvas.
type CanvasViews<'w, 's> = Query<
    'w,
    's,
    (&'static ExtractedCamera, &'static ExtractedView),
    (With<Camera2d>, With<VelloView>),
>;

/// What else draws into the canvas this frame: post-processing, a shader
/// background, a shader transition's targets, and the drawables the
/// post-process leaves out with their target.
type CanvasEffects<'w> = (
    Option<Res<'w, crate::post_process_gpu::ExtractedPostProcess>>,
    Option<Res<'w, crate::background_gpu::ExtractedShaderBackground>>,
    Option<Res<'w, crate::post_process_gpu::CanvasTransitionTargets>>,
    Option<Res<'w, crate::post_process_gpu::ExtractedUnprocessed>>,
    Option<Res<'w, crate::post_process_gpu::CanvasOverlayTarget>>,
);

/// Rasterizes the extracted scene into the canvas texture before the camera
/// passes sample it. A frame identical to the one the texture holds is skipped.
#[allow(clippy::too_many_arguments)]
fn render_canvas(
    extracted: Res<ExtractedCanvas>,
    views: CanvasViews,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    renderer: Res<VelloRenderer>,
    stats: Res<VelloFrameStats>,
    effects: CanvasEffects,
    mut object_effects: ResMut<CanvasObjectEffects>,
    mut rendered: ResMut<RenderedCanvas>,
    mut overflow: ResMut<CanvasOverflow>,
    three_d: Option<Res<crate::three_d_gpu::RenderGpuThreeD>>,
) {
    let Some(target) = extracted.image.and_then(|image| images.get(image)) else {
        return;
    };
    if overflow.skipped(device.wgpu_device()) == Some(true) {
        // Draw the skipped frame again, with larger buffers while allowed.
        rendered.0 = None;
        if overflow.scale < MAX_BUMP_SCALE {
            overflow.scale *= 2;
            info!(
                "the scene outgrew Vello's buffers; drawing it with {}x buffers",
                overflow.scale
            );
        }
    }

    let full = target.texture_descriptor.size;
    let full = UVec2::new(full.width, full.height);
    let pixels = scaled_canvas_size(full, extracted.scale);
    // Exactly one at full resolution, which keeps the historical mapping.
    let rasterized = pixels.as_vec2() / full.max(UVec2::ONE).as_vec2();
    let placed = extracted.scene.as_ref().and_then(|(scene, model)| {
        let (camera, view) = views.iter().last()?;
        Some((
            Arc::clone(scene),
            scene_to_pixels(*model, camera, view, rasterized)?,
        ))
    });
    let current = CanvasFrame {
        view: target.texture_view.id(),
        size: pixels,
        scene: placed,
    };
    // Post-processing rewrites the texture in place, and shader backgrounds
    // animate outside the scene encoding: frames with either are always drawn.
    // A soft effect's image keeps its pixels while the scene draws it.
    let (post, shader, transition_targets, unprocessed, overlay_target) = effects;
    // The drawables the post-process leaves out draw into their own target
    // when it runs, otherwise into the canvas with the rest.
    let unprocessed = unprocessed.and_then(|layer| layer.0.clone());
    let overlay_view = overlay_target
        .as_deref()
        .and_then(|target| target.view())
        .filter(|_| post.as_ref().is_some_and(|post| post.is_active()))
        .cloned();
    let volatile = post.is_some_and(|post| post.is_active())
        || shader.is_some_and(|shader| shader.is_active())
        || extracted.transition.is_some()
        || extracted.effects.iter().any(|layer| !layer.fixed)
        // The GPU's meshes drew a new image, which the scene draws unchanged.
        || three_d.is_some_and(|three_d| three_d.changed);
    if !volatile && rendered.0.as_ref().is_some_and(|last| last.same(&current)) {
        return;
    }
    rendered.0 = None;

    let mut frame = Scene::new();
    let mut scenes = 0;
    if let Some((scene, affine)) = &current.scene {
        frame.append(scene, Some(*affine));
        if overlay_view.is_none()
            && let Some(unprocessed) = &unprocessed
        {
            frame.append(unprocessed, Some(*affine));
        }
        scenes = 1;
    }

    let encoding = frame.encoding();
    let counts = &stats.0;
    counts.scenes.store(scenes, Ordering::Relaxed);
    counts.paths.store(encoding.n_paths, Ordering::Relaxed);
    counts
        .path_segments
        .store(encoding.n_path_segments, Ordering::Relaxed);
    counts.clips.store(encoding.n_clips, Ordering::Relaxed);
    counts
        .open_clips
        .store(encoding.n_open_clips, Ordering::Relaxed);
    counts.rendered.store(true, Ordering::Release);

    let Ok(mut renderer) = renderer.lock() else {
        return;
    };
    // Vello reads these on the thread that renders.
    vello_encoding::set_bump_buffer_scale(overflow.scale);
    vello_encoding::set_max_bump_buffer_bytes(
        device
            .wgpu_device()
            .limits()
            .max_storage_buffer_binding_size,
    );
    let probes = overflow.probe_points(pixels);
    if let Some(points) = &probes {
        CanvasOverflow::arm(&queue, &target.texture, points);
    }
    let placement = current.scene.as_ref().map(|(_, affine)| *affine);
    // Effects fill the images the frame draws, so they render first.
    if (!extracted.effects.is_empty() || object_effects.0.is_active())
        && let Err(error) = object_effects.0.render(
            device.wgpu_device(),
            &queue,
            &mut renderer,
            &extracted.effects,
            CANVAS_ANTIALIASING,
        )
    {
        error!("Vello failed to render a shader effect: {error}");
    }
    match renderer.render_to_texture(
        device.wgpu_device(),
        &queue,
        &frame,
        &target.texture_view,
        &RenderParams {
            base_color: vello::peniko::Color::TRANSPARENT,
            // A reduced preview fills the top-left of the texture.
            width: pixels.x,
            height: pixels.y,
            antialiasing_method: CANVAS_ANTIALIASING,
        },
    ) {
        Ok(()) => {
            rendered.0 = (!volatile).then_some(current);
            if let Some(points) = &probes {
                overflow.read_back(device.wgpu_device(), &queue, &target.texture, points);
            }
        }
        Err(error) => error!("Vello failed to render the canvas: {error}"),
    }

    // A shader transition draws its other layers with the same placement;
    // the post-process pass blends them into the canvas.
    let layers = extracted.transition.as_ref().zip(placement).zip(
        transition_targets
            .as_deref()
            .and_then(|targets| targets.views()),
    );
    if let Some((((incoming, above), affine), (incoming_view, above_view))) = layers {
        for (scene, view) in [(incoming, incoming_view), (above, above_view)] {
            let mut placed = Scene::new();
            placed.append(scene, Some(affine));
            if let Err(error) = renderer.render_to_texture(
                device.wgpu_device(),
                &queue,
                &placed,
                view,
                &RenderParams {
                    base_color: vello::peniko::Color::TRANSPARENT,
                    width: pixels.x,
                    height: pixels.y,
                    antialiasing_method: CANVAS_ANTIALIASING,
                },
            ) {
                error!("Vello failed to render a transition layer: {error}");
            }
        }
    }
    if let (Some(view), Some(unprocessed), Some(affine)) = (&overlay_view, &unprocessed, placement)
    {
        let mut placed = Scene::new();
        placed.append(unprocessed, Some(affine));
        if let Err(error) = renderer.render_to_texture(
            device.wgpu_device(),
            &queue,
            &placed,
            view,
            &RenderParams {
                base_color: vello::peniko::Color::TRANSPARENT,
                width: pixels.x,
                height: pixels.y,
                antialiasing_method: CANVAS_ANTIALIASING,
            },
        ) {
            error!("Vello failed to render the drawables left out of the post-process: {error}");
        }
    }
}

impl CompositePipelines {
    fn new(device: &RenderDevice) -> Self {
        let layout = device.create_bind_group_layout(
            "gaanim_canvas_composite",
            &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        );
        let region = device.create_buffer(&BufferDescriptor {
            label: Some("gaanim_canvas_region"),
            size: std::mem::size_of::<[f32; 4]>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader =
            device
                .wgpu_device()
                .create_shader_module(vello::wgpu::ShaderModuleDescriptor {
                    label: Some("gaanim_canvas_composite"),
                    source: vello::wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
                });
        Self {
            layout,
            shader,
            pipelines: Vec::new(),
            region,
            // Nothing written yet: the first frame always uploads its region.
            region_value: [0.0; 4],
            bind_group: None,
        }
    }

    fn pipeline(&self, format: TextureFormat, samples: u32) -> Option<&RenderPipeline> {
        self.pipelines
            .iter()
            .find(|(key, _)| *key == (format, samples))
            .map(|(_, pipeline)| pipeline)
    }

    fn ensure_pipeline(&mut self, device: &RenderDevice, format: TextureFormat, samples: u32) {
        if self.pipeline(format, samples).is_some() {
            return;
        }
        let layout = device.create_pipeline_layout(&vello::wgpu::PipelineLayoutDescriptor {
            label: Some("gaanim_canvas_composite"),
            bind_group_layouts: &[Some(&self.layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RawRenderPipelineDescriptor {
            label: Some("gaanim_canvas_composite"),
            layout: Some(&layout),
            vertex: RawVertexState {
                module: &self.shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(RawFragmentState {
                module: &self.shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format,
                    // Vello leaves straight alpha; blend it over the 3D pass.
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.push(((format, samples), pipeline));
    }
}

fn prepare_composite(
    extracted: Res<ExtractedCanvas>,
    views: Query<(&ViewTarget, &Msaa), With<VelloView>>,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut composite: ResMut<CompositePipelines>,
) {
    for (target, msaa) in &views {
        composite.ensure_pipeline(&device, target.main_texture_format(), msaa.samples());
    }
    composite.ensure_pipeline(&device, MIRROR_FORMAT, 1);
    let Some(image) = extracted.image.and_then(|image| images.get(image)) else {
        composite.bind_group = None;
        return;
    };
    let full = image.texture_descriptor.size;
    let full = UVec2::new(full.width, full.height);
    let used = scaled_canvas_size(full, extracted.scale).as_vec2() / full.as_vec2();
    let region = [used.x, used.y, 0.0, 0.0];
    if composite.region_value != region {
        let bytes: Vec<u8> = region
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        queue.write_buffer(&composite.region, 0, &bytes);
        composite.region_value = region;
    }
    let view = image.texture_view.id();
    if composite
        .bind_group
        .as_ref()
        .is_some_and(|(bound, _)| *bound == view)
    {
        return;
    }
    let bind_group = device.create_bind_group(
        "gaanim_canvas_composite",
        &composite.layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&image.texture_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: composite.region.as_entire_binding(),
            },
        ],
    );
    composite.bind_group = Some((view, bind_group));
}

/// Draws the canvas texture over the camera's main 2D pass.
fn composite_canvas(
    view: ViewQuery<(&ExtractedCamera, &ViewTarget, &Msaa), With<VelloView>>,
    composite: Option<Res<CompositePipelines>>,
    mut ctx: RenderContext,
) {
    let (camera, target, msaa) = view.into_inner();
    let Some(composite) = composite else {
        return;
    };
    let (Some((_, bind_group)), Some(pipeline)) = (
        composite.bind_group.as_ref(),
        composite.pipeline(target.main_texture_format(), msaa.samples()),
    ) else {
        return;
    };
    let color_attachments = [Some(target.get_color_attachment())];
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("gaanim_canvas_composite"),
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    if let Some(viewport) = camera.viewport.as_ref() {
        pass.set_camera_viewport(viewport);
    }
    pass.set_render_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

/// Draws the canvas, post-processed, into the [`CanvasMirror`] image over
/// its clear color. It runs at the mirror's few pixels, after the canvas
/// reached the camera, and only while a mirror is set.
fn mirror_canvas(
    _view: ViewQuery<(), With<VelloView>>,
    mirror: Res<ExtractedMirror>,
    images: Res<RenderAssets<GpuImage>>,
    composite: Option<Res<CompositePipelines>>,
    mut ctx: RenderContext,
) {
    let Some(target) = mirror.image.and_then(|image| images.get(image)) else {
        return;
    };
    let Some(composite) = composite else {
        return;
    };
    let (Some((_, bind_group)), Some(pipeline)) = (
        composite.bind_group.as_ref(),
        composite.pipeline(MIRROR_FORMAT, 1),
    ) else {
        return;
    };
    let clear = mirror.clear;
    let color_attachments = [Some(RenderPassColorAttachment {
        view: &target.texture_view,
        depth_slice: None,
        resolve_target: None,
        ops: Operations {
            load: LoadOp::Clear(vello::wgpu::Color {
                r: f64::from(clear.red),
                g: f64::from(clear.green),
                b: f64::from(clear.blue),
                a: f64::from(clear.alpha),
            }),
            store: StoreOp::Store,
        },
    })];
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("gaanim_canvas_mirror"),
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_render_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

/// One full-screen triangle that copies the canvas texel under each pixel.
/// A reduced preview fills only part of the texture and is scaled up with
/// bilinear filtering of premultiplied colors, so transparent texels do not
/// darken edges. Vello writes sRGB-encoded values into a linear texture, so
/// they are decoded before the sRGB view target encodes them again.
const COMPOSITE_SHADER: &str = r"
@group(0) @binding(0) var canvas: texture_2d<f32>;
// xy: fraction of the texture that holds the rasterized canvas.
@group(0) @binding(1) var<uniform> region: vec4<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOutput;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn linear_from_srgba(srgba: vec4<f32>) -> vec4<f32> {
    return vec4(
        select(
            srgba.rgb / 12.92,
            pow((srgba.rgb + 0.055) / 1.055, vec3(2.4)),
            srgba.rgb > vec3(0.04045),
        ),
        srgba.a,
    );
}

fn premultiplied(texel: vec2<i32>) -> vec4<f32> {
    let color = textureLoad(canvas, texel, 0);
    return vec4(color.rgb * color.a, color.a);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let size = textureDimensions(canvas);
    if all(region.xy >= vec2(1.0)) {
        let texel = min(vec2<u32>(in.uv * vec2<f32>(size)), size - vec2<u32>(1u));
        return linear_from_srgba(textureLoad(canvas, texel, 0));
    }
    let used = max(vec2<f32>(size) * region.xy, vec2(1.0));
    let last = vec2<i32>(round(used)) - vec2(1);
    let position = in.uv * used - vec2(0.5);
    let base = floor(position);
    let weight = position - base;
    let low = clamp(vec2<i32>(base), vec2(0), last);
    let high = clamp(vec2<i32>(base) + vec2(1), vec2(0), last);
    let color = mix(
        mix(premultiplied(low), premultiplied(vec2(high.x, low.y)), weight.x),
        mix(premultiplied(vec2(low.x, high.y)), premultiplied(high), weight.x),
        weight.y,
    );
    if color.a <= 0.0 {
        return vec4(0.0);
    }
    return linear_from_srgba(vec4(color.rgb / color.a, color.a));
}
";

#[cfg(test)]
mod tests {
    use super::*;
    use vello::kurbo::{Circle, Rect};
    use vello::peniko::{Color, Fill};

    fn scene(radius: f64) -> Scene {
        let mut scene = Scene::new();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &Circle::new((0.0, 0.0), radius),
        );
        scene
    }

    #[test]
    fn the_canvas_shows_a_turned_camera_as_exports_do() {
        use bevy::camera::CameraProjection;
        use gaanim_core::glam::{DQuat, DVec3};
        use gaanim_math::{CameraViewport, Projection as Lens, ResolvedCamera};
        use vello::kurbo::Point;

        let (width, height) = (1600.0_f32, 900.0_f32);
        let shifted = CameraViewport {
            scale: 0.8,
            offset_y: -120.0,
            offset_x: -210.0,
        };
        for (angle, (x, y), zoom, viewport) in [
            (0.0, (0.0, 0.0), 1.0, CameraViewport::default()),
            (0.0, (4.0, -2.0), 2.5, shifted),
            (0.3, (-9.0, 3.2), 0.7, CameraViewport::default()),
            (2.03, (6.0, 1.5), 1.0, shifted),
            (-2.5, (-3.0, -7.0), 1.7, shifted),
        ] {
            let mut camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
            camera.position = DVec3::new(x, y, 0.0);
            camera.rotation = DQuat::from_rotation_z(angle);
            camera.projection = Lens::Orthographic { zoom };
            let pose = crate::pipeline::canvas_camera_pose(&ResolvedCamera::new(camera, viewport));
            let mut projection = OrthographicProjection {
                scale: pose.scale,
                ..OrthographicProjection::default_2d()
            };
            projection.update(width, height);
            let canvas = scene_to_canvas(
                Transform::from_scale(Vec3::new(1.0, -1.0, 1.0)).to_matrix(),
                Transform::from_translation(pose.translation.extend(0.0))
                    .with_rotation(pose.rotation)
                    .to_matrix(),
                projection.get_clip_from_view(),
                Vec2::new(width, height),
            );
            let export = camera.to_vello_transform_with_viewport(viewport);
            for point in [Point::ORIGIN, Point::new(3.0, -1.0), Point::new(x, y)] {
                let (shown, exported) = (canvas * point, export * point);
                assert!(
                    (shown - exported).hypot() < 0.05,
                    "angle {angle}: {point:?} at {shown:?}, exported at {exported:?}"
                );
            }
        }
    }

    /// Play `seconds` of frames whose duration depends on the preview scale.
    fn play(
        adapter: &mut ResolutionAdapter,
        scale: &mut f32,
        seconds: f64,
        frame_seconds: impl Fn(f32) -> f64,
    ) {
        let mut elapsed = 0.0;
        while elapsed < seconds {
            let dt = frame_seconds(*scale);
            *scale = adapter.update(true, dt, *scale);
            elapsed += dt;
        }
    }

    #[test]
    fn slow_rasterization_lowers_the_preview_until_a_pause() {
        let mut adapter = ResolutionAdapter::default();
        let mut scale = 1.0;
        // Rasterizing dominates: frame time follows the pixel count.
        let raster_bound = |scale: f32| 0.04 * f64::from(scale * scale);

        play(&mut adapter, &mut scale, 1.2, raster_bound);
        assert_eq!(scale, 1.0, "the first window and a single slow one wait");
        play(&mut adapter, &mut scale, 6.0, raster_bound);
        assert_eq!(scale, 0.5);

        assert_eq!(adapter.update(false, 0.016, scale), 1.0, "a pause is sharp");
        assert!(!adapter.playing);
    }

    #[test]
    fn a_reduction_that_does_not_help_is_undone() {
        let mut adapter = ResolutionAdapter::default();
        let mut scale = 1.0;
        // The scene, not the rasterizer, takes 40 ms whatever the resolution.
        play(&mut adapter, &mut scale, 6.0, |_| 0.04);
        assert_eq!(scale, 1.0);
        assert!(adapter.locked);

        let mut fluid = ResolutionAdapter::default();
        let mut scale = 1.0;
        play(&mut fluid, &mut scale, 6.0, |_| 1.0 / 60.0);
        assert_eq!(scale, 1.0);
        assert!(!fluid.locked);
    }

    #[test]
    fn preview_resolution_settings() {
        let fixed = |scale| PreviewResolution {
            adaptive: false,
            scale,
        };
        // Captures and exports that draw through the canvas keep every pixel.
        assert_eq!(PreviewResolution::default(), PreviewResolution::FULL);
        assert_eq!(
            PreviewResolution::from_setting(None),
            PreviewResolution::AUTO
        );
        assert!(PreviewResolution::from_setting(Some("auto")).adaptive);
        assert!(PreviewResolution::from_setting(Some("blurry")).adaptive);
        assert_eq!(PreviewResolution::from_setting(Some("FULL")), fixed(1.0));
        assert_eq!(PreviewResolution::from_setting(Some(" 0.5 ")), fixed(0.5));
        assert_eq!(
            PreviewResolution::from_setting(Some("0.01")),
            fixed(MIN_PREVIEW_SCALE)
        );
        assert_eq!(PreviewResolution::from_setting(Some("2")), fixed(1.0));
    }

    #[test]
    fn a_scaled_canvas_keeps_whole_pixels_inside_the_texture() {
        let full = UVec2::new(1920, 1080);
        assert_eq!(scaled_canvas_size(full, 1.0), full);
        assert_eq!(scaled_canvas_size(full, 0.5), UVec2::new(960, 540));
        assert_eq!(scaled_canvas_size(full, 0.75), UVec2::new(1440, 810));
        assert_eq!(scaled_canvas_size(full, f32::NAN), full);
        assert_eq!(scaled_canvas_size(UVec2::ONE, 0.25), UVec2::ONE);
    }

    #[test]
    fn an_identical_frame_keeps_the_shared_scene() {
        let mut shown = VelloScene2d::from(scene(1.0));
        let before = Arc::clone(&shown.0);

        assert!(!shown.replace_if_different(scene(1.0)));
        assert!(Arc::ptr_eq(&before, &shown.0));

        assert!(shown.replace_if_different(scene(2.0)));
        assert!(!Arc::ptr_eq(&before, &shown.0));
    }

    #[test]
    fn frames_differing_only_in_a_layer_or_a_gradient_are_different() {
        let mut layered = scene(1.0);
        layered.push_layer(
            Fill::NonZero,
            vello::peniko::BlendMode::default(),
            0.5,
            Affine::IDENTITY,
            &Rect::new(0.0, 0.0, 1.0, 1.0),
        );
        layered.pop_layer();
        assert!(!draws_same(&scene(1.0), &layered));

        let gradient = |end: Color| {
            let mut scene = Scene::new();
            let brush = vello::peniko::Gradient::new_linear((0.0, 0.0), (1.0, 0.0))
                .with_stops([Color::BLACK, end]);
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &brush,
                None,
                &Rect::new(0.0, 0.0, 1.0, 1.0),
            );
            scene
        };
        assert!(draws_same(&gradient(Color::WHITE), &gradient(Color::WHITE)));
        assert!(!draws_same(
            &gradient(Color::WHITE),
            &gradient(Color::from_rgb8(255, 0, 0))
        ));
    }
}
