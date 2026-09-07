//! Rolling numeric displays backed by the normal scalar timeline.
use gaanim_api::canvas::{RollingMode, RollingNumberOptions};
use pyo3::{exceptions::PyValueError, prelude::*, pyclass_init::PyClassInitializer};

use crate::{
    color::PyColor,
    pycanvas::PyVisualization,
    pydrawable::{PyCanvasAnim, PyDrawable},
    visualization::PyParameter,
};

#[pyclass(name = "RollingNumber", module = "gaanim_core", extends = PyDrawable)]
pub struct PyRollingNumber {
    parameter: PyParameter,
    visual: PyDrawable,
    options: RollingNumberOptions,
}

#[pymethods]
impl PyRollingNumber {
    fn fill<'py>(
        slf: PyRef<'py, Self>,
        paint: crate::brush::PyPaint,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        slf.visual.0.clone().fill_brush(paint.0);
        Ok(slf)
    }

    fn opacity<'py>(slf: PyRef<'py, Self>, op: &Bound<'py, PyAny>) -> PyResult<PyRef<'py, Self>> {
        slf.visual.opacity(op)?;
        Ok(slf)
    }

    #[pyo3(signature = (x, y=None, anchor=None))]
    fn move_to<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'py, PyAny>,
        y: Option<&Bound<'py, PyAny>>,
        anchor: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        if let Some(anchor) = anchor.and_then(|value| {
            value
                .extract::<PyRef<'_, crate::pytext::PyTextAnchor>>()
                .ok()
        }) {
            slf.visual.require_free_position("move_to")?;
            let y = y.ok_or_else(|| {
                pyo3::exceptions::PyTypeError::new_err(
                    "move_to with a TextAnchor requires both x and y coordinates",
                )
            })?;
            let sx =
                crate::visualization::extract_scalar_source_for_drawable(x.clone(), &slf.visual.0)?;
            let sy =
                crate::visualization::extract_scalar_source_for_drawable(y.clone(), &slf.visual.0)?;
            if let (Some(x), Some(y)) = (sx.constant_value(), sy.constant_value()) {
                slf.visual.0.clone().at_text_anchor(x, y, anchor.0);
            } else {
                slf.visual
                    .0
                    .clone()
                    .bind_text_position([sx, sy, 0.0.into()], anchor.0, false)
                    .map_err(PyValueError::new_err)?;
            }
        } else {
            let anchor = anchor
                .map(|value| value.extract::<PyRef<'_, crate::pylayout::PyAnchor>>())
                .transpose()?;
            slf.visual.move_to(x, y, anchor.as_deref())?;
        }
        Ok(slf)
    }

    #[getter]
    fn parameter(&self) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.parameter.clone())
    }

    #[getter]
    fn visual(&self) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.visual.clone())
    }

    #[getter]
    fn current(&self) -> PyResult<f64> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.parameter.inner.current())
    }

    fn set<'py>(slf: PyRef<'py, Self>, value: f64) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        slf.options
            .validate_value(value)
            .map_err(PyValueError::new_err)?;
        slf.parameter
            .inner
            .set(value)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(slf)
    }

    #[getter]
    fn animate(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.parameter.inner.animate(),
        })
    }

    #[pyo3(signature = (value, *, duration=1.0))]
    fn count_to(&self, value: f64, duration: f64) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        self.options
            .validate_value(value)
            .map_err(PyValueError::new_err)?;
        if !duration.is_finite() || duration < 0.0 {
            return Err(PyValueError::new_err(
                "duration must be finite and non-negative",
            ));
        }
        Ok(PyCanvasAnim {
            inner: self.parameter.inner.animate().set(value).duration(duration),
        })
    }
}

#[pymethods]
impl PyVisualization {
    #[pyo3(signature = (value=0.0, *, decimals=0, min_digits=1, group_separator="", decimal_separator=".", prefix="", suffix="", show_plus=false, font_family=None, font_size=0.75, digit_spacing=0.02, line_height=1.25, mode="odometer", direction="up", color=None))]
    fn rolling_number(
        &self,
        py: Python<'_>,
        value: f64,
        decimals: i64,
        min_digits: i64,
        group_separator: &str,
        decimal_separator: &str,
        prefix: &str,
        suffix: &str,
        show_plus: bool,
        font_family: Option<String>,
        font_size: f64,
        digit_spacing: f64,
        line_height: f64,
        mode: &str,
        direction: &str,
        color: Option<PyColor>,
    ) -> PyResult<Py<PyRollingNumber>> {
        crate::custom::ensure_authoring_allowed()?;
        let options = RollingNumberOptions {
            decimals: usize::try_from(decimals)
                .map_err(|_| PyValueError::new_err("decimals must be 0..6"))?,
            min_digits: usize::try_from(min_digits)
                .map_err(|_| PyValueError::new_err("min_digits must be 1..15"))?,
            group_separator: group_separator.into(),
            decimal_separator: decimal_separator.into(),
            prefix: prefix.into(),
            suffix: suffix.into(),
            show_plus,
            font_family,
            font_size,
            digit_spacing,
            line_height,
            mode: match mode {
                "odometer" => RollingMode::Odometer,
                "continuous" => RollingMode::Continuous,
                _ => {
                    return Err(PyValueError::new_err(
                        "mode must be 'odometer' or 'continuous'",
                    ))
                }
            },
            roll_up: match direction {
                "up" => true,
                "down" => false,
                _ => return Err(PyValueError::new_err("direction must be 'up' or 'down'")),
            },
        };
        options.validate().map_err(PyValueError::new_err)?;
        options
            .validate_value(value)
            .map_err(PyValueError::new_err)?;
        let mut canvas = self.inner.lock().expect("scene canvas poisoned");
        let parameter = canvas
            .parameter(value)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let mut drawable = canvas
            .rolling_number(parameter.source(), options.clone())
            .map_err(PyValueError::new_err)?;
        if let Some(color) = color {
            drawable = drawable.fill(color.0);
        }
        Py::new(
            py,
            PyClassInitializer::from(PyDrawable(drawable.clone())).add_subclass(PyRollingNumber {
                parameter: PyParameter { inner: parameter },
                visual: PyDrawable(drawable),
                options,
            }),
        )
    }
}
