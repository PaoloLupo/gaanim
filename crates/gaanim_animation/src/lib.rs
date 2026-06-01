pub mod prelude;
pub mod signals;
pub mod tween;

pub use signals::{
    signal_binding_system, AlwaysRedraw, ColorSignal, FloatSignal, MobjectSpec, Signal,
    SignalBinding, SpecValue, Vec3Signal,
};
pub use tween::{
    evaluate_tweens_system, evaluate_custom_tweens_system, sync_delta_time_system, AnimatableLens, DeltaTime, PathCompletion, PropertyLens, Tween,
    TweenState,
};

use bevy::prelude::*;
use gaanim_scene::SceneSet;

/// Main plugin registering the tween evaluation and reactive signal systems in Bevy's deterministic schedule.
pub struct GaanimAnimationPlugin;

impl bevy::prelude::Plugin for GaanimAnimationPlugin {
    fn build(&self, app: &mut App) {
        // Register DeltaTime resource
        app.init_resource::<DeltaTime>();

        // Sync Bevy's Time -> DeltaTime before animation evaluation.
        app.add_systems(
            Update,
            sync_delta_time_system
                .in_set(SceneSet::Input),
        );

        // Register tween evaluation in the Animation Phase.
        // evaluate_tweens_system runs in parallel for built-in lenses,
        // followed by evaluate_custom_tweens_system for exclusive World access.
        app.add_systems(
            Update,
            (
                evaluate_tweens_system,
                evaluate_custom_tweens_system.after(evaluate_tweens_system),
            )
                .in_set(SceneSet::Animation),
        );

        // Register standard signal binders in the Updaters Phase
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
