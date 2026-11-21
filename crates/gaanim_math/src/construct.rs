//! Ruler-and-compass constructions: points found from other points, as a
//! geometry demonstration builds them.
//!
//! A line is given by two of its points and a circle by its center and a
//! point on it. [`Construction::solve`] returns `None` where the
//! construction has no answer (parallel lines, a point inside the circle,
//! points in a row), so what depends on it can hide instead of failing.

use glam::DVec2;

/// Below this, lengths and areas count as zero.
const EPSILON: f64 = 1e-9;

/// A point constructed from other points; see [`Self::arity`] for the
/// points each one takes, in order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Construction {
    /// `[p, q]`: the point `length` from `p` toward `q` (back, if negative).
    Extend { length: f64 },
    /// `[a, b, c]`: one unit from `c` across line `ab`, so that line
    /// through `c` and it is the perpendicular to `ab` through `c`.
    Perpendicular,
    /// `[a, b, c]`: one unit from `c` along `ab`: with `c`, the parallel.
    Parallel,
    /// `[a, o, b]`: one unit from `o` along the bisector of angle `aob`.
    AngleBisector,
    /// `[a, b, c, d]`: where lines `ab` and `cd` cross.
    LineLine,
    /// `[a, b, center, on]`: where line `ab` meets the circle, `which` 0
    /// nearer `a`, 1 nearer `b`.
    LineCircle { which: u8 },
    /// `[c1, on1, c2, on2]`: where two circles meet, `which` 0 left of the
    /// line from the first center to the second, 1 right of it.
    CircleCircle { which: u8 },
    /// `[center, on, p]`: where a tangent from `p` touches the circle,
    /// `which` 0 left of the line from `p` to the center, 1 right.
    Tangent { which: u8 },
    /// `[a, b, c]`: the center of the circle through the three points.
    Circumcenter,
    /// `[a, b, c]`: the center of the circle inside the triangle that
    /// touches its sides.
    Incenter,
    /// `[a, b, c]`: where the medians meet.
    Centroid,
    /// `[a, b, p]`: the foot of the perpendicular from `p` to line `ab`.
    Foot,
}

impl Construction {
    /// How many points the construction takes.
    pub fn arity(&self) -> usize {
        match self {
            Self::Extend { .. } => 2,
            Self::Perpendicular
            | Self::Parallel
            | Self::AngleBisector
            | Self::Tangent { .. }
            | Self::Circumcenter
            | Self::Incenter
            | Self::Centroid
            | Self::Foot => 3,
            Self::LineLine | Self::LineCircle { .. } | Self::CircleCircle { .. } => 4,
        }
    }

    /// The constructed point, or `None` where there is none.
    pub fn solve(&self, points: &[DVec2]) -> Option<DVec2> {
        if points.len() != self.arity() || points.iter().any(|point| !point.is_finite()) {
            return None;
        }
        let point = match *self {
            Self::Extend { length } => points[0] + unit(points[1] - points[0])? * length,
            Self::Perpendicular => points[2] + unit(points[1] - points[0])?.perp(),
            Self::Parallel => points[2] + unit(points[1] - points[0])?,
            Self::AngleBisector => {
                let (a, o, b) = (points[0], points[1], points[2]);
                let (along_a, along_b) = (unit(a - o)?, unit(b - o)?);
                // A straight angle bisects at a right angle.
                o + unit(along_a + along_b).unwrap_or(along_a.perp())
            }
            Self::LineLine => line_line(points[0], points[1], points[2], points[3])?,
            Self::LineCircle { which } => {
                let radius = points[3].distance(points[2]);
                line_circle(points[0], points[1], points[2], radius, which)?
            }
            Self::CircleCircle { which } => {
                let (r1, r2) = (points[1].distance(points[0]), points[3].distance(points[2]));
                circle_circle(points[0], r1, points[2], r2, which)?
            }
            Self::Tangent { which } => {
                let (center, p) = (points[0], points[2]);
                let radius = points[1].distance(center);
                tangent_point(center, radius, p, which)?
            }
            Self::Circumcenter => circumcenter(points[0], points[1], points[2])?,
            Self::Incenter => {
                let (a, b, c) = (points[0], points[1], points[2]);
                let (la, lb, lc) = (b.distance(c), c.distance(a), a.distance(b));
                let total = la + lb + lc;
                if cross(b - a, c - a).abs() <= EPSILON || total <= EPSILON {
                    return None;
                }
                (a * la + b * lb + c * lc) / total
            }
            Self::Centroid => (points[0] + points[1] + points[2]) / 3.0,
            Self::Foot => {
                let (a, b, p) = (points[0], points[1], points[2]);
                let direction = b - a;
                let length = direction.length_squared();
                if length <= EPSILON {
                    return None;
                }
                a + direction * ((p - a).dot(direction) / length)
            }
        };
        point.is_finite().then_some(point)
    }
}

fn unit(vector: DVec2) -> Option<DVec2> {
    let length = vector.length();
    (length > EPSILON).then(|| vector / length)
}

fn cross(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn line_line(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<DVec2> {
    let (r, s) = (b - a, d - c);
    let denominator = cross(r, s);
    if denominator.abs() <= EPSILON * r.length().max(1.0) * s.length().max(1.0) {
        return None;
    }
    Some(a + r * (cross(c - a, s) / denominator))
}

fn line_circle(a: DVec2, b: DVec2, center: DVec2, radius: f64, which: u8) -> Option<DVec2> {
    let direction = unit(b - a)?;
    let foot = a + direction * (center - a).dot(direction);
    let distance = foot.distance(center);
    if distance > radius + EPSILON {
        return None;
    }
    let half = (radius * radius - distance * distance).max(0.0).sqrt();
    Some(if which == 0 {
        foot - direction * half
    } else {
        foot + direction * half
    })
}

fn circle_circle(c1: DVec2, r1: f64, c2: DVec2, r2: f64, which: u8) -> Option<DVec2> {
    let between = c2 - c1;
    let distance = between.length();
    if distance <= EPSILON || distance > r1 + r2 + EPSILON || distance < (r1 - r2).abs() - EPSILON {
        return None;
    }
    let along = (r1 * r1 - r2 * r2 + distance * distance) / (2.0 * distance);
    let height = (r1 * r1 - along * along).max(0.0).sqrt();
    let direction = between / distance;
    let base = c1 + direction * along;
    Some(if which == 0 {
        base + direction.perp() * height
    } else {
        base - direction.perp() * height
    })
}

fn tangent_point(center: DVec2, radius: f64, p: DVec2, which: u8) -> Option<DVec2> {
    let toward = center - p;
    let distance = toward.length();
    if distance < radius - EPSILON || radius <= EPSILON {
        return None;
    }
    // The tangent points lie on the circle with diameter from p to the
    // center: where the two circles meet.
    let midpoint = (p + center) / 2.0;
    let side = if which == 0 { 1 } else { 0 };
    circle_circle(center, radius, midpoint, distance / 2.0, side).or_else(|| {
        // p on the circle: it is its own tangent point.
        (distance <= radius + EPSILON).then_some(p)
    })
}

fn circumcenter(a: DVec2, b: DVec2, c: DVec2) -> Option<DVec2> {
    let denominator = 2.0 * cross(b - a, c - a);
    if denominator.abs() <= EPSILON {
        return None;
    }
    let (ab, ac) = (b - a, c - a);
    let offset = DVec2::new(
        ac.y * ab.length_squared() - ab.y * ac.length_squared(),
        ab.x * ac.length_squared() - ac.x * ab.length_squared(),
    ) / denominator;
    Some(a + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Option<DVec2>, b: DVec2) -> bool {
        a.is_some_and(|a| a.distance(b) < 1e-9)
    }

    #[test]
    fn lines_cross_unless_parallel() {
        let x = DVec2::new;
        assert!(close(
            Construction::LineLine.solve(&[x(0.0, 0.0), x(2.0, 2.0), x(0.0, 2.0), x(2.0, 0.0)]),
            x(1.0, 1.0)
        ));
        assert_eq!(
            Construction::LineLine.solve(&[x(0.0, 0.0), x(1.0, 0.0), x(0.0, 1.0), x(3.0, 1.0)]),
            None
        );
    }

    #[test]
    fn circles_meet_lines_and_each_other() {
        let x = DVec2::new;
        // The unit circle and the x axis.
        let points = [x(-3.0, 0.0), x(3.0, 0.0), x(0.0, 0.0), x(0.0, 1.0)];
        assert!(close(
            Construction::LineCircle { which: 0 }.solve(&points),
            x(-1.0, 0.0)
        ));
        assert!(close(
            Construction::LineCircle { which: 1 }.solve(&points),
            x(1.0, 0.0)
        ));
        // A line that misses it.
        let missing = [x(-3.0, 2.0), x(3.0, 2.0), x(0.0, 0.0), x(0.0, 1.0)];
        assert_eq!(Construction::LineCircle { which: 0 }.solve(&missing), None);
        // Two unit circles a unit apart meet at x = 0.5.
        let circles = [x(0.0, 0.0), x(1.0, 0.0), x(1.0, 0.0), x(2.0, 0.0)];
        let top = Construction::CircleCircle { which: 0 }
            .solve(&circles)
            .unwrap();
        let bottom = Construction::CircleCircle { which: 1 }
            .solve(&circles)
            .unwrap();
        assert!((top.x - 0.5).abs() < 1e-9 && top.y > 0.0 && bottom.y < 0.0);
        let apart = [x(0.0, 0.0), x(1.0, 0.0), x(5.0, 0.0), x(6.0, 0.0)];
        assert_eq!(Construction::CircleCircle { which: 0 }.solve(&apart), None);
    }

    #[test]
    fn tangents_touch_the_circle_at_right_angles() {
        let x = DVec2::new;
        let (center, p) = (x(0.0, 0.0), x(2.0, 0.0));
        for which in [0, 1] {
            let touch = Construction::Tangent { which }
                .solve(&[center, x(1.0, 0.0), p])
                .unwrap();
            assert!((touch.length() - 1.0).abs() < 1e-9);
            assert!((touch - center).dot(touch - p).abs() < 1e-9);
        }
        // From inside the circle there is none.
        assert_eq!(
            Construction::Tangent { which: 0 }.solve(&[center, x(1.0, 0.0), x(0.5, 0.0)]),
            None
        );
    }

    #[test]
    fn triangle_centers() {
        let x = DVec2::new;
        let (a, b, c) = (x(0.0, 0.0), x(4.0, 0.0), x(0.0, 3.0));
        let center = Construction::Circumcenter.solve(&[a, b, c]).unwrap();
        assert!(close(Some(center), x(2.0, 1.5)));
        assert!((center.distance(a) - center.distance(c)).abs() < 1e-9);
        // A 3-4-5 triangle's inradius is 1.
        assert!(close(Construction::Incenter.solve(&[a, b, c]), x(1.0, 1.0)));
        assert!(close(
            Construction::Centroid.solve(&[a, b, c]),
            x(4.0 / 3.0, 1.0)
        ));
        assert!(close(
            Construction::Foot.solve(&[a, b, x(1.0, 2.0)]),
            x(1.0, 0.0)
        ));
        // Points in a row have no circumcenter or incenter.
        let row = [a, b, x(8.0, 0.0)];
        assert_eq!(Construction::Circumcenter.solve(&row), None);
        assert_eq!(Construction::Incenter.solve(&row), None);
    }

    #[test]
    fn directions_for_lines() {
        let x = DVec2::new;
        let (a, b, c) = (x(0.0, 0.0), x(2.0, 0.0), x(1.0, 1.0));
        assert!(close(
            Construction::Perpendicular.solve(&[a, b, c]),
            x(1.0, 2.0)
        ));
        assert!(close(Construction::Parallel.solve(&[a, b, c]), x(2.0, 1.0)));
        assert!(close(
            Construction::Extend { length: -2.0 }.solve(&[a, b]),
            x(-2.0, 0.0)
        ));
        let bisector = Construction::AngleBisector
            .solve(&[x(1.0, 0.0), a, x(0.0, 1.0)])
            .unwrap();
        assert!((bisector.x - bisector.y).abs() < 1e-9);
        assert_eq!(Construction::Perpendicular.solve(&[a, a, c]), None);
    }
}
