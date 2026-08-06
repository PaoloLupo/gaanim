//! DrawableHandle — ergonomic mobject handle with fluent configuration.

use std::collections::HashMap;
use std::sync::Arc;

use gaanim_animation::AxisMask;
use gaanim_core::ObjectId;
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_core::kurbo::BezPath;
use gaanim_core::peniko::{Brush, Color};
use gaanim_layout::{Anchor, Direction};
use gaanim_math::RateFunc;

use crate::anim::{AnimationBuilder, AnimationType, DrawAnimationConfig};
use crate::canvas::ops::{
    FragmentRevealStyle, Op, SharedCanvasState, SharedObjectSpec, UpdaterPreset,
};
use crate::canvas::types::{Anim, LayoutOp, ObjectSpec, OptDuration, SpawnKind};

/// An ergonomic handle to a mobject on a Canvas.
///
/// - Instant setters return `Self` (fluent): `obj.fill(RED).at(0,0)`.
/// - Animation methods return `Anim` and auto-enqueue the animation on the
///   active segment's sequential track. They accept an optional duration:
///     - `obj.fade_in()`       — default 1.0s
///     - `obj.fade_in(2.0)`    — 2.0s
///     - `obj.fade_in(None)`   — default 1.0s (explicit)
///   You can still chain `.duration()` or any other `Anim` method after.
#[derive(Debug, Clone)]
pub struct DrawableHandle {
    pub id: ObjectId,
    pub(crate) spec: SharedObjectSpec,
    pub(crate) state: SharedCanvasState,
    pub(crate) segment_idx: usize,
    svg_parts: Option<Arc<HashMap<String, DrawableHandle>>>,
    style_targets: Arc<Vec<SharedObjectSpec>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SvgPartError {
    #[error("this drawable has no named SVG parts")]
    NotSvg,
    #[error("unknown SVG part '{id}'; available ids: {available}")]
    Unknown { id: String, available: String },
}

/// A deferred glyph selection inside a text-like [`DrawableHandle`].
#[derive(Debug, Clone)]
pub struct FragmentSelection {
    pub(crate) target: ObjectId,
    pub(crate) fragment: String,
    pub(crate) occurrence: Option<usize>,
    state: SharedCanvasState,
    segment_idx: usize,
}

/// Split ordinary mathematical source into display terms when an equation has
/// not declared semantic tags. Operators stand alone; adjacent non-operator
/// characters (for example `c^2` or `2x`) remain one term.
fn math_source_terms(source: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in source.chars() {
        if ch.is_whitespace() {
            if !current.is_empty() {
                terms.push(std::mem::take(&mut current));
            }
        } else if matches!(ch, '=' | '+' | '-' | '*' | '/' | '(' | ')' | '[' | ']') {
            if !current.is_empty() {
                terms.push(std::mem::take(&mut current));
            }
            terms.push(ch.to_string());
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    terms
}

impl DrawableHandle {
    pub(crate) fn new(
        id: ObjectId,
        kind: SpawnKind,
        state: SharedCanvasState,
        segment_idx: usize,
    ) -> Self {
        Self {
            id,
            spec: std::sync::Arc::new(std::sync::Mutex::new(ObjectSpec::new(id, kind))),
            state,
            segment_idx,
            svg_parts: None,
            style_targets: Arc::new(Vec::new()),
        }
    }

    fn update_spec(&self, f: impl FnOnce(&mut ObjectSpec)) -> Self {
        f(&mut self.spec.lock().expect("object spec poisoned"));
        self.clone()
    }

    fn update_style(&self, f: impl Fn(&mut ObjectSpec)) -> Self {
        f(&mut self.spec.lock().expect("object spec poisoned"));
        for target in self.style_targets.iter() {
            f(&mut target.lock().expect("SVG part spec poisoned"));
        }
        self.clone()
    }

    pub(crate) fn with_style_targets(mut self, targets: Vec<SharedObjectSpec>) -> Self {
        self.style_targets = Arc::new(targets);
        self
    }

    pub(crate) fn with_svg_parts(mut self, parts: HashMap<String, DrawableHandle>) -> Self {
        self.svg_parts = Some(Arc::new(parts));
        self
    }

    /// If this drawable is an axes, return its x/y ranges and full config.
    pub fn axes_info(&self) -> Option<((f64, f64, f64), (f64, f64, f64), crate::canvas::AxesConfig)> {
        let spec = self.spec.lock().ok()?;
        if let crate::canvas::SpawnKind::Axes {
            x_range,
            y_range,
            config,
        } = &spec.kind
        {
            Some((*x_range, *y_range, config.clone()))
        } else {
            None
        }
    }

    /// Resolve a named source group or path from an imported SVG.
    pub fn part(&self, id: &str) -> Result<DrawableHandle, SvgPartError> {
        let Some(parts) = &self.svg_parts else {
            return Err(SvgPartError::NotSvg);
        };
        parts.get(id).cloned().ok_or_else(|| {
            let mut available = parts.keys().cloned().collect::<Vec<_>>();
            available.sort();
            SvgPartError::Unknown {
                id: id.to_owned(),
                available: available.join(", "),
            }
        })
    }

    fn push_layout(&self, op: LayoutOp) -> Self {
        self.update_spec(|spec| spec.layout_ops.push(op))
    }

    // -- Instant setters (return Self) --

    pub fn fill(self, color: Color) -> Self {
        self.fill_brush(Brush::Solid(color))
    }

    pub fn fill_brush(self, brush: Brush) -> Self {
        self.update_style(|spec| {
            spec.fill = Some(brush.clone());
            spec.fill_overridden = true;
        })
    }

    pub fn no_fill(self) -> Self {
        self.update_style(|spec| {
            spec.fill = None;
            spec.fill_overridden = true;
        })
    }

    pub fn stroke(self, color: Color, width: f64) -> Self {
        self.stroke_brush(Brush::Solid(color), width)
    }

    pub fn stroke_brush(self, brush: Brush, width: f64) -> Self {
        self.update_style(|spec| {
            spec.stroke = Some((brush.clone(), width));
            spec.stroke_overridden = true;
        })
    }

    pub fn no_stroke(self) -> Self {
        self.update_style(|spec| {
            spec.stroke = None;
            spec.stroke_overridden = true;
        })
    }

    /// Add a soft outer glow. The effect is compiled into the retained vector fragment.
    pub fn glow(self, color: Color, radius: f64, intensity: f32) -> Self {
        self.update_style(|spec| {
            spec.glow = Some(gaanim_renderer::effects::Glow {
                color,
                radius,
                intensity,
            });
        })
    }

    /// Apply a soft vector blur to this drawable.
    pub fn blur(self, sigma: f64) -> Self {
        self.update_style(|spec| {
            spec.blur = Some(gaanim_renderer::effects::GaussianBlur { sigma });
        })
    }

    /// Add a soft shadow behind this drawable.
    pub fn shadow(self, color: Color, offset: DVec2, blur_radius: f64) -> Self {
        self.update_style(|spec| {
            spec.shadow = Some(gaanim_renderer::effects::DropShadow {
                color,
                offset,
                blur_radius,
            });
        })
    }

    /// Remove all visual effects while preserving fill and stroke styling.
    pub fn no_effects(self) -> Self {
        self.update_style(|spec| {
            spec.glow = None;
            spec.blur = None;
            spec.shadow = None;
        })
    }

    /// Clip this drawable to another drawable's vector geometry.
    ///
    /// The mask keeps its own visibility, so call `mask.no_fill().no_stroke()`
    /// when it should act only as clipping geometry.
    pub fn clip(self, mask: &DrawableHandle, rule: gaanim_core::peniko::Fill) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SetClip {
                target: self.id,
                mask: Some(mask.id),
                rule,
            });
        self
    }

    /// Remove a previously configured clipping mask.
    pub fn no_clip(self) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SetClip {
                target: self.id,
                mask: None,
                rule: gaanim_core::peniko::Fill::NonZero,
            });
        self
    }

    /// Colors every matching text or equation fragment in this drawable.
    ///
    /// Matching is case-insensitive and tolerant of math formatting, so a
    /// query such as `"mc"` can target `m c` in a Typst equation. Later calls
    /// take precedence when selections overlap.
    pub fn color_by(self, fragment: impl Into<String>, color: Color) -> Self {
        let fragment = fragment.into();
        self.update_spec(|spec| {
            if !fragment.is_empty() {
                spec.fragment_fills.push((fragment, color));
            }
        })
    }

    /// Creates a deferred selection for glyph-level styling and emphasis.
    pub fn select(&self, fragment: impl Into<String>) -> FragmentSelection {
        FragmentSelection {
            target: self.id,
            fragment: fragment.into(),
            occurrence: None,
            state: self.state.clone(),
            segment_idx: self.segment_idx,
        }
    }

    /// Creates a deferred selection for one zero-based occurrence of a fragment.
    pub fn select_nth(&self, fragment: impl Into<String>, occurrence: usize) -> FragmentSelection {
        FragmentSelection {
            target: self.id,
            fragment: fragment.into(),
            occurrence: Some(occurrence),
            state: self.state.clone(),
            segment_idx: self.segment_idx,
        }
    }

    /// Registers a semantic name for a text or equation fragment.
    pub fn define_tag(
        self,
        name: impl Into<String>,
        fragment: impl Into<String>,
        occurrence: Option<usize>,
    ) -> Self {
        let name = name.into();
        let fragment = fragment.into();
        self.update_spec(|spec| {
            if !name.trim().is_empty() && !fragment.trim().is_empty() {
                spec.fragment_tags.push((name, fragment, occurrence));
            }
        })
    }

    /// Selects a previously registered semantic tag.
    pub fn tag(&self, name: &str) -> Option<FragmentSelection> {
        let (fragment, occurrence) = self
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .iter()
            .rev()
            .find(|(tag, _, _)| tag == name)
            .map(|(_, fragment, occurrence)| (fragment.clone(), *occurrence))?;
        Some(FragmentSelection {
            target: self.id,
            fragment,
            occurrence,
            state: self.state.clone(),
            segment_idx: self.segment_idx,
        })
    }

    /// Writes semantic terms in tag order instead of staggering individual
    /// glyphs. If `tags` is omitted, all declared tags are used in declaration
    /// order.
    pub fn write_by_terms(&self, tags: Option<Vec<String>>, duration: f64) -> Self {
        if !duration.is_finite() || duration <= 0.0 {
            return self.clone();
        }
        let spec = self.spec.lock().expect("object spec poisoned").clone();
        let declared = spec.fragment_tags;
        let terms: Vec<(String, Option<usize>)> = match tags {
            Some(names) => names
                .into_iter()
                .filter_map(|name| {
                    declared
                        .iter()
                        .rev()
                        .find(|(tag, _, _)| tag == &name)
                        .map(|(_, fragment, occurrence)| (fragment.clone(), *occurrence))
                })
                .collect(),
            None if !declared.is_empty() => declared
                .into_iter()
                .map(|(_, fragment, occurrence)| (fragment, occurrence))
                .collect(),
            None => match spec.kind {
                SpawnKind::Equation(source) | SpawnKind::Text(source) => math_source_terms(&source)
                    .into_iter()
                    .map(|fragment| (fragment, None))
                    .collect(),
                _ => Vec::new(),
            },
        };
        if !terms.is_empty() {
            self.state.lock().expect("canvas state poisoned").segments[self.segment_idx]
                .ops
                .push(Op::WriteTerms {
                    target: self.id,
                    terms,
                    duration,
                });
        }
        self.clone()
    }

    /// Sets the initial value of a `ValueTracker` before the scene is compiled.
    /// Calling this on a regular drawable has no effect.
    pub fn set_value(self, value: f64) -> Self {
        self.update_spec(|spec| {
            if let SpawnKind::ValueTracker(current) = &mut spec.kind {
                *current = value;
            }
        })
    }

    pub fn opacity(self, op: f32) -> Self {
        self.update_spec(|spec| spec.opacity = op)
    }

    pub fn z_index(self, z: i32) -> Self {
        self.update_spec(|spec| spec.z_index = z)
    }

    pub fn at(self, x: f64, y: f64) -> Self {
        self.push_layout(LayoutOp::SetTranslation(DVec3::new(x, y, 0.0)))
    }

    pub fn scaled(self, factor: f64) -> Self {
        self.push_layout(LayoutOp::SetScale(factor))
    }

    pub fn rotated(self, radians: f64) -> Self {
        self.push_layout(LayoutOp::SetRotation(radians))
    }

    /// Set the scene-space pivot used by rotations and uniform scaling.
    ///
    /// This is the natural way to rotate a mechanism around a known hinge or
    /// disk center. The engine converts the point to the group's local anchor
    /// after all initial layout has been resolved.
    pub fn with_pivot(self, x: f64, y: f64) -> Self {
        self.push_layout(LayoutOp::SetPivot(DVec3::new(x, y, 0.0)))
    }

    /// Alias for [`Self::with_pivot`].
    pub fn pivot(self, x: f64, y: f64) -> Self {
        self.with_pivot(x, y)
    }

    pub fn at_anchor(self, x: f64, y: f64, anchor: Anchor) -> Self {
        self.push_layout(LayoutOp::MoveAnchorTo {
            target: DVec3::new(x, y, 0.0),
            anchor,
        })
    }

    pub fn next_to(self, reference: &DrawableHandle, direction: Direction, spacing: f64) -> Self {
        self.next_to_aligned(reference, direction, spacing, Anchor::Center)
    }

    pub fn next_to_aligned(
        self,
        reference: &DrawableHandle,
        direction: Direction,
        spacing: f64,
        aligned_edge: Anchor,
    ) -> Self {
        self.push_layout(LayoutOp::NextTo {
            reference: reference.id,
            direction,
            spacing,
            aligned_edge,
        })
    }

    pub fn align_to(
        self,
        reference: &DrawableHandle,
        target_anchor: Anchor,
        reference_anchor: Anchor,
    ) -> Self {
        self.push_layout(LayoutOp::AlignTo {
            reference: reference.id,
            target_anchor,
            reference_anchor,
        })
    }

    pub fn to_edge(self, direction: Direction, buff: f64) -> Self {
        self.push_layout(LayoutOp::ToEdge { direction, buff })
    }

    pub fn to_corner(self, corner: Anchor, buff: f64) -> Self {
        self.push_layout(LayoutOp::ToCorner { corner, buff })
    }

    /// Arranges this group's direct children along a direction.
    pub fn arrange(self, direction: Direction, spacing: f64, aligned_edge: Anchor) -> Self {
        self.push_layout(LayoutOp::Arrange {
            direction,
            spacing: spacing.max(0.0),
            aligned_edge,
        })
    }

    /// Stacks this group's direct children from top to bottom.
    pub fn vstack(self, spacing: f64, aligned_edge: Anchor) -> Self {
        self.arrange(Direction::Down, spacing, aligned_edge)
    }

    /// Stacks this group's direct children from left to right.
    pub fn hstack(self, spacing: f64, aligned_edge: Anchor) -> Self {
        self.arrange(Direction::Right, spacing, aligned_edge)
    }

    // -- Internal helpers --

    fn anim(&self, ty: AnimationType) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(self.id, ty, self.state.clone(), active_idx)
    }

    fn anim_dur(&self, ty: AnimationType, dur: Option<f64>) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(self.id, ty, self.state.clone(), active_idx).with_duration(dur)
    }

    /// Animates a `ValueTracker` to `to`. Regular drawables ignore this lens.
    pub fn animate_value_to(&self, to: f64) -> Anim {
        self.anim(AnimationType::SignalFloat { to })
    }

    // -- Animation methods (return Anim, auto-enqueued) --
    //
    // Every method accepts an optional duration via `impl OptDuration`:
    //   obj.fade_in()        — default 1.0s
    //   obj.fade_in(2.0)     — 2.0s
    //   obj.fade_in(None)    — default 1.0s

    pub fn r#move(&self, dx: f64, dy: f64) -> Anim {
        self.anim(AnimationType::TranslateBy {
            delta: DVec3::new(dx, dy, 0.0),
        })
    }

    pub fn move_to(&self, x: f64, y: f64) -> Anim {
        self.anim(AnimationType::TranslateTo {
            to: DVec3::new(x, y, 0.0),
        })
    }

    pub fn glide_to(&self, x: f64, y: f64) -> Anim {
        self.move_to(x, y)
    }

    pub fn scale(&self, factor: f64) -> Anim {
        self.anim(AnimationType::ScaleUniform { factor })
    }

    pub fn rotate(&self, rad: f64) -> Anim {
        self.anim(AnimationType::RotateBy { angle_radians: rad })
    }

    pub fn fade_in(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::FadeIn, dur.into_opt())
    }

    /// Appears while moving from `direction` toward its final position.
    pub fn fade_in_from(&self, direction: Direction, distance: f64, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::FadeInFrom {
                offset: direction.to_vector() * distance.max(0.0),
            },
            dur.into_opt(),
        )
    }

    pub fn fade_out(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::FadeOut, dur.into_opt())
    }

    pub fn fade_to(&self, alpha: f32) -> Anim {
        self.anim(AnimationType::FadeTo { to: alpha })
    }

    pub fn write(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Write {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn create(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Create {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn unwrite(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Unwrite {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn uncreate(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Uncreate {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn grow_from_center(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::GrowFromCenter, dur.into_opt())
    }

    pub fn shrink_to_center(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::ShrinkToCenter, dur.into_opt())
    }

    pub fn spin_in_from_nothing(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::SpinInFromNothing, dur.into_opt())
    }

    pub fn grow_from_point(&self, px: f64, py: f64) -> Anim {
        self.anim(AnimationType::GrowFromPoint { px, py })
    }

    pub fn grow_from_edge(&self, dir: &str) -> Anim {
        self.anim(AnimationType::GrowFromEdge {
            direction: dir.to_string(),
        })
    }

    pub fn draw_border_then_fill(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::DrawBorderThenFill {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub fn indicate(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Indicate {
                color: None,
                scale_factor: 1.3,
            },
            dur.into_opt(),
        )
    }

    pub fn circumscribe(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::Circumscribe { color: None }, dur.into_opt())
    }

    pub fn flash(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Flash {
                color: None,
                n_lines: 16,
                radius: 100.0,
            },
            dur.into_opt(),
        )
    }

    pub fn wiggle(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::Wiggle, dur.into_opt())
    }

    pub fn move_along_path(&self, path: BezPath) -> Anim {
        self.anim(AnimationType::MoveAlongPath { path })
    }

    pub fn fade_transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::FadeTransform { target: target.id })
    }

    pub fn transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::Transform { target: target.id })
    }

    pub fn replacement_transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::ReplacementTransform { target: target.id })
    }

    // -- Reactive methods --

    /// Attach a preset updater that runs every frame.
    ///
    /// Use `UpdaterPreset` variants: `Orbit`, `AdvanceX`, `Bob`, `Rotate`, `Pulse`.
    pub fn add_updater(&self, preset: UpdaterPreset) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachUpdater {
                target: self.id,
                preset,
            });
    }

    /// Remove any updater attached to this entity.
    pub fn remove_updater(&self) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::RemoveUpdater(self.id));
    }

    /// Copy the source entity's Y position each frame (after updaters run).
    pub fn bind_y_from(&self, source: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPositionBinding {
                target: self.id,
                source: source.id,
                axes: AxisMask::Y,
            });
    }

    /// Copy the source entity's X position each frame (after updaters run).
    pub fn bind_x_from(&self, source: &DrawableHandle) {
        self.bind_position_from(source, AxisMask::X);
    }

    /// Keep this drawable centered on `source` each frame.
    ///
    /// This is an exact XY position binding. It is useful for labels, markers,
    /// and accents that should travel with an independently animated object.
    pub fn attach_to(&self, source: &DrawableHandle) {
        self.bind_position_from(source, AxisMask::XY);
    }

    /// Follow `source` while preserving a scene-space `(x, y)` offset.
    pub fn follow_to(&self, source: &DrawableHandle, offset_x: f64, offset_y: f64) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPositionFollow {
                target: self.id,
                source: source.id,
                offset: DVec3::new(offset_x, offset_y, 0.0),
            });
    }

    /// Copy the source entity's position on specified axes each frame.
    pub fn bind_position_from(&self, source: &DrawableHandle, axes: AxisMask) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPositionBinding {
                target: self.id,
                source: source.id,
                axes,
            });
    }

    /// Start the legacy `.animate()` compound builder. This intentionally does
    /// not auto-enqueue because the old builder is incomplete until a concrete
    /// animation kind is selected.
    pub fn animate(&self) -> AnimationBuilder {
        AnimationBuilder {
            target: self.id,
            anim_type: AnimationType::TranslateBy { delta: DVec3::ZERO },
            duration: 1.0,
            rate_func: RateFunc::Smooth,
            delay: 0.0,
        }
    }
}

impl FragmentSelection {
    fn push(&self, op: Op) {
        self.state.lock().expect("canvas state poisoned").segments[self.segment_idx]
            .ops
            .push(op);
    }

    /// Instantly colors this selected fragment.
    pub fn fill(self, color: Color) -> Self {
        if !self.fragment.trim().is_empty() {
            self.push(Op::FragmentFill {
                target: self.target,
                fragment: self.fragment.clone(),
                occurrence: self.occurrence,
                color,
            });
        }
        self
    }

    /// Emphasizes this fragment without affecting the surrounding text.
    pub fn indicate(self, duration: impl OptDuration) -> Self {
        if !self.fragment.trim().is_empty() {
            self.push(Op::FragmentIndicate {
                target: self.target,
                fragment: self.fragment.clone(),
                occurrence: self.occurrence,
                color: None,
                duration: duration.into_opt().unwrap_or(1.0),
            });
        }
        self
    }

    /// Reveals this fragment with `Fade`, `Wipe`, or `FromBelow`.
    pub fn reveal(self, style: FragmentRevealStyle, duration: impl OptDuration) -> Self {
        if !self.fragment.trim().is_empty() {
            self.push(Op::FragmentReveal {
                target: self.target,
                fragment: self.fragment.clone(),
                occurrence: self.occurrence,
                style,
                duration: duration.into_opt().unwrap_or(1.0),
            });
        }
        self
    }

    /// Strikes through this fragment and fades it from the equation.
    pub fn cancel(self, duration: impl OptDuration) -> Self {
        if !self.fragment.trim().is_empty() {
            self.push(Op::CancelFragment {
                target: self.target,
                fragment: self.fragment.clone(),
                occurrence: self.occurrence,
                duration: duration.into_opt().unwrap_or(0.6),
            });
        }
        self
    }

    /// Animates this fragment's fill to `color`.
    pub fn color_to(self, color: Color, duration: impl OptDuration) -> Self {
        if !self.fragment.trim().is_empty() {
            self.push(Op::FragmentFillTo {
                target: self.target,
                fragment: self.fragment.clone(),
                occurrence: self.occurrence,
                color,
                duration: duration.into_opt().unwrap_or(1.0),
            });
        }
        self
    }

    /// Morphs this selection into `target`, pairing matching glyphs in order.
    /// Unpaired glyphs remain unchanged; use equal-sized fragments for the
    /// clearest equation derivations.
    pub fn transform_to(self, target: &FragmentSelection, duration: impl OptDuration) -> Self {
        if !self.fragment.trim().is_empty() && !target.fragment.trim().is_empty() {
            self.push(Op::FragmentTransform {
                source: self.target,
                source_fragment: self.fragment.clone(),
                source_occurrence: self.occurrence,
                target: target.target,
                target_fragment: target.fragment.clone(),
                target_occurrence: target.occurrence,
                duration: duration.into_opt().unwrap_or(1.0),
            });
        }
        self
    }
}
