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
    /// Phase 7: Computing local and world bounding boxes (AABBs) for culling and clipping.
    Bounds,
    /// Phase 8: Extracting visible Mobjects into the Vello or 3D rendering cache.
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

        // Register default propagation systems in the Propagation SystemSet.
        // Both propagation systems use `run_if` to skip entirely when no
        // local component has changed, avoiding unnecessary per-entity iteration
        // on static frames.
        app.add_systems(
            Update,
            (
                crate::systems::transform_propagation_system
                    .run_if(crate::systems::has_transform_changes),
                crate::systems::opacity_propagation_system
                    .run_if(crate::systems::has_opacity_changes),
                crate::systems::sync_new_opacities,
                crate::systems::style_propagation_system,
            )
                .in_set(SceneSet::Propagation),
        );
        // 3D helpers: sync mesh transforms and billboard after hierarchy propagation.
        app.add_systems(
            Update,
            (
                crate::systems::request_gltf_assets_system,
                crate::systems::ensure_default_3d_light_system,
                crate::systems::attach_gltf_scenes_system,
                crate::systems::finalize_gltf_instances_system,
            )
                .chain()
                .run_if(resource_exists::<AssetServer>)
                .in_set(SceneSet::Input),
        );
        app.add_systems(
            Update,
            (
                crate::systems::billboard_system,
                crate::systems::sync_3d_mesh_transform_system,
                crate::systems::sync_gltf_wrapper_transform_system,
                crate::systems::sync_gltf_visibility_system,
                crate::systems::sync_gltf_material_opacity_system
                    .run_if(resource_exists::<Assets<StandardMaterial>>),
                crate::systems::sync_material_3d_system
                    .run_if(resource_exists::<Assets<StandardMaterial>>),
            )
                .in_set(SceneSet::Propagation)
                .after(crate::systems::transform_propagation_system),
        );
        // Build raw mesh data into Bevy Mesh3d handles (runs before extraction).
        app.add_systems(
            Update,
            crate::systems::build_3d_meshes_system.in_set(SceneSet::Bounds),
        );
        app.add_systems(
            Update,
            crate::systems::update_3d_line_meshes_system
                .in_set(SceneSet::Bounds)
                .after(crate::systems::build_3d_meshes_system),
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
    fn scene_plugin_runs_without_pbr_asset_resources() {
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
