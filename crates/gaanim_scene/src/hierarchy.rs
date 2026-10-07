use bevy::prelude::*;

/// The global execution schedule ordering for a single animation frame update.
///
/// Centralizing execution ordering into `SceneSet` SystemSets guarantees 100%
/// deterministic updates, eliminating non-deterministic visual jitter, frame-lag on hierarchy,
/// or race conditions between timeline animations, physics-based springs, and layout pins.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneSet {
    /// Phase 1: Processing external inputs, Python scripting events, timeline seeks, and commands.
    Input,
    /// Phase 2: Evaluating animation tweens, lenses, and keyframes.
    Animation,
    /// Resolve reactive camera constraints after layout and before propagation.
    Camera,
    /// Phase 3: Applying custom Mobject updaters (e.g. rotate updater, orbit updaters, tracked paths).
    Updaters,
    /// Phase 4: Regenerating coordinate spaces, plots, and data-driven marks.
    Visualization,
    /// Phase 5: Resolving layout constraints (e.g. flexbox, pins, coordinate alignments).
    Layout,
    /// Phase 6: Propagating child-parent hierarchies (e.g. spatial transforms and opacity cascade).
    Propagation,
    /// Phase 7: Rebuild geometry derived from other vector paths (masks and booleans).
    DerivedGeometry,
    /// Phase 8: Computing local and world bounding boxes (AABBs) for culling and clipping.
    Bounds,
    /// Phase 9: Extracting visible Mobjects into the Vello or 3D rendering cache.
    Extraction,
    /// Phase 9: Performing pointer hover, click, drag hit testing and dispatching event callbacks.
    Interaction,
}

/// Core hierarchy and schedule plugin for `gaanim_scene`.
///
/// Registers the global `SceneSet` update pipeline ordering.
pub struct GaanimScenePlugin;

impl Plugin for GaanimScenePlugin {
    fn build(&self, app: &mut App) {
        // Enforce deterministic execution order:
        // Input -> Animation -> Updaters -> Visualization -> Layout ->
        // Propagation -> Bounds -> Extraction -> Interaction
        app.configure_sets(
            Update,
            (
                SceneSet::Input,
                SceneSet::Animation,
                SceneSet::Updaters,
                SceneSet::Visualization,
                SceneSet::Layout,
                SceneSet::Camera,
                SceneSet::Propagation,
                SceneSet::DerivedGeometry,
                SceneSet::Bounds,
                SceneSet::Extraction,
                SceneSet::Interaction,
            )
                .chain(),
        );

        app.init_resource::<gaanim_math::ResolvedCamera>()
            .init_resource::<gaanim_math::CameraViewOverride>()
            .init_resource::<gaanim_math::CameraViewport>()
            .add_systems(
                Update,
                crate::systems::resolve_camera_system.in_set(SceneSet::Camera),
            );

        app.add_systems(
            Update,
            (
                crate::media_frame::update_media_frames,
                crate::systems::coordinate_tick_level_system,
            )
                .in_set(SceneSet::Visualization),
        );

        // Register default propagation systems in the Propagation SystemSet.
        // Both propagation systems use `run_if` to skip entirely when no
        // local component has changed, avoiding unnecessary per-entity iteration
        // on static frames.
        app.add_systems(
            Update,
            (
                crate::systems::transform_propagation_system
                    .run_if(crate::systems::has_transform_changes),
                crate::systems::pin_parallax_layers_system
                    .after(crate::systems::transform_propagation_system),
                crate::systems::pin_hud_overlays_system
                    .after(crate::systems::pin_parallax_layers_system),
                crate::systems::opacity_propagation_system
                    .run_if(crate::systems::has_opacity_changes)
                    .after(crate::systems::sync_new_opacities),
                crate::systems::sync_new_opacities,
                crate::systems::style_propagation_system,
            )
                .in_set(SceneSet::Propagation),
        );
        // Billboards face the camera after hierarchy propagation.
        app.add_systems(
            Update,
            crate::systems::billboard_system
                .in_set(SceneSet::Propagation)
                .after(crate::systems::transform_propagation_system),
        );
        app.add_systems(
            Update,
            crate::systems::sync_3d_bounds_system.in_set(SceneSet::Bounds),
        );

        // Register bounds systems in the Bounds SystemSet.
        // The entire set is guarded by `has_bounds_changes` so that on
        // static frames (no transform/bounds mutations) all three systems
        // are skipped without per-entity iteration.
        app.add_systems(
            Update,
            (
                crate::systems::world_bounds_propagation_system,
                crate::systems::world_bounds_fallback_system,
                crate::systems::hierarchical_bounds_system,
            )
                .run_if(crate::systems::has_bounds_changes)
                .in_set(SceneSet::Bounds),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_plugin_runs_on_a_bare_app() {
        let mut app = App::new();
        app.add_plugins(GaanimScenePlugin);

        app.update();
    }

    #[test]
    fn presentation_override_never_mutates_authored_camera() {
        let mut app = App::new();
        app.add_plugins(GaanimScenePlugin);
        let authored = gaanim_math::Camera::ortho_2d(640, 360);
        let mut free = gaanim_math::Camera::perspective_3d(640, 360, 0.8);
        free.position.x = 7.0;
        app.insert_resource(authored)
            .insert_resource(gaanim_math::CameraViewOverride(Some(free)));

        app.update();

        assert_eq!(*app.world().resource::<gaanim_math::Camera>(), authored);
        assert_eq!(
            app.world().resource::<gaanim_math::ResolvedCamera>().camera,
            free
        );
    }
}
