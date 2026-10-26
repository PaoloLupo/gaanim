//! Sankey diagrams: flows between nodes, drawn as ribbons whose widths are
//! proportional to their values.
//!
//! Nodes come from the flows' ends. Each node sits in a layer by the longest
//! chain of flows that leads to it; with [`SankeyAlign::Justify`] the nodes
//! no flow leaves go to the last layer. A node is as tall as the larger of
//! what enters and what leaves it, on one scale for the whole diagram. The
//! layers are then relaxed a fixed number of times, as in d3-sankey: each
//! node moves toward the middle of the flows that reach it, first from the
//! right and then from the left, and the nodes of each layer are pushed
//! apart until they no longer overlap. Everything is computed in a fixed
//! order, so a layout is the same on every run.

use std::collections::HashMap;

use kurbo::{BezPath, Rect};

/// Where nodes without incoming or outgoing flows go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SankeyAlign {
    /// Each node in the layer of its longest incoming chain; nodes that no
    /// flow leaves in the last layer.
    #[default]
    Justify,
    /// Each node in the layer of its longest incoming chain.
    Left,
    /// Each node as far right as its longest outgoing chain allows.
    Right,
}

impl SankeyAlign {
    /// Parse the Python spelling: `"justify"`, `"left"` or `"right"`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "justify" => Some(Self::Justify),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

/// How a sankey diagram is laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SankeyOptions {
    /// Width of each node's bar.
    pub node_width: f64,
    /// Vertical gap between the nodes of a layer.
    pub node_gap: f64,
    /// Relaxation passes.
    pub iterations: usize,
    pub align: SankeyAlign,
}

impl Default for SankeyOptions {
    fn default() -> Self {
        Self {
            node_width: 0.2,
            node_gap: 0.2,
            iterations: 6,
            align: SankeyAlign::Justify,
        }
    }
}

/// One node of a laid-out sankey diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyNode {
    pub name: String,
    /// The larger of what enters and what leaves the node.
    pub value: f64,
    /// Its layer, from 0 at the left.
    pub layer: usize,
    /// Its bar, in the diagram's coordinates (y up).
    pub rect: Rect,
}

/// One flow of a laid-out sankey diagram: a ribbon from the right edge of
/// its source to the left edge of its target.
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyLink {
    /// Indices of its ends in the nodes.
    pub source: usize,
    pub target: usize,
    pub value: f64,
    /// The ribbon's width, on the nodes' scale.
    pub width: f64,
    /// The middle of the ribbon where it leaves its source and where it
    /// reaches its target.
    pub source_y: f64,
    pub target_y: f64,
}

/// A laid-out sankey diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyLayout {
    /// Nodes, in the order their names first appear in the flows.
    pub nodes: Vec<SankeyNode>,
    /// Flows, in their given order.
    pub links: Vec<SankeyLink>,
}

/// Invalid sankey data.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SankeyError {
    #[error("a sankey diagram needs at least one flow with a positive value")]
    Empty,
    #[error("flow {flow} has the value {value}; values must be finite and not negative")]
    InvalidValue { flow: usize, value: f64 },
    #[error("flow {flow} goes from '{node}' to itself")]
    SelfLink { flow: usize, node: String },
    #[error("the flows run in a circle through '{node}'")]
    Cycle { node: String },
    #[error("the diagram's rectangle must be wider than its node width and have a positive height")]
    InvalidRect,
    #[error("node width must be finite and positive, and node gap finite and not negative")]
    InvalidOptions,
}

/// Lay out `flows`, each `(source, target, value)`, inside `rect`.
pub fn sankey(
    flows: &[(String, String, f64)],
    rect: Rect,
    options: SankeyOptions,
) -> Result<SankeyLayout, SankeyError> {
    if !(options.node_width.is_finite()
        && options.node_width > 0.0
        && options.node_gap.is_finite()
        && options.node_gap >= 0.0)
    {
        return Err(SankeyError::InvalidOptions);
    }
    if !(rect.width() > options.node_width && rect.height() > 0.0) {
        return Err(SankeyError::InvalidRect);
    }
    let mut names: Vec<String> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut ends = Vec::with_capacity(flows.len());
    for (flow, (source, target, value)) in flows.iter().enumerate() {
        if !value.is_finite() || *value < 0.0 {
            return Err(SankeyError::InvalidValue {
                flow,
                value: *value,
            });
        }
        if source == target {
            return Err(SankeyError::SelfLink {
                flow,
                node: source.clone(),
            });
        }
        let mut pair = [0; 2];
        for (slot, name) in pair.iter_mut().zip([source, target]) {
            *slot = *index.entry(name.as_str()).or_insert_with(|| {
                names.push(name.clone());
                names.len() - 1
            });
        }
        ends.push((pair[0], pair[1]));
    }
    let count = names.len();
    let mut incoming = vec![0.0; count];
    let mut outgoing = vec![0.0; count];
    let mut targets: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut sources: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (flow, &(source, target)) in ends.iter().enumerate() {
        outgoing[source] += flows[flow].2;
        incoming[target] += flows[flow].2;
        targets[source].push(flow);
        sources[target].push(flow);
    }
    let values: Vec<f64> = (0..count)
        .map(|node| incoming[node].max(outgoing[node]))
        .collect();
    if !values.iter().any(|value| *value > 0.0) {
        return Err(SankeyError::Empty);
    }

    // Longest chains, in a topological order.
    let order = topological(&ends, count).map_err(|node| SankeyError::Cycle {
        node: names[node].clone(),
    })?;
    let mut depth = vec![0usize; count];
    for &node in &order {
        for &flow in &targets[node] {
            let target = ends[flow].1;
            depth[target] = depth[target].max(depth[node] + 1);
        }
    }
    let mut height = vec![0usize; count];
    for &node in order.iter().rev() {
        for &flow in &targets[node] {
            height[node] = height[node].max(height[ends[flow].1] + 1);
        }
    }
    let last = depth.iter().copied().max().unwrap_or(0);
    let layers: Vec<usize> = (0..count)
        .map(|node| match options.align {
            SankeyAlign::Left => depth[node],
            SankeyAlign::Right => last - height[node],
            SankeyAlign::Justify if targets[node].is_empty() => last,
            SankeyAlign::Justify => depth[node],
        })
        .collect();
    let columns = layers.iter().copied().max().unwrap_or(0) + 1;
    let mut column_nodes: Vec<Vec<usize>> = vec![Vec::new(); columns];
    for node in 0..count {
        column_nodes[layers[node]].push(node);
    }

    // One scale for every layer: the fullest one fills the height.
    let total_height = rect.height();
    let longest = column_nodes.iter().map(Vec::len).max().unwrap_or(1);
    let gap = if longest > 1 {
        options
            .node_gap
            .min(total_height / (2.0 * (longest - 1) as f64))
    } else {
        options.node_gap
    };
    let scale = column_nodes
        .iter()
        .filter_map(|nodes| {
            let sum: f64 = nodes.iter().map(|node| values[*node]).sum();
            (sum > 0.0).then(|| (total_height - gap * (nodes.len() - 1) as f64) / sum)
        })
        .fold(f64::INFINITY, f64::min);

    // Positions measured down from the top of `rect`.
    let mut tops = vec![0.0; count];
    let sizes: Vec<f64> = values.iter().map(|value| value * scale).collect();
    for nodes in &column_nodes {
        let mut top = 0.0;
        for &node in nodes {
            tops[node] = top;
            top += sizes[node] + gap;
        }
        // Center each layer, as the relaxation then works from both sides.
        let used = top - gap;
        let shift = (total_height - used).max(0.0) / 2.0;
        for &node in nodes {
            tops[node] += shift;
        }
    }
    let middle = |tops: &[f64], node: usize| tops[node] + sizes[node] / 2.0;
    let mut alpha = 1.0;
    for _ in 0..options.iterations {
        alpha *= 0.99;
        // From the right: each node toward the flows it sends on.
        for nodes in column_nodes.iter().rev().skip(1) {
            for &node in nodes {
                let (mut sum, mut weight) = (0.0, 0.0);
                for &flow in &targets[node] {
                    let other = ends[flow].1;
                    let w = flows[flow].2 * (layers[other] - layers[node]) as f64;
                    sum += middle(&tops, other) * w;
                    weight += w;
                }
                if weight > 0.0 {
                    tops[node] += (sum / weight - middle(&tops, node)) * alpha;
                }
            }
            resolve_collisions(nodes, &mut tops, &sizes, gap, total_height);
        }
        // From the left: each node toward the flows that reach it.
        for nodes in column_nodes.iter().skip(1) {
            for &node in nodes {
                let (mut sum, mut weight) = (0.0, 0.0);
                for &flow in &sources[node] {
                    let other = ends[flow].0;
                    let w = flows[flow].2 * (layers[node] - layers[other]) as f64;
                    sum += middle(&tops, other) * w;
                    weight += w;
                }
                if weight > 0.0 {
                    tops[node] += (sum / weight - middle(&tops, node)) * alpha;
                }
            }
            resolve_collisions(nodes, &mut tops, &sizes, gap, total_height);
        }
    }

    // Each node stacks its flows in the order of the nodes at their other
    // ends, top to bottom, so ribbons cross as little as they can.
    let mut source_offset = vec![0.0; flows.len()];
    let mut target_offset = vec![0.0; flows.len()];
    for node in 0..count {
        let mut out = targets[node].clone();
        out.sort_by(|a, b| {
            middle(&tops, ends[*a].1)
                .total_cmp(&middle(&tops, ends[*b].1))
                .then(a.cmp(b))
        });
        let mut offset = 0.0;
        for flow in out {
            source_offset[flow] = offset;
            offset += flows[flow].2 * scale;
        }
        let mut into = sources[node].clone();
        into.sort_by(|a, b| {
            middle(&tops, ends[*a].0)
                .total_cmp(&middle(&tops, ends[*b].0))
                .then(a.cmp(b))
        });
        let mut offset = 0.0;
        for flow in into {
            target_offset[flow] = offset;
            offset += flows[flow].2 * scale;
        }
    }

    let step = if columns > 1 {
        (rect.width() - options.node_width) / (columns - 1) as f64
    } else {
        0.0
    };
    let left = |layer: usize| {
        if columns > 1 {
            rect.x0 + layer as f64 * step
        } else {
            rect.center().x - options.node_width / 2.0
        }
    };
    let down = |offset: f64| rect.y1 - offset;
    let nodes = (0..count)
        .map(|node| SankeyNode {
            name: names[node].clone(),
            value: values[node],
            layer: layers[node],
            rect: Rect::new(
                left(layers[node]),
                down(tops[node] + sizes[node]),
                left(layers[node]) + options.node_width,
                down(tops[node]),
            ),
        })
        .collect();
    let links = ends
        .iter()
        .enumerate()
        .map(|(flow, &(source, target))| {
            let width = flows[flow].2 * scale;
            SankeyLink {
                source,
                target,
                value: flows[flow].2,
                width,
                source_y: down(tops[source] + source_offset[flow] + width / 2.0),
                target_y: down(tops[target] + target_offset[flow] + width / 2.0),
            }
        })
        .collect();
    Ok(SankeyLayout { nodes, links })
}

/// Nodes in an order where every flow's source comes before its target,
/// or a node on a cycle.
fn topological(ends: &[(usize, usize)], count: usize) -> Result<Vec<usize>, usize> {
    let mut entering = vec![0usize; count];
    for &(_, target) in ends {
        entering[target] += 1;
    }
    let mut ready: Vec<usize> = (0..count).filter(|node| entering[*node] == 0).collect();
    ready.reverse();
    let mut order = Vec::with_capacity(count);
    while let Some(node) = ready.pop() {
        order.push(node);
        for &(source, target) in ends {
            if source == node {
                entering[target] -= 1;
                if entering[target] == 0 {
                    ready.push(target);
                }
            }
        }
    }
    if order.len() < count {
        return Err((0..count).find(|node| entering[*node] > 0).unwrap_or(0));
    }
    Ok(order)
}

/// Push the `nodes` of one layer apart, keeping them inside `0..height`.
fn resolve_collisions(nodes: &[usize], tops: &mut [f64], sizes: &[f64], gap: f64, height: f64) {
    let mut sorted = nodes.to_vec();
    sorted.sort_by(|a, b| tops[*a].total_cmp(&tops[*b]).then(a.cmp(b)));
    let mut floor = 0.0;
    for &node in &sorted {
        if tops[node] < floor {
            tops[node] = floor;
        }
        floor = tops[node] + sizes[node] + gap;
    }
    let mut ceiling = height;
    for &node in sorted.iter().rev() {
        if tops[node] + sizes[node] > ceiling {
            tops[node] = ceiling - sizes[node];
        }
        ceiling = tops[node] - gap;
    }
}

/// The ribbon of `link` in `layout`: its upper edge from source to target,
/// eased like d3-sankey's horizontal link, and its lower edge back.
pub fn ribbon(layout: &SankeyLayout, link: &SankeyLink) -> BezPath {
    let x0 = layout.nodes[link.source].rect.x1;
    let x1 = layout.nodes[link.target].rect.x0;
    let mid = (x0 + x1) / 2.0;
    let half = link.width / 2.0;
    let mut path = BezPath::new();
    path.move_to((x0, link.source_y + half));
    path.curve_to(
        (mid, link.source_y + half),
        (mid, link.target_y + half),
        (x1, link.target_y + half),
    );
    path.line_to((x1, link.target_y - half));
    path.curve_to(
        (mid, link.target_y - half),
        (mid, link.source_y - half),
        (x0, link.source_y - half),
    );
    path.close_path();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flows(items: &[(&str, &str, f64)]) -> Vec<(String, String, f64)> {
        items
            .iter()
            .map(|(a, b, value)| (a.to_string(), b.to_string(), *value))
            .collect()
    }

    fn energy() -> Vec<(String, String, f64)> {
        flows(&[
            ("coal", "power", 30.0),
            ("gas", "power", 20.0),
            ("gas", "homes", 10.0),
            ("power", "homes", 25.0),
            ("power", "industry", 25.0),
            ("solar", "power", 10.0),
        ])
    }

    fn rect() -> Rect {
        Rect::new(-4.0, -2.0, 4.0, 2.0)
    }

    #[test]
    fn nodes_take_layers_and_heights_on_one_scale() {
        let layout = sankey(&energy(), rect(), SankeyOptions::default()).unwrap();
        let names: Vec<&str> = layout.nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(
            names,
            ["coal", "power", "gas", "homes", "industry", "solar"]
        );
        let layer = |name: &str| layout.nodes.iter().find(|n| n.name == name).unwrap().layer;
        assert_eq!(
            [
                layer("coal"),
                layer("gas"),
                layer("solar"),
                layer("power"),
                layer("homes"),
                layer("industry")
            ],
            [0, 0, 0, 1, 2, 2]
        );
        // Power: 60 in, 50 out. Heights are proportional to values.
        let power = &layout.nodes[1];
        assert_eq!(power.value, 60.0);
        let unit = power.rect.height() / 60.0;
        for node in &layout.nodes {
            assert!(
                (node.rect.height() - node.value * unit).abs() < 1e-9,
                "{}",
                node.name
            );
            assert!(node.rect.y0 >= rect().y0 - 1e-9 && node.rect.y1 <= rect().y1 + 1e-9);
        }
        // The first and last layers touch the sides.
        assert!((layout.nodes[0].rect.x0 - rect().x0).abs() < 1e-9);
        assert!((layout.nodes[3].rect.x1 - rect().x1).abs() < 1e-9);
        for link in &layout.links {
            assert!((link.width - link.value * unit).abs() < 1e-9);
        }
    }

    #[test]
    fn a_layer_does_not_overlap_and_flows_stack_inside_their_nodes() {
        let layout = sankey(&energy(), rect(), SankeyOptions::default()).unwrap();
        for layer in 0..3 {
            let mut bars: Vec<Rect> = layout
                .nodes
                .iter()
                .filter(|node| node.layer == layer)
                .map(|node| node.rect)
                .collect();
            bars.sort_by(|a, b| a.y0.total_cmp(&b.y0));
            for pair in bars.windows(2) {
                assert!(pair[0].y1 <= pair[1].y0 + 1e-9, "{pair:?}");
            }
        }
        for link in &layout.links {
            let (source, target) = (
                layout.nodes[link.source].rect,
                layout.nodes[link.target].rect,
            );
            let half = link.width / 2.0;
            assert!(link.source_y + half <= source.y1 + 1e-9);
            assert!(link.source_y - half >= source.y0 - 1e-9);
            assert!(link.target_y + half <= target.y1 + 1e-9);
            assert!(link.target_y - half >= target.y0 - 1e-9);
        }
        // Flows leaving power do not overlap one another.
        let mut out: Vec<(f64, f64)> = layout
            .links
            .iter()
            .filter(|link| link.source == 1)
            .map(|link| {
                (
                    link.source_y - link.width / 2.0,
                    link.source_y + link.width / 2.0,
                )
            })
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(out[0].1 <= out[1].0 + 1e-9);
    }

    #[test]
    fn layouts_repeat_exactly() {
        let a = sankey(&energy(), rect(), SankeyOptions::default()).unwrap();
        let b = sankey(&energy(), rect(), SankeyOptions::default()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn alignments_place_the_ends() {
        let data = flows(&[
            ("a", "b", 1.0),
            ("b", "c", 1.0),
            ("x", "c", 1.0),
            ("a", "y", 1.0),
        ]);
        let layer = |align, name: &str| {
            let layout = sankey(
                &data,
                rect(),
                SankeyOptions {
                    align,
                    ..Default::default()
                },
            )
            .unwrap();
            layout
                .nodes
                .iter()
                .find(|node| node.name == name)
                .unwrap()
                .layer
        };
        assert_eq!(layer(SankeyAlign::Justify, "y"), 2);
        assert_eq!(layer(SankeyAlign::Left, "y"), 1);
        assert_eq!(layer(SankeyAlign::Left, "x"), 0);
        assert_eq!(layer(SankeyAlign::Right, "x"), 1);
        assert_eq!(SankeyAlign::parse("RIGHT"), Some(SankeyAlign::Right));
        assert_eq!(SankeyAlign::parse("top"), None);
    }

    #[test]
    fn a_ribbon_joins_the_edges_of_its_nodes() {
        let layout = sankey(&energy(), rect(), SankeyOptions::default()).unwrap();
        let link = &layout.links[0];
        let path = ribbon(&layout, link);
        let bounds = kurbo::Shape::bounding_box(&path);
        assert!((bounds.x0 - layout.nodes[link.source].rect.x1).abs() < 1e-9);
        assert!((bounds.x1 - layout.nodes[link.target].rect.x0).abs() < 1e-9);
    }

    #[test]
    fn invalid_flows_are_rejected() {
        let options = SankeyOptions::default();
        assert_eq!(sankey(&[], rect(), options), Err(SankeyError::Empty));
        assert_eq!(
            sankey(&flows(&[("a", "b", 0.0)]), rect(), options),
            Err(SankeyError::Empty)
        );
        assert!(matches!(
            sankey(&flows(&[("a", "b", -1.0)]), rect(), options),
            Err(SankeyError::InvalidValue { flow: 0, .. })
        ));
        assert_eq!(
            sankey(&flows(&[("a", "a", 1.0)]), rect(), options),
            Err(SankeyError::SelfLink {
                flow: 0,
                node: "a".to_owned()
            })
        );
        assert!(matches!(
            sankey(
                &flows(&[("a", "b", 1.0), ("b", "c", 1.0), ("c", "a", 1.0)]),
                rect(),
                options
            ),
            Err(SankeyError::Cycle { .. })
        ));
        assert_eq!(
            sankey(
                &flows(&[("a", "b", 1.0)]),
                Rect::new(0.0, 0.0, 0.1, 1.0),
                options
            ),
            Err(SankeyError::InvalidRect)
        );
        assert_eq!(
            sankey(
                &flows(&[("a", "b", 1.0)]),
                rect(),
                SankeyOptions {
                    node_gap: -1.0,
                    ..options
                }
            ),
            Err(SankeyError::InvalidOptions)
        );
        assert_eq!(
            sankey(
                &flows(&[("a", "b", 1.0)]),
                rect(),
                SankeyOptions {
                    node_width: 0.0,
                    ..options
                }
            ),
            Err(SankeyError::InvalidOptions)
        );
    }
}
