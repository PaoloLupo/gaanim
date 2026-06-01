pub mod font;
pub mod shaper;
pub mod typst_compiler;
pub mod config;
pub mod prelude;

use bevy::prelude::*;
use font::FontRegistry;
use config::TextConfig;

/// The Bevy plugin initializing the central FontRegistry and TextConfig caches.
pub struct GaanimTextPlugin;

impl Plugin for GaanimTextPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FontRegistry>()
           .init_resource::<TextConfig>();
    }
}
