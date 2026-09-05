//! Deterministic interpolation of native vector paints.
use gaanim_core::peniko::{Brush, Color, Gradient, GradientKind};

/// Validate authored vector paint data before it reaches the renderer.
pub fn validate_paint(paint: &Brush) -> Result<(), &'static str> {
    match paint {
        Brush::Solid(color) => {
            if color.components.iter().any(|value| !value.is_finite()) {
                return Err("paint color components must be finite");
            }
        }
        Brush::Gradient(gradient) => {
            if gradient.stops.is_empty()
                || gradient
                    .stops
                    .iter()
                    .any(|stop| !stop.offset.is_finite() || !(0.0..=1.0).contains(&stop.offset))
                || gradient
                    .stops
                    .windows(2)
                    .any(|pair| pair[0].offset > pair[1].offset)
            {
                return Err("gradient animation requires nonempty sorted finite stops in [0, 1]");
            }
            if gradient
                .stops
                .iter()
                .any(|stop| stop.color.components.iter().any(|value| !value.is_finite()))
            {
                return Err("gradient color components must be finite");
            }
            let valid = match gradient.kind {
                GradientKind::Linear(position) => {
                    position.start.is_finite()
                        && position.end.is_finite()
                        && position.start != position.end
                }
                GradientKind::Radial(position) => {
                    position.start_center.is_finite()
                        && position.end_center.is_finite()
                        && position.start_radius.is_finite()
                        && position.end_radius.is_finite()
                        && position.start_radius >= 0.0
                        && position.end_radius >= 0.0
                        && (position.start_center != position.end_center
                            || position.start_radius != position.end_radius)
                }
                GradientKind::Sweep(position) => {
                    position.center.is_finite()
                        && position.start_angle.is_finite()
                        && position.end_angle.is_finite()
                        && position.start_angle != position.end_angle
                }
            };
            if !valid {
                return Err("gradient geometry must be finite and nondegenerate");
            }
        }
        Brush::Image(image) => {
            if !image.sampler.alpha.is_finite() || image.sampler.alpha < 0.0 {
                return Err("image paint alpha must be finite and nonnegative");
            }
        }
    }
    Ok(())
}

/// Check compatibility before scheduling a paint animation.
pub fn validate_paint_transition(from: &Brush, to: &Brush) -> Result<(), &'static str> {
    validate_paint(from)?;
    validate_paint(to)?;
    match (from, to) {
        (Brush::Image(_), _) | (_, Brush::Image(_)) => Err("image paints cannot be interpolated"),
        (Brush::Gradient(a), Brush::Gradient(b)) => {
            if std::mem::discriminant(&a.kind) != std::mem::discriminant(&b.kind) {
                return Err("gradient animation requires gradients of the same kind");
            }
            if a.extend != b.extend
                || a.interpolation_cs != b.interpolation_cs
                || a.hue_direction != b.hue_direction
                || a.interpolation_alpha_space != b.interpolation_alpha_space
            {
                return Err("gradient animation requires matching extend and interpolation modes");
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Interpolate compatible paints, preserving exact authored endpoint brushes.
pub fn interpolate_paint(from: &Brush, to: &Brush, t: f64) -> Brush {
    if t <= 0.0 {
        return from.clone();
    }
    if t >= 1.0 {
        return to.clone();
    }
    match (from, to) {
        (Brush::Solid(a), Brush::Solid(b)) => {
            Brush::Solid(gaanim_core::interpolate_color(*a, *b, t))
        }
        (Brush::Solid(color), Brush::Gradient(g)) => {
            interpolate_paint(&solid_gradient(*color, g), to, t)
        }
        (Brush::Gradient(g), Brush::Solid(color)) => {
            interpolate_paint(from, &solid_gradient(*color, g), t)
        }
        (Brush::Gradient(a), Brush::Gradient(b)) => {
            let mut result = a.clone();
            let lerp = |a: f32, b: f32| a + (b - a) * t as f32;
            match (&mut result.kind, &b.kind) {
                (GradientKind::Linear(a), GradientKind::Linear(b)) => {
                    a.start = a.start.lerp(b.start, t);
                    a.end = a.end.lerp(b.end, t);
                }
                (GradientKind::Radial(a), GradientKind::Radial(b)) => {
                    a.start_center = a.start_center.lerp(b.start_center, t);
                    a.end_center = a.end_center.lerp(b.end_center, t);
                    a.start_radius = lerp(a.start_radius, b.start_radius);
                    a.end_radius = lerp(a.end_radius, b.end_radius);
                }
                (GradientKind::Sweep(a), GradientKind::Sweep(b)) => {
                    a.center = a.center.lerp(b.center, t);
                    a.start_angle = lerp(a.start_angle, b.start_angle);
                    a.end_angle = lerp(a.end_angle, b.end_angle);
                }
                _ => unreachable!("paint transition must be validated before scheduling"),
            }
            let mut offsets: Vec<f32> = a
                .stops
                .iter()
                .chain(b.stops.iter())
                .map(|s| s.offset)
                .collect();
            offsets.sort_by(f32::total_cmp);
            offsets.dedup();
            result.stops.clear();
            for offset in offsets {
                let hard_stop = a.stops.iter().filter(|stop| stop.offset == offset).count() > 1
                    || b.stops.iter().filter(|stop| stop.offset == offset).count() > 1;
                for left_side in [true, false] {
                    if left_side && !hard_stop {
                        continue;
                    }
                    let color = gaanim_core::interpolate_color(
                        sample(a, offset, left_side),
                        sample(b, offset, left_side),
                        t,
                    );
                    result.stops.push((offset, color).into());
                }
            }
            Brush::Gradient(result)
        }
        _ => unreachable!("paint transition must be validated before scheduling"),
    }
}

fn solid_gradient(color: Color, reference: &Gradient) -> Brush {
    let mut gradient = reference.clone();
    for stop in gradient.stops.iter_mut() {
        stop.color = color.into();
    }
    Brush::Gradient(gradient)
}

fn sample(gradient: &Gradient, offset: f32, left_side: bool) -> Color {
    let Some(first) = gradient.stops.first() else {
        return Color::TRANSPARENT;
    };
    let mut exact = gradient.stops.iter().filter(|stop| stop.offset == offset);
    if let Some(first) = exact.next() {
        return if left_side {
            first.color
        } else {
            exact.last().unwrap_or(first).color
        }
        .to_alpha_color();
    }
    if offset <= first.offset {
        return first.color.to_alpha_color();
    }
    for pair in gradient.stops.windows(2) {
        if offset < pair[1].offset {
            let t = (offset - pair[0].offset) / (pair[1].offset - pair[0].offset);
            let color = match gradient.interpolation_alpha_space {
                gaanim_core::peniko::InterpolationAlphaSpace::Premultiplied => pair[0]
                    .color
                    .interpolate(
                        pair[1].color,
                        gradient.interpolation_cs,
                        gradient.hue_direction,
                    )
                    .eval(t),
                gaanim_core::peniko::InterpolationAlphaSpace::Unpremultiplied => pair[0]
                    .color
                    .interpolate_unpremultiplied(
                        pair[1].color,
                        gradient.interpolation_cs,
                        gradient.hue_direction,
                    )
                    .eval(t),
            };
            return color.to_alpha_color();
        }
    }
    gradient.stops.last().unwrap().color.to_alpha_color()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn linear(stops: &[(f32, Color)]) -> Brush {
        Brush::Gradient(Gradient::new_linear((0., 0.), (10., 0.)).with_stops(stops))
    }
    #[test]
    fn gradients_union_stops_and_preserve_endpoints() {
        let a = linear(&[(0., Color::BLACK), (1., Color::WHITE)]);
        let b = linear(&[(0., Color::WHITE), (0.5, Color::BLACK), (1., Color::WHITE)]);
        assert!(validate_paint_transition(&a, &b).is_ok());
        assert_eq!(interpolate_paint(&a, &b, 0.), a);
        assert_eq!(interpolate_paint(&a, &b, 1.), b);
        let Brush::Gradient(mid) = interpolate_paint(&a, &b, 0.5) else {
            panic!()
        };
        assert_eq!(mid.stops.len(), 3);
        assert_eq!(mid.stops[1].offset, 0.5);
    }
    #[test]
    fn solid_gradient_is_continuous_and_incompatible_kinds_fail() {
        let a = Brush::Solid(Color::BLACK);
        let b = linear(&[(0., Color::WHITE), (1., Color::BLACK)]);
        assert!(validate_paint_transition(&a, &b).is_ok());
        assert!(matches!(interpolate_paint(&a, &b, 0.5), Brush::Gradient(_)));
        let radial = Brush::Gradient(
            Gradient::new_radial((0., 0.), 2.).with_stops([Color::BLACK, Color::WHITE]),
        );
        assert!(validate_paint_transition(&b, &radial).is_err());
    }

    #[test]
    fn gradient_union_preserves_hard_stops() {
        let a = linear(&[
            (0., Color::BLACK),
            (0.5, Color::BLACK),
            (0.5, Color::WHITE),
            (1., Color::WHITE),
        ]);
        let b = linear(&[(0., Color::BLACK), (1., Color::WHITE)]);
        let Brush::Gradient(mid) = interpolate_paint(&a, &b, 0.5) else {
            panic!()
        };
        let stops: Vec<_> = mid.stops.iter().filter(|stop| stop.offset == 0.5).collect();
        assert_eq!(stops.len(), 2);
        assert_ne!(stops[0].color, stops[1].color);
    }

    #[test]
    fn gradient_geometry_and_colors_interpolate() {
        let stops = [Color::BLACK, Color::WHITE];
        for (a, b) in [
            (
                Gradient::new_linear((0., 0.), (10., 0.)),
                Gradient::new_linear((4., 6.), (20., 0.)),
            ),
            (
                Gradient::new_radial((0., 0.), 2.),
                Gradient::new_radial((4., 6.), 6.),
            ),
            (
                Gradient::new_sweep((0., 0.), 0., 1.),
                Gradient::new_sweep((4., 6.), 2., 3.),
            ),
        ] {
            let from = Brush::Gradient(a.with_stops(stops));
            let to = Brush::Gradient(b.with_stops([Color::WHITE, Color::BLACK]));
            let Brush::Gradient(mid) = interpolate_paint(&from, &to, 0.5) else {
                panic!()
            };
            match mid.kind {
                GradientKind::Linear(position) => {
                    assert_eq!(position.start, (2., 3.).into());
                    assert_eq!(position.end, (15., 0.).into());
                }
                GradientKind::Radial(position) => {
                    assert_eq!(position.end_center, (2., 3.).into());
                    assert_eq!(position.end_radius, 4.);
                }
                GradientKind::Sweep(position) => {
                    assert_eq!(position.center, (2., 3.).into());
                    assert_eq!(position.start_angle, 1.);
                    assert_eq!(position.end_angle, 2.);
                }
            }
            let color: Color = mid.stops[0].color.to_alpha_color();
            assert_eq!(color.to_rgba8().r, 127);
        }
    }

    #[test]
    fn gradient_modes_must_match() {
        let a = linear(&[(0., Color::BLACK), (1., Color::WHITE)]);
        let Brush::Gradient(mut b) = a.clone() else {
            panic!()
        };
        b.extend = gaanim_core::peniko::Extend::Repeat;
        assert!(validate_paint_transition(&a, &Brush::Gradient(b)).is_err());
    }

    #[test]
    fn paint_rejects_nonfinite_colors_and_geometry() {
        assert!(validate_paint(&Brush::Solid(Color::new([f32::NAN, 0., 0., 1.]))).is_err());
        let invalid =
            Gradient::new_linear((f64::NAN, 0.), (1., 1.)).with_stops([Color::BLACK, Color::WHITE]);
        assert!(validate_paint(&Brush::Gradient(invalid)).is_err());
    }
}
