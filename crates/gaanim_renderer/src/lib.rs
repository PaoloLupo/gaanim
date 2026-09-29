use bevy::prelude::*;

pub mod background;
mod background_gpu;
pub mod background_presets;
pub mod canvas;
pub mod diagnostics;
pub mod effects;
pub mod fragment;
mod gpu_scope;
pub mod lottie;
pub mod offscreen;
pub mod pipeline;
mod post_bloom;
pub mod post_presets;
pub mod post_process;
mod post_process_gpu;
pub mod prelude;
mod stroke;
mod three_d;

// MainVelloScene is re-exported via prelude; used implicitly by the plugin system registration.
#[allow(unused_imports)]
use pipeline::MainVelloScene;

/// Bevy integration plugin for the high-performance Vello vector renderer.
pub struct GaanimRendererPlugin;

/// Resolves vector geometry derived from other drawables without requiring a
/// window or the Vello render plugin. Headless vector capture uses this plugin
/// before compiling the world directly into a Vello scene.
pub struct GaanimDerivedGeometryPlugin;

impl Plugin for GaanimDerivedGeometryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            lottie::sample_lottie_system.in_set(gaanim_scene::SceneSet::Updaters),
        );
        app.add_systems(
            Update,
            (
                pipeline::resolve_dynamic_clip_masks_system,
                pipeline::resolve_dynamic_boolean_system,
                pipeline::resolve_fill_level_system,
                pipeline::resolve_vector_outline_system,
                pipeline::resolve_connect_system,
            )
                .chain()
                // A boolean or a mask may use a live frame as an operand, so
                // the frame is rebuilt first: without an order between them a
                // frame could be composed from the previous frame's outline.
                .after(gaanim_animation::surrounding_rect_system)
                .in_set(gaanim_scene::SceneSet::DerivedGeometry),
        );
        // Exports install only this plugin, so tips get their layers here.
        app.add_systems(
            Update,
            effects::sync_stroke_tip_layers_system.in_set(gaanim_scene::SceneSet::DerivedGeometry),
        );
    }
}

impl Plugin for GaanimRendererPlugin {
    fn build(&self, app: &mut App) {
        // Rasterize the composed scene with Vello and draw it in the window.
        app.add_plugins(canvas::VelloCanvasPlugin);
        // Scenes rendered into images (the web Presenter View's previews).
        app.add_plugins(offscreen::VelloImagePlugin);

        // Shader backgrounds render on the render device when one exists.
        background_gpu::build(app);
        // Post-processing runs on the Vello render target after Vello draws it.
        post_process_gpu::build(app);

        // Initialize the fragment retain cache
        app.init_resource::<pipeline::GaanimRenderCache>();
        app.init_resource::<diagnostics::RenderHealth>();
        app.init_resource::<diagnostics::VelloDiagnostics>();
        diagnostics::install_render_error_handler(app);

        // Register camera sync systems in Bounds phase so the Bevy cameras
        // match gaanim's Camera resource before rendering in Extraction.
        app.add_systems(
            Update,
            (
                pipeline::sync_canvas_background_clear_system,
                pipeline::sync_gaanim_camera_to_bevy_system,
            )
                .in_set(gaanim_scene::SceneSet::Bounds),
        );

        // Masks bind to source geometry instead of freezing a path at canvas
        // compilation time. This runs after hierarchy propagation and before
        // bounds/extraction, so transform and morph animation are frame exact.
        if !app.is_plugin_added::<GaanimDerivedGeometryPlugin>() {
            app.add_plugins(GaanimDerivedGeometryPlugin);
        }

        // Register cache cleanup systems before extraction.
        // Sweep dead fragments by comparing active ObjectIds against cache keys.
        app.add_systems(
            Update,
            pipeline::gaanim_render_cache_sweep_system.before(gaanim_scene::SceneSet::Extraction),
        );

        // Register the extraction and composition system in the scene extraction phase
        // A playback bundle shows recorded frames through the same
        // composition instead of the world's drawables.
        app.add_systems(
            Update,
            (
                pipeline::gaanim_render_system
                    .run_if(not(resource_exists::<pipeline::ExternalFrame>)),
                pipeline::external_frame_system.run_if(resource_exists::<pipeline::ExternalFrame>),
            )
                .in_set(gaanim_scene::SceneSet::Extraction),
        );
        app.add_systems(
            Update,
            diagnostics::collect_vello_diagnostics_system
                .in_set(gaanim_scene::SceneSet::Extraction),
        );
    }
}
