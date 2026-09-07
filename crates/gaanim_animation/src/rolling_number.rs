//! Deterministic odometer geometry. Glyphs are shaped once, never per frame.
use std::collections::HashMap;

use bevy::prelude::Component;
use gaanim_core::kurbo::{Affine, BezPath, PathEl, Point, Shape};
use gaanim_math::Bounds3D;
use gaanim_text::{font::FontRegistry, shaper::compile_text_to_path};

/// How higher decimal places turn as the numeric value increases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RollingMode {
    /// Higher places turn only during the last unit before their carry.
    #[default]
    Odometer,
    /// Every wheel turns continuously at its decimal-place speed.
    Continuous,
}

/// Formatting and motion of a rolling numeric display, in scene units.
#[derive(Debug, Clone)]
pub struct RollingNumberOptions {
    pub decimals: usize,
    pub min_digits: usize,
    pub group_separator: String,
    pub decimal_separator: String,
    pub prefix: String,
    pub suffix: String,
    pub show_plus: bool,
    /// None inherits the scene's body font when the display is compiled.
    pub font_family: Option<String>,
    pub font_size: f64,
    /// Extra horizontal space between cells, in scene units.
    pub digit_spacing: f64,
    /// Cell height relative to digit ink height; also the distance between wheel glyphs.
    pub line_height: f64,
    pub mode: RollingMode,
    /// Increasing magnitudes move upwards when true, downwards otherwise.
    pub roll_up: bool,
}

impl Default for RollingNumberOptions {
    fn default() -> Self {
        Self {
            decimals: 0,
            min_digits: 1,
            group_separator: String::new(),
            decimal_separator: ".".into(),
            prefix: String::new(),
            suffix: String::new(),
            show_plus: false,
            font_family: None,
            font_size: 0.75,
            digit_spacing: 0.02,
            line_height: 1.25,
            mode: RollingMode::Odometer,
            roll_up: true,
        }
    }
}

impl RollingNumberOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.decimals > 6
            || !(1..=15).contains(&self.min_digits)
            || self.min_digits + self.decimals > 15
        {
            return Err("decimals must be 0..6, min_digits 1..15, and their sum at most 15".into());
        }
        if !self.font_size.is_finite()
            || self.font_size <= 0.0
            || !self.digit_spacing.is_finite()
            || self.digit_spacing < 0.0
            || !self.line_height.is_finite()
            || self.line_height < 1.0
        {
            return Err("font_size must be positive, digit_spacing non-negative, and line_height at least 1; all must be finite".into());
        }
        if self
            .font_family
            .as_ref()
            .is_some_and(|family| family.trim().is_empty())
            || self.decimal_separator.chars().count() != 1
            || self.group_separator.chars().count() > 1
            || self.prefix.len() + self.suffix.len() > 256
            || [
                &self.prefix,
                &self.suffix,
                &self.group_separator,
                &self.decimal_separator,
            ]
            .iter()
            .any(|text| text.chars().any(char::is_control))
        {
            return Err("use a font family, one decimal separator, at most one grouping separator, and short single-line affixes".into());
        }
        if !self.group_separator.is_empty() && self.group_separator == self.decimal_separator {
            return Err("group_separator and decimal_separator must differ".into());
        }
        Ok(())
    }

    pub fn validate_value(&self, value: f64) -> Result<(), String> {
        if !value.is_finite() || value.abs() * 10_f64.powi(self.decimals as i32) >= 1e15 {
            Err(
                "rolling values must be finite and have fewer than 10^15 smallest display units"
                    .into(),
            )
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
struct Glyph {
    path: BezPath,
    width: f64,
}

/// Cached font outlines and configuration attached to a reactive readout.
#[derive(Component, Debug, Clone)]
pub struct RollingNumber {
    pub options: RollingNumberOptions,
    glyphs: HashMap<char, Glyph>,
    digit_width: f64,
    digit_height: f64,
    baseline: f64,
    pub last_value: Option<f64>,
}

/// Returns the current digit and progress towards its successor.
fn wheel(units: f64, place: usize, mode: RollingMode) -> (u8, f64) {
    let divisor = 10_f64.powi(place as i32);
    let quotient = (units / divisor).floor();
    let phase = match mode {
        RollingMode::Continuous => units / divisor - quotient,
        RollingMode::Odometer => (units - quotient * divisor - (divisor - 1.0)).clamp(0.0, 1.0),
    };
    ((quotient % 10.0) as u8, phase)
}

impl RollingNumber {
    pub fn new(registry: &FontRegistry, options: RollingNumberOptions) -> Result<Self, String> {
        options.validate()?;
        let mut glyphs = HashMap::new();
        let mut digit_width: f64 = 0.0;
        let (digits, _) = compile_text_to_path(
            registry,
            "0123456789",
            options.font_family.as_deref().unwrap_or("sans-serif"),
            options.font_size,
        )
        .map_err(|error| error.to_string())?;
        let digit_bounds = digits.bounding_box();
        let digit_height = digit_bounds.height();
        let center_y = (digit_bounds.y0 + digit_bounds.y1) * 0.5;
        for ch in format!(
            "0123456789+-{}{}{}{}—",
            options.prefix, options.suffix, options.decimal_separator, options.group_separator
        )
        .chars()
        {
            if glyphs.contains_key(&ch) {
                continue;
            }
            let (mut path, _) = compile_text_to_path(
                registry,
                &ch.to_string(),
                options.font_family.as_deref().unwrap_or("sans-serif"),
                options.font_size,
            )
            .map_err(|error| error.to_string())?;
            let rect = path.bounding_box();
            let width = if ch.is_whitespace() {
                options.font_size * 0.33
            } else {
                rect.width()
            };
            path.apply_affine(Affine::translate((-rect.x0, -center_y)));
            if ch.is_ascii_digit() {
                digit_width = digit_width.max(width);
            }
            glyphs.insert(ch, Glyph { path, width });
        }
        Ok(Self {
            options,
            glyphs,
            digit_width,
            digit_height,
            baseline: -center_y,
            last_value: None,
        })
    }

    /// Typographic baseline of settled glyphs in the display's local coordinates.
    pub fn baseline(&self) -> f64 {
        self.baseline
    }

    /// Pure geometry at a value: identical for playback, reverse seeks, and export.
    /// Right anchored, with fixed-width digit cells.
    pub fn geometry(&self, value: f64) -> (BezPath, Bounds3D) {
        let o = &self.options;
        let height = self.digit_height * o.line_height;
        let mut path = BezPath::new();
        let mut x = 0.0;
        let mut append = |ch: char, digit: Option<(u8, f64)>| {
            let width = if digit.is_some() {
                self.digit_width
            } else {
                self.glyphs[&ch].width
            };
            if let Some((number, phase)) = digit {
                for (n, offset) in [(number, phase), ((number + 1) % 10, phase - 1.0)] {
                    if offset.abs() >= 1.0 {
                        continue;
                    }
                    let glyph = &self.glyphs[&char::from(b'0' + n)];
                    let dx = x + (width - glyph.width) * 0.5;
                    let dy = offset * height * if o.roll_up { 1.0 } else { -1.0 };
                    append_clipped(&mut path, &glyph.path, dx, dy, height * 0.5);
                }
            } else {
                let glyph = &self.glyphs[&ch];
                path.extend(
                    (Affine::translate((x, 0.0)) * &glyph.path)
                        .elements()
                        .iter()
                        .copied(),
                );
            }
            x += width + o.digit_spacing;
        };
        if o.validate_value(value).is_err() {
            append('—', None);
        } else {
            for ch in o.prefix.chars() {
                append(ch, None);
            }
            // The right anchor keeps digit positions stable across sign changes.
            let sign = if value < 0.0 {
                '-'
            } else if o.show_plus {
                '+'
            } else {
                ' '
            };
            if sign != ' ' {
                append(sign, None);
            }
            let units = value.abs() * 10_f64.powi(o.decimals as i32);
            // Remove representation noise at exact user-authored display boundaries.
            let units = if (units - units.round()).abs() < 1e-7 {
                units.round()
            } else {
                units
            };
            let integer = (units / 10_f64.powi(o.decimals as i32)).floor() as u64;
            let integer_digits = if integer == 0 {
                1
            } else {
                integer.ilog10() as usize + 1
            };
            let total = integer_digits.max(o.min_digits) + o.decimals;
            for place in (0..total).rev() {
                let state = wheel(units, place, o.mode);
                append('0', Some(state));
                if place == o.decimals && o.decimals > 0 {
                    append(o.decimal_separator.chars().next().unwrap(), None);
                } else if place > o.decimals && (place - o.decimals) % 3 == 0 {
                    if let Some(ch) = o.group_separator.chars().next() {
                        append(ch, None);
                    }
                }
            }
            for ch in o.suffix.chars() {
                append(ch, None);
            }
        }
        let width = (x - o.digit_spacing).max(0.0);
        path.apply_affine(Affine::translate((-width, 0.0)));
        let ink = path.bounding_box();
        (
            path,
            Bounds3D::new_2d(
                -width,
                (-height * 0.5).min(ink.y0),
                0.0,
                (height * 0.5).max(ink.y1),
            ),
        )
    }
}

// Clip each closed glyph contour to the wheel's horizontal window, retaining
// contour winding (including holes). Curves are flattened only at the window
// boundary; fully visible glyphs retain their original Bezier outlines.
fn append_clipped(out: &mut BezPath, source: &BezPath, dx: f64, dy: f64, half: f64) {
    let translated = Affine::translate((dx, dy)) * source;
    let rect = translated.bounding_box();
    if rect.y0 >= -half && rect.y1 <= half {
        out.extend(translated.elements().iter().copied());
        return;
    }
    let mut contour = Vec::new();
    let mut flush = |points: &mut Vec<Point>| {
        for (edge, above) in [(-half, true), (half, false)] {
            let input = std::mem::take(points);
            if input.is_empty() {
                break;
            }
            let mut previous = *input.last().unwrap();
            for current in input {
                let inside = |p: Point| if above { p.y >= edge } else { p.y <= edge };
                if inside(previous) != inside(current) {
                    let t = (edge - previous.y) / (current.y - previous.y);
                    points.push(Point::new(previous.x + t * (current.x - previous.x), edge));
                }
                if inside(current) {
                    points.push(current);
                }
                previous = current;
            }
        }
        if points.len() >= 3 {
            out.move_to(points[0]);
            for point in &points[1..] {
                out.line_to(*point);
            }
            out.close_path();
        }
        points.clear();
    };
    gaanim_core::kurbo::flatten(
        translated.elements().iter().copied(),
        half * 0.0002,
        |el| match el {
            PathEl::MoveTo(p) => {
                flush(&mut contour);
                contour.push(p);
            }
            PathEl::LineTo(p) => contour.push(p),
            PathEl::ClosePath => flush(&mut contour),
            _ => unreachable!(),
        },
    );
    flush(&mut contour);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rolling_carry_and_continuous_wheels() {
        assert_eq!(wheel(18.5, 1, RollingMode::Odometer), (1, 0.0));
        assert_eq!(wheel(19.5, 1, RollingMode::Odometer), (1, 0.5));
        assert_eq!(wheel(99.5, 2, RollingMode::Odometer), (0, 0.5));
        assert_eq!(wheel(100.0, 2, RollingMode::Odometer), (1, 0.0));
        assert_eq!(wheel(15.0, 1, RollingMode::Continuous), (1, 0.5));
    }
    #[test]
    fn rolling_validation_bounds_work() {
        let mut options = RollingNumberOptions::default();
        assert!(options.validate().is_ok());
        assert!(options.validate_value(f64::NAN).is_err());
        options.decimals = 2;
        assert!(options.validate_value(1e13).is_err());
        assert!(options.validate_value(-12.34).is_ok());
        options.min_digits = 15;
        assert!(options.validate().is_err());
    }
    #[test]
    fn rolling_geometry_is_stable_clipped_and_reversible() {
        let registry = FontRegistry::new();
        let options = RollingNumberOptions {
            min_digits: 3,
            decimals: 2,
            prefix: "$".into(),
            group_separator: ",".into(),
            ..Default::default()
        };
        let rolling = RollingNumber::new(&registry, options).unwrap();
        let (start, bounds) = rolling.geometry(99.99);
        let (middle, middle_bounds) = rolling.geometry(99.995);
        let (end, end_bounds) = rolling.geometry(100.0);
        assert!(!start.is_empty());
        assert_ne!(start, middle);
        assert_ne!(middle, end);
        assert_eq!(bounds, middle_bounds);
        assert_eq!(bounds, end_bounds);
        assert_eq!(start, rolling.geometry(99.99).0);
        let rect = middle.bounding_box();
        assert!(rect.y0 >= middle_bounds.min.y - 1e-9);
        assert!(rect.y1 <= middle_bounds.max.y + 1e-9);
        assert_ne!(rolling.geometry(-99.99).0, start);
        assert_eq!(
            rolling.geometry(f64::NAN).0,
            rolling.geometry(f64::INFINITY).0
        );
        assert_eq!(
            rolling.geometry(1.15).0,
            rolling.geometry(1.1500000000000001).0
        );
        let unpadded = RollingNumber::new(&registry, RollingNumberOptions::default()).unwrap();
        assert_eq!(
            unpadded.geometry(99.9999999999).0,
            unpadded.geometry(100.0).0
        );
        assert!(unpadded.geometry(999_999_999_999_999.0).1.min.x.is_finite());
    }

    #[test]
    fn rolling_update_restores_cached_path_and_respects_reveal() {
        use crate::{FloatSignal, ReactiveReadout, ScalarSource, reactive_readout_update_system};
        use bevy::prelude::{App, Update};
        use gaanim_scene::{LocalBounds, Path2D, PathSource, TextBaseline};
        use std::sync::Arc;
        let registry = FontRegistry::new();
        let rolling = RollingNumber::new(&registry, RollingNumberOptions::default()).unwrap();
        let mut app = App::new();
        app.insert_resource(registry);
        app.add_systems(Update, reactive_readout_update_system);
        let logical = gaanim_core::ObjectId::from_raw(42);
        let signal = app.world_mut().spawn(FloatSignal::new(9.5)).id();
        let empty = Arc::new(BezPath::new());
        let entity = app
            .world_mut()
            .spawn((
                rolling,
                ReactiveReadout {
                    source: ScalarSource::signal(logical),
                    parameters: vec![(logical, signal)],
                    format: String::new(),
                    prefix: String::new(),
                    suffix: String::new(),
                    invalid: "—".into(),
                    font_family: "sans-serif".into(),
                    font_size: 0.75,
                    last_text: String::new(),
                    last_path: empty.clone(),
                    last_bounds: Bounds3D::default(),
                },
                Path2D(empty.clone()),
                PathSource(empty.clone()),
                LocalBounds::default(),
                TextBaseline::default(),
                crate::writing::PathReveal(0.0),
            ))
            .id();
        app.update();
        assert!(app.world().get::<Path2D>(entity).unwrap().0.is_empty());
        let cached = app.world().get::<PathSource>(entity).unwrap().0.clone();
        assert!(!cached.is_empty());
        app.world_mut().get_mut::<PathSource>(entity).unwrap().0 = empty.clone();
        app.world_mut()
            .get_mut::<crate::writing::PathReveal>(entity)
            .unwrap()
            .0 = 1.0;
        app.update();
        assert_eq!(*app.world().get::<Path2D>(entity).unwrap().0, *cached);
        let expected_baseline = app.world().get::<RollingNumber>(entity).unwrap().baseline();
        assert!(expected_baseline.abs() > 1e-3);
        assert_eq!(
            app.world().get::<TextBaseline>(entity).unwrap().0,
            expected_baseline
        );
        app.world_mut()
            .get_mut::<FloatSignal>(signal)
            .unwrap()
            .value = 10.0;
        app.update();
        assert_ne!(*app.world().get::<Path2D>(entity).unwrap().0, *cached);
        app.world_mut()
            .get_mut::<FloatSignal>(signal)
            .unwrap()
            .value = 9.5;
        app.update();
        assert_eq!(*app.world().get::<Path2D>(entity).unwrap().0, *cached);
    }
    #[test]
    fn rolling_clip_preserves_window_and_hole_winding() {
        let mut source = BezPath::new();
        source.move_to((0.0, -2.0));
        source.line_to((4.0, -2.0));
        source.line_to((4.0, 2.0));
        source.line_to((0.0, 2.0));
        source.close_path();
        source.move_to((1.0, -2.0));
        source.line_to((1.0, 2.0));
        source.line_to((3.0, 2.0));
        source.line_to((3.0, -2.0));
        source.close_path();
        let mut output = BezPath::new();
        append_clipped(&mut output, &source, 0.0, 0.0, 1.0);
        assert_eq!(output.bounding_box().y0, -1.0);
        assert_eq!(output.bounding_box().y1, 1.0);
        assert!((output.area().abs() - 4.0).abs() < 1e-8);
    }
}
