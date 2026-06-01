pub mod font;
pub mod shaper;
pub mod typst_compiler;
pub mod prelude;

use bevy::prelude::*;
use font::FontRegistry;

/// The Bevy plugin initializing the central FontRegistry cache.
pub struct GaanimTextPlugin;

impl Plugin for GaanimTextPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FontRegistry>();
    }
}
