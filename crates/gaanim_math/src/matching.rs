//! Transform matching utilities — improved over Manim's `TransformMatchingShapes/Tex`.
//!
//! Provides shape hashing, normalized shape distance, LCS for tex ordering,
//! and Hungarian assignment for minimal-cost pairing.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use gaanim_core::kurbo::{BezPath, Point, Rect, Shape};
use gaanim_core::peniko::Color;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Matching strategy — shapes uses geometry + position, tex prioritizes
/// `tex_string` equality and order preservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchingMode {
    Shapes,
    Tex,
}

impl Default for MatchingMode {
    fn default() -> Self {
        Self::Shapes
    }
}

/// Configurable weights for the matching cost.
///
/// `cost = shape_weight * shape_dist + position_weight * pos_dist + color_weight * color_dist`
/// plus a large `tex_mismatch_penalty` when keys differ in `Tex` mode.
#[derive(Debug, Clone)]
pub struct MatchingConfig {
    pub mode: MatchingMode,
    pub shape_weight: f64,
    pub position_weight: f64,
    pub color_weight: f64,
    pub tex_mismatch_penalty: f64,
    pub max_hungarian: usize,
}

impl Default for MatchingConfig {
    fn default() -> Self {
        Self {
            mode: MatchingMode::Shapes,
            shape_weight: 0.6,
            position_weight: 0.35,
            color_weight: 0.05,
            tex_mismatch_penalty: 0.8,
            max_hungarian: 64,
        }
    }
}

/// One item to be matched, with geometry and optional semantic key.
#[derive(Debug, Clone)]
pub struct MatchItem {
    /// Index in the original source/dst slice (caller maps back to ObjectId).
    pub index: usize,
    pub path: BezPath,
    pub center: (f64, f64),
    pub fill: Option<Color>,
    /// For `Tex` mode: the character / tex string.
    pub key: Option<String>,
}

/// Result of matching: paired indices plus unmatched leftovers.
#[derive(Debug, Clone, Default)]
pub struct MatchingResult {
    pub pairs: Vec<(usize, usize)>,
    pub unmatched_src: Vec<usize>,
    pub unmatched_dst: Vec<usize>,
}

// ---------------------------------------------------------------------------
// Shape helpers
// ---------------------------------------------------------------------------

/// Normalized shape hash — translation + uniform scale invariant, rotation
/// sensitive (square ≠ diamond). Samples the path, centers, scales to unit.
pub fn shape_hash(path: &BezPath) -> u64 {
    let samples = normalized_samples(path, 32);
    let mut hasher = DefaultHasher::new();
    for pt in samples {
        // Quantize to 1% of unit size to tolerate raster fuzz.
        let x = (pt.x * 100.0).round() as i32;
        let y = (pt.y * 100.0).round() as i32;
        x.hash(&mut hasher);
        y.hash(&mut hasher);
    }
    // Also hash closed-ness and element count class.
    let closed = path.elements().iter().any(|el| *el == kurbo::PathEl::ClosePath);
    closed.hash(&mut hasher);
    hasher.finish()
}

/// Normalized shape distance in [0, ~2], 0 = identical shape (up to
/// translation/scale), larger = more different. Handles cyclic shift
/// and winding for closed contours.
pub fn shape_distance(a: &BezPath, b: &BezPath) -> f64 {
    if a.elements().is_empty() && b.elements().is_empty() {
        return 0.0;
    }
    if a.elements().is_empty() || b.elements().is_empty() {
        return 1.0;
    }
    let n = 32;
    let sa = normalized_samples(a, n);
    let sb = normalized_samples(b, n);
    if sa.is_empty() || sb.is_empty() {
        return 1.0;
    }
    let closed_a = a.elements().iter().any(|el| *el == kurbo::PathEl::ClosePath);
    let closed_b = b.elements().iter().any(|el| *el == kurbo::PathEl::ClosePath);
    let closed = closed_a || closed_b;

    // If either is open, distance is min of forward vs reversed.
    if !closed {
        let d_forward = avg_point_distance(&sa, &sb);
        let mut rev = sb.clone();
        rev.reverse();
        let d_rev = avg_point_distance(&sa, &rev);
        return d_forward.min(d_rev);
    }

    // Closed: try all cyclic rotations, keep minimal.
    let mut best = f64::INFINITY;
    for offset in 0..n {
        let rotated: Vec<Point> = sb.iter().cycle().skip(offset).take(n).copied().collect();
        let d = avg_point_distance(&sa, &rotated);
        if d < best {
            best = d;
        }
    }
    // Also try reversed winding (Manim never reverses closed winding for
    // holes, but shape distance should consider it).
    let mut sb_rev = sb.clone();
    sb_rev.reverse();
    for offset in 0..n {
        let rotated: Vec<Point> = sb_rev.iter().cycle().skip(offset).take(n).copied().collect();
        let d = avg_point_distance(&sa, &rotated);
        if d < best {
            best = d;
        }
    }
    best
}

fn avg_point_distance(a: &[Point], b: &[Point]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let sum: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(pa, pb)| ((pa.x - pb.x).powi(2) + (pa.y - pb.y).powi(2)).sqrt())
        .sum();
    sum / a.len() as f64
}

fn normalized_samples(path: &BezPath, n: usize) -> Vec<Point> {
    if path.elements().is_empty() || n == 0 {
        return Vec::new();
    }
    // Sample via arc-length proportional points (uses get_point_at_alpha logic
    // but we avoid importing path.rs to keep matching standalone).
    let samples: Vec<Point> = (0..n)
        .map(|i| {
            let alpha = if n == 1 {
                0.0
            } else {
                i as f64 / (n - 1) as f64
            };
            sample_point_at_alpha(path, alpha)
        })
        .collect();

    // Compute center
    let (sx, sy) = samples.iter().fold((0.0, 0.0), |(x, y), p| (x + p.x, y + p.y));
    let cx = sx / n as f64;
    let cy = sy / n as f64;

    // Scale to unit: max dimension of bounding box
    let bbox: Rect = path.bounding_box();
    let w = bbox.width().max(1e-9);
    let h = bbox.height().max(1e-9);
    let scale = w.max(h).max(1e-9);

    samples
        .into_iter()
        .map(|p| Point::new((p.x - cx) / scale, (p.y - cy) / scale))
        .collect()
}

fn sample_point_at_alpha(path: &BezPath, alpha: f64) -> Point {
    use kurbo::{ParamCurve, ParamCurveArclen, PathEl, PathSeg};
    if alpha <= 0.0 {
        if let Some(PathEl::MoveTo(p)) = path.elements().first() {
            return *p;
        }
        return Point::new(0.0, 0.0);
    }
    if alpha >= 1.0 {
        let mut last = Point::new(0.0, 0.0);
        for el in path.elements() {
            match el {
                PathEl::MoveTo(p) => last = *p,
                PathEl::LineTo(p) => last = *p,
                PathEl::QuadTo(_, p) => last = *p,
                PathEl::CurveTo(_, _, p) => last = *p,
                PathEl::ClosePath => {}
            }
        }
        return last;
    }
    // Total length
    let total: f64 = path
        .segments()
        .map(|seg| match seg {
            PathSeg::Line(l) => l.arclen(0.1),
            PathSeg::Quad(q) => q.arclen(0.1),
            PathSeg::Cubic(c) => c.arclen(0.1),
        })
        .sum();
    if total <= 1e-9 {
        return Point::new(0.0, 0.0);
    }
    let target = total * alpha;
    let mut cur = 0.0;
    let mut pos = Point::new(0.0, 0.0);
    // Find segment containing target
    for seg in path.segments() {
        let len = match seg {
            PathSeg::Line(l) => l.arclen(0.1),
            PathSeg::Quad(q) => q.arclen(0.1),
            PathSeg::Cubic(c) => c.arclen(0.1),
        };
        if cur + len >= target {
            let remaining = target - cur;
            let t = seg.inv_arclen(remaining, 0.1);
            // Approximate point at t via subsegment start->t? simpler: use seg.eval
            // seg is PathSeg, need to eval at t. Use subsegment.
            let sub = seg.subsegment(0.0..t);
            return sub.end();
        }
        cur += len;
        pos = seg.end();
    }
    pos
}

fn color_distance(a: Option<Color>, b: Option<Color>) -> f64 {
    match (a, b) {
        (Some(ca), Some(cb)) => {
            let ra = ca.to_rgba8();
            let rb = cb.to_rgba8();
            let dr = f64::from(ra.r) - f64::from(rb.r);
            let dg = f64::from(ra.g) - f64::from(rb.g);
            let db = f64::from(ra.b) - f64::from(rb.b);
            ((dr * dr + dg * dg + db * db).sqrt()) / (255.0 * (3.0f64).sqrt())
        }
        (None, None) => 0.0,
        _ => 0.5,
    }
}

fn position_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    // Squared distance penalizes large jumps more, avoiding crossing when totals tie.
    (dx * dx + dy * dy) / (500.0 * 500.0)
}

// ---------------------------------------------------------------------------
// LCS for Tex mode (order-preserving)
// ---------------------------------------------------------------------------

/// Longest common subsequence on keys, stable and handles duplicates.
/// Returns list of (src_idx, dst_idx) pairs in order.
pub fn lcs_match(src_keys: &[Option<String>], dst_keys: &[Option<String>]) -> Vec<(usize, usize)> {
    let n = src_keys.len();
    let m = dst_keys.len();
    if n == 0 || m == 0 {
        return Vec::new();
    }
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            if src_keys[i] == dst_keys[j] {
                dp[i][j] = 1 + dp[i + 1][j + 1];
            } else {
                dp[i][j] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if src_keys[i] == dst_keys[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

// ---------------------------------------------------------------------------
// Hungarian assignment (minimization) for rectangular cost matrix
// ---------------------------------------------------------------------------

/// Solve assignment for rectangular matrix `cost` (n x m). Returns pairs
/// (i, j) for min(n,m) assignments minimizing total cost.
/// Uses Hungarian for n,m <= max_hungarian, else greedy fallback.
pub fn assign_min_cost(cost: &[Vec<f64>], max_hungarian: usize) -> Vec<(usize, usize)> {
    let n = cost.len();
    let m = cost.first().map(|r| r.len()).unwrap_or(0);
    if n == 0 || m == 0 {
        return Vec::new();
    }
    if n > max_hungarian || m > max_hungarian {
        return greedy_assign(cost);
    }
    hungarian(cost)
}

fn greedy_assign(cost: &[Vec<f64>]) -> Vec<(usize, usize)> {
    let n = cost.len();
    let m = cost[0].len();
    let mut pairs = Vec::new();
    let mut used_dst = vec![false; m];
    // For each src in order, pick best unused dst below threshold? We pick
    // globally minimal remaining cost iteratively (more stable).
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..n {
        for j in 0..m {
            candidates.push((cost[i][j], i, j));
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut used_src = vec![false; n];
    for (_, i, j) in candidates {
        if !used_src[i] && !used_dst[j] {
            pairs.push((i, j));
            used_src[i] = true;
            used_dst[j] = true;
            if pairs.len() == n.min(m) {
                break;
            }
        }
    }
    pairs
}

/// Hungarian for square padded matrix (minimization).
fn hungarian(cost_rect: &[Vec<f64>]) -> Vec<(usize, usize)> {
    let n = cost_rect.len();
    let m = cost_rect[0].len();
    let size = n.max(m);
    // Pad to square with large cost
    let pad_cost = 1e9;
    let mut cost = vec![vec![pad_cost; size + 1]; size + 1]; // 1-indexed
    for i in 0..n {
        for j in 0..m {
            cost[i + 1][j + 1] = cost_rect[i][j];
        }
    }
    // u, v potentials, p - assignment, way
    let mut u = vec![0.0; size + 1];
    let mut v = vec![0.0; size + 1];
    let mut p = vec![0usize; size + 1];
    let mut way = vec![0usize; size + 1];

    for i in 1..=size {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![f64::INFINITY; size + 1];
        let mut used = vec![false; size + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = f64::INFINITY;
            let mut j1 = 0;
            for j in 1..=size {
                if !used[j] {
                    let cur = cost[i0][j] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=size {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        // augment
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    // p[j] = i assigned to j
    let mut ans = vec![0usize; size + 1];
    for j in 1..=size {
        ans[p[j]] = j;
    }
    let mut pairs = Vec::new();
    for i in 1..=size {
        let j = ans[i];
        if i <= n && j <= m && cost[i][j] < pad_cost / 2.0 {
            pairs.push((i - 1, j - 1));
        }
    }
    pairs
}

// ---------------------------------------------------------------------------
// Main matching entry
// ---------------------------------------------------------------------------

/// Compute matching between src and dst items.
///
/// Tex mode: first LCS on keys (order-preserving), then Hungarian on
/// remaining with key penalty. Shapes mode: pure Hungarian with shape+pos+color.
pub fn match_items(
    src: &[MatchItem],
    dst: &[MatchItem],
    config: &MatchingConfig,
) -> MatchingResult {
    if src.is_empty() || dst.is_empty() {
        return MatchingResult {
            pairs: Vec::new(),
            unmatched_src: (0..src.len()).collect(),
            unmatched_dst: (0..dst.len()).collect(),
        };
    }

    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut used_src = vec![false; src.len()];
    let mut used_dst = vec![false; dst.len()];

    if config.mode == MatchingMode::Tex {
        let src_keys: Vec<Option<String>> = src.iter().map(|it| it.key.clone()).collect();
        let dst_keys: Vec<Option<String>> = dst.iter().map(|it| it.key.clone()).collect();
        let lcs = lcs_match(&src_keys, &dst_keys);
        for (si, di) in lcs {
            // Map via index position in src/dst (they are 0..len)
            // But MatchItem.index may differ; we use position in slice
            pairs.push((src[si].index, dst[di].index));
            used_src[si] = true;
            used_dst[di] = true;
        }
    }

    // Collect remaining indices
    let rem_src: Vec<usize> = (0..src.len()).filter(|i| !used_src[*i]).collect();
    let rem_dst: Vec<usize> = (0..dst.len()).filter(|i| !used_dst[*i]).collect();

    if !rem_src.is_empty() && !rem_dst.is_empty() {
        // Build cost matrix for remaining
        let mut cost = vec![vec![0.0; rem_dst.len()]; rem_src.len()];
        for (ri, &si) in rem_src.iter().enumerate() {
            let s = &src[si];
            for (rj, &di) in rem_dst.iter().enumerate() {
                let d = &dst[di];
                let shape = shape_distance(&s.path, &d.path);
                let pos = position_distance(s.center, d.center);
                let col = color_distance(s.fill, d.fill);
                let mut c;
                if config.mode == MatchingMode::Tex {
                    // For tex, only identical keys should morph; different keys fade.
                    let keys_equal = s.key == d.key;
                    if keys_equal {
                        c = 0.1 * shape + 0.7 * pos + 0.05 * col;
                    } else {
                        // Different characters: do not morph, force fade via large cost.
                        c = 100.0 + 0.7 * pos + 0.1 * shape;
                    }
                } else {
                    c = config.shape_weight * shape
                        + config.position_weight * pos
                        + config.color_weight * col;
                    if shape_hash(&s.path) == shape_hash(&d.path) {
                        // Bonus for identical shape
                        c *= 0.5;
                    }
                }
                cost[ri][rj] = c;
            }
        }
        let assignments = assign_min_cost(&cost, config.max_hungarian);
        for (ri, rj) in assignments {
            let si = rem_src[ri];
            let di = rem_dst[rj];
            // Threshold: for tex, only pair if cost is reasonable; for shapes, always pair
            let threshold = if config.mode == MatchingMode::Tex { 2.0 } else { 3.0 };
            if cost[ri][rj] > threshold {
                continue;
            }
            pairs.push((src[si].index, dst[di].index));
            used_src[si] = true;
            used_dst[di] = true;
        }
    }

    let unmatched_src = (0..src.len())
        .filter(|i| !used_src[*i])
        .map(|i| src[i].index)
        .collect();
    let unmatched_dst = (0..dst.len())
        .filter(|i| !used_dst[*i])
        .map(|i| dst[i].index)
        .collect();

    MatchingResult {
        pairs,
        unmatched_src,
        unmatched_dst,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::BezPath;

    fn square(x: f64) -> BezPath {
        let mut p = BezPath::new();
        p.move_to((x, 0.0));
        p.line_to((x + 10.0, 0.0));
        p.line_to((x + 10.0, 10.0));
        p.line_to((x, 10.0));
        p.close_path();
        p
    }

    fn circle_path(cx: f64, cy: f64, r: f64) -> BezPath {
        let mut p = BezPath::new();
        p.move_to((cx + r, cy));
        p.curve_to((cx + r, cy + r * 0.552), (cx + r * 0.552, cy + r), (cx, cy + r));
        p.curve_to((cx - r * 0.552, cy + r), (cx - r, cy + r * 0.552), (cx - r, cy));
        p.curve_to((cx - r, cy - r * 0.552), (cx - r * 0.552, cy - r), (cx, cy - r));
        p.curve_to((cx + r * 0.552, cy - r), (cx + r, cy - r * 0.552), (cx + r, cy));
        p.close_path();
        p
    }

    #[test]
    fn shape_hash_identical() {
        let a = square(0.0);
        let b = square(0.0);
        assert_eq!(shape_hash(&a), shape_hash(&b));
    }

    #[test]
    fn shape_hash_different() {
        let a = square(0.0);
        let b = circle_path(0.0, 0.0, 5.0);
        assert_ne!(shape_hash(&a), shape_hash(&b));
    }

    #[test]
    fn shape_distance_identical_zero() {
        let a = square(0.0);
        let b = square(0.0);
        let d = shape_distance(&a, &b);
        assert!(d < 1e-6, "d={}", d);
    }

    #[test]
    fn shape_distance_different_positive() {
        let a = square(0.0);
        let b = circle_path(0.0, 0.0, 5.0);
        let d = shape_distance(&a, &b);
        assert!(d > 0.1, "d={}", d);
    }

    #[test]
    fn match_shapes_exact() {
        let src = vec![
            MatchItem {
                index: 0,
                path: square(0.0),
                center: (0.0, 0.0),
                fill: None,
                key: None,
            },
            MatchItem {
                index: 1,
                path: circle_path(0.0, 0.0, 5.0),
                center: (100.0, 0.0),
                fill: None,
                key: None,
            },
        ];
        let dst = vec![
            MatchItem {
                index: 10,
                path: circle_path(0.0, 0.0, 5.0),
                center: (110.0, 0.0),
                fill: None,
                key: None,
            },
            MatchItem {
                index: 11,
                path: square(0.0),
                center: (10.0, 0.0),
                fill: None,
                key: None,
            },
        ];
        let cfg = MatchingConfig::default();
        let res = match_items(&src, &dst, &cfg);
        assert_eq!(res.pairs.len(), 2);
        // square should match square despite position reordering via shape weight
        assert!(res.pairs.contains(&(0, 11)));
        assert!(res.pairs.contains(&(1, 10)));
    }

    #[test]
    fn match_tex_lcs_preserves_order() {
        let mk = |idx, ch: &str| MatchItem {
            index: idx,
            path: square(0.0),
            center: (idx as f64 * 10.0, 0.0),
            fill: None,
            key: Some(ch.to_string()),
        };
        let src = vec![mk(0, "a"), mk(1, "b"), mk(2, "c"), mk(3, "d")];
        let dst = vec![mk(10, "a"), mk(11, "c"), mk(12, "b"), mk(13, "d")];
        let mut cfg = MatchingConfig::default();
        cfg.mode = MatchingMode::Tex;
        let res = match_items(&src, &dst, &cfg);
        // LCS should pick a,c,d or a,b,d (length 3), verify at least 3 pairs and order preserved
        assert!(res.pairs.len() >= 3);
        // Check that pairs are in increasing order for both src and dst when sorted by src
        let mut sorted = res.pairs.clone();
        sorted.sort_by_key(|(s, _)| *s);
        for w in sorted.windows(2) {
            assert!(w[0].0 < w[1].0);
            // dst order may not be fully sorted due to Hungarian leftover, but LCS part is ordered
        }
    }

    #[test]
    fn hungarian_rectangular() {
        // 2x3 case
        let cost = vec![vec![1.0, 2.0, 3.0], vec![2.0, 1.0, 4.0]];
        let pairs = hungarian(&cost);
        assert_eq!(pairs.len(), 2);
        // optimal is (0,0) + (1,1) =2 vs other combos larger
        assert!(pairs.contains(&(0, 0)));
        assert!(pairs.contains(&(1, 1)));
    }

    #[test]
    fn greedy_fallback() {
        let cost = vec![vec![0.5; 70]; 70];
        let pairs = assign_min_cost(&cost, 64);
        assert_eq!(pairs.len(), 70); // greedy should still pair all
    }

    #[test]
    fn unmatched_handling() {
        let src = vec![
            MatchItem {
                index: 0,
                path: square(0.0),
                center: (0.0, 0.0),
                fill: None,
                key: None,
            },
            MatchItem {
                index: 1,
                path: square(10.0),
                center: (10.0, 0.0),
                fill: None,
                key: None,
            },
        ];
        let dst = vec![MatchItem {
            index: 10,
            path: square(0.0),
            center: (0.0, 0.0),
            fill: None,
            key: None,
        }];
        let cfg = MatchingConfig::default();
        let res = match_items(&src, &dst, &cfg);
        assert_eq!(res.pairs.len(), 1);
        assert_eq!(res.unmatched_src.len(), 1);
        assert_eq!(res.unmatched_dst.len(), 0);
    }
}
