//! `Anim.keyframes`, `Anim.throw` and `Anim.inertia`.

use gaanim_animation::motion::{Keyframes, ScalarKeyframes, Snap, Spatial, Stops};
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_math::RateFunc;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

use crate::brush::PyPaint;
use crate::easing::PyEasing;
use crate::pydrawable::PyCanvasAnim;

type Values<'py> = Option<Vec<Option<Bound<'py, PyAny>>>>;

fn stops<'py, T>(
    values: Values<'py>,
    name: &str,
    parse: impl Fn(&Bound<'py, PyAny>) -> PyResult<T>,
) -> PyResult<Stops<T>> {
    values
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    value
                        .as_ref()
                        .map(|value| {
                            parse(value).map_err(|_| {
                                PyTypeError::new_err(format!(
                                    "keyframes() {name} has a value of the wrong type: {value}"
                                ))
                            })
                        })
                        .transpose()
                })
                .collect()
        })
        .transpose()
}

fn point(value: &Bound<'_, PyAny>) -> PyResult<DVec3> {
    if let Ok((x, y)) = value.extract::<(f64, f64)>() {
        return Ok(DVec3::new(x, y, 0.0));
    }
    let (x, y, z) = value.extract::<(f64, f64, f64)>()?;
    Ok(DVec3::new(x, y, z))
}

fn scale(value: &Bound<'_, PyAny>) -> PyResult<DVec3> {
    if let Ok(uniform) = value.extract::<f64>() {
        return Ok(DVec3::splat(uniform));
    }
    if let Ok((x, y)) = value.extract::<(f64, f64)>() {
        return Ok(DVec3::new(x, y, 1.0));
    }
    let (x, y, z) = value.extract::<(f64, f64, f64)>()?;
    Ok(DVec3::new(x, y, z))
}

/// One easing for every segment, or one per segment.
fn easings(easing: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<RateFunc>> {
    let Some(easing) = easing else {
        return Ok(Vec::new());
    };
    if let Ok(easing) = easing.extract::<PyEasing>() {
        return Ok(vec![easing.inner]);
    }
    easing
        .extract::<Vec<PyEasing>>()
        .map(|easings| easings.into_iter().map(|easing| easing.inner).collect())
        .map_err(|_| PyTypeError::new_err("easing must be an Easing or a list of Easing"))
}

fn vector(value: &Bound<'_, PyAny>, method: &str) -> PyResult<DVec2> {
    if let Ok(x) = value.extract::<f64>() {
        return Ok(DVec2::new(x, 0.0));
    }
    value
        .extract::<(f64, f64)>()
        .map(|(x, y)| DVec2::new(x, y))
        .map_err(|_| {
            PyTypeError::new_err(format!("{method} velocity must be a number or (vx, vy)"))
        })
}

#[pymethods]
impl PyCanvasAnim {
    /// Several stops in one clip; see `gaanim_core.pyi`.
    #[pyo3(signature = (
        times, *, position=None, rotation=None, scale=None, opacity=None,
        fill=None, stroke=None, values=None, easing=None, spatial="linear"
    ))]
    #[allow(clippy::too_many_arguments)]
    fn keyframes<'py>(
        &self,
        times: Vec<f64>,
        position: Values<'py>,
        rotation: Values<'py>,
        scale: Values<'py>,
        opacity: Values<'py>,
        fill: Values<'py>,
        stroke: Values<'py>,
        values: Option<Vec<Option<f64>>>,
        easing: Option<Bound<'py, PyAny>>,
        spatial: &str,
    ) -> PyResult<Self> {
        self.require_native_animation()?;
        let easings = easings(easing.as_ref())?;
        let spatial = match spatial {
            "linear" => Spatial::Linear,
            "catmull_rom" => Spatial::CatmullRom,
            _ => {
                return Err(PyValueError::new_err(
                    "spatial must be 'linear' or 'catmull_rom'",
                ));
            }
        };
        if let Some(values) = values {
            if [&position, &rotation, &scale, &opacity, &fill, &stroke]
                .iter()
                .any(|channel| channel.is_some())
            {
                return Err(PyValueError::new_err(
                    "keyframes() takes values= for a Parameter, or drawable channels, not both",
                ));
            }
            if spatial != Spatial::Linear {
                return Err(PyValueError::new_err(
                    "spatial applies to position keyframes",
                ));
            }
            return self
                .inner
                .clone()
                .signal_keyframes(ScalarKeyframes {
                    times,
                    values,
                    easings,
                })
                .map(|inner| Self { inner })
                .map_err(PyValueError::new_err);
        }
        self.require_transformable()?;
        if spatial != Spatial::Linear && position.is_none() {
            return Err(PyValueError::new_err(
                "spatial applies to position keyframes",
            ));
        }
        let keyframes = Keyframes {
            times,
            position: stops(position, "position", point)?,
            rotation: stops(rotation, "rotation", |value| value.extract::<f64>())?,
            scale: stops(scale, "scale", self::scale)?,
            opacity: stops(opacity, "opacity", |value| value.extract::<f32>())?,
            fill: stops(fill, "fill", |value| Ok(value.extract::<PyPaint>()?.0))?,
            stroke: stops(stroke, "stroke", |value| Ok(value.extract::<PyPaint>()?.0))?,
            easings,
            spatial,
        };
        self.inner
            .clone()
            .keyframes(keyframes)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    /// A ballistic throw with bounces; see `gaanim_core.pyi`.
    #[pyo3(signature = (velocity, *, gravity=9.8, floor=None, restitution=0.55))]
    fn throw(
        &self,
        velocity: Bound<'_, PyAny>,
        gravity: f64,
        floor: Option<f64>,
        restitution: f64,
    ) -> PyResult<Self> {
        self.require_native_animation()?;
        self.require_transformable()?;
        let velocity = vector(&velocity, "throw()")?;
        self.inner
            .clone()
            .throw(velocity, gravity, floor, restitution)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }

    /// An inertial glide to a snap point; see `gaanim_core.pyi`.
    #[pyo3(signature = (velocity, *, friction=3.0, snap=None))]
    fn inertia(
        &self,
        velocity: Bound<'_, PyAny>,
        friction: f64,
        snap: Option<Vec<Bound<'_, PyAny>>>,
    ) -> PyResult<Self> {
        self.require_native_animation()?;
        self.require_transformable()?;
        let velocity = vector(&velocity, "inertia()")?;
        let snap = snap
            .unwrap_or_default()
            .iter()
            .map(|value| {
                if let Ok(x) = value.extract::<f64>() {
                    return Ok(Snap::X(x));
                }
                value
                    .extract::<(f64, f64)>()
                    .map(|(x, y)| Snap::Point(DVec2::new(x, y)))
                    .map_err(|_| {
                        PyTypeError::new_err(
                            "inertia() snap entries must be x values or (x, y) points",
                        )
                    })
            })
            .collect::<PyResult<Vec<_>>>()?;
        self.inner
            .clone()
            .inertia(velocity, friction, snap)
            .map(|inner| Self { inner })
            .map_err(PyValueError::new_err)
    }
}
