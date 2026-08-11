use std::collections::HashMap;

use gaanim_api::canvas::{ThemePaint, ThemeStrokeStyle, ThemeStyle};
use gaanim_core::kurbo::{Cap, Join, Stroke};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

use crate::brush::PyBrush;
use crate::color::PyColor;
use crate::pytext::PyTextStyle;

fn paint_from_python(value: &Bound<'_, PyAny>) -> PyResult<ThemePaint> {
    if let Ok(brush) = value.extract::<PyRef<'_, PyBrush>>() {
        return Ok(ThemePaint::Brush(brush.0.clone()));
    }
    if let Ok(color) = value.extract::<PyRef<'_, PyColor>>() {
        return Ok(ThemePaint::Color(color.0));
    }
    if let Ok(name) = value.extract::<String>() {
        return Ok(ThemePaint::Named(name));
    }
    if let Ok(color) = value.extract::<PyColor>() {
        return Ok(ThemePaint::Color(color.0));
    }
    Err(PyTypeError::new_err(
        "paint must be a Color, Brush, CSS color, or theme token name",
    ))
}

fn parse_cap(value: &str) -> PyResult<Cap> {
    match value {
        "butt" => Ok(Cap::Butt),
        "round" => Ok(Cap::Round),
        "square" => Ok(Cap::Square),
        _ => Err(PyValueError::new_err(
            "cap must be 'butt', 'round', or 'square'",
        )),
    }
}

fn parse_join(value: &str) -> PyResult<Join> {
    match value {
        "bevel" => Ok(Join::Bevel),
        "miter" => Ok(Join::Miter),
        "round" => Ok(Join::Round),
        _ => Err(PyValueError::new_err(
            "join must be 'bevel', 'miter', or 'round'",
        )),
    }
}

#[pyclass(name = "StrokeStyle", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyStrokeStyle(pub ThemeStrokeStyle);

#[pymethods]
impl PyStrokeStyle {
    #[new]
    #[pyo3(signature = (paint, width=2.0, *, cap="round", join="round", miter_limit=4.0, dashes=Vec::new(), dash_offset=0.0))]
    fn new(
        paint: &Bound<'_, PyAny>,
        width: f64,
        cap: &str,
        join: &str,
        miter_limit: f64,
        dashes: Vec<f64>,
        dash_offset: f64,
    ) -> PyResult<Self> {
        let cap = parse_cap(cap)?;
        let mut stroke = Stroke::new(width);
        stroke.start_cap = cap;
        stroke.end_cap = cap;
        stroke.join = parse_join(join)?;
        stroke.miter_limit = miter_limit;
        stroke.dash_pattern.extend(dashes);
        stroke.dash_offset = dash_offset;
        let result = ThemeStrokeStyle {
            paint: paint_from_python(paint)?,
            style: stroke,
        };
        result.validate().map_err(PyValueError::new_err)?;
        Ok(Self(result))
    }
}

#[pyclass(name = "Style", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyStyle(pub ThemeStyle);

#[pymethods]
impl PyStyle {
    #[new]
    #[pyo3(signature = (*, fill=None, stroke=None, opacity=None, text=None))]
    fn new(
        fill: Option<&Bound<'_, PyAny>>,
        stroke: Option<PyStrokeStyle>,
        opacity: Option<f32>,
        text: Option<PyTextStyle>,
    ) -> PyResult<Self> {
        let result = ThemeStyle {
            fill: fill.map(paint_from_python).transpose()?,
            stroke: stroke.map(|stroke| stroke.0),
            opacity,
            text: text.map(|text| text.0),
        };
        result.validate().map_err(PyValueError::new_err)?;
        Ok(Self(result))
    }
}

/// Axes-part rules expanded under the selector supplied by `Theme.styles`.
#[pyclass(name = "AxesStyle", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Default)]
pub struct PyAxesStyle {
    axis: Option<ThemeStrokeStyle>,
    grid: Option<ThemeStrokeStyle>,
    minor_grid: Option<ThemeStrokeStyle>,
    ticks: Option<ThemeStrokeStyle>,
    numbers: Option<gaanim_text::prelude::TextStyle>,
    labels: Option<gaanim_text::prelude::TextStyle>,
}

#[pymethods]
impl PyAxesStyle {
    #[new]
    #[pyo3(signature = (*, axis=None, grid=None, minor_grid=None, ticks=None, numbers=None, labels=None))]
    fn new(
        axis: Option<PyStrokeStyle>,
        grid: Option<PyStrokeStyle>,
        minor_grid: Option<PyStrokeStyle>,
        ticks: Option<PyStrokeStyle>,
        numbers: Option<PyTextStyle>,
        labels: Option<PyTextStyle>,
    ) -> Self {
        Self {
            axis: axis.map(|style| style.0),
            grid: grid.map(|style| style.0),
            minor_grid: minor_grid.map(|style| style.0),
            ticks: ticks.map(|style| style.0),
            numbers: numbers.map(|style| style.0),
            labels: labels.map(|style| style.0),
        }
    }
}

impl PyAxesStyle {
    pub(crate) fn expand(&self, selector: &str) -> HashMap<String, ThemeStyle> {
        let mut result = HashMap::new();
        for (part, stroke) in [
            ("axis", &self.axis),
            ("grid", &self.grid),
            ("minor_grid", &self.minor_grid),
            ("ticks", &self.ticks),
        ] {
            if let Some(stroke) = stroke {
                result.insert(
                    format!("{selector}/{part}"),
                    ThemeStyle {
                        stroke: Some(stroke.clone()),
                        ..Default::default()
                    },
                );
            }
        }
        for (part, text) in [("numbers", &self.numbers), ("labels", &self.labels)] {
            if let Some(text) = text {
                result.insert(
                    format!("{selector}/{part}"),
                    ThemeStyle {
                        text: Some(text.clone()),
                        ..Default::default()
                    },
                );
            }
        }
        result
    }
}
