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
}
