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
use gaanim_core::kurbo::BezPath;
use gaanim_math::Bounds3D;
use gaanim_scene::{LocalBounds, Path2D, PathSource, SceneSet};
use gaanim_text::font::FontRegistry;

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

/// Shape DecimalNumber text through Typst, the resolver `scene.text` uses.
///
/// The pen origin and baseline stay at `(0, 0)` and, as with the legacy
/// shaper, the bounds include that origin. Glyphs are cached per character so
/// continuously changing values do not grow the Typst cache without bound.
pub(crate) fn shape_decimal_number(
    registry: &FontRegistry,
    prefix: &str,
    value: f64,
    num_decimals: usize,
    suffix: &str,
    font_family: &str,
    font_size: f64,
) -> Result<(BezPath, Bounds3D), String> {
    let number = format!("{value:.num_decimals$}");
    let (path, ink) = gaanim_animation::shape_readout_text(
        registry,
        prefix,
        &number,
        suffix,
        font_family,
        font_size,
    )?;
    Ok((path, ink.union(&Bounds3D::new_2d(0.0, 0.0, 0.0, 0.0))))
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
                if let Ok((new_path, new_bounds)) = shape_decimal_number(
                    &registry,
                    &dec.prefix,
                    val,
                    dec.num_decimals,
                    &dec.suffix,
                    &dec.font_family,
                    dec.font_size,
                ) {
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
    use super::*;
    use gaanim_core::kurbo::{PathEl, Shape};

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

    #[test]
    fn readout_decimal_separator_localizes_digits_and_grouping() {
        use gaanim_animation::localize_decimal_separator;
        assert_eq!(localize_decimal_separator("3.14", ','), "3,14");
        assert_eq!(localize_decimal_separator("1,234.50", ','), "1.234,50");
        assert_eq!(localize_decimal_separator("-0.5", '.'), "-0.5");
        assert_eq!(localize_decimal_separator("2.5e-3", ','), "2,5e-3");
    }

    #[test]
    fn decimal_number_resolves_family_like_scene_text() {
        let mut app = App::new();
        app.insert_resource(FontRegistry::new());
        app.add_systems(Update, decimal_number_update_system);
        let signal = app.world_mut().spawn(FloatSignal::new(2.5)).id();
        let empty = std::sync::Arc::new(BezPath::new());
        let number = app
            .world_mut()
            .spawn((
                DecimalNumber {
                    signal_entity: signal,
                    num_decimals: 2,
                    prefix: "x = ".to_owned(),
                    suffix: " m".to_owned(),
                    font_family: "Libertinus Serif".to_owned(),
                    font_size: 0.75,
                    last_value: None,
                },
                Path2D(empty.clone()),
                PathSource(empty),
                LocalBounds(Bounds3D::default()),
            ))
            .id();

        app.update();

        // Libertinus is known to Typst but not by name to the legacy registry,
        // which silently fell back to a system sans face.
        let registry = app.world().resource::<FontRegistry>();
        let expected = gaanim_text::typst_compiler::shape_typst_text_run(
            registry,
            "x = 2.50 m",
            "Libertinus Serif",
            None,
            0.75,
        )
        .unwrap()
        .path;
        let points = |path: &BezPath| {
            path.elements()
                .iter()
                .flat_map(|element| match *element {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
                    PathEl::QuadTo(a, b) => vec![a, b],
                    PathEl::CurveTo(a, b, c) => vec![a, b, c],
                    PathEl::ClosePath => vec![],
                })
                .collect::<Vec<_>>()
        };
        let path = app.world().get::<Path2D>(number).unwrap().0.clone();
        assert_eq!(path.elements().len(), expected.elements().len());
        for (actual, expected) in points(&path).into_iter().zip(points(&expected)) {
            assert!(
                actual.distance(expected) < 1e-9,
                "{actual:?} != {expected:?}"
            );
        }
        assert!(std::sync::Arc::ptr_eq(
            &path,
            &app.world().get::<PathSource>(number).unwrap().0
        ));
        if let Ok((legacy, _)) = gaanim_text::shaper::compile_text_to_path(
            registry,
            "x = 2.50 m",
            "Libertinus Serif",
            0.75,
        ) {
            assert_ne!(legacy.bounding_box(), expected.bounding_box());
        }

        // Bounds keep the legacy convention of including the pen origin.
        let ink = expected.bounding_box();
        let bounds = app.world().get::<LocalBounds>(number).unwrap().0;
        let wanted = Bounds3D::new_2d(ink.x0.min(0.0), ink.y0.min(0.0), ink.x1, ink.y1);
        assert!((bounds.min - wanted.min).length() < 1e-9, "{bounds:?}");
        assert!((bounds.max - wanted.max).length() < 1e-9, "{bounds:?}");
    }
}
