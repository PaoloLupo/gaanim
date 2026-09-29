//! `Falloff`: per-instance influence that drives drawable channels.

use std::sync::Arc;

use gaanim_animation::{
    ColorRamp, FalloffChannel, FalloffEffect, FalloffExpr, FalloffShape, FalloffTarget,
};
use gaanim_core::glam::DVec2;
use gaanim_math::{Noise, RateFunc};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

use crate::color::PyColor;
use crate::easing::PyEasing;
use crate::pydrawable::PyDrawable;
use gaanim_api::canvas::DrawableHandle;

/// A value per instance, from its index, its distance to a target or noise.
///
/// Connect it to a channel with `Drawable.drive`. Build one with
/// `Falloff.distance`, `index`, `linear`, `noise` or `constant`, reshape it
/// with `remap` and `invert`, and combine falloffs with `+`, `-`, `*`,
/// `maximum` and `minimum`.
#[pyclass(name = "Falloff", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyFalloff {
    pub(crate) expr: FalloffExpr,
    /// The drawables the falloff measures against, to keep them in one scene.
    pub(crate) targets: Vec<DrawableHandle>,
}

/// A falloff turned into colors with `Falloff.gradient`.
#[pyclass(name = "FalloffColor", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyFalloffColor {
    ramp: ColorRamp,
    expr: FalloffExpr,
    targets: Vec<DrawableHandle>,
}

fn finite(name: &str, value: f64) -> PyResult<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(PyValueError::new_err(format!("{name} must be finite")))
    }
}

fn shape_of(name: &str) -> PyResult<FalloffShape> {
    match name {
        "linear" => Ok(FalloffShape::Linear),
        "smooth" => Ok(FalloffShape::Smooth),
        "sharp" => Ok(FalloffShape::Sharp),
        "round" => Ok(FalloffShape::Round),
        _ => Err(PyValueError::new_err(
            "falloff must be 'linear', 'smooth', 'sharp' or 'round'",
        )),
    }
}

/// A drawable or an `(x, y)` scene point; a drawable's handle comes with it.
pub(crate) fn target_of(
    value: &Bound<'_, PyAny>,
) -> PyResult<(FalloffTarget, Option<DrawableHandle>)> {
    if let Ok(drawable) = value.extract::<PyRef<'_, PyDrawable>>() {
        return Ok((
            FalloffTarget::Object(drawable.0.id),
            Some(drawable.0.clone()),
        ));
    }
    let (x, y) = value
        .extract::<(f64, f64)>()
        .map_err(|_| PyTypeError::new_err("a target must be a Drawable or an (x, y) point"))?;
    Ok((
        FalloffTarget::Point(DVec2::new(finite("x", x)?, finite("y", y)?)),
        None,
    ))
}

fn operand(value: &Bound<'_, PyAny>) -> PyResult<(FalloffExpr, Vec<DrawableHandle>)> {
    if let Ok(falloff) = value.extract::<PyRef<'_, PyFalloff>>() {
        return Ok((falloff.expr.clone(), falloff.targets.clone()));
    }
    if let Ok(number) = value.extract::<f64>() {
        return Ok((
            FalloffExpr::Constant(finite("operand", number)?),
            Vec::new(),
        ));
    }
    Err(PyTypeError::new_err(
        "a Falloff combines with a number or another Falloff",
    ))
}

impl PyFalloff {
    fn combine(
        &self,
        other: &Bound<'_, PyAny>,
        make: fn(Box<FalloffExpr>, Box<FalloffExpr>) -> FalloffExpr,
        flipped: bool,
    ) -> PyResult<Self> {
        let (other, other_targets) = operand(other)?;
        let this = self.expr.clone();
        let (left, right) = if flipped {
            (other, this)
        } else {
            (this, other)
        };
        let mut targets = self.targets.clone();
        targets.extend(other_targets);
        Ok(Self {
            expr: make(Box::new(left), Box::new(right)),
            targets,
        })
    }
}

#[pymethods]
impl PyFalloff {
    /// The same value for every instance.
    #[staticmethod]
    fn constant(value: f64) -> PyResult<Self> {
        Ok(Self {
            expr: FalloffExpr::Constant(finite("value", value)?),
            targets: Vec::new(),
        })
    }

    /// The instance's place in its group: 0 for the first, 1 for the last,
    /// shaped by `easing`.
    #[staticmethod]
    #[pyo3(signature = (*, easing=None, reverse=false))]
    fn index(easing: Option<&PyEasing>, reverse: bool) -> Self {
        Self {
            expr: FalloffExpr::Index {
                easing: easing.map_or(RateFunc::Linear, |easing| easing.inner.clone()),
                reverse,
            },
            targets: Vec::new(),
        }
    }

    /// 1 at `target` (a drawable, followed wherever animations move it, or an
    /// `(x, y)` point), 0 at `radius` scene units and beyond.
    #[staticmethod]
    #[pyo3(signature = (target, *, radius=2.0, falloff="smooth"))]
    fn distance(target: &Bound<'_, PyAny>, radius: f64, falloff: &str) -> PyResult<Self> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(PyValueError::new_err("radius must be finite and positive"));
        }
        let (target, handle) = target_of(target)?;
        Ok(Self {
            expr: FalloffExpr::Distance {
                target,
                radius,
                shape: shape_of(falloff)?,
            },
            targets: handle.into_iter().collect(),
        })
    }

    /// 0 at `start`, 1 at `end` and in between along the line joining them;
    /// each is a drawable or an `(x, y)` point.
    #[staticmethod]
    #[pyo3(signature = (start, end, *, falloff="linear"))]
    fn linear(start: &Bound<'_, PyAny>, end: &Bound<'_, PyAny>, falloff: &str) -> PyResult<Self> {
        let (from, from_handle) = target_of(start)?;
        let (to, to_handle) = target_of(end)?;
        Ok(Self {
            expr: FalloffExpr::Linear {
                from,
                to,
                shape: shape_of(falloff)?,
            },
            targets: from_handle.into_iter().chain(to_handle).collect(),
        })
    }

    /// Seeded simplex noise in `[0, 1]` over each instance's position and the
    /// time: `scale` is cycles per scene unit (neighbors get similar values)
    /// and `frequency` how fast the field drifts.
    #[staticmethod]
    #[pyo3(signature = (*, frequency=0.5, scale=0.5, octaves=1, seed=0))]
    fn noise(frequency: f64, scale: f64, octaves: u32, seed: u64) -> PyResult<Self> {
        finite("frequency", frequency)?;
        finite("scale", scale)?;
        if !(1..=8).contains(&octaves) {
            return Err(PyValueError::new_err("octaves must be between 1 and 8"));
        }
        Ok(Self {
            expr: FalloffExpr::Noise {
                noise: Arc::new(Noise::new(seed, 1.0, 1.0, octaves)),
                scale,
                speed: frequency,
            },
            targets: Vec::new(),
        })
    }

    /// Map `[0, 1]` to `[low, high]`: 0 becomes `low` and 1 becomes `high`.
    fn remap(&self, low: f64, high: f64) -> PyResult<Self> {
        Ok(Self {
            expr: FalloffExpr::Remap {
                input: Box::new(self.expr.clone()),
                low: finite("low", low)?,
                high: finite("high", high)?,
            },
            targets: self.targets.clone(),
        })
    }

    /// `1 - value`: far becomes near and near becomes far.
    fn invert(&self) -> Self {
        Self {
            expr: FalloffExpr::Invert(Box::new(self.expr.clone())),
            targets: self.targets.clone(),
        }
    }

    /// The larger of this falloff and `other` (a falloff or a number).
    fn maximum(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.combine(other, FalloffExpr::Max, false)
    }

    /// The smaller of this falloff and `other` (a falloff or a number).
    fn minimum(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.combine(other, FalloffExpr::Min, false)
    }

    /// Turn the value into a color: 0 is the first color, 1 the last, evenly
    /// spaced colors in between.
    #[pyo3(signature = (*colors))]
    fn gradient(&self, colors: Vec<PyColor>) -> PyResult<PyFalloffColor> {
        if colors.len() < 2 {
            return Err(PyValueError::new_err(
                "a gradient needs at least two colors",
            ));
        }
        Ok(PyFalloffColor {
            ramp: ColorRamp(colors.into_iter().map(|color| color.0).collect()),
            expr: self.expr.clone(),
            targets: self.targets.clone(),
        })
    }

    fn __add__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.combine(other, FalloffExpr::Add, false)
    }

    fn __radd__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.combine(other, FalloffExpr::Add, true)
    }

    fn __sub__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.combine(other, FalloffExpr::Sub, false)
    }

    fn __rsub__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.combine(other, FalloffExpr::Sub, true)
    }

    fn __mul__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.combine(other, FalloffExpr::Mul, false)
    }

    fn __rmul__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.combine(other, FalloffExpr::Mul, true)
    }

    fn __repr__(&self) -> String {
        "Falloff(...)".to_owned()
    }
}

#[pymethods]
impl PyFalloffColor {
    fn __repr__(&self) -> String {
        format!("FalloffColor({} colors)", self.ramp.0.len())
    }
}

impl PyDrawable {
    /// `drive`, `look_at` and `clear_drive` share the falloff plumbing.
    fn attach(&self, effect: FalloffEffect, targets: &[DrawableHandle]) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        if targets.iter().any(|target| !self.0.same_canvas(target)) {
            return Err(PyValueError::new_err(
                "a falloff target belongs to another scene",
            ));
        }
        self.0.drive_falloff(effect);
        Ok(())
    }
}

#[pymethods]
impl PyDrawable {
    /// Connect a falloff to a channel of this drawable, or of each member of
    /// this group, from the timeline cursor on.
    ///
    /// `channel` is `"scale"` (multiplies the scale), `"opacity"` (multiplies
    /// the opacity), `"rotation"` (adds radians), `"x"` or `"y"` (add scene
    /// units) with a `Falloff`, or `"fill"` with a `FalloffColor` from
    /// `Falloff.gradient`. A value is a pure function of the time, so seeks
    /// and exports agree.
    fn drive(&self, channel: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if channel == "fill" {
            let color = value.extract::<PyRef<'_, PyFalloffColor>>().map_err(|_| {
                PyTypeError::new_err("fill takes a FalloffColor: use Falloff.gradient(...)")
            })?;
            return self.attach(
                FalloffEffect::Fill {
                    ramp: color.ramp.clone(),
                    expr: color.expr.clone(),
                },
                &color.targets,
            );
        }
        let channel = match channel {
            "scale" => FalloffChannel::Scale,
            "rotation" => FalloffChannel::Rotation,
            "opacity" => FalloffChannel::Opacity,
            "x" => FalloffChannel::OffsetX,
            "y" => FalloffChannel::OffsetY,
            _ => {
                return Err(PyValueError::new_err(
                    "channel must be 'scale', 'rotation', 'opacity', 'x', 'y' or 'fill'",
                ));
            }
        };
        let falloff = value.extract::<PyRef<'_, PyFalloff>>().map_err(|_| {
            PyTypeError::new_err("this channel takes a Falloff (a FalloffColor drives 'fill')")
        })?;
        self.attach(
            FalloffEffect::Scalar {
                channel,
                expr: falloff.expr.clone(),
            },
            &falloff.targets,
        )
    }

    /// Turn this drawable, or each member of this group, so its x axis
    /// points at `target` (a drawable or an `(x, y)` point), plus `offset`
    /// radians, from the cursor on.
    #[pyo3(signature = (target, *, offset=0.0))]
    fn look_at(&self, target: &Bound<'_, PyAny>, offset: f64) -> PyResult<()> {
        let (target, handle) = target_of(target)?;
        self.attach(
            FalloffEffect::LookAt {
                target,
                offset: finite("offset", offset)?,
            },
            &handle.into_iter().collect::<Vec<_>>(),
        )
    }

    /// End every `drive` and `look_at` of this drawable, or of the members of
    /// this group, at the timeline cursor.
    fn clear_drive(&self) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        self.0.clear_falloff();
        Ok(())
    }
}
