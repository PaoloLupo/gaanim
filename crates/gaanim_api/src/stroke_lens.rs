//! Interpolation of stroke geometry that has no dedicated timeline lens.

use bevy::prelude::{Entity, World};
use gaanim_core::kurbo::{BezPath, PathEl, Point};
use gaanim_scene::{LocalBounds, Path2D, PathSource, StrokeBrush};
use std::sync::Arc;

/// Lens between two dash offsets of a stroke, in scene units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DashOffsetLens {
    pub from: f64,
    pub to: f64,
}

impl DashOffsetLens {
    pub fn at(&self, t: f64) -> f64 {
        self.from + (self.to - self.from) * t
    }
}

impl gaanim_animation::AnimatableLens for DashOffsetLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        if let Some(mut stroke) = world.get_mut::<StrokeBrush>(entity) {
            stroke.style.dash_offset = self.at(t);
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(*self)
    }

    fn type_name(&self) -> &'static str {
        "DashOffset"
    }

    fn history_free(&self) -> bool {
        true
    }
}

/// `path` with its vertices, the points of its `MoveTo` and `LineTo`
/// elements in order, moved to `points`; `None` unless the path is made of
/// straight segments only and has exactly one vertex per point.
pub fn with_vertices(path: &BezPath, points: &[(f64, f64)]) -> Option<BezPath> {
    let mut next = points.iter().map(|&(x, y)| Point::new(x, y));
    let mut elements = Vec::with_capacity(path.elements().len());
    for element in path.elements() {
        elements.push(match element {
            PathEl::MoveTo(_) => PathEl::MoveTo(next.next()?),
            PathEl::LineTo(_) => PathEl::LineTo(next.next()?),
            PathEl::ClosePath => PathEl::ClosePath,
            PathEl::QuadTo(..) | PathEl::CurveTo(..) => return None,
        });
    }
    next.next().is_none().then(|| BezPath::from_vec(elements))
}

/// Lens that moves each vertex of a straight-segment path to its position
/// in `to`, without resampling: both paths have the same elements.
#[derive(Debug, Clone, PartialEq)]
pub struct PathPointsLens {
    pub from: Arc<BezPath>,
    pub to: Arc<BezPath>,
}

impl PathPointsLens {
    /// The outline `t` of the way from `from` to `to`.
    pub fn at(&self, t: f64) -> BezPath {
        if t == 1.0 {
            return (*self.to).clone();
        }
        let lerp = |a: Point, b: Point| a.lerp(b, t);
        BezPath::from_vec(
            self.from
                .elements()
                .iter()
                .zip(self.to.elements())
                .map(|(from, to)| match (from, to) {
                    (PathEl::MoveTo(a), PathEl::MoveTo(b)) => PathEl::MoveTo(lerp(*a, *b)),
                    (PathEl::LineTo(a), PathEl::LineTo(b)) => PathEl::LineTo(lerp(*a, *b)),
                    _ => *to,
                })
                .collect(),
        )
    }
}

impl gaanim_animation::AnimatableLens for PathPointsLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let path = Arc::new(self.at(t));
        // Culling and layout read the outline's current extent.
        let rect = gaanim_core::kurbo::Shape::bounding_box(&*path);
        if let Some(mut bounds) = world.get_mut::<LocalBounds>(entity) {
            bounds.0 = gaanim_math::Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1);
        }
        if let Some(mut current) = world.get_mut::<Path2D>(entity) {
            current.0 = path.clone();
        }
        // Keep the stroke clipping source in lockstep, as a path morph does.
        if let Some(mut source) = world.get_mut::<PathSource>(entity) {
            source.0 = path;
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "PathPoints"
    }

    fn history_free(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_animation::AnimatableLens;

    #[test]
    fn dash_offset_lens_writes_the_stroke() {
        let mut world = World::new();
        let entity = world
            .spawn(StrokeBrush::new(gaanim_core::peniko::Color::WHITE, 0.1))
            .id();
        let lens = DashOffsetLens { from: 0.5, to: 2.5 };
        lens.interpolate(&mut world, entity, 0.25);
        assert_eq!(
            world.get::<StrokeBrush>(entity).unwrap().style.dash_offset,
            1.0
        );
        assert_eq!(lens.at(1.0), 2.5);
    }

    #[test]
    fn vertices_move_straight_to_their_targets() {
        let mut square = BezPath::new();
        square.move_to((0.0, 0.0));
        square.line_to((2.0, 0.0));
        square.line_to((2.0, 2.0));
        square.line_to((0.0, 2.0));
        square.close_path();
        let diamond =
            with_vertices(&square, &[(1.0, -1.0), (3.0, 1.0), (1.0, 3.0), (-1.0, 1.0)]).unwrap();
        let lens = PathPointsLens {
            from: Arc::new(square.clone()),
            to: Arc::new(diamond.clone()),
        };
        let half = lens.at(0.5);
        assert_eq!(half.elements().len(), square.elements().len());
        assert_eq!(half.elements()[1], PathEl::LineTo(Point::new(2.5, 0.5)));
        assert_eq!(half.elements()[4], PathEl::ClosePath);
        assert_eq!(lens.at(0.0), square);
        assert_eq!(lens.at(1.0), diamond);

        assert!(with_vertices(&square, &[(0.0, 0.0)]).is_none());
        assert!(with_vertices(&square, &[(0.0, 0.0); 5]).is_none());
        let mut curve = BezPath::new();
        curve.move_to((0.0, 0.0));
        curve.quad_to((1.0, 1.0), (2.0, 0.0));
        assert!(with_vertices(&curve, &[(0.0, 0.0), (2.0, 0.0)]).is_none());

        let mut world = World::new();
        let entity = world
            .spawn((
                Path2D(Arc::new(square.clone())),
                PathSource(Arc::new(square)),
            ))
            .id();
        gaanim_animation::AnimatableLens::interpolate(&lens, &mut world, entity, 0.5);
        assert_eq!(*world.get::<Path2D>(entity).unwrap().0, half);
        assert_eq!(*world.get::<PathSource>(entity).unwrap().0, half);
    }
}
