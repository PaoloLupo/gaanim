//! Core types: coordinate system, mobject kinds, specs, and queued Anim.

use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_core::peniko::{Brush, Color, ImageData};
use gaanim_layout::{Anchor, Direction};
use gaanim_math::{Bounds3D, EasingCurve, RateFunc};
use gaanim_objects::prelude::{ImageView, SvgDocument};

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

/// Per-side canvas margin (in the same unit as the coordinate system).
///
/// Layout operations like `to_edge` and `to_corner` respect these margins
/// as an automatic inset from the frame bounds, so individual `buff`
/// values stack on top of the margin.
#[derive(Debug, Clone, Copy)]
pub struct Margin {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Margin {
    pub const ZERO: Self = Self {
        left: 0.0,
        right: 0.0,
        top: 0.0,
        bottom: 0.0,
    };

    /// Uniform margin on all four sides.
    pub fn all(v: f64) -> Self {
        Self {
            left: v,
            right: v,
            top: v,
            bottom: v,
        }
    }

    /// Horizontal (left+right) and vertical (top+bottom) margins.
    pub fn hv(h: f64, v: f64) -> Self {
        Self {
            left: h,
            right: h,
            top: v,
            bottom: v,
        }
    }
}

impl Default for Margin {
    fn default() -> Self {
        Self::ZERO
    }
}

/// How an image should use a requested target size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageFit {
    /// Preserve aspect ratio inside the target rectangle.
    #[default]
    Contain,
    /// Preserve aspect ratio while filling the target rectangle, clipping excess pixels.
    Cover,
    /// Fill the target rectangle even when that distorts the source aspect ratio.
    Stretch,
}

/// A rectangle in source image pixel coordinates (top-left origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageCrop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Optional destination sizing and source crop for `Canvas::image_with_options`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImageOptions {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub fit: ImageFit,
    pub crop: Option<ImageCrop>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageOptionsError {
    #[error("image width and height must be finite positive values")]
    InvalidTargetSize,
    #[error("crop must be finite, positive in size, and contained within the source image")]
    InvalidCrop,
}

impl ImageOptions {
    /// Resolve options into the exact source-to-destination mapping used by the renderer.
    pub fn resolve(
        self,
        source_width: u32,
        source_height: u32,
    ) -> Result<ImageView, ImageOptionsError> {
        let source_width = f64::from(source_width);
        let source_height = f64::from(source_height);
        let crop = self.crop.unwrap_or(ImageCrop {
            x: 0.0,
            y: 0.0,
            width: source_width,
            height: source_height,
        });
        if !crop.x.is_finite()
            || !crop.y.is_finite()
            || !crop.width.is_finite()
            || !crop.height.is_finite()
            || crop.x < 0.0
            || crop.y < 0.0
            || crop.width <= 0.0
            || crop.height <= 0.0
            || crop.x + crop.width > source_width
            || crop.y + crop.height > source_height
        {
            return Err(ImageOptionsError::InvalidCrop);
        }

        for value in [self.width, self.height].into_iter().flatten() {
            if !value.is_finite() || value <= 0.0 {
                return Err(ImageOptionsError::InvalidTargetSize);
            }
        }

        let (display_width, display_height, scale_x, scale_y) = match (self.width, self.height) {
            (None, None) => (crop.width, crop.height, 1.0, 1.0),
            (Some(width), None) => {
                let scale = width / crop.width;
                (width, crop.height * scale, scale, scale)
            }
            (None, Some(height)) => {
                let scale = height / crop.height;
                (crop.width * scale, height, scale, scale)
            }
            (Some(width), Some(height)) => match self.fit {
                ImageFit::Contain => {
                    let scale = (width / crop.width).min(height / crop.height);
                    (crop.width * scale, crop.height * scale, scale, scale)
                }
                ImageFit::Cover => {
                    let scale = (width / crop.width).max(height / crop.height);
                    (width, height, scale, scale)
                }
                ImageFit::Stretch => (width, height, width / crop.width, height / crop.height),
            },
        };

        Ok(ImageView {
            source_x: crop.x,
            source_y: crop.y,
            source_width: crop.width,
            source_height: crop.height,
            display_width,
            display_height,
            scale_x,
            scale_y,
        })
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
    /// Circular arc centered at `(cx, cy)`, in radians.
    Arc {
        center: (f64, f64),
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
    /// Curved arrow connecting two points with an angular deflection.
    CurvedArrow(f64, f64, f64, f64, f64),
    /// Curved arrow following an explicit circular arc.
    CurvedArrowArc {
        center: (f64, f64),
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
    /// Technical measurement: extension lines plus a double-headed arrow.
    Dimension {
        start: (f64, f64),
        end: (f64, f64),
        offset: f64,
    },
    /// Open sequence of straight segments. Useful for springs, rails, and paths.
    Polyline(Vec<(f64, f64)>),
    Axes {
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        grid: bool,
        labels: bool,
    },
    Text(String),
    Title(String),
    Subtitle(String),
    Equation(String),
    /// Decoded RGBA texture plus its source and destination mapping.
    Image {
        image: ImageData,
        view: ImageView,
    },
    /// Resolved vector paths imported from an SVG document.
    Svg(SvgDocument),
    Group(Vec<ObjectId>),
    /// Invisible value tracker entity (FloatSignal). No visual output.
    ValueTracker(f64),
    /// Placeholder line entity whose Path2D will be overwritten by TracedPath.
    TracedPathLine,
    /// Placeholder line entity whose Path2D will be overwritten by TrackingLine.
    TrackingLine,
}

#[derive(Debug, Clone)]
pub enum LayoutOp {
    SetTranslation(DVec3),
    SetScale(f64),
    SetRotation(f64),
    /// Scene-space point around which rotation and scaling are performed.
    SetPivot(DVec3),
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
    pub fill_overridden: bool,
    pub stroke: Option<(Color, f64)>,
    pub stroke_overridden: bool,
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
            fill_overridden: false,
            stroke: None,
            stroke_overridden: false,
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

#[cfg(test)]
mod tests {
    use super::{ImageCrop, ImageFit, ImageOptions};

    #[test]
    fn image_fit_resolves_contain_cover_and_crop() {
        let contain = ImageOptions {
            width: Some(200.0),
            height: Some(100.0),
            fit: ImageFit::Contain,
            crop: None,
        }
        .resolve(400, 100)
        .unwrap();
        assert_eq!(
            (contain.display_width, contain.display_height),
            (200.0, 50.0)
        );
        assert_eq!((contain.scale_x, contain.scale_y), (0.5, 0.5));

        let cover = ImageOptions {
            width: Some(200.0),
            height: Some(100.0),
            fit: ImageFit::Cover,
            crop: None,
        }
        .resolve(400, 100)
        .unwrap();
        assert_eq!((cover.display_width, cover.display_height), (200.0, 100.0));
        assert_eq!((cover.scale_x, cover.scale_y), (1.0, 1.0));

        let crop = ImageOptions {
            width: None,
            height: None,
            fit: ImageFit::Contain,
            crop: Some(ImageCrop {
                x: 50.0,
                y: 20.0,
                width: 120.0,
                height: 60.0,
            }),
        }
        .resolve(400, 100)
        .unwrap();
        assert_eq!((crop.source_x, crop.source_y), (50.0, 20.0));
        assert_eq!((crop.display_width, crop.display_height), (120.0, 60.0));
    }
}
