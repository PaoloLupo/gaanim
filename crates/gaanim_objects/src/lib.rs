pub mod boolean;
pub mod prelude;
pub mod primitives;
pub mod svg;
pub mod text;

use bevy::prelude::*;

/// Main Bevy Plugin registering the Mobject primitives and visual components.
pub struct GaanimObjectsPlugin;

impl Plugin for GaanimObjectsPlugin {
    fn build(&self, _app: &mut App) {
        // High-level primitives and components are spawned as custom static Bevy bundles
        // and animated/rendered by the other downstream engine plugins.
    }
}
