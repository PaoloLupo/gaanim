//! Emphasis effects that draw their own helpers: a flash around or under a
//! target, ripples, a spotlight and an animated boundary.
//!
//! Each one spawns its helper drawables when it is declared, the way a
//! `surrounding_rect` is, and returns an [`Anim`] that plays on them. The
//! helpers stay invisible until their animation starts (and again after it),
//! so declaring an effect shows nothing.

use gaanim_core::peniko::Color;

use super::canvas_impl::{
    SurroundingRectError, SurroundingRectHandle, spawn_in, surrounding_rect_in,
};
use super::duplicate::clone_spec_in;
use super::ops::{Op, SharedCanvasState};
use super::types::SpawnKind;
use super::{Anim, BooleanRule, DrawableHandle, SceneModel};
use crate::anim::{AnimationType, BoundsTarget};

/// Most ripples one broadcast draws.
pub const MAX_RIPPLES: usize = 64;

/// Stroke width, in scene units, of a flash or boundary that names a color
/// without one.
const DEFAULT_STROKE_WIDTH: f64 = 0.05;

/// Most passes a hand-drawn notation draws over itself.
pub const MAX_NOTATION_PASSES: u32 = 8;

fn check_padding(padding: [f64; 4]) -> Result<(), String> {
    if padding
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
    {
        Ok(())
    } else {
        Err("padding must contain finite non-negative values".to_string())
    }
}

fn check_time_width(time_width: f64) -> Result<(), String> {
    if time_width.is_finite() && time_width > 0.0 && time_width <= 1.0 {
        Ok(())
    } else {
        Err("time_width must be in (0, 1]".to_string())
    }
}

/// A flash names a width only together with a color.
fn check_stroke(color: Option<Color>, width: Option<f64>) -> Result<(), String> {
    match (color, width) {
        (None, Some(_)) => Err("width needs a color".to_string()),
        (_, Some(width)) => check_positive("width", width),
        _ => Ok(()),
    }
}

fn check_positive(name: &str, value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be finite and positive"))
    }
}

/// `handle` stroked with `color`, or with the theme's stroke when it has none.
fn stroked(
    handle: DrawableHandle,
    color: Option<Color>,
    width: Option<f64>,
) -> Result<DrawableHandle, String> {
    match (color, width) {
        (Some(color), width) => {
            let width = width.unwrap_or(DEFAULT_STROKE_WIDTH);
            check_positive("width", width)?;
            Ok(handle.stroke(color, width))
        }
        (None, Some(_)) => Err("width needs a color".to_string()),
        (None, None) => Ok(handle),
    }
}

impl Anim {
    /// The scene and the object or text selection an effect draws around.
    fn emphasis_target(&self) -> Result<(SharedCanvasState, BoundsTarget), String> {
        if !self.inner.anim_type.is_empty_properties() {
            return Err("this effect cannot be combined with property targets or another effect in one Anim; combine separate animations with parallel()".to_string());
        }
        let state = self
            .owner
            .clone()
            .ok_or_else(|| "this effect needs a drawable of a scene".to_string())?;
        let target = match &self.inner.anim_type {
            AnimationType::TextSelectionProperties {
                fragment,
                occurrence,
                ..
            } => BoundsTarget::TextSelection {
                target: self.inner.target,
                fragment: fragment.clone(),
                occurrence: *occurrence,
            },
            _ => BoundsTarget::Drawable(self.inner.target),
        };
        Ok((state, target))
    }

    /// A line-only helper (a frame or an underline) swept by a bright
    /// window: the flash the returned animation plays.
    fn passing_flash(
        state: SharedCanvasState,
        helper: DrawableHandle,
        time_width: f64,
    ) -> Result<Anim, String> {
        let active = state.lock().expect("canvas state poisoned").active_idx;
        Ok(Anim::queued(
            helper.id,
            AnimationType::ShowPassingFlash { time_width },
            state,
            active,
        ))
    }

    /// A bright window sweeps a frame around the target. The frame is a new
    /// drawable that follows the target's bounds, `padding` (top, right,
    /// bottom, left) away, and is invisible outside the flash.
    pub fn flash_around(
        self,
        color: Option<Color>,
        width: Option<f64>,
        padding: [f64; 4],
        corner_radius: f64,
        time_width: f64,
    ) -> Result<Anim, String> {
        check_padding(padding)?;
        check_time_width(time_width)?;
        check_stroke(color, width)?;
        if !corner_radius.is_finite() || corner_radius < 0.0 {
            return Err("corner_radius must be finite and non-negative".to_string());
        }
        let (state, target) = self.emphasis_target()?;
        let frame = surrounding_rect_in(&state, vec![target], padding, corner_radius)
            .map_err(|error: SurroundingRectError| error.to_string())?;
        let helper = stroked(frame.drawable, color, width)?;
        Self::passing_flash(state, helper, time_width)
    }

    /// A bright window sweeps a line under the target, `gap` scene units
    /// below it and `overhang` wider on each side.
    pub fn flash_under(
        self,
        color: Option<Color>,
        width: Option<f64>,
        gap: f64,
        overhang: f64,
        time_width: f64,
    ) -> Result<Anim, String> {
        check_padding([0.0, overhang, gap, overhang])?;
        check_time_width(time_width)?;
        check_stroke(color, width)?;
        let (state, target) = self.emphasis_target()?;
        let line = spawn_in(&state, SpawnKind::SurroundingRect, true).no_fill();
        state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachUnderline {
                target: line.id,
                sources: vec![target],
                gap,
                overhang,
            });
        let helper = stroked(line, color, width)?;
        Self::passing_flash(state, helper, time_width)
    }

    /// `count` ripples spread from the target: copies of it, placed where it
    /// is when the animation starts, each grow to `max_scale` times its size
    /// while fading out, `lag` of a ripple after the previous one.
    pub fn broadcast(self, count: usize, max_scale: f64, lag: f64) -> Result<Anim, String> {
        if !(1..=MAX_RIPPLES).contains(&count) {
            return Err(format!(
                "count must be between 1 and {MAX_RIPPLES}, got {count}"
            ));
        }
        if !max_scale.is_finite() || max_scale < 1.0 {
            return Err("max_scale must be finite and at least 1".to_string());
        }
        if !lag.is_finite() || lag < 0.0 {
            return Err("lag must be finite and non-negative".to_string());
        }
        if !self.inner.anim_type.is_empty_properties() {
            return Err("broadcast() cannot be combined with property targets or another effect in one Anim; combine separate animations with parallel()".to_string());
        }
        let state = self
            .owner
            .clone()
            .ok_or_else(|| "broadcast() needs a drawable of a scene".to_string())?;
        let spec = self
            .property_spec
            .clone()
            .ok_or_else(|| "broadcast() needs a drawable, not a text selection".to_string())?;
        let original = spec.lock().expect("object spec poisoned").clone();
        let ghosts: Vec<_> = (0..count)
            .map(|_| clone_spec_in(&state, &original).id)
            .collect();
        let active = state.lock().expect("canvas state poisoned").active_idx;
        Ok(Anim::queued(
            ghosts[0],
            AnimationType::Broadcast {
                source: self.inner.target,
                ghosts,
                max_scale,
                lag,
            },
            state,
            active,
        ))
    }
}

impl SceneModel {
    /// Dim everything outside `targets`: an overlay with a hole around them (a
    /// frame `padding` away, rounded by `corner_radius`) fades in to `dim`
    /// opacity and, by the default rate function, back out. The hole follows
    /// the targets if they move.
    pub fn spotlight(
        &mut self,
        targets: Vec<BoundsTarget>,
        dim: f64,
        padding: [f64; 4],
        corner_radius: f64,
    ) -> Result<Anim, String> {
        if !dim.is_finite() || dim <= 0.0 || dim > 1.0 {
            return Err("dim must be in (0, 1]".to_string());
        }
        check_padding(padding)?;
        if !corner_radius.is_finite() || corner_radius < 0.0 {
            return Err("corner_radius must be finite and non-negative".to_string());
        }
        // Bigger than the frame, so a moving camera still sees it whole.
        let cover = self
            .rect(self.frame.width * 2.0, self.frame.height * 2.0)
            .fill(Color::BLACK)
            .no_stroke();
        let hole = self
            .surrounding_rect(targets, padding, corner_radius)
            .map_err(|error| error.to_string())?
            .drawable
            .no_fill()
            .no_stroke();
        let overlay = self
            .boolean(
                &[&cover, &hole],
                super::BooleanOperation::Difference,
                true,
                0.0025,
                BooleanRule::NonZero,
            )
            .map_err(|error| error.to_string())?
            .z_index(10_000);
        // The overlay copied the cover's look; the operands only shape it.
        cover.opacity(0.0);
        hole.opacity(0.0);
        let state = self.state.clone();
        let active = state.lock().expect("canvas state poisoned").active_idx;
        Ok(Anim::queued(
            overlay.id,
            AnimationType::Spotlight { dim },
            state,
            active,
        ))
    }
}

impl DrawableHandle {
    /// A hand-drawn `notation` (rough-notation style) around `target`, this
    /// drawable or one of its text selections, grown by `padding` (top,
    /// right, bottom, left). The mark follows the target's bounds every
    /// frame and stays hidden until a `play` includes it, so
    /// `mark.create()` draws it on, one pass after another. Without a color
    /// the theme's stroke applies; a width needs a color.
    pub fn rough_notation(
        &self,
        target: BoundsTarget,
        notation: gaanim_math::RoughNotation,
        padding: [f64; 4],
        color: Option<Color>,
        width: Option<f64>,
    ) -> Result<DrawableHandle, String> {
        let owner = match &target {
            BoundsTarget::Drawable(id) => *id,
            BoundsTarget::TextSelection { target, .. } => *target,
        };
        if owner != self.id {
            return Err(
                "a notation's target must be this drawable or one of its selections".to_string(),
            );
        }
        check_padding(padding)?;
        check_stroke(color, width)?;
        if !notation.roughness.is_finite() || notation.roughness < 0.0 {
            return Err("roughness must be finite and non-negative".to_string());
        }
        if !(1..=MAX_NOTATION_PASSES).contains(&notation.passes) {
            return Err(format!(
                "passes must be between 1 and {MAX_NOTATION_PASSES}"
            ));
        }
        if let gaanim_math::NotationShape::Bracket(sides) = notation.shape
            && !(sides.left || sides.right || sides.top || sides.bottom)
        {
            return Err("a bracket needs at least one side".to_string());
        }
        let mark = spawn_in(&self.state, SpawnKind::SurroundingRect, true).no_fill();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachRoughNotation {
                target: mark.id,
                sources: vec![target],
                padding,
                notation,
            });
        mark.defer_visibility_until_play();
        stroked(mark, color, width)
    }

    /// A live frame around this drawable whose stroke cycles through `colors`,
    /// `cycle_rate` turns through the list per second from the timeline
    /// cursor on. The frame is a new drawable, visible right away: fade it in
    /// with its own animations if it should appear later.
    pub fn animated_boundary(
        &self,
        colors: Vec<Color>,
        cycle_rate: f64,
        width: f64,
        padding: [f64; 4],
        corner_radius: f64,
    ) -> Result<SurroundingRectHandle, String> {
        if colors.len() < 2 {
            return Err("an animated boundary needs at least two colors".to_string());
        }
        if !cycle_rate.is_finite() {
            return Err("cycle_rate must be finite".to_string());
        }
        check_positive("width", width)?;
        let frame = surrounding_rect_in(
            &self.state,
            vec![BoundsTarget::Drawable(self.id)],
            padding,
            corner_radius,
        )
        .map_err(|error| error.to_string())?;
        // Style setters change the spec the handles share.
        let _ = frame.drawable.clone().stroke(colors[0], width);
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachStrokeCycle {
                target: frame.drawable.id,
                colors,
                rate: cycle_rate,
            });
        Ok(frame)
    }
}
