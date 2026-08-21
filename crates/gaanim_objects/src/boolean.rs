//! Boolean operations on BezPaths using `i_overlay`.
//!
//! Operations supported: union, intersection, difference (subtract), and exclusion
//! (symmetric difference). Bezier curves are flattened to polylines before
//! processing because `i_overlay` only operates on linear contours.

use gaanim_core::kurbo::{self, PathEl, Shape};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

/// Boolean operation kind. Maps 1:1 to [`i_overlay::core::overlay_rule::OverlayRule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    /// Area in either subject or clip (logical OR).
    Union,
    /// Area in both subject and clip (logical AND).
    Intersection,
    /// Area in subject but not in clip (subject - clip).
    Difference,
    /// Area in subject or clip but not both (logical XOR).
    Exclusion,
}

/// Interior interpretation for boolean input contours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BooleanFillRule {
    #[default]
    NonZero,
    EvenOdd,
}

impl BooleanFillRule {
    fn as_overlay(self) -> FillRule {
        match self {
            Self::NonZero => FillRule::NonZero,
            Self::EvenOdd => FillRule::EvenOdd,
        }
    }
}

impl BooleanOp {
    pub fn overlay_rule(self) -> OverlayRule {
        match self {
            BooleanOp::Union => OverlayRule::Union,
            BooleanOp::Intersection => OverlayRule::Intersect,
            BooleanOp::Difference => OverlayRule::Difference,
            BooleanOp::Exclusion => OverlayRule::Xor,
        }
    }
}

/// Flattening tolerance passed to `kurbo::flatten`. Smaller = more accurate,
/// larger = faster. 0.25 is a good balance for screen-space rendering.
const FLATTEN_TOLERANCE: f64 = 0.25;

/// Result of a boolean operation. Each element is one outer contour
/// (holes are encoded as additional subpaths in the same `BezPath`).
#[derive(Debug, Clone)]
pub struct BooleanResult {
    pub paths: Vec<kurbo::BezPath>,
    pub bounds: kurbo::Rect,
}

/// Apply a boolean op between two BezPaths. Each path may have multiple
/// subpaths (outer rings + holes). Returns one or more resulting paths.
pub fn apply(a: &kurbo::BezPath, b: &kurbo::BezPath, op: BooleanOp) -> BooleanResult {
    apply_with_options(a, b, op, FLATTEN_TOLERANCE, BooleanFillRule::NonZero)
}

/// Apply an operation with explicit flattening tolerance and interior rule.
pub fn apply_with_options(
    a: &kurbo::BezPath,
    b: &kurbo::BezPath,
    op: BooleanOp,
    tolerance: f64,
    rule: BooleanFillRule,
) -> BooleanResult {
    let tolerance = if tolerance.is_finite() && tolerance > 0.0 {
        tolerance
    } else {
        FLATTEN_TOLERANCE
    };
    let subj = bezpath_to_shape_with_tolerance(a, tolerance);
    let clip = bezpath_to_shape_with_tolerance(b, tolerance);

    if subj.is_empty() || clip.is_empty() {
        let keep = match op {
            BooleanOp::Union | BooleanOp::Exclusion => {
                if subj.is_empty() {
                    b
                } else {
                    a
                }
            }
            BooleanOp::Difference if !subj.is_empty() => a,
            BooleanOp::Intersection | BooleanOp::Difference => {
                return BooleanResult {
                    paths: Vec::new(),
                    bounds: kurbo::Rect::ZERO,
                };
            }
        };
        let bounds = keep.bounding_box();
        return BooleanResult {
            paths: (!keep.elements().is_empty())
                .then(|| keep.clone())
                .into_iter()
                .collect(),
            bounds,
        };
    }

    let shapes = subj.overlay(&clip, op.overlay_rule(), rule.as_overlay());

    let mut paths = Vec::with_capacity(shapes.len());
    let mut union_bounds = kurbo::Rect::ZERO;
    let mut has_bounds = false;
    for shape in shapes {
        let path = shapes_to_bezpath(&shape);
        let b = path.bounding_box();
        if !has_bounds {
            union_bounds = b;
            has_bounds = true;
        } else {
            union_bounds = union_bounds.union(b);
        }
        paths.push(path);
    }

    BooleanResult {
        paths,
        bounds: if has_bounds {
            union_bounds
        } else {
            kurbo::Rect::ZERO
        },
    }
}

/// Convert a single `BezPath` into the shape format expected by i_overlay:
/// one outer ring plus optional holes, all flattened to polylines.
///
/// Returns `Vec<Vec<[f64; 2]>>` = list of contours, where contour[0] is
/// the outer ring and any subsequent contours are holes (with even-odd
/// fill rule).
pub fn bezpath_to_shape(path: &kurbo::BezPath) -> Vec<Vec<[f64; 2]>> {
    bezpath_to_shape_with_tolerance(path, FLATTEN_TOLERANCE)
}

pub fn bezpath_to_shape_with_tolerance(
    path: &kurbo::BezPath,
    tolerance: f64,
) -> Vec<Vec<[f64; 2]>> {
    let subpaths = collect_subpaths(path, tolerance);
    subpaths.into_iter().filter(|c| c.len() >= 3).collect()
}

/// Walk a `BezPath` and collect each closed subpath as a `Vec<[f64; 2]>`,
/// flattening curves along the way.
fn collect_subpaths(path: &kurbo::BezPath, tolerance: f64) -> Vec<Vec<[f64; 2]>> {
    let mut subpaths: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut current: Option<Vec<[f64; 2]>> = None;

    let elements: Vec<PathEl> = path.iter().collect();
    let mut last_pt: Option<kurbo::Point> = None;

    for el in elements {
        match el {
            PathEl::MoveTo(p) => {
                if let Some(buf) = current.take() {
                    subpaths.push(buf);
                }
                let mut buf = Vec::new();
                buf.push([p.x, p.y]);
                current = Some(buf);
                last_pt = Some(p);
            }
            PathEl::LineTo(p) => {
                if current.is_none() {
                    current = Some(Vec::new());
                }
                if let Some(buf) = current.as_mut() {
                    buf.push([p.x, p.y]);
                }
                last_pt = Some(p);
            }
            PathEl::QuadTo(p1, p2) => {
                if let (Some(start), Some(buf)) = (last_pt, current.as_mut()) {
                    extend_with_flattened(buf, start, tolerance, |cb| {
                        cb(PathEl::MoveTo(start));
                        cb(PathEl::QuadTo(p1, p2));
                    });
                }
                last_pt = Some(p2);
            }
            PathEl::CurveTo(p1, p2, p3) => {
                if let (Some(start), Some(buf)) = (last_pt, current.as_mut()) {
                    extend_with_flattened(buf, start, tolerance, |cb| {
                        cb(PathEl::MoveTo(start));
                        cb(PathEl::CurveTo(p1, p2, p3));
                    });
                }
                last_pt = Some(p3);
            }
            PathEl::ClosePath => {
                if let Some(buf) = current.take()
                    && buf.len() >= 3
                {
                    subpaths.push(buf);
                }
                last_pt = None;
            }
        }
    }
    if let Some(buf) = current.take()
        && buf.len() >= 3
    {
        subpaths.push(buf);
    }
    subpaths
}

/// Append points generated by `kurbo::flatten` to `buf`, skipping the
/// initial MoveTo (which is the same as the previous point in the polyline).
fn extend_with_flattened<F: FnOnce(&mut dyn FnMut(PathEl))>(
    buf: &mut Vec<[f64; 2]>,
    _start: kurbo::Point,
    tolerance: f64,
    build: F,
) {
    let mut collected: Vec<PathEl> = Vec::new();
    let mut sink = |el: PathEl| collected.push(el);
    build(&mut sink);
    let mut cb = |el: PathEl| {
        if let PathEl::LineTo(p) = el {
            let point = [p.x, p.y];
            if buf.last() != Some(&point) {
                buf.push(point);
            }
        }
    };
    // `kurbo::flatten` needs the leading MoveTo to establish the curve's
    // current point. Dropping it reduces every quadratic/cubic to its endpoint,
    // which turns circles into diamonds before boolean operations.
    kurbo::flatten(collected.into_iter(), tolerance, &mut cb);
}

/// Convert an i_overlay shape (Vec of contours) back into a `BezPath`.
/// The first contour is the outer ring, the rest are holes.
fn shapes_to_bezpath(shape: &[Vec<[f64; 2]>]) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    for contour in shape {
        if contour.is_empty() {
            continue;
        }
        let first = contour[0];
        path.move_to(kurbo::Point::new(first[0], first[1]));
        for p in &contour[1..] {
            path.line_to(kurbo::Point::new(p[0], p[1]));
        }
        path.close_path();
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_path(x0: f64, y0: f64, x1: f64, y1: f64) -> kurbo::BezPath {
        let mut p = kurbo::BezPath::new();
        p.move_to(kurbo::Point::new(x0, y0));
        p.line_to(kurbo::Point::new(x1, y0));
        p.line_to(kurbo::Point::new(x1, y1));
        p.line_to(kurbo::Point::new(x0, y1));
        p.close_path();
        p
    }

    #[test]
    fn union_of_overlapping_squares() {
        let a = rect_path(0.0, 0.0, 4.0, 4.0);
        let b = rect_path(2.0, 2.0, 6.0, 6.0);
        let r = apply(&a, &b, BooleanOp::Union);
        assert_eq!(r.paths.len(), 1);
        let bb = r.paths[0].bounding_box();
        assert!((bb.x0 - 0.0).abs() < 0.5);
        assert!((bb.x1 - 6.0).abs() < 0.5);
        assert!((bb.y0 - 0.0).abs() < 0.5);
        assert!((bb.y1 - 6.0).abs() < 0.5);
    }

    #[test]
    fn empty_operands_obey_boolean_identities() {
        let a = rect_path(0.0, 0.0, 4.0, 4.0);
        let empty = kurbo::BezPath::new();
        assert_eq!(apply(&a, &empty, BooleanOp::Union).paths.len(), 1);
        assert_eq!(apply(&a, &empty, BooleanOp::Exclusion).paths.len(), 1);
        assert_eq!(apply(&a, &empty, BooleanOp::Difference).paths.len(), 1);
        assert!(apply(&a, &empty, BooleanOp::Intersection).paths.is_empty());
    }

    #[test]
    fn intersection_of_overlapping_squares() {
        let a = rect_path(0.0, 0.0, 4.0, 4.0);
        let b = rect_path(2.0, 2.0, 6.0, 6.0);
        let r = apply(&a, &b, BooleanOp::Intersection);
        assert_eq!(r.paths.len(), 1);
        let bb = r.paths[0].bounding_box();
        assert!((bb.x0 - 2.0).abs() < 0.5);
        assert!((bb.x1 - 4.0).abs() < 0.5);
        assert!((bb.y0 - 2.0).abs() < 0.5);
        assert!((bb.y1 - 4.0).abs() < 0.5);
    }

    #[test]
    fn bezier_circle_is_flattened_to_more_than_its_four_curve_endpoints() {
        let circle = kurbo::Circle::new((0.0, 0.0), 100.0).to_path(0.1);
        let contours = bezpath_to_shape_with_tolerance(&circle, 0.25);

        assert_eq!(contours.len(), 1);
        assert!(
            contours[0].len() > 32,
            "a cubic circle must be flattened into a smooth polygon"
        );

        let band = kurbo::Rect::new(-100.0, -100.0, 100.0, 50.0).to_path(0.1);
        let filled = apply_with_options(
            &circle,
            &band,
            BooleanOp::Intersection,
            0.25,
            BooleanFillRule::NonZero,
        );
        assert_eq!(filled.paths.len(), 1);
        assert!(filled.paths[0].elements().len() > 20);
        assert!((filled.bounds.width() - 200.0).abs() < 0.5);
        assert!((filled.bounds.height() - 150.0).abs() < 0.5);
    }

    #[test]
    fn difference_removes_overlap() {
        let a = rect_path(0.0, 0.0, 4.0, 4.0);
        let b = rect_path(2.0, 2.0, 6.0, 6.0);
        let r = apply(&a, &b, BooleanOp::Difference);
        assert!(!r.paths.is_empty());
    }

    #[test]
    fn exclusion_yields_two_regions() {
        let a = rect_path(0.0, 0.0, 4.0, 4.0);
        let b = rect_path(2.0, 2.0, 6.0, 6.0);
        let r = apply(&a, &b, BooleanOp::Exclusion);
        assert_eq!(r.paths.len(), 2);
    }

    #[test]
    fn disjoint_inputs_produce_separate_paths() {
        let a = rect_path(0.0, 0.0, 1.0, 1.0);
        let b = rect_path(5.0, 5.0, 6.0, 6.0);
        let r = apply(&a, &b, BooleanOp::Union);
        assert_eq!(r.paths.len(), 2);
    }

    #[test]
    fn difference_with_circle() {
        let a = rect_path(0.0, 0.0, 10.0, 10.0);
        let mut b = kurbo::BezPath::new();
        // a circle approximated with cubic beziers
        let circle = kurbo::Circle::new(kurbo::Point::new(5.0, 5.0), 3.0);
        b.extend(circle.path_elements(0.1));
        let r = apply(&a, &b, BooleanOp::Difference);
        assert_eq!(
            r.paths.len(),
            1,
            "rect minus a circle inside should be one path"
        );
    }
}
