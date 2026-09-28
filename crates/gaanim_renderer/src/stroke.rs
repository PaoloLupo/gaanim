//! Stroke geometry in scene units, independent of a drawable's scale.

use gaanim_core::{
    kurbo::{Affine, BezPath, Stroke},
    peniko::Brush,
};

/// Correction that keeps a stroke pen in scene units under `world`, the
/// transform that places the drawable's fragment.
///
/// Returns the stretch `S` of the polar decomposition `world = Q S` (`Q`
/// orthogonal, `S` symmetric positive definite). The fragment strokes
/// `S * path` with the authored pen and returns through `S⁻¹`, so the
/// placed stroke is `Q` applied to a scene-unit pen: no scale or skew of
/// the drawable, its groups or a coordinate view widens or distorts it.
/// Rotations and translations leave `S` unchanged and reuse the fragment.
/// `None` when no correction is needed or `world` is singular.
pub(crate) fn scene_unit_stroke_transform(world: Affine) -> Option<Affine> {
    let [a, b, c, d, _, _] = world.as_coeffs();
    let determinant = (a * d - b * c).abs();
    // sqrt(P) = (P + sqrt(det P) I) / sqrt(tr P + 2 sqrt(det P)), P = AᵀA.
    let (p, q, r) = (a * a + b * b, a * c + b * d, c * c + d * d);
    let norm = (p + r + 2.0 * determinant).sqrt();
    if !determinant.is_finite() || determinant <= 1.0e-12 || !norm.is_finite() {
        return None;
    }
    let stretch = [
        (p + determinant) / norm,
        q / norm,
        q / norm,
        (r + determinant) / norm,
    ];
    let identity = [1.0, 0.0, 0.0, 1.0];
    if stretch
        .iter()
        .zip(identity)
        .all(|(value, unit)| (value - unit).abs() <= 1.0e-9)
    {
        return None;
    }
    Some(Affine::new([
        stretch[0], stretch[1], stretch[2], stretch[3], 0.0, 0.0,
    ]))
}

pub(crate) fn draw_stroke(
    scene: &mut vello::Scene,
    style: &Stroke,
    origin: Affine,
    brush: &Brush,
    view: Option<Affine>,
    path: &BezPath,
) {
    if let Some(view) = view {
        // Stroke the zoomed centerline, then return to fragment-local space.
        // The outer object transform still controls fills, clips and placement.
        // Keep gradient/image brushes in their original local coordinate frame.
        let mapped = view * path;
        scene.stroke(style, view.inverse() * origin, brush, Some(view), &mapped);
    } else {
        scene.stroke(style, origin, brush, None, path);
    }
}

/// Filled outline of `path` stroked with a half width of `half_width` times
/// `profile`'s factor at each point's share of its sub-path's arc length.
///
/// Each sub-path is flattened and offset along mitred normals; an open
/// sub-path becomes one closed contour, a closed one two opposite contours,
/// so the band fills with the non-zero rule.
pub(crate) fn profiled_outline(
    path: &BezPath,
    half_width: f64,
    profile: &crate::effects::StrokeProfile,
) -> BezPath {
    let mut outline = BezPath::new();
    let tolerance = (half_width * 0.05).clamp(1e-4, 0.02);
    let mut polylines: Vec<(Vec<gaanim_core::kurbo::Point>, bool)> = Vec::new();
    gaanim_core::kurbo::flatten(path, tolerance, |element| match element {
        gaanim_core::kurbo::PathEl::MoveTo(point) => polylines.push((vec![point], false)),
        gaanim_core::kurbo::PathEl::LineTo(point) => {
            if let Some((points, _)) = polylines.last_mut()
                && points.last() != Some(&point)
            {
                points.push(point);
            }
        }
        gaanim_core::kurbo::PathEl::ClosePath => {
            if let Some((points, closed)) = polylines.last_mut() {
                if points.len() > 1 && points.first() == points.last() {
                    points.pop();
                }
                *closed = true;
            }
        }
        _ => {}
    });
    for (points, closed) in polylines {
        if points.len() < 2 {
            continue;
        }
        let points = subdivided(&points, closed);
        let count = points.len();
        let mut lengths = Vec::with_capacity(count);
        let mut total = 0.0;
        for index in 0..count {
            lengths.push(total);
            if index + 1 < count {
                total += (points[index + 1] - points[index]).hypot();
            }
        }
        if closed {
            total += (points[0] - points[count - 1]).hypot();
        }
        if total <= f64::EPSILON {
            continue;
        }
        let direction = |from: gaanim_core::kurbo::Point, to: gaanim_core::kurbo::Point| {
            let delta = to - from;
            let length = delta.hypot();
            (length > f64::EPSILON).then(|| delta / length)
        };
        let mut left = Vec::with_capacity(count);
        let mut right = Vec::with_capacity(count);
        for index in 0..count {
            let previous = if index > 0 {
                Some(points[index - 1])
            } else if closed {
                Some(points[count - 1])
            } else {
                None
            };
            let next = if index + 1 < count {
                Some(points[index + 1])
            } else if closed {
                Some(points[0])
            } else {
                None
            };
            let incoming = previous.and_then(|previous| direction(previous, points[index]));
            let outgoing = next.and_then(|next| direction(points[index], next));
            let (normal, scale) = match (incoming, outgoing) {
                (Some(a), Some(b)) => {
                    let na = gaanim_core::kurbo::Vec2::new(-a.y, a.x);
                    let nb = gaanim_core::kurbo::Vec2::new(-b.y, b.x);
                    let sum = na + nb;
                    if sum.hypot() <= 1e-9 {
                        (na, 1.0)
                    } else {
                        let miter = sum / sum.hypot();
                        // Keep the band's width across corners, up to a miter limit.
                        (miter, 1.0 / miter.dot(na).max(0.25))
                    }
                }
                (Some(a), None) | (None, Some(a)) => {
                    (gaanim_core::kurbo::Vec2::new(-a.y, a.x), 1.0)
                }
                (None, None) => continue,
            };
            let width = half_width * profile.factor_at(lengths[index] / total).max(0.0) * scale;
            left.push(points[index] + normal * width);
            right.push(points[index] - normal * width);
        }
        if closed {
            for contour in [left, right.into_iter().rev().collect()] {
                let mut contour: Vec<_> = contour;
                if let Some(first) = contour.first().copied() {
                    outline.move_to(first);
                    for point in contour.drain(1..) {
                        outline.line_to(point);
                    }
                    outline.close_path();
                }
            }
        } else if let Some(first) = left.first().copied() {
            outline.move_to(first);
            for point in left.into_iter().skip(1).chain(right.into_iter().rev()) {
                outline.line_to(point);
            }
            outline.close_path();
        }
    }
    outline
}

/// Samples along a flattened sub-path this many times at least, so the
/// profile shapes long straight segments too.
const PROFILE_SAMPLES: f64 = 96.0;

/// `points` with long segments split so none exceeds 1/[`PROFILE_SAMPLES`]
/// of the sub-path's length.
fn subdivided(
    points: &[gaanim_core::kurbo::Point],
    closed: bool,
) -> Vec<gaanim_core::kurbo::Point> {
    let segments = points.len() - 1 + usize::from(closed);
    let segment = |index: usize| (points[index], points[(index + 1) % points.len()]);
    let total: f64 = (0..segments)
        .map(|index| {
            let (a, b) = segment(index);
            (b - a).hypot()
        })
        .sum();
    let step = total / PROFILE_SAMPLES;
    let mut out = Vec::with_capacity(points.len() + PROFILE_SAMPLES as usize);
    for index in 0..segments {
        let (a, b) = segment(index);
        out.push(a);
        let pieces = if step > 0.0 {
            ((b - a).hypot() / step).ceil() as usize
        } else {
            1
        };
        for piece in 1..pieces {
            out.push(a.lerp(b, piece as f64 / pieces as f64));
        }
    }
    if !closed && let Some(last) = points.last() {
        out.push(*last);
    }
    out
}

/// Draw `path` with `style`'s width shaped by `profile`, filled with `brush`.
pub(crate) fn draw_profiled_stroke(
    scene: &mut vello::Scene,
    style: &Stroke,
    brush: &Brush,
    view: Option<Affine>,
    path: &BezPath,
    profile: &crate::effects::StrokeProfile,
) {
    let half_width = style.width.abs() * 0.5;
    match view {
        Some(view) => {
            let outline = profiled_outline(&(view * path), half_width, profile);
            scene.fill(
                gaanim_core::peniko::Fill::NonZero,
                view.inverse(),
                brush,
                Some(view),
                &outline,
            );
        }
        None => {
            let outline = profiled_outline(path, half_width, profile);
            scene.fill(
                gaanim_core::peniko::Fill::NonZero,
                Affine::IDENTITY,
                brush,
                None,
                &outline,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::kurbo::Shape;

    /// Whether `affine`'s linear part is orthogonal: it keeps a round pen
    /// round and its width unchanged.
    fn is_orthogonal(affine: Affine) -> bool {
        let [a, b, c, d, _, _] = affine.as_coeffs();
        (a * a + b * b - 1.0).abs() < 1e-9
            && (c * c + d * d - 1.0).abs() < 1e-9
            && (a * c + b * d).abs() < 1e-9
    }

    #[test]
    fn strokes_keep_a_scene_unit_pen_under_any_scale_or_skew() {
        let placements = [
            Affine::scale_non_uniform(5.5, 2.8),
            Affine::translate((3.0, -1.0)) * Affine::rotate(0.7) * Affine::scale(2.0),
            Affine::rotate(-0.4) * Affine::skew(0.6, 0.0) * Affine::scale_non_uniform(0.3, 1.7),
            // A mirrored placement, like the y-up flip of a chart.
            Affine::scale_non_uniform(2.0, -3.0),
        ];
        for world in placements {
            let correction = scene_unit_stroke_transform(world).expect("scaled placement");
            // The pen is drawn through world * S⁻¹: only a rotation or mirror.
            assert!(is_orthogonal(world * correction.inverse()), "{world:?}");
            let point = gaanim_core::kurbo::Point::new(2.0, 3.0);
            let placed = world * correction.inverse() * (correction * point);
            assert!(placed.distance(world * point) < 1e-9);
        }

        // Rotating or moving a scaled drawable reuses its stroke geometry.
        let scaled = Affine::scale_non_uniform(5.5, 2.8);
        let turned = Affine::translate((4.0, 2.0)) * Affine::rotate(1.1) * scaled;
        let (a, b) = (
            scene_unit_stroke_transform(scaled).unwrap(),
            scene_unit_stroke_transform(turned).unwrap(),
        );
        for (a, b) in a.as_coeffs().into_iter().zip(b.as_coeffs()) {
            assert!((a - b).abs() < 1e-9);
        }

        // No correction without scale, and none that encodes NaNs.
        for world in [
            Affine::IDENTITY,
            Affine::translate((2.0, 1.0)) * Affine::rotate(0.3),
            Affine::scale(0.0),
            Affine::scale_non_uniform(1.0, 0.0),
        ] {
            assert_eq!(scene_unit_stroke_transform(world), None, "{world:?}");
        }
    }

    #[test]
    fn scene_unit_strokes_keep_brush_coordinates() {
        let correction = scene_unit_stroke_transform(Affine::scale_non_uniform(3.0, 0.5)).unwrap();
        let path = gaanim_core::kurbo::Line::new((0.0, 0.0), (2.0, 3.0)).to_path(0.01);
        let brush = Brush::Gradient(
            gaanim_core::peniko::Gradient::new_linear((0.0, 0.0), (2.0, 3.0)).with_stops([
                gaanim_core::peniko::Color::BLACK,
                gaanim_core::peniko::Color::WHITE,
            ]),
        );
        let mut scene = vello::Scene::new();
        draw_stroke(
            &mut scene,
            &Stroke::new(0.04),
            Affine::IDENTITY,
            &brush,
            Some(correction),
            &path,
        );
        let brush_transform = scene.encoding().transforms.last().unwrap().to_kurbo();
        for (actual, expected) in brush_transform
            .as_coeffs()
            .into_iter()
            .zip(Affine::IDENTITY.as_coeffs())
        {
            assert!((actual - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn profiled_outlines_taper_open_and_closed_paths() {
        let profile =
            crate::effects::StrokeProfile(vec![(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)].into());
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((4.0, 0.0));
        let outline = profiled_outline(&line, 0.5, &profile);
        let bounds = outline.bounding_box();
        assert!((bounds.x0 - 0.0).abs() < 1e-9 && (bounds.x1 - 4.0).abs() < 1e-9);
        // Widest at the middle, zero at both ends.
        assert!((bounds.y1 - 0.5).abs() < 0.05 && (bounds.y0 + 0.5).abs() < 0.05);
        assert!(outline.contains(gaanim_core::kurbo::Point::new(2.0, 0.4)));
        assert!(!outline.contains(gaanim_core::kurbo::Point::new(0.2, 0.2)));

        let square = gaanim_core::kurbo::Rect::new(0.0, 0.0, 2.0, 2.0).to_path(0.1);
        let uniform = crate::effects::StrokeProfile(vec![(0.0, 1.0)].into());
        let band = profiled_outline(&square, 0.1, &uniform);
        assert!(band.contains(gaanim_core::kurbo::Point::new(1.0, 0.05)));
        assert!(!band.contains(gaanim_core::kurbo::Point::new(1.0, 1.0)));
        assert!(
            profiled_outline(&BezPath::new(), 0.1, &uniform)
                .elements()
                .is_empty()
        );
    }
}
