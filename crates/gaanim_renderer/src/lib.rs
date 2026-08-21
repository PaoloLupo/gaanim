use bevy::prelude::*;

pub mod background;
pub mod diagnostics;
pub mod effects;
pub mod pipeline;
pub mod prelude;

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
            (
                pipeline::resolve_dynamic_clip_masks_system,
                pipeline::resolve_dynamic_boolean_system,
                pipeline::resolve_fill_level_system,
                pipeline::resolve_vector_outline_system,
            )
                .chain()
                .in_set(gaanim_scene::SceneSet::DerivedGeometry),
        );
    }
}

impl Plugin for GaanimRendererPlugin {
    fn build(&self, app: &mut App) {
        // Automatically register bevy_vello if not already registered
        if !app.is_plugin_added::<bevy_vello::VelloPlugin>() {
            app.add_plugins(bevy_vello::VelloPlugin::default());
        }

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
                pipeline::sync_gaanim_camera_to_bevy_3d_system,
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
        app.add_systems(
            Update,
            pipeline::gaanim_render_system.in_set(gaanim_scene::SceneSet::Extraction),
        );
        app.add_systems(
            Update,
            diagnostics::collect_vello_diagnostics_system
                .in_set(gaanim_scene::SceneSet::Extraction),
        );
    }
}
