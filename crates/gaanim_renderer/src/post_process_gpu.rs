//! Post-processes the Vello canvas texture of the interactive window.

use bevy::core_pipeline::{Core2d, Core2dSystems};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::window::PrimaryWindow;
use gaanim_core::kurbo;
use vello::wgpu;

use crate::canvas::{PreviewResolution, VelloCanvas, VelloView, scaled_canvas_size};
use crate::post_process::{
    CanvasPostProcess, GpuPostProcess, PostProcessRequest, TransitionInputs,
};

/// Post-process of the next frame and the canvas texture it applies to.
#[derive(Resource, Default)]
struct PostProcessFrame(Option<(PostProcessRequest, AssetId<Image>)>);

#[derive(Resource, Default)]
pub(crate) struct ExtractedPostProcess(Option<(PostProcessRequest, AssetId<Image>)>);

impl ExtractedPostProcess {
    /// Whether this frame post-processes the canvas texture.
    pub(crate) fn is_active(&self) -> bool {
        self.0.is_some()
    }
}

#[derive(Resource, Default)]
struct RenderPostProcess(GpuPostProcess);

/// Render-world targets of a shader transition's incoming segment and of the
/// layer above its blend, the size of the canvas texture. `render_canvas`
/// draws them while a transition runs.
#[derive(Resource, Default)]
pub(crate) struct CanvasTransitionTargets(Option<[(wgpu::Texture, wgpu::TextureView); 2]>);

impl CanvasTransitionTargets {
    /// Views of the incoming and above targets, if a transition prepared them.
    pub(crate) fn views(&self) -> Option<(&wgpu::TextureView, &wgpu::TextureView)> {
        self.0
            .as_ref()
            .map(|[incoming, above]| (&incoming.1, &above.1))
    }

    /// Targets matching `size`, created or resized as needed.
    fn ensure(
        &mut self,
        device: &wgpu::Device,
        size: wgpu::Extent3d,
    ) -> &[(wgpu::Texture, wgpu::TextureView); 2] {
        let fits = self
            .0
            .as_ref()
            .is_some_and(|[incoming, _]| incoming.0.size() == size);
        if !fits {
            self.0 = Some(
                [
                    "gaanim-canvas-transition-incoming",
                    "gaanim-canvas-transition-above",
                ]
                .map(|label| {
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::STORAGE_BINDING
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    let view = texture.create_view(&Default::default());
                    (texture, view)
                }),
            );
        }
        self.0.as_ref().expect("targets were just created")
    }
}

pub(crate) fn build(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<ExtractedPostProcess>()
        .init_resource::<RenderPostProcess>()
        .init_resource::<CanvasTransitionTargets>()
        .add_systems(ExtractSchedule, extract_post_process)
        .add_systems(
            Render,
            prepare_post_process
                .in_set(RenderSystems::PrepareResources)
                .run_if(resource_exists::<RenderDevice>),
        )
        // Vello draws its target before the camera passes sample it, so the
        // pass runs ahead of the main pass of the Vello camera.
        .add_systems(Core2d, encode_post_process.in_set(Core2dSystems::Prepass));
    app.init_resource::<PostProcessFrame>().add_systems(
        Update,
        update_post_process_frame.in_set(gaanim_scene::SceneSet::Extraction),
    );
}

#[allow(clippy::too_many_arguments)]
fn update_post_process_frame(
    post: Option<Res<CanvasPostProcess>>,
    camera: Option<Res<gaanim_math::ResolvedCamera>>,
    playback: Option<Res<gaanim_animation::PlaybackState>>,
    ambient: Option<Res<gaanim_animation::AmbientClock>>,
    views: Query<(&Camera, Option<&bevy::camera::RenderTarget>), With<VelloView>>,
    windows: Query<&Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    (canvas, preview, layers): (
        Res<VelloCanvas>,
        Option<Res<PreviewResolution>>,
        Option<Res<crate::pipeline::TransitionLayers>>,
    ),
    signals: Query<&gaanim_animation::FloatSignal>,
    mut frame: ResMut<PostProcessFrame>,
) {
    frame.0 = (|| {
        let camera = camera?;
        let (view, target) = views.single().ok()?;
        let window = match target {
            Some(bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(entity))) => {
                *entity
            }
            Some(bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Primary)) | None => {
                primary_window.single().ok()?
            }
            Some(_) => return None,
        };
        let window = windows.get(window).ok()?;
        let viewport =
            crate::pipeline::fitted_canvas_viewport(&camera.camera, camera.viewport, window)?;
        // The canvas texture covers the camera viewport when one is set.
        let target_origin = view
            .viewport
            .as_ref()
            .map_or(UVec2::ZERO, |viewport| viewport.physical_position);
        // A reduced preview rasterizes the viewport into part of the texture.
        let full = canvas.size().max(UVec2::ONE);
        let scale = preview.as_ref().map_or(1.0, |preview| preview.scale);
        let used = scaled_canvas_size(full, scale).as_dvec2() / full.as_dvec2();
        let origin = (viewport.physical_position.as_dvec2() - target_origin.as_dvec2()) * used;
        let size = viewport.physical_size.as_dvec2() * used;
        let time = playback.map_or(0.0, |state| state.current_time);
        let rect = kurbo::Rect::new(origin.x, origin.y, origin.x + size.x, origin.y + size.y);
        let request = post.and_then(|post| {
            post.request_with(time, rect, |entity| {
                signals.get(entity).ok().map(|signal| signal.value)
            })
        });
        // A shader transition blends its segments first.
        let mut request = PostProcessRequest::with_transition(
            request,
            layers
                .as_ref()
                .and_then(|layers| layers.0.as_ref())
                .map(|layers| &layers.shader),
            rect,
            time as f32,
        )?;
        // The passes are the timeline's; a resting presentation keeps them
        // moving (see `AmbientClock`).
        if let Some(clock) = ambient {
            request.time = clock.at(time) as f32;
        }
        (canvas.image != Handle::default()).then(|| (request, canvas.image.id()))
    })();
}

fn extract_post_process(
    frame: Extract<Option<Res<PostProcessFrame>>>,
    mut extracted: ResMut<ExtractedPostProcess>,
) {
    extracted.0 = frame.as_ref().and_then(|frame| frame.0.clone());
}

fn prepare_post_process(
    extracted: Res<ExtractedPostProcess>,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut state: ResMut<RenderPostProcess>,
    mut targets: ResMut<CanvasTransitionTargets>,
) {
    let target = extracted
        .0
        .as_ref()
        .and_then(|(request, image)| Some((request, images.get(*image)?)));
    match target {
        Some((request, image)) => {
            let inputs = request.transition.as_ref().map(|_| {
                let [incoming, above] = targets.ensure(device.wgpu_device(), image.texture.size());
                TransitionInputs {
                    incoming: &incoming.0,
                    above: &above.0,
                }
            });
            state.0.prepare(
                device.wgpu_device(),
                &queue,
                &image.texture,
                Some(request),
                inputs,
            );
        }
        None => state.0.clear(),
    }
}

fn encode_post_process(
    _view: ViewQuery<(), With<VelloView>>,
    state: Res<RenderPostProcess>,
    mut ctx: RenderContext,
) {
    state.0.encode(ctx.command_encoder());
}
