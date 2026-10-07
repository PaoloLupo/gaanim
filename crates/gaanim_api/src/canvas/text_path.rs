//! Text on a path (TX-06): the glyphs of a Text placed along a path by arc
//! length and turned to its tangent, with an animatable `path_offset`.
//!
//! Each glyph keeps its own shape; only its transform changes. The glyph's
//! horizontal center on the text's first baseline is laid on the path at the
//! arc length it had along the line of text, so spacing, kerning and the
//! text's own tracking carry over. The placement is a pure function of the
//! offset, so a seek shows exactly what playback does.

use std::sync::Arc;

use bevy::prelude::{Entity, World};
use gaanim_core::ObjectId;
use gaanim_core::kurbo::{self, Affine, Shape};
use gaanim_math::{Bounds3D, PathArcLength, SpatialTransform};
use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};

use crate::anim::{AnimationBuilder, AnimationType};
use crate::builder::SceneBuilder;

use super::DrawableHandle;

/// Where the text sits along its path when the offset is 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextPathAlign {
    /// The text starts at the start of the path.
    #[default]
    Start,
    /// The text is centered on the middle of the path.
    Center,
    /// The text ends at the end of the path.
    End,
}

impl TextPathAlign {
    pub const NAMES: &'static str = "start, center, or end";

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "start" => Self::Start,
            "center" => Self::Center,
            "end" => Self::End,
            _ => return None,
        })
    }
}

/// How a Text follows a path, recorded on the Text's spec.
#[derive(Debug, Clone, PartialEq)]
pub struct TextPathSpec {
    /// The drawable whose path the text follows.
    pub path: ObjectId,
    pub align: TextPathAlign,
    /// Turn each glyph to the path's direction; otherwise glyphs stay upright.
    pub orient: bool,
    /// Follow the path from its end to its start.
    pub reverse: bool,
    /// Shift along the path, as a fraction of its length.
    pub offset: f64,
}

/// A glyph of a Text on its path, in the Text's local space.
#[derive(Debug, Clone, PartialEq)]
struct GlyphOnPath {
    id: ObjectId,
    /// Arc length from the text's start to the glyph's center.
    along: f64,
    /// The glyph's center on the baseline, which sits on the path.
    anchor: kurbo::Point,
    /// The glyph's transform as laid out in a straight line.
    rest: Affine,
}

/// The glyphs of a Text and the path they follow, in the Text's local space.
#[derive(Debug, Clone, PartialEq)]
pub struct TextPathLayout {
    arc: PathArcLength,
    align: TextPathAlign,
    orient: bool,
    text_length: f64,
    glyphs: Vec<GlyphOnPath>,
}

impl TextPathLayout {
    /// Lay out glyphs given by their id, box and transform along a straight
    /// line on `baseline`, in reading order, over `path`.
    fn new(
        path: &kurbo::BezPath,
        spec: &TextPathSpec,
        baseline: f64,
        glyphs: &[(ObjectId, kurbo::Rect, Affine)],
    ) -> Option<Self> {
        let arc = PathArcLength::new(path);
        if arc.length() <= 0.0 || glyphs.is_empty() {
            return None;
        }
        let start = glyphs
            .iter()
            .map(|(_, rect, _)| rect.x0)
            .fold(f64::INFINITY, f64::min);
        let end = glyphs
            .iter()
            .map(|(_, rect, _)| rect.x1)
            .fold(f64::NEG_INFINITY, f64::max);
        Some(Self {
            arc,
            align: spec.align,
            orient: spec.orient,
            text_length: (end - start).max(0.0),
            glyphs: glyphs
                .iter()
                .map(|(id, rect, rest)| GlyphOnPath {
                    id: *id,
                    along: rect.center().x - start,
                    anchor: kurbo::Point::new(rect.center().x, baseline),
                    rest: *rest,
                })
                .collect(),
        })
    }

    /// Arc length of the text's start at `offset`.
    fn start(&self, offset: f64) -> f64 {
        let length = self.arc.length();
        let aligned = match self.align {
            TextPathAlign::Start => 0.0,
            TextPathAlign::Center => (length - self.text_length) / 2.0,
            TextPathAlign::End => length - self.text_length,
        };
        aligned + offset * length
    }

    /// The transform of glyph `index` with the text shifted by `offset`.
    pub fn transform(&self, index: usize, offset: f64) -> SpatialTransform {
        let glyph = &self.glyphs[index];
        let Some((point, angle)) = self.arc.sample(self.start(offset) + glyph.along) else {
            return SpatialTransform::from_affine_2d(&glyph.rest);
        };
        let rotation = if self.orient { angle } else { 0.0 };
        let placed = Affine::translate(point.to_vec2())
            * Affine::rotate(rotation)
            * Affine::translate(-glyph.anchor.to_vec2())
            * glyph.rest;
        SpatialTransform::from_affine_2d(&placed)
    }
}

/// Moves one glyph of a Text along its path between two offsets.
#[derive(Debug, Clone)]
pub struct TextPathLens {
    layout: Arc<TextPathLayout>,
    glyph: usize,
    from: f64,
    to: f64,
}

impl gaanim_animation::AnimatableLens for TextPathLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let offset = self.from + (self.to - self.from) * t;
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            *transform = self.layout.transform(self.glyph, offset);
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "TextPathOffset"
    }

    fn history_free(&self) -> bool {
        true
    }
}

fn check_offset(offset: f64) -> Result<(), String> {
    if offset.is_finite() {
        Ok(())
    } else {
        Err(format!("path_offset must be finite, got {offset}"))
    }
}

impl super::SceneModel {
    /// Create a Text from `spec` whose glyphs follow the path of `path`.
    ///
    /// The path is read in scene coordinates where `path` stands when the
    /// Text is created, and then belongs to the Text: moving the Text moves
    /// the path with it, and later changes to `path` do not move the glyphs.
    /// Glyphs sit on the left of the direction of travel (outside a
    /// counterclockwise circle when `reverse`), with the text's first
    /// baseline on the path.
    pub fn text_on_path(
        &mut self,
        spec: gaanim_text::prelude::TextSpec,
        path: &DrawableHandle,
        align: TextPathAlign,
        orient: bool,
        reverse: bool,
        offset: f64,
    ) -> Result<DrawableHandle, String> {
        if !std::sync::Arc::ptr_eq(&path.state, &self.state) {
            return Err("the path of text.on_path must belong to the same scene".to_string());
        }
        check_offset(offset)?;
        if matches!(
            path.spec.lock().expect("object spec poisoned").kind,
            super::SpawnKind::Group(_) | super::SpawnKind::GroupNoCenter(_)
        ) {
            return Err(
                "the path of text.on_path must be a shape with a path, not a group".to_string(),
            );
        }
        if spec
            .rendered_text()
            .chars()
            .all(|character| character.is_whitespace())
        {
            return Err("text.on_path needs visible characters".to_string());
        }
        let handle = self.text_spec(spec);
        handle.spec.lock().expect("object spec poisoned").text_path = Some(TextPathSpec {
            path: path.id,
            align,
            orient,
            reverse,
            offset,
        });
        Ok(handle)
    }
}

impl DrawableHandle {
    /// Shift a Text made by [`super::SceneModel::text_on_path`] along its
    /// path, as a fraction of the path's length. Declared before the first
    /// `play`, it is the initial offset; later it cuts at the cursor.
    pub fn path_offset(self, offset: f64) -> Result<Self, String> {
        check_offset(offset)?;
        {
            let mut spec = self.spec.lock().expect("object spec poisoned");
            let text_path = spec
                .text_path
                .as_mut()
                .ok_or("path_offset() requires a Text created with text.on_path")?;
            text_path.offset = offset;
        }
        self.push_immediate(self.id, AnimationType::TextPathOffset { to: offset });
        Ok(self)
    }
}

impl super::Anim {
    /// Move a Text made by [`super::SceneModel::text_on_path`] along its path
    /// to `offset`, a fraction of the path's length: on a closed path 1.0 is
    /// one full turn.
    pub fn path_offset(mut self, offset: f64) -> Result<Self, String> {
        check_offset(offset)?;
        let spec = self
            .property_spec
            .as_ref()
            .ok_or("path_offset() requires Text.animate")?;
        if spec
            .lock()
            .expect("object spec poisoned")
            .text_path
            .is_none()
        {
            return Err("path_offset() requires a Text created with text.on_path".to_string());
        }
        if !self.inner.anim_type.is_empty_properties() {
            return Err(
                "path_offset() cannot be combined with property targets or another effect in one Anim"
                    .to_string(),
            );
        }
        self.inner.anim_type = AnimationType::TextPathOffset { to: offset };
        Ok(self)
    }
}

impl SceneBuilder<'_, '_, '_> {
    /// Lay the glyphs of the Text `root` along the path of `path`, read in
    /// scene coordinates where it is now and kept in the Text's own space,
    /// so moving the Text later moves it with its path.
    pub(crate) fn attach_text_path(&mut self, root: ObjectId, path: ObjectId, spec: &TextPathSpec) {
        let Some(source) = self.states.get(path) else {
            gaanim_core::console::warn(
                "text",
                "on_path: the path drawable is not in the scene; the text stays straight",
            );
            return;
        };
        let mut route = (*source.path).clone();
        route.apply_affine(self.get_world_transform(path).to_affine_2d());
        if spec.reverse {
            route = route.reverse_subpaths();
        }
        let Some(state) = self.states.get(root) else {
            return;
        };
        let glyphs: Vec<(ObjectId, kurbo::Rect, Affine)> = state
            .child_spans
            .iter()
            .filter_map(|child| {
                let glyph = self.states.get(child.id)?;
                if glyph.path.elements().is_empty() {
                    return None;
                }
                let rest = glyph.transform.to_affine_2d();
                Some((
                    child.id,
                    rest.transform_rect_bbox(glyph.path.bounding_box()),
                    rest,
                ))
            })
            .collect();
        let baseline = self
            .text_metrics
            .get(&root)
            .map(|metrics| metrics.first_baseline)
            .unwrap_or_else(|| {
                glyphs
                    .iter()
                    .map(|(_, rect, _)| rect.y0)
                    .fold(f64::INFINITY, f64::min)
            });
        let Some(layout) = TextPathLayout::new(&route, spec, baseline, &glyphs) else {
            gaanim_core::console::warn(
                "text",
                "on_path: the path has no length; the text stays straight",
            );
            return;
        };
        let mut bounds: Option<Bounds3D> = None;
        for index in 0..layout.glyphs.len() {
            let transform = layout.transform(index, spec.offset);
            let Some(glyph) = self.states.get_mut(layout.glyphs[index].id) else {
                continue;
            };
            glyph.transform = transform;
            let placed = gaanim_layout::transform_bounds(glyph.bounds, &transform);
            bounds = Some(bounds.map_or(placed, |bounds| bounds.union(&placed)));
            self.commands.entity(glyph.entity).insert(transform);
        }
        if let Some(bounds) = bounds
            && let Some(state) = self.states.get_mut(root)
        {
            state.bounds = bounds;
            // Scale and rotation turn the text around the middle of its path
            // (a ring around its circle's center), not around the scene
            // origin its glyphs were laid out from.
            let center = route.bounding_box().center();
            state.transform.anchor = gaanim_core::glam::DVec3::new(center.x, center.y, 0.0);
            self.commands
                .entity(state.entity)
                .insert((gaanim_scene::LocalBounds(bounds), state.transform));
        }
        self.text_paths
            .insert(root, (Arc::new(layout), spec.offset));
    }

    /// Schedule a `path_offset` change of a Text laid on a path.
    pub(crate) fn play_text_path_offset_internal(&mut self, anim: AnimationBuilder) {
        let AnimationType::TextPathOffset { to } = anim.anim_type else {
            return;
        };
        let Some((layout, from)) = self.text_paths.get(&anim.target).cloned() else {
            gaanim_core::console::warn(
                "text",
                "path_offset needs a Text created with text.on_path",
            );
            return;
        };
        self.text_paths.insert(anim.target, (layout.clone(), to));
        let track = self.ensure_track(anim.target);
        let start = self.current_time + anim.delay;
        for (index, glyph) in layout.glyphs.iter().enumerate() {
            let Some(state) = self.states.get_mut(glyph.id) else {
                continue;
            };
            state.transform = layout.transform(index, to);
            self.timeline.add_clip(
                track,
                start,
                anim.duration,
                ClipPayload::Animation(AnimationSpec {
                    target: glyph.id,
                    lens: PropertyLensSpec::Dynamic(gaanim_animation::tween::DynamicLens(
                        Arc::new(TextPathLens {
                            layout: layout.clone(),
                            glyph: index,
                            from,
                            to,
                        }),
                    )),
                    rate_func: anim.rate_func.clone(),
                    delay: 0.0,
                    label: self.current_label.clone(),
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyphs() -> Vec<(ObjectId, kurbo::Rect, Affine)> {
        (0..3)
            .map(|index| {
                let x = index as f64;
                (
                    ObjectId::from_parts(index, 1),
                    kurbo::Rect::new(x, 0.0, x + 0.8, 1.0),
                    Affine::IDENTITY,
                )
            })
            .collect()
    }

    fn spec(align: TextPathAlign, orient: bool) -> TextPathSpec {
        TextPathSpec {
            path: ObjectId::from_parts(9, 1),
            align,
            orient,
            reverse: false,
            offset: 0.0,
        }
    }

    fn anchor_at(layout: &TextPathLayout, index: usize, offset: f64) -> kurbo::Point {
        let anchor = layout.glyphs[index].anchor;
        layout.transform(index, offset).to_affine_2d() * anchor
    }

    #[test]
    fn glyphs_keep_their_spacing_along_a_line() {
        let mut path = kurbo::BezPath::new();
        path.move_to((0.0, 5.0));
        path.line_to((10.0, 5.0));
        let layout =
            TextPathLayout::new(&path, &spec(TextPathAlign::Start, true), 0.2, &glyphs()).unwrap();
        for index in 0..3 {
            let point = anchor_at(&layout, index, 0.0);
            assert!(
                (point - kurbo::Point::new(index as f64 + 0.4, 5.0)).hypot() < 1e-6,
                "{point:?}"
            );
        }
        // An offset of 0.5 moves every glyph half the path further.
        let shifted = anchor_at(&layout, 0, 0.5);
        assert!((shifted - kurbo::Point::new(5.4, 5.0)).hypot() < 1e-6);

        let centered =
            TextPathLayout::new(&path, &spec(TextPathAlign::Center, true), 0.2, &glyphs()).unwrap();
        assert!((anchor_at(&centered, 1, 0.0) - kurbo::Point::new(5.0, 5.0)).hypot() < 1e-6);
        let ended =
            TextPathLayout::new(&path, &spec(TextPathAlign::End, true), 0.2, &glyphs()).unwrap();
        assert!((anchor_at(&ended, 2, 0.0) - kurbo::Point::new(9.6, 5.0)).hypot() < 1e-6);
    }

    #[test]
    fn oriented_glyphs_turn_with_the_path_and_upright_ones_do_not() {
        let mut path = kurbo::BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((0.0, 10.0));
        let oriented =
            TextPathLayout::new(&path, &spec(TextPathAlign::Start, true), 0.0, &glyphs()).unwrap();
        let angle = 2.0
            * oriented
                .transform(0, 0.0)
                .rotation
                .z
                .atan2(oriented.transform(0, 0.0).rotation.w);
        assert!((angle - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((anchor_at(&oriented, 1, 0.0) - kurbo::Point::new(0.0, 1.4)).hypot() < 1e-6);
        let upright =
            TextPathLayout::new(&path, &spec(TextPathAlign::Start, false), 0.0, &glyphs()).unwrap();
        assert!(upright.transform(0, 0.0).rotation.z.abs() < 1e-12);
    }

    #[test]
    fn a_closed_path_wraps_a_whole_turn_back_to_the_start() {
        let circle = kurbo::Circle::new((0.0, 0.0), 3.0).to_path(1e-9);
        let layout =
            TextPathLayout::new(&circle, &spec(TextPathAlign::Start, true), 0.0, &glyphs())
                .unwrap();
        for index in 0..3 {
            let start = anchor_at(&layout, index, 0.0);
            assert!((start.to_vec2().hypot() - 3.0).abs() < 1e-3);
            assert!((anchor_at(&layout, index, 1.0) - start).hypot() < 1e-6);
        }
        assert!(
            TextPathLayout::new(
                &kurbo::BezPath::new(),
                &spec(TextPathAlign::Start, true),
                0.0,
                &glyphs()
            )
            .is_none()
        );
    }
}
