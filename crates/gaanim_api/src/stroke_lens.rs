//! Interpolation of stroke geometry that has no dedicated timeline lens.

use bevy::prelude::{Entity, World};
use gaanim_core::kurbo::{BezPath, PathEl, Point, Vec2};
use gaanim_scene::{LocalBounds, Path2D, PathSource, StrokeBrush};
use std::f64::consts::PI;
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

/// How `animate.points` carries the vertices of a polygon or polyline to
/// their new positions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum VertexPath {
    /// Each vertex travels in a straight line.
    #[default]
    Linear,
    /// The outline turns, scales evenly and moves as one piece, by the turn,
    /// scale and move that best carry the old vertices onto the new ones;
    /// what that leaves over travels straight, so a target that is the
    /// shape turned, scaled and moved keeps its form all the way. `about`,
    /// in the coordinates the shape was declared in, travels straight while
    /// the outline turns around it; `None` takes the vertices' center.
    Rigid { about: Option<Point> },
}

/// The vertices of `path`, the points of its `MoveTo` and `LineTo` elements
/// in order; `None` when it has curves.
fn vertices(path: &BezPath) -> Option<Vec<Point>> {
    path.elements()
        .iter()
        .filter_map(|element| match element {
            PathEl::MoveTo(point) | PathEl::LineTo(point) => Some(Some(*point)),
            PathEl::ClosePath => None,
            PathEl::QuadTo(..) | PathEl::CurveTo(..) => Some(None),
        })
        .collect()
}

/// `path` with `points` as its vertices. With as many points as vertices it
/// keeps the elements of `path`; with another number, a path of one open or
/// closed outline becomes the same outline through `points`. `None` when
/// the path has curves, when `points` is empty, or when a path of several
/// outlines would change its number of vertices.
pub fn with_vertices(path: &BezPath, points: &[Point]) -> Option<BezPath> {
    let current = vertices(path)?;
    let (&first, rest) = points.split_first()?;
    if current.len() != points.len() {
        let mut moves = path
            .elements()
            .iter()
            .filter(|element| matches!(element, PathEl::MoveTo(_)));
        if moves.next().is_none() || moves.next().is_some() {
            return None;
        }
        let mut outline = BezPath::new();
        outline.move_to(first);
        rest.iter().for_each(|&point| outline.line_to(point));
        if path.elements().last() == Some(&PathEl::ClosePath) {
            outline.close_path();
        }
        return Some(outline);
    }
    let mut next = points.iter().copied();
    Some(BezPath::from_vec(
        path.elements()
            .iter()
            .map(|element| match element {
                PathEl::MoveTo(_) => PathEl::MoveTo(next.next().unwrap_or(first)),
                PathEl::LineTo(_) => PathEl::LineTo(next.next().unwrap_or(first)),
                other => *other,
            })
            .collect(),
    ))
}

/// Where each vertex starts and ends when `start` becomes `end`: vertex `i`
/// of the longer list pairs with vertex `i * shorter / longer` of the
/// shorter one, so the outline with fewer corners repeats some of them,
/// spread evenly, instead of being resampled.
fn paired_vertices(start: &[Point], end: &[Point]) -> Vec<(Point, Point)> {
    let count = start.len().max(end.len());
    (0..count)
        .map(|index| {
            (
                start[index * start.len() / count],
                end[index * end.len() / count],
            )
        })
        .collect()
}

/// The turn, even scale and move that best carry the start of each vertex
/// onto its end, spread over the animation around a pivot.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RigidMotion {
    /// Where the pivot starts and where the motion takes it.
    pivot: (Point, Point),
    /// Turn in radians, in (-π, π]: a half turn goes counterclockwise.
    angle: f64,
    scale: f64,
}

impl RigidMotion {
    /// The similarity that minimizes the squared distances from the turned,
    /// scaled and moved starts to the ends: in complex numbers, the factor
    /// `Σ conj(a - ā)(b - b̄) / Σ |a - ā|²` about the centers `ā` and `b̄`.
    fn fit(pairs: &[(Point, Point)], about: Option<Point>) -> Self {
        let count = pairs.len().max(1) as f64;
        let (start_sum, end_sum) = pairs
            .iter()
            .fold((Vec2::ZERO, Vec2::ZERO), |(starts, ends), (start, end)| {
                (starts + start.to_vec2(), ends + end.to_vec2())
            });
        let (start_center, end_center) = (start_sum / count, end_sum / count);
        let (mut real, mut imaginary, mut spread) = (0.0, 0.0, 0.0);
        for (start, end) in pairs {
            let (a, b) = (start.to_vec2() - start_center, end.to_vec2() - end_center);
            real += a.dot(b);
            imaginary += a.cross(b);
            spread += a.hypot2();
        }
        let (angle, scale) = if spread > 0.0 {
            let angle = imaginary.atan2(real);
            // Rounding picks the side of an exact half turn; take one.
            let angle = if angle <= -PI + 1e-9 { PI } else { angle };
            (angle, real.hypot(imaginary) / spread)
        } else {
            (0.0, 1.0)
        };
        let start = about.unwrap_or(start_center.to_point());
        let end = end_center + turned(start.to_vec2() - start_center, angle) * scale;
        Self {
            pivot: (start, end.to_point()),
            angle,
            scale,
        }
    }

    /// Where the motion has carried `start` at `t`.
    fn carried(&self, start: Point, t: f64) -> Point {
        let pivot = self.pivot.0.lerp(self.pivot.1, t);
        let scale = 1.0 + (self.scale - 1.0) * t;
        pivot + turned(start - self.pivot.0, self.angle * t) * scale
    }

    /// Where the vertex from `start` to `end` is at `t`: carried by the
    /// motion, plus that share of what the motion leaves over.
    fn at(&self, start: Point, end: Point, t: f64) -> Point {
        self.carried(start, t) + (end - self.carried(start, 1.0)) * t
    }
}

/// `vector` turned counterclockwise by `angle` radians.
fn turned(vector: Vec2, angle: f64) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(
        vector.x * cos - vector.y * sin,
        vector.x * sin + vector.y * cos,
    )
}

/// Lens that carries each vertex of a straight-segment path to its new
/// position, without resampling.
#[derive(Debug, Clone, PartialEq)]
pub struct PathPointsLens {
    pub from: Arc<BezPath>,
    pub to: Arc<BezPath>,
    /// The elements of the outlines in between, whose vertices `pairs` give.
    between: Arc<BezPath>,
    /// Start and end of each vertex of the outlines in between.
    pairs: Arc<[(Point, Point)]>,
    rigid: Option<RigidMotion>,
}

impl PathPointsLens {
    /// The lens from `from` to the same outline through `points`, along
    /// `path`; `None` where [`with_vertices`] has no outline.
    pub fn new(from: Arc<BezPath>, points: &[Point], path: VertexPath) -> Option<Self> {
        let to = Arc::new(with_vertices(&from, points)?);
        let start = vertices(&from)?;
        if start.is_empty() {
            return None;
        }
        let pairs = paired_vertices(&start, points);
        let between = if start.len() == points.len() {
            from.clone()
        } else {
            let starts: Vec<Point> = pairs.iter().map(|pair| pair.0).collect();
            Arc::new(with_vertices(&from, &starts)?)
        };
        let rigid = match path {
            VertexPath::Linear => None,
            VertexPath::Rigid { about } => Some(RigidMotion::fit(&pairs, about)),
        };
        Some(Self {
            from,
            to,
            between,
            pairs: pairs.into(),
            rigid,
        })
    }

    /// The outline `t` of the way from `from` to `to`.
    pub fn at(&self, t: f64) -> BezPath {
        if t == 0.0 {
            return (*self.from).clone();
        }
        if t == 1.0 {
            return (*self.to).clone();
        }
        let mut next = self.pairs.iter().map(|&(start, end)| match &self.rigid {
            Some(motion) => motion.at(start, end, t),
            None => start.lerp(end, t),
        });
        BezPath::from_vec(
            self.between
                .elements()
                .iter()
                .map(|element| match element {
                    PathEl::MoveTo(_) => PathEl::MoveTo(next.next().unwrap_or_default()),
                    PathEl::LineTo(_) => PathEl::LineTo(next.next().unwrap_or_default()),
                    other => *other,
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

    fn points(coordinates: &[(f64, f64)]) -> Vec<Point> {
        coordinates.iter().map(|&point| point.into()).collect()
    }

    fn polygon(coordinates: &[(f64, f64)]) -> BezPath {
        let mut path = BezPath::new();
        path.move_to(coordinates[0]);
        coordinates[1..]
            .iter()
            .for_each(|&point| path.line_to(point));
        path.close_path();
        path
    }

    fn assert_near(actual: Point, expected: Point) {
        assert!(
            (actual - expected).hypot() < 1e-9,
            "{actual:?} is not {expected:?}"
        );
    }

    #[test]
    fn vertices_move_straight_to_their_targets() {
        let square = polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        let diamond = points(&[(1.0, -1.0), (3.0, 1.0), (1.0, 3.0), (-1.0, 1.0)]);
        let lens =
            PathPointsLens::new(Arc::new(square.clone()), &diamond, VertexPath::Linear).unwrap();
        let half = lens.at(0.5);
        assert_eq!(half.elements().len(), square.elements().len());
        assert_eq!(half.elements()[1], PathEl::LineTo(Point::new(2.5, 0.5)));
        assert_eq!(half.elements()[4], PathEl::ClosePath);
        assert_eq!(lens.at(0.0), square);
        assert_eq!(
            lens.at(1.0),
            polygon(&[(1.0, -1.0), (3.0, 1.0), (1.0, 3.0), (-1.0, 1.0)])
        );

        assert!(with_vertices(&square, &[]).is_none());
        let mut curve = BezPath::new();
        curve.move_to((0.0, 0.0));
        curve.quad_to((1.0, 1.0), (2.0, 0.0));
        assert!(with_vertices(&curve, &points(&[(0.0, 0.0), (2.0, 0.0)])).is_none());
        let mut two = square.clone();
        two.move_to((5.0, 5.0));
        two.line_to((6.0, 5.0));
        assert!(with_vertices(&two, &points(&[(0.0, 0.0); 5])).is_none());
        assert!(with_vertices(&two, &points(&[(0.0, 0.0); 6])).is_some());

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

    #[test]
    fn a_shape_with_fewer_vertices_repeats_some_of_them() {
        let triangle = polygon(&[(0.0, 0.0), (2.0, 0.0), (1.0, 2.0)]);
        let square = points(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        let lens =
            PathPointsLens::new(Arc::new(triangle.clone()), &square, VertexPath::Linear).unwrap();
        // Corner 0 splits into the square's first two corners.
        assert_eq!(
            lens.at(0.5),
            polygon(&[(0.0, 0.0), (1.0, 0.0), (2.0, 1.0), (0.5, 2.0)])
        );
        assert_eq!(lens.at(0.0), triangle);
        assert_eq!(
            lens.at(1.0),
            polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)])
        );

        // The other way the square's first two corners merge, and an open
        // outline stays open.
        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((2.0, 0.0));
        open.line_to((2.0, 2.0));
        open.line_to((0.0, 2.0));
        let lens = PathPointsLens::new(
            Arc::new(open),
            &points(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0)]),
            VertexPath::Linear,
        )
        .unwrap();
        let half = lens.at(0.5);
        assert_eq!(half.elements().len(), 4);
        assert_eq!(half.elements()[1], PathEl::LineTo(Point::new(1.0, 0.0)));
        assert_eq!(lens.at(1.0).elements().len(), 3);
        assert!(!lens.at(1.0).elements().contains(&PathEl::ClosePath));
    }

    #[test]
    fn a_rigid_path_turns_and_scales_without_deforming() {
        let corners = [(-1.0, -0.6), (1.0, -0.6), (0.3, 1.0)];
        let triangle = polygon(&corners);
        // Half a turn about the origin: the straight path collapses at the middle.
        let turned: Vec<Point> = corners.iter().map(|&(x, y)| Point::new(-x, -y)).collect();
        let about = Point::ORIGIN;
        let lens = PathPointsLens::new(
            Arc::new(triangle.clone()),
            &turned,
            VertexPath::Rigid { about: Some(about) },
        )
        .unwrap();
        let quarter = vertices(&lens.at(0.5)).unwrap();
        // Counterclockwise by a quarter turn about the origin.
        for (vertex, &(x, y)) in quarter.iter().zip(&corners) {
            assert_near(*vertex, Point::new(-y, x));
        }
        for (vertex, expected) in vertices(&lens.at(1.0)).unwrap().iter().zip(&turned) {
            assert_near(*vertex, *expected);
        }

        // Turned a third of a turn, doubled and moved: every frame is similar.
        let (sin, cos) = (std::f64::consts::TAU / 3.0).sin_cos();
        let target: Vec<Point> = corners
            .iter()
            .map(|&(x, y)| {
                Point::new(
                    2.0 * (x * cos - y * sin) + 3.0,
                    2.0 * (x * sin + y * cos) - 1.0,
                )
            })
            .collect();
        let lens = PathPointsLens::new(
            Arc::new(triangle),
            &target,
            VertexPath::Rigid { about: None },
        )
        .unwrap();
        let side = |vertices: &[Point]| (vertices[1] - vertices[0]).hypot();
        for t in [0.25, 0.5, 0.75] {
            let frame = vertices(&lens.at(t)).unwrap();
            let scale = 1.0 + t;
            assert!((side(&frame) - 2.0 * scale).abs() < 1e-9, "side at {t}");
            assert!(((frame[2] - frame[0]).hypot() - scale * 1.3f64.hypot(1.6)).abs() < 1e-9);
            // The vertices' center travels straight.
            let center = frame
                .iter()
                .fold(Vec2::ZERO, |sum, vertex| sum + vertex.to_vec2())
                / 3.0;
            let start = Vec2::new(0.1, -0.2 / 3.0);
            let end = target
                .iter()
                .fold(Vec2::ZERO, |sum, vertex| sum + vertex.to_vec2())
                / 3.0;
            assert_near(center.to_point(), start.lerp(end, t).to_point());
        }

        // A target that is not similar still lands on its points.
        let square = points(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        let lens = PathPointsLens::new(
            Arc::new(polygon(&corners)),
            &square,
            VertexPath::Rigid { about: None },
        )
        .unwrap();
        let motion = lens.rigid.unwrap();
        for &(start, end) in lens.pairs.iter() {
            assert_near(motion.at(start, end, 0.0), start);
            assert_near(motion.at(start, end, 1.0), end);
        }
    }
}
