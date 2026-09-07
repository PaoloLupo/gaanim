use gaanim_api::canvas::{Composition, PlayError, Schedule};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::easing::PyEasing;
use crate::pycanvas::{PyAudio, PyLottie, PyVideo};
use crate::pydrawable::PyCanvasAnim;

fn play_error(error: PlayError) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(error.to_string())
}

#[pyclass(name = "Composition", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyComposition {
    pub(crate) inner: Composition,
}

#[pyclass(
    name = "ScheduleEntry",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Debug)]
pub struct PyScheduleEntry {
    path: Vec<usize>,
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    start: f64,
    #[pyo3(get)]
    duration: Option<f64>,
    #[pyo3(get)]
    end: Option<f64>,
}

#[pyclass(name = "Schedule", module = "gaanim_core", frozen, skip_from_py_object)]
#[derive(Clone, Debug)]
pub struct PySchedule {
    #[pyo3(get)]
    span: f64,
    entries: Vec<PyScheduleEntry>,
}

impl From<Schedule> for PySchedule {
    fn from(value: Schedule) -> Self {
        Self {
            span: value.span,
            entries: value
                .entries
                .into_iter()
                .map(|entry| PyScheduleEntry {
                    path: entry.path,
                    kind: entry.kind.to_owned(),
                    start: entry.start,
                    duration: entry.duration,
                    end: entry.end,
                })
                .collect(),
        }
    }
}

#[pymethods]
impl PySchedule {
    #[getter]
    fn entries<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let entries = self
            .entries
            .iter()
            .cloned()
            .map(|entry| Py::new(py, entry))
            .collect::<PyResult<Vec<_>>>()?;
        PyTuple::new(py, entries)
    }
}

#[pymethods]
impl PyScheduleEntry {
    #[getter]
    fn path<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        PyTuple::new(py, self.path.iter().copied())
    }
}

#[pymethods]
impl PyComposition {
    fn delay(&self, seconds: f64) -> PyResult<Self> {
        self.inner
            .clone()
            .delay(seconds)
            .map(|inner| Self { inner })
            .map_err(play_error)
    }

    #[pyo3(signature = (*, duration=None, easing=None))]
    fn defaults(&self, duration: Option<f64>, easing: Option<&PyEasing>) -> PyResult<Self> {
        self.inner
            .clone()
            .defaults(duration, easing.map(|value| value.inner.clone()))
            .map(|inner| Self { inner })
            .map_err(play_error)
    }

    fn stretch(&self, seconds: f64) -> PyResult<Self> {
        self.inner
            .clone()
            .stretch(seconds)
            .map(|inner| Self { inner })
            .map_err(play_error)
    }

    #[pyo3(signature = (*, duration=None))]
    fn schedule(&self, duration: Option<f64>) -> PyResult<PySchedule> {
        self.inner
            .schedule(duration)
            .map(Into::into)
            .map_err(play_error)
    }
}

pub(crate) fn extract_playable(item: &Bound<'_, PyAny>) -> PyResult<Composition> {
    if let Ok(composition) = item.extract::<PyRef<'_, PyComposition>>() {
        return Ok(composition.inner.clone());
    }
    if let Ok(anim) = item.extract::<PyRef<'_, PyCanvasAnim>>() {
        return Ok(Composition::leaf(anim.inner.clone()));
    }
    if let Ok(audio) = item.extract::<PyRef<'_, PyAudio>>() {
        return Ok(Composition::leaf(audio.inner.clone()));
    }
    if let Ok(segment) = item.extract::<PyRef<'_, crate::pydrawable::PyVideoSegment>>() {
        return Ok(Composition::leaf(segment.inner.clone()));
    }
    if let Ok(video) = item.extract::<PyRef<'_, PyVideo>>() {
        return Ok(Composition::leaf(video.inner.clone()));
    }
    if let Ok(lottie) = item.extract::<PyRef<'_, PyLottie>>() {
        return Ok(Composition::leaf(lottie.inner.clone()));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "expected Anim, Audio, Video, VideoSegment, Lottie, or Composition",
    ))
}

pub(crate) fn extract_play_root(item: &Bound<'_, PyAny>) -> PyResult<Composition> {
    if let Ok(leaf) = extract_playable(item) {
        return Ok(leaf);
    }
    let children = item
        .try_iter()?
        .map(|child| extract_playable(&child?))
        .collect::<PyResult<Vec<_>>>()?;
    Composition::parallel(children).map_err(play_error)
}

fn tuple_children(items: &Bound<'_, PyTuple>) -> PyResult<Vec<Composition>> {
    items.iter().map(|item| extract_playable(&item)).collect()
}

#[pyfunction]
#[pyo3(signature = (*items))]
pub fn parallel(items: &Bound<'_, PyTuple>) -> PyResult<PyComposition> {
    Composition::parallel(tuple_children(items)?)
        .map(|inner| PyComposition { inner })
        .map_err(play_error)
}

#[pyfunction]
#[pyo3(signature = (*items, gap=0.0))]
pub fn sequence(items: &Bound<'_, PyTuple>, gap: f64) -> PyResult<PyComposition> {
    Composition::sequence(tuple_children(items)?, gap)
        .map(|inner| PyComposition { inner })
        .map_err(play_error)
}

#[pyfunction]
#[pyo3(signature = (*items, each=0.1))]
pub fn stagger(items: &Bound<'_, PyTuple>, each: f64) -> PyResult<PyComposition> {
    Composition::stagger(tuple_children(items)?, each)
        .map(|inner| PyComposition { inner })
        .map_err(play_error)
}
