//! Slices of pie and donut charts and their in-between states.
//!
//! A slice is described by its angles and radii rather than by its outline,
//! so an animation interpolates those numbers and every frame is a true
//! slice: growing a pie, or moving a slice to its new share, sweeps around
//! the center instead of cutting across it. The outline always has the same
//! segments, so a slice and its partially grown state build paths with the
//! same structure.

use kurbo::{BezPath, Point};

/// Cubic segments of each arc of a slice, whatever its sweep: a full turn
/// stays within a quarter of a degree of the circle.
const ARC_SEGMENTS: usize = 16;

/// One slice of a pie or donut chart, centered at the origin.
///
/// Angles are in radians, counterclockwise from the positive x axis, with
/// `start <= end`; radii are in scene units, with `inner` 0 for a pie. `pad`
/// is the angle left empty between neighbors, half on each side of the slice.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ArcSlice {
    pub start: f64,
    pub end: f64,
    pub inner: f64,
    pub outer: f64,
    pub pad: f64,
}

impl ArcSlice {
    /// The angle in the middle of the slice.
    pub fn mid_angle(&self) -> f64 {
        (self.start + self.end) * 0.5
    }

    /// The same slice with no sweep, at `angle`: where a slice grows from or
    /// shrinks to.
    pub fn collapsed_at(&self, angle: f64) -> Self {
        Self {
            start: angle,
            end: angle,
            ..*self
        }
    }

    /// The slice `t` of the way from `self` to `other`, every number
    /// interpolated linearly.
    pub fn interpolate(&self, other: &Self, t: f64) -> Self {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        Self {
            start: mix(self.start, other.start),
            end: mix(self.end, other.end),
            inner: mix(self.inner, other.inner),
            outer: mix(self.outer, other.outer),
            pad: mix(self.pad, other.pad),
        }
    }

    /// Where a label of the slice goes: at its middle angle, halfway between
    /// its radii.
    pub fn centroid(&self) -> Point {
        let angle = self.mid_angle();
        let radius = (self.inner + self.outer) * 0.5;
        Point::new(radius * angle.cos(), radius * angle.sin())
    }

    /// The closed outline: the outer arc counterclockwise, then the inner arc
    /// back, or the center for a pie. A slice narrower than its padding
    /// collapses to its middle angle and draws nothing.
    pub fn path(&self) -> BezPath {
        let half_pad = (self.pad.max(0.0) * 0.5).min((self.end - self.start).max(0.0) * 0.5);
        let start = self.start + half_pad;
        let end = self.end - half_pad;
        let inner = self.inner.max(0.0);
        let outer = self.outer.max(inner);
        let mut path = BezPath::new();
        let at = |radius: f64, angle: f64| Point::new(radius * angle.cos(), radius * angle.sin());
        path.move_to(at(outer, start));
        append_arc(&mut path, outer, start, end);
        if inner > 0.0 {
            path.line_to(at(inner, end));
            append_arc(&mut path, inner, end, start);
        } else {
            path.line_to(Point::ORIGIN);
        }
        path.close_path();
        path
    }
}

/// Append [`ARC_SEGMENTS`] cubics along the circle of `radius` from `from`
/// to `to` (either direction), starting at the current point.
fn append_arc(path: &mut BezPath, radius: f64, from: f64, to: f64) {
    let step = (to - from) / ARC_SEGMENTS as f64;
    // Tangent length of a cubic that follows a circular arc of `step`.
    let handle = radius * 4.0 / 3.0 * (step / 4.0).tan();
    for index in 0..ARC_SEGMENTS {
        let a0 = from + step * index as f64;
        let a1 = a0 + step;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        path.curve_to(
            Point::new(radius * c0 - handle * s0, radius * s0 + handle * c0),
            Point::new(radius * c1 + handle * s1, radius * s1 - handle * c1),
            Point::new(radius * c1, radius * s1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    fn slice(start: f64, end: f64, inner: f64) -> ArcSlice {
        ArcSlice {
            start,
            end,
            inner,
            outer: 2.0,
            pad: 0.0,
        }
    }

    #[test]
    fn a_slice_covers_its_share_of_the_disc_or_ring() {
        let quarter = slice(0.0, FRAC_PI_2, 0.0).path();
        assert!((quarter.area().abs() - PI).abs() < 1e-3);
        let ring = slice(0.0, TAU, 1.0).path();
        assert!((ring.area().abs() - 3.0 * PI).abs() < 1e-3);
    }

    #[test]
    fn every_state_has_the_same_segments() {
        let full = slice(0.0, PI, 0.5);
        let empty = full.collapsed_at(0.0);
        let count = |slice: ArcSlice| slice.path().elements().len();
        assert_eq!(count(full), count(empty));
        assert_eq!(count(full), count(full.interpolate(&empty, 0.3)));
        assert!(empty.path().area().abs() < 1e-9);
    }

    #[test]
    fn padding_trims_both_sides_and_never_inverts_a_slice() {
        let padded = ArcSlice {
            pad: 0.2,
            ..slice(0.0, FRAC_PI_2, 0.0)
        };
        let bounds = padded.path().bounding_box();
        assert!(bounds.y0 > -1e-9 && bounds.x0 > -1e-9);
        let narrow = ArcSlice {
            pad: 1.0,
            ..slice(0.0, 0.1, 0.0)
        };
        assert!(narrow.path().area().abs() < 1e-9);
    }

    #[test]
    fn the_centroid_sits_between_the_radii_at_the_middle_angle() {
        let centroid = slice(0.0, PI, 1.0).centroid();
        assert!(centroid.x.abs() < 1e-12);
        assert!((centroid.y - 1.5).abs() < 1e-12);
    }
}
