//! Polygon triangulation with holes by ear clipping, ported from Mapbox's
//! earcut (ISC license) without its z-order hash: extruded outlines have at
//! most a few thousand points.
//!
//! Holes are first bridged into the outer ring, turning the polygon into one
//! ring, which is then cut ear by ear. When no ear is left (self-touching
//! rings, as polygon cleanup can produce), the ring is filtered, its local
//! self-intersections cured, and finally split along a valid diagonal.

/// A node of the doubly linked ring, stored in an arena.
#[derive(Debug, Clone, Copy)]
struct Node {
    /// Index of the vertex in the input.
    i: usize,
    x: f64,
    y: f64,
    prev: usize,
    next: usize,
    /// Bridge copies of a single-point hole.
    steiner: bool,
}

struct Rings {
    nodes: Vec<Node>,
}

/// Triangulate a polygon given as `points`, whose first `outer` points form
/// the outer ring and each later run starting at an entry of `holes` forms a
/// hole. Returns vertex indices, three per triangle, all with the outer
/// ring's winding.
pub fn earcut(points: &[[f64; 2]], holes: &[usize]) -> Vec<usize> {
    let mut rings = Rings {
        nodes: Vec::with_capacity(points.len() * 3 / 2),
    };
    let outer_len = holes.first().copied().unwrap_or(points.len());
    let mut triangles = Vec::new();
    let Some(mut outer) = rings.linked_list(points, 0, outer_len, true) else {
        return triangles;
    };
    if rings.next(outer) == rings.prev(outer) {
        return triangles;
    }
    if !holes.is_empty() {
        outer = rings.eliminate_holes(points, holes, outer);
    }
    rings.earcut_linked(Some(outer), &mut triangles, 0);
    triangles
}

/// Twice the signed area of the run, positive for counterclockwise rings in a
/// y-up frame.
fn signed_area(points: &[[f64; 2]], start: usize, end: usize) -> f64 {
    let mut sum = 0.0;
    let mut j = end - 1;
    for i in start..end {
        sum += (points[j][0] - points[i][0]) * (points[i][1] + points[j][1]);
        j = i;
    }
    sum
}

fn point_in_triangle(a: (f64, f64), b: (f64, f64), c: (f64, f64), p: (f64, f64)) -> bool {
    (c.0 - p.0) * (a.1 - p.1) >= (a.0 - p.0) * (c.1 - p.1)
        && (a.0 - p.0) * (b.1 - p.1) >= (b.0 - p.0) * (a.1 - p.1)
        && (b.0 - p.0) * (c.1 - p.1) >= (c.0 - p.0) * (b.1 - p.1)
}

fn sign(value: f64) -> i8 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

impl Rings {
    fn prev(&self, n: usize) -> usize {
        self.nodes[n].prev
    }

    fn next(&self, n: usize) -> usize {
        self.nodes[n].next
    }

    fn xy(&self, n: usize) -> (f64, f64) {
        (self.nodes[n].x, self.nodes[n].y)
    }

    fn insert(&mut self, i: usize, point: [f64; 2], last: Option<usize>) -> usize {
        let n = self.nodes.len();
        let mut node = Node {
            i,
            x: point[0],
            y: point[1],
            prev: n,
            next: n,
            steiner: false,
        };
        if let Some(last) = last {
            node.next = self.nodes[last].next;
            node.prev = last;
            let after = self.nodes[last].next;
            self.nodes[after].prev = n;
            self.nodes[last].next = n;
        }
        self.nodes.push(node);
        n
    }

    fn remove(&mut self, n: usize) {
        let (prev, next) = (self.nodes[n].prev, self.nodes[n].next);
        self.nodes[next].prev = prev;
        self.nodes[prev].next = next;
    }

    fn equals(&self, a: usize, b: usize) -> bool {
        self.xy(a) == self.xy(b)
    }

    /// Negative when `p, q, r` turn counterclockwise.
    fn area(&self, p: usize, q: usize, r: usize) -> f64 {
        let (p, q, r) = (self.xy(p), self.xy(q), self.xy(r));
        (q.1 - p.1) * (r.0 - q.0) - (q.0 - p.0) * (r.1 - q.1)
    }

    /// A ring over `points[start..end]`, counterclockwise when `outer`,
    /// clockwise otherwise; `None` for an empty run.
    fn linked_list(
        &mut self,
        points: &[[f64; 2]],
        start: usize,
        end: usize,
        outer: bool,
    ) -> Option<usize> {
        if end <= start {
            return None;
        }
        let mut last = None;
        if outer == (signed_area(points, start, end) > 0.0) {
            for (i, point) in points.iter().enumerate().take(end).skip(start) {
                last = Some(self.insert(i, *point, last));
            }
        } else {
            for i in (start..end).rev() {
                last = Some(self.insert(i, points[i], last));
            }
        }
        if let Some(node) = last
            && self.equals(node, self.next(node))
        {
            let next = self.next(node);
            self.remove(node);
            last = Some(next);
        }
        last
    }

    /// Remove duplicate and collinear points from the ring through `start`.
    fn filter_points(&mut self, start: usize, end: Option<usize>) -> usize {
        let mut end = end.unwrap_or(start);
        let mut p = start;
        loop {
            let mut again = false;
            let next = self.next(p);
            if !self.nodes[p].steiner
                && (self.equals(p, next) || self.area(self.prev(p), p, next) == 0.0)
            {
                self.remove(p);
                p = self.prev(p);
                end = p;
                if p == self.next(p) {
                    break;
                }
                again = true;
            } else {
                p = next;
            }
            if !again && p == end {
                break;
            }
        }
        end
    }

    fn earcut_linked(&mut self, ear: Option<usize>, triangles: &mut Vec<usize>, pass: u8) {
        let Some(mut ear) = ear else {
            return;
        };
        let mut stop = ear;
        while self.prev(ear) != self.next(ear) {
            let (prev, next) = (self.prev(ear), self.next(ear));
            if self.is_ear(ear) {
                triangles.extend([self.nodes[prev].i, self.nodes[ear].i, self.nodes[next].i]);
                self.remove(ear);
                ear = self.next(next);
                stop = ear;
                continue;
            }
            ear = next;
            if ear == stop {
                match pass {
                    0 => {
                        let filtered = self.filter_points(ear, None);
                        self.earcut_linked(Some(filtered), triangles, 1);
                    }
                    1 => {
                        let filtered = self.filter_points(ear, None);
                        let cured = self.cure_local_intersections(filtered, triangles);
                        self.earcut_linked(Some(cured), triangles, 2);
                    }
                    _ => self.split_earcut(ear, triangles),
                }
                break;
            }
        }
    }

    fn is_ear(&self, ear: usize) -> bool {
        let (a, b, c) = (self.prev(ear), ear, self.next(ear));
        if self.area(a, b, c) >= 0.0 {
            return false;
        }
        let (pa, pb, pc) = (self.xy(a), self.xy(b), self.xy(c));
        let (x0, x1) = (pa.0.min(pb.0).min(pc.0), pa.0.max(pb.0).max(pc.0));
        let (y0, y1) = (pa.1.min(pb.1).min(pc.1), pa.1.max(pb.1).max(pc.1));
        let mut p = self.next(c);
        while p != a {
            let point = self.xy(p);
            if point.0 >= x0
                && point.0 <= x1
                && point.1 >= y0
                && point.1 <= y1
                && point != pa
                && point_in_triangle(pa, pb, pc, point)
                && self.area(self.prev(p), p, self.next(p)) >= 0.0
            {
                return false;
            }
            p = self.next(p);
        }
        true
    }

    fn cure_local_intersections(&mut self, start: usize, triangles: &mut Vec<usize>) -> usize {
        let mut start = start;
        let mut p = start;
        loop {
            let a = self.prev(p);
            let b = self.next(self.next(p));
            if !self.equals(a, b)
                && self.intersects(a, p, self.next(p), b)
                && self.locally_inside(a, b)
                && self.locally_inside(b, a)
            {
                triangles.extend([self.nodes[a].i, self.nodes[p].i, self.nodes[b].i]);
                let after = self.next(p);
                self.remove(p);
                self.remove(after);
                p = b;
                start = b;
            }
            p = self.next(p);
            if p == start {
                break;
            }
        }
        self.filter_points(p, None)
    }

    fn split_earcut(&mut self, start: usize, triangles: &mut Vec<usize>) {
        let mut a = start;
        loop {
            let mut b = self.next(self.next(a));
            while b != self.prev(a) {
                if self.nodes[a].i != self.nodes[b].i && self.is_valid_diagonal(a, b) {
                    let c = self.split_polygon(a, b);
                    let a = self.filter_points(a, Some(self.next(a)));
                    let c = self.filter_points(c, Some(self.next(c)));
                    self.earcut_linked(Some(a), triangles, 0);
                    self.earcut_linked(Some(c), triangles, 0);
                    return;
                }
                b = self.next(b);
            }
            a = self.next(a);
            if a == start {
                break;
            }
        }
    }

    fn eliminate_holes(&mut self, points: &[[f64; 2]], holes: &[usize], outer: usize) -> usize {
        let mut queue = Vec::with_capacity(holes.len());
        for (index, &start) in holes.iter().enumerate() {
            let end = holes.get(index + 1).copied().unwrap_or(points.len());
            if end <= start {
                continue;
            }
            let Some(list) = self.linked_list(points, start, end, false) else {
                continue;
            };
            if list == self.next(list) {
                self.nodes[list].steiner = true;
            }
            queue.push(self.leftmost(list));
        }
        queue.sort_by(|&a, &b| {
            let (pa, pb) = (self.xy(a), self.xy(b));
            pa.0.total_cmp(&pb.0).then(pa.1.total_cmp(&pb.1))
        });
        let mut outer = outer;
        for hole in queue {
            outer = self.eliminate_hole(hole, outer);
        }
        outer
    }

    fn eliminate_hole(&mut self, hole: usize, outer: usize) -> usize {
        let Some(bridge) = self.find_hole_bridge(hole, outer) else {
            return outer;
        };
        let reverse = self.split_polygon(bridge, hole);
        self.filter_points(reverse, Some(self.next(reverse)));
        self.filter_points(bridge, Some(self.next(bridge)))
    }

    /// The outer ring vertex the hole connects to with a segment that
    /// crosses nothing (David Eberly's algorithm).
    fn find_hole_bridge(&self, hole: usize, outer: usize) -> Option<usize> {
        let (hx, hy) = self.xy(hole);
        let mut p = outer;
        let mut qx = f64::NEG_INFINITY;
        let mut m = None;
        loop {
            let (px, py) = self.xy(p);
            let (nx, ny) = self.xy(self.next(p));
            if hy <= py && hy >= ny && ny != py {
                let x = px + (hy - py) * (nx - px) / (ny - py);
                if x <= hx && x > qx {
                    qx = x;
                    m = Some(if px < nx { p } else { self.next(p) });
                    if x == hx {
                        return m;
                    }
                }
            }
            p = self.next(p);
            if p == outer {
                break;
            }
        }
        let mut m = m?;
        let stop = m;
        let (mx, my) = self.xy(m);
        let mut tan_min = f64::INFINITY;
        p = m;
        loop {
            let (px, py) = self.xy(p);
            let (a, c) = if hy < my {
                ((hx, hy), (qx, hy))
            } else {
                ((qx, hy), (hx, hy))
            };
            if hx >= px && px >= mx && hx != px && point_in_triangle(a, (mx, my), c, (px, py)) {
                let tan = (hy - py).abs() / (hx - px);
                let (cx, _) = self.xy(m);
                if self.locally_inside(p, hole)
                    && (tan < tan_min
                        || (tan == tan_min
                            && (px > cx || (px == cx && self.sector_contains_sector(m, p)))))
                {
                    m = p;
                    tan_min = tan;
                }
            }
            p = self.next(p);
            if p == stop {
                break;
            }
        }
        Some(m)
    }

    fn sector_contains_sector(&self, m: usize, p: usize) -> bool {
        self.area(self.prev(m), m, self.prev(p)) < 0.0
            && self.area(self.next(p), m, self.next(m)) < 0.0
    }

    fn leftmost(&self, start: usize) -> usize {
        let mut p = start;
        let mut leftmost = start;
        loop {
            let (px, py) = self.xy(p);
            let (lx, ly) = self.xy(leftmost);
            if px < lx || (px == lx && py < ly) {
                leftmost = p;
            }
            p = self.next(p);
            if p == start {
                break;
            }
        }
        leftmost
    }

    fn is_valid_diagonal(&self, a: usize, b: usize) -> bool {
        let (ai, bi) = (self.nodes[a].i, self.nodes[b].i);
        self.nodes[self.next(a)].i != bi
            && self.nodes[self.prev(a)].i != bi
            && !self.intersects_polygon(a, b)
            && ((self.locally_inside(a, b)
                && self.locally_inside(b, a)
                && self.middle_inside(a, b)
                && (self.area(self.prev(a), a, self.prev(b)) != 0.0
                    || self.area(a, self.prev(b), b) != 0.0))
                || (self.equals(a, b)
                    && self.area(self.prev(a), a, self.next(a)) > 0.0
                    && self.area(self.prev(b), b, self.next(b)) > 0.0))
            && ai != bi
    }

    fn on_segment(p: (f64, f64), q: (f64, f64), r: (f64, f64)) -> bool {
        q.0 <= p.0.max(r.0) && q.0 >= p.0.min(r.0) && q.1 <= p.1.max(r.1) && q.1 >= p.1.min(r.1)
    }

    fn intersects(&self, p1: usize, q1: usize, p2: usize, q2: usize) -> bool {
        let o1 = sign(self.area(p1, q1, p2));
        let o2 = sign(self.area(p1, q1, q2));
        let o3 = sign(self.area(p2, q2, p1));
        let o4 = sign(self.area(p2, q2, q1));
        let (p1, q1, p2, q2) = (self.xy(p1), self.xy(q1), self.xy(p2), self.xy(q2));
        (o1 != o2 && o3 != o4)
            || (o1 == 0 && Self::on_segment(p1, p2, q1))
            || (o2 == 0 && Self::on_segment(p1, q2, q1))
            || (o3 == 0 && Self::on_segment(p2, p1, q2))
            || (o4 == 0 && Self::on_segment(p2, q1, q2))
    }

    fn intersects_polygon(&self, a: usize, b: usize) -> bool {
        let (ai, bi) = (self.nodes[a].i, self.nodes[b].i);
        let mut p = a;
        loop {
            let next = self.next(p);
            let (pi, ni) = (self.nodes[p].i, self.nodes[next].i);
            if pi != ai && ni != ai && pi != bi && ni != bi && self.intersects(p, next, a, b) {
                return true;
            }
            p = next;
            if p == a {
                return false;
            }
        }
    }

    fn locally_inside(&self, a: usize, b: usize) -> bool {
        if self.area(self.prev(a), a, self.next(a)) < 0.0 {
            self.area(a, b, self.next(a)) >= 0.0 && self.area(a, self.prev(a), b) >= 0.0
        } else {
            self.area(a, b, self.prev(a)) < 0.0 || self.area(a, self.next(a), b) < 0.0
        }
    }

    fn middle_inside(&self, a: usize, b: usize) -> bool {
        let (ax, ay) = self.xy(a);
        let (bx, by) = self.xy(b);
        let (px, py) = ((ax + bx) / 2.0, (ay + by) / 2.0);
        let mut inside = false;
        let mut p = a;
        loop {
            let (x, y) = self.xy(p);
            let (nx, ny) = self.xy(self.next(p));
            if ((y > py) != (ny > py)) && ny != y && px < (nx - x) * (py - y) / (ny - y) + x {
                inside = !inside;
            }
            p = self.next(p);
            if p == a {
                return inside;
            }
        }
    }

    /// Link `a` to `b` with a two-way bridge, splitting the ring in two;
    /// returns the copy of `b` that starts the second ring.
    fn split_polygon(&mut self, a: usize, b: usize) -> usize {
        let a2 = self.nodes.len();
        let b2 = a2 + 1;
        let (an, bp) = (self.next(a), self.prev(b));
        let mut a_copy = self.nodes[a];
        let mut b_copy = self.nodes[b];
        a_copy.steiner = false;
        b_copy.steiner = false;
        self.nodes.push(a_copy);
        self.nodes.push(b_copy);
        self.nodes[a].next = b;
        self.nodes[b].prev = a;
        self.nodes[a2].next = an;
        self.nodes[an].prev = a2;
        self.nodes[b2].next = a2;
        self.nodes[a2].prev = b2;
        self.nodes[bp].next = b2;
        self.nodes[b2].prev = bp;
        b2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Twice the total area of the triangles.
    fn covered(points: &[[f64; 2]], triangles: &[usize]) -> f64 {
        triangles
            .chunks(3)
            .map(|t| {
                let (a, b, c) = (points[t[0]], points[t[1]], points[t[2]]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs()
            })
            .sum()
    }

    #[test]
    fn a_square_is_two_triangles() {
        let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let triangles = earcut(&square, &[]);
        assert_eq!(triangles.len(), 6);
        assert!((covered(&square, &triangles) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn holes_are_left_uncovered() {
        // A 4x4 square with a 2x2 hole and a 1x1 hole, in either winding.
        let points = [
            [0.0, 0.0],
            [4.0, 0.0],
            [4.0, 4.0],
            [0.0, 4.0],
            [0.5, 0.5],
            [0.5, 2.5],
            [2.5, 2.5],
            [2.5, 0.5],
            [3.0, 3.0],
            [3.5, 3.0],
            [3.5, 3.5],
            [3.0, 3.5],
        ];
        let triangles = earcut(&points, &[4, 8]);
        assert!((covered(&points, &triangles) - 2.0 * (16.0 - 4.0 - 0.25)).abs() < 1e-9);
    }

    #[test]
    fn concave_rings_with_collinear_runs() {
        // An L shape with extra points along its edges.
        let points = [
            [0.0, 0.0],
            [1.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [1.0, 3.0],
            [0.0, 3.0],
            [0.0, 1.5],
        ];
        let triangles = earcut(&points, &[]);
        assert!((covered(&points, &triangles) - 2.0 * 4.0).abs() < 1e-9);
        for t in triangles.chunks(3) {
            let (a, b, c) = (points[t[0]], points[t[1]], points[t[2]]);
            // Same winding as the counterclockwise input.
            assert!((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) >= 0.0);
        }
    }

    #[test]
    fn degenerate_input_gives_nothing() {
        assert!(earcut(&[], &[]).is_empty());
        assert!(earcut(&[[0.0, 0.0], [1.0, 0.0]], &[]).is_empty());
        assert!(earcut(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], &[]).is_empty());
    }
}
