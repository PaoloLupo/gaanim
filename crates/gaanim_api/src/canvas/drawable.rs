//! DrawableHandle — ergonomic mobject handle with fluent configuration.

use gaanim_animation::AxisMask;
use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::BezPath;
use gaanim_core::peniko::{Brush, Color};
use gaanim_layout::{Anchor, Direction};
use gaanim_math::RateFunc;

use crate::anim::{AnimationBuilder, AnimationType, DrawAnimationConfig};
use crate::canvas::ops::{Op, SharedCanvasState, SharedObjectSpec, UpdaterPreset};
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
        }
    }

    fn update_spec(&self, f: impl FnOnce(&mut ObjectSpec)) -> Self {
        f(&mut self.spec.lock().expect("object spec poisoned"));
        self.clone()
    }

    fn push_layout(&self, op: LayoutOp) -> Self {
        self.update_spec(|spec| spec.layout_ops.push(op))
    }

    // -- Instant setters (return Self) --

    pub fn fill(self, color: Color) -> Self {
        self.update_spec(|spec| {
            spec.fill = Some(Brush::Solid(color));
            spec.fill_overridden = true;
        })
    }

    pub fn no_fill(self) -> Self {
        self.update_spec(|spec| {
            spec.fill = None;
            spec.fill_overridden = true;
        })
    }

    pub fn stroke(self, color: Color, width: f64) -> Self {
        self.update_spec(|spec| {
            spec.stroke = Some((color, width));
            spec.stroke_overridden = true;
        })
    }

    pub fn no_stroke(self) -> Self {
        self.update_spec(|spec| {
            spec.stroke = None;
            spec.stroke_overridden = true;
        })
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
