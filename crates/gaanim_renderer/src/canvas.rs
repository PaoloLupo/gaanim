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
    BlendState, ColorTargetState, ColorWrites, Extent3d, MultisampleState, PrimitiveState,
    RawFragmentState, RawRenderPipelineDescriptor, RawVertexState, RenderPassDescriptor,
    RenderPipeline, ShaderStages, TextureDimension, TextureFormat, TextureSampleType,
    TextureUsages, TextureViewDimension,
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

/// The Vello renderer on the render device, shared with GPU shader backgrounds.
#[derive(Resource, Deref, DerefMut)]
pub struct VelloRenderer(Arc<Mutex<vello::Renderer>>);

impl VelloRenderer {
    fn try_new(device: &vello::wgpu::Device, use_cpu: bool) -> Result<Self, vello::Error> {
        vello::Renderer::new(
            device,
            vello::RendererOptions {
                use_cpu,
                // Vello cannot add antialiasing modes after initialization.
                antialiasing_support: AaSupport::all(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map(|renderer| Self(Arc::new(Mutex::new(renderer))))
    }
}

/// Antialiasing of the interactive canvas; exports choose their own.
const CANVAS_ANTIALIASING: AaConfig = AaConfig::Area;

/// Texture the canvas is rasterized into, sized to the camera viewport.
#[derive(Resource, Clone, Default)]
pub struct VelloCanvas {
    pub image: Handle<Image>,
    size: UVec2,
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
}

#[derive(Resource)]
struct CompositePipelines {
    layout: BindGroupLayout,
    shader: vello::wgpu::ShaderModule,
    pipelines: Vec<((TextureFormat, u32), RenderPipeline)>,
    bind_group: Option<BindGroup>,
}

pub(crate) struct VelloCanvasPlugin;

impl Plugin for VelloCanvasPlugin {
    fn build(&self, app: &mut App) {
        let stats = VelloFrameStats::default();
        app.add_plugins(ExtractComponentPlugin::<VelloView>::default())
            .init_resource::<VelloCanvas>()
            .insert_resource(stats.clone())
            .add_systems(PostUpdate, resize_canvas_target.after(CameraUpdateSystems));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .insert_resource(stats)
            .init_resource::<ExtractedCanvas>()
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
    scenes: Extract<Query<(&VelloScene2d, &Transform), With<MainVelloScene>>>,
    mut extracted: ResMut<ExtractedCanvas>,
) {
    extracted.image = (canvas.image != Handle::default()).then(|| canvas.image.id());
    extracted.scene = scenes
        .iter()
        .next()
        .map(|(scene, transform)| (scene.0.clone(), transform.to_matrix()));
}

/// Maps the scene from world space to the canvas pixels of `view`.
///
/// Same chain as bevy_vello 0.14: world → view → projection → NDC → pixels,
/// with Y flipped into Vello's Y-down space.
fn scene_to_pixels(model: Mat4, camera: &ExtractedCamera, view: &ExtractedView) -> Option<Affine> {
    let size = camera.physical_viewport_size?;
    let (pixels_x, pixels_y) = (size.x as f32, size.y as f32);
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
/// passes sample it.
fn render_canvas(
    extracted: Res<ExtractedCanvas>,
    views: CanvasViews,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    renderer: Res<VelloRenderer>,
    stats: Res<VelloFrameStats>,
) {
    let Some(target) = extracted.image.and_then(|image| images.get(image)) else {
        return;
    };

    let mut frame = Scene::new();
    let mut scenes = 0;
    if let Some((scene, model)) = &extracted.scene
        && let Some(affine) = views
            .iter()
            .last()
            .and_then(|(camera, view)| scene_to_pixels(*model, camera, view))
    {
        frame.append(scene, Some(affine));
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

    let size = target.texture_descriptor.size;
    let Ok(mut renderer) = renderer.lock() else {
        return;
    };
    if let Err(error) = renderer.render_to_texture(
        device.wgpu_device(),
        &queue,
        &frame,
        &target.texture_view,
        &RenderParams {
            base_color: vello::peniko::Color::TRANSPARENT,
            width: size.width,
            height: size.height,
            antialiasing_method: CANVAS_ANTIALIASING,
        },
    ) {
        error!("Vello failed to render the canvas: {error}");
    }
}

impl CompositePipelines {
    fn new(device: &RenderDevice) -> Self {
        let layout = device.create_bind_group_layout(
            "gaanim_canvas_composite",
            &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        );
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
    mut composite: ResMut<CompositePipelines>,
) {
    for (target, msaa) in &views {
        composite.ensure_pipeline(&device, target.main_texture_format(), msaa.samples());
    }
    composite.bind_group = extracted
        .image
        .and_then(|image| images.get(image))
        .map(|image| {
            device.create_bind_group(
                "gaanim_canvas_composite",
                &composite.layout,
                &[BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&image.texture_view),
                }],
            )
        });
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
    let (Some(bind_group), Some(pipeline)) = (
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
/// Vello writes sRGB-encoded values into a linear texture, so they are
/// decoded before the sRGB view target encodes them again.
const COMPOSITE_SHADER: &str = r"
@group(0) @binding(0) var canvas: texture_2d<f32>;

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

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let size = textureDimensions(canvas);
    let texel = min(vec2<u32>(in.uv * vec2<f32>(size)), size - vec2<u32>(1u));
    return linear_from_srgba(textureLoad(canvas, texel, 0));
}
";
