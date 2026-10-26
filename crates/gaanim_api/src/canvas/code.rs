//! Code blocks whose source changes.
//!
//! A block keeps the panel it was made with and draws its code with a body
//! that shows one source. Editing the block gives it a body for the new
//! source: an animated edit morphs the tokens both sources share into their
//! new places and fades the others (see [`gaanim_text::code_diff`]), and an
//! immediate one swaps the bodies at the cursor. The bodies sit in a group
//! whose origin is the block's declared center, so a body made after the
//! block moved still lands where the block now is.

use std::ops::Range;
use std::sync::{Arc, Mutex};

use gaanim_core::peniko::Color;
use gaanim_text::prelude::{CodeHighlight, CodeMarkup};

use super::ops::Op;
use super::types::SpawnKind;
use super::{Anchor, Anim, DrawableHandle, SceneModel};
use crate::anim::{AnimationBuilder, AnimationType, CodeSelectionEffect, TextSelectionEffect};

/// How a code block draws its source: everything but the source itself.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeStyle {
    /// A syntax name or file extension Typst knows.
    pub language: String,
    /// Text size, in scene units.
    pub font_size: f64,
    /// Width the lines may take, in scene units.
    pub width: f64,
    /// Monospaced family; the bundled one without it.
    pub font: Option<String>,
    /// Color of plain text.
    pub foreground: Color,
    pub highlight: CodeHighlight,
    /// Where the top-left corner of the code sits, in the block's declared
    /// coordinates.
    pub top_left: (f64, f64),
}

/// Invalid input to a code block.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodeError {
    #[error("source must not be empty")]
    EmptySource,
    #[error("lines must run from 1 to {count}, the block's lines; got {first} to {last}")]
    Lines {
        first: usize,
        last: usize,
        count: usize,
    },
    #[error("{text:?} does not occur in the block")]
    Missing { text: String },
    #[error("dim must be between 0 and 1")]
    Dim,
    #[error("the duration must be a finite positive number")]
    Duration,
}

/// A code block: its panel and the body that draws its current source.
#[derive(Clone)]
pub struct CodeHandle {
    /// The whole block.
    pub group: DrawableHandle,
    /// The group the bodies sit in.
    slot: DrawableHandle,
    current: Arc<Mutex<CodeBody>>,
    style: Arc<CodeStyle>,
}

struct CodeBody {
    source: String,
    body: DrawableHandle,
}

impl SceneModel {
    /// A code block showing `source`: `frame` (its panel and decorations)
    /// with a body drawn as `style` says, grouped like `group` does.
    pub fn code_block(
        &mut self,
        frame: &[&DrawableHandle],
        source: &str,
        style: CodeStyle,
    ) -> Result<CodeHandle, CodeError> {
        if source.trim().is_empty() {
            return Err(CodeError::EmptySource);
        }
        let body = self.code_body(source, &style);
        let slot = self.group_no_center(&[&body]);
        let members: Vec<&DrawableHandle> = frame.iter().copied().chain([&slot]).collect();
        let group = self.group(&members);
        Ok(CodeHandle {
            group,
            slot,
            current: Arc::new(Mutex::new(CodeBody {
                source: source.to_owned(),
                body,
            })),
            style: Arc::new(style),
        })
    }

    fn code_body(&mut self, source: &str, style: &CodeStyle) -> DrawableHandle {
        let markup = code_markup(source, style);
        let (left, top) = style.top_left;
        self.typst_in_scene_units(&markup)
            .at_anchor(left, top, Anchor::TopLeft)
    }
}

fn code_markup(source: &str, style: &CodeStyle) -> String {
    CodeMarkup {
        source,
        language: &style.language,
        font_size: style.font_size,
        width: style.width,
        font: style.font.as_deref(),
        foreground: style.foreground,
        highlight: style.highlight.clone(),
    }
    .typst()
}

impl CodeHandle {
    /// The source the block shows after its latest edit.
    pub fn source(&self) -> String {
        self.current
            .lock()
            .expect("code block poisoned")
            .source
            .clone()
    }

    /// The language the block highlights.
    pub fn language(&self) -> &str {
        &self.style.language
    }

    /// The number of lines of the current source.
    pub fn line_count(&self) -> usize {
        gaanim_text::code_diff::line_count(&self.source())
    }

    /// Show `source` from the cursor. Before the block is first shown this
    /// is its declared source. `scene` is the block's scene.
    pub fn edit(&self, scene: &mut SceneModel, source: &str) -> Result<(), CodeError> {
        if source.trim().is_empty() {
            return Err(CodeError::EmptySource);
        }
        let mut current = self.current.lock().expect("code block poisoned");
        let frozen = self
            .group
            .state
            .lock()
            .expect("canvas state poisoned")
            .frozen_spawn_specs
            .contains_key(&current.body.id);
        if !frozen {
            let mut spec = current.body.spec.lock().expect("object spec poisoned");
            if let SpawnKind::Typst { source: markup, .. } = &mut spec.kind {
                *markup = code_markup(source, &self.style);
            }
            drop(spec);
            current.source = source.to_owned();
            return Ok(());
        }
        let body = self.next_body(scene, source);
        let anim_type = AnimationType::CodeEdit {
            target: body.id,
            from: Arc::from(current.source.as_str()),
            to: Arc::from(source),
        };
        let rate_func = anim_type.default_rate_func();
        self.group
            .state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Animate {
                anim: AnimationBuilder {
                    target: current.body.id,
                    anim_type,
                    duration: 0.0,
                    delay: 0.0,
                    rate_func,
                },
                active: true,
            });
        *current = CodeBody {
            source: source.to_owned(),
            body,
        };
        Ok(())
    }

    /// An edit to `source`: the tokens it shares with the current source
    /// slide to their new places while the others fade out and in. Later
    /// edits and selections start from `source`. Without a `duration` it
    /// lasts as long as `Scene.play` says. `scene` is the block's scene.
    pub fn edit_anim(
        &self,
        scene: &mut SceneModel,
        source: &str,
        duration: Option<f64>,
    ) -> Result<Anim, CodeError> {
        if source.trim().is_empty() {
            return Err(CodeError::EmptySource);
        }
        check_duration(duration)?;
        let mut current = self.current.lock().expect("code block poisoned");
        let body = self.next_body(scene, source);
        let anim = Self::anim(
            &current.body,
            AnimationType::CodeEdit {
                target: body.id,
                from: Arc::from(current.source.as_str()),
                to: Arc::from(source),
            },
            duration,
        );
        *current = CodeBody {
            source: source.to_owned(),
            body,
        };
        Ok(anim)
    }

    /// A body for `source` in the block's slot, hidden until an edit shows it.
    fn next_body(&self, scene: &mut SceneModel, source: &str) -> DrawableHandle {
        let body = scene.code_body(source, &self.style);
        self.group
            .state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachToGroupLocal {
                group: self.slot.id,
                child: body.id,
            });
        body
    }

    /// Byte ranges of the current source that `lines` (from 1, both ends
    /// included) and the occurrences of `text` cover; the whole source when
    /// neither is given. A word in `text` matches whole words only (see
    /// [`gaanim_text::code_diff::occurrences`]).
    pub fn ranges(
        &self,
        lines: Option<(usize, usize)>,
        text: Option<&str>,
    ) -> Result<Vec<Range<usize>>, CodeError> {
        let source = self.source();
        let mut ranges = Vec::new();
        if let Some((first, last)) = lines {
            let range = gaanim_text::code_diff::line_range(&source, first, last).ok_or(
                CodeError::Lines {
                    first,
                    last,
                    count: gaanim_text::code_diff::line_count(&source),
                },
            )?;
            ranges.push(range);
        }
        if let Some(text) = text {
            let found = gaanim_text::code_diff::occurrences(&source, text);
            if found.is_empty() {
                return Err(CodeError::Missing {
                    text: text.to_owned(),
                });
            }
            ranges.extend(found);
        }
        if lines.is_none() && text.is_none() {
            ranges.push(0..source.len());
        }
        Ok(ranges)
    }

    /// Show `ranges` of the current source and dim the rest to `dim` from
    /// the cursor.
    pub fn select(&self, ranges: Vec<Range<usize>>, dim: f32) -> Result<(), CodeError> {
        check_dim(dim)?;
        let current = self.current.lock().expect("code block poisoned");
        let anim_type = AnimationType::CodeSelection {
            source: Arc::from(current.source.as_str()),
            ranges,
            effect: CodeSelectionEffect::Select { dim },
        };
        let rate_func = anim_type.default_rate_func();
        self.group
            .state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Immediate(AnimationBuilder {
                target: current.body.id,
                anim_type,
                duration: 0.0,
                delay: 0.0,
                rate_func,
            }));
        Ok(())
    }

    /// Fade the current source outside `ranges` to `dim`, and what `ranges`
    /// cover back to full opacity.
    pub fn select_anim(
        &self,
        ranges: Vec<Range<usize>>,
        dim: f32,
        duration: Option<f64>,
    ) -> Result<Anim, CodeError> {
        check_dim(dim)?;
        check_duration(duration)?;
        self.selection_anim(ranges, CodeSelectionEffect::Select { dim }, duration)
    }

    /// A text selection effect on the glyphs that draw `ranges` of the
    /// current source.
    pub fn effect_anim(
        &self,
        ranges: Vec<Range<usize>>,
        effect: TextSelectionEffect,
        duration: Option<f64>,
    ) -> Result<Anim, CodeError> {
        check_duration(duration)?;
        self.selection_anim(ranges, CodeSelectionEffect::Text(effect), duration)
    }

    fn selection_anim(
        &self,
        ranges: Vec<Range<usize>>,
        effect: CodeSelectionEffect,
        duration: Option<f64>,
    ) -> Result<Anim, CodeError> {
        let current = self.current.lock().expect("code block poisoned");
        Ok(Self::anim(
            &current.body,
            AnimationType::CodeSelection {
                source: Arc::from(current.source.as_str()),
                ranges,
                effect,
            },
            duration,
        ))
    }

    fn anim(body: &DrawableHandle, anim_type: AnimationType, duration: Option<f64>) -> Anim {
        let active_idx = body.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(body.id, anim_type, body.state.clone(), active_idx).with_duration(duration)
    }
}

fn check_dim(dim: f32) -> Result<(), CodeError> {
    if dim.is_finite() && (0.0..=1.0).contains(&dim) {
        Ok(())
    } else {
        Err(CodeError::Dim)
    }
}

fn check_duration(duration: Option<f64>) -> Result<(), CodeError> {
    match duration {
        Some(duration) if !duration.is_finite() || duration <= 0.0 => Err(CodeError::Duration),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bevy::ecs::world::CommandQueue;
    use bevy::prelude::{ChildOf, Commands, Entity, World};
    use gaanim_core::kurbo::Shape;
    use gaanim_math::SpatialTransform;
    use gaanim_scene::components::{Opacity, TextSpan};
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    use super::*;
    use crate::anim::TextSelectionEffect;

    fn block(scene: &mut SceneModel, source: &str) -> CodeHandle {
        let panel = scene.rounded_rect(7.6, 3.0, 0.1);
        scene
            .code_block(
                &[&panel],
                source,
                CodeStyle {
                    language: "python".to_owned(),
                    font_size: 0.2,
                    width: 7.0,
                    font: None,
                    foreground: Color::WHITE,
                    highlight: CodeHighlight::Off,
                    top_left: (-3.5, 1.0),
                },
            )
            .unwrap()
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

    fn drawn_opacity(world: &World, entity: Entity) -> f32 {
        let mut opacity = 1.0;
        let mut current = Some(entity);
        while let Some(node) = current {
            opacity *= world.get::<Opacity>(node).map_or(1.0, |value| value.0);
            current = world.get::<ChildOf>(node).map(ChildOf::parent);
        }
        opacity
    }

    /// The glyphs of every body: its characters in reading order, with how
    /// opaque each is drawn and where its outline's box starts in the scene.
    fn bodies(world: &mut World) -> Vec<Vec<(char, f32, f64)>> {
        let glyphs: Vec<(Entity, Entity, TextSpan)> = world
            .query::<(Entity, &ChildOf, &TextSpan)>()
            .iter(world)
            .map(|(entity, child_of, span)| (entity, child_of.parent(), *span))
            .collect();
        let mut bodies: BTreeMap<Entity, Vec<(usize, char, f32, f64)>> = BTreeMap::new();
        for (glyph, body, span) in glyphs {
            let mut affine = gaanim_core::kurbo::Affine::IDENTITY;
            let mut current = Some(glyph);
            while let Some(node) = current {
                if let Some(transform) = world.get::<SpatialTransform>(node) {
                    affine = transform.to_affine_2d() * affine;
                }
                current = world.get::<ChildOf>(node).map(ChildOf::parent);
            }
            let left = world
                .get::<gaanim_scene::components::Path2D>(glyph)
                .map_or(0.0, |path| {
                    (affine * path.0.as_ref().clone()).bounding_box().x0
                });
            bodies.entry(body).or_default().push((
                span.char_index,
                span.character,
                drawn_opacity(world, glyph),
                left,
            ));
        }
        bodies
            .into_values()
            .map(|mut glyphs| {
                glyphs.sort_by_key(|glyph| glyph.0);
                glyphs
                    .into_iter()
                    .map(|(_, character, opacity, left)| (character, opacity, left))
                    .collect()
            })
            .collect()
    }

    /// The characters drawn at full opacity, body by body.
    fn shown(world: &mut World) -> Vec<String> {
        bodies(world)
            .into_iter()
            .map(|glyphs| {
                glyphs
                    .into_iter()
                    .filter(|(_, opacity, _)| *opacity > 0.999)
                    .map(|(character, _, _)| character)
                    .collect::<String>()
            })
            .filter(|text| !text.is_empty())
            .collect()
    }

    #[test]
    fn an_edit_morphs_the_kept_tokens_and_ends_on_the_new_source() {
        let mut scene = SceneModel::new(640, 360);
        let code = block(&mut scene, "x = 1 + y");
        scene.wait(0.5);
        let anim = code.edit_anim(&mut scene, "x = 2 + y", Some(1.0)).unwrap();
        scene.play(vec![anim]);
        scene.wait(0.5);
        assert_eq!(code.source(), "x = 2 + y");
        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.25);
        assert_eq!(shown(&mut world), ["x=1+y"]);
        timeline.seek(&mut world, 1.9);
        assert_eq!(shown(&mut world), ["x=2+y"]);
        // The pairs are the kept tokens: four glyph morphs, one leaving and
        // one entering glyph.
        let morphs = timeline
            .clips
            .values()
            .filter(|clip| {
                matches!(
                    &clip.payload,
                    gaanim_timeline::clip::ClipPayload::Animation(spec)
                        if spec.label.as_deref() == Some("EquationSemanticMorph")
                )
            })
            .count();
        assert!(morphs >= 4, "{morphs} morph clips");
    }

    #[test]
    fn an_edit_before_the_block_is_shown_changes_its_declared_source() {
        let mut scene = SceneModel::new(640, 360);
        let code = block(&mut scene, "a = 1");
        code.edit(&mut scene, "b = 22").unwrap();
        scene.wait(0.5);
        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.25);
        assert_eq!(shown(&mut world), ["b=22"]);
        assert_eq!(bodies(&mut world).len(), 1);
    }

    #[test]
    fn an_immediate_edit_swaps_the_bodies_at_the_cursor() {
        let mut scene = SceneModel::new(640, 360);
        let code = block(&mut scene, "a = 1");
        scene.wait(0.5);
        code.edit(&mut scene, "a = 2").unwrap();
        scene.wait(0.5);
        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.25);
        assert_eq!(shown(&mut world), ["a=1"]);
        timeline.seek(&mut world, 0.75);
        assert_eq!(shown(&mut world), ["a=2"]);
    }

    #[test]
    fn a_new_body_lands_where_the_block_has_moved() {
        let mut scene = SceneModel::new(640, 360);
        let code = block(&mut scene, "total = 1");
        scene.wait(0.2);
        scene.play(vec![code.group.animate().shift_by(2.0, 0.0).duration(0.5)]);
        scene.wait(0.2);
        let anim = code.edit_anim(&mut scene, "total = 2", Some(0.5)).unwrap();
        scene.play(vec![anim]);
        scene.wait(0.2);
        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.8);
        let before = bodies(&mut world);
        timeline.seek(&mut world, 1.55);
        let after = bodies(&mut world);
        let first_shown = |bodies: &[Vec<(char, f32, f64)>]| {
            bodies
                .iter()
                .flatten()
                .find(|(character, opacity, _)| *character == 't' && *opacity > 0.999)
                .map(|(_, _, left)| *left)
                .unwrap()
        };
        let (from, to) = (first_shown(&before), first_shown(&after));
        assert!((from - to).abs() < 1e-6, "{from} → {to}");
        assert!(from > -1.6, "the block moved right: {from}");
    }

    #[test]
    fn a_selection_dims_the_other_lines() {
        let mut scene = SceneModel::new(640, 360);
        let code = block(&mut scene, "a = 1\nb = 2\nc = 3");
        scene.wait(0.2);
        let lines = code.ranges(Some((2, 2)), None).unwrap();
        let anim = code.select_anim(lines, 0.25, Some(0.5)).unwrap();
        scene.play(vec![anim]);
        scene.wait(0.2);
        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 0.85);
        let [glyphs] = bodies(&mut world).try_into().unwrap();
        let opacities: Vec<(char, f32)> = glyphs
            .into_iter()
            .map(|(character, opacity, _)| (character, (opacity * 100.0).round() / 100.0))
            .collect();
        assert_eq!(
            opacities,
            [
                ('a', 0.25),
                ('=', 0.25),
                ('1', 0.25),
                ('b', 1.0),
                ('=', 1.0),
                ('2', 1.0),
                ('c', 0.25),
                ('=', 0.25),
                ('3', 0.25)
            ]
        );
        // `select` with every line shows the block again.
        let all = code.ranges(None, None).unwrap();
        code.select(all, 0.25).unwrap();
        scene.wait(0.2);
        let (mut world, mut timeline) = compiled(&scene);
        timeline.seek(&mut world, 1.0);
        assert_eq!(shown(&mut world), ["a=1b=2c=3"]);
    }

    #[test]
    fn selections_and_edits_reject_invalid_input() {
        let mut scene = SceneModel::new(640, 360);
        let code = block(&mut scene, "a\nb");
        assert_eq!(code.line_count(), 2);
        assert_eq!(
            code.ranges(Some((2, 3)), None),
            Err(CodeError::Lines {
                first: 2,
                last: 3,
                count: 2
            })
        );
        assert_eq!(code.ranges(None, Some("b")).unwrap(), [2..3]);
        assert_eq!(
            code.ranges(None, Some("z")),
            Err(CodeError::Missing {
                text: "z".to_owned()
            })
        );
        assert!(matches!(code.select(vec![], 1.5), Err(CodeError::Dim)));
        assert!(matches!(
            code.edit_anim(&mut scene, "  ", None),
            Err(CodeError::EmptySource)
        ));
        assert!(matches!(
            code.effect_anim(vec![], TextSelectionEffect::Highlight, Some(0.0)),
            Err(CodeError::Duration)
        ));
        assert_eq!(code.source(), "a\nb");
    }
}
