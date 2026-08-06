//! Python-facing reactive float value tracker.

use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

use crate::pydrawable::PyCanvasAnim;

/// A scalar that can drive reactive geometry and timeline animations.
#[pyclass(name = "ValueTracker", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyValueTracker {
    pub inner: gaanim_api::canvas::DrawableHandle,
    value: Arc<Mutex<f64>>,
}

impl PyValueTracker {
    pub fn new(inner: gaanim_api::canvas::DrawableHandle, initial: f64) -> Self {
        Self {
            inner,
            value: Arc::new(Mutex::new(initial)),
        }
    }

    pub fn current_value(&self) -> f64 {
        *self.value.lock().expect("value tracker poisoned")
    }
}

#[pymethods]
impl PyValueTracker {
    #[getter]
    fn current(&self) -> f64 {
        self.current_value()
    }

    fn set_value(&self, value: f64) {
        *self.value.lock().expect("value tracker poisoned") = value;
        self.inner.clone().set_value(value);
    }

    fn get_value(&self) -> f64 {
        self.current_value()
    }

    fn animate_to(&self, value: f64) -> PyCanvasAnim {
        *self.value.lock().expect("value tracker poisoned") = value;
        PyCanvasAnim {
            inner: self.inner.animate_value_to(value),
        }
    }
}
