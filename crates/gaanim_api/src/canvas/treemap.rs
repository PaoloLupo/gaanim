//! Treemaps: `scene.viz.treemap` (layout in [`gaanim_visualization::treemap`]).
//!
//! Every cell is an ordinary rectangle named by its key, the path of names
//! from its outermost group joined with `/`, and so is its label (`label:`
//! and the key). A change of data builds the cells again and magic-moves
//! into them: cells that keep their key slide and resize, the others fade.
//! Leaves take the color of their outermost group, kept by name across
//! changes; groups are panels with their name at the top.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gaanim_core::kurbo::Rect;
use gaanim_core::peniko::Color;
use gaanim_text::prelude::{TextFlow, TextRole, TextSpec, TextStyle};
use gaanim_visualization::{TreemapCell, TreemapError, TreemapSpacing};

use super::ops::Op;
use super::{Anchor, Anim, DrawableHandle, MagicMoveKey, MagicMoveUnmatched, SceneModel};

/// Smallest label, as a fraction of the treemap's label size, before a
/// label that does not fit is left out.
const MIN_LABEL_SCALE: f64 = 0.6;

/// How a treemap draws its cells.
#[derive(Debug, Clone, PartialEq)]
pub struct TreemapOptions {
    /// Gap between sibling cells.
    pub gap: f64,
    /// Space a group keeps around its children.
    pub padding: f64,
    /// Whether cells show their names.
    pub labels: bool,
    /// Colors of the outermost groups, in order of appearance; the theme's
    /// categorical palette without them.
    pub colors: Option<Vec<Color>>,
}

impl Default for TreemapOptions {
    fn default() -> Self {
        Self {
            gap: 0.04,
            padding: 0.06,
            labels: true,
            colors: None,
        }
    }
}

/// One cell of a drawn treemap.
#[derive(Clone)]
pub struct TreemapPart {
    pub cell: TreemapCell,
    /// Its rectangle.
    pub rect: DrawableHandle,
    /// Its name, when shown.
    pub label: Option<DrawableHandle>,
}

struct TreemapParts {
    content: DrawableHandle,
    parts: Vec<TreemapPart>,
}

/// A treemap: its region, its cells and how to draw them again.
#[derive(Clone)]
pub struct TreemapHandle {
    /// The whole treemap.
    pub group: DrawableHandle,
    /// The group the cells of each version sit in.
    slot: DrawableHandle,
    region: Rect,
    options: Arc<TreemapOptions>,
    current: Arc<Mutex<TreemapParts>>,
    /// The color index of each outermost group, by name.
    colors: Arc<Mutex<HashMap<String, usize>>>,
}

impl SceneModel {
    /// A treemap of `rows`, each a path of names and a value, filling
    /// `region`.
    pub fn treemap(
        &mut self,
        rows: &[(Vec<String>, f64)],
        region: Rect,
        options: TreemapOptions,
    ) -> Result<TreemapHandle, TreemapError> {
        let colors = Arc::new(Mutex::new(HashMap::new()));
        let parts = self.treemap_parts(rows, region, &options, &colors)?;
        let slot = self.group_no_center(&[&parts.content]);
        let group = self.group(&[&slot]);
        Ok(TreemapHandle {
            group,
            slot,
            region,
            options: Arc::new(options),
            current: Arc::new(Mutex::new(parts)),
            colors,
        })
    }

    fn treemap_label_style(&self) -> TextStyle {
        let body = self.themed_text_config().roles[&TextRole::Body].size;
        TextStyle {
            size: Some(body * 0.8),
            ..TextStyle::default()
        }
    }

    fn treemap_parts(
        &mut self,
        rows: &[(Vec<String>, f64)],
        region: Rect,
        options: &TreemapOptions,
        colors: &Mutex<HashMap<String, usize>>,
    ) -> Result<TreemapParts, TreemapError> {
        let style = self.treemap_label_style();
        let label_size = style.size.unwrap_or(0.24);
        let nested = rows.iter().any(|(path, _)| path.len() > 1);
        let spacing = TreemapSpacing {
            gap: options.gap.max(0.0),
            padding: options.padding.max(0.0),
            header: if nested && options.labels {
                label_size * 1.5
            } else {
                0.0
            },
        };
        let cells = gaanim_visualization::treemap(rows, region, spacing)?;
        let palette = match &options.colors {
            Some(colors) if !colors.is_empty() => {
                gaanim_core::Palette::new(colors.clone()).unwrap_or_default()
            }
            _ => self.categorical_palette(),
        };
        // Outermost groups keep their color by name, in order of appearance.
        let mut indices = colors.lock().expect("treemap colors poisoned");
        for (path, _) in rows {
            if let Some(name) = path.first() {
                let next = indices.len();
                indices.entry(name.clone()).or_insert(next);
            }
        }
        let panel = self
            .theme_color("panel")
            .unwrap_or(Color::from_rgb8(0x2A, 0x2A, 0x2A));
        let foreground = self.theme_color("foreground").unwrap_or(Color::WHITE);
        let background = self.theme_color("background").unwrap_or(Color::BLACK);
        let readable = |fill: Color| {
            if gaanim_text::code::contrast(foreground, fill)
                >= gaanim_text::code::contrast(background, fill)
            {
                foreground
            } else {
                background
            }
        };

        let mut parts = Vec::with_capacity(cells.len());
        let mut fills = Vec::with_capacity(cells.len());
        for cell in cells {
            let fill = if cell.leaf {
                palette.color(indices[&cell.path[0]])
            } else {
                panel
            };
            let rect = self
                .rect(cell.rect.width(), cell.rect.height())
                .move_to(cell.rect.center().x, cell.rect.center().y)
                .fill(fill)
                .no_stroke()
                .named(cell.key());
            fills.push(fill);
            parts.push(TreemapPart {
                cell,
                rect,
                label: None,
            });
        }
        // Names after every cell, so they draw over all of them, also when a
        // cell slides past another's name in a change.
        if options.labels {
            for (part, fill) in parts.iter_mut().zip(fills) {
                let name = part.cell.path.last().cloned().unwrap_or_default();
                part.label = self
                    .treemap_label(&name, &part.cell, &style, readable(fill), spacing)
                    .map(|label| label.named(format!("label:{}", part.cell.key())));
            }
        }
        let members: Vec<&DrawableHandle> = parts
            .iter()
            .flat_map(|part| std::iter::once(&part.rect).chain(part.label.as_ref()))
            .collect();
        let content = self.group_no_center(&members);
        Ok(TreemapParts { content, parts })
    }

    /// The name of `cell`: centered on a leaf, at the top left of a group,
    /// smaller when it would not fit and left out when even that does not.
    fn treemap_label(
        &mut self,
        name: &str,
        cell: &TreemapCell,
        style: &TextStyle,
        color: Color,
        spacing: TreemapSpacing,
    ) -> Option<DrawableHandle> {
        if name.trim().is_empty() {
            return None;
        }
        let size = style.size.unwrap_or(0.24);
        let margin = (spacing.padding + spacing.gap).max(size * 0.25);
        let room_width = cell.rect.width() - 2.0 * margin;
        let room_height = if cell.leaf {
            cell.rect.height() - 2.0 * margin
        } else {
            spacing.header.max(size * 1.2)
        };
        let spec = |size: f64| {
            TextSpec::new(
                vec![name.into()],
                None,
                TextStyle {
                    size: Some(size),
                    color: Some(color),
                    ..style.clone()
                },
                TextFlow::default(),
            )
            .ok()
        };
        let (width, height) = self.measure_text_spec(&spec(size)?).ok()?;
        let scale = (room_width / width).min(room_height / height).min(1.0);
        if scale.is_nan() || scale < MIN_LABEL_SCALE {
            return None;
        }
        let label = self.text_spec(spec(size * scale)?);
        Some(if cell.leaf {
            label.at_anchor(cell.rect.center().x, cell.rect.center().y, Anchor::Center)
        } else {
            label.at_anchor(
                cell.rect.x0 + margin,
                cell.rect.y1 - spacing.padding,
                Anchor::TopLeft,
            )
        })
    }
}

impl TreemapHandle {
    /// The keys of the current cells, parents before their children.
    pub fn keys(&self) -> Vec<String> {
        self.current
            .lock()
            .expect("treemap poisoned")
            .parts
            .iter()
            .map(|part| part.cell.key())
            .collect()
    }

    /// The current cells.
    pub fn parts(&self) -> Vec<TreemapPart> {
        self.current.lock().expect("treemap poisoned").parts.clone()
    }

    /// The rectangle of the cell `key`.
    pub fn cell(&self, key: &str) -> Option<DrawableHandle> {
        self.current
            .lock()
            .expect("treemap poisoned")
            .parts
            .iter()
            .find(|part| part.cell.key() == key)
            .map(|part| part.rect.clone())
    }

    /// A change of data to `rows`: cells that keep their key move and
    /// resize, the others fade out and in. Later changes start from `rows`.
    /// `scene` is the treemap's scene.
    pub fn to_anim(
        &self,
        scene: &mut SceneModel,
        rows: &[(Vec<String>, f64)],
        duration: Option<f64>,
    ) -> Result<Anim, TreemapError> {
        let parts = scene.treemap_parts(rows, self.region, &self.options, &self.colors)?;
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
        let mut current = self.current.lock().expect("treemap poisoned");
        let anim = current
            .content
            .magic_move_to(
                &parts.content,
                MagicMoveKey::Name,
                MagicMoveUnmatched::Fade,
                duration,
            )
            .expect("a treemap's versions are distinct drawables of one scene");
        *current = parts;
        Ok(anim)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use bevy::ecs::world::CommandQueue;
    use bevy::prelude::{ChildOf, Commands, Entity, World};
    use gaanim_core::kurbo::Shape;
    use gaanim_math::SpatialTransform;
    use gaanim_scene::Visibility;
    use gaanim_scene::components::{Opacity, Path2D};
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    use super::*;

    fn rows(items: &[(&str, &str, f64)]) -> Vec<(Vec<String>, f64)> {
        items
            .iter()
            .map(|(group, name, value)| (vec![group.to_string(), name.to_string()], *value))
            .collect()
    }

    fn plain() -> TreemapOptions {
        TreemapOptions {
            labels: false,
            ..TreemapOptions::default()
        }
    }

    pub(crate) fn compiled(scene: &SceneModel) -> (World, Timeline) {
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

    /// The scene boxes of the paths drawn at full opacity.
    pub(crate) fn shown_boxes(world: &mut World) -> Vec<Rect> {
        let paths: Vec<(Entity, Path2D)> = world
            .query::<(Entity, &Path2D)>()
            .iter(world)
            .map(|(entity, path)| (entity, path.clone()))
            .collect();
        paths
            .into_iter()
            .filter_map(|(entity, path)| {
                let mut affine = gaanim_core::kurbo::Affine::IDENTITY;
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
                (opacity > 0.999).then(|| (affine * path.0.as_ref().clone()).bounding_box())
            })
            .collect()
    }

    pub(crate) fn has(boxes: &[Rect], rect: Rect) -> bool {
        boxes.iter().any(|shown| {
            (shown.x0 - rect.x0).abs() < 1e-4
                && (shown.y0 - rect.y0).abs() < 1e-4
                && (shown.x1 - rect.x1).abs() < 1e-4
                && (shown.y1 - rect.y1).abs() < 1e-4
        })
    }

    fn cell_rect(handle: &TreemapHandle, key: &str) -> Rect {
        handle
            .parts()
            .into_iter()
            .find(|part| part.cell.key() == key)
            .map(|part| part.cell.rect)
            .unwrap()
    }

    #[test]
    fn a_treemap_draws_a_panel_per_group_and_a_cell_per_row() {
        let mut scene = SceneModel::new(640, 360);
        let region = Rect::new(-4.0, -2.0, 4.0, 2.0);
        let treemap = scene
            .treemap(
                &rows(&[("A", "x", 3.0), ("A", "y", 1.0), ("B", "z", 2.0)]),
                region,
                plain(),
            )
            .unwrap();
        scene.wait(0.5);
        let keys = treemap.keys();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(sorted, ["A", "A/x", "A/y", "B", "B/z"]);
        let position = |key: &str| keys.iter().position(|found| found == key).unwrap();
        assert!(position("A") < position("A/x") && position("B") < position("B/z"));
        assert!(treemap.cell("A/x").is_some() && treemap.cell("C").is_none());

        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.25);
        let boxes = shown_boxes(&mut world);
        for key in &keys {
            let rect = cell_rect(&treemap, key);
            assert!(has(&boxes, rect), "{key}: {rect:?} not in {boxes:?}");
            assert!(
                rect.x0 >= region.x0 - 1e-9
                    && rect.y0 >= region.y0 - 1e-9
                    && rect.x1 <= region.x1 + 1e-9
                    && rect.y1 <= region.y1 + 1e-9
            );
        }
    }

    #[test]
    fn a_change_moves_kept_cells_and_fades_the_others() {
        let mut scene = SceneModel::new(640, 360);
        let region = Rect::new(-4.0, -2.0, 4.0, 2.0);
        let treemap = scene
            .treemap(
                &rows(&[("A", "x", 3.0), ("A", "y", 1.0), ("B", "z", 2.0)]),
                region,
                plain(),
            )
            .unwrap();
        scene.wait(0.5);
        let gone = cell_rect(&treemap, "A/y");
        let before: Vec<Rect> = treemap.parts().iter().map(|part| part.cell.rect).collect();
        let anim = treemap
            .to_anim(
                &mut scene,
                &rows(&[("A", "x", 1.0), ("B", "z", 2.0), ("C", "w", 3.0)]),
                Some(1.0),
            )
            .unwrap();
        scene.play(vec![anim]);
        scene.wait(0.5);
        let mut keys = treemap.keys();
        keys.sort();
        assert_eq!(keys, ["A", "A/x", "B", "B/z", "C", "C/w"]);
        let arrived = cell_rect(&treemap, "C/w");

        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.25);
        let boxes = shown_boxes(&mut world);
        assert!(before.iter().all(|rect| has(&boxes, *rect)), "{boxes:?}");
        assert!(!has(&boxes, arrived));
        timeline.seek(&mut world, 1.9);
        let boxes = shown_boxes(&mut world);
        for key in treemap.keys() {
            let rect = cell_rect(&treemap, &key);
            assert!(has(&boxes, rect), "{key}: {rect:?} not in {boxes:?}");
        }
        assert!(!has(&boxes, gone));
    }

    #[test]
    fn a_change_after_the_treemap_moved_lands_inside_it() {
        let mut scene = SceneModel::new(640, 360);
        let treemap = scene
            .treemap(
                &rows(&[("A", "x", 3.0), ("B", "z", 2.0)]),
                Rect::new(-4.0, -2.0, 4.0, 2.0),
                plain(),
            )
            .unwrap();
        scene.wait(0.2);
        scene.play(vec![
            treemap.group.animate().shift_by(1.5, 0.0).duration(0.5),
        ]);
        let anim = treemap
            .to_anim(
                &mut scene,
                &rows(&[("A", "x", 1.0), ("B", "z", 2.0)]),
                Some(0.5),
            )
            .unwrap();
        scene.play(vec![anim]);
        scene.wait(0.3);
        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 1.45);
        let boxes = shown_boxes(&mut world);
        for key in treemap.keys() {
            let rect = cell_rect(&treemap, &key);
            let moved = Rect::new(rect.x0 + 1.5, rect.y0, rect.x1 + 1.5, rect.y1);
            assert!(has(&boxes, moved), "{key}: {moved:?} not in {boxes:?}");
        }
    }

    #[test]
    fn groups_keep_their_color_across_changes() {
        let mut scene = SceneModel::new(640, 360);
        let treemap = scene
            .treemap(
                &rows(&[("A", "x", 3.0), ("B", "z", 2.0)]),
                Rect::new(-4.0, -2.0, 4.0, 2.0),
                plain(),
            )
            .unwrap();
        let fill = |handle: &TreemapHandle, key: &str| {
            handle.cell(key).unwrap().spec.lock().unwrap().fill.clone()
        };
        let (a, b) = (fill(&treemap, "A/x"), fill(&treemap, "B/z"));
        assert_ne!(a, b);
        treemap
            .to_anim(
                &mut scene,
                &rows(&[("C", "w", 5.0), ("B", "z", 2.0), ("A", "x", 1.0)]),
                None,
            )
            .unwrap();
        assert_eq!(fill(&treemap, "A/x"), a);
        assert_eq!(fill(&treemap, "B/z"), b);
        assert_ne!(fill(&treemap, "C/w"), a);
    }
}
