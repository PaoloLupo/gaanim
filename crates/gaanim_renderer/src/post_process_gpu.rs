//! Post-processes the Vello canvas texture of the interactive window.

use bevy::core_pipeline::{Core2d, Core2dSystems};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::window::PrimaryWindow;
use gaanim_core::kurbo;

use crate::canvas::{PreviewResolution, VelloCanvas, VelloView, scaled_canvas_size};
use crate::post_process::{CanvasPostProcess, GpuPostProcess, PostProcessRequest};

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

pub(crate) fn build(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<ExtractedPostProcess>()
        .init_resource::<RenderPostProcess>()
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
    (canvas, preview): (Res<VelloCanvas>, Option<Res<PreviewResolution>>),
    signals: Query<&gaanim_animation::FloatSignal>,
    mut frame: ResMut<PostProcessFrame>,
) {
    frame.0 = (|| {
        let (post, camera) = (post?, camera?);
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
        let mut request = post.request_with(
            time,
            kurbo::Rect::new(origin.x, origin.y, origin.x + size.x, origin.y + size.y),
            |entity| signals.get(entity).ok().map(|signal| signal.value),
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
) {
    let target = extracted
        .0
        .as_ref()
        .and_then(|(request, image)| Some((request, images.get(*image)?)));
    match target {
        Some((request, image)) => {
            state
                .0
                .prepare(device.wgpu_device(), &queue, &image.texture, Some(request));
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
