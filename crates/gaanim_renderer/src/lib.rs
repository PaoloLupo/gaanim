use bevy::prelude::*;

pub mod background;
pub mod effects;
pub mod pipeline;
pub mod prelude;

// MainVelloScene is re-exported via prelude; used implicitly by the plugin system registration.
#[allow(unused_imports)]
use pipeline::MainVelloScene;

/// Bevy integration plugin for the high-performance Vello vector renderer.
pub struct GaanimRendererPlugin;

impl Plugin for GaanimRendererPlugin {
    fn build(&self, app: &mut App) {
        // Automatically register bevy_vello if not already registered
        if !app.is_plugin_added::<bevy_vello::VelloPlugin>() {
            app.add_plugins(bevy_vello::VelloPlugin::default());
        }

        // Initialize the fragment retain cache
        app.init_resource::<pipeline::GaanimRenderCache>();

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
    }
}
