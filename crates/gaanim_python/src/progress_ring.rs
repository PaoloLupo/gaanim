//! Progress rings and countdown timers driven by one Parameter.
use gaanim_api::canvas::{ProgressLabel, ProgressRingOptions};
use pyo3::{exceptions::PyValueError, prelude::*, pyclass_init::PyClassInitializer};

use crate::{
    color::PyColor,
    pycanvas::PyVisualization,
    pydrawable::{PyCanvasAnim, PyDrawable},
    visualization::PyParameter,
};

#[pyclass(name = "ProgressRing", module = "gaanim_core", extends = PyDrawable)]
pub struct PyProgressRing {
    ring: gaanim_api::canvas::ProgressRing,
}

impl PyProgressRing {
    fn create(
        py: Python<'_>,
        visualization: &PyVisualization,
        value: f64,
        options: ProgressRingOptions,
    ) -> PyResult<Py<Self>> {
        crate::custom::ensure_authoring_allowed()?;
        let ring = visualization
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .progress_ring(value, options)
            .map_err(PyValueError::new_err)?;
        Py::new(
            py,
            PyClassInitializer::from(PyDrawable(ring.group.clone())).add_subclass(Self { ring }),
        )
    }
}

/// Track color of `track=True`, or `None` without a track.
fn track_color(track: bool, color: Option<PyColor>) -> Option<gaanim_core::peniko::Color> {
    track.then(|| {
        color.map_or_else(
            || ProgressRingOptions::default().track.expect("default track"),
            |color| color.0,
        )
    })
}

fn label_decimals(decimals: i64) -> PyResult<usize> {
    usize::try_from(decimals).map_err(|_| PyValueError::new_err("decimals must be 0..6"))
}

#[pymethods]
impl PyProgressRing {
    /// The group of the track, the arc and the label.
    #[getter]
    fn visual(&self) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyDrawable(self.ring.group.clone()))
    }

    /// Position the ring and keep the ProgressRing for fluent chaining.
    #[pyo3(signature = (x, y=None, anchor=None))]
    fn move_to<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: Option<&Bound<'_, PyAny>>,
        anchor: Option<&crate::pylayout::PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        PyDrawable(slf.ring.group.clone()).move_to_impl(x, y, anchor)?;
        Ok(slf)
    }

    fn shift_by<'py>(slf: PyRef<'py, Self>, dx: f64, dy: f64) -> PyResult<PyRef<'py, Self>> {
        PyDrawable(slf.ring.group.clone()).shift_by_impl(dx, dy)?;
        Ok(slf)
    }

    fn opacity<'py>(slf: PyRef<'py, Self>, op: &Bound<'_, PyAny>) -> PyResult<PyRef<'py, Self>> {
        PyDrawable(slf.ring.group.clone()).opacity_impl(op)?;
        Ok(slf)
    }

    /// Underlying scalar, usable in computed inputs, readouts and sampled drivers.
    #[getter]
    fn parameter(&self) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyParameter {
            inner: self.ring.parameter.clone(),
        })
    }

    #[getter]
    fn arc(&self) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyDrawable(self.ring.arc.clone()))
    }

    #[getter]
    fn track(&self) -> PyResult<Option<PyDrawable>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.ring.track.clone().map(PyDrawable))
    }

    #[getter]
    fn label(&self) -> PyResult<Option<PyDrawable>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.ring.label.clone().map(PyDrawable))
    }

    #[getter]
    fn maximum(&self) -> PyResult<f64> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.ring.maximum)
    }

    #[getter]
    fn current(&self) -> PyResult<f64> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.ring.parameter.current())
    }

    fn set<'py>(slf: PyRef<'py, Self>, value: f64) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        slf.ring
            .parameter
            .set(value)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(slf)
    }

    /// Scalar animation proxy of the value: `animate.set(value)`.
    #[getter]
    fn animate(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.ring.parameter.animate(),
        })
    }

    #[pyo3(signature = (duration=None))]
    fn count_down(&self, duration: Option<f64>) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        let duration = duration.unwrap_or_else(|| self.ring.parameter.current());
        if !duration.is_finite() || duration < 0.0 {
            return Err(PyValueError::new_err(
                "duration must be finite and non-negative",
            ));
        }
        Ok(PyCanvasAnim {
            inner: self
                .ring
                .parameter
                .animate()
                .set(0.0)
                .duration(duration)
                .rate_func(gaanim_math::RateFunc::Linear),
        })
    }
}

#[pymethods]
impl PyVisualization {
    #[pyo3(signature = (value=0.0, *, radius=1.0, width=0.12, color=None, track=true, track_color=None, label=true, decimals=0, label_color=None, font_size=0.5))]
    #[allow(clippy::too_many_arguments)]
    fn progress_ring(
        &self,
        py: Python<'_>,
        value: f64,
        radius: f64,
        width: f64,
        color: Option<PyColor>,
        track: bool,
        track_color: Option<PyColor>,
        label: bool,
        decimals: i64,
        label_color: Option<PyColor>,
        font_size: f64,
    ) -> PyResult<Py<PyProgressRing>> {
        let options = ProgressRingOptions {
            radius,
            width,
            maximum: 1.0,
            color: color.map(|color| color.0),
            track: self::track_color(track, track_color),
            label: if label {
                ProgressLabel::Percent {
                    decimals: label_decimals(decimals)?,
                }
            } else {
                ProgressLabel::None
            },
            label_color: label_color.map(|color| color.0),
            font_size,
        };
        PyProgressRing::create(py, self, value, options)
    }

    #[pyo3(signature = (seconds, *, radius=1.0, width=0.12, color=None, track=true, track_color=None, label=true, label_color=None, font_size=0.6))]
    #[allow(clippy::too_many_arguments)]
    fn countdown(
        &self,
        py: Python<'_>,
        seconds: f64,
        radius: f64,
        width: f64,
        color: Option<PyColor>,
        track: bool,
        track_color: Option<PyColor>,
        label: bool,
        label_color: Option<PyColor>,
        font_size: f64,
    ) -> PyResult<Py<PyProgressRing>> {
        if !seconds.is_finite() || seconds <= 0.0 {
            return Err(PyValueError::new_err("seconds must be finite and positive"));
        }
        let options = ProgressRingOptions {
            radius,
            width,
            maximum: seconds,
            color: color.map(|color| color.0),
            track: self::track_color(track, track_color),
            label: if label {
                ProgressLabel::Value { decimals: 0 }
            } else {
                ProgressLabel::None
            },
            label_color: label_color.map(|color| color.0),
            font_size,
        };
        PyProgressRing::create(py, self, seconds, options)
    }
}
