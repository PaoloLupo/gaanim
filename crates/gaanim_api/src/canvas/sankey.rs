//! Sankey diagrams: `scene.viz.sankey` (layout in [`gaanim_visualization::sankey`]).
//!
//! Every node is an ordinary bar named by its name, every flow a ribbon
//! named `source→target`, and every label `label:` and its node's name. A
//! change of data builds the diagram again and magic-moves into it: parts
//! that keep their name move and reshape, the others fade. Nodes keep their
//! colors by name across changes, and each ribbon takes its source's color.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gaanim_core::kurbo::Rect;
use gaanim_core::peniko::Color;
use gaanim_text::prelude::{TextFlow, TextRole, TextSpec, TextStyle};
use gaanim_visualization::{SankeyError, SankeyLayout, SankeyOptions};

use super::ops::Op;
use super::{Anchor, Anim, DrawableHandle, MagicMoveKey, MagicMoveUnmatched, SceneModel};

/// How a sankey diagram draws its nodes and flows.
#[derive(Debug, Clone, PartialEq)]
pub struct SankeyChartOptions {
    pub layout: SankeyOptions,
    /// Whether nodes show their names.
    pub labels: bool,
    /// Node colors, in order of appearance; the theme's categorical palette
    /// without them.
    pub colors: Option<Vec<Color>>,
    /// Opacity of the ribbons.
    pub flow_opacity: f32,
}

impl Default for SankeyChartOptions {
    fn default() -> Self {
        Self {
            layout: SankeyOptions::default(),
            labels: true,
            colors: None,
            flow_opacity: 0.45,
        }
    }
}

/// One node of a drawn sankey diagram.
#[derive(Clone)]
pub struct SankeyNodePart {
    pub name: String,
    pub value: f64,
    /// Its bar.
    pub bar: DrawableHandle,
    /// Its name, when shown.
    pub label: Option<DrawableHandle>,
}

/// One flow of a drawn sankey diagram.
#[derive(Clone)]
pub struct SankeyFlowPart {
    pub source: String,
    pub target: String,
    pub value: f64,
    /// Its ribbon.
    pub ribbon: DrawableHandle,
}

struct SankeyParts {
    content: DrawableHandle,
    nodes: Vec<SankeyNodePart>,
    flows: Vec<SankeyFlowPart>,
}

/// A sankey diagram: its region, its parts and how to draw them again.
#[derive(Clone)]
pub struct SankeyHandle {
    /// The whole diagram.
    pub group: DrawableHandle,
    /// The group the parts of each version sit in.
    slot: DrawableHandle,
    region: Rect,
    options: Arc<SankeyChartOptions>,
    current: Arc<Mutex<SankeyParts>>,
    /// The color index of each node, by name.
    colors: Arc<Mutex<HashMap<String, usize>>>,
}

/// The name a flow's ribbon answers to.
pub fn sankey_flow_key(source: &str, target: &str) -> String {
    format!("{source}→{target}")
}

impl SceneModel {
    /// A sankey diagram of `flows`, each `(source, target, value)`, filling
    /// `region` with its bars; labels sit beside them, inside the region.
    pub fn sankey(
        &mut self,
        flows: &[(String, String, f64)],
        region: Rect,
        options: SankeyChartOptions,
    ) -> Result<SankeyHandle, SankeyError> {
        let colors = Arc::new(Mutex::new(HashMap::new()));
        let parts = self.sankey_parts(flows, region, &options, &colors)?;
        let slot = self.group_no_center(&[&parts.content]);
        let group = self.group(&[&slot]);
        Ok(SankeyHandle {
            group,
            slot,
            region,
            options: Arc::new(options),
            current: Arc::new(Mutex::new(parts)),
            colors,
        })
    }

    fn sankey_parts(
        &mut self,
        flows: &[(String, String, f64)],
        region: Rect,
        options: &SankeyChartOptions,
        colors: &Mutex<HashMap<String, usize>>,
    ) -> Result<SankeyParts, SankeyError> {
        let layout: SankeyLayout = gaanim_visualization::sankey(flows, region, options.layout)?;
        let palette = match &options.colors {
            Some(colors) if !colors.is_empty() => {
                gaanim_core::Palette::new(colors.clone()).unwrap_or_default()
            }
            _ => self.categorical_palette(),
        };
        let mut indices = colors.lock().expect("sankey colors poisoned");
        for node in &layout.nodes {
            let next = indices.len();
            indices.entry(node.name.clone()).or_insert(next);
        }
        let node_colors: Vec<Color> = layout
            .nodes
            .iter()
            .map(|node| palette.color(indices[&node.name]))
            .collect();
        drop(indices);

        // Ribbons first, so the bars draw over their ends.
        let mut parts_flows = Vec::with_capacity(layout.links.len());
        for (link, (source, target, value)) in layout.links.iter().zip(flows) {
            let path = gaanim_visualization::ribbon(&layout, link);
            let bounds = {
                let rect = gaanim_core::kurbo::Shape::bounding_box(&path);
                gaanim_math::Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1)
            };
            let color = node_colors[link.source];
            let ribbon = self
                .visualization_path(path, bounds, color, 0.0, "SankeyFlow")
                .fill(color.multiply_alpha(options.flow_opacity.clamp(0.0, 1.0)))
                .no_stroke()
                .named(sankey_flow_key(source, target));
            parts_flows.push(SankeyFlowPart {
                source: source.clone(),
                target: target.clone(),
                value: *value,
                ribbon,
            });
        }

        let style = {
            let body = self.themed_text_config().roles[&TextRole::Body].size;
            TextStyle {
                size: Some(body * 0.8),
                ..TextStyle::default()
            }
        };
        let label_color = self.theme_color("foreground").unwrap_or(Color::WHITE);
        let gap = options.layout.node_width.max(0.08);
        let mut parts_nodes = Vec::with_capacity(layout.nodes.len());
        let last_layer = layout
            .nodes
            .iter()
            .map(|node| node.layer)
            .max()
            .unwrap_or(0);
        for (node, color) in layout.nodes.iter().zip(&node_colors) {
            let rect = node.rect;
            let bar = self
                .rect(rect.width().max(1e-6), rect.height().max(1e-6))
                .move_to(rect.center().x, rect.center().y)
                .fill(*color)
                .no_stroke()
                .named(&node.name);
            parts_nodes.push(SankeyNodePart {
                name: node.name.clone(),
                value: node.value,
                bar,
                label: None,
            });
        }
        // Names after every bar, so they draw over all of them and the
        // ribbons, also while those move. They face inward: right of the
        // bars in the left half, left of them in the right half.
        if options.labels {
            for (part, node) in parts_nodes.iter_mut().zip(&layout.nodes) {
                let Ok(spec) = TextSpec::new(
                    vec![node.name.as_str().into()],
                    None,
                    TextStyle {
                        color: Some(label_color),
                        ..style.clone()
                    },
                    TextFlow::default(),
                ) else {
                    continue;
                };
                let label = self.text_spec(spec);
                let (rect, y) = (node.rect, node.rect.center().y);
                let label = if last_layer > 0 && node.layer * 2 > last_layer {
                    label.at_anchor(rect.x0 - gap, y, Anchor::Right)
                } else {
                    label.at_anchor(rect.x1 + gap, y, Anchor::Left)
                };
                part.label = Some(label.named(format!("label:{}", node.name)));
            }
        }
        let members: Vec<&DrawableHandle> = parts_flows
            .iter()
            .map(|flow| &flow.ribbon)
            .chain(
                parts_nodes
                    .iter()
                    .flat_map(|node| std::iter::once(&node.bar).chain(node.label.as_ref())),
            )
            .collect();
        let content = self.group_no_center(&members);
        Ok(SankeyParts {
            content,
            nodes: parts_nodes,
            flows: parts_flows,
        })
    }
}

impl SankeyHandle {
    /// The current nodes, in the order their names first appear in the flows.
    pub fn nodes(&self) -> Vec<SankeyNodePart> {
        self.current.lock().expect("sankey poisoned").nodes.clone()
    }

    /// The current flows, in their given order.
    pub fn flows(&self) -> Vec<SankeyFlowPart> {
        self.current.lock().expect("sankey poisoned").flows.clone()
    }

    /// The bar of the node `name`.
    pub fn node(&self, name: &str) -> Option<DrawableHandle> {
        self.current
            .lock()
            .expect("sankey poisoned")
            .nodes
            .iter()
            .find(|node| node.name == name)
            .map(|node| node.bar.clone())
    }

    /// The ribbon of the first flow from `source` to `target`.
    pub fn flow(&self, source: &str, target: &str) -> Option<DrawableHandle> {
        self.current
            .lock()
            .expect("sankey poisoned")
            .flows
            .iter()
            .find(|flow| flow.source == source && flow.target == target)
            .map(|flow| flow.ribbon.clone())
    }

    /// A change of data to `flows`: parts that keep their name move and
    /// reshape, the others fade out and in. Later changes start from
    /// `flows`. `scene` is the diagram's scene.
    pub fn to_anim(
        &self,
        scene: &mut SceneModel,
        flows: &[(String, String, f64)],
        duration: Option<f64>,
    ) -> Result<Anim, SankeyError> {
        let parts = scene.sankey_parts(flows, self.region, &self.options, &self.colors)?;
        scene
            .state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachToGroupLocal {
                group: self.slot.id,
                child: parts.content.id,
            });
        let mut current = self.current.lock().expect("sankey poisoned");
        let anim = current
            .content
            .magic_move_to(
                &parts.content,
                MagicMoveKey::Name,
                MagicMoveUnmatched::Fade,
                duration,
            )
            .expect("a sankey diagram's versions are distinct drawables of one scene");
        *current = parts;
        Ok(anim)
    }
}

#[cfg(test)]
mod tests {
    use gaanim_core::kurbo::Shape;

    use super::*;
    use crate::canvas::treemap::tests::{compiled, has, shown_boxes};

    fn flows(items: &[(&str, &str, f64)]) -> Vec<(String, String, f64)> {
        items
            .iter()
            .map(|(source, target, value)| (source.to_string(), target.to_string(), *value))
            .collect()
    }

    fn plain() -> SankeyChartOptions {
        SankeyChartOptions {
            labels: false,
            ..SankeyChartOptions::default()
        }
    }

    /// The bars and ribbons a layout of `flows` draws in `region`.
    fn expected(flows: &[(String, String, f64)], region: Rect) -> Vec<Rect> {
        let layout = gaanim_visualization::sankey(flows, region, plain().layout).unwrap();
        layout
            .nodes
            .iter()
            .map(|node| node.rect)
            .chain(
                layout
                    .links
                    .iter()
                    .map(|link| gaanim_visualization::ribbon(&layout, link).bounding_box()),
            )
            .collect()
    }

    #[test]
    fn a_sankey_draws_a_bar_per_node_and_a_ribbon_per_flow() {
        let mut scene = SceneModel::new(640, 360);
        let region = Rect::new(-4.0, -2.0, 4.0, 2.0);
        let data = flows(&[("A", "B", 3.0), ("A", "C", 1.0), ("B", "D", 2.0)]);
        let sankey = scene.sankey(&data, region, plain()).unwrap();
        scene.wait(0.5);
        let names: Vec<String> = sankey.nodes().into_iter().map(|node| node.name).collect();
        assert_eq!(names, ["A", "B", "C", "D"]);
        assert_eq!(sankey.flows().len(), 3);
        assert!(sankey.flow("A", "C").is_some() && sankey.flow("C", "A").is_none());
        assert!(sankey.node("D").is_some() && sankey.node("E").is_none());

        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.25);
        let boxes = shown_boxes(&mut world);
        for rect in expected(&data, region) {
            assert!(has(&boxes, rect), "{rect:?} not in {boxes:?}");
        }
    }

    #[test]
    fn a_change_moves_kept_parts_and_fades_the_others() {
        let mut scene = SceneModel::new(640, 360);
        let region = Rect::new(-4.0, -2.0, 4.0, 2.0);
        let first = flows(&[("A", "B", 3.0), ("A", "C", 1.0), ("B", "D", 2.0)]);
        let sankey = scene.sankey(&first, region, plain()).unwrap();
        scene.wait(0.5);
        let second = flows(&[
            ("A", "B", 1.0),
            ("A", "C", 3.0),
            ("C", "E", 2.0),
            ("C", "F", 1.0),
        ]);
        let anim = sankey.to_anim(&mut scene, &second, Some(1.0)).unwrap();
        scene.play(vec![anim]);
        scene.wait(0.5);
        let names: Vec<String> = sankey.nodes().into_iter().map(|node| node.name).collect();
        assert_eq!(names, ["A", "B", "C", "E", "F"]);
        // What moves or leaves is no longer drawn where it was.
        let after = expected(&second, region);
        let gone: Vec<Rect> = expected(&first, region)
            .into_iter()
            .filter(|rect| !has(&after, *rect))
            .collect();
        assert!(!gone.is_empty());

        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.25);
        let boxes = shown_boxes(&mut world);
        assert!(
            expected(&first, region)
                .iter()
                .all(|rect| has(&boxes, *rect))
        );
        timeline.seek(&mut world, 1.9);
        let boxes = shown_boxes(&mut world);
        for rect in after {
            assert!(has(&boxes, rect), "{rect:?} not in {boxes:?}");
        }
        for rect in gone {
            assert!(!has(&boxes, rect), "{rect:?} is still drawn");
        }
    }

    #[test]
    fn nodes_keep_their_colors_and_ribbons_take_their_sources() {
        let mut scene = SceneModel::new(640, 360);
        let region = Rect::new(-4.0, -2.0, 4.0, 2.0);
        let sankey = scene
            .sankey(&flows(&[("A", "B", 3.0), ("B", "C", 2.0)]), region, plain())
            .unwrap();
        let fill = |drawable: DrawableHandle| drawable.spec.lock().unwrap().fill.clone();
        let (a, b) = (
            fill(sankey.node("A").unwrap()),
            fill(sankey.node("B").unwrap()),
        );
        assert_ne!(a, b);
        sankey
            .to_anim(
                &mut scene,
                &flows(&[("B", "C", 2.0), ("A", "B", 1.0)]),
                None,
            )
            .unwrap();
        assert_eq!(fill(sankey.node("A").unwrap()), a);
        assert_eq!(fill(sankey.node("B").unwrap()), b);
        let Some(gaanim_core::peniko::Brush::Solid(ribbon)) = fill(sankey.flow("A", "B").unwrap())
        else {
            panic!("ribbons are solid");
        };
        let Some(gaanim_core::peniko::Brush::Solid(source)) = a else {
            panic!("bars are solid");
        };
        assert_eq!(ribbon, source.multiply_alpha(0.45));
    }
}
