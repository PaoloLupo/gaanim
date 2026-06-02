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
    /// Phase 3: Applying custom Mobject updaters (e.g. rotate updater, orbit updaters, tracked paths).
    Updaters,
    /// Phase 4: Resolving layout constraints (e.g. flexbox, pins, coordinate alignments).
    Layout,
    /// Phase 5: Propagating child-parent hierarchies (e.g. spatial transforms and opacity cascade).
    Propagation,
    /// Phase 6: Computing local and world bounding boxes (AABBs) for culling and clipping.
    Bounds,
    /// Phase 7: Extracting visible Mobjects into the Vello or 3D rendering cache.
    Extraction,
    /// Phase 8: Performing pointer hover, click, drag hit testing and dispatching event callbacks.
    Interaction,
}

/// Core hierarchy and schedule plugin for `gaanim_scene`.
///
/// Registers the global `SceneSet` update pipeline ordering.
pub struct GaanimScenePlugin;

impl Plugin for GaanimScenePlugin {
    fn build(&self, app: &mut App) {
        // Enforce deterministic execution order:
        // Input -> Animation -> Updaters -> Layout -> Propagation -> Bounds -> Extraction -> Interaction
        app.configure_sets(
            Update,
            (
                SceneSet::Input,
                SceneSet::Animation,
                SceneSet::Updaters,
                SceneSet::Layout,
                SceneSet::Propagation,
                SceneSet::Bounds,
                SceneSet::Extraction,
                SceneSet::Interaction,
            )
                .chain(),
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

        // Register bounds systems in the Bounds SystemSet
        app.add_systems(
            Update,
            (
                crate::systems::world_bounds_propagation_system,
                crate::systems::world_bounds_fallback_system,
                crate::systems::hierarchical_bounds_system,
            )
                .in_set(SceneSet::Bounds),
        );
    }
}
