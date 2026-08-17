use gaanim_core::peniko::{self, Brush, Extend, Gradient};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::color::PyColor;

/// Full scene-bounds paint, including timeline-driven custom WGSL shaders.
#[pyclass(name = "Background", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone, Debug)]
pub struct PyBackground(pub gaanim_api::canvas::BackgroundPaint);

/// Accept a Background, Brush, or any value accepted by Color.
#[derive(Clone, Debug)]
pub struct PyBackgroundInput(pub gaanim_api::canvas::BackgroundPaint);

impl<'a, 'py> FromPyObject<'a, 'py> for PyBackgroundInput {
    type Error = PyErr;

    fn extract(obj: pyo3::Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(background) = obj.cast::<PyBackground>() {
            return Ok(Self(background.borrow().0.clone()));
        }
        PyPaint::extract(obj).map(|paint| Self(gaanim_api::canvas::BackgroundPaint::Brush(paint.0)))
    }
}

#[pymethods]
impl PyBackground {
    #[new]
    fn new(paint: PyPaint) -> Self {
        Self(gaanim_api::canvas::BackgroundPaint::Brush(paint.0))
    }

    /// Build a WGSL background evaluated with exact timeline time.
    #[staticmethod]
    #[pyo3(signature = (source, *, fallback=None))]
    fn shader(source: String, fallback: Option<PyColor>) -> PyResult<Self> {
        let fallback = fallback.map_or(peniko::Color::BLACK, |color| color.0);
        gaanim_api::canvas::ShaderBackground::new(source, fallback)
            .map(gaanim_api::canvas::BackgroundPaint::Shader)
            .map(Self)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    #[getter]
    fn fallback(&self) -> PyColor {
        PyColor(self.0.fallback_color())
    }

    fn __repr__(&self) -> &'static str {
        match &self.0 {
            gaanim_api::canvas::BackgroundPaint::Brush(_) => "Background(...)",
            gaanim_api::canvas::BackgroundPaint::Shader(_) => "Background.shader(...)",
        }
    }
}

/// A reusable solid or gradient paint accepted by drawables and scene backgrounds.
#[pyclass(name = "Brush", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone, Debug)]
pub struct PyBrush(pub Brush);

/// Accept either a Brush or any value accepted by Color.
#[derive(Clone, Debug)]
pub struct PyPaint(pub Brush);

impl<'a, 'py> FromPyObject<'a, 'py> for PyPaint {
    type Error = PyErr;

    fn extract(obj: pyo3::Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(brush) = obj.cast::<PyBrush>() {
            return Ok(Self(brush.borrow().0.clone()));
        }
        PyColor::extract(obj).map(|color| Self(Brush::Solid(color.0)))
    }
}

#[pymethods]
impl PyBrush {
    #[staticmethod]
    fn solid(color: PyColor) -> Self {
        Self(Brush::Solid(color.0))
    }

    /// Linear gradient in the drawable's local coordinates.
    #[staticmethod]
    #[pyo3(signature = (colors, *, start, end, extend="pad"))]
    fn linear(
        colors: Vec<PyColor>,
        start: (f64, f64),
        end: (f64, f64),
        extend: &str,
    ) -> PyResult<Self> {
        validate_point("start", start)?;
        validate_point("end", end)?;
        if start == end {
            return Err(PyValueError::new_err(
                "linear gradient start and end must differ",
            ));
        }
        let stops = uniform_stops(colors)?;
        let gradient = Gradient::new_linear(start, end)
            .with_extend(parse_extend(extend)?)
            .with_stops(stops.as_slice());
        Ok(Self(Brush::Gradient(gradient)))
    }

    /// Radial gradient in the drawable's local coordinates.
    #[staticmethod]
    #[pyo3(signature = (colors, *, center=(0.0, 0.0), radius, extend="pad"))]
    fn radial(
        colors: Vec<PyColor>,
        center: (f64, f64),
        radius: f64,
        extend: &str,
    ) -> PyResult<Self> {
        validate_point("center", center)?;
        if !radius.is_finite() || radius <= 0.0 || radius > f32::MAX as f64 {
            return Err(PyValueError::new_err(
                "radial gradient radius must be finite and positive",
            ));
        }
        let stops = uniform_stops(colors)?;
        let gradient = Gradient::new_radial(center, radius as f32)
            .with_extend(parse_extend(extend)?)
            .with_stops(stops.as_slice());
        Ok(Self(Brush::Gradient(gradient)))
    }

    /// Angular gradient. Public angles are degrees for presentation ergonomics.
    #[staticmethod]
    #[pyo3(signature = (colors, *, center=(0.0, 0.0), start_angle=0.0, end_angle=360.0, extend="pad"))]
    fn sweep(
        colors: Vec<PyColor>,
        center: (f64, f64),
        start_angle: f64,
        end_angle: f64,
        extend: &str,
    ) -> PyResult<Self> {
        validate_point("center", center)?;
        if !start_angle.is_finite() || !end_angle.is_finite() || start_angle == end_angle {
            return Err(PyValueError::new_err(
                "sweep gradient angles must be finite and differ",
            ));
        }
        let stops = uniform_stops(colors)?;
        let gradient = Gradient::new_sweep(
            center,
            start_angle.to_radians() as f32,
            end_angle.to_radians() as f32,
        )
        .with_extend(parse_extend(extend)?)
        .with_stops(stops.as_slice());
        Ok(Self(Brush::Gradient(gradient)))
    }

    fn __repr__(&self) -> &'static str {
        match &self.0 {
            Brush::Solid(_) => "Brush.solid(...)",
            Brush::Gradient(_) => "Brush.gradient(...)",
            Brush::Image(_) => "Brush.image(...)",
        }
    }
}

fn uniform_stops(colors: Vec<PyColor>) -> PyResult<Vec<(f32, peniko::Color)>> {
    if colors.len() < 2 {
        return Err(PyValueError::new_err(
            "a gradient requires at least two colors",
        ));
    }
    let denominator = (colors.len() - 1) as f32;
    Ok(colors
        .into_iter()
        .enumerate()
        .map(|(index, color)| (index as f32 / denominator, color.0))
        .collect())
}

fn parse_extend(value: &str) -> PyResult<Extend> {
    match value {
        "pad" => Ok(Extend::Pad),
        "repeat" => Ok(Extend::Repeat),
        "reflect" => Ok(Extend::Reflect),
        _ => Err(PyValueError::new_err(
            "gradient extend must be 'pad', 'repeat', or 'reflect'",
        )),
    }
}

fn validate_point(name: &str, point: (f64, f64)) -> PyResult<()> {
    if point.0.is_finite() && point.1.is_finite() {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "{name} coordinates must be finite"
        )))
    }
}
