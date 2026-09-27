//! Stroke geometry in a Cartesian domain view, independent of its zoom.

use gaanim_core::{
    glam::DVec3,
    kurbo::{Affine, BezPath, Stroke},
    peniko::Brush,
};
use gaanim_math::SpatialTransform;
use gaanim_scene::{CoordinateViewRole, prelude::Entity};

fn linear(transform: &SpatialTransform) -> Affine {
    let [a, b, c, d, _, _] = transform.to_affine_2d().as_coeffs();
    Affine::new([a, b, c, d, 0.0, 0.0])
}

/// Factor the domain scale out of the stroke pen, preserving authored scales
/// and rotations below/above the view. Translation never invalidates a fragment.
pub(crate) fn view_stroke_transform(
    mut entity: Entity,
    mut lookup: impl FnMut(
        Entity,
    )
        -> Option<(SpatialTransform, Option<Entity>, Option<CoordinateViewRole>)>,
) -> Option<Affine> {
    let mut geometry = Affine::IDENTITY;
    let mut pen = Affine::IDENTITY;
    let mut has_zoom = false;
    let mut correction = None;
    while let Some((local, parent, role)) = lookup(entity) {
        // Text already omits view scale during hierarchy propagation.
        if role == Some(CoordinateViewRole::Label) {
            return None;
        }
        geometry = linear(&local) * geometry;
        let mut pen_local = local;
        if role == Some(CoordinateViewRole::View) {
            has_zoom |= local.scale != DVec3::ONE;
            pen_local.scale = DVec3::ONE;
        }
        pen = linear(&pen_local) * pen;
        if role == Some(CoordinateViewRole::View) && has_zoom {
            // Compute at the view: enclosing layout transforms cancel, so
            // moving/scaling/rotating the whole chart can reuse its fragment.
            let mapped = pen.inverse() * geometry;
            correction = (mapped.as_coeffs().iter().all(|v| v.is_finite())
                && mapped.inverse().as_coeffs().iter().all(|v| v.is_finite()))
            .then_some(mapped);
        }
        let Some(parent) = parent else { break };
        entity = parent;
    }
    correction
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
    use gaanim_scene::prelude::{ChildOf, World};

    #[test]
    fn coordinate_view_preserves_authored_pen_transforms_and_brush_coordinates() {
        let mut world = World::new();
        let root_local = SpatialTransform::new_2d(4.0, -2.0)
            .with_rotation_2d(0.3)
            .scale_uniform(1.5);
        let root = world.spawn(root_local).id();
        let view_local = SpatialTransform::new_2d(2.0, 1.0).with_scale_2d(3.0, 0.5);
        let view = world
            .spawn((view_local, CoordinateViewRole::View, ChildOf(root)))
            .id();
        let leaf_local = SpatialTransform::new_2d(1.0, 2.0)
            .with_rotation_2d(0.7)
            .with_scale_2d(0.8, 1.2);
        let leaf = world.spawn((leaf_local, ChildOf(view))).id();
        let resolve = |world: &World| {
            view_stroke_transform(leaf, |entity| {
                Some((
                    *world.get::<SpatialTransform>(entity)?,
                    world.get::<ChildOf>(entity).map(|parent| parent.parent()),
                    world.get::<CoordinateViewRole>(entity).copied(),
                ))
            })
        };
        let correction = resolve(&world).unwrap();
        let full =
            root_local.to_affine_2d() * view_local.to_affine_2d() * leaf_local.to_affine_2d();
        let expected_pen = linear(&root_local) * linear(&leaf_local);
        let actual_pen = full * correction.inverse();
        for (actual, expected) in actual_pen.as_coeffs()[..4]
            .iter()
            .zip(&expected_pen.as_coeffs()[..4])
        {
            assert!((actual - expected).abs() < 1e-9);
        }
        let point = gaanim_core::kurbo::Point::new(2.0, 3.0);
        assert!((actual_pen * (correction * point)).distance(full * point) < 1e-9);

        // Enclosing chart transforms and panning do not change cached geometry.
        world
            .entity_mut(root)
            .insert(root_local.with_rotation_2d(-0.4).scale_uniform(2.0));
        world
            .entity_mut(view)
            .insert(view_local.shift_2d(10.0, 20.0));
        assert_eq!(resolve(&world), Some(correction));

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

        world.entity_mut(view).insert(SpatialTransform::default());
        assert_eq!(resolve(&world), None);
        world.entity_mut(view).insert(view_local);
        world.entity_mut(leaf).insert(CoordinateViewRole::Label);
        assert_eq!(resolve(&world), None, "text already has scale compensation");
        world.entity_mut(leaf).remove::<CoordinateViewRole>();
        world
            .entity_mut(leaf)
            .insert(leaf_local.with_scale_2d(0.0, 0.0));
        assert_eq!(
            resolve(&world),
            None,
            "a collapsed object must not encode NaNs"
        );
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
