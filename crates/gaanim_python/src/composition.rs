use gaanim_api::canvas::{Composition, PlayError, Schedule, StaggerLayout, StaggerOrigin};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::easing::PyEasing;
use crate::pycanvas::{PyAudio, PyLottie, PyVideo};
use crate::pydrawable::{PyCanvasAnim, PyDrawable};

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

    #[pyo3(signature = (count, *, delay=0.0))]
    fn repeat(&self, count: i64, delay: f64) -> PyResult<Self> {
        if !(1..=u32::MAX as i64).contains(&count) {
            return Err(PyValueError::new_err("count must be at least 1"));
        }
        self.inner
            .clone()
            .repeat(count as u32, delay)
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

fn stagger_origin(origin: &Bound<'_, PyAny>, seed: u64) -> PyResult<StaggerOrigin> {
    if let Ok(name) = origin.extract::<String>() {
        return match name.as_str() {
            "start" => Ok(StaggerOrigin::Start),
            "end" => Ok(StaggerOrigin::End),
            "center" => Ok(StaggerOrigin::Center),
            "edges" => Ok(StaggerOrigin::Edges),
            "random" => Ok(StaggerOrigin::Random(seed)),
            other => Err(PyValueError::new_err(format!(
                "unknown origin {other:?}; expected \"start\", \"end\", \"center\", \"edges\", \"random\" or an (x, y) point"
            ))),
        };
    }
    let (x, y) = origin
        .extract::<(f64, f64)>()
        .map_err(|_| PyTypeError::new_err("origin must be a name or an (x, y) point"))?;
    if !x.is_finite() || !y.is_finite() {
        return Err(PyValueError::new_err("origin point must be finite"));
    }
    Ok(StaggerOrigin::Point(x, y))
}

fn stagger_grid(grid: Option<&Bound<'_, PyAny>>) -> PyResult<Option<(usize, usize)>> {
    let Some(grid) = grid else {
        return Ok(None);
    };
    if grid.extract::<String>().is_ok_and(|name| name == "auto") {
        return Ok(None);
    }
    let (rows, columns) = grid
        .extract::<(usize, usize)>()
        .map_err(|_| PyTypeError::new_err("grid must be \"auto\" or (rows, columns)"))?;
    if rows == 0 || columns == 0 {
        return Err(PyValueError::new_err(
            "grid needs at least one row and column",
        ));
    }
    Ok(Some((rows, columns)))
}

#[pyfunction]
#[pyo3(signature = (*items, each=0.1, total=None, origin=None, grid=None, easing=None, seed=0))]
#[allow(clippy::too_many_arguments)]
pub fn stagger(
    items: &Bound<'_, PyTuple>,
    each: f64,
    total: Option<f64>,
    origin: Option<&Bound<'_, PyAny>>,
    grid: Option<&Bound<'_, PyAny>>,
    easing: Option<&PyEasing>,
    seed: u64,
) -> PyResult<PyComposition> {
    let children = tuple_children(items)?;
    let spatial = origin.is_some() || grid.is_some() || total.is_some() || easing.is_some();
    let composition = if spatial {
        let origin = match origin {
            Some(origin) => stagger_origin(origin, seed)?,
            None => StaggerOrigin::Start,
        };
        Composition::stagger_layout(
            children,
            each,
            StaggerLayout {
                origin,
                grid: stagger_grid(grid)?,
                total,
                easing: easing.map(|easing| easing.inner.clone()),
            },
        )
    } else {
        Composition::stagger(children, each)
    };
    composition
        .map(|inner| PyComposition { inner })
        .map_err(play_error)
}

/// Spread values from `low` to `high` over drawables by their distance from
/// `origin`, with the same ordering as a spatial `stagger`.
#[pyfunction]
#[pyo3(signature = (items, low, high, *, origin=None, grid=None, easing=None, seed=0))]
pub fn distribute(
    items: Vec<PyRef<'_, PyDrawable>>,
    low: f64,
    high: f64,
    origin: Option<&Bound<'_, PyAny>>,
    grid: Option<&Bound<'_, PyAny>>,
    easing: Option<&PyEasing>,
    seed: u64,
) -> PyResult<Vec<f64>> {
    if !low.is_finite() || !high.is_finite() {
        return Err(PyValueError::new_err("low and high must be finite"));
    }
    let origin = match origin {
        Some(origin) => stagger_origin(origin, seed)?,
        None => StaggerOrigin::Start,
    };
    let positions: Vec<gaanim_core::glam::DVec2> = match stagger_grid(grid)? {
        Some((_, columns)) => (0..items.len())
            .map(|index| {
                gaanim_core::glam::DVec2::new((index % columns) as f64, -((index / columns) as f64))
            })
            .collect(),
        None => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                item.0
                    .authored_position()
                    .unwrap_or(gaanim_core::glam::DVec2::new(index as f64, 0.0))
            })
            .collect(),
    };
    let easing = easing.map(|easing| easing.inner.clone());
    let (weights, _) = gaanim_api::canvas::stagger_weights(&positions, origin, easing.as_ref());
    Ok(weights
        .into_iter()
        .map(|weight| low + (high - low) * weight)
        .collect())
}
