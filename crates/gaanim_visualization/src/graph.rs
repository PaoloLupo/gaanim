//! Graph layouts: where the nodes of a graph go.
//!
//! Nodes are indices and edges pairs of them. Every layout is computed in a
//! fixed order from the nodes and edges alone, so the same graph always gets
//! the same places, in a seek as in playback:
//!
//! - **Tree**: a tidy tree. Roots are the nodes no edge enters (for a
//!   directed graph) or the first node of each component; each other node
//!   hangs from the first node that reaches it. Leaves take consecutive
//!   columns, each parent sits above the middle of its children, and depth
//!   goes down.
//! - **Layered**: a layered drawing after Sugiyama. In a directed graph,
//!   edges that close a cycle are turned around and each node goes to the
//!   layer of the longest chain of edges that leads to it; without
//!   direction, each node goes as many layers down as edges separate it
//!   from the first node of its component. An edge that skips layers keeps
//!   a place on each layer it crosses, so no node sits in its way, and the
//!   nodes of each layer are then reordered a few times by the mean
//!   position of their neighbors, so edges cross less.
//! - **Circular**: the nodes on a circle, the first at the top, clockwise.
//! - **Force**: a Fruchterman–Reingold layout from seeded starting places:
//!   edges pull their nodes together and every pair of nodes pushes apart,
//!   for a fixed number of steps that cool down.
//!
//! Places are in units of the space between neighbors, with y up; fitting
//! them into a region is up to the caller.

use glam::DVec2;

/// How the nodes of a graph are placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphLayout {
    Tree,
    Layered,
    Circular,
    Force { seed: u64 },
}

impl GraphLayout {
    /// Parse the Python spelling: `"tree"`, `"layered"`, `"circular"` or
    /// `"force"` (which takes `seed`).
    pub fn parse(name: &str, seed: u64) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "tree" => Some(Self::Tree),
            "layered" => Some(Self::Layered),
            "circular" | "circle" => Some(Self::Circular),
            "force" => Some(Self::Force { seed }),
            _ => None,
        }
    }

    /// Whether the layout's two directions mean different things (depth and
    /// order), so it may be stretched to fill a region; the others keep
    /// their proportions.
    pub fn stretches(self) -> bool {
        matches!(self, Self::Tree | Self::Layered)
    }
}

/// Steps of the force layout.
const FORCE_STEPS: usize = 300;
/// Reordering sweeps of the layered layout.
const LAYER_SWEEPS: usize = 8;

/// Places of `count` nodes joined by `edges` (pairs of node indices; edges
/// that name a node past `count`, or a node and itself, are ignored).
/// `directed` decides the roots of a tree and the direction of layers.
pub fn graph_layout(
    count: usize,
    edges: &[(usize, usize)],
    directed: bool,
    layout: GraphLayout,
) -> Vec<DVec2> {
    let edges: Vec<(usize, usize)> = edges
        .iter()
        .copied()
        .filter(|&(a, b)| a < count && b < count && a != b)
        .collect();
    if count == 0 {
        return Vec::new();
    }
    let mut places = match layout {
        GraphLayout::Tree => tree(count, &edges, directed),
        GraphLayout::Layered => layered(count, &edges, directed),
        GraphLayout::Circular => circular(count),
        GraphLayout::Force { seed } => force(count, &edges, seed),
    };
    // Centered on the origin.
    let (min, max) = bounds(&places);
    let center = (min + max) / 2.0;
    for place in &mut places {
        *place -= center;
    }
    places
}

/// The smallest and largest coordinates of `places`.
pub fn bounds(places: &[DVec2]) -> (DVec2, DVec2) {
    places.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(min, max), place| (min.min(*place), max.max(*place)),
    )
}

/// Neighbors of every node: those an edge leads to and, without
/// `directed`, those an edge comes from, in the order of the edges.
fn neighbors(count: usize, edges: &[(usize, usize)], directed: bool) -> Vec<Vec<usize>> {
    let mut next = vec![Vec::new(); count];
    for &(a, b) in edges {
        next[a].push(b);
        if !directed {
            next[b].push(a);
        }
    }
    next
}

fn tree(count: usize, edges: &[(usize, usize)], directed: bool) -> Vec<DVec2> {
    let next = neighbors(count, edges, directed);
    let mut entered = vec![false; count];
    if directed {
        for &(_, b) in edges {
            entered[b] = true;
        }
    }
    // Roots first, then any node a cycle kept unreached, in order.
    let mut parent: Vec<Option<usize>> = vec![None; count];
    let mut reached = vec![false; count];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut roots = Vec::new();
    let candidates: Vec<usize> = (0..count)
        .filter(|node| !entered[*node])
        .chain(0..count)
        .collect();
    for start in candidates {
        if reached[start] {
            continue;
        }
        roots.push(start);
        reached[start] = true;
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            for &child in &next[node] {
                if !reached[child] {
                    reached[child] = true;
                    parent[child] = Some(node);
                    children[node].push(child);
                    queue.push_back(child);
                }
            }
        }
    }
    let mut places = vec![DVec2::ZERO; count];
    let mut column = 0.0;
    for root in roots {
        place_subtree(root, 0, &children, &mut places, &mut column);
    }
    places
}

/// Place the subtree of `node` at `depth`, its leaves from `column` on.
fn place_subtree(
    node: usize,
    depth: usize,
    children: &[Vec<usize>],
    places: &mut [DVec2],
    column: &mut f64,
) {
    // An explicit stack: deep chains must not overflow the call stack.
    let mut stack = vec![(node, depth, false)];
    while let Some((node, depth, done)) = stack.pop() {
        let y = -(depth as f64);
        if children[node].is_empty() {
            places[node] = DVec2::new(*column, y);
            *column += 1.0;
        } else if done {
            let first = places[children[node][0]].x;
            let last = places[*children[node].last().unwrap()].x;
            places[node] = DVec2::new((first + last) / 2.0, y);
        } else {
            stack.push((node, depth, true));
            for &child in children[node].iter().rev() {
                stack.push((child, depth + 1, false));
            }
        }
    }
}

fn layered(count: usize, edges: &[(usize, usize)], directed: bool) -> Vec<DVec2> {
    // Edges point down a layer. Without a direction, those within a layer
    // do not order it.
    let (edges, mut layer) = if directed {
        let edges = acyclic(count, edges);
        let layer = longest_chains(count, &edges);
        (edges, layer)
    } else {
        let layer = breadth_depths(count, edges);
        let edges = edges
            .iter()
            .filter_map(|&(a, b)| match layer[a].cmp(&layer[b]) {
                std::cmp::Ordering::Less => Some((a, b)),
                std::cmp::Ordering::Greater => Some((b, a)),
                std::cmp::Ordering::Equal => None,
            })
            .collect();
        (edges, layer)
    };
    // An edge that skips layers runs through a stand-in node on each layer
    // between, which keeps a place for it there, so no node sits in its way.
    let mut total = count;
    let mut links = Vec::with_capacity(edges.len());
    for &(a, b) in &edges {
        let mut from = a;
        for depth in layer[a] + 1..layer[b] {
            layer.push(depth);
            links.push((from, total));
            from = total;
            total += 1;
        }
        links.push((from, b));
    }
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); total];
    let mut previous: Vec<Vec<usize>> = vec![Vec::new(); total];
    for &(a, b) in &links {
        next[a].push(b);
        previous[b].push(a);
    }
    let layers = layer.iter().copied().max().unwrap_or(0) + 1;
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for node in 0..total {
        rows[layer[node]].push(node);
    }
    let mut order = vec![0.0; total];
    let renumber = |rows: &[Vec<usize>], order: &mut [f64]| {
        for row in rows {
            for (index, node) in row.iter().enumerate() {
                order[*node] = index as f64;
            }
        }
    };
    renumber(&rows, &mut order);
    for sweep in 0..LAYER_SWEEPS {
        let down = sweep % 2 == 0;
        let range: Vec<usize> = if down {
            (1..layers).collect()
        } else {
            (0..layers.saturating_sub(1)).rev().collect()
        };
        for index in range {
            let keys: Vec<(usize, f64)> = rows[index]
                .iter()
                .map(|&node| {
                    let near = if down { &previous[node] } else { &next[node] };
                    let key = if near.is_empty() {
                        order[node]
                    } else {
                        near.iter().map(|other| order[*other]).sum::<f64>() / near.len() as f64
                    };
                    (node, key)
                })
                .collect();
            let mut sorted = keys;
            sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
            rows[index] = sorted.into_iter().map(|(node, _)| node).collect();
            renumber(&rows[index..=index], &mut order);
        }
    }
    let widest = rows.iter().map(Vec::len).max().unwrap_or(1) as f64;
    let mut places = vec![DVec2::ZERO; total];
    for (index, row) in rows.iter().enumerate() {
        let offset = (widest - row.len() as f64) / 2.0;
        for (position, node) in row.iter().enumerate() {
            places[*node] = DVec2::new(offset + position as f64, -(index as f64));
        }
    }
    places.truncate(count);
    places
}

/// The layer of each node: the length of the longest chain of `edges`
/// (which have no cycle) that leads to it.
fn longest_chains(count: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut entering = vec![0usize; count];
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &(a, b) in edges {
        entering[b] += 1;
        next[a].push(b);
    }
    let mut layer = vec![0usize; count];
    let mut ready: Vec<usize> = (0..count)
        .filter(|node| entering[*node] == 0)
        .rev()
        .collect();
    while let Some(node) = ready.pop() {
        for &child in &next[node] {
            layer[child] = layer[child].max(layer[node] + 1);
            entering[child] -= 1;
            if entering[child] == 0 {
                ready.push(child);
            }
        }
    }
    layer
}

/// How many edges separate each node from the first node of its
/// component, breadth first and without direction.
fn breadth_depths(count: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let next = neighbors(count, edges, false);
    let mut depth = vec![usize::MAX; count];
    for start in 0..count {
        if depth[start] != usize::MAX {
            continue;
        }
        depth[start] = 0;
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            for &other in &next[node] {
                if depth[other] == usize::MAX {
                    depth[other] = depth[node] + 1;
                    queue.push_back(other);
                }
            }
        }
    }
    depth
}

/// Directed `edges` without cycles: those that close one, found by a
/// depth-first walk in node order, turn around.
fn acyclic(count: usize, edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let next = neighbors(count, edges, true);
    // 0: unseen, 1: on the walk, 2: done; and when each node was first seen.
    let mut state = vec![0u8; count];
    let mut seen = vec![usize::MAX; count];
    let mut clock = 0;
    for start in 0..count {
        if state[start] != 0 {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state[start] = 1;
        seen[start] = clock;
        clock += 1;
        while let Some((node, index)) = stack.pop() {
            if let Some(&child) = next[node].get(index) {
                stack.push((node, index + 1));
                if state[child] == 0 {
                    state[child] = 1;
                    seen[child] = clock;
                    clock += 1;
                    stack.push((child, 0));
                }
            } else {
                state[node] = 2;
            }
        }
    }
    edges
        .iter()
        .map(|&(a, b)| {
            // An edge turns around when it leads back to an earlier node of
            // its own walk, which is what closes a cycle.
            if closes_cycle(&next, a, b, &seen) {
                (b, a)
            } else {
                (a, b)
            }
        })
        .collect()
}

/// Whether the directed edge `a → b` leads back to an ancestor of `a` in
/// the depth-first walk: `b` was seen first and reaches `a`.
fn closes_cycle(next: &[Vec<usize>], a: usize, b: usize, seen: &[usize]) -> bool {
    if seen[b] > seen[a] {
        return false;
    }
    let mut visited = vec![false; next.len()];
    let mut stack = vec![b];
    while let Some(node) = stack.pop() {
        if node == a {
            return true;
        }
        if std::mem::replace(&mut visited[node], true) {
            continue;
        }
        stack.extend(
            next[node]
                .iter()
                .copied()
                .filter(|other| seen[*other] >= seen[b]),
        );
    }
    false
}

fn circular(count: usize) -> Vec<DVec2> {
    if count == 1 {
        return vec![DVec2::ZERO];
    }
    // Neighbors on the circle stay one unit apart.
    let radius = 0.5 / (std::f64::consts::PI / count as f64).sin();
    (0..count)
        .map(|index| {
            let angle =
                std::f64::consts::FRAC_PI_2 - std::f64::consts::TAU * index as f64 / count as f64;
            DVec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

/// A small deterministic generator (SplitMix64).
struct Random(u64);

impl Random {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn force(count: usize, edges: &[(usize, usize)], seed: u64) -> Vec<DVec2> {
    let side = (count as f64).sqrt();
    let mut random = Random(seed);
    let mut places: Vec<DVec2> = (0..count)
        .map(|_| DVec2::new(random.next(), random.next()) * side)
        .collect();
    // The ideal distance between nodes is one unit.
    let k = 1.0;
    let start = side / 4.0 + 0.5;
    for step in 0..FORCE_STEPS {
        let temperature = start * (1.0 - step as f64 / FORCE_STEPS as f64);
        let mut moves = vec![DVec2::ZERO; count];
        for a in 0..count {
            for b in a + 1..count {
                let mut delta = places[a] - places[b];
                let mut distance = delta.length();
                if distance < 1e-9 {
                    // Coincident nodes part along a fixed direction.
                    delta = DVec2::new(1.0, 0.0);
                    distance = 1e-9;
                }
                let push = delta / distance * (k * k / distance);
                moves[a] += push;
                moves[b] -= push;
            }
        }
        for &(a, b) in edges {
            let delta = places[a] - places[b];
            let distance = delta.length().max(1e-9);
            let pull = delta / distance * (distance * distance / k);
            moves[a] -= pull;
            moves[b] += pull;
        }
        for (place, step) in places.iter_mut().zip(moves) {
            let length = step.length();
            if length > 0.0 {
                *place += step / length * length.min(temperature);
            }
        }
    }
    places
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: DVec2, b: DVec2) -> bool {
        (a - b).length() < 1e-9
    }

    #[test]
    fn a_tree_puts_parents_over_the_middle_of_their_children() {
        // 0 → 1, 0 → 2, 1 → 3, 1 → 4
        let places = graph_layout(
            5,
            &[(0, 1), (0, 2), (1, 3), (1, 4)],
            true,
            GraphLayout::Tree,
        );
        let y: Vec<f64> = places.iter().map(|place| place.y).collect();
        assert!(y[0] > y[1] && (y[1] - y[2]).abs() < 1e-9 && y[1] > y[3]);
        assert!((places[1].x - (places[3].x + places[4].x) / 2.0).abs() < 1e-9);
        assert!((places[0].x - (places[1].x + places[2].x) / 2.0).abs() < 1e-9);
        // Leaves one unit apart, in order.
        assert!((places[4].x - places[3].x - 1.0).abs() < 1e-9);
        assert!(places[2].x > places[4].x);
        // Centered.
        let (min, max) = bounds(&places);
        assert!(close(min + max, DVec2::ZERO));
    }

    #[test]
    fn trees_root_at_unentered_nodes_or_at_the_first_of_each_component() {
        // Directed: 2 is the root, though it comes last.
        let places = graph_layout(3, &[(2, 0), (2, 1)], true, GraphLayout::Tree);
        assert!(places[2].y > places[0].y);
        // Undirected: the first node of each component is its root, and
        // components sit side by side.
        let places = graph_layout(4, &[(1, 0), (3, 2)], false, GraphLayout::Tree);
        assert!(places[1].y < places[0].y && places[3].y < places[2].y);
        assert!(places[2].x > places[0].x);
        // A cycle still gets every node a place.
        let places = graph_layout(3, &[(0, 1), (1, 2), (2, 0)], true, GraphLayout::Tree);
        let mut ys: Vec<f64> = places.iter().map(|place| place.y).collect();
        ys.sort_by(f64::total_cmp);
        assert!(ys[0] < ys[1] && ys[1] < ys[2]);
    }

    #[test]
    fn layers_follow_the_longest_chain_and_cycles_turn_around() {
        // 0 → 1 → 2 and 0 → 2: 2 is two layers down.
        let places = graph_layout(3, &[(0, 1), (1, 2), (0, 2)], true, GraphLayout::Layered);
        assert!(places[0].y > places[1].y && places[1].y > places[2].y);
        // A cycle 0 → 1 → 2 → 0 lays out as a chain.
        let places = graph_layout(3, &[(0, 1), (1, 2), (2, 0)], true, GraphLayout::Layered);
        assert!(places[0].y > places[1].y && places[1].y > places[2].y);
    }

    #[test]
    fn undirected_layers_count_edges_from_the_first_node() {
        // A ring 0 - 1 - 2 - 3 - 0: 1 and 3 are one edge from 0, 2 two.
        let places = graph_layout(
            4,
            &[(0, 1), (1, 2), (2, 3), (3, 0)],
            false,
            GraphLayout::Layered,
        );
        assert!((places[1].y - places[3].y).abs() < 1e-9);
        assert!(places[0].y > places[1].y && places[1].y > places[2].y);
    }

    #[test]
    fn edges_that_skip_layers_keep_their_way_clear() {
        // 0 → 1 → 2 and 0 → 2: the long edge keeps a place beside 1.
        let places = graph_layout(3, &[(0, 1), (1, 2), (0, 2)], true, GraphLayout::Layered);
        assert!((places[0].x - places[2].x).abs() < 1e-9);
        assert!((places[1].x - places[0].x).abs() > 0.4, "{places:?}");
    }

    #[test]
    fn layered_ordering_uncrosses_edges() {
        // 0 → 3 and 1 → 2: the bottom layer swaps to follow its parents.
        let places = graph_layout(4, &[(0, 3), (1, 2)], true, GraphLayout::Layered);
        assert!(places[0].x < places[1].x);
        assert!(places[3].x < places[2].x);
    }

    #[test]
    fn a_circle_starts_at_the_top_and_runs_clockwise() {
        let places = graph_layout(4, &[], false, GraphLayout::Circular);
        assert!(places[0].y > 0.0 && places[0].x.abs() < 1e-9);
        assert!(places[1].x > 0.0 && places[1].y.abs() < 1e-9);
        assert!(((places[0] - places[1]).length() - 1.0).abs() < 1e-9);
        assert_eq!(
            graph_layout(1, &[], false, GraphLayout::Circular),
            [DVec2::ZERO]
        );
    }

    #[test]
    fn force_layouts_repeat_and_keep_neighbors_close() {
        let edges = [(0, 1), (1, 2), (2, 0), (3, 4)];
        let layout = GraphLayout::Force { seed: 7 };
        let a = graph_layout(5, &edges, false, layout);
        let b = graph_layout(5, &edges, false, layout);
        assert_eq!(a, b);
        let c = graph_layout(5, &edges, false, GraphLayout::Force { seed: 8 });
        assert_ne!(a, c);
        for place in &a {
            assert!(place.is_finite());
        }
        // Joined nodes end closer than the farthest pair.
        let farthest = (0..5)
            .flat_map(|i| (0..5).map(move |j| (i, j)))
            .map(|(i, j)| (a[i] - a[j]).length())
            .fold(0.0, f64::max);
        for (i, j) in edges {
            assert!((a[i] - a[j]).length() < farthest);
        }
    }

    #[test]
    fn invalid_edges_are_ignored_and_names_parse() {
        let places = graph_layout(2, &[(0, 0), (0, 5), (0, 1)], true, GraphLayout::Tree);
        assert!(places[0].y > places[1].y);
        assert!(graph_layout(0, &[], false, GraphLayout::Tree).is_empty());
        assert_eq!(GraphLayout::parse("Circle", 0), Some(GraphLayout::Circular));
        assert_eq!(
            GraphLayout::parse("force", 3),
            Some(GraphLayout::Force { seed: 3 })
        );
        assert_eq!(GraphLayout::parse("grid", 0), None);
        assert!(GraphLayout::Tree.stretches() && !GraphLayout::Circular.stretches());
    }
}
