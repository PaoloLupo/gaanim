//! Graphs and trees: `scene.viz.graph` (layouts in [`gaanim_visualization::graph`]).
//!
//! Every node is a circle with its name, grouped and placed where the layout
//! puts it; every edge is a reactive line or arrow between the edges of two
//! circles, so it follows its nodes wherever they move. A change of
//! structure lays the graph out again: kept nodes move to their new places,
//! new nodes and edges fade in there, and removed ones fade out.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gaanim_core::glam::DVec2;
use gaanim_core::kurbo::Rect;
use gaanim_core::peniko::Color;
use gaanim_text::prelude::{TextFlow, TextRole, TextSpec, TextStyle};
use gaanim_visualization::GraphLayout;

use super::ops::Op;
use super::{Anchor, Anim, CanvasEndpoint, Composition, DrawableHandle, SceneModel};

/// Most scene units between neighbors: a small graph does not spread over
/// its whole region.
const MAX_SPACING: f64 = 2.4;

/// How a graph is laid out and drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphOptions {
    pub layout: GraphLayout,
    /// Whether edges are arrows from their first node to their second.
    pub directed: bool,
    /// Smallest radius of a node; a longer name widens its node.
    pub node_radius: f64,
    /// Whether nodes show their names.
    pub labels: bool,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            layout: GraphLayout::Layered,
            directed: true,
            node_radius: 0.3,
            labels: true,
        }
    }
}

/// Invalid graph structure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    #[error("a graph needs at least one node")]
    Empty,
    #[error("the node {0:?} appears twice")]
    DuplicateNode(String),
    #[error("{0:?} is not a node of the graph")]
    UnknownNode(String),
    #[error("an edge from {0:?} to itself")]
    SelfLoop(String),
    #[error("there is no edge from {0:?} to {1:?}")]
    UnknownEdge(String, String),
    #[error("node radius must be a finite positive number")]
    InvalidRadius,
}

/// One node of a drawn graph.
#[derive(Clone)]
pub struct GraphNode {
    pub name: String,
    /// The circle and its name, moved together.
    pub group: DrawableHandle,
    pub circle: DrawableHandle,
    pub label: Option<DrawableHandle>,
    pub radius: f64,
    /// Where the layout put it, in the graph's frame.
    pub place: DVec2,
}

/// One edge of a drawn graph.
#[derive(Clone)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub drawable: DrawableHandle,
}

struct GraphParts {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

/// A graph: its nodes and edges, and how to lay them out again.
#[derive(Clone)]
pub struct GraphHandle {
    /// The whole graph.
    pub group: DrawableHandle,
    /// The group nodes and edges sit in, its origin at the scene's.
    slot: DrawableHandle,
    region: Rect,
    options: Arc<GraphOptions>,
    current: Arc<Mutex<GraphParts>>,
}

/// Where the nodes go in `region`: the layout scaled to fill it, inset by
/// `margin`, without more than [`MAX_SPACING`] between neighbors, and
/// centered.
fn fit(places: &[DVec2], region: Rect, margin: f64, stretch: bool) -> Vec<DVec2> {
    let (min, max) = gaanim_visualization::graph::bounds(places);
    let span = max - min;
    let room = DVec2::new(
        (region.width() - 2.0 * margin).max(0.0),
        (region.height() - 2.0 * margin).max(0.0),
    );
    let scale = |room: f64, span: f64| {
        if span > 1e-9 {
            (room / span).min(MAX_SPACING)
        } else {
            MAX_SPACING
        }
    };
    let (mut sx, mut sy) = (scale(room.x, span.x), scale(room.y, span.y));
    if !stretch {
        let both = sx.min(sy);
        (sx, sy) = (both, both);
    }
    let center = DVec2::new(region.center().x, region.center().y);
    let middle = (min + max) / 2.0;
    places
        .iter()
        .map(|place| center + (*place - middle) * DVec2::new(sx, sy))
        .collect()
}

impl SceneModel {
    /// A graph of `nodes` joined by `edges` (pairs of node names), laid
    /// out in `region`.
    pub fn graph(
        &mut self,
        nodes: &[String],
        edges: &[(String, String)],
        region: Rect,
        options: GraphOptions,
    ) -> Result<GraphHandle, GraphError> {
        if !(options.node_radius.is_finite() && options.node_radius > 0.0) {
            return Err(GraphError::InvalidRadius);
        }
        check_structure(nodes, edges)?;
        let mut parts = GraphParts {
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        for name in nodes {
            parts.nodes.push(self.graph_node(name, &options));
        }
        let places = graph_places(&parts.nodes, edges, region, &options);
        for (node, place) in parts.nodes.iter_mut().zip(places) {
            node.group.clone().move_to(place.x, place.y);
            node.place = place;
        }
        for (from, to) in edges {
            let edge = self.graph_edge(&parts.nodes, from, to, &options);
            parts.edges.push(edge);
        }
        let members: Vec<&DrawableHandle> = parts
            .edges
            .iter()
            .map(|edge| &edge.drawable)
            .chain(parts.nodes.iter().map(|node| &node.group))
            .collect();
        let slot = self.group_no_center(&members);
        let group = self.group(&[&slot]);
        Ok(GraphHandle {
            group,
            slot,
            region,
            options: Arc::new(options),
            current: Arc::new(Mutex::new(parts)),
        })
    }

    fn graph_node(&mut self, name: &str, options: &GraphOptions) -> GraphNode {
        let fill = self
            .theme_color("accent")
            .unwrap_or(Color::from_rgb8(0xF2, 0xA5, 0x41));
        let foreground = self.theme_color("foreground").unwrap_or(Color::WHITE);
        let background = self.theme_color("background").unwrap_or(Color::BLACK);
        let ink = if gaanim_text::code::contrast(foreground, fill)
            >= gaanim_text::code::contrast(background, fill)
        {
            foreground
        } else {
            background
        };
        let size = self.themed_text_config().roles[&TextRole::Body].size * 0.8;
        let spec = options
            .labels
            .then(|| {
                TextSpec::new(
                    vec![name.into()],
                    None,
                    TextStyle {
                        size: Some(size.min(options.node_radius * 1.1)),
                        color: Some(ink),
                        ..TextStyle::default()
                    },
                    TextFlow::default(),
                )
                .ok()
            })
            .flatten();
        let mut radius = options.node_radius;
        if let Some(spec) = &spec
            && let Ok((width, height)) = self.measure_text_spec(spec)
        {
            let pad = options.node_radius * 0.3;
            radius = radius.max((width * width + height * height).sqrt() / 2.0 + pad);
        }
        // The circle first, so its name draws over it.
        let circle = self.circle(radius).fill(fill).no_stroke();
        let label = spec.map(|spec| self.text_spec(spec).at_anchor(0.0, 0.0, Anchor::Center));
        let members: Vec<&DrawableHandle> =
            std::iter::once(&circle).chain(label.as_ref()).collect();
        // Over the edges, which are made after their nodes.
        let group = self.group(&members).named(name).z_index(1);
        GraphNode {
            name: name.to_owned(),
            group,
            circle,
            label,
            radius,
            place: DVec2::ZERO,
        }
    }

    fn graph_edge(
        &mut self,
        nodes: &[GraphNode],
        from: &str,
        to: &str,
        options: &GraphOptions,
    ) -> GraphEdge {
        let node = |name: &str| {
            nodes
                .iter()
                .find(|node| node.name == name)
                .expect("edges join nodes of the graph")
        };
        let (a, b) = (node(from), node(to));
        // A little outside each circle, in its own frame, so edges keep
        // their gap when a node or the graph scales.
        let gap = options.node_radius * 0.15;
        let end = |at: &GraphNode, other: &GraphNode| CanvasEndpoint::Rim {
            object: at.circle.id,
            toward: Box::new(CanvasEndpoint::Entity(other.circle.id)),
            radius: at.radius + gap,
        };
        let color = self
            .theme_color("muted")
            .unwrap_or(Color::from_rgb8(0x9A, 0x9A, 0x9A));
        let (start, finish) = (end(a, b), end(b, a));
        let drawable = if options.directed {
            let size = options.node_radius;
            self.connector(
                start,
                finish,
                Vec::new(),
                size * 0.5,
                size * 0.42,
                size * 0.08,
                None,
            )
            .expect("graph edges join drawables of the scene")
            .fill(color)
        } else {
            self.line_between(start, finish)
                .stroke(color, options.node_radius * 0.08)
        };
        GraphEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            drawable: drawable.named(format!("{from}→{to}")),
        }
    }
}

fn check_structure(nodes: &[String], edges: &[(String, String)]) -> Result<(), GraphError> {
    if nodes.is_empty() {
        return Err(GraphError::Empty);
    }
    let mut seen = std::collections::HashSet::new();
    for name in nodes {
        if !seen.insert(name.as_str()) {
            return Err(GraphError::DuplicateNode(name.clone()));
        }
    }
    for (from, to) in edges {
        for name in [from, to] {
            if !seen.contains(name.as_str()) {
                return Err(GraphError::UnknownNode(name.clone()));
            }
        }
        if from == to {
            return Err(GraphError::SelfLoop(from.clone()));
        }
    }
    Ok(())
}

/// Where `nodes` go in `region` for `edges`.
fn graph_places(
    nodes: &[GraphNode],
    edges: &[(String, String)],
    region: Rect,
    options: &GraphOptions,
) -> Vec<DVec2> {
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.name.as_str(), index))
        .collect();
    let pairs: Vec<(usize, usize)> = edges
        .iter()
        .filter_map(|(from, to)| Some((*index.get(from.as_str())?, *index.get(to.as_str())?)))
        .collect();
    let places =
        gaanim_visualization::graph_layout(nodes.len(), &pairs, options.directed, options.layout);
    let margin = nodes.iter().map(|node| node.radius).fold(0.0, f64::max);
    fit(&places, region, margin, options.layout.stretches())
}

impl GraphHandle {
    /// The node names, in order.
    pub fn nodes(&self) -> Vec<String> {
        let parts = self.current.lock().expect("graph poisoned");
        parts.nodes.iter().map(|node| node.name.clone()).collect()
    }

    /// The edges, as pairs of node names, in order.
    pub fn edges(&self) -> Vec<(String, String)> {
        let parts = self.current.lock().expect("graph poisoned");
        parts
            .edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone()))
            .collect()
    }

    /// The circle and name of the node `name`.
    pub fn node(&self, name: &str) -> Option<DrawableHandle> {
        let parts = self.current.lock().expect("graph poisoned");
        parts
            .nodes
            .iter()
            .find(|node| node.name == name)
            .map(|node| node.group.clone())
    }

    /// The first edge from `from` to `to`; without direction, either way.
    pub fn edge(&self, from: &str, to: &str) -> Option<DrawableHandle> {
        let parts = self.current.lock().expect("graph poisoned");
        let directed = self.options.directed;
        parts
            .edges
            .iter()
            .find(|edge| {
                (edge.from == from && edge.to == to)
                    || (!directed && edge.from == to && edge.to == from)
            })
            .map(|edge| edge.drawable.clone())
    }

    /// The change to `nodes` and `edges`: nodes both have move to their new
    /// places, the others fade in or out where they are, and so do edges.
    /// New parts stay hidden until the change plays. Each part takes
    /// `duration`, when given. Later changes start from the new structure.
    /// `scene` is the graph's scene.
    pub fn to(
        &self,
        scene: &mut SceneModel,
        nodes: &[String],
        edges: &[(String, String)],
        duration: Option<f64>,
    ) -> Result<Composition, GraphError> {
        check_structure(nodes, edges)?;
        let mut parts = self.current.lock().expect("graph poisoned");
        let mut children = Vec::new();
        let mut push = |anim: Anim| {
            children.push(Composition::leaf(match duration {
                Some(seconds) => anim.duration(seconds),
                None => anim,
            }))
        };
        let mut next_nodes = Vec::with_capacity(nodes.len());
        let mut entering = Vec::new();
        for name in nodes {
            match parts.nodes.iter().find(|node| &node.name == name) {
                Some(node) => next_nodes.push(node.clone()),
                None => {
                    let node = scene.graph_node(name, &self.options);
                    node.group.defer_visibility_until_play();
                    self.attach(scene, &node.group);
                    entering.push(node.name.clone());
                    next_nodes.push(node);
                }
            }
        }
        // Kept nodes move by the change of their place in the graph's frame,
        // so a graph that moved, scaled or turned keeps them inside it.
        let places = graph_places(&next_nodes, edges, self.region, &self.options);
        for (node, place) in next_nodes.iter_mut().zip(places) {
            if entering.contains(&node.name) {
                node.group.clone().move_to(place.x, place.y);
                push(node.group.animate().fade_in());
            } else {
                let delta = place - node.place;
                push(node.group.animate().shift_by(delta.x, delta.y));
            }
            node.place = place;
        }
        for node in &parts.nodes {
            if !nodes.contains(&node.name) {
                push(node.group.animate().fade_out());
            }
        }
        // Edges pair in order: the n-th edge between two nodes is kept when
        // the new structure has an n-th one too.
        let mut kept: Vec<bool> = vec![false; parts.edges.len()];
        let mut next_edges = Vec::with_capacity(edges.len());
        for (from, to) in edges {
            let existing = parts
                .edges
                .iter()
                .enumerate()
                .find(|(index, edge)| !kept[*index] && &edge.from == from && &edge.to == to);
            match existing {
                Some((index, edge)) => {
                    kept[index] = true;
                    next_edges.push(edge.clone());
                }
                None => {
                    let edge = scene.graph_edge(&next_nodes, from, to, &self.options);
                    edge.drawable.defer_visibility_until_play();
                    self.attach(scene, &edge.drawable);
                    push(edge.drawable.animate().fade_in());
                    next_edges.push(edge);
                }
            }
        }
        for (edge, kept) in parts.edges.iter().zip(&kept) {
            if !kept {
                push(edge.drawable.animate().fade_out());
            }
        }
        *parts = GraphParts {
            nodes: next_nodes,
            edges: next_edges,
        };
        Ok(Composition::parallel(children).expect("a graph has at least one node"))
    }

    /// Add `name`, joined by `edges`, and lay the graph out again.
    pub fn add_node(
        &self,
        scene: &mut SceneModel,
        name: &str,
        edges: &[(String, String)],
        duration: Option<f64>,
    ) -> Result<Composition, GraphError> {
        let (mut nodes, mut all) = (self.nodes(), self.edges());
        if nodes.iter().any(|node| node == name) {
            return Err(GraphError::DuplicateNode(name.to_owned()));
        }
        nodes.push(name.to_owned());
        all.extend(edges.iter().cloned());
        self.to(scene, &nodes, &all, duration)
    }

    /// Remove `name` and its edges, and lay the graph out again.
    pub fn remove_node(
        &self,
        scene: &mut SceneModel,
        name: &str,
        duration: Option<f64>,
    ) -> Result<Composition, GraphError> {
        let nodes = self.nodes();
        if !nodes.iter().any(|node| node == name) {
            return Err(GraphError::UnknownNode(name.to_owned()));
        }
        let nodes: Vec<String> = nodes.into_iter().filter(|node| node != name).collect();
        let edges: Vec<(String, String)> = self
            .edges()
            .into_iter()
            .filter(|(from, to)| from != name && to != name)
            .collect();
        self.to(scene, &nodes, &edges, duration)
    }

    /// Add an edge from `from` to `to`, and lay the graph out again.
    pub fn add_edge(
        &self,
        scene: &mut SceneModel,
        from: &str,
        to: &str,
        duration: Option<f64>,
    ) -> Result<Composition, GraphError> {
        let mut edges = self.edges();
        edges.push((from.to_owned(), to.to_owned()));
        self.to(scene, &self.nodes(), &edges, duration)
    }

    /// Remove the first edge from `from` to `to` (either way without
    /// direction), and lay the graph out again.
    pub fn remove_edge(
        &self,
        scene: &mut SceneModel,
        from: &str,
        to: &str,
        duration: Option<f64>,
    ) -> Result<Composition, GraphError> {
        let mut edges = self.edges();
        let directed = self.options.directed;
        let index = edges
            .iter()
            .position(|(a, b)| (a == from && b == to) || (!directed && a == to && b == from))
            .ok_or_else(|| GraphError::UnknownEdge(from.to_owned(), to.to_owned()))?;
        edges.remove(index);
        self.to(scene, &self.nodes(), &edges, duration)
    }

    /// Put `part` in the graph's group, where it was placed.
    fn attach(&self, scene: &mut SceneModel, part: &DrawableHandle) {
        scene
            .state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachToGroupLocal {
                group: self.slot.id,
                child: part.id,
            });
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::world::CommandQueue;
    use bevy::prelude::{ChildOf, Commands, Entity, World};
    use gaanim_animation::updaters::{TrackingConnector, tracking_line_system};
    use gaanim_core::kurbo::{Affine, PathEl, Shape};
    use gaanim_math::SpatialTransform;
    use gaanim_scene::Visibility;
    use gaanim_scene::components::{Opacity, Path2D};
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    use super::*;

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|name| name.to_string()).collect()
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect()
    }

    fn region() -> Rect {
        Rect::new(-4.0, -2.0, 4.0, 2.0)
    }

    fn compiled(scene: &SceneModel) -> (World, Timeline) {
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let config = gaanim_text::prelude::TextConfig::default();
        scene.compile_into(&mut commands, &mut timeline, &fonts, &config);
        queue.apply(&mut world);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        (world, timeline)
    }

    /// The scene transform of `entity` and how opaque it is drawn.
    fn placed(world: &World, entity: Entity) -> (Affine, f32) {
        let mut affine = Affine::IDENTITY;
        let mut opacity = 1.0;
        let mut current = Some(entity);
        while let Some(node) = current {
            if let Some(transform) = world.get::<SpatialTransform>(node) {
                affine = transform.to_affine_2d() * affine;
            }
            opacity *= world.get::<Opacity>(node).map_or(1.0, |value| value.0);
            if world.get::<Visibility>(node) == Some(&Visibility::Hidden) {
                opacity = 0.0;
            }
            current = world.get::<ChildOf>(node).map(ChildOf::parent);
        }
        (affine, opacity)
    }

    /// The scene boxes of the paths drawn at full opacity.
    fn shown_boxes(world: &mut World) -> Vec<Rect> {
        let paths: Vec<(Entity, Path2D)> = world
            .query::<(Entity, &Path2D)>()
            .iter(world)
            .map(|(entity, path)| (entity, path.clone()))
            .collect();
        paths
            .into_iter()
            .filter_map(|(entity, path)| {
                let (affine, opacity) = placed(world, entity);
                (opacity > 0.999).then(|| (affine * path.0.as_ref().clone()).bounding_box())
            })
            .collect()
    }

    fn has_circle(boxes: &[Rect], center: DVec2, radius: f64) -> bool {
        boxes.iter().any(|shown| {
            (shown.x0 - (center.x - radius)).abs() < 1e-4
                && (shown.y0 - (center.y - radius)).abs() < 1e-4
                && (shown.x1 - (center.x + radius)).abs() < 1e-4
                && (shown.y1 - (center.y + radius)).abs() < 1e-4
        })
    }

    /// Where the layout puts each node of `graph` now, by name.
    fn places(graph: &GraphHandle) -> Vec<(String, DVec2, f64)> {
        let parts = graph.current.lock().unwrap();
        let edges: Vec<(String, String)> = parts
            .edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone()))
            .collect();
        let places = graph_places(&parts.nodes, &edges, graph.region, &graph.options);
        parts
            .nodes
            .iter()
            .zip(places)
            .map(|(node, place)| (node.name.clone(), place, node.radius))
            .collect()
    }

    fn plain(layout: GraphLayout) -> GraphOptions {
        GraphOptions {
            layout,
            labels: false,
            ..GraphOptions::default()
        }
    }

    #[test]
    fn a_graph_draws_every_node_where_its_layout_puts_it() {
        let mut scene = SceneModel::new(640, 360);
        let graph = scene
            .graph(
                &names(&["a", "b", "c", "d"]),
                &pairs(&[("a", "b"), ("a", "c"), ("c", "d")]),
                region(),
                plain(GraphLayout::Tree),
            )
            .unwrap();
        scene.wait(0.5);
        assert_eq!(graph.nodes(), ["a", "b", "c", "d"]);
        assert_eq!(graph.edges(), pairs(&[("a", "b"), ("a", "c"), ("c", "d")]));
        assert!(graph.node("d").is_some() && graph.node("e").is_none());
        assert!(graph.edge("c", "d").is_some() && graph.edge("d", "c").is_none());
        let places = places(&graph);
        // A tree hangs its children below their parent.
        let at = |name: &str| places.iter().find(|place| place.0 == name).unwrap().1;
        assert!(at("b").y < at("a").y - 0.5 && at("d").y < at("c").y - 0.5);
        assert!((at("b").y - at("c").y).abs() < 1e-9);

        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.25);
        let boxes = shown_boxes(&mut world);
        for (name, place, radius) in &places {
            assert!(
                has_circle(&boxes, *place, *radius),
                "{name} at {place} (r {radius}) not in {boxes:?}"
            );
            let inside = place.x - radius >= region().x0 - 1e-9
                && place.x + radius <= region().x1 + 1e-9
                && place.y - radius >= region().y0 - 1e-9
                && place.y + radius <= region().y1 + 1e-9;
            assert!(inside, "{name} at {place} leaves the region");
        }
    }

    #[test]
    fn edges_leave_their_nodes_at_their_rims_as_the_nodes_move() {
        let mut scene = SceneModel::new(640, 360);
        let graph = scene
            .graph(
                &names(&["a", "b"]),
                &pairs(&[("a", "b")]),
                region(),
                plain(GraphLayout::Layered),
            )
            .unwrap();
        scene.wait(0.2);
        let b = graph.node("b").unwrap();
        scene.play(vec![b.animate().shift_by(2.0, 0.5).duration(1.0)]);
        scene.wait(0.2);
        let places = places(&graph);
        let (centers, radius) = ([places[0].1, places[1].1], places[0].2);
        let gap = GraphOptions::default().node_radius * 0.15;
        let (mut world, mut timeline) = compiled(&scene);
        for (time, shift) in [(0.1, DVec2::ZERO), (1.3, DVec2::new(2.0, 0.5))] {
            timeline.seek(&mut world, time);
            tracking_line_system(&mut world);
            let (entity, _) = world
                .query::<(Entity, &TrackingConnector)>()
                .iter(&world)
                .next()
                .unwrap();
            let path = world.get::<Path2D>(entity).unwrap().0.as_ref().clone();
            let (affine, _) = placed(&world, entity);
            let (a, b) = (centers[0], centers[1] + shift);
            let along = (b - a).normalize();
            let reach: Vec<f64> = (affine * path)
                .elements()
                .iter()
                .filter_map(|element| match element {
                    PathEl::MoveTo(point) | PathEl::LineTo(point) => Some(*point),
                    _ => None,
                })
                .map(|point| (DVec2::new(point.x, point.y) - a).dot(along))
                .collect();
            let first = reach.iter().copied().fold(f64::INFINITY, f64::min);
            let last = reach.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            assert!((first - (radius + gap)).abs() < 1e-6, "{time}: starts {first}");
            let end = (b - a).length() - (radius + gap);
            assert!((last - end).abs() < 1e-6, "{time}: ends {last}, not {end}");
        }
    }

    #[test]
    fn a_new_node_lands_where_the_moved_graph_is() {
        let mut scene = SceneModel::new(640, 360);
        let graph = scene
            .graph(
                &names(&["a", "b"]),
                &pairs(&[("a", "b")]),
                region(),
                plain(GraphLayout::Tree),
            )
            .unwrap();
        scene.wait(0.2);
        scene.play(vec![graph.group.animate().shift_by(1.5, 0.0).duration(0.5)]);
        let change = graph
            .add_node(&mut scene, "c", &pairs(&[("a", "c")]), Some(0.5))
            .unwrap();
        scene.play_composition_configured(change, None, None).unwrap();
        scene.wait(0.3);
        assert_eq!(graph.nodes(), ["a", "b", "c"]);
        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 1.45);
        let boxes = shown_boxes(&mut world);
        for (name, place, radius) in places(&graph) {
            let moved = place + DVec2::new(1.5, 0.0);
            assert!(has_circle(&boxes, moved, radius), "{name} at {moved}");
        }
    }

    #[test]
    fn a_change_keeps_named_nodes_and_fades_the_rest() {
        let mut scene = SceneModel::new(640, 360);
        let graph = scene
            .graph(
                &names(&["a", "b", "c"]),
                &pairs(&[("a", "b"), ("b", "c")]),
                region(),
                plain(GraphLayout::Layered),
            )
            .unwrap();
        scene.wait(0.5);
        let before = places(&graph);
        let change = graph
            .to(
                &mut scene,
                &names(&["a", "b", "d", "e"]),
                &pairs(&[("a", "b"), ("a", "d"), ("a", "e")]),
                Some(1.0),
            )
            .unwrap();
        // The new structure holds at once, before the change plays.
        assert_eq!(graph.nodes(), ["a", "b", "d", "e"]);
        assert_eq!(graph.edges(), pairs(&[("a", "b"), ("a", "d"), ("a", "e")]));
        scene.play_composition_configured(change, None, None).unwrap();
        scene.wait(0.5);
        let after = places(&graph);
        let place = |places: &[(String, DVec2, f64)], name: &str| {
            places.iter().find(|place| place.0 == name).cloned().unwrap()
        };

        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.25);
        let boxes = shown_boxes(&mut world);
        let (_, c, radius) = place(&before, "c");
        assert!(has_circle(&boxes, c, radius));
        let (_, e, radius) = place(&after, "e");
        assert!(!has_circle(&boxes, e, radius), "e shows before its change");
        timeline.seek(&mut world, 1.9);
        let boxes = shown_boxes(&mut world);
        for (name, place, radius) in &after {
            assert!(has_circle(&boxes, *place, *radius), "{name} at {place}");
        }
        let (_, c, radius) = place(&before, "c");
        assert!(!has_circle(&boxes, c, radius), "c is still drawn");
    }

    #[test]
    fn invalid_structures_are_rejected_and_change_nothing() {
        let mut scene = SceneModel::new(640, 360);
        let options = GraphOptions::default;
        let mut graph = |nodes: &[&str], edges: &[(&str, &str)], options: GraphOptions| {
            scene
                .graph(&names(nodes), &pairs(edges), region(), options)
                .err()
        };
        assert_eq!(graph(&[], &[], options()), Some(GraphError::Empty));
        assert_eq!(
            graph(&["a", "a"], &[], options()),
            Some(GraphError::DuplicateNode("a".into()))
        );
        assert_eq!(
            graph(&["a"], &[("a", "b")], options()),
            Some(GraphError::UnknownNode("b".into()))
        );
        assert_eq!(
            graph(&["a"], &[("a", "a")], options()),
            Some(GraphError::SelfLoop("a".into()))
        );
        let flat = GraphOptions {
            node_radius: 0.0,
            ..options()
        };
        assert_eq!(graph(&["a"], &[], flat), Some(GraphError::InvalidRadius));

        let graph = scene
            .graph(&names(&["a", "b"]), &pairs(&[("a", "b")]), region(), options())
            .unwrap();
        assert!(matches!(
            graph.add_node(&mut scene, "a", &[], None),
            Err(GraphError::DuplicateNode(_))
        ));
        assert!(matches!(
            graph.add_node(&mut scene, "c", &pairs(&[("c", "z")]), None),
            Err(GraphError::UnknownNode(_))
        ));
        assert!(matches!(
            graph.remove_node(&mut scene, "z", None),
            Err(GraphError::UnknownNode(_))
        ));
        assert!(matches!(
            graph.remove_edge(&mut scene, "b", "a", None),
            Err(GraphError::UnknownEdge(..))
        ));
        assert_eq!(graph.nodes(), ["a", "b"]);
        assert_eq!(graph.edges(), pairs(&[("a", "b")]));
    }
}
