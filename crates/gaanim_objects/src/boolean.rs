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
    let subj = bezpath_to_shape(a);
    let clip = bezpath_to_shape(b);

    if subj.is_empty() || clip.is_empty() {
        return BooleanResult {
            paths: Vec::new(),
            bounds: kurbo::Rect::ZERO,
        };
    }

    let shapes = subj.overlay(&clip, op.overlay_rule(), FillRule::EvenOdd);

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
        bounds: if has_bounds { union_bounds } else { kurbo::Rect::ZERO },
    }
}

/// Convert a single `BezPath` into the shape format expected by i_overlay:
/// one outer ring plus optional holes, all flattened to polylines.
///
/// Returns `Vec<Vec<[f64; 2]>>` = list of contours, where contour[0] is
/// the outer ring and any subsequent contours are holes (with even-odd
/// fill rule).
pub fn bezpath_to_shape(path: &kurbo::BezPath) -> Vec<Vec<[f64; 2]>> {
    let subpaths = collect_subpaths(path);
    subpaths
        .into_iter()
        .filter(|c| c.len() >= 3)
        .collect()
}

/// Walk a `BezPath` and collect each closed subpath as a `Vec<[f64; 2]>`,
/// flattening curves along the way.
fn collect_subpaths(path: &kurbo::BezPath) -> Vec<Vec<[f64; 2]>> {
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
                    extend_with_flattened(buf, start, |cb| {
                        cb(PathEl::MoveTo(start));
                        cb(PathEl::QuadTo(p1, p2));
                    });
                }
                last_pt = Some(p2);
            }
            PathEl::CurveTo(p1, p2, p3) => {
                if let (Some(start), Some(buf)) = (last_pt, current.as_mut()) {
                    extend_with_flattened(buf, start, |cb| {
                        cb(PathEl::MoveTo(start));
                        cb(PathEl::CurveTo(p1, p2, p3));
                    });
                }
                last_pt = Some(p3);
            }
            PathEl::ClosePath => {
                if let Some(buf) = current.take()
                    && buf.len() >= 3 {
                        subpaths.push(buf);
                    }
                last_pt = None;
            }
        }
    }
    if let Some(buf) = current.take()
        && buf.len() >= 3 {
            subpaths.push(buf);
        }
    subpaths
}

/// Append points generated by `kurbo::flatten` to `buf`, skipping the
/// initial MoveTo (which is the same as the previous point in the polyline).
fn extend_with_flattened<F: FnOnce(&mut dyn FnMut(PathEl))>(
    buf: &mut Vec<[f64; 2]>,
    _start: kurbo::Point,
    build: F,
) {
    let mut collected: Vec<PathEl> = Vec::new();
    let mut sink = |el: PathEl| collected.push(el);
    build(&mut sink);
    let mut iter = collected.into_iter();
    if let Some(first) = iter.next()
        && let PathEl::MoveTo(p) = first {
            buf.push([p.x, p.y]);
        }
    let mut cb = |el: PathEl| {
        if let PathEl::LineTo(p) = el {
            buf.push([p.x, p.y]);
        }
    };
    kurbo::flatten(iter, FLATTEN_TOLERANCE, &mut cb);
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
        assert_eq!(r.paths.len(), 1, "rect minus a circle inside should be one path");
    }
}
