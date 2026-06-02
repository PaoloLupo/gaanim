use gaanim_api::anim::{AnimationBuilder, AnimationType};
use gaanim_api::prelude::MobjectRef;
use gaanim_core::glam::DVec3;
use gaanim_core::ObjectId;
use gaanim_math::{EasingCurve, RateFunc};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::color::PyColor;
use crate::id::PyObjectId;
use crate::mobject::PyMobject;

/// A configured animation that targets a single Mobject.
///
/// Built fluently from `Mobject.animate()` or any of the per-mobject animation
/// helpers (`shift`, `translate_to`, `scale`, `rotate`, `fade_in`, etc.). Each
/// chained call returns a fresh spec — the original is unchanged.
///
/// Pass to `Scene.play(*specs)` to enqueue the animation on the timeline.
#[pyclass(name = "AnimSpec", module = "gaanim_core", from_py_object)]
#[derive(Clone, Debug)]
pub struct PyAnimationSpec {
    pub inner: AnimationBuilder,
}

impl PyAnimationSpec {
    /// Default spec used by `Mobject.animate()`. The kind is a no-op `shift(0,0)`;
    /// the user is expected to chain a real kind before passing to `play()`.
    pub fn new(target: ObjectId) -> Self {
        Self {
            inner: AnimationBuilder {
                target,
                anim_type: AnimationType::TranslateBy { delta: DVec3::ZERO },
                duration: 1.0,
                rate_func: RateFunc::Smooth,
            },
        }
    }

    /// Internal constructor for `Mobject.shift(...)` style methods.
    pub fn from_kind(target: ObjectId, anim_type: AnimationType) -> Self {
        Self {
            inner: AnimationBuilder {
                target,
                anim_type,
                duration: 1.0,
                rate_func: RateFunc::Smooth,
            },
        }
    }

    /// Internal constructor that wraps a pre-built `AnimationBuilder`
    /// (used by the `Mobject.write(...)` shortcut to forward the duration
    /// set by `MobjectRef::write_with_stroke_width`).
    pub fn from_builder(builder: AnimationBuilder) -> Self {
        Self { inner: builder }
    }
}

#[pymethods]
impl PyAnimationSpec {
    // ====== property accessors ======

    #[getter]
    fn target(&self) -> PyObjectId {
        PyObjectId(self.inner.target)
    }

    #[getter]
    fn duration_val(&self) -> f64 {
        self.inner.duration
    }

    #[getter]
    fn rate_func_name(&self) -> String {
        rate_func_name(&self.inner.rate_func)
    }

    fn __repr__(&self) -> String {
        let kind = match &self.inner.anim_type {
            AnimationType::TranslateTo { to } => format!("translate_to({:?})", to),
            AnimationType::TranslateBy { delta } => format!("shift({:?})", delta),
            AnimationType::ScaleUniform { factor } => format!("scale({})", factor),
            AnimationType::ScaleTo { to } => format!("scale_to({:?})", to),
            AnimationType::RotateTo { to } => format!("rotate_to({:?})", to),
            AnimationType::RotateBy { angle_radians } => format!("rotate({} rad)", angle_radians),
            AnimationType::FadeTo { to } => format!("fade_to({})", to),
            AnimationType::FadeIn => "fade_in".to_string(),
            AnimationType::FadeOut => "fade_out".to_string(),
            AnimationType::FillColorTo { to } => format!("fill_color({:?})", to),
            AnimationType::StrokeColorTo { to } => format!("stroke_color({:?})", to),
            AnimationType::StrokeWidthTo { to } => format!("stroke_width({})", to),
            AnimationType::Write { stroke_width } => {
                format!("write(stroke_width={:?})", stroke_width)
            }
            AnimationType::Create { stroke_width } => {
                format!("create(stroke_width={:?})", stroke_width)
            }
            AnimationType::Uncreate { stroke_width } => {
                format!("uncreate(stroke_width={:?})", stroke_width)
            }
            AnimationType::Unwrite { stroke_width } => {
                format!("unwrite(stroke_width={:?})", stroke_width)
            }
            AnimationType::GrowFromCenter => "grow_from_center".to_string(),
            AnimationType::ShrinkToCenter => "shrink_to_center".to_string(),
            AnimationType::SpinInFromNothing => "spin_in_from_nothing".to_string(),
            AnimationType::Indicate {
                color,
                scale_factor,
            } => {
                format!("indicate(color={:?}, scale_factor={})", color, scale_factor)
            }
            AnimationType::FadeTransform { target } => {
                format!("fade_transform(target={:?})", target)
            }
            AnimationType::Wiggle => "wiggle".to_string(),
            AnimationType::GrowFromPoint { px, py } => {
                format!("grow_from_point({}, {})", px, py)
            }
            AnimationType::GrowFromEdge { direction } => {
                format!("grow_from_edge({})", direction)
            }
            AnimationType::DrawBorderThenFill => "draw_border_then_fill".to_string(),
            AnimationType::Flash {
                color,
                n_lines,
                radius,
            } => {
                format!("flash(color={:?}, n_lines={}, radius={})", color, n_lines, radius)
            }
            AnimationType::Circumscribe { color } => {
                format!("circumscribe(color={:?})", color)
            }
        };
        format!(
            "AnimSpec(target=ObjectId({}v{}), kind={}, duration={}, rate={})",
            self.inner.target.index(),
            self.inner.target.generation(),
            kind,
            self.inner.duration,
            rate_func_name(&self.inner.rate_func),
        )
    }

    // ====== mutating builders (each returns a new spec) ======

    fn duration(&self, d: f64) -> Self {
        Self {
            inner: self.inner.clone().duration(d),
        }
    }

    fn rate_func(&self, name: &str) -> PyResult<Self> {
        let rf = rate_func_from_name(name)?;
        Ok(Self {
            inner: self.inner.clone().rate_func(rf),
        })
    }

    fn spring(&self) -> Self {
        Self {
            inner: self.inner.clone().spring(),
        }
    }

    fn smooth(&self) -> Self {
        Self {
            inner: self.inner.clone().smooth(),
        }
    }

    fn linear(&self) -> Self {
        Self {
            inner: self.inner.clone().linear(),
        }
    }

    /// Discrete step interpolation. Clamps to `n` evenly-spaced levels.
    fn steps(&self, n: u32) -> Self {
        Self {
            inner: self
                .inner
                .clone()
                .rate_func(RateFunc::Steps(n)),
        }
    }

    /// CSS-style cubic-bezier easing curve.
    fn cubic_bezier(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            inner: self
                .inner
                .clone()
                .rate_func(RateFunc::CubicBezier(x1, y1, x2, y2)),
        }
    }

    /// Mirror a named rate function (go to peak then back symmetrically).
    fn mirror(&self, inner_name: &str) -> PyResult<Self> {
        let inner = rate_func_from_name(inner_name)?;
        Ok(Self {
            inner: self.inner.clone().rate_func(RateFunc::Mirror(Box::new(inner))),
        })
    }

    /// There-and-back with a pause at the peak.
    /// `pause_ratio` controls what fraction of duration is spent at peak (0.0–0.9).
    fn there_and_back_with_pause(&self, pause_ratio: f64) -> Self {
        Self {
            inner: self
                .inner
                .clone()
                .rate_func(RateFunc::ThereAndBackWithPause(pause_ratio)),
        }
    }

    /// Replace the underlying animation kind (chaining overrides the previous kind).
    fn shift(&self, dx: f64, dy: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.shift_2d(dx, dy),
        }
    }

    fn translate_to(&self, x: f64, y: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.translate_to_2d(x, y),
        }
    }

    fn scale(&self, factor: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.scale_uniform(factor),
        }
    }

    fn scale_to(&self, factor: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.scale_to(DVec3::splat(factor)),
        }
    }

    fn rotate(&self, radians: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.rotate_by(radians),
        }
    }

    fn rotate_to(&self, radians: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.rotate_to_2d(radians),
        }
    }

    fn fade_in(&self) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.fade_in(),
        }
    }

    fn fade_out(&self) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.fade_out(),
        }
    }

    fn fade_to(&self, opacity: f32) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.fade_to(opacity),
        }
    }

    fn fill_color(&self, color: &PyColor) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.fill_color_to(color.0),
        }
    }

    fn stroke_color(&self, color: &PyColor) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.stroke_color_to(color.0),
        }
    }

    fn stroke_width(&self, width: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.stroke_width_to(width),
        }
    }

    /// Manim-style **Write** animation. Replaces the underlying animation
    /// kind, so chaining order matters: `mobject.animate().write(1.5)`
    /// will write, but `mobject.animate().shift(50,0).write(1.5)` will
    /// *only* write (the shift is discarded). Combine with other animations
    /// via parallel `play()` calls if you need both.
    #[pyo3(signature = (duration=1.0, stroke_width=None))]
    fn write(&self, duration: f64, stroke_width: Option<f64>) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.write_with_stroke_width(duration, stroke_width),
        }
    }

    /// Progressive draw animation in parallel (without character/element stagger).
    #[pyo3(signature = (duration=1.0, stroke_width=None))]
    fn create(&self, duration: f64, stroke_width: Option<f64>) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.create_with_stroke_width(duration, stroke_width),
        }
    }

    /// Progressive erasure of the Mobject's path(s) and fill in parallel.
    #[pyo3(signature = (duration=1.0, stroke_width=None))]
    fn uncreate(&self, duration: f64, stroke_width: Option<f64>) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.uncreate_with_stroke_width(duration, stroke_width),
        }
    }

    /// Staggered sequential erasure of the Mobject's path(s) and fill in reverse order.
    #[pyo3(signature = (duration=1.0, stroke_width=None))]
    fn unwrite(&self, duration: f64, stroke_width: Option<f64>) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.unwrite_with_stroke_width(duration, stroke_width),
        }
    }

    /// Scale up from 0.0 to original size centered at current local position.
    fn grow_from_center(&self) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.grow_from_center(),
        }
    }

    /// Scale down from current size to 0.0 centered at current local position.
    fn shrink_to_center(&self) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.shrink_to_center(),
        }
    }

    /// Scale up from 0.0 and rotate 360 degrees concurrently.
    fn spin_in_from_nothing(&self) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.spin_in_from_nothing(),
        }
    }

    /// Temporarily scale up and highlight with custom parameters before returning to baseline.
    #[pyo3(signature = (color=None, scale_factor=1.25))]
    fn indicate(&self, color: Option<&PyColor>, scale_factor: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }
                .indicate_with_color_and_scale(color.map(|c| c.0), scale_factor),
        }
    }

    /// Fade out source and fade in target concurrently over the same duration.
    fn fade_transform(&self, target: &PyMobject) -> Self {
        let my_target = self.inner.target;
        Self {
            inner: MobjectRef { id: my_target }
                .fade_transform(target.id),
        }
    }

    /// Oscillating horizontal wiggle vibration.
    fn wiggle(&self) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.wiggle(),
        }
    }

    /// Scale from zero at a specific anchor point, growing to full size.
    fn grow_from_point(&self, px: f64, py: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.grow_from_point(px, py),
        }
    }

    /// Scale from zero at a specific edge direction (up/down/left/right/top/bottom).
    fn grow_from_edge(&self, direction: &str) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.grow_from_edge(direction),
        }
    }

    /// Draw the outline first, then fill in the shape.
    fn draw_border_then_fill(&self) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }.draw_border_then_fill(),
        }
    }

    /// Lines radiating outward from the object (flash of insight effect).
    #[pyo3(signature = (color=None, n_lines=12, radius=100.0))]
    fn flash(&self, color: Option<&PyColor>, n_lines: u32, radius: f64) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }
                .flash(color.map(|c| c.0), n_lines, radius),
        }
    }

    /// A rectangle/circle that appears around the target, grows, and fades.
    #[pyo3(signature = (color=None))]
    fn circumscribe(&self, color: Option<&PyColor>) -> Self {
        let target = self.inner.target;
        Self {
            inner: MobjectRef { id: target }
                .circumscribe(color.map(|c| c.0)),
        }
    }
}

pub fn rate_func_from_name(name: &str) -> PyResult<RateFunc> {
    Ok(match name {
        "linear" => RateFunc::Linear,
        "smooth" => RateFunc::Smooth,
        "double_smooth" => RateFunc::DoubleSmooth,
        "lingering" => RateFunc::Lingering,
        "running_start" => RateFunc::RunningStart,
        "spring" => RateFunc::Spring {
            stiffness: 90.0,
            damping: 12.0,
        },
        "spring_soft" => RateFunc::Spring {
            stiffness: 60.0,
            damping: 18.0,
        },
        "spring_bouncy" => RateFunc::Spring {
            stiffness: 180.0,
            damping: 8.0,
        },
        "ease_in" => RateFunc::EaseIn(EasingCurve::Cubic),
        "ease_out" => RateFunc::EaseOut(EasingCurve::Cubic),
        "ease_in_out" => RateFunc::EaseInOut(EasingCurve::Cubic),
        "back_in" => RateFunc::EaseIn(EasingCurve::Back),
        "back_out" => RateFunc::EaseOut(EasingCurve::Back),
        "back_in_out" => RateFunc::EaseInOut(EasingCurve::Back),
        "bounce_in" => RateFunc::EaseIn(EasingCurve::Bounce),
        "bounce_out" => RateFunc::EaseOut(EasingCurve::Bounce),
        "bounce_in_out" => RateFunc::EaseInOut(EasingCurve::Bounce),
        "elastic_in" => RateFunc::EaseIn(EasingCurve::Elastic),
        "elastic_out" => RateFunc::EaseOut(EasingCurve::Elastic),
        "elastic_in_out" => RateFunc::EaseInOut(EasingCurve::Elastic),
        "there_and_back" => RateFunc::ThereAndBack,
        "there_and_back_with_pause" => RateFunc::ThereAndBackWithPause(0.2),
        "exponential_decay" => RateFunc::ExponentialDecay,
        "not_quite_there" => RateFunc::NotQuiteThere,
        _ => {
            return Err(PyValueError::new_err(format!(
                "unknown rate function: {}",
                name
            )))
        }
    })
}

pub fn rate_func_name(rf: &RateFunc) -> String {
    match rf {
        RateFunc::Linear => "linear".into(),
        RateFunc::Smooth => "smooth".into(),
        RateFunc::DoubleSmooth => "double_smooth".into(),
        RateFunc::Lingering => "lingering".into(),
        RateFunc::RunningStart => "running_start".into(),
        RateFunc::Spring { .. } => "spring".into(),
        RateFunc::EaseIn(EasingCurve::Cubic) => "ease_in".into(),
        RateFunc::EaseOut(EasingCurve::Cubic) => "ease_out".into(),
        RateFunc::EaseInOut(EasingCurve::Cubic) => "ease_in_out".into(),
        RateFunc::EaseIn(EasingCurve::Back) => "back_in".into(),
        RateFunc::EaseOut(EasingCurve::Back) => "back_out".into(),
        RateFunc::EaseInOut(EasingCurve::Back) => "back_in_out".into(),
        RateFunc::EaseIn(EasingCurve::Bounce) => "bounce_in".into(),
        RateFunc::EaseOut(EasingCurve::Bounce) => "bounce_out".into(),
        RateFunc::EaseInOut(EasingCurve::Bounce) => "bounce_in_out".into(),
        RateFunc::EaseIn(EasingCurve::Elastic) => "elastic_in".into(),
        RateFunc::EaseOut(EasingCurve::Elastic) => "elastic_out".into(),
        RateFunc::EaseInOut(EasingCurve::Elastic) => "elastic_in_out".into(),
        RateFunc::ThereAndBack => "there_and_back".into(),
        RateFunc::ThereAndBackWithPause(_) => "there_and_back_with_pause".into(),
        RateFunc::ExponentialDecay => "exponential_decay".into(),
        RateFunc::NotQuiteThere => "not_quite_there".into(),
        _ => "<custom>".into(),
    }
}
