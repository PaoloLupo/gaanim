pub mod anim;
pub mod builder;
pub mod prelude;

use bevy::prelude::*;

/// Main Bevy Plugin for the high-level fluent API helper systems.
pub struct GaanimApiPlugin;

impl Plugin for GaanimApiPlugin {
    fn build(&self, _app: &mut App) {
        // High-level API does not require complex rendering system schedules on its own.
        // It wraps and drive the scene, animation, timeline, and objects systems.
    }
}
