//! Hand-drawn notations (AN-01): underlines, boxes, circles, brackets and
//! strikes around a rectangle, in the style of rough-notation.
//!
//! Lines follow the Rough.js algorithm: each end moves a little at random
//! and the stroke bows through two control points placed near 20–40 % of
//! the way, so a straight edge reads as drawn by hand. Each pass draws the
//! whole shape again with new randomness, which gives the doubled stroke.
//! Everything is a pure function of the seed and of the rectangle's size:
//! moving the rectangle moves the scribble without changing it.

use kurbo::{BezPath, Point, Rect, Vec2};

use crate::SeededRng;

/// Rough.js works in pixels; its constants are kept at this many per scene unit.
const PIXELS_PER_UNIT: f64 = 100.0;
/// Rough.js `maxRandomnessOffset`, in pixels.
const MAX_OFFSET: f64 = 2.0;
/// Rough.js `bowing`.
const BOWING: f64 = 1.0;
/// Points sampled around an ellipse per pass, as Rough.js `curveStepCount`.
const ELLIPSE_STEPS: usize = 9;

/// Which sides of the rectangle a bracket notation draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BracketSides {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

/// The shape a notation draws around its rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NotationShape {
    /// A line along the bottom edge.
    Underline,
    /// The four edges.
    Box,
    /// An ellipse through the middle of each edge.
    Circle,
    /// A line across the middle.
    StrikeThrough,
    /// Both diagonals.
    CrossedOff,
    /// Square brackets on the chosen sides.
    Bracket(BracketSides),
}

/// A hand-drawn notation: its shape and how rough it is.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoughNotation {
    pub shape: NotationShape,
    /// 0 draws clean geometry; 1 is the Rough.js default wobble.
    pub roughness: f64,
    /// How many times the shape is drawn over itself.
    pub passes: u32,
    pub seed: u64,
}

impl RoughNotation {
    /// The notation's path around `rect`, every pass one after another so a
    /// sequential reveal draws them in order.
    pub fn path(&self, rect: Rect) -> BezPath {
        let size = Vec2::new(rect.width().abs(), rect.height().abs());
        let mut path = BezPath::new();
        let mut rng = SeededRng::new(self.seed);
        for _ in 0..self.passes.max(1) {
            let mut pass = SeededRng::new(rng.next_u64());
            self.draw_pass(size, &mut pass, &mut path);
        }
        let origin = Point::new(rect.x0.min(rect.x1), rect.y0.min(rect.y1));
        path.apply_affine(kurbo::Affine::translate(origin.to_vec2()));
        path
    }

    /// One pass of the shape over a rectangle at the origin of `size`.
    fn draw_pass(&self, size: Vec2, rng: &mut SeededRng, path: &mut BezPath) {
        let (w, h) = (size.x, size.y);
        let corner = |x: f64, y: f64| Point::new(x, y);
        let mut line = |a: Point, b: Point| rough_line(a, b, self.roughness, rng, path);
        match self.shape {
            NotationShape::Underline => line(corner(0.0, 0.0), corner(w, 0.0)),
            NotationShape::StrikeThrough => line(corner(0.0, h / 2.0), corner(w, h / 2.0)),
            NotationShape::CrossedOff => {
                line(corner(0.0, h), corner(w, 0.0));
                line(corner(0.0, 0.0), corner(w, h));
            }
            NotationShape::Box => {
                line(corner(0.0, h), corner(w, h));
                line(corner(w, h), corner(w, 0.0));
                line(corner(w, 0.0), corner(0.0, 0.0));
                line(corner(0.0, 0.0), corner(0.0, h));
            }
            NotationShape::Bracket(sides) => {
                let arm = (0.15 * w.min(h)).clamp(0.04, 0.2).min(w / 2.0).min(h / 2.0);
                let mut polyline = |points: [Point; 4]| {
                    for pair in points.windows(2) {
                        line(pair[0], pair[1]);
                    }
                };
                if sides.left {
                    polyline([
                        corner(arm, h),
                        corner(0.0, h),
                        corner(0.0, 0.0),
                        corner(arm, 0.0),
                    ]);
                }
                if sides.right {
                    polyline([
                        corner(w - arm, h),
                        corner(w, h),
                        corner(w, 0.0),
                        corner(w - arm, 0.0),
                    ]);
                }
                if sides.top {
                    polyline([
                        corner(0.0, h - arm),
                        corner(0.0, h),
                        corner(w, h),
                        corner(w, h - arm),
                    ]);
                }
                if sides.bottom {
                    polyline([
                        corner(0.0, arm),
                        corner(0.0, 0.0),
                        corner(w, 0.0),
                        corner(w, arm),
                    ]);
                }
            }
            NotationShape::Circle => rough_ellipse(
                Point::new(w / 2.0, h / 2.0),
                size / 2.0,
                self.roughness,
                rng,
                path,
            ),
        }
    }
}

/// A uniform value in `[-limit, limit]` scaled by the roughness.
fn jitter(rng: &mut SeededRng, limit: f64, roughness: f64, gain: f64) -> f64 {
    roughness * gain * rng.uniform(-limit, limit)
}

/// A Rough.js line from `a` to `b`: one bowed cubic with jittered ends.
fn rough_line(a: Point, b: Point, roughness: f64, rng: &mut SeededRng, path: &mut BezPath) {
    let (x1, y1) = (a.x * PIXELS_PER_UNIT, a.y * PIXELS_PER_UNIT);
    let (x2, y2) = (b.x * PIXELS_PER_UNIT, b.y * PIXELS_PER_UNIT);
    let length = (x2 - x1).hypot(y2 - y1);
    let gain = if length < 200.0 {
        1.0
    } else if length > 500.0 {
        0.4
    } else {
        -0.0016668 * length + 1.233334
    };
    let offset = if MAX_OFFSET * MAX_OFFSET * 100.0 > length * length {
        length / 10.0
    } else {
        MAX_OFFSET
    };
    let diverge = 0.2 + rng.next_f64() * 0.2;
    let bow_x = BOWING * MAX_OFFSET * (y2 - y1) / 200.0;
    let bow_y = BOWING * MAX_OFFSET * (x1 - x2) / 200.0;
    let bow_x = jitter(rng, bow_x.abs(), roughness, gain);
    let bow_y = jitter(rng, bow_y.abs(), roughness, gain);
    let mut r = || jitter(rng, offset, roughness, gain);
    let unit = |x: f64, y: f64| Point::new(x / PIXELS_PER_UNIT, y / PIXELS_PER_UNIT);
    path.move_to(unit(x1 + r(), y1 + r()));
    path.curve_to(
        unit(
            bow_x + x1 + (x2 - x1) * diverge + r(),
            bow_y + y1 + (y2 - y1) * diverge + r(),
        ),
        unit(
            bow_x + x1 + 2.0 * (x2 - x1) * diverge + r(),
            bow_y + y1 + 2.0 * (y2 - y1) * diverge + r(),
        ),
        unit(x2 + r(), y2 + r()),
    );
}

/// A Rough.js ellipse: points around it with jittered radii, joined by a
/// smooth curve that starts at a random angle and overlaps its start.
fn rough_ellipse(
    center: Point,
    radii: Vec2,
    roughness: f64,
    rng: &mut SeededRng,
    path: &mut BezPath,
) {
    let (rx, ry) = (radii.x * PIXELS_PER_UNIT, radii.y * PIXELS_PER_UNIT);
    let perimeter = std::f64::consts::TAU * ((rx * rx + ry * ry) / 2.0).sqrt();
    let gain = (perimeter / 400.0).clamp(1.0, 2.0);
    let rx = rx + jitter(rng, rx * 0.04, roughness, 1.0);
    let ry = ry + jitter(rng, ry * 0.04, roughness, 1.0);
    let step = std::f64::consts::TAU / ELLIPSE_STEPS as f64;
    let start = rng.uniform(0.0, std::f64::consts::TAU);
    let overlap = step * rng.uniform(0.3, 0.8) * roughness.clamp(0.0, 1.0);
    let end = start + std::f64::consts::TAU + overlap;
    let mut points = Vec::with_capacity(ELLIPSE_STEPS + 3);
    let mut angle = start;
    loop {
        let angle_now = angle.min(end);
        points.push(Point::new(
            center.x * PIXELS_PER_UNIT
                + rx * angle_now.cos()
                + jitter(rng, MAX_OFFSET, roughness, gain),
            center.y * PIXELS_PER_UNIT
                + ry * angle_now.sin()
                + jitter(rng, MAX_OFFSET, roughness, gain),
        ));
        if angle >= end {
            break;
        }
        angle += step;
    }
    let unit = |point: Point| Point::new(point.x / PIXELS_PER_UNIT, point.y / PIXELS_PER_UNIT);
    // Catmull-Rom through the points, the ends repeated as their own neighbors.
    path.move_to(unit(points[0]));
    for index in 0..points.len() - 1 {
        let before = points[index.saturating_sub(1)];
        let from = points[index];
        let to = points[index + 1];
        let after = points[(index + 2).min(points.len() - 1)];
        path.curve_to(
            unit(from + (to - before) / 6.0),
            unit(to - (after - from) / 6.0),
            unit(to),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    fn notation(shape: NotationShape, roughness: f64) -> RoughNotation {
        RoughNotation {
            shape,
            roughness,
            passes: 2,
            seed: 7,
        }
    }

    fn subpaths(path: &BezPath) -> usize {
        path.elements()
            .iter()
            .filter(|element| matches!(element, kurbo::PathEl::MoveTo(_)))
            .count()
    }

    #[test]
    fn shapes_draw_their_strokes_once_per_pass() {
        let rect = Rect::new(-1.0, -0.5, 2.0, 0.5);
        let both = BracketSides {
            left: true,
            right: true,
            ..BracketSides::default()
        };
        for (shape, strokes) in [
            (NotationShape::Underline, 1),
            (NotationShape::StrikeThrough, 1),
            (NotationShape::CrossedOff, 2),
            (NotationShape::Box, 4),
            (NotationShape::Circle, 1),
            (NotationShape::Bracket(both), 6),
        ] {
            assert_eq!(
                subpaths(&notation(shape, 1.0).path(rect)),
                2 * strokes,
                "{shape:?}"
            );
        }
    }

    #[test]
    fn without_roughness_the_geometry_is_exact() {
        let rect = Rect::new(1.0, 2.0, 4.0, 3.0);
        let underline = notation(NotationShape::Underline, 0.0).path(rect);
        let bounds = underline.bounding_box();
        assert!((bounds.x0 - 1.0).abs() < 1e-9 && (bounds.x1 - 4.0).abs() < 1e-9);
        assert!((bounds.y0 - 2.0).abs() < 1e-9 && (bounds.y1 - 2.0).abs() < 1e-9);
        let boxed = notation(NotationShape::Box, 0.0).path(rect).bounding_box();
        for (actual, expected) in [
            (boxed.x0, rect.x0),
            (boxed.y0, rect.y0),
            (boxed.x1, rect.x1),
            (boxed.y1, rect.y1),
        ] {
            assert!((actual - expected).abs() < 1e-9);
        }
    }

    #[test]
    fn rough_strokes_stay_near_the_shape_and_repeat_with_the_seed() {
        let rect = Rect::new(0.0, 0.0, 3.0, 1.0);
        for shape in [NotationShape::Box, NotationShape::Circle] {
            let path = notation(shape, 1.0).path(rect);
            let bounds = path.bounding_box();
            assert!(
                bounds.x0 > -0.25 && bounds.x1 < 3.25,
                "{shape:?} {bounds:?}"
            );
            assert!(
                bounds.y0 > -0.25 && bounds.y1 < 1.25,
                "{shape:?} {bounds:?}"
            );
            assert_eq!(path, notation(shape, 1.0).path(rect));
            let other = RoughNotation {
                seed: 8,
                ..notation(shape, 1.0)
            };
            assert_ne!(path, other.path(rect));
        }
        // Moving the rectangle moves the scribble without redrawing it.
        let moved = notation(NotationShape::Circle, 1.0).path(rect + Vec2::new(5.0, -2.0));
        let mut expected = notation(NotationShape::Circle, 1.0).path(rect);
        expected.apply_affine(kurbo::Affine::translate((5.0, -2.0)));
        for (a, b) in moved.elements().iter().zip(expected.elements()) {
            let (a, b) = (a.end_point().unwrap(), b.end_point().unwrap());
            assert!((a - b).hypot() < 1e-9);
        }
    }
}
