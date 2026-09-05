pub mod anim;
pub mod builder;
pub mod canvas;
pub mod export;
pub mod host;
pub mod matrix;
pub mod prelude;
pub mod runtime;

use bevy::prelude::*;
use gaanim_animation::signals::FloatSignal;
use gaanim_scene::{LocalBounds, Path2D, PathSource, SceneSet};
use gaanim_text::font::FontRegistry;
use gaanim_text::shaper::compile_text_to_path;

pub use gaanim_animation::{
    CustomAnimation, CustomChannel, CustomValues, PropertyChannel, PropertySources,
    ReactiveFunction, ReactiveReadout, ScalarSource, format_reactive_number,
    reactive_readout_update_system,
};

/// Component for dynamically-rendered decimal numbers that bind to a FloatSignal.
#[derive(Component, Debug, Clone)]
pub struct DecimalNumber {
    pub signal_entity: Entity,
    pub num_decimals: usize,
    pub prefix: String,
    pub suffix: String,
    pub font_family: String,
    pub font_size: f64,
    pub last_value: Option<f64>,
}

/// System that updates DecimalNumber mobjects when their bound FloatSignal changes.
pub fn decimal_number_update_system(
    registry: Res<FontRegistry>,
    mut query: Query<(
        Entity,
        &mut DecimalNumber,
        &mut Path2D,
        &mut PathSource,
        &mut LocalBounds,
    )>,
    signals: Query<&FloatSignal>,
) {
    for (_entity, mut dec, mut path, mut path_src, mut bounds) in &mut query {
        if let Ok(sig) = signals.get(dec.signal_entity) {
            let val = sig.value;
            if dec.last_value != Some(val) {
                let text = format!(
                    "{}{:.width$}{}",
                    dec.prefix,
                    val,
                    dec.suffix,
                    width = dec.num_decimals
                );
                if let Ok((new_path, new_bounds)) =
                    compile_text_to_path(&registry, &text, &dec.font_family, dec.font_size)
                {
                    let arc_path = std::sync::Arc::new(new_path);
                    path.0 = arc_path.clone();
                    path_src.0 = arc_path;
                    bounds.0 = new_bounds;
                    dec.last_value = Some(val);
                }
            }
        }
    }
}

/// Main Bevy Plugin for the high-level fluent API helper systems.
pub struct GaanimApiPlugin;

impl Plugin for GaanimApiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            decimal_number_update_system.in_set(SceneSet::Updaters),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::format_reactive_number;

    #[test]
    fn reactive_number_format_supports_precision_grouping_sign_and_percent() {
        assert_eq!(format_reactive_number(1234.5, ",.2f", "—"), "1,234.50");
        assert_eq!(format_reactive_number(0.125, "+.1%", "—"), "+12.5%");
        assert_eq!(
            format_reactive_number(f64::NAN, ".2f", "invalid"),
            "invalid"
        );
        assert_eq!(format_reactive_number(2.0, "6.0f", "—"), "     2");
    }
}
