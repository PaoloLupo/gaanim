//! Typewriter and scramble text motion (TX-03, TX-04).
//!
//! Authoring resolves a [`TextMotion`] against the object's typed state
//! ([`TypedText`]) to know its default duration. Compilation turns it into
//! one timeline clip per glyph (plus one for the cursor) whose lenses from
//! [`gaanim_animation::text_motion`] rewrite `Path2D` as a pure function of
//! progress. New content (`retype`, `scramble_to`) is typeset once into extra
//! glyph entities under the same Text root, aligned to the original pen
//! origin, and kept hidden until the motion reveals it.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use bevy::prelude::{BuildChildrenTransformExt, Entity};
use gaanim_animation::text_motion::{
    CursorLens, GlyphVisibilityLens, ScrambleCharset, ScrambleGlyphLens,
};
use gaanim_animation::tween::{AnimatableLens, DynamicLens};
use gaanim_core::ObjectId;
use gaanim_core::kurbo::{BezPath, Rect, Shape};
use gaanim_core::peniko::Brush;
use gaanim_math::{Bounds3D, RateFunc, SpatialTransform};
use gaanim_scene::{FillBrush, Path2D, StrokeBrush};
use gaanim_text::motion::{
    TypingLayout, TypingPlan, common_prefix_graphemes, grapheme_prefix, graphemes,
    scramble_duration, settle_fractions,
};
use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec, TrackId};

use crate::anim::{AnimationBuilder, AnimationType};
use crate::builder::{MobjectState, SceneBuilder};

/// Default typing speed, in graphemes per second.
pub const DEFAULT_CPS: f64 = 18.0;
/// Default deletion speed of `backspace`, in graphemes per second.
pub const DEFAULT_BACKSPACE_CPS: f64 = 24.0;
/// Default cursor glyph: a left three-eighths block.
pub const DEFAULT_CURSOR: &str = "\u{258D}";
/// Default cursor blink rate, in on/off cycles per second.
pub const DEFAULT_BLINK: f64 = 2.0;

/// What a text motion does, validated at authoring time.
#[derive(Debug, Clone, PartialEq)]
pub enum TextMotionKind {
    /// Clear the text, then type all of it (`cps` graphemes per second with
    /// seeded `jitter`), followed by an optional cursor.
    Typewriter {
        cps: f64,
        jitter: f64,
        seed: u64,
        cursor: Option<String>,
        blink: f64,
        keep_cursor: bool,
    },
    /// Delete the last `count` visible graphemes at a steady `cps`.
    Backspace { count: usize, cps: f64 },
    /// Delete back to the longest common prefix with `text`, then type the rest.
    Retype {
        text: String,
        cps: f64,
        jitter: f64,
        seed: u64,
    },
    /// Cycle seeded charset glyphs in each grapheme cell, settling left to
    /// right after `reveal_delay` seconds; `text` replaces the content.
    Scramble {
        text: Option<String>,
        charset: Vec<String>,
        reveal_delay: f64,
        speed: f64,
        seed: u64,
    },
}

/// The content a Text shows through typing motions: its document and how
/// many leading graphemes of it are visible.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TypedText {
    pub document: String,
    pub visible: usize,
}

impl TypedText {
    /// A fully visible document.
    pub fn full(document: impl Into<String>) -> Self {
        let document = document.into();
        let visible = graphemes(&document).len();
        Self { document, visible }
    }

    /// The visible prefix of the document.
    pub fn shown(&self) -> &str {
        grapheme_prefix(&self.document, self.visible)
    }
}

/// How a motion changes glyph visibility.
#[derive(Debug, Clone, PartialEq)]
pub enum Keystrokes {
    Typing {
        /// Hide every grapheme first (typewriter).
        restart: bool,
        /// Graphemes kept from the old document (the common prefix).
        kept: usize,
        /// Whether the typed graphemes belong to a new document.
        replace: bool,
        typed: Range<usize>,
        plan: TypingPlan,
    },
    Scramble {
        replace: bool,
        /// Graphemes that settle (all non-whitespace graphemes).
        settling: usize,
    },
}

/// Resolve a motion against the current typed state: the state afterwards,
/// the keystrokes, and the natural duration in seconds.
pub fn resolve(before: &TypedText, kind: &TextMotionKind) -> (TypedText, Keystrokes, f64) {
    let length = graphemes(&before.document).len();
    let visible = before.visible.min(length);
    match kind {
        TextMotionKind::Typewriter {
            cps, jitter, seed, ..
        } => {
            let plan = TypingPlan::new(0, *cps, 0..length, *cps, *jitter, *seed);
            let duration = plan.duration;
            (
                TypedText::full(before.document.clone()),
                Keystrokes::Typing {
                    restart: true,
                    kept: 0,
                    replace: false,
                    typed: 0..length,
                    plan,
                },
                duration,
            )
        }
        TextMotionKind::Backspace { count, cps } => {
            let count = (*count).min(visible);
            let plan = TypingPlan::new(count, *cps, 0..0, *cps, 0.0, 0);
            let duration = plan.duration;
            (
                TypedText {
                    document: before.document.clone(),
                    visible: visible - count,
                },
                Keystrokes::Typing {
                    restart: false,
                    kept: visible - count,
                    replace: false,
                    typed: 0..0,
                    plan,
                },
                duration,
            )
        }
        TextMotionKind::Retype {
            text,
            cps,
            jitter,
            seed,
        } => {
            let target = graphemes(text).len();
            let (kept, replace) = if *text == before.document {
                (visible, false)
            } else {
                (
                    common_prefix_graphemes(grapheme_prefix(&before.document, visible), text),
                    true,
                )
            };
            let plan = TypingPlan::new(visible - kept, *cps, kept..target, *cps, *jitter, *seed);
            let duration = plan.duration;
            (
                TypedText::full(text.clone()),
                Keystrokes::Typing {
                    restart: false,
                    kept,
                    replace,
                    typed: kept..target,
                    plan,
                },
                duration,
            )
        }
        TextMotionKind::Scramble {
            text, reveal_delay, ..
        } => {
            let document = text.clone().unwrap_or_else(|| before.document.clone());
            let replace = document != before.document;
            let settling = graphemes(&document)
                .iter()
                .filter(|grapheme| !grapheme.trim().is_empty())
                .count();
            (
                TypedText::full(document),
                Keystrokes::Scramble { replace, settling },
                scramble_duration(settling, *reveal_delay),
            )
        }
    }
}

/// Font and typesetting facts of the target Text, attached at compile time.
#[derive(Clone)]
pub struct TextMotionContext {
    /// The declared text as rendered (markup removed).
    pub rendered: String,
    pub font_family: String,
    pub math_font: String,
    pub font_size: f64,
    pub weight: Option<u16>,
    /// Typst source laying out new plain content exactly like the Text.
    pub typeset: Arc<dyn Fn(&str) -> Option<String> + Send + Sync>,
}

impl std::fmt::Debug for TextMotionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextMotionContext")
            .field("rendered", &self.rendered)
            .field("font_family", &self.font_family)
            .field("font_size", &self.font_size)
            .field("weight", &self.weight)
            .finish_non_exhaustive()
    }
}

/// A typewriter or scramble animation of one Text.
#[derive(Debug, Clone)]
pub struct TextMotion {
    pub kind: TextMotionKind,
    /// Typed state after the motion, committed to the authoring spec on play.
    pub after: TypedText,
    pub context: Option<TextMotionContext>,
}

#[derive(Clone)]
struct TypingCursor {
    id: ObjectId,
    entity: Entity,
    glyph: Arc<BezPath>,
    blink: f64,
    keep: bool,
    enabled: bool,
}

#[derive(Clone)]
struct TypingObject {
    document: String,
    visible: usize,
    layout: Arc<TypingLayout>,
    /// Glyph ids and full outlines of the current document.
    glyphs: Vec<(ObjectId, Arc<BezPath>)>,
    /// Transform of glyph entities (the Text's centering offset).
    transform: SpatialTransform,
    cursor: Option<TypingCursor>,
}

/// Compile-time typing state of every Text a motion has touched.
#[derive(Clone, Default)]
pub struct TextMotionState {
    objects: HashMap<ObjectId, TypingObject>,
    charsets: HashMap<String, Arc<ScrambleCharset>>,
}

/// Outline of a cursor string with its pen origin on the baseline. Left
/// block elements (U+2588–U+258F) are drawn as exact rectangles so the
/// cursor never depends on font coverage.
fn cursor_outline(
    registry: &gaanim_text::font::FontRegistry,
    context: &TextMotionContext,
    cursor: &str,
) -> BezPath {
    let em = context.font_size;
    let block = |eighths: u32| {
        let width = 0.6 * em * eighths as f64 / 8.0;
        Rect::new(0.02 * em, -0.22 * em, 0.02 * em + width, 0.78 * em).to_path(1e-9)
    };
    let mut chars = cursor.chars();
    if let (Some(character), None) = (chars.next(), chars.next())
        && ('\u{2588}'..='\u{258F}').contains(&character)
    {
        return block(8 - (character as u32 - 0x2588));
    }
    gaanim_text::typst_compiler::shape_typst_text_run(
        registry,
        cursor,
        &context.font_family,
        context.weight,
        context.font_size,
    )
    .ok()
    .map(|run| run.path)
    .filter(|path| !path.elements().is_empty())
    .unwrap_or_else(|| block(3))
}

/// Ids and outlines of the glyphs drawing grapheme `index`.
fn glyphs_of(
    layout: &TypingLayout,
    glyphs: &[(ObjectId, Arc<BezPath>)],
    index: usize,
) -> Vec<(ObjectId, Arc<BezPath>)> {
    layout.cells[index]
        .glyphs
        .iter()
        .filter_map(|glyph| glyphs.get(*glyph).cloned())
        .collect()
}

impl SceneBuilder<'_, '_, '_> {
    fn add_text_motion_clip(
        &mut self,
        track: TrackId,
        start: f64,
        duration: f64,
        target: ObjectId,
        rate_func: &RateFunc,
        lens: impl AnimatableLens,
    ) {
        self.timeline.add_clip(
            track,
            start,
            duration,
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::Dynamic(DynamicLens(Arc::new(lens))),
                rate_func: rate_func.clone(),
                delay: 0.0,
                label: self.current_label.clone(),
            }),
        );
    }

    fn typing_object(&self, root: ObjectId, context: &TextMotionContext) -> Option<TypingObject> {
        let state = self.states.get(root)?;
        let transform = state
            .child_spans
            .first()
            .map(|child| {
                self.states
                    .get(child.id)
                    .map_or(child.transform, |state| state.transform)
            })
            .unwrap_or_default();
        let boxes: Vec<(char, Rect)> = state
            .child_spans
            .iter()
            .map(|child| (child.span.character, child.path.bounding_box()))
            .collect();
        let first_baseline = self
            .text_metrics
            .get(&root)
            .map_or(0.0, |metrics| metrics.first_baseline)
            - transform.translation.y;
        let layout =
            TypingLayout::build(&context.rendered, &boxes, first_baseline, context.font_size);
        Some(TypingObject {
            document: context.rendered.clone(),
            visible: layout.graphemes.len(),
            layout: Arc::new(layout),
            glyphs: state
                .child_spans
                .iter()
                .map(|child| (child.id, child.path.clone()))
                .collect(),
            transform,
            cursor: None,
        })
    }

    /// Typeset `text` like the Text and add its glyphs, hidden, under `root`.
    fn spawn_typing_glyphs(
        &mut self,
        root: ObjectId,
        object: &TypingObject,
        context: &TextMotionContext,
        text: &str,
    ) -> Option<(Vec<(ObjectId, Arc<BezPath>)>, TypingLayout)> {
        let source = (context.typeset)(text)?;
        let root_entity = self.states.get(root)?.entity;
        let (fill, stroke) = object
            .glyphs
            .first()
            .and_then(|(id, _)| self.states.get(*id))
            .map(|state| (state.fill.clone(), state.stroke.clone()))
            .unwrap_or_else(|| {
                (
                    Some(Brush::Solid(gaanim_core::peniko::Color::WHITE)),
                    StrokeBrush::transparent(),
                )
            });
        let temp_id = self.next_id();
        let id_counter = &mut self.id_counter;
        let next_id_fn = move || {
            let id = ObjectId::from_parts(*id_counter, 1);
            *id_counter += 1;
            id
        };
        let mut children = Vec::new();
        let (temp_entity, _, metrics) =
            gaanim_text::typst_compiler::compile_scaled_typst_to_hierarchy(
                self.commands,
                self.font_registry,
                &source,
                false,
                Some(&context.font_family),
                Some(&context.math_font),
                Some(context.font_size),
                Some(context.font_size),
                fill.clone(),
                stroke.clone(),
                temp_id,
                next_id_fn,
                &mut children,
                1.0,
            );
        // Glyph paths are in page space; the new page shares the old origin.
        let page_center_y = children
            .first()
            .map_or(0.0, |child| -child.transform.translation.y);
        let first_baseline = metrics.first_baseline + page_center_y;
        let empty = Arc::new(BezPath::new());
        let mut merged = BezPath::new();
        let mut ink: Option<Rect> = None;
        let offset = gaanim_core::kurbo::Affine::translate((
            object.transform.translation.x,
            object.transform.translation.y,
        ));
        for child in &mut children {
            child.transform = object.transform;
            self.tag_entity(child.entity);
            self.commands
                .entity(child.entity)
                .insert((
                    Path2D(empty.clone()),
                    object.transform,
                    FillBrush(fill.clone()),
                    stroke.clone(),
                ))
                .set_parent_in_place(root_entity);
            merged.extend((offset * &*child.path).elements().iter().copied());
            let rect = offset.transform_rect_bbox(child.path.bounding_box());
            ink = Some(ink.map_or(rect, |ink| ink.union(rect)));
            self.states.insert(
                child.id,
                MobjectState {
                    fill_level: 0.0,
                    path: child.path.clone(),
                    bounds: child.bounds,
                    transform: object.transform,
                    opacity: 1.0,
                    fill: fill.clone(),
                    stroke: stroke.clone(),
                    entity: child.entity,
                    child_spans: Vec::new(),
                    children: Vec::new(),
                    parent: Some(root),
                    exclude_from_parent_draw: false,
                },
            );
        }
        self.commands.entity(temp_entity).despawn();

        let boxes: Vec<(char, Rect)> = children
            .iter()
            .map(|child| (child.span.character, child.path.bounding_box()))
            .collect();
        let layout = TypingLayout::build(text, &boxes, first_baseline, context.font_size);
        let glyphs = children
            .iter()
            .map(|child| (child.id, child.path.clone()))
            .collect();
        // Later text operations (selections, write, effects) see the new glyphs.
        if let Some(state) = self.states.get_mut(root) {
            state.children = children.iter().map(|child| child.id).collect();
            state.child_spans = children;
            state.path = Arc::new(merged);
            if let Some(ink) = ink {
                state.bounds = Bounds3D::new_2d(ink.x0, ink.y0, ink.x1, ink.y1);
            }
        }
        Some((glyphs, layout))
    }

    fn ensure_typing_cursor(
        &mut self,
        root: ObjectId,
        object: &mut TypingObject,
        context: &TextMotionContext,
        cursor: Option<&str>,
        blink: f64,
        keep: bool,
    ) {
        let glyph =
            cursor.map(|cursor| Arc::new(cursor_outline(self.font_registry, context, cursor)));
        if let Some(existing) = &mut object.cursor {
            existing.enabled = glyph.is_some();
            if let Some(glyph) = glyph {
                existing.glyph = glyph;
            }
            existing.blink = blink;
            existing.keep = keep;
            return;
        }
        let Some(glyph) = glyph else { return };
        let Some(root_entity) = self.states.get(root).map(|state| state.entity) else {
            return;
        };
        let fill = object
            .glyphs
            .first()
            .and_then(|(id, _)| self.states.get(*id))
            .and_then(|state| state.fill.clone())
            .or_else(|| Some(Brush::Solid(gaanim_core::peniko::Color::WHITE)));
        let id = self.next_id();
        let mut bundle =
            gaanim_objects::prelude::MobjectBundle::new(id, BezPath::new(), Bounds3D::default());
        bundle.fill = FillBrush(fill.clone());
        bundle.transform = object.transform;
        let entity = self.commands.spawn(bundle).id();
        self.commands
            .entity(entity)
            .set_parent_in_place(root_entity);
        self.tag_entity(entity);
        self.states.insert(
            id,
            MobjectState {
                fill_level: 0.0,
                path: Arc::new(BezPath::new()),
                bounds: Bounds3D::default(),
                transform: object.transform,
                opacity: 1.0,
                fill,
                stroke: StrokeBrush::transparent(),
                entity,
                child_spans: Vec::new(),
                children: Vec::new(),
                parent: Some(root),
                exclude_from_parent_draw: false,
            },
        );
        object.cursor = Some(TypingCursor {
            id,
            entity,
            glyph,
            blink,
            keep,
            enabled: true,
        });
    }

    fn scramble_charset(
        &mut self,
        context: &TextMotionContext,
        charset: &[String],
    ) -> Arc<ScrambleCharset> {
        let key = format!(
            "{}\u{1f}{:?}\u{1f}{}\u{1f}{}",
            context.font_family,
            context.weight,
            context.font_size.to_bits(),
            charset.join("\u{1f}")
        );
        if let Some(cached) = self.text_motion.charsets.get(&key) {
            return cached.clone();
        }
        let outlines = charset.iter().filter_map(|grapheme| {
            gaanim_text::typst_compiler::shape_typst_text_run(
                self.font_registry,
                grapheme,
                &context.font_family,
                context.weight,
                context.font_size,
            )
            .ok()
            .map(|run| run.path)
        });
        let shaped = Arc::new(ScrambleCharset::new(outlines));
        self.text_motion.charsets.insert(key, shaped.clone());
        shaped
    }

    /// Schedule a [`TextMotion`]: one visibility or scramble clip per
    /// affected glyph and one clip for the cursor.
    pub(crate) fn play_text_motion_internal(&mut self, anim: AnimationBuilder, track: TrackId) {
        let AnimationType::TextMotion(motion) = anim.anim_type else {
            return;
        };
        let Some(context) = motion.context.clone() else {
            bevy::prelude::warn!("text motion skipped: the target is not a Text");
            return;
        };
        let root = anim.target;
        let mut object = match self.text_motion.objects.remove(&root) {
            Some(object) => object,
            None => match self.typing_object(root, &context) {
                Some(object) => object,
                None => return,
            },
        };
        let start = self.current_time + anim.delay.max(0.0);
        let duration = anim.duration.max(0.0);
        let rate = anim.rate_func.clone();
        let before = TypedText {
            document: object.document.clone(),
            visible: object.visible,
        };
        let (after, keystrokes, _) = resolve(&before, &motion.kind);

        // Cursor configuration: typewriter sets it; other typing motions
        // reuse it, or start with the default cursor.
        match &motion.kind {
            TextMotionKind::Typewriter {
                cursor,
                blink,
                keep_cursor,
                ..
            } => self.ensure_typing_cursor(
                root,
                &mut object,
                &context,
                cursor.as_deref(),
                *blink,
                *keep_cursor,
            ),
            TextMotionKind::Backspace { .. } | TextMotionKind::Retype { .. }
                if object.cursor.is_none() =>
            {
                self.ensure_typing_cursor(
                    root,
                    &mut object,
                    &context,
                    Some(DEFAULT_CURSOR),
                    DEFAULT_BLINK,
                    true,
                )
            }
            _ => {}
        }

        let old_layout = object.layout.clone();
        let old_glyphs = object.glyphs.clone();
        let old_visible = before.visible.min(old_layout.cells.len());
        let mut cursor_steps: Vec<(f64, f64, f64)> = Vec::new();

        match keystrokes {
            Keystrokes::Typing {
                restart,
                kept,
                replace,
                typed,
                plan,
            } => {
                let fraction = |time: f64| plan.fraction(time, duration);
                let (x, y) = old_layout.cursor_after(if restart { 0 } else { old_visible });
                cursor_steps.push((0.0, x, y));
                // Deletions, last visible grapheme first.
                for (step, time) in plan.delete_at.iter().enumerate() {
                    let grapheme = old_visible - 1 - step;
                    let at = fraction(*time);
                    for (id, path) in glyphs_of(&old_layout, &old_glyphs, grapheme) {
                        self.add_text_motion_clip(
                            track,
                            start,
                            duration,
                            id,
                            &rate,
                            GlyphVisibilityLens {
                                path,
                                appear: None,
                                vanish: Some(at),
                            },
                        );
                    }
                    let (x, y) = old_layout.cursor_after(grapheme);
                    cursor_steps.push((at, x, y));
                }
                let mut layout = old_layout.clone();
                let mut glyphs = old_glyphs.clone();
                if replace {
                    let swap = plan.switch_fraction(duration);
                    let Some((new_glyphs, new_layout)) =
                        self.spawn_typing_glyphs(root, &object, &context, &after.document)
                    else {
                        bevy::prelude::warn!("text motion skipped: new text failed to typeset");
                        self.text_motion.objects.insert(root, object);
                        return;
                    };
                    // The kept prefix switches to the new glyphs when deleting ends.
                    for grapheme in 0..kept.min(old_layout.cells.len()) {
                        for (id, path) in glyphs_of(&old_layout, &old_glyphs, grapheme) {
                            self.add_text_motion_clip(
                                track,
                                start,
                                duration,
                                id,
                                &rate,
                                GlyphVisibilityLens {
                                    path,
                                    appear: None,
                                    vanish: Some(swap),
                                },
                            );
                        }
                    }
                    layout = Arc::new(new_layout);
                    glyphs = new_glyphs;
                    for grapheme in 0..kept.min(layout.cells.len()) {
                        for (id, path) in glyphs_of(&layout, &glyphs, grapheme) {
                            self.add_text_motion_clip(
                                track,
                                start,
                                duration,
                                id,
                                &rate,
                                GlyphVisibilityLens {
                                    path,
                                    appear: Some(swap),
                                    vanish: None,
                                },
                            );
                        }
                    }
                    let (x, y) = layout.cursor_after(kept);
                    cursor_steps.push((swap, x, y));
                }
                for (step, grapheme) in typed.clone().enumerate() {
                    let Some(time) = plan.type_at.get(step) else {
                        break;
                    };
                    if grapheme >= layout.cells.len() {
                        break;
                    }
                    let at = fraction(*time);
                    for (id, path) in glyphs_of(&layout, &glyphs, grapheme) {
                        self.add_text_motion_clip(
                            track,
                            start,
                            duration,
                            id,
                            &rate,
                            GlyphVisibilityLens {
                                path,
                                appear: Some(at),
                                vanish: None,
                            },
                        );
                    }
                    let (x, y) = layout.cursor_after(grapheme + 1);
                    cursor_steps.push((at, x, y));
                }
                object.layout = layout;
                object.glyphs = glyphs;
            }
            Keystrokes::Scramble { replace, settling } => {
                let TextMotionKind::Scramble {
                    charset,
                    reveal_delay,
                    speed,
                    seed,
                    ..
                } = &motion.kind
                else {
                    unreachable!("scramble keystrokes come from a scramble motion");
                };
                if replace {
                    let Some((new_glyphs, new_layout)) =
                        self.spawn_typing_glyphs(root, &object, &context, &after.document)
                    else {
                        bevy::prelude::warn!("text motion skipped: new text failed to typeset");
                        self.text_motion.objects.insert(root, object);
                        return;
                    };
                    for (id, path) in &old_glyphs {
                        self.add_text_motion_clip(
                            track,
                            start,
                            duration,
                            *id,
                            &rate,
                            GlyphVisibilityLens {
                                path: path.clone(),
                                appear: None,
                                vanish: Some(0.0),
                            },
                        );
                    }
                    object.layout = Arc::new(new_layout);
                    object.glyphs = new_glyphs;
                }
                let charset = self.scramble_charset(&context, charset);
                let delay = if duration > 0.0 {
                    reveal_delay / duration
                } else {
                    0.0
                };
                let settles = settle_fractions(settling, delay);
                let layout = object.layout.clone();
                let inked = (0..layout.cells.len()).filter(|index| {
                    !layout.graphemes[*index].trim().is_empty()
                        && !layout.cells[*index].glyphs.is_empty()
                });
                for (order, grapheme) in inked.enumerate() {
                    let settle_at = if duration > 0.0 {
                        settles.get(order).copied().unwrap_or(1.0)
                    } else {
                        0.0
                    };
                    let cell = &layout.cells[grapheme];
                    for (rank, (id, path)) in glyphs_of(&layout, &object.glyphs, grapheme)
                        .into_iter()
                        .enumerate()
                    {
                        self.add_text_motion_clip(
                            track,
                            start,
                            duration,
                            id,
                            &rate,
                            ScrambleGlyphLens {
                                final_path: path,
                                charset: charset.clone(),
                                anchor: (cell.center, cell.baseline),
                                settle_at,
                                duration,
                                speed: *speed,
                                seed: *seed,
                                position: grapheme,
                                primary: rank == 0,
                            },
                        );
                    }
                }
                let (x, y) = layout.cursor_after(layout.cells.len());
                cursor_steps.push((0.0, x, y));
            }
        }

        object.document = after.document;
        object.visible = after.visible;

        if let Some(cursor) = object.cursor.clone() {
            let steps = if cursor.enabled {
                cursor_steps
            } else {
                Vec::new()
            };
            self.add_text_motion_clip(
                track,
                start,
                duration,
                cursor.id,
                &rate,
                CursorLens {
                    glyph: cursor.glyph.clone(),
                    steps: Arc::from(steps),
                    hide_at_end: !cursor.keep,
                },
            );
            // Solid while typing; idle blinking resumes when the motion ends.
            let blink =
                (cursor.enabled && cursor.keep && cursor.blink > 0.0).then_some(cursor.blink);
            let end = start + duration;
            self.commands.entity(cursor.entity).queue(
                move |mut entity: bevy::prelude::EntityWorldMut| {
                    let mut motion = entity
                        .get::<gaanim_animation::ProceduralMotion>()
                        .cloned()
                        .unwrap_or_default();
                    motion.stop_at(start);
                    if let Some(frequency) = blink {
                        motion.push(
                            gaanim_animation::ProceduralLayer::Oscillate {
                                channel: gaanim_animation::OscillatedChannel::Opacity,
                                waveform: gaanim_animation::Waveform::Square,
                                frequency,
                                low: 0.0,
                                high: 1.0,
                                // Start visible for the first half period.
                                phase: 0.5,
                            },
                            end,
                        );
                    }
                    entity.insert(motion);
                },
            );
        }
        self.text_motion.objects.insert(root, object);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_state_follows_typewriter_backspace_and_retype() {
        let typed = TypedText::full("gaanim render");
        let typewriter = TextMotionKind::Typewriter {
            cps: 10.0,
            jitter: 0.0,
            seed: 0,
            cursor: None,
            blink: 0.0,
            keep_cursor: true,
        };
        let (after, _, duration) = resolve(&typed, &typewriter);
        assert_eq!(after, typed);
        assert!((duration - 1.3).abs() < 1e-12);

        let (short, keys, duration) = resolve(
            &typed,
            &TextMotionKind::Backspace {
                count: 6,
                cps: 20.0,
            },
        );
        assert_eq!(short.shown(), "gaanim ");
        assert!((duration - 0.3).abs() < 1e-12);
        assert!(matches!(keys, Keystrokes::Typing { kept: 7, .. }));
        // Deleting more than is visible stops at the start.
        let (empty, _, _) = resolve(
            &short,
            &TextMotionKind::Backspace {
                count: 99,
                cps: 20.0,
            },
        );
        assert_eq!(empty.visible, 0);

        let retype = TextMotionKind::Retype {
            text: "gaanim export --from clímax".into(),
            cps: 10.0,
            jitter: 0.0,
            seed: 0,
        };
        let (retyped, keys, duration) = resolve(&typed, &retype);
        assert_eq!(retyped, TypedText::full("gaanim export --from clímax"));
        let Keystrokes::Typing {
            kept,
            replace,
            typed: range,
            plan,
            ..
        } = keys
        else {
            panic!("retype types");
        };
        assert_eq!((kept, replace), (7, true));
        assert_eq!(range, 7..27);
        assert_eq!(plan.delete_at.len(), 6);
        assert!((duration - 2.6).abs() < 1e-9);

        // Retyping the same document continues after a backspace.
        let (_, keys, _) = resolve(
            &short,
            &TextMotionKind::Retype {
                text: "gaanim render".into(),
                cps: 10.0,
                jitter: 0.0,
                seed: 0,
            },
        );
        assert!(matches!(
            keys,
            Keystrokes::Typing {
                replace: false,
                kept: 7,
                ..
            }
        ));
    }

    #[test]
    fn scramble_reserves_the_final_text() {
        let typed = TypedText::full("LAUNCH");
        let (after, keys, duration) = resolve(
            &typed,
            &TextMotionKind::Scramble {
                text: Some("LANZA MIENTO".into()),
                charset: vec!["0".into(), "1".into()],
                reveal_delay: 0.3,
                speed: 20.0,
                seed: 0,
            },
        );
        assert_eq!(after, TypedText::full("LANZA MIENTO"));
        assert_eq!(
            keys,
            Keystrokes::Scramble {
                replace: true,
                settling: 11
            }
        );
        assert!((duration - (0.3 + 0.6)).abs() < 1e-12);
    }

    #[test]
    fn cursor_blocks_are_font_independent() {
        let registry = gaanim_text::font::FontRegistry::new();
        let context = TextMotionContext {
            rendered: String::new(),
            font_family: "New Computer Modern".into(),
            math_font: "New Computer Modern Math".into(),
            font_size: 1.0,
            weight: None,
            typeset: Arc::new(|_| None),
        };
        let bar = cursor_outline(&registry, &context, DEFAULT_CURSOR).bounding_box();
        assert!((bar.width() - 0.225).abs() < 1e-9);
        let full = cursor_outline(&registry, &context, "\u{2588}").bounding_box();
        assert!((full.width() - 0.6).abs() < 1e-9);
        assert!(bar.y0 < 0.0 && bar.y1 > 0.5);
    }
}
