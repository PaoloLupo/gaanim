//! Bar chart races driven by one keyframe-position Parameter.
use gaanim_api::canvas::{BAR_RACE_PALETTE, BarRace, BarRaceLabels, BarRaceOptions, ValueFormat};
use pyo3::{
    exceptions::{PyKeyError, PyTypeError, PyValueError},
    prelude::*,
    pyclass_init::PyClassInitializer,
    types::{PyBool, PyDict, PyFloat, PyInt, PyMapping, PyString},
};

use crate::{
    color::PyColor,
    pycanvas::PyVisualization,
    pydrawable::{PyCanvasAnim, PyDrawable},
    visualization::PyParameter,
};

#[pyclass(name = "BarRace", module = "gaanim_core", extends = PyDrawable)]
pub struct PyBarRace {
    race: BarRace,
}

/// Animation proxy of a bar race: ``race.animate.play()``.
#[pyclass(name = "BarRaceAnimation", module = "gaanim_core", skip_from_py_object)]
pub struct PyBarRaceAnimation {
    race: BarRace,
}

#[pymethods]
impl PyBarRaceAnimation {
    /// Run from the current position to the last keyframe at a constant rate.
    fn play(&self) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyCanvasAnim {
            inner: self.race.play(),
        })
    }

    /// Move to a keyframe position (fractional positions lie between keyframes).
    fn to(&self, position: f64) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        self.race
            .to(position)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(PyValueError::new_err)
    }
}

#[pymethods]
impl PyBarRace {
    /// The group of every bar and the ticker.
    #[getter]
    fn visual(&self) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyDrawable(self.race.group.clone()))
    }

    /// Position the race by its center and keep the BarRace for chaining.
    #[pyo3(signature = (x, y=None, anchor=None))]
    fn move_to<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: Option<&Bound<'_, PyAny>>,
        anchor: Option<&crate::pylayout::PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        PyDrawable(slf.race.group.clone()).move_to_impl(x, y, anchor)?;
        Ok(slf)
    }

    fn shift_by<'py>(slf: PyRef<'py, Self>, dx: f64, dy: f64) -> PyResult<PyRef<'py, Self>> {
        PyDrawable(slf.race.group.clone()).shift_by_impl(dx, dy)?;
        Ok(slf)
    }

    fn opacity<'py>(slf: PyRef<'py, Self>, op: &Bound<'_, PyAny>) -> PyResult<PyRef<'py, Self>> {
        PyDrawable(slf.race.group.clone()).opacity_impl(op)?;
        Ok(slf)
    }

    /// Keyframe position, usable in computed inputs and readouts.
    #[getter]
    fn parameter(&self) -> PyResult<PyParameter> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyParameter {
            inner: self.race.parameter.clone(),
        })
    }

    #[getter]
    fn position(&self) -> PyResult<f64> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.race.parameter.current())
    }

    #[getter]
    fn frame_count(&self) -> PyResult<usize> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.race.model.frame_count())
    }

    #[getter]
    fn names(&self) -> PyResult<Vec<String>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.race.model.names().to_vec())
    }

    #[getter]
    fn ticker(&self) -> PyResult<Option<PyDrawable>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.race.ticker.clone().map(PyDrawable))
    }

    /// The row of one bar: its bar, name and value, moving between slots.
    fn bar(&self, name: &str) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        self.race
            .bars
            .iter()
            .find(|bar| bar.name == name)
            .map(|bar| PyDrawable(bar.row.clone()))
            .ok_or_else(|| PyKeyError::new_err(name.to_owned()))
    }

    /// Jump to a keyframe position; after declaration this is a reversible cut.
    fn set<'py>(slf: PyRef<'py, Self>, position: f64) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        let last = slf.race.last_position();
        if !position.is_finite() || !(0.0..=last).contains(&position) {
            return Err(PyValueError::new_err(format!(
                "bar race position must be between 0 and {last}"
            )));
        }
        slf.race
            .parameter
            .set(position)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(slf)
    }

    #[getter]
    fn animate(&self) -> PyResult<PyBarRaceAnimation> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyBarRaceAnimation {
            race: self.race.clone(),
        })
    }
}

/// `(label, {name: value})` pairs from a mapping or a sequence of pairs.
fn keyframes<'py>(
    frames: &Bound<'py, PyAny>,
) -> PyResult<Vec<(Bound<'py, PyAny>, Bound<'py, PyAny>)>> {
    let items = if let Ok(mapping) = frames.cast::<PyMapping>() {
        mapping.items()?.into_any()
    } else {
        frames.clone()
    };
    let mut pairs = Vec::new();
    for item in items.try_iter()? {
        let item = item?;
        let (label, row) = item.extract::<(Bound<'py, PyAny>, Bound<'py, PyAny>)>().map_err(|_| {
            PyTypeError::new_err(
                "frames must map each keyframe label to {name: value}, or be a sequence of (label, {name: value}) pairs",
            )
        })?;
        pairs.push((label, row));
    }
    Ok(pairs)
}

/// Whole-number labels roll in the ticker; anything else is shown as text.
fn labels(labels: &[Bound<'_, PyAny>]) -> PyResult<BarRaceLabels> {
    let numeric = labels
        .iter()
        .map(|label| {
            if label.is_instance_of::<PyBool>() {
                return None;
            }
            if label.is_instance_of::<PyInt>() || label.is_instance_of::<PyFloat>() {
                let value = label.extract::<f64>().ok()?;
                return (value.is_finite() && value.fract() == 0.0).then_some(value);
            }
            None
        })
        .collect::<Option<Vec<_>>>();
    Ok(match numeric {
        Some(values) => BarRaceLabels::Numeric(values),
        None => BarRaceLabels::Text(
            labels
                .iter()
                .map(|label| {
                    if let Ok(text) = label.cast::<PyString>() {
                        Ok(text.to_string())
                    } else {
                        Ok(label.str()?.to_string())
                    }
                })
                .collect::<PyResult<_>>()?,
        ),
    })
}

#[pymethods]
impl PyVisualization {
    #[pyo3(signature = (frames, *, top=10, rank_smoothing=0.3, value_format="{:,.0f}", width=10.0, height=6.0, label_width=None, bar_gap=0.18, colors=None, label_color=None, font_size=None, ticker=true, ticker_size=None, ticker_color=None))]
    #[allow(clippy::too_many_arguments)]
    fn bar_race(
        &self,
        py: Python<'_>,
        frames: &Bound<'_, PyAny>,
        top: i64,
        rank_smoothing: f64,
        value_format: &str,
        width: f64,
        height: f64,
        label_width: Option<f64>,
        bar_gap: f64,
        colors: Option<&Bound<'_, PyAny>>,
        label_color: Option<PyColor>,
        font_size: Option<f64>,
        ticker: bool,
        ticker_size: Option<f64>,
        ticker_color: Option<PyColor>,
    ) -> PyResult<Py<PyBarRace>> {
        crate::custom::ensure_authoring_allowed()?;
        let top = usize::try_from(top)
            .ok()
            .filter(|top| *top > 0)
            .ok_or_else(|| PyValueError::new_err("top must be at least 1"))?;
        let value_format = ValueFormat::parse(value_format)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let pairs = keyframes(frames)?;
        // Bars appear in the order their names first occur.
        let mut names: Vec<String> = Vec::new();
        let mut rows = Vec::with_capacity(pairs.len());
        for (_, row) in &pairs {
            let row = row.cast::<PyMapping>().map_err(|_| {
                PyTypeError::new_err("each keyframe must be a mapping of bar name to value")
            })?;
            let mut values = Vec::new();
            for item in row.items()?.try_iter()? {
                let (name, value) = item?.extract::<(String, f64)>().map_err(|_| {
                    PyTypeError::new_err("keyframe entries must map a str name to a number")
                })?;
                if !names.contains(&name) {
                    names.push(name.clone());
                }
                values.push((name, value));
            }
            rows.push(values);
        }
        // A bar missing from a keyframe is zero there.
        let values = rows
            .into_iter()
            .map(|row| {
                let mut values = vec![0.0; names.len()];
                for (name, value) in row {
                    let index = names
                        .iter()
                        .position(|known| *known == name)
                        .expect("name recorded");
                    values[index] = value;
                }
                values
            })
            .collect::<Vec<_>>();
        let labels = labels(
            &pairs
                .iter()
                .map(|(label, _)| label.clone())
                .collect::<Vec<_>>(),
        )?;
        let colors = match colors {
            None => Vec::new(),
            Some(colors)
                if colors.is_instance_of::<PyDict>() || colors.cast::<PyMapping>().is_ok() =>
            {
                let colors = colors
                    .cast::<PyMapping>()
                    .map_err(|_| PyTypeError::new_err("colors must be a mapping or a sequence"))?;
                names
                    .iter()
                    .enumerate()
                    .map(|(index, name)| {
                        Ok(match colors.get_item(name) {
                            Ok(color) => color.extract::<PyColor>()?.0,
                            Err(error) if error.is_instance_of::<PyKeyError>(py) => {
                                BAR_RACE_PALETTE[index % BAR_RACE_PALETTE.len()]
                            }
                            Err(error) => return Err(error),
                        })
                    })
                    .collect::<PyResult<Vec<_>>>()?
            }
            Some(colors) => {
                let colors = colors
                    .try_iter()?
                    .map(|color| Ok(color?.extract::<PyColor>()?.0))
                    .collect::<PyResult<Vec<_>>>()?;
                if colors.is_empty() {
                    return Err(PyValueError::new_err("colors must not be empty"));
                }
                colors
            }
        };
        let options = BarRaceOptions {
            top,
            rank_smoothing,
            value_format,
            width,
            height,
            label_width,
            bar_gap,
            colors,
            label_color: label_color.map(|color| color.0),
            font_size,
            ticker,
            ticker_size,
            ticker_color: ticker_color.map(|color| color.0),
        };
        let race = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .bar_race(labels, names, values, options)
            .map_err(PyValueError::new_err)?;
        Py::new(
            py,
            PyClassInitializer::from(PyDrawable(race.group.clone()))
                .add_subclass(PyBarRace { race }),
        )
    }
}
