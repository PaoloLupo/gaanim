//! Metaballs: the outline of circles that melt into each other.
//!
//! Every circle is a signed distance field (negative inside); a smooth
//! minimum joins them, so two circles closer than the smoothness bridge
//! with a neck instead of a corner. The zero level of the field is traced
//! with marching squares on a grid and fitted with cubic curves, giving a
//! vector outline that stays sharp at any size and exports as a path.

use std::collections::HashMap;

use kurbo::{BezPath, Point, Rect, Vec2};

/// A circle of the field, in scene units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ball {
    pub center: Point,
    pub radius: f64,
}

/// A cell edge of the grid: its lower or left corner and whether it is
/// vertical.
type EdgeKey = (usize, usize, bool);

/// Grid cells along the longer side of the traced region, at most.
const MAX_CELLS: usize = 240;
/// Cells across the smallest circle, at least, when the grid allows it.
const CELLS_PER_RADIUS: f64 = 6.0;

/// Polynomial smooth minimum: like `min(a, b)` but blended where the two
/// are within `k` of each other.
fn smooth_min(a: f64, b: f64, k: f64) -> f64 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// Signed distance of `point` to the blended circles: negative inside.
/// `threshold` divides every radius (1 keeps them, more thins them); two
/// circles less than `smoothness` apart join with a neck.
pub fn field(balls: &[Ball], point: Point, threshold: f64, smoothness: f64) -> f64 {
    // The blend lowers the field by up to a quarter of its width.
    let width = 2.0 * smoothness;
    balls
        .iter()
        .map(|ball| (point - ball.center).hypot() - ball.radius / threshold)
        .reduce(|a, b| smooth_min(a, b, width))
        .unwrap_or(f64::INFINITY)
}

/// The outline of `balls` blended with `smoothness` (scene units; 0 is a
/// plain union), as closed cubic curves wound with the inside on the left,
/// so holes stay open under the nonzero rule. Empty without balls.
pub fn metaballs_path(balls: &[Ball], threshold: f64, smoothness: f64) -> BezPath {
    let threshold = if threshold.is_finite() && threshold > 0.0 {
        threshold
    } else {
        1.0
    };
    let smoothness = if smoothness.is_finite() {
        smoothness.max(0.0)
    } else {
        0.0
    };
    let balls: Vec<Ball> = balls
        .iter()
        .copied()
        .filter(|ball| {
            ball.radius.is_finite()
                && ball.radius > 0.0
                && ball.center.x.is_finite()
                && ball.center.y.is_finite()
        })
        .collect();
    let Some(region) = balls
        .iter()
        .map(|ball| {
            let reach = ball.radius / threshold + smoothness;
            Rect::new(
                ball.center.x - reach,
                ball.center.y - reach,
                ball.center.x + reach,
                ball.center.y + reach,
            )
        })
        .reduce(|a, b| a.union(b))
    else {
        return BezPath::new();
    };
    let smallest = balls
        .iter()
        .map(|ball| ball.radius / threshold)
        .fold(f64::INFINITY, f64::min);
    let longest = region.width().max(region.height());
    let cell = (smallest / CELLS_PER_RADIUS).max(longest / MAX_CELLS as f64);
    // One empty cell around the region keeps every outline closed.
    let region = region.inflate(cell, cell);
    let columns = (region.width() / cell).ceil() as usize + 1;
    let rows = (region.height() / cell).ceil() as usize + 1;
    let at = |column: usize, row: usize| {
        Point::new(
            region.x0 + column as f64 * cell,
            region.y0 + row as f64 * cell,
        )
    };
    let mut values = vec![0.0; (columns + 1) * (rows + 1)];
    for row in 0..=rows {
        for column in 0..=columns {
            values[row * (columns + 1) + column] =
                field(&balls, at(column, row), threshold, smoothness);
        }
    }
    let value = |column: usize, row: usize| values[row * (columns + 1) + column];

    // Where the outline crosses a cell edge.
    let crossing = |a: (usize, usize), b: (usize, usize)| {
        let (va, vb) = (value(a.0, a.1), value(b.0, b.1));
        let t = (va / (va - vb)).clamp(0.0, 1.0);
        at(a.0, a.1).lerp(at(b.0, b.1), t)
    };
    // Each directed segment, keyed by the edge it starts on: where it ends.
    let mut segments: HashMap<EdgeKey, (EdgeKey, Point)> = HashMap::new();
    for row in 0..rows {
        for column in 0..columns {
            // Corners counterclockwise from the bottom left (y up).
            let corners = [
                (column, row),
                (column + 1, row),
                (column + 1, row + 1),
                (column, row + 1),
            ];
            let inside: Vec<bool> = corners.iter().map(|&(c, r)| value(c, r) < 0.0).collect();
            // Edges counterclockwise: bottom, right, top, left.
            let edges = [
                ((column, row, false), corners[0], corners[1]),
                ((column + 1, row, true), corners[1], corners[2]),
                ((column, row + 1, false), corners[3], corners[2]),
                ((column, row, true), corners[0], corners[3]),
            ];
            let cut: Vec<usize> = (0..4)
                .filter(|&edge| inside[edge] != inside[(edge + 1) % 4])
                .collect();
            let pairs: Vec<(usize, usize)> = match cut.as_slice() {
                [a, b] => vec![(*a, *b)],
                [a, b, c, d] => {
                    // A saddle: the cell's center decides which corners join.
                    let center = 0.25
                        * (value(column, row)
                            + value(column + 1, row)
                            + value(column + 1, row + 1)
                            + value(column, row + 1));
                    if (center < 0.0) == inside[0] {
                        vec![(*a, *b), (*c, *d)]
                    } else {
                        vec![(*d, *a), (*b, *c)]
                    }
                }
                _ => Vec::new(),
            };
            for (first, second) in pairs {
                let (key_a, a0, a1) = edges[first];
                let (key_b, b0, b1) = edges[second];
                let (pa, pb) = (crossing(a0, a1), crossing(b0, b1));
                // Inside on the left: the field grows to the right.
                let middle = pa.midpoint(pb);
                let gradient = gradient_at(&balls, middle, threshold, smoothness, cell);
                let along = pb - pa;
                let left = Vec2::new(-along.y, along.x);
                if left.dot(gradient) <= 0.0 {
                    segments.insert(key_a, (key_b, pb));
                } else {
                    segments.insert(key_b, (key_a, pa));
                }
            }
        }
    }

    let mut path = BezPath::new();
    while let Some(&start) = segments.keys().next() {
        let mut loop_points = Vec::new();
        let mut key = start;
        while let Some((next, point)) = segments.remove(&key) {
            loop_points.push(point);
            key = next;
            if key == start {
                break;
            }
        }
        if loop_points.len() >= 3 {
            append_smooth_loop(&mut path, &loop_points);
        }
    }
    path
}

fn gradient_at(balls: &[Ball], point: Point, threshold: f64, smoothness: f64, step: f64) -> Vec2 {
    let h = step * 0.25;
    let dx = field(balls, point + Vec2::new(h, 0.0), threshold, smoothness)
        - field(balls, point - Vec2::new(h, 0.0), threshold, smoothness);
    let dy = field(balls, point + Vec2::new(0.0, h), threshold, smoothness)
        - field(balls, point - Vec2::new(0.0, h), threshold, smoothness);
    Vec2::new(dx, dy)
}

/// A closed Catmull-Rom curve through `points`, as cubic Béziers.
fn append_smooth_loop(path: &mut BezPath, points: &[Point]) {
    let count = points.len();
    let point = |index: isize| points[index.rem_euclid(count as isize) as usize];
    path.move_to(points[0]);
    for index in 0..count as isize {
        let (p0, p1, p2, p3) = (
            point(index - 1),
            point(index),
            point(index + 1),
            point(index + 2),
        );
        path.curve_to(p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2);
    }
    path.close_path();
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    fn ball(x: f64, y: f64, radius: f64) -> Ball {
        Ball {
            center: Point::new(x, y),
            radius,
        }
    }

    #[test]
    fn one_ball_traces_its_circle() {
        let path = metaballs_path(&[ball(1.0, 2.0, 1.0)], 1.0, 0.4);
        let bounds = path.bounding_box();
        assert!((bounds.width() - 2.0).abs() < 0.02, "{bounds:?}");
        assert!((bounds.center().x - 1.0).abs() < 0.01);
        assert!(
            (path.area() - std::f64::consts::PI).abs() < 0.02,
            "{}",
            path.area()
        );
        // A higher threshold thins the ball.
        let thin = metaballs_path(&[ball(0.0, 0.0, 1.0)], 2.0, 0.0);
        assert!((thin.bounding_box().width() - 1.0).abs() < 0.02);
    }

    #[test]
    fn close_balls_melt_into_one_outline_and_far_ones_stay_apart() {
        let close = metaballs_path(&[ball(-0.9, 0.0, 0.6), ball(0.9, 0.0, 0.6)], 1.0, 0.8);
        let loops = |path: &BezPath| {
            path.elements()
                .iter()
                .filter(|element| matches!(element, kurbo::PathEl::MoveTo(_)))
                .count()
        };
        assert_eq!(loops(&close), 1);
        // The neck fills the gap between them.
        assert!(close.winding(Point::new(0.0, 0.0)) != 0);
        let apart = metaballs_path(&[ball(-2.0, 0.0, 0.6), ball(2.0, 0.0, 0.6)], 1.0, 0.4);
        assert_eq!(loops(&apart), 2);
        assert_eq!(apart.winding(Point::new(0.0, 0.0)), 0);
        // Without smoothness, touching circles only union.
        let union = metaballs_path(&[ball(-0.9, 0.0, 0.6), ball(0.9, 0.0, 0.6)], 1.0, 0.0);
        assert_eq!(union.winding(Point::new(0.0, 0.0)), 0);
    }

    #[test]
    fn a_ring_of_balls_keeps_its_hole() {
        let balls: Vec<Ball> = (0..12)
            .map(|index| {
                let angle = index as f64 / 12.0 * std::f64::consts::TAU;
                ball(2.0 * angle.cos(), 2.0 * angle.sin(), 0.55)
            })
            .collect();
        let path = metaballs_path(&balls, 1.0, 0.4);
        assert_eq!(path.winding(Point::new(0.0, 0.0)), 0, "the hole is open");
        assert_ne!(path.winding(Point::new(2.0, 0.0)), 0);
    }

    #[test]
    fn invalid_balls_are_ignored() {
        assert!(metaballs_path(&[], 1.0, 0.4).elements().is_empty());
        assert!(
            metaballs_path(&[ball(f64::NAN, 0.0, 1.0), ball(0.0, 0.0, 0.0)], 1.0, 0.4)
                .elements()
                .is_empty()
        );
    }
}
