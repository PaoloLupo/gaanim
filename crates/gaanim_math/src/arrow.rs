//! Solid arrow silhouettes and their partially grown states.
//!
//! The geometry lives here, below the object and animation crates, so the
//! spawned arrow and the `GrowArrow` lens build byte-identical paths: a grown
//! arrow at progress `1.0` is exactly the authored arrow.

use kurbo::{BezPath, Point, Vec2};

/// Angular step for the polyline that approximates a curved shaft.
const ARC_STEP_ANGLE: f64 = 0.035;

/// Polyline segment count for a circular sweep, sampled by angle so the
/// facet error stays below a pixel at any scene scale.
pub fn arc_steps(sweep_angle: f64, min_steps: u32) -> u32 {
    ((sweep_angle.abs() / ARC_STEP_ANGLE).ceil() as u32).max(min_steps)
}

/// Local-space description of a solid single-headed arrow.
///
/// Dimensions are absolute scene units. The tip is at `end` (straight) or at
/// the end of the sweep (arc).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ArrowShape {
    Straight {
        start: Point,
        end: Point,
        head_length: f64,
        head_width: f64,
        body_width: f64,
    },
    Arc {
        center: Point,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
        head_length: f64,
        head_width: f64,
        body_width: f64,
    },
}

impl ArrowShape {
    /// Arrow between two points bent by a signed angular deflection. A
    /// negligible deflection or chord yields a straight arrow.
    pub fn curved(
        start: Point,
        end: Point,
        angle: f64,
        head_length: f64,
        head_width: f64,
        body_width: f64,
    ) -> Self {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let chord = (dx * dx + dy * dy).sqrt();
        if chord <= f64::EPSILON || angle.abs() <= 1e-6 {
            return Self::Straight {
                start,
                end,
                head_length,
                head_width,
                body_width,
            };
        }

        let radius = (chord * 0.5) / (angle * 0.5).sin().abs();
        let r_sign = angle.signum();
        let h = (radius * radius - chord * chord * 0.25).max(0.0).sqrt();
        let center = Point::new(
            (start.x + end.x) * 0.5 + (-dy / chord) * h * r_sign,
            (start.y + end.y) * 0.5 + (dx / chord) * h * r_sign,
        );

        let sa = (start.y - center.y).atan2(start.x - center.x);
        let ea = (end.y - center.y).atan2(end.x - center.x);
        let mut sweep = ea - sa;
        if angle > 0.0 && sweep < 0.0 {
            sweep += 2.0 * std::f64::consts::PI;
        } else if angle < 0.0 && sweep > 0.0 {
            sweep -= 2.0 * std::f64::consts::PI;
        }

        Self::Arc {
            center,
            radius,
            start_angle: sa,
            sweep_angle: sweep,
            head_length,
            head_width,
            body_width,
        }
    }

    /// Length of the arrow's spine from tail to tip.
    pub fn length(&self) -> f64 {
        match self {
            Self::Straight { start, end, .. } => start.distance(*end),
            Self::Arc {
                radius,
                sweep_angle,
                ..
            } => radius.abs() * sweep_angle.abs(),
        }
    }

    /// The complete arrow silhouette.
    pub fn path(&self) -> BezPath {
        match *self {
            Self::Straight {
                start,
                end,
                head_length,
                head_width,
                body_width,
            } => straight_arrow_path(start, end, head_length, head_width, body_width),
            Self::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
                head_length,
                head_width,
                body_width,
            } => arc_arrow_path(
                center,
                radius,
                start_angle,
                sweep_angle,
                head_length,
                head_width,
                body_width,
            ),
        }
    }

    /// The arrow grown to `progress` of its spine length.
    ///
    /// The tail stays fixed and the tip travels along the spine (following
    /// the arc for curved arrows, with the head turned along the tangent).
    /// Over the first head length the head emerges from the tail with its
    /// proportions intact; afterwards it keeps its authored size while the
    /// shaft extends behind it. Unlike a uniform scale about the tail, the
    /// head and body are never distorted. Progress is clamped to `[0, 1]`;
    /// `0` yields an empty path and `1` yields exactly [`Self::path`].
    pub fn grown_path(&self, progress: f64) -> BezPath {
        let progress = if progress.is_nan() {
            0.0
        } else {
            progress.clamp(0.0, 1.0)
        };
        if progress >= 1.0 {
            return self.path();
        }
        let length = self.length();
        if progress <= 0.0 || length <= f64::EPSILON {
            return BezPath::new();
        }

        let visible = length * progress;
        match *self {
            Self::Straight {
                start,
                end,
                head_length,
                head_width,
                body_width,
            } => {
                let intro = head_length.min(length);
                let s = if intro > 0.0 {
                    (visible / intro).min(1.0)
                } else {
                    1.0
                };
                let tip = start + (end - start) * progress;
                straight_arrow_path(start, tip, head_length * s, head_width * s, body_width * s)
            }
            Self::Arc {
                center,
                radius,
                start_angle,
                sweep_angle,
                head_length,
                head_width,
                body_width,
            } => {
                // The arc builder caps the head at half the sweep.
                let intro = head_length.min(length * 0.5);
                let s = if intro > 0.0 {
                    (visible / intro).min(1.0)
                } else {
                    1.0
                };
                arc_arrow_path(
                    center,
                    radius,
                    start_angle,
                    sweep_angle * progress,
                    head_length * s,
                    head_width * s,
                    body_width * s,
                )
            }
        }
    }
}

/// Single closed subpath shaped like a straight arrow: a rectangular body
/// capped by a wider triangular head whose tip is `end`.
pub fn straight_arrow_path(
    start: Point,
    end: Point,
    head_length: f64,
    head_width: f64,
    body_width: f64,
) -> BezPath {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();

    let head_len = head_length;
    let head_half_width = head_width / 2.0;
    let body_half_t = body_width / 2.0;

    let mut path = BezPath::new();
    if len > 0.0 {
        let ux = dx / len;
        let uy = dy / len;
        let perp_x = -uy;
        let perp_y = ux;

        let base_x = end.x - ux * head_len;
        let base_y = end.y - uy * head_len;

        let start_top_x = start.x - perp_x * body_half_t;
        let start_top_y = start.y - perp_y * body_half_t;
        let start_bot_x = start.x + perp_x * body_half_t;
        let start_bot_y = start.y + perp_y * body_half_t;

        let shoulder_top_x = base_x - perp_x * body_half_t;
        let shoulder_top_y = base_y - perp_y * body_half_t;
        let shoulder_bot_x = base_x + perp_x * body_half_t;
        let shoulder_bot_y = base_y + perp_y * body_half_t;

        let h1_x = base_x - perp_x * head_half_width;
        let h1_y = base_y - perp_y * head_half_width;
        let h2_x = base_x + perp_x * head_half_width;
        let h2_y = base_y + perp_y * head_half_width;

        // Single closed subpath: pentagonal arrow silhouette.
        // The fill covers the whole shape so the body is just as
        // visible as the head after PathCompletion reaches 1.0.
        path.move_to(Point::new(start_top_x, start_top_y));
        path.line_to(Point::new(shoulder_top_x, shoulder_top_y));
        path.line_to(Point::new(h1_x, h1_y));
        path.line_to(end);
        path.line_to(Point::new(h2_x, h2_y));
        path.line_to(Point::new(shoulder_bot_x, shoulder_bot_y));
        path.line_to(Point::new(start_bot_x, start_bot_y));
        path.close_path();
    }
    path
}

/// Closed curved-arrow silhouette along a circular arc, tip at the end of the
/// sweep. On arcs shorter than twice the head length the head shortens to
/// half the sweep, and on small radii its width stays inside the circle.
/// Degenerate radii or sweeps yield an empty path.
pub fn arc_arrow_path(
    center: Point,
    radius: f64,
    start_angle: f64,
    sweep_angle: f64,
    head_length: f64,
    head_width: f64,
    body_width: f64,
) -> BezPath {
    let radius = radius.abs();
    let sa = start_angle;
    let end_angle = start_angle + sweep_angle;
    let end = center + Vec2::new(radius * end_angle.cos(), radius * end_angle.sin());

    let mut path = BezPath::new();
    if radius <= f64::EPSILON || sweep_angle.abs() <= f64::EPSILON {
        return path;
    }

    let head_len = head_length;
    let body_half_t = body_width * 0.5;
    // Keep the inner shoulder on the same side of the center as the shaft;
    // this avoids an oversized/inverted fill for small-radius arcs.
    let head_half_width = (head_width * 0.5).min((radius * 0.45).max(body_half_t));

    let sweep = sweep_angle;
    let sweep_sign = sweep.signum();
    let sweep_abs = sweep.abs();
    let head_angle = (head_len / radius).min(sweep_abs * 0.5);
    let shaft_sweep = (sweep_abs - head_angle).max(0.0);
    let sa_shoulder = end_angle - sweep_sign * head_angle;

    // The shaft is a thin closed ribbon. Vector renderers implicitly close
    // open subpaths for filling, which would otherwise turn a large arc into
    // a filled circular sector.
    let r_outer = radius + body_half_t;
    let r_inner = (radius - body_half_t).max(0.0);
    path.move_to(center + Vec2::new(r_outer * sa.cos(), r_outer * sa.sin()));

    let steps = arc_steps(shaft_sweep, 8);
    for i in 0..=steps {
        let a = sa + sweep_sign * shaft_sweep * (i as f64 / steps as f64);
        path.line_to(center + Vec2::new(r_outer * a.cos(), r_outer * a.sin()));
    }

    // The arrowhead tip lies exactly on the requested arc.
    let p_shoulder_outer = center
        + Vec2::new(
            (radius + head_half_width) * sa_shoulder.cos(),
            (radius + head_half_width) * sa_shoulder.sin(),
        );
    path.line_to(p_shoulder_outer);

    // Tip
    path.line_to(end);

    // Inner shoulder of the arrow head
    let p_shoulder_inner = center
        + Vec2::new(
            (radius - head_half_width) * sa_shoulder.cos(),
            (radius - head_half_width) * sa_shoulder.sin(),
        );
    path.line_to(p_shoulder_inner);
    path.line_to(center + Vec2::new(r_inner * sa_shoulder.cos(), r_inner * sa_shoulder.sin()));

    for i in 0..=steps {
        let a = sa_shoulder - sweep_sign * shaft_sweep * (i as f64 / steps as f64);
        path.line_to(center + Vec2::new(r_inner * a.cos(), r_inner * a.sin()));
    }
    path.close_path();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    fn straight() -> ArrowShape {
        ArrowShape::Straight {
            start: Point::new(-1.0, 0.0),
            end: Point::new(2.0, 0.0),
            head_length: 0.3,
            head_width: 0.2,
            body_width: 0.05,
        }
    }

    fn tip_x(path: &BezPath) -> f64 {
        path.bounding_box().x1
    }

    #[test]
    fn grown_path_endpoints_match_empty_and_full_arrow() {
        for shape in [
            straight(),
            ArrowShape::curved(Point::ZERO, Point::new(2.0, 0.0), 1.2, 0.3, 0.2, 0.05),
        ] {
            assert!(shape.grown_path(0.0).elements().is_empty());
            assert!(shape.grown_path(-1.0).elements().is_empty());
            assert!(shape.grown_path(f64::NAN).elements().is_empty());
            assert_eq!(shape.grown_path(1.0), shape.path());
            assert_eq!(shape.grown_path(3.0), shape.path());
        }
    }

    #[test]
    fn straight_growth_keeps_tail_and_moves_tip_along_spine() {
        let shape = straight();
        let half = shape.grown_path(0.5).bounding_box();
        assert!((half.x0 - -1.0).abs() < 1e-9, "tail stays anchored");
        assert!((tip_x(&shape.grown_path(0.5)) - 0.5).abs() < 1e-9);
        assert!(tip_x(&shape.grown_path(0.25)) < tip_x(&shape.grown_path(0.75)));
    }

    #[test]
    fn straight_head_keeps_authored_size_once_emerged() {
        let shape = straight();
        // Past the first head length the head has full width.
        let grown = shape.grown_path(0.5).bounding_box();
        assert!((grown.height() - 0.2).abs() < 1e-9);
        // Inside the first head length it emerges proportionally.
        let early = shape.grown_path(0.05).bounding_box();
        assert!((early.height() - 0.2 * 0.5).abs() < 1e-9);
        assert!((early.width() - 0.15).abs() < 1e-9);
    }

    #[test]
    fn curved_growth_follows_the_arc() {
        let shape = ArrowShape::curved(
            Point::new(-1.0, 0.0),
            Point::new(1.0, 0.0),
            std::f64::consts::PI,
            0.2,
            0.15,
            0.04,
        );
        let ArrowShape::Arc { center, radius, .. } = shape else {
            panic!("expected an arc arrow");
        };
        // Every vertex of a partially grown curved arrow stays within the
        // head half-width of the circle, so the tip never cuts the chord.
        for progress in [0.1, 0.4, 0.7, 0.95] {
            let path = shape.grown_path(progress);
            assert!(!path.elements().is_empty());
            for el in path.elements() {
                if let Some(p) = el.end_point() {
                    assert!((p.distance(center) - radius).abs() <= 0.075 + 1e-9);
                }
            }
        }
    }

    #[test]
    fn negligible_deflection_is_straight() {
        let shape = ArrowShape::curved(Point::ZERO, Point::new(1.0, 0.0), 0.0, 0.2, 0.1, 0.02);
        assert!(matches!(shape, ArrowShape::Straight { .. }));
        assert!((shape.length() - 1.0).abs() < 1e-12);
    }
}
