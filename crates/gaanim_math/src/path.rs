//! Bézier path utilities used by the Write / Create / drawing animations.
//!
//! These functions operate on `kurbo::BezPath` and compute arc-length-based
//! sub-segments, which is what the "pen effect" (Manim-style `Write`) needs
//! in order to draw a path progressively along its true length, not its
//! parameter count.
//!
//! Ported from the reference implementation in `crabanim::engine::geometry`.

use kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point};

/// Total arc length of a Bézier path, computed segment-by-segment.
///
/// Uses `arclen(0.1)` accuracy which is the kurbo default. The
/// accumulated length can occasionally exceed the "true" length by a few
/// ulps due to floating-point integration; the Write animation is
/// tolerant of that (see `get_subpath` and the `target_length + 0.1`
/// slack used by `get_completed_subpaths` in the reference impl).
pub fn get_path_length(path: &BezPath) -> f64 {
    let mut length = 0.0;
    for segment in path.segments() {
        length += match segment {
            PathSeg::Line(l) => l.arclen(0.1),
            PathSeg::Quad(q) => q.arclen(0.1),
            PathSeg::Cubic(c) => c.arclen(0.1),
        };
    }
    length
}

/// A trimmed sub-segment of a `BezPath` from the start up to a fractional
/// arc-length position `alpha` in `[0.0, 1.0]`.
///
/// - `alpha == 0.0` returns either an empty path or, if the original
///   starts with a `MoveTo`, just that `MoveTo` (so the result is a
///   valid path with a starting point).
/// - `alpha == 1.0` returns the full path (cloned).
/// - Intermediate values trim each sub-path to `alpha` of its **own**
///   arc length. This means all sub-paths (e.g. the outer and inner
///   contour of an "O", or the four pen strokes of an "E") are revealed
///   simultaneously, proportional to `alpha`, producing the natural
///   Manim-style "Write" effect where a pen appears to trace every
///   stroke at once. Earlier implementations concatenated all sub-paths
///   into a single arc-length and trimmed globally, which made the first
///   sub-path appear complete before the pen ever reached the next one.
pub fn get_subpath(path: &BezPath, alpha: f64) -> BezPath {
    if alpha >= 1.0 {
        return path.clone();
    }
    if alpha <= 0.0 {
        let mut result = BezPath::new();
        if let Some(PathEl::MoveTo(p)) = path.elements().first() {
            result.move_to(*p);
        }
        return result;
    }

    let mut result = BezPath::new();
    let mut current_subpath: Vec<PathEl> = Vec::new();

    let flush = |sub: &[PathEl], result: &mut BezPath, alpha: f64| {
        if sub.is_empty() {
            return;
        }
        let sub_path = BezPath::from_vec(sub.to_vec());
        let trimmed = get_subpath_proportional(&sub_path, alpha);
        for el in trimmed.elements() {
            result.push(*el);
        }
    };

    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                flush(&current_subpath, &mut result, alpha);
                current_subpath.clear();
                current_subpath.push(PathEl::MoveTo(p));
            }
            other => current_subpath.push(other),
        }
    }
    flush(&current_subpath, &mut result, alpha);

    result
}

/// Trims a single contiguous sub-path (no `MoveTo` boundary) to `alpha`
/// of its own arc length. This is the building block of the
/// per-subpath proportional trimming used by `get_subpath`.
fn get_subpath_proportional(path: &BezPath, alpha: f64) -> BezPath {
    if alpha >= 1.0 {
        return path.clone();
    }
    if alpha <= 0.0 {
        let mut result = BezPath::new();
        if let Some(PathEl::MoveTo(p)) = path.elements().first() {
            result.move_to(*p);
        }
        return result;
    }

    let total_length = get_path_length(path);
    let target_length = total_length * alpha;

    let mut current_length = 0.0;
    let mut result = BezPath::new();

    let mut current_pos = Point::default();
    let mut start_of_subpath = Point::default();

    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                current_pos = p;
                start_of_subpath = p;
                result.move_to(p);
            }
            PathEl::LineTo(p) => {
                let segment = PathSeg::Line(kurbo::Line::new(current_pos, p));
                let seg_len = segment.arclen(0.1);

                if current_length + seg_len <= target_length {
                    result.line_to(p);
                    current_length += seg_len;
                    current_pos = p;
                } else {
                    let remaining = target_length - current_length;
                    let t = segment.inv_arclen(remaining, 0.1);
                    let trimmed = segment.subsegment(0.0..t);
                    if let PathSeg::Line(l) = trimmed {
                        result.line_to(l.p1);
                    }
                    break;
                }
            }
            PathEl::QuadTo(p1, p2) => {
                let segment = PathSeg::Quad(kurbo::QuadBez::new(current_pos, p1, p2));
                let seg_len = segment.arclen(0.1);

                if current_length + seg_len <= target_length {
                    result.quad_to(p1, p2);
                    current_length += seg_len;
                    current_pos = p2;
                } else {
                    let remaining = target_length - current_length;
                    let t = segment.inv_arclen(remaining, 0.1);
                    let trimmed = segment.subsegment(0.0..t);
                    if let PathSeg::Quad(q) = trimmed {
                        result.quad_to(q.p1, q.p2);
                    }
                    break;
                }
            }
            PathEl::CurveTo(p1, p2, p3) => {
                let segment = PathSeg::Cubic(kurbo::CubicBez::new(current_pos, p1, p2, p3));
                let seg_len = segment.arclen(0.1);

                if current_length + seg_len <= target_length {
                    result.curve_to(p1, p2, p3);
                    current_length += seg_len;
                    current_pos = p3;
                } else {
                    let remaining = target_length - current_length;
                    let t = segment.inv_arclen(remaining, 0.1);
                    let trimmed = segment.subsegment(0.0..t);
                    if let PathSeg::Cubic(c) = trimmed {
                        result.curve_to(c.p1, c.p2, c.p3);
                    }
                    break;
                }
            }
            PathEl::ClosePath => {
                let segment = PathSeg::Line(kurbo::Line::new(current_pos, start_of_subpath));
                let seg_len = segment.arclen(0.1);

                if current_length + seg_len <= target_length {
                    result.close_path();
                    current_length += seg_len;
                    current_pos = start_of_subpath;
                } else {
                    let remaining = target_length - current_length;
                    let t = segment.inv_arclen(remaining, 0.1);
                    let trimmed = segment.subsegment(0.0..t);
                    if let PathSeg::Line(l) = trimmed {
                        result.line_to(l.p1);
                    }
                    break;
                }
            }
        }
    }

    result
}

/// Point at parameter `alpha` in `[0.0, 1.0]` along a `BezPath`.
///
/// Used by the tip-glow effect to position a small "pen tip" entity
/// at the current end of the draw progression.
pub fn get_point_at_alpha(path: &BezPath, alpha: f64) -> Point {
    if path.elements().is_empty() {
        return Point::default();
    }

    if alpha <= 0.0 {
        if let Some(PathEl::MoveTo(p)) = path.elements().first() {
            return *p;
        }
        return Point::default();
    }

    let subpath = get_subpath(path, alpha);

    let mut last_point = Point::default();
    for el in subpath.elements() {
        match *el {
            PathEl::MoveTo(p) => last_point = p,
            PathEl::LineTo(p) => last_point = p,
            PathEl::QuadTo(_, p) => last_point = p,
            PathEl::CurveTo(_, _, p) => last_point = p,
            PathEl::ClosePath => {}
        }
    }
    last_point
}

/// Trims a path to a range [from_alpha, to_alpha] proportionally.
/// This is used to extract a sliding window (destello) along a curve for ShowPassingFlash.
pub fn get_subpath_range(path: &BezPath, from_alpha: f64, to_alpha: f64) -> BezPath {
    let from_alpha = from_alpha.clamp(0.0, 1.0);
    let to_alpha = to_alpha.clamp(0.0, 1.0);
    if from_alpha >= to_alpha {
        let mut result = BezPath::new();
        if let Some(PathEl::MoveTo(p)) = path.elements().first() {
            result.move_to(*p);
        }
        return result;
    }
    
    let mut result = BezPath::new();
    let mut current_subpath: Vec<PathEl> = Vec::new();

    let flush = |sub: &[PathEl], result: &mut BezPath, from: f64, to: f64| {
        if sub.is_empty() {
            return;
        }
        let sub_path = BezPath::from_vec(sub.to_vec());
        let trimmed = get_subpath_proportional_range(&sub_path, from, to);
        for el in trimmed.elements() {
            result.push(*el);
        }
    };

    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                flush(&current_subpath, &mut result, from_alpha, to_alpha);
                current_subpath.clear();
                current_subpath.push(PathEl::MoveTo(p));
            }
            other => current_subpath.push(other),
        }
    }
    flush(&current_subpath, &mut result, from_alpha, to_alpha);

    result
}

fn get_subpath_proportional_range(path: &BezPath, from_alpha: f64, to_alpha: f64) -> BezPath {
    let total_length = get_path_length(path);
    let target_start = total_length * from_alpha;
    let target_end = total_length * to_alpha;

    let mut current_length = 0.0;
    let mut result = BezPath::new();
    let mut current_pos = Point::default();
    let mut started = false;

    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                current_pos = p;
            }
            _ => {
                let segment = match *el {
                    PathEl::LineTo(p) => PathSeg::Line(kurbo::Line::new(current_pos, p)),
                    PathEl::QuadTo(p1, p2) => PathSeg::Quad(kurbo::QuadBez::new(current_pos, p1, p2)),
                    PathEl::CurveTo(p1, p2, p3) => PathSeg::Cubic(kurbo::CubicBez::new(current_pos, p1, p2, p3)),
                    PathEl::ClosePath => PathSeg::Line(kurbo::Line::new(current_pos, current_pos)),
                    _ => unreachable!(),
                };
                let seg_len = segment.arclen(0.1);
                let next_length = current_length + seg_len;
                let next_pos = segment.end();

                if next_length <= target_start {
                    // Completamente antes
                } else if current_length <= target_start && next_length <= target_end {
                    let start_rem = target_start - current_length;
                    let t0 = segment.inv_arclen(start_rem, 0.1);
                    let trimmed = segment.subsegment(t0..1.0);
                    if !started {
                        result.move_to(trimmed.start());
                        started = true;
                    }
                    push_segment_to_path(&mut result, &trimmed);
                } else if current_length >= target_start && next_length <= target_end {
                    if !started {
                        result.move_to(segment.start());
                        started = true;
                    }
                    push_segment_to_path(&mut result, &segment);
                } else if current_length >= target_start && current_length <= target_end && next_length >= target_end {
                    let end_rem = target_end - current_length;
                    let t1 = segment.inv_arclen(end_rem, 0.1);
                    let trimmed = segment.subsegment(0.0..t1);
                    if !started {
                        result.move_to(trimmed.start());
                    }
                    push_segment_to_path(&mut result, &trimmed);
                    break;
                } else if current_length <= target_start && next_length >= target_end {
                    let start_rem = target_start - current_length;
                    let end_rem = target_end - current_length;
                    let t0 = segment.inv_arclen(start_rem, 0.1);
                    let t1 = segment.inv_arclen(end_rem, 0.1);
                    let trimmed = segment.subsegment(t0..t1);
                    result.move_to(trimmed.start());
                    push_segment_to_path(&mut result, &trimmed);
                    break;
                }

                current_length = next_length;
                current_pos = next_pos;
            }
        }
    }

    result
}

fn push_segment_to_path(path: &mut BezPath, seg: &PathSeg) {
    match *seg {
        PathSeg::Line(l) => path.line_to(l.p1),
        PathSeg::Quad(q) => path.quad_to(q.p1, q.p2),
        PathSeg::Cubic(c) => path.curve_to(c.p1, c.p2, c.p3),
    }
}

