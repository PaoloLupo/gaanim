//! Bézier path utilities used by the Write / Create / drawing animations.
//!
//! These functions operate on `kurbo::BezPath` and compute arc-length-based
//! sub-segments, which is what the "pen effect" (Manim-style `Write`) needs
//! in order to draw a path progressively along its true length, not its
//! parameter count.
//!
//! Ported from the reference implementation in `crabanim::engine::geometry`.

use gaanim_core::glam::DVec3;
#[allow(unused_imports)]
use kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point, Shape};

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

/// Sample a 3D polyline at normalized arc length.
pub fn get_point_on_polyline(points: &[DVec3], alpha: f64) -> DVec3 {
    let Some(first) = points.first().copied() else {
        return DVec3::ZERO;
    };
    if points.len() == 1 || alpha <= 0.0 {
        return first;
    }
    if alpha >= 1.0 {
        return points.last().copied().unwrap_or(first);
    }
    let lengths = points
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<f64>();
    if total <= f64::EPSILON {
        return first;
    }
    let target = alpha.clamp(0.0, 1.0) * total;
    let mut traversed = 0.0;
    for (index, length) in lengths.into_iter().enumerate() {
        if target <= traversed + length || index + 2 == points.len() {
            let local = if length <= f64::EPSILON {
                0.0
            } else {
                (target - traversed) / length
            };
            return points[index].lerp(points[index + 1], local.clamp(0.0, 1.0));
        }
        traversed += length;
    }
    points.last().copied().unwrap_or(first)
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
        let (source_segments, target_segments, closed) = match (source, target) {
            (Some(source_idx), Some(target_idx)) => {
                let source = &subs_a[source_idx];
                let target = &subs_b[target_idx];
                normalize_contour_pair(source, target)
            }
            (Some(source_idx), None) => {
                let source = &subs_a[source_idx];
                let center = nearest_center(source.center, &subs_b);
                (
                    source.segments.clone(),
                    degenerate_segments(center, source.segments.len()),
                    source.closed,
                )
            }
            (None, Some(target_idx)) => {
                let target = &subs_b[target_idx];
                let center = nearest_center(target.center, &subs_a);
                (
                    degenerate_segments(center, target.segments.len()),
                    target.segments.clone(),
                    target.closed,
                )
            }
            (None, None) => continue,
        };

        let Some((first_source, first_target)) =
            source_segments.first().zip(target_segments.first())
        else {
            continue;
        };
        result.move_to(first_source.p0.lerp(first_target.p0, t));
        for (source, target) in source_segments.iter().zip(&target_segments) {
            result.curve_to(
                source.p1.lerp(target.p1, t),
                source.p2.lerp(target.p2, t),
                source.p3.lerp(target.p3, t),
            );
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
    segments: Vec<CubicBez>,
    center: Point,
    area: f64,
    closed: bool,
}

impl SampledContour {
    fn new(path: BezPath, sample_count: usize) -> Self {
        let closed = path.elements().contains(&PathEl::ClosePath);
        let segments = cubic_segments(&path);
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
            segments,
            center,
            area,
            closed,
        }
    }
}

fn cubic_segments(path: &BezPath) -> Vec<CubicBez> {
    path.segments()
        .map(|segment| match segment {
            PathSeg::Line(line) => CubicBez::new(
                line.p0,
                line.p0.lerp(line.p1, 1.0 / 3.0),
                line.p0.lerp(line.p1, 2.0 / 3.0),
                line.p1,
            ),
            PathSeg::Quad(quad) => CubicBez::new(
                quad.p0,
                quad.p0.lerp(quad.p1, 2.0 / 3.0),
                quad.p2.lerp(quad.p1, 2.0 / 3.0),
                quad.p2,
            ),
            PathSeg::Cubic(cubic) => cubic,
        })
        .collect()
}

fn normalize_contour_pair(
    source: &SampledContour,
    target: &SampledContour,
) -> (Vec<CubicBez>, Vec<CubicBez>, bool) {
    let segment_count = source.segments.len().max(target.segments.len());
    let source_segments = subdivide_to_count(&source.segments, segment_count);
    let target_segments = subdivide_to_count(&target.segments, segment_count);
    let closed = source.closed || target.closed;
    let target_segments = align_cubic_segments(&source_segments, &target_segments, closed);
    (source_segments, target_segments, closed)
}

fn subdivide_to_count(segments: &[CubicBez], target_count: usize) -> Vec<CubicBez> {
    if segments.is_empty() || target_count <= segments.len() {
        return segments.to_vec();
    }

    let lengths: Vec<_> = segments.iter().map(|segment| segment.arclen(0.1)).collect();
    let mut subdivisions = vec![1usize; segments.len()];
    while subdivisions.iter().sum::<usize>() < target_count {
        let index = lengths
            .iter()
            .zip(&subdivisions)
            .enumerate()
            .max_by(|(_, (a_length, a_parts)), (_, (b_length, b_parts))| {
                (**a_length / **a_parts as f64).total_cmp(&(**b_length / **b_parts as f64))
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        subdivisions[index] += 1;
    }

    segments
        .iter()
        .zip(subdivisions)
        .flat_map(|(segment, parts)| {
            (0..parts).map(move |part| {
                let start = part as f64 / parts as f64;
                let end = (part + 1) as f64 / parts as f64;
                segment.subsegment(start..end)
            })
        })
        .collect()
}

fn degenerate_segments(point: Point, count: usize) -> Vec<CubicBez> {
    vec![CubicBez::new(point, point, point, point); count]
}

fn reverse_cubic_segments(segments: &[CubicBez]) -> Vec<CubicBez> {
    segments
        .iter()
        .rev()
        .map(|segment| CubicBez::new(segment.p3, segment.p2, segment.p1, segment.p0))
        .collect()
}

fn cubic_alignment_cost(source: &[CubicBez], target: &[CubicBez]) -> f64 {
    source
        .iter()
        .zip(target)
        .map(|(source, target)| {
            distance_squared(source.p0, target.p0)
                + distance_squared(source.p1, target.p1)
                + distance_squared(source.p2, target.p2)
                + distance_squared(source.p3, target.p3)
        })
        .sum()
}

fn align_cubic_segments(source: &[CubicBez], target: &[CubicBez], closed: bool) -> Vec<CubicBez> {
    if source.len() != target.len() || source.is_empty() {
        return target.to_vec();
    }

    if !closed {
        let reversed = reverse_cubic_segments(target);
        return if cubic_alignment_cost(source, target) <= cubic_alignment_cost(source, &reversed) {
            target.to_vec()
        } else {
            reversed
        };
    }

    // Keep target winding intact so inner contours remain holes under non-zero fill.
    (0..target.len())
        .map(|offset| {
            target
                .iter()
                .cycle()
                .skip(offset)
                .take(target.len())
                .copied()
                .collect::<Vec<_>>()
        })
        .min_by(|a, b| cubic_alignment_cost(source, a).total_cmp(&cubic_alignment_cost(source, b)))
        .unwrap_or_else(|| target.to_vec())
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
    fn active_morph_preserves_bezier_curves_instead_of_polygonizing_them() {
        let source = kurbo::Circle::new((0.0, 0.0), 10.0).to_path(0.1);
        let target = kurbo::Circle::new((24.0, 3.0), 14.0).to_path(0.1);

        let midpoint = interpolate_paths_continuous(&source, &target, 0.5);
        let curve_count = midpoint
            .elements()
            .iter()
            .filter(|element| matches!(element, PathEl::CurveTo(..)))
            .count();
        let line_count = midpoint
            .elements()
            .iter()
            .filter(|element| matches!(element, PathEl::LineTo(..)))
            .count();

        assert!(
            curve_count > 0,
            "active morph discarded all Bezier controls"
        );
        assert_eq!(line_count, 0, "active morph polygonized a curved glyph");
    }

    #[test]
    fn normalized_bezier_representation_is_geometrically_exact_at_endpoints() {
        let mut source = BezPath::new();
        source.move_to((0.0, 0.0));
        source.curve_to((0.0, 12.0), (12.0, 12.0), (12.0, 0.0));

        let mut target = BezPath::new();
        target.move_to((2.0, 1.0));
        target.curve_to((2.0, 8.0), (6.0, 13.0), (10.0, 8.0));
        target.curve_to((14.0, 3.0), (18.0, 8.0), (18.0, 1.0));

        let source_contour = SampledContour::new(source, 64);
        let target_contour = SampledContour::new(target, 64);
        let (normalized_source, normalized_target, _) =
            normalize_contour_pair(&source_contour, &target_contour);

        assert_eq!(normalized_source.len(), 2);
        assert_eq!(normalized_target, target_contour.segments);
        for (segment_index, segment) in normalized_source.iter().enumerate() {
            for sample in 0..=16 {
                let local_t = sample as f64 / 16.0;
                let original_t = (segment_index as f64 + local_t) / 2.0;
                let distance = segment
                    .eval(local_t)
                    .distance(source_contour.segments[0].eval(original_t));
                assert!(
                    distance < 1e-9,
                    "de Casteljau subdivision changed geometry by {distance}"
                );
            }
        }
    }

    #[test]
    fn open_curves_align_reversed_targets_without_collapsing() {
        let mut source = BezPath::new();
        source.move_to((0.0, 0.0));
        source.curve_to((0.0, 10.0), (10.0, 10.0), (10.0, 0.0));

        let mut reversed_target = BezPath::new();
        reversed_target.move_to((10.0, 0.0));
        reversed_target.curve_to((10.0, 10.0), (0.0, 10.0), (0.0, 0.0));

        let midpoint = interpolate_paths_continuous(&source, &reversed_target, 0.5);
        assert!(get_point_at_alpha(&midpoint, 0.0).distance(Point::ZERO) < 1e-6);
        assert!(get_point_at_alpha(&midpoint, 0.5).y > 7.0);
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

    #[test]
    fn samples_3d_polyline_by_arc_length() {
        let points = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(2.0, 6.0, 0.0),
        ];
        assert_eq!(get_point_on_polyline(&points, 0.25), points[1]);
        assert_eq!(
            get_point_on_polyline(&points, 0.625),
            DVec3::new(2.0, 3.0, 0.0)
        );
        assert_eq!(get_point_on_polyline(&points, 1.0), points[2]);
    }
}
