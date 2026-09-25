//! Text range animator (TX-02) and the presets built on it: masked reveals
//! (TX-01), blur-in and tracking (TX-05).
//!
//! A range selector sweeps across the units of a Text (graphemes, words,
//! explicit lines or semantic parts). Each unit owns a window of the sweep;
//! inside it the unit's *influence* follows the selector shape and every
//! glyph of the unit interpolates between its rest state (influence 0) and
//! the animator's "out" state (influence 1). The influence of each unit is
//! baked into a native rate function of clip progress, so a seek to `t`
//! evaluates exactly what continuous playback shows and no Python runs per
//! frame.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::prelude::{Entity, World};
use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec2, DVec3};
use gaanim_core::kurbo::{self, Shape};
use gaanim_core::peniko::{Brush, Color};
use gaanim_math::{EasingCurve, RateFunc, RepeatMode, SpatialTransform};
use gaanim_renderer::effects::{ClipMask, GaussianBlur};
use gaanim_scene::{FillBrush, Opacity};
use gaanim_text::prelude::{TextAlign, TextRevealUnit};
use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};

use super::{Anim, DrawableHandle, SpawnKind};
use crate::anim::{AnimationBuilder, AnimationType, DrawOrder};
use crate::builder::SceneBuilder;
use crate::effect_lens::{EffectLens, EffectState};

/// Samples of the per-unit influence table inside the unit's window.
const INFLUENCE_SAMPLES: usize = 129;
/// Smallest unit window, as a fraction of the sweep, when an explicit
/// stagger does not fit in the duration.
const MIN_UNIT_SPAN: f64 = 0.05;
/// Vertical padding of a unit mask, as a fraction of its row height.
const MASK_PAD: f64 = 0.04;
/// Largest rotation a unit can reach: rotations interpolate along the
/// shortest arc, so half a turn is the unambiguous limit.
const MAX_UNIT_ROTATION: f64 = std::f64::consts::PI * 0.999;
const INFLUENCE_EPSILON: f64 = 1.0e-9;

/// Profile of the range selector: the influence of a unit as a function of
/// its eased progress through its window.
///
/// `Square`, `Ramp`, `Smooth`, `EaseIn` and `EaseOut` move a unit from the
/// out state (influence 1) to rest (influence 0), so a sweep reveals.
/// `Triangle` and `Round` rise to the out state and settle back, so a sweep
/// sends a wave through the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectorShape {
    Square,
    Ramp,
    #[default]
    Smooth,
    EaseIn,
    EaseOut,
    Triangle,
    Round,
}

impl SelectorShape {
    pub const NAMES: &'static str =
        "square, ramp, smooth, ease_in, ease_out, triangle, or round";

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "square" => Self::Square,
            "ramp" => Self::Ramp,
            "smooth" => Self::Smooth,
            "ease_in" => Self::EaseIn,
            "ease_out" => Self::EaseOut,
            "triangle" => Self::Triangle,
            "round" => Self::Round,
            _ => return None,
        })
    }

    /// Influence of a unit whose eased window progress is `progress`.
    /// `Ramp` is not clamped, so overshooting easings overshoot the rest
    /// state as well.
    pub fn influence(self, progress: f64) -> f64 {
        let clamped = progress.clamp(0.0, 1.0);
        match self {
            Self::Square => {
                if progress < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Ramp => 1.0 - progress,
            Self::Smooth => 1.0 - clamped * clamped * (3.0 - 2.0 * clamped),
            Self::EaseIn => 1.0 - clamped * clamped * clamped,
            Self::EaseOut => (1.0 - clamped).powi(3),
            Self::Triangle => 1.0 - (2.0 * clamped - 1.0).abs(),
            Self::Round => (std::f64::consts::PI * clamped).sin(),
        }
    }
}

/// The "out" state of a text animator. Unset channels keep the rest state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextAnimatorOut {
    /// Displacement in scene units.
    pub offset: DVec2,
    /// Displacement in multiples of each unit's row height (used by the
    /// masked reveal presets so a unit clears its own mask).
    pub relative_offset: DVec2,
    /// Absolute opacity of every glyph.
    pub opacity: Option<f32>,
    /// Scale factor around the center of each unit.
    pub scale: Option<f64>,
    /// Counterclockwise rotation in radians around the center of each unit.
    pub rotation: Option<f64>,
    /// Gaussian blur sigma in scene units.
    pub blur: Option<f64>,
    /// Extra space between neighboring glyphs, in scene units.
    pub tracking: Option<f64>,
    /// Solid fill color.
    pub color: Option<Color>,
}

/// What a [`TextAnimatorSpec`] schedules.
#[derive(Debug, Clone, PartialEq)]
pub enum TextAnimatorKind {
    /// Move the range from `from` to `to` (0 is before the first unit, 1
    /// after the last). `stagger` is the delay in seconds between units;
    /// `None` staggers adaptively. `mask` clips each unit to its row box.
    /// `exit` complements the influence, so the range takes units from
    /// rest to the out state: an exit that continues the reveal's motion,
    /// with units leaving in `order`.
    Sweep {
        from: f64,
        to: f64,
        stagger: Option<f64>,
        mask: bool,
        exit: bool,
    },
    /// Set the extra spacing between glyphs to `value` scene units.
    Tracking { value: f64 },
}

/// Deferred per-unit text animation, resolved against glyph geometry when
/// the scene compiles.
#[derive(Debug, Clone, PartialEq)]
pub struct TextAnimatorSpec {
    pub kind: TextAnimatorKind,
    pub shape: SelectorShape,
    pub order: DrawOrder,
    pub seed: u64,
    pub out: TextAnimatorOut,
    /// Visible characters with the index of the unit containing each one.
    pub unit_chars: Vec<(char, Option<usize>)>,
    /// Visible characters with the index of their explicit line.
    pub line_chars: Vec<(char, Option<usize>)>,
    /// Tracking anchor inside each row: 0 left, 0.5 center, 1 right.
    pub align: f64,
}

/// Errors raised while authoring text animations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextAnimatorError {
    #[error("{0}() requires a Text target")]
    NotText(&'static str),
}

/// Built-in styles of [`Anim::text_reveal`] and [`Anim::text_conceal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextRevealStyle {
    #[default]
    SlideUp,
    SlideDown,
    Fade,
    Scale,
    Blur,
}

impl TextRevealStyle {
    pub const NAMES: &'static str = "slide_up, slide_down, fade, scale, or blur";

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "slide_up" => Self::SlideUp,
            "slide_down" => Self::SlideDown,
            "fade" => Self::Fade,
            "scale" => Self::Scale,
            "blur" => Self::Blur,
            _ => return None,
        })
    }

    /// Out state of the style. Masked slides travel one full row so each
    /// unit hides behind its own mask; unmasked slides travel less and fade.
    pub fn out_state(self, mask: bool) -> TextAnimatorOut {
        let slide = |direction: f64| {
            if mask {
                TextAnimatorOut {
                    relative_offset: DVec2::new(0.0, direction),
                    ..Default::default()
                }
            } else {
                TextAnimatorOut {
                    relative_offset: DVec2::new(0.0, 0.6 * direction),
                    opacity: Some(0.0),
                    ..Default::default()
                }
            }
        };
        match self {
            Self::SlideUp => slide(-1.0),
            Self::SlideDown => slide(1.0),
            Self::Fade => TextAnimatorOut {
                opacity: Some(0.0),
                ..Default::default()
            },
            Self::Scale => TextAnimatorOut {
                opacity: Some(0.0),
                scale: Some(0.4),
                ..Default::default()
            },
            Self::Blur => TextAnimatorOut {
                opacity: Some(0.0),
                blur: Some(0.15),
                ..Default::default()
            },
        }
    }

    /// Out state of the style as an exit: slides keep travelling the way the
    /// reveal moves (`SlideUp` leaves upward), the other styles mirror it.
    pub fn exit_state(self, mask: bool) -> TextAnimatorOut {
        let mut out = self.out_state(mask);
        out.relative_offset = -out.relative_offset;
        out
    }

    fn masks(self) -> bool {
        matches!(self, Self::SlideUp | Self::SlideDown)
    }
}

/// Default per-unit easing of the reveal presets.
fn preset_easing() -> RateFunc {
    RateFunc::EaseOut(EasingCurve::Cubic)
}

/// Unit segmentation of a Text spec, or `None` for other drawables.
fn text_units(
    spec: &super::ObjectSpec,
    unit: TextRevealUnit,
) -> Option<(Vec<(char, Option<usize>)>, Vec<(char, Option<usize>)>, f64)> {
    let SpawnKind::Text(text) = &spec.kind else {
        return None;
    };
    let align = match text.flow.align {
        TextAlign::Left => 0.0,
        TextAlign::Center | TextAlign::Justify => 0.5,
        TextAlign::Right => 1.0,
    };
    Some((
        text.visible_char_units(unit),
        text.visible_char_units(TextRevealUnit::Line),
        align,
    ))
}

fn spec_for(
    spec: &Mutex<super::ObjectSpec>,
    kind: TextAnimatorKind,
    unit: TextRevealUnit,
    shape: SelectorShape,
    order: DrawOrder,
    out: TextAnimatorOut,
    operation: &'static str,
) -> Result<TextAnimatorSpec, TextAnimatorError> {
    let spec = spec.lock().expect("object spec poisoned");
    let (unit_chars, line_chars, align) =
        text_units(&spec, unit).ok_or(TextAnimatorError::NotText(operation))?;
    Ok(TextAnimatorSpec {
        kind,
        shape,
        order,
        seed: 0,
        out,
        unit_chars,
        line_chars,
        align,
    })
}

/// A reusable range selector over the units of one Text.
///
/// [`Self::set`] defines the out state; [`Self::sweep`] returns an ordinary
/// [`Anim`] that composes with `play`, `parallel`, `sequence` and `stagger`.
#[derive(Clone)]
pub struct TextAnimator {
    handle: DrawableHandle,
    spec: Arc<Mutex<TextAnimatorSpec>>,
}

impl std::fmt::Debug for TextAnimator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextAnimator")
            .field("target", &self.handle.id)
            .field("spec", &*self.spec.lock().expect("animator poisoned"))
            .finish()
    }
}

impl TextAnimator {
    /// Update the out state in place.
    pub fn set(&self, update: impl FnOnce(&mut TextAnimatorOut)) {
        update(&mut self.spec.lock().expect("animator poisoned").out);
    }

    /// The current out state.
    pub fn out_state(&self) -> TextAnimatorOut {
        self.spec.lock().expect("animator poisoned").out.clone()
    }

    /// Sweep the range from `from` to `to`; `0 → 1` moves across every unit
    /// once. `stagger` is the delay in seconds between consecutive units.
    /// The easing of the returned animation eases each unit's transition.
    pub fn sweep(&self, from: f64, to: f64, stagger: Option<f64>) -> Anim {
        let mut spec = self.spec.lock().expect("animator poisoned").clone();
        spec.kind = TextAnimatorKind::Sweep {
            from,
            to,
            stagger,
            mask: false,
            exit: false,
        };
        let mut anim = self.handle.animate();
        anim.inner.anim_type = AnimationType::TextAnimator(Box::new(spec));
        anim.inner.rate_func = RateFunc::Linear;
        anim
    }
}

impl DrawableHandle {
    /// Create a range animator over the `unit`s of this Text.
    pub fn animator(
        &self,
        unit: TextRevealUnit,
        shape: SelectorShape,
        order: DrawOrder,
        seed: u64,
    ) -> Result<TextAnimator, TextAnimatorError> {
        let mut spec = spec_for(
            &self.spec,
            TextAnimatorKind::Sweep {
                from: 0.0,
                to: 1.0,
                stagger: None,
                mask: false,
                exit: false,
            },
            unit,
            shape,
            order,
            TextAnimatorOut::default(),
            "animator",
        )?;
        spec.seed = seed;
        Ok(TextAnimator {
            handle: self.clone(),
            spec: Arc::new(Mutex::new(spec)),
        })
    }

    /// Immediately set the extra spacing between glyphs, in scene units,
    /// without laying the text out again.
    pub fn tracking(self, value: f64) -> Result<Self, TextAnimatorError> {
        let spec = spec_for(
            &self.spec,
            TextAnimatorKind::Tracking { value },
            TextRevealUnit::Grapheme,
            SelectorShape::default(),
            DrawOrder::Forward,
            TextAnimatorOut::default(),
            "tracking",
        )?;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .push_immediate(AnimationBuilder {
                target: self.id,
                anim_type: AnimationType::TextAnimator(Box::new(spec)),
                duration: 0.0,
                delay: 0.0,
                rate_func: RateFunc::Linear,
            });
        Ok(self)
    }
}

impl Anim {
    fn text_animator(
        mut self,
        operation: &'static str,
        kind: TextAnimatorKind,
        unit: TextRevealUnit,
        out: TextAnimatorOut,
        rate_func: RateFunc,
    ) -> Result<Self, TextAnimatorError> {
        let spec = self
            .property_spec
            .as_ref()
            .ok_or(TextAnimatorError::NotText(operation))?;
        let spec = spec_for(
            spec,
            kind,
            unit,
            SelectorShape::Ramp,
            DrawOrder::Forward,
            out,
            operation,
        )?;
        self.inner.anim_type = AnimationType::TextAnimator(Box::new(spec));
        self.inner.rate_func = rate_func;
        Ok(self)
    }

    /// Reveal a Text unit by unit, `stagger` seconds apart. Slide styles
    /// clip each unit to its row box when `mask` is set, so units rise from
    /// behind an invisible edge; other styles ignore `mask`.
    pub fn text_reveal(
        self,
        unit: TextRevealUnit,
        style: TextRevealStyle,
        mask: bool,
        stagger: f64,
    ) -> Result<Self, TextAnimatorError> {
        let mask = mask && style.masks();
        self.text_animator(
            "reveal",
            TextAnimatorKind::Sweep {
                from: 0.0,
                to: 1.0,
                stagger: Some(stagger),
                mask,
                exit: false,
            },
            unit,
            style.out_state(mask),
            preset_easing(),
        )
    }

    /// The exit matching [`Self::text_reveal`]: units leave in reading
    /// order, `stagger` seconds apart, and stay hidden. Slides continue the
    /// reveal's motion, so `SlideUp` exits upward out of each row mask and
    /// `SlideDown` downward; units accelerate out (ease-in cubic).
    pub fn text_conceal(
        self,
        unit: TextRevealUnit,
        style: TextRevealStyle,
        mask: bool,
        stagger: f64,
    ) -> Result<Self, TextAnimatorError> {
        let mask = mask && style.masks();
        self.text_animator(
            "conceal",
            TextAnimatorKind::Sweep {
                from: 0.0,
                to: 1.0,
                stagger: Some(stagger),
                mask,
                exit: true,
            },
            unit,
            style.exit_state(mask),
            RateFunc::EaseIn(EasingCurve::Cubic),
        )
    }

    /// Bring units in from a transparent Gaussian blur of `sigma` scene
    /// units, `stagger` seconds apart.
    pub fn blur_in(
        self,
        sigma: f64,
        unit: TextRevealUnit,
        stagger: f64,
    ) -> Result<Self, TextAnimatorError> {
        self.text_animator(
            "blur_in",
            TextAnimatorKind::Sweep {
                from: 0.0,
                to: 1.0,
                stagger: Some(stagger),
                mask: false,
                exit: false,
            },
            unit,
            TextAnimatorOut {
                opacity: Some(0.0),
                blur: Some(sigma.max(0.0)),
                ..Default::default()
            },
            preset_easing(),
        )
    }

    /// Animate the extra spacing between glyphs to `value` scene units,
    /// shifting glyphs without laying the text out again.
    pub fn tracking(self, value: f64) -> Result<Self, TextAnimatorError> {
        self.text_animator(
            "tracking",
            TextAnimatorKind::Tracking { value },
            TextRevealUnit::Grapheme,
            TextAnimatorOut::default(),
            RateFunc::Smooth,
        )
    }
}

impl TextAnimatorSpec {
    /// Glyph channels written by this animation, used to reject two
    /// simultaneous animations that would overwrite each other.
    pub fn channels(&self) -> Vec<String> {
        let out = &self.out;
        let (translation, mask) = match self.kind {
            TextAnimatorKind::Tracking { .. } => (true, false),
            TextAnimatorKind::Sweep { mask, .. } => (
                out.offset != DVec2::ZERO
                    || out.relative_offset != DVec2::ZERO
                    || out.tracking.is_some(),
                mask,
            ),
        };
        [
            (translation, "translation"),
            (out.rotation.is_some(), "rotation"),
            (out.scale.is_some(), "scale"),
            (out.opacity.is_some(), "opacity"),
            (out.color.is_some(), "fill"),
            (out.blur.is_some(), "blur"),
            (mask, "clip"),
        ]
        .into_iter()
        .filter(|(present, _)| *present)
        .map(|(_, name)| format!("glyphs:{name}"))
        .collect()
    }

    /// Direction of a ping-pong cycle played backward.
    fn reversed(&self, previous_tracking: f64) -> Self {
        let mut spec = self.clone();
        match &mut spec.kind {
            TextAnimatorKind::Sweep { from, to, .. } => std::mem::swap(from, to),
            TextAnimatorKind::Tracking { value } => *value = previous_tracking,
        }
        spec
    }
}

/// Stagger slot of each of `count` units; `Random` is a permutation fixed
/// by `seed` and `count`.
pub(crate) fn unit_slots(count: usize, order: DrawOrder, seed: u64) -> Vec<usize> {
    if order != DrawOrder::Random {
        return crate::builder::draw_group_slots(count, order);
    }
    // Fisher-Yates driven by SplitMix64; seed 0 matches `write(order="random")`.
    let mut state =
        0x9E37_79B9_7F4A_7C15_u64 ^ count as u64 ^ seed.wrapping_mul(0xD1B5_4A32_D192_ED03);
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    };
    let mut units: Vec<usize> = (0..count).collect();
    for index in (1..count).rev() {
        let other = (next() % (index as u64 + 1)) as usize;
        units.swap(index, other);
    }
    let mut slots = vec![0; count];
    for (slot, unit) in units.into_iter().enumerate() {
        slots[unit] = slot;
    }
    slots
}

/// Fraction of the sweep each unit's window spans.
pub(crate) fn unit_span(duration: f64, slots: usize, stagger: Option<f64>) -> f64 {
    if slots <= 1 {
        return 1.0;
    }
    let steps = (slots - 1) as f64;
    match stagger {
        Some(stagger) if duration > 0.0 => {
            ((duration - steps * stagger.max(0.0)) / duration).clamp(MIN_UNIT_SPAN, 1.0)
        }
        Some(_) => 1.0,
        None => 1.0 / (1.0 + steps * crate::builder::adaptive_lag_ratio(slots)),
    }
}

/// Rate function giving the influence of the unit whose window of the range
/// is `window` while the range runs `from → to` over the clip. `easing`
/// eases the unit's progress through its window; `exit` complements the
/// shape so the unit goes from rest to the out state.
pub(crate) fn unit_rate(
    shape: SelectorShape,
    easing: &RateFunc,
    window: (f64, f64),
    from: f64,
    to: f64,
    exit: bool,
) -> RateFunc {
    let (low, high) = window;
    let influence = |progress: f64| {
        let influence = shape.influence(easing.evaluate(progress));
        if exit { 1.0 - influence } else { influence }
    };
    if (to - from).abs() <= f64::EPSILON || high <= low {
        let progress = if high <= low {
            if from < low { 0.0 } else { 1.0 }
        } else {
            ((from - low) / (high - low)).clamp(0.0, 1.0)
        };
        return RateFunc::Sampled(Arc::from([influence(progress)]));
    }
    let enter = (low - from) / (to - from);
    let leave = (high - from) / (to - from);
    let reversed = enter > leave;
    let (start, end) = if reversed {
        (leave, enter)
    } else {
        (enter, leave)
    };
    let table: Vec<f64> = (0..INFLUENCE_SAMPLES)
        .map(|index| {
            let fraction = index as f64 / (INFLUENCE_SAMPLES - 1) as f64;
            influence(if reversed { 1.0 - fraction } else { fraction })
        })
        .collect();
    RateFunc::Squish {
        inner: Box::new(RateFunc::Sampled(table.into())),
        start,
        end,
    }
}

/// Split explicit lines into visual rows: a glyph whose top lies below the
/// middle of the current row starts the next one (scene y points up).
pub(crate) fn split_rows(lines: &[Vec<usize>], boxes: &[kurbo::Rect]) -> Vec<Vec<usize>> {
    let mut rows = Vec::new();
    for line in lines {
        let mut row: Vec<usize> = Vec::new();
        let mut extent: Option<kurbo::Rect> = None;
        for &glyph in line {
            let bounds = boxes[glyph];
            if let Some(current) = extent
                && bounds.y1 < (current.y0 + current.y1) * 0.5
            {
                rows.push(std::mem::take(&mut row));
                extent = None;
            }
            row.push(glyph);
            extent = Some(extent.map_or(bounds, |current| current.union(bounds)));
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }
    rows
}

/// Tracking multiplier of each glyph: its gap count from the row's anchor
/// (`align` 0 left, 0.5 center, 1 right), in visual left-to-right order.
pub(crate) fn tracking_factors(
    rows: &[Vec<usize>],
    boxes: &[kurbo::Rect],
    align: f64,
    count: usize,
) -> Vec<f64> {
    let mut factors = vec![0.0; count];
    for row in rows {
        let mut ordered = row.clone();
        ordered.sort_by(|a, b| boxes[*a].center().x.total_cmp(&boxes[*b].center().x));
        let anchor = align * ordered.len().saturating_sub(1) as f64;
        for (index, glyph) in ordered.into_iter().enumerate() {
            factors[glyph] = index as f64 - anchor;
        }
    }
    factors
}

/// Transform of a glyph at influence `t` between its rest and out states,
/// matching the translation/rotation/scale lenses of the timeline.
fn lerp_transform(rest: &SpatialTransform, out: &SpatialTransform, t: f64) -> SpatialTransform {
    SpatialTransform {
        translation: rest.translation.lerp(out.translation, t),
        rotation: rest.rotation.slerp(out.rotation, t),
        scale: rest.scale.lerp(out.scale, t),
        anchor: rest.anchor,
    }
}

/// Clips a glyph to the fixed box of its unit while the glyph moves, so it
/// slides in from behind an invisible edge. The mask lives in the Text's
/// local space; the glyph transform at `t` is recomputed here, so the
/// result does not depend on the order in which clips are evaluated.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitMaskLens {
    pub rect: kurbo::Rect,
    pub rest: SpatialTransform,
    pub out: SpatialTransform,
}

impl UnitMaskLens {
    /// The mask in the glyph's local space, or `None` at rest.
    pub fn mask_at(&self, t: f64) -> Option<kurbo::BezPath> {
        if t.abs() <= INFLUENCE_EPSILON {
            return None;
        }
        let mut path = self.rect.to_path(0.1);
        path.apply_affine(
            lerp_transform(&self.rest, &self.out, t)
                .to_affine_2d()
                .inverse(),
        );
        Some(path)
    }
}

fn unit_clip(path: kurbo::BezPath) -> ClipMask {
    ClipMask {
        path,
        rule: gaanim_core::peniko::Fill::NonZero,
        sources: Vec::new(),
        invert: false,
    }
}

impl gaanim_animation::AnimatableLens for UnitMaskLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let mask = self.mask_at(t).map(unit_clip);
        if world.get::<ClipMask>(entity) == mask.as_ref() {
            return;
        }
        let Ok(mut target) = world.get_entity_mut(entity) else {
            return;
        };
        match mask {
            Some(mask) => {
                target.insert(mask);
            }
            None => {
                target.remove::<ClipMask>();
            }
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "TextUnitMask"
    }
}

/// Glyphs of a Text root in reading order with their geometry in the
/// root's local space.
struct GlyphLayout {
    ids: Vec<ObjectId>,
    boxes: Vec<kurbo::Rect>,
    units: Vec<Vec<usize>>,
    rows: Vec<Vec<usize>>,
    row_of: Vec<usize>,
}

fn dynamic(lens: impl gaanim_animation::AnimatableLens) -> PropertyLensSpec {
    PropertyLensSpec::Dynamic(gaanim_animation::tween::DynamicLens(Arc::new(lens)))
}

impl SceneBuilder<'_, '_, '_> {
    fn text_glyph_layout(&self, root: ObjectId, spec: &TextAnimatorSpec) -> Option<GlyphLayout> {
        let state = self.states.get(root)?;
        let glyphs: Vec<(ObjectId, char)> = state
            .child_spans
            .iter()
            .filter(|child| self.states.get(child.id).is_some())
            .map(|child| (child.id, child.span.character))
            .collect();
        if glyphs.is_empty() {
            return None;
        }
        let ids: Vec<ObjectId> = glyphs.iter().map(|(id, _)| *id).collect();
        let index: HashMap<ObjectId, usize> =
            ids.iter().enumerate().map(|(index, id)| (*id, index)).collect();
        let boxes: Vec<kurbo::Rect> = ids
            .iter()
            .map(|id| {
                let glyph = self.states.get(*id).expect("glyph state exists");
                let affine = glyph.transform.to_affine_2d();
                if glyph.path.elements().is_empty() {
                    let origin = affine * kurbo::Point::ORIGIN;
                    kurbo::Rect::from_points(origin, origin)
                } else {
                    affine.transform_rect_bbox(glyph.path.bounding_box())
                }
            })
            .collect();
        let resolve = |chars: &[(char, Option<usize>)]| {
            super::compile::reveal_groups(&glyphs, chars).map(|groups| {
                groups
                    .into_iter()
                    .map(|group| group.iter().map(|id| index[id]).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            })
        };
        let units = resolve(&spec.unit_chars)
            .unwrap_or_else(|| (0..ids.len()).map(|glyph| vec![glyph]).collect());
        let lines = resolve(&spec.line_chars).unwrap_or_else(|| vec![(0..ids.len()).collect()]);
        let rows = split_rows(&lines, &boxes);
        let mut row_of = vec![0; ids.len()];
        for (row, members) in rows.iter().enumerate() {
            for &glyph in members {
                row_of[glyph] = row;
            }
        }
        Some(GlyphLayout {
            ids,
            boxes,
            units,
            rows,
            row_of,
        })
    }

    /// Schedule a [`TextAnimatorSpec`] on the glyphs of the Text `anim.target`.
    pub(crate) fn play_text_animator_internal(
        &mut self,
        anim: AnimationBuilder,
        spec: TextAnimatorSpec,
    ) {
        if let RateFunc::Repeat {
            inner,
            count,
            gap,
            mode,
        } = &anim.rate_func
        {
            // Each cycle is a complete sweep eased by `inner`; ping-pong
            // cycles run the range backward.
            let count = (*count).max(1);
            let gap = gap.max(0.0);
            let cycle = anim.duration / (count as f64 + (count - 1) as f64 * gap);
            let previous = self.text_tracking.get(&anim.target).copied().unwrap_or(0.0);
            for index in 0..count {
                let cycle_spec = if *mode == RepeatMode::PingPong && index % 2 == 1 {
                    spec.reversed(previous)
                } else {
                    spec.clone()
                };
                self.play_text_animator_internal(
                    AnimationBuilder {
                        target: anim.target,
                        anim_type: AnimationType::TextAnimator(Box::new(cycle_spec.clone())),
                        duration: cycle,
                        delay: anim.delay + index as f64 * cycle * (1.0 + gap),
                        rate_func: (**inner).clone(),
                    },
                    cycle_spec,
                );
            }
            return;
        }
        let Some(layout) = self.text_glyph_layout(anim.target, &spec) else {
            bevy::prelude::warn!("text animations require a Text target with visible glyphs");
            return;
        };
        let world = self.get_world_transform(anim.target).to_affine_2d();
        let coefficients = world.as_coeffs();
        let x_scale = coefficients[0].hypot(coefficients[1]).max(1.0e-12);
        let factors = tracking_factors(&layout.rows, &layout.boxes, spec.align, layout.ids.len());
        let track = self.ensure_track(anim.target);
        let start = self.current_time + anim.delay;
        let label = Some("TextAnimator".to_string());

        let (from, to, stagger, mask, exit) = match spec.kind {
            TextAnimatorKind::Tracking { value } => {
                let current = self.text_tracking.insert(anim.target, value).unwrap_or(0.0);
                let delta = (value - current) / x_scale;
                if delta.abs() <= f64::EPSILON {
                    return;
                }
                for (glyph, id) in layout.ids.iter().enumerate() {
                    let shift = delta * factors[glyph];
                    if shift == 0.0 {
                        continue;
                    }
                    let state = self.states.get_mut(*id).expect("glyph state exists");
                    let from = state.transform.translation;
                    let to = from + DVec3::new(shift, 0.0, 0.0);
                    state.transform.translation = to;
                    self.timeline.add_clip(
                        track,
                        start,
                        anim.duration,
                        ClipPayload::Animation(AnimationSpec {
                            target: *id,
                            lens: PropertyLensSpec::Translation { from, to },
                            rate_func: anim.rate_func.clone(),
                            delay: 0.0,
                            label: label.clone(),
                        }),
                    );
                }
                return;
            }
            TextAnimatorKind::Sweep {
                from,
                to,
                stagger,
                mask,
                exit,
            } => (from, to, stagger, mask, exit),
        };

        let out = &spec.out;
        let inverse = world.inverse();
        let local_offset = inverse * kurbo::Point::new(out.offset.x, out.offset.y)
            - inverse * kurbo::Point::ORIGIN;
        let slots = unit_slots(layout.units.len(), spec.order, spec.seed);
        let slot_count = slots.iter().max().map_or(1, |slot| slot + 1);
        let span = unit_span(anim.duration, slot_count, stagger);
        let mut end_influences = Vec::with_capacity(layout.units.len());

        for (unit, members) in layout.units.iter().enumerate() {
            let position = if slot_count > 1 {
                slots[unit] as f64 / (slot_count - 1) as f64
            } else {
                0.0
            };
            let low = position * (1.0 - span);
            let rate = unit_rate(
                spec.shape,
                &anim.rate_func,
                (low, low + span),
                from,
                to,
                exit,
            );
            let initial = rate.evaluate(0.0);
            let final_influence = rate.evaluate(1.0);
            end_influences.push(final_influence);

            let unit_box = members
                .iter()
                .map(|glyph| layout.boxes[*glyph])
                .reduce(|a, b| a.union(b))
                .expect("units are non-empty");
            let row_box = members
                .iter()
                .flat_map(|glyph| layout.rows[layout.row_of[*glyph]].iter())
                .map(|glyph| layout.boxes[*glyph])
                .reduce(|a, b| a.union(b))
                .unwrap_or(unit_box);
            let mask_box = kurbo::Rect::new(unit_box.x0, row_box.y0, unit_box.x1, row_box.y1);
            let pad = MASK_PAD * mask_box.height();
            let travel = mask_box.height() + 2.0 * pad;
            let pivot = unit_box.center();

            for &glyph in members {
                let id = layout.ids[glyph];
                let state = self.states.get_mut(id).expect("glyph state exists");
                let entity = state.entity;
                let mut rest = state.transform;
                let identity_linear = rest.rotation.abs_diff_eq(DQuat::IDENTITY, 1.0e-12)
                    && rest.scale.abs_diff_eq(DVec3::ONE, 1.0e-12);
                if (out.rotation.is_some() || out.scale.is_some()) && identity_linear {
                    // With an identity rotation and scale the anchor does not
                    // move the glyph, so pivot it on the unit center.
                    let anchor = DVec3::new(
                        pivot.x - rest.translation.x,
                        pivot.y - rest.translation.y,
                        rest.anchor.z,
                    );
                    if anchor != rest.anchor {
                        rest.anchor = anchor;
                        state.transform.anchor = anchor;
                        self.commands.entity(entity).insert(rest);
                    }
                }
                let mut target = rest;
                let tracking = out.tracking.unwrap_or(0.0) * factors[glyph] / x_scale;
                target.translation += DVec3::new(
                    local_offset.x + out.relative_offset.x * travel + tracking,
                    local_offset.y + out.relative_offset.y * travel,
                    0.0,
                );
                if let Some(rotation) = out.rotation {
                    target.rotation = rest.rotation
                        * DQuat::from_rotation_z(
                            rotation.clamp(-MAX_UNIT_ROTATION, MAX_UNIT_ROTATION),
                        );
                }
                if let Some(scale) = out.scale {
                    target.scale = rest.scale * scale;
                }
                let moves = target != rest;
                let rest_opacity = state.opacity;
                let rest_fill = match &state.fill {
                    Some(Brush::Solid(color)) => Some(*color),
                    _ => None,
                };
                let rest_effects = self.effects.get(&id).cloned().unwrap_or_default();

                let mut lenses = Vec::new();
                if target.translation != rest.translation {
                    lenses.push(PropertyLensSpec::Translation {
                        from: rest.translation,
                        to: target.translation,
                    });
                }
                if out.rotation.is_some() {
                    lenses.push(PropertyLensSpec::Rotation {
                        from: rest.rotation,
                        to: target.rotation,
                    });
                }
                if out.scale.is_some() {
                    lenses.push(PropertyLensSpec::Scale {
                        from: rest.scale,
                        to: target.scale,
                    });
                }
                if let Some(opacity) = out.opacity {
                    lenses.push(PropertyLensSpec::Opacity {
                        from: rest_opacity,
                        to: opacity,
                    });
                }
                let color = out.color.zip(rest_fill);
                if let Some((to, from)) = color {
                    lenses.push(PropertyLensSpec::FillColor { from, to });
                }
                let blur = out.blur.map(|sigma| EffectLens {
                    from: rest_effects.clone(),
                    to: EffectState {
                        blur: (sigma > 0.0).then_some(GaussianBlur { sigma }),
                        ..rest_effects.clone()
                    },
                });
                if let Some(lens) = &blur {
                    lenses.push(dynamic(lens.clone()));
                }
                let unit_mask = (mask && moves).then(|| UnitMaskLens {
                    rect: mask_box.inflate(mask_box.height(), pad),
                    rest,
                    out: target,
                });
                if let Some(lens) = &unit_mask {
                    lenses.push(dynamic(lens.clone()));
                }

                // Entry sweeps hold their first frame until they start, as
                // fade_in and write do.
                if initial.abs() > INFLUENCE_EPSILON {
                    let mut commands = self.commands.entity(entity);
                    if moves {
                        commands.insert(lerp_transform(&rest, &target, initial));
                    }
                    if let Some(opacity) = out.opacity {
                        commands.insert(Opacity(
                            rest_opacity + (opacity - rest_opacity) * initial as f32,
                        ));
                    }
                    if let Some((to, from)) = color {
                        commands.insert(FillBrush(Some(Brush::Solid(
                            gaanim_core::interpolate_color(from, to, initial),
                        ))));
                    }
                    if let Some(component) = blur.as_ref().and_then(|lens| lens.blur_at(initial)) {
                        commands.insert(component);
                    }
                    if let Some(path) = unit_mask.as_ref().and_then(|lens| lens.mask_at(initial)) {
                        commands.insert(unit_clip(path));
                    }
                }

                let state = self.states.get_mut(id).expect("glyph state exists");
                if moves {
                    state.transform = lerp_transform(&rest, &target, final_influence);
                }
                if let Some(opacity) = out.opacity {
                    state.opacity = rest_opacity + (opacity - rest_opacity) * final_influence as f32;
                }
                if let Some((to, from)) = color {
                    state.fill = Some(Brush::Solid(gaanim_core::interpolate_color(
                        from,
                        to,
                        final_influence,
                    )));
                }
                if let Some(lens) = &blur {
                    let effects = EffectState {
                        blur: lens.blur_at(final_influence),
                        ..rest_effects
                    };
                    if effects == EffectState::default() {
                        self.effects.remove(&id);
                    } else {
                        self.effects.insert(id, effects);
                    }
                }

                for lens in lenses {
                    self.timeline.add_clip(
                        track,
                        start,
                        anim.duration,
                        ClipPayload::Animation(AnimationSpec {
                            target: id,
                            lens,
                            rate_func: rate.clone(),
                            delay: 0.0,
                            label: label.clone(),
                        }),
                    );
                }
            }
        }

        // A sweep that leaves every unit equally shifted keeps that tracking
        // for later `tracking(...)` animations.
        if let Some(tracking) = out.tracking
            && let Some(first) = end_influences.first()
            && end_influences
                .iter()
                .all(|influence| (influence - first).abs() <= INFLUENCE_EPSILON)
            && first.abs() > INFLUENCE_EPSILON
        {
            *self.text_tracking.entry(anim.target).or_insert(0.0) += tracking * first;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_shapes_reveal_or_pass_a_wave() {
        for shape in [
            SelectorShape::Square,
            SelectorShape::Ramp,
            SelectorShape::Smooth,
            SelectorShape::EaseIn,
            SelectorShape::EaseOut,
        ] {
            assert_eq!(shape.influence(0.0), 1.0, "{shape:?} starts out");
            assert_eq!(shape.influence(1.0), 0.0, "{shape:?} ends at rest");
        }
        for shape in [SelectorShape::Triangle, SelectorShape::Round] {
            assert!(shape.influence(0.0).abs() < 1e-12);
            assert!((shape.influence(0.5) - 1.0).abs() < 1e-12);
            assert!(shape.influence(1.0).abs() < 1e-12);
        }
        assert!((SelectorShape::Smooth.influence(0.5) - 0.5).abs() < 1e-12);
        assert_eq!(SelectorShape::Square.influence(0.49), 1.0);
        assert_eq!(SelectorShape::Square.influence(0.51), 0.0);
        // Ramp keeps an easing's overshoot.
        assert!(SelectorShape::Ramp.influence(1.1) < 0.0);
        assert_eq!(SelectorShape::parse("ease_out"), Some(SelectorShape::EaseOut));
        assert_eq!(SelectorShape::parse("wobble"), None);
    }

    #[test]
    fn unit_rates_follow_their_window_and_hold_outside() {
        let rate = unit_rate(SelectorShape::Ramp, &RateFunc::Linear, (0.25, 0.75), 0.0, 1.0, false);
        assert!((rate.evaluate(0.0) - 1.0).abs() < 1e-12);
        assert!((rate.evaluate(0.25) - 1.0).abs() < 1e-12);
        assert!((rate.evaluate(0.5) - 0.5).abs() < 1e-9);
        assert!(rate.evaluate(0.75).abs() < 1e-12);
        assert!(rate.evaluate(1.0).abs() < 1e-12);
        // Evaluation is a pure function of progress: seeks repeat exactly.
        assert_eq!(rate.evaluate(0.4).to_bits(), rate.evaluate(0.4).to_bits());

        // A backward range enters the window from its far end.
        let back = unit_rate(SelectorShape::Ramp, &RateFunc::Linear, (0.25, 0.75), 1.0, 0.0, false);
        assert!(back.evaluate(0.0).abs() < 1e-12);
        assert!((back.evaluate(0.5) - 0.5).abs() < 1e-9);
        assert!((back.evaluate(1.0) - 1.0).abs() < 1e-12);

        // The unit easing shapes the transition inside the window.
        let eased = unit_rate(
            SelectorShape::Ramp,
            &RateFunc::EaseOut(EasingCurve::Cubic),
            (0.0, 1.0),
            0.0,
            1.0,
            false,
        );
        assert!(eased.evaluate(0.5) < 0.2);

        // An exit runs forward from rest to the out state.
        let exit = unit_rate(SelectorShape::Ramp, &RateFunc::Linear, (0.25, 0.75), 0.0, 1.0, true);
        assert!(exit.evaluate(0.0).abs() < 1e-12);
        assert!((exit.evaluate(0.5) - 0.5).abs() < 1e-9);
        assert!((exit.evaluate(1.0) - 1.0).abs() < 1e-12);

        // A frozen range holds one influence.
        let frozen = unit_rate(SelectorShape::Ramp, &RateFunc::Linear, (0.0, 1.0), 0.5, 0.5, false);
        assert!((frozen.evaluate(0.0) - 0.5).abs() < 1e-12);
        assert!((frozen.evaluate(1.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn spans_fit_the_requested_stagger() {
        // Three units 0.1 s apart in one second: each unit gets 0.8 s.
        assert!((unit_span(1.0, 3, Some(0.1)) - 0.8).abs() < 1e-12);
        assert_eq!(unit_span(1.0, 1, Some(0.1)), 1.0);
        assert_eq!(unit_span(1.0, 40, Some(1.0)), MIN_UNIT_SPAN);
        assert_eq!(unit_span(0.0, 3, Some(0.1)), 1.0);
        let adaptive = unit_span(1.0, 10, None);
        assert!(adaptive > 0.0 && adaptive < 1.0);
    }

    #[test]
    fn random_slots_are_seeded_permutations() {
        let a = unit_slots(12, DrawOrder::Random, 7);
        assert_eq!(a, unit_slots(12, DrawOrder::Random, 7));
        assert_ne!(a, unit_slots(12, DrawOrder::Random, 8));
        let mut sorted = a.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..12).collect::<Vec<_>>());
        assert_eq!(
            unit_slots(12, DrawOrder::Random, 0),
            crate::builder::draw_group_slots(12, DrawOrder::Random)
        );
        assert_eq!(unit_slots(3, DrawOrder::Reverse, 5), [2, 1, 0]);
    }

    #[test]
    fn rows_split_wrapped_lines_and_tracking_spreads_from_the_anchor() {
        let glyph = |x: f64, y: f64| kurbo::Rect::new(x, y, x + 0.4, y + 0.5);
        let boxes = vec![
            glyph(0.0, 0.0),
            glyph(0.5, 0.0),
            glyph(1.0, 0.0),
            // Wrapped onto the next row, below the first.
            glyph(0.0, -0.8),
            glyph(0.5, -0.8),
        ];
        let rows = split_rows(&[vec![0, 1, 2, 3, 4]], &boxes);
        assert_eq!(rows, vec![vec![0, 1, 2], vec![3, 4]]);

        let centered = tracking_factors(&rows, &boxes, 0.5, 5);
        assert_eq!(centered, [-1.0, 0.0, 1.0, -0.5, 0.5]);
        let left = tracking_factors(&rows, &boxes, 0.0, 5);
        assert_eq!(left, [0.0, 1.0, 2.0, 0.0, 1.0]);
    }

    #[test]
    fn masks_stay_fixed_while_the_glyph_moves() {
        let rest = SpatialTransform::new_2d(1.0, 0.0);
        let out = SpatialTransform::new_2d(1.0, -1.0);
        let lens = UnitMaskLens {
            rect: kurbo::Rect::new(0.0, 0.0, 2.0, 1.0),
            rest,
            out,
        };
        assert!(lens.mask_at(0.0).is_none());
        // Halfway the glyph sits 0.5 lower, so in its own space the mask is
        // 0.5 higher; seen from the parent it has not moved.
        let bounds = lens.mask_at(0.5).unwrap().bounding_box();
        assert!((bounds.y0 - 0.5).abs() < 1e-9 && (bounds.y1 - 1.5).abs() < 1e-9);
        assert!((bounds.x0 + 1.0).abs() < 1e-9);
    }

    fn compiled(canvas: &super::super::SceneModel) -> gaanim_timeline::timeline::Timeline {
        use bevy::ecs::world::CommandQueue;
        use bevy::prelude::Commands;
        let world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = gaanim_timeline::timeline::Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        timeline
    }

    /// `(target, start, lens debug, rate at 0, rate at 1)` of animator clips.
    fn animator_clips(
        timeline: &gaanim_timeline::timeline::Timeline,
    ) -> Vec<(ObjectId, f64, String, f64, f64)> {
        let mut clips: Vec<_> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(spec)
                    if spec.label.as_deref() == Some("TextAnimator") =>
                {
                    Some((
                        spec.target,
                        clip.start,
                        format!("{:?}", spec.lens),
                        spec.rate_func.evaluate(0.0),
                        spec.rate_func.evaluate(1.0),
                    ))
                }
                _ => None,
            })
            .collect();
        clips.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        clips
    }

    #[test]
    fn line_reveal_masks_each_line_and_lands_at_rest() {
        let mut canvas = super::super::SceneModel::new(640, 360);
        let text = canvas.text("uno dos\ntres");
        canvas.play(vec![
            text.animate()
                .text_reveal(TextRevealUnit::Line, TextRevealStyle::SlideUp, true, 0.2)
                .unwrap()
                .duration(1.0),
        ]);
        let clips = animator_clips(&compiled(&canvas));
        let translations: Vec<_> = clips
            .iter()
            .filter(|clip| clip.2.starts_with("Translation"))
            .collect();
        let masks: Vec<_> = clips
            .iter()
            .filter(|clip| clip.2.contains("UnitMaskLens"))
            .collect();
        // One translation and one mask per visible glyph ("unodostres").
        assert_eq!(translations.len(), 10);
        assert_eq!(masks.len(), 10);
        // Every glyph starts out of place and ends at rest.
        assert!(translations
            .iter()
            .all(|clip| (clip.3 - 1.0).abs() < 1e-9 && clip.4.abs() < 1e-9));
        assert!(clips.iter().all(|clip| clip.2.find("Opacity").is_none()));
    }

    #[test]
    fn staggered_units_enter_in_order_and_conceal_leaves_them_out() {
        let influence_at = |anim: fn(Anim) -> Anim, t: f64| {
            let mut canvas = super::super::SceneModel::new(640, 360);
            let text = canvas.text("ab cd");
            canvas.play(vec![anim(text.animate()).duration(1.0)]);
            let timeline = compiled(&canvas);
            let mut opacity: Vec<(ObjectId, f64, f64)> = timeline
                .clips
                .values()
                .filter_map(|clip| match &clip.payload {
                    ClipPayload::Animation(spec)
                        if matches!(spec.lens, PropertyLensSpec::Opacity { .. }) =>
                    {
                        Some((
                            spec.target,
                            spec.rate_func.evaluate(t),
                            spec.rate_func.evaluate(1.0),
                        ))
                    }
                    _ => None,
                })
                .collect();
            opacity.sort_by_key(|clip| clip.0);
            opacity
        };
        let reveal = influence_at(
            |anim| {
                anim.text_reveal(TextRevealUnit::Word, TextRevealStyle::Fade, true, 0.3)
                    .unwrap()
            },
            0.5,
        );
        assert_eq!(reveal.len(), 4);
        // Halfway, the first word is further along (less out) than the second.
        assert!(reveal[0].1 < reveal[2].1);
        assert_eq!(reveal[0].1, reveal[1].1);
        assert!(reveal.iter().all(|clip| clip.2.abs() < 1e-9));

        let conceal = influence_at(
            |anim| {
                anim.text_conceal(TextRevealUnit::Word, TextRevealStyle::Fade, true, 0.3)
                    .unwrap()
            },
            0.5,
        );
        // The first word leaves first, like the reveal's order, and every
        // glyph ends hidden.
        assert!(conceal[0].1 > conceal[2].1);
        assert!(conceal.iter().all(|clip| (clip.2 - 1.0).abs() < 1e-9));
    }

    #[test]
    fn slide_conceal_keeps_moving_the_way_the_reveal_moved() {
        // Vertical travel (`to.y - from.y`) of every glyph translation.
        let travel = |anim: fn(Anim) -> Anim| {
            let mut canvas = super::super::SceneModel::new(640, 360);
            let text = canvas.text("uno\ndos");
            canvas.play(vec![anim(text.animate()).duration(1.0)]);
            animator_clips(&compiled(&canvas))
                .into_iter()
                .filter(|clip| clip.2.starts_with("Translation"))
                .filter_map(|clip| {
                    let y = |value: &str| value.split(',').nth(1)?.trim().parse::<f64>().ok();
                    let from = clip.2.split("from: DVec3(").nth(1)?;
                    let to = clip.2.split("to: DVec3(").nth(1)?;
                    Some((y(to)? - y(from)?, clip.3, clip.4))
                })
                .collect::<Vec<_>>()
        };
        let reveal = travel(|anim| {
            anim.text_reveal(TextRevealUnit::Line, TextRevealStyle::SlideUp, true, 0.2)
                .unwrap()
        });
        let conceal = travel(|anim| {
            anim.text_conceal(TextRevealUnit::Line, TextRevealStyle::SlideUp, true, 0.2)
                .unwrap()
        });
        assert_eq!(reveal.len(), 6);
        assert_eq!(conceal.len(), 6);
        // The reveal rises from below (its out state is lower)...
        assert!(reveal.iter().all(|clip| clip.0 < 0.0 && clip.1 > 0.99));
        // ...and the conceal starts at rest and leaves upward.
        assert!(conceal
            .iter()
            .all(|clip| clip.0 > 0.0 && clip.1.abs() < 1e-9 && (clip.2 - 1.0).abs() < 1e-9));
        let down = travel(|anim| {
            anim.text_conceal(TextRevealUnit::Line, TextRevealStyle::SlideDown, true, 0.2)
                .unwrap()
        });
        assert!(down.iter().all(|clip| clip.0 < 0.0));
    }

    #[test]
    fn tracking_is_absolute_and_spreads_around_the_anchor() {
        let mut canvas = super::super::SceneModel::new(640, 360);
        let text = canvas.text("abc");
        let text = text.tracking(0.4).unwrap();
        canvas.play(vec![text.animate().tracking(0.0).unwrap().duration(1.0)]);
        let clips = animator_clips(&compiled(&canvas));
        let shifts: Vec<f64> = clips
            .iter()
            .filter(|clip| clip.2.starts_with("Translation"))
            .filter_map(|clip| {
                let from = clip.2.split("from: DVec3(").nth(1)?;
                let to = clip.2.split("to: DVec3(").nth(1)?;
                let x = |value: &str| value.split(',').next()?.trim().parse::<f64>().ok();
                Some(x(to)? - x(from)?)
            })
            .collect();
        // Left-aligned: the first glyph stays and the others move one and
        // two gaps when tracking opens, then move back when it closes.
        let mut opened: Vec<f64> = shifts.iter().copied().filter(|s| *s > 0.0).collect();
        let mut closed: Vec<f64> = shifts.iter().map(|s| -s).filter(|s| *s > 0.0).collect();
        opened.sort_by(f64::total_cmp);
        closed.sort_by(f64::total_cmp);
        assert_eq!(opened.len(), 2, "{shifts:?}");
        assert!((opened[1] - 2.0 * opened[0]).abs() < 1e-9, "{shifts:?}");
        assert_eq!(opened.len(), closed.len());
        assert!(opened.iter().zip(&closed).all(|(a, b)| (a - b).abs() < 1e-9));
    }

    #[test]
    fn animator_rejects_non_text_targets() {
        let mut canvas = super::super::SceneModel::new(640, 360);
        let circle = canvas.circle(1.0);
        assert_eq!(
            circle
                .animator(TextRevealUnit::Grapheme, SelectorShape::Smooth, DrawOrder::Forward, 0)
                .unwrap_err(),
            TextAnimatorError::NotText("animator")
        );
        assert!(circle.animate().blur_in(0.3, TextRevealUnit::Grapheme, 0.02).is_err());
    }

    #[test]
    fn reveal_styles_describe_their_out_state() {
        let masked = TextRevealStyle::SlideUp.out_state(true);
        assert_eq!(masked.relative_offset, DVec2::new(0.0, -1.0));
        assert_eq!(masked.opacity, None);
        let unmasked = TextRevealStyle::SlideDown.out_state(false);
        assert!(unmasked.relative_offset.y > 0.0);
        assert_eq!(unmasked.opacity, Some(0.0));
        assert!(TextRevealStyle::Blur.out_state(true).blur.is_some());
        assert!(!TextRevealStyle::Fade.masks());
        assert_eq!(TextRevealStyle::parse("slide_up"), Some(TextRevealStyle::SlideUp));
        assert_eq!(TextRevealStyle::parse("wipe"), None);
    }
}
