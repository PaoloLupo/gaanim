pub mod prelude;
pub mod signals;
pub mod tween;
pub mod updaters;
pub mod writing;

pub use signals::{
    AlwaysRedraw, AlwaysRedrawRegen, AxisMask, ColorSignal, FloatSignal, MobjectSpec,
    PositionBinding, Signal, SignalBinding, SpecValue, Vec3Signal, always_redraw_regen_system,
    position_binding_system, signal_binding_system,
};
pub use tween::{
    AnimatableLens, DeltaTime, PropertyLens, Tween, TweenState, evaluate_custom_tweens_system,
    evaluate_tweens_system, sync_delta_time_system,
};
pub use updaters::{
    PlaybackState, TracedPath, TrackingEndpoint, TrackingLine, Updater, advance_x_updater,
    bob_updater, follow_updater, orbit_updater, pulse_updater, rotate_updater,
    traced_path_system, tracking_line_system, updater_system,
};
pub use writing::{FillDrawProgress, PathSource, path_source_seed_added_system};

use bevy::prelude::*;
use gaanim_scene::SceneSet;

/// Main plugin registering the tween evaluation and reactive signal systems in Bevy's deterministic schedule.
pub struct GaanimAnimationPlugin;

impl bevy::prelude::Plugin for GaanimAnimationPlugin {
    fn build(&self, app: &mut App) {
        // Register DeltaTime resource
        app.init_resource::<DeltaTime>();
        app.init_resource::<PlaybackState>();

        // Sync Bevy's Time -> DeltaTime before animation evaluation.
        app.add_systems(Update, sync_delta_time_system.in_set(SceneSet::Input));

        // PathSource seed runs in Input so that any entity that has just
        // received a `Path2D` (from spawn, snapshot restore, morph, etc.)
        // gets a `PathSource` mirror before the animation phase reads it.
        app.add_systems(
            Update,
            path_source_seed_added_system
                .in_set(SceneSet::Input)
                .after(sync_delta_time_system),
        );

        // Register tween evaluation in the Animation Phase.
        // evaluate_tweens_system runs in parallel for built-in lenses,
        // followed by evaluate_custom_tweens_system for exclusive World access.
        // The `PathCompletion` lens writes the trimmed `Path2D` directly
        // from the cached `PathSource` (seeded once at spawn), so we no
        // longer need a separate application system for it.
        app.add_systems(
            Update,
            (
                evaluate_tweens_system,
                evaluate_custom_tweens_system.after(evaluate_tweens_system),
            )
                .in_set(SceneSet::Animation),
        );

        // Register standard signal binders and continuous updaters in the Updaters Phase.
        // Ordering: updaters run first (modify positions), then bindings copy positions,
        // then tracking lines and traced paths read the final positions.
        app.add_systems(
            Update,
            (
                updater_system,
                always_redraw_regen_system,
                position_binding_system.after(updater_system),
                tracking_line_system.after(position_binding_system),
                traced_path_system.after(tracking_line_system),
            )
                .in_set(SceneSet::Updaters),
        );
        app.add_systems(
            Update,
            (
                signal_binding_system::<f64>,
                signal_binding_system::<gaanim_core::glam::DVec3>,
                signal_binding_system::<gaanim_core::peniko::Color>,
            )
                .in_set(SceneSet::Updaters),
        );
    }
}
