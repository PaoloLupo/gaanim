//! Core types: coordinate system, mobject kinds, specs, and queued Anim.

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::peniko::{Brush, Color};
use gaanim_layout::{Anchor, Direction};
use gaanim_math::{Bounds3D, EasingCurve, RateFunc};

use crate::anim::{AnimationBuilder, AnimationType};
use crate::canvas::ops::{Op, SharedCanvasState};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoordinateSystem {
    Pixels,
    Scene { frame_width: f64, frame_height: f64 },
}

impl Default for CoordinateSystem {
    fn default() -> Self {
        Self::Pixels
    }
}

impl CoordinateSystem {
    pub fn frame_bounds(&self, canvas_width: u32, canvas_height: u32) -> Bounds3D {
        match self {
            Self::Pixels => {
                let half_width = canvas_width as f64 * 0.5;
                let half_height = canvas_height as f64 * 0.5;
                Bounds3D::new_2d(-half_width, -half_height, half_width, half_height)
            }
            Self::Scene {
                frame_width,
                frame_height,
            } => {
                let half_width = *frame_width * 0.5;
                let half_height = *frame_height * 0.5;
                Bounds3D::new_2d(-half_width, -half_height, half_width, half_height)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum SpawnKind {
    Circle(f64),
    Rect(f64, f64),
    RoundedRect(f64, f64, f64),
    Square(f64),
    Dot(f64),
    Ellipse(f64, f64),
    Line(f64, f64, f64, f64),
    Arrow(f64, f64, f64, f64),
    Text(String),
    Title(String),
    Subtitle(String),
    Equation(String),
    Group(Vec<ObjectId>),
}

#[derive(Debug, Clone)]
pub enum LayoutOp {
    SetTranslation(DVec3),
    MoveAnchorTo {
        target: DVec3,
        anchor: Anchor,
    },
    NextTo {
        reference: ObjectId,
        direction: Direction,
        spacing: f64,
        aligned_edge: Anchor,
    },
    AlignTo {
        reference: ObjectId,
        target_anchor: Anchor,
        reference_anchor: Anchor,
    },
    ToEdge {
        direction: Direction,
        buff: f64,
    },
    ToCorner {
        corner: Anchor,
        buff: f64,
    },
}

#[derive(Debug, Clone)]
pub struct ObjectSpec {
    pub id: ObjectId,
    pub kind: SpawnKind,
    pub fill: Option<Brush>,
    pub stroke: Option<(Color, f64)>,
    pub opacity: f32,
    pub z_index: i32,
    pub layout_ops: Vec<LayoutOp>,
}

impl ObjectSpec {
    pub(crate) fn new(id: ObjectId, kind: SpawnKind) -> Self {
        Self {
            id,
            kind,
            fill: None,
            stroke: None,
            opacity: 1.0,
            z_index: 0,
            layout_ops: Vec::new(),
        }
    }
}

/// A queued Canvas animation.
///
/// Methods like `DrawableHandle::fade_in()` create one of these and immediately
/// append an active `Op::Animate` to the owning segment. Fluent configuration
/// methods update both this value and the queued op, so `obj.fade_in(2.0)` or
/// keeps the deferred timeline consistent.
#[derive(Debug, Clone)]
pub struct Anim {
    pub inner: AnimationBuilder,
    queued: Option<QueuedAnim>,
}

#[derive(Debug, Clone)]
struct QueuedAnim {
    state: SharedCanvasState,
    segment_idx: usize,
    op_idx: usize,
}

impl Anim {
    pub(crate) fn new(target: ObjectId, anim_type: AnimationType) -> Self {
        Self {
            inner: AnimationBuilder {
                target,
                rate_func: anim_type.default_rate_func(),
                anim_type,
                duration: 1.0,
                delay: 0.0,
            },
            queued: None,
        }
    }

    pub(crate) fn queued(
        target: ObjectId,
        anim_type: AnimationType,
        state: SharedCanvasState,
        segment_idx: usize,
    ) -> Self {
        let mut anim = Self::new(target, anim_type);
        let mut guard = state.lock().expect("canvas state poisoned");
        let segment = &mut guard.segments[segment_idx];
        let op_idx = segment.ops.len();
        // Only advance cursor by duration; delay is per-animation and does not
        // occupy segment time (it shifts the clip start during compilation).
        segment.cursor += anim.inner.duration;
        segment.ops.push(Op::Animate {
            anim: anim.inner.clone(),
            active: true,
        });
        drop(guard);
        anim.queued = Some(QueuedAnim {
            state,
            segment_idx,
            op_idx,
        });
        anim
    }

    pub fn into_builder(self) -> AnimationBuilder {
        self.inner
    }

    /// Set the duration if `sec` is `Some`, otherwise leave the default.
    /// Used internally by animation methods that accept an optional duration
    /// parameter (e.g. `obj.fade_in(2.0)`).
    pub(crate) fn with_duration(mut self, sec: Option<f64>) -> Self {
        if let Some(sec) = sec {
            let old = self.inner.duration;
            self.inner.duration = sec.max(0.0);
            self.sync_queued(Some(old));
        }
        self
    }

    pub(crate) fn deactivate_auto_queue(&self) -> bool {
        let Some(queue) = &self.queued else {
            return false;
        };
        let mut guard = queue.state.lock().expect("canvas state poisoned");
        let Some(segment) = guard.segments.get_mut(queue.segment_idx) else {
            return false;
        };
        let Some(Op::Animate { anim, active }) = segment.ops.get_mut(queue.op_idx) else {
            return false;
        };
        if !*active {
            return false;
        }
        *active = false;
        segment.cursor -= anim.duration;
        true
    }

    fn sync_queued(&self, old_duration: Option<f64>) {
        let Some(queue) = &self.queued else {
            return;
        };
        let mut guard = queue.state.lock().expect("canvas state poisoned");
        let Some(segment) = guard.segments.get_mut(queue.segment_idx) else {
            return;
        };
        let Some(Op::Animate { anim, active }) = segment.ops.get_mut(queue.op_idx) else {
            return;
        };
        if let Some(old_duration) = old_duration {
            if *active {
                segment.cursor += self.inner.duration - old_duration;
            }
        }
        *anim = self.inner.clone();
    }

    pub fn duration(mut self, sec: f64) -> Self {
        let old = self.inner.duration;
        self.inner.duration = sec.max(0.0);
        self.sync_queued(Some(old));
        self
    }

    pub fn rate_func(mut self, f: RateFunc) -> Self {
        self.inner.rate_func = f;
        self.sync_queued(None);
        self
    }

    pub fn spring(self) -> Self {
        self.rate_func(RateFunc::Spring {
            stiffness: 90.0,
            damping: 12.0,
        })
    }

    pub fn smooth(self) -> Self {
        self.rate_func(RateFunc::Smooth)
    }

    pub fn linear(self) -> Self {
        self.rate_func(RateFunc::Linear)
    }

    pub fn lag_ratio(mut self, lag_ratio: f64) -> Self {
        self.inner = self.inner.lag_ratio(lag_ratio);
        self.sync_queued(None);
        self
    }

    pub fn stroke_width(mut self, stroke_width: f64) -> Self {
        self.inner = self.inner.stroke_width(stroke_width);
        self.sync_queued(None);
        self
    }

    pub fn with_pen_tip(mut self) -> Self {
        self.inner = self.inner.with_pen_tip();
        self.sync_queued(None);
        self
    }

    pub fn delay(mut self, sec: f64) -> Self {
        let delay = sec.max(0.0);
        self.inner.delay = delay;
        // Sync the delay into the queued Op so the compiler sees it.
        // Do NOT adjust segment.cursor: delay is a per-animation offset,
        // not segment time. The cursor only tracks cumulative duration.
        if let Some(queue) = &self.queued {
            let mut guard = queue.state.lock().expect("canvas state poisoned");
            if let Some(segment) = guard.segments.get_mut(queue.segment_idx) {
                if let Some(Op::Animate { anim, .. }) = segment.ops.get_mut(queue.op_idx) {
                    anim.delay = delay;
                }
            }
        }
        self
    }

    pub fn steps(self, n: u32) -> Self {
        self.rate_func(RateFunc::Steps(n.max(1)))
    }

    pub fn rate(self, name: &str) -> Self {
        self.ease(name)
    }

    pub fn ease(self, name: &str) -> Self {
        let rate_func = match name {
            "linear" => RateFunc::Linear,
            "smooth" | "ease" => RateFunc::Smooth,
            "ease_in" | "ease_in_quad" => RateFunc::EaseIn(EasingCurve::Quadratic),
            "ease_out" | "ease_out_quad" => RateFunc::EaseOut(EasingCurve::Quadratic),
            "ease_in_out" => RateFunc::EaseInOut(EasingCurve::Quadratic),
            "ease_in_cubic" => RateFunc::EaseIn(EasingCurve::Cubic),
            "ease_out_cubic" => RateFunc::EaseOut(EasingCurve::Cubic),
            "bounce" | "ease_out_bounce" => RateFunc::EaseOut(EasingCurve::Bounce),
            "elastic" | "ease_out_elastic" => RateFunc::EaseOut(EasingCurve::Elastic),
            "spring" => RateFunc::Spring {
                stiffness: 300.0,
                damping: 20.0,
            },
            "back" | "ease_out_back" => RateFunc::EaseOut(EasingCurve::Back),
            "there_and_back" => RateFunc::ThereAndBack,
            "running_start" => RateFunc::RunningStart,
            "exponential_decay" => RateFunc::ExponentialDecay,
            _ => RateFunc::Smooth,
        };
        self.rate_func(rate_func)
    }
}

impl From<Anim> for AnimationBuilder {
    fn from(anim: Anim) -> Self {
        anim.inner
    }
}

// ---------------------------------------------------------------------------
// OptDuration — allow animation methods to accept an optional duration
// ---------------------------------------------------------------------------

/// Trait that converts `()`, `f64`, or `Option<f64>` into an optional duration.
///
/// This lets animation methods accept all three forms:
/// - `obj.fade_in()`          — uses the default duration (1.0s)
/// - `obj.fade_in(2.0)`       — uses 2.0s
/// - `obj.fade_in(None)`      — uses the default duration (explicit)
pub trait OptDuration {
    fn into_opt(self) -> Option<f64>;
}

impl OptDuration for () {
    fn into_opt(self) -> Option<f64> {
        None
    }
}

impl OptDuration for f64 {
    fn into_opt(self) -> Option<f64> {
        Some(self)
    }
}

impl OptDuration for Option<f64> {
    fn into_opt(self) -> Option<f64> {
        self
    }
}
