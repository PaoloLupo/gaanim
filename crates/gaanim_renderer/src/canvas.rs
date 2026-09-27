//! Draws the composed Vello scene into the interactive window.
//!
//! Each frame the render world rasterizes the [`MainVelloScene`] with Vello
//! into a storage texture sized to the [`VelloView`] camera's viewport, then a
//! full-screen pass composites that texture into the camera's main 2D pass.
//! Headless export bypasses this plugin and renders scenes directly.
//!
//! [`MainVelloScene`]: crate::pipeline::MainVelloScene

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    ColorWrites, Extent3d, MultisampleState, PrimitiveState, RawFragmentState,
    RawRenderPipelineDescriptor, RawVertexState, RenderPassDescriptor, RenderPipeline,
    ShaderStages, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureViewDimension, TextureViewId,
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

/// Whether two scenes draw exactly the same pixels: equal command streams and
/// equal late-bound resources. Images compare by their shared data, which
/// cannot change in place. Glyph runs are not compared (Gaanim draws text as
/// paths), so scenes that use them always differ.
pub fn draws_same(a: &Scene, b: &Scene) -> bool {
    use vello_encoding::Patch;
    let (a, b) = (a.encoding(), b.encoding());
    let (resources_a, resources_b) = (&a.resources, &b.resources);
    let same_patches = resources_a.patches.len() == resources_b.patches.len()
        && resources_a
            .patches
            .iter()
            .zip(&resources_b.patches)
            .all(|patches| match patches {
                (
                    Patch::Ramp {
                        draw_data_offset: offset_a,
                        stops: stops_a,
                        extend: extend_a,
                    },
                    Patch::Ramp {
                        draw_data_offset: offset_b,
                        stops: stops_b,
                        extend: extend_b,
                    },
                ) => offset_a == offset_b && stops_a == stops_b && extend_a == extend_b,
                (
                    Patch::Image {
                        draw_data_offset: offset_a,
                        image: image_a,
                    },
                    Patch::Image {
                        draw_data_offset: offset_b,
                        image: image_b,
                    },
                ) => offset_a == offset_b && image_a == image_b,
                _ => false,
            });
    a.n_paths == b.n_paths
        && a.n_path_segments == b.n_path_segments
        && a.n_clips == b.n_clips
        && a.n_open_clips == b.n_open_clips
        && resources_a.glyph_runs.is_empty()
        && resources_b.glyph_runs.is_empty()
        && a.path_tags == b.path_tags
        && a.path_data == b.path_data
        && a.draw_tags == b.draw_tags
        && a.draw_data == b.draw_data
        && a.transforms == b.transforms
        && a.styles == b.styles
        && resources_a.color_stops == resources_b.color_stops
        && same_patches
}

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
    /// Preview resolution scale; see [`PreviewResolution`].
    scale: f32,
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
            .add_systems(ExtractSchedule, extract_canvas)
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
                composite_canvas
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
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::STORAGE_BINDING;
    canvas.image = images.add(image);
    canvas.size = size;
}

fn extract_canvas(
    canvas: Extract<Res<VelloCanvas>>,
    preview: Extract<Option<Res<PreviewResolution>>>,
    scenes: Extract<Query<(&VelloScene2d, &Transform), With<MainVelloScene>>>,
    mut extracted: ResMut<ExtractedCanvas>,
) {
    extracted.image = (canvas.image != Handle::default()).then(|| canvas.image.id());
    extracted.scale = preview.as_ref().map_or(1.0, |preview| preview.scale);
    extracted.scene = scenes
        .iter()
        .next()
        .map(|(scene, transform)| (scene.0.clone(), transform.to_matrix()));
}

/// Maps the scene from world space to the canvas pixels of `view`, with the
/// viewport rasterized at `scale` of its pixels per axis.
///
/// Same chain as bevy_vello 0.14: world → view → projection → NDC → pixels,
/// with Y flipped into Vello's Y-down space.
fn scene_to_pixels(
    model: Mat4,
    camera: &ExtractedCamera,
    view: &ExtractedView,
    scale: Vec2,
) -> Option<Affine> {
    let size = camera.physical_viewport_size?;
    let (pixels_x, pixels_y) = (size.x as f32 * scale.x, size.y as f32 * scale.y);
    let ndc_to_pixels = Mat4::from_cols_array_2d(&[
        [pixels_x / 2.0, 0.0, 0.0, pixels_x / 2.0],
        [0.0, pixels_y / 2.0, 0.0, pixels_y / 2.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
    .transpose();
    let mut view_matrix = view.world_from_view.to_matrix();
    view_matrix.w_axis.y *= -1.0;
    let view_proj = view.clip_from_view * view_matrix.inverse();
    let mut model = model;
    model.w_axis.y *= -1.0;
    let m = (ndc_to_pixels * view_proj * model).to_cols_array();
    // Negated skews keep rotations turning the same way as the Y-up world.
    Some(Affine::new([
        m[0] as f64,
        -m[1] as f64,
        -m[4] as f64,
        m[5] as f64,
        m[12] as f64,
        m[13] as f64,
    ]))
}

/// Render-world cameras that show the canvas.
type CanvasViews<'w, 's> = Query<
    'w,
    's,
    (&'static ExtractedCamera, &'static ExtractedView),
    (With<Camera2d>, With<VelloView>),
>;

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
    effects: (
        Option<Res<crate::post_process_gpu::ExtractedPostProcess>>,
        Option<Res<crate::background_gpu::ExtractedShaderBackground>>,
    ),
    mut rendered: ResMut<RenderedCanvas>,
) {
    let Some(target) = extracted.image.and_then(|image| images.get(image)) else {
        return;
    };

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
    let (post, shader) = effects;
    let volatile = post.is_some_and(|post| post.is_active())
        || shader.is_some_and(|shader| shader.is_active());
    if !volatile && rendered.0.as_ref().is_some_and(|last| last.same(&current)) {
        return;
    }
    rendered.0 = None;

    let mut frame = Scene::new();
    let mut scenes = 0;
    if let Some((scene, affine)) = &current.scene {
        frame.append(scene, Some(*affine));
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
        Ok(()) => rendered.0 = (!volatile).then_some(current),
        Err(error) => error!("Vello failed to render the canvas: {error}"),
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
