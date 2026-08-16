//! Bézier path utilities used by the Write / Create / drawing animations.
//!
//! These functions operate on `kurbo::BezPath` and compute arc-length-based
//! sub-segments, which is what the "pen effect" (Manim-style `Write`) needs
//! in order to draw a path progressively along its true length, not its
//! parameter count.
//!
//! Ported from the reference implementation in `crabanim::engine::geometry`.

#[allow(unused_imports)]
use kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point, Shape};

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
/// - `alpha == 0.0` returns an empty path so zero-progress strokes cannot
///   leak a cap or antialiased pixel into deterministic snapshots.
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
        return BezPath::new();
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
        return BezPath::new();
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
                    PathEl::QuadTo(p1, p2) => {
                        PathSeg::Quad(kurbo::QuadBez::new(current_pos, p1, p2))
                    }
                    PathEl::CurveTo(p1, p2, p3) => {
                        PathSeg::Cubic(kurbo::CubicBez::new(current_pos, p1, p2, p3))
                    }
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
                } else if current_length >= target_start
                    && current_length <= target_end
                    && next_length >= target_end
                {
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

/// Interpolates two paths after matching and normalizing their contours.
///
/// Contours are paired by geometry instead of raw SVG order. Their winding
/// and starting points are aligned, while unmatched contours shrink to or
/// grow from a point. This prevents crossed outlines and halfway popping in
/// text and math morphs.
pub fn interpolate_paths(a: &BezPath, b: &BezPath, t: f64) -> BezPath {
    if t <= 0.0 {
        return a.clone();
    }
    if t >= 1.0 {
        return b.clone();
    }

    interpolate_paths_continuous(a, b, t)
}

/// Interpolates paths without replacing overshooting values with an endpoint.
///
/// Active animations should use this variant so easing curves such as
/// [`RateFunc::Spring`](crate::RateFunc::Spring) keep the same normalized path
/// representation while crossing `t = 1`. The animation system is responsible
/// for assigning the exact target path once the clip has actually completed.
pub fn interpolate_paths_continuous(a: &BezPath, b: &BezPath, t: f64) -> BezPath {
    // Fallback: Split into subpaths, discretize each pair into a polyline, and lerp.
    // This preserves multi-contour glyphs (e.g. "o" has an outer ring + inner hole).
    let subs_a: Vec<_> = split_subpaths(a)
        .into_iter()
        .map(|path| SampledContour::new(path, 64))
        .collect();
    let subs_b: Vec<_> = split_subpaths(b)
        .into_iter()
        .map(|path| SampledContour::new(path, 64))
        .collect();

    if subs_a.is_empty() || subs_b.is_empty() {
        return if t < 0.5 { a.clone() } else { b.clone() };
    }

    let mut result = BezPath::new();

    for (source, target) in match_contours(&subs_a, &subs_b) {
        let (source_points, target_points, closed) = match (source, target) {
            (Some(source_idx), Some(target_idx)) => {
                let source = &subs_a[source_idx];
                let target = &subs_b[target_idx];
                (
                    source.points.clone(),
                    align_points(&source.points, &target.points, source.closed),
                    source.closed || target.closed,
                )
            }
            (Some(source_idx), None) => {
                let source = &subs_a[source_idx];
                (
                    source.points.clone(),
                    vec![nearest_center(source.center, &subs_b); source.points.len()],
                    source.closed,
                )
            }
            (None, Some(target_idx)) => {
                let target = &subs_b[target_idx];
                (
                    vec![nearest_center(target.center, &subs_a); target.points.len()],
                    target.points.clone(),
                    target.closed,
                )
            }
            (None, None) => continue,
        };

        for (idx, (source, target)) in source_points.iter().zip(&target_points).enumerate() {
            let point = source.lerp(*target, t);
            if idx == 0 {
                result.move_to(point);
            } else {
                result.line_to(point);
            }
        }
        if closed {
            result.close_path();
        }
    }

    result
}

/// Splits a `BezPath` into its constituent subpaths at `MoveTo` boundaries.
fn split_subpaths(path: &BezPath) -> Vec<BezPath> {
    let mut subpaths = Vec::new();
    let mut current = BezPath::new();
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                if !current.elements().is_empty() {
                    subpaths.push(current);
                }
                current = BezPath::new();
                current.move_to(p);
            }
            other => current.push(other),
        }
    }
    if !current.elements().is_empty() {
        subpaths.push(current);
    }
    subpaths
}

#[derive(Debug)]
struct SampledContour {
    points: Vec<Point>,
    center: Point,
    area: f64,
    closed: bool,
}

impl SampledContour {
    fn new(path: BezPath, sample_count: usize) -> Self {
        let closed = path.elements().contains(&PathEl::ClosePath);
        let denominator = if closed {
            sample_count
        } else {
            sample_count.saturating_sub(1).max(1)
        } as f64;
        let points: Vec<_> = (0..sample_count)
            .map(|idx| get_point_at_alpha(&path, idx as f64 / denominator))
            .collect();
        let center = average_point(&points);
        let area = signed_area(&points);
        Self {
            points,
            center,
            area,
            closed,
        }
    }
}

fn average_point(points: &[Point]) -> Point {
    if points.is_empty() {
        return Point::ZERO;
    }
    let (x, y) = points
        .iter()
        .fold((0.0, 0.0), |(x, y), point| (x + point.x, y + point.y));
    Point::new(x / points.len() as f64, y / points.len() as f64)
}

fn signed_area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f64>()
        * 0.5
}

fn distance_squared(a: Point, b: Point) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

fn contour_cost(a: &SampledContour, b: &SampledContour) -> f64 {
    let center_cost = distance_squared(a.center, b.center);
    let area_scale = a.area.abs().max(b.area.abs()).max(1.0);
    let area_cost = (a.area.abs() - b.area.abs()).abs() / area_scale;
    let winding_cost = if a.area.signum() == b.area.signum() {
        0.0
    } else {
        0.25
    };
    center_cost + area_cost * 100.0 + winding_cost * 100.0
}

fn match_contours(
    a: &[SampledContour],
    b: &[SampledContour],
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut unmatched_a: Vec<_> = (0..a.len()).collect();
    let mut unmatched_b: Vec<_> = (0..b.len()).collect();
    let mut matches = Vec::with_capacity(a.len().max(b.len()));

    while !unmatched_a.is_empty() && !unmatched_b.is_empty() {
        let mut best = (0, 0, f64::INFINITY);
        for (a_pos, &a_idx) in unmatched_a.iter().enumerate() {
            for (b_pos, &b_idx) in unmatched_b.iter().enumerate() {
                let cost = contour_cost(&a[a_idx], &b[b_idx]);
                if cost < best.2 {
                    best = (a_pos, b_pos, cost);
                }
            }
        }
        let a_idx = unmatched_a.remove(best.0);
        let b_idx = unmatched_b.remove(best.1);
        matches.push((Some(a_idx), Some(b_idx)));
    }

    matches.extend(unmatched_a.into_iter().map(|idx| (Some(idx), None)));
    matches.extend(unmatched_b.into_iter().map(|idx| (None, Some(idx))));
    matches
}

fn nearest_center(center: Point, contours: &[SampledContour]) -> Point {
    contours
        .iter()
        .min_by(|a, b| {
            distance_squared(center, a.center).total_cmp(&distance_squared(center, b.center))
        })
        .map(|contour| contour.center)
        .unwrap_or(center)
}

fn point_alignment_cost(a: &[Point], b: &[Point]) -> f64 {
    a.iter().zip(b).map(|(a, b)| distance_squared(*a, *b)).sum()
}

fn align_points(source: &[Point], target: &[Point], closed: bool) -> Vec<Point> {
    if source.len() != target.len() || source.is_empty() {
        return target.to_vec();
    }

    if !closed {
        let mut candidates = vec![target.to_vec()];
        let mut reversed = target.to_vec();
        reversed.reverse();
        candidates.push(reversed);
        return candidates
            .into_iter()
            .min_by(|a, b| {
                point_alignment_cost(source, a).total_cmp(&point_alignment_cost(source, b))
            })
            .unwrap_or_default();
    }

    // Non-zero filling depends on the relative winding of every closed
    // contour. Reversing each target contour independently to minimize a
    // pairwise match can make the inner contour of an "o" agree with its
    // outer ring, temporarily turning the glyph into a filled disk. Keep the
    // target winding intact and optimize only the cyclic start point.
    let candidate = target.to_vec();
    let mut best = candidate.clone();
    let mut best_cost = f64::INFINITY;
    for offset in 0..candidate.len() {
        let rotated: Vec<_> = candidate
            .iter()
            .cycle()
            .skip(offset)
            .take(candidate.len())
            .copied()
            .collect();
        let cost = point_alignment_cost(source, &rotated);
        if cost < best_cost {
            best_cost = cost;
            best = rotated;
        }
    }
    best
}

#[cfg(test)]
mod morph_tests {
    use super::*;

    fn square(x: f64, opposite_start: bool) -> BezPath {
        let mut path = BezPath::new();
        if opposite_start {
            path.move_to((x + 10.0, 10.0));
            path.line_to((x, 10.0));
            path.line_to((x, 0.0));
            path.line_to((x + 10.0, 0.0));
        } else {
            path.move_to((x, 0.0));
            path.line_to((x + 10.0, 0.0));
            path.line_to((x + 10.0, 10.0));
            path.line_to((x, 10.0));
        }
        path.close_path();
        path
    }

    fn centered_square(size: f64, reversed: bool) -> BezPath {
        let half = size / 2.0;
        let mut path = BezPath::new();
        path.move_to((-half, -half));
        if reversed {
            path.line_to((-half, half));
            path.line_to((half, half));
            path.line_to((half, -half));
        } else {
            path.line_to((half, -half));
            path.line_to((half, half));
            path.line_to((-half, half));
        }
        path.close_path();
        path
    }

    #[test]
    fn morph_preserves_exact_endpoints() {
        let a = square(0.0, false);
        let b = square(20.0, true);
        assert_eq!(interpolate_paths(&a, &b, 0.0), a);
        assert_eq!(interpolate_paths(&a, &b, 1.0), b);
    }

    #[test]
    fn morph_aligns_closed_contour_start_points() {
        let a = square(0.0, false);
        let b = square(0.0, true);
        let midpoint = interpolate_paths(&a, &b, 0.5);
        let sampled = SampledContour::new(midpoint, 64);
        assert!(
            sampled.area.abs() > 90.0,
            "area collapsed to {}",
            sampled.area
        );
    }

    #[test]
    fn morph_matches_reordered_contours_by_position() {
        let mut a = square(0.0, false);
        a.extend(square(100.0, false));
        let mut b = square(100.0, true);
        b.extend(square(0.0, true));

        let midpoint = interpolate_paths(&a, &b, 0.5);
        let mut centers: Vec<_> = split_subpaths(&midpoint)
            .into_iter()
            .map(|path| SampledContour::new(path, 32).center.x)
            .collect();
        centers.sort_by(f64::total_cmp);
        assert!(centers[0] < 10.0);
        assert!(centers[1] > 100.0);
    }

    #[test]
    fn morph_keeps_unmatched_contours_continuous() {
        let a = square(0.0, false);
        let mut b = square(0.0, false);
        b.extend(square(30.0, false));

        for t in [0.25, 0.5, 0.75] {
            assert_eq!(split_subpaths(&interpolate_paths(&a, &b, t)).len(), 2);
        }
    }

    #[test]
    fn active_morph_keeps_one_representation_through_spring_overshoot() {
        let a = square(0.0, false);
        let b = square(20.0, true);

        let before = interpolate_paths_continuous(&a, &b, 0.999);
        let after = interpolate_paths_continuous(&a, &b, 1.001);

        assert_eq!(before.elements().len(), after.elements().len());
        assert_ne!(after, b, "overshoot must not switch to the exact endpoint");
    }

    #[test]
    fn closed_contours_keep_hole_winding_during_morph() {
        let mut a = centered_square(10.0, false);
        a.extend(centered_square(4.0, true));
        // The target globally reverses both windings. Its fill topology is
        // still a ring, and the active morph must preserve that topology.
        let mut b = centered_square(12.0, true);
        b.extend(centered_square(5.0, false));

        for t in [0.25, 0.5, 0.999, 1.001] {
            let morphed = interpolate_paths_continuous(&a, &b, t);
            let areas: Vec<_> = split_subpaths(&morphed)
                .into_iter()
                .map(|path| SampledContour::new(path, 64).area)
                .collect();
            assert_eq!(areas.len(), 2);
            assert!(
                areas[0].signum() != areas[1].signum(),
                "hole winding collapsed at t={t}: {areas:?}"
            );
        }
    }

    #[test]
    fn test_circle_get_point_at_alpha() {
        let circle_path = kurbo::Circle::new(kurbo::Point::ZERO, 100.0).to_path(0.1);
        let p0 = get_point_at_alpha(&circle_path, 0.0);
        let p25 = get_point_at_alpha(&circle_path, 0.25);
        let p50 = get_point_at_alpha(&circle_path, 0.5);
        let p75 = get_point_at_alpha(&circle_path, 0.75);
        let p100 = get_point_at_alpha(&circle_path, 1.0);
        assert!(
            (p0.x - 100.0).abs() < 1e-3 && p0.y.abs() < 1e-3,
            "p0 should be (100,0), got {p0:?}"
        );
        assert!(
            p25.x.abs() < 1e-3 && (p25.y - 100.0).abs() < 1e-3,
            "p25 should be (0,100), got {p25:?}"
        );
        assert!(
            (p50.x + 100.0).abs() < 1e-3 && p50.y.abs() < 1e-3,
            "p50 should be (-100,0), got {p50:?}"
        );
        assert!(
            p75.x.abs() < 1e-3 && (p75.y + 100.0).abs() < 1e-3,
            "p75 should be (0,-100), got {p75:?}"
        );
        assert!(
            (p100.x - 100.0).abs() < 1e-3 && p100.y.abs() < 1e-3,
            "p100 should be (100,0), got {p100:?}"
        );
    }
}
