pub mod config;
pub mod font;
pub mod prelude;
pub mod shaper;
pub mod structured;
pub mod typst_compiler;

use bevy::prelude::*;
use config::TextConfig;
use font::FontRegistry;

/// The Bevy plugin initializing the central FontRegistry and TextConfig caches.
pub struct GaanimTextPlugin;

impl Plugin for GaanimTextPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FontRegistry>()
            .init_resource::<TextConfig>();
    }
}
