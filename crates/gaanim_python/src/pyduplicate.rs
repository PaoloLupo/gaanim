//! `Distribution` and the repeat/duplicate geometry helpers.

use gaanim_core::glam::DVec2;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::pydrawable::PyDrawable;

/// Where `Geometry.duplicate` places the copies of a drawable.
#[pyclass(
    name = "Distribution",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
#[derive(Clone, Debug)]
pub struct PyDistribution(pub gaanim_api::canvas::Distribution);

fn checked(distribution: gaanim_api::canvas::Distribution) -> PyResult<PyDistribution> {
    distribution.validate().map_err(PyValueError::new_err)?;
    Ok(PyDistribution(distribution))
}

fn spacing_of(value: &Bound<'_, PyAny>) -> PyResult<DVec2> {
    if let Ok(spacing) = value.extract::<f64>() {
        return Ok(DVec2::splat(spacing));
    }
    let (x, y) = value.extract::<(f64, f64)>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("spacing must be a number or an (x, y) pair")
    })?;
    Ok(DVec2::new(x, y))
}

#[pymethods]
impl PyDistribution {
    /// `columns` x `rows` cells, centred on `center`, filled row by row.
    #[staticmethod]
    #[pyo3(signature = (columns, rows, spacing=None, *, center=(0.0, 0.0)))]
    fn grid(
        columns: usize,
        rows: usize,
        spacing: Option<&Bound<'_, PyAny>>,
        center: (f64, f64),
    ) -> PyResult<Self> {
        let spacing = spacing.map_or(Ok(DVec2::splat(0.5)), spacing_of)?;
        checked(gaanim_api::canvas::Distribution::Grid {
            columns,
            rows,
            spacing,
            center: DVec2::new(center.0, center.1),
        })
    }

    /// `count` points on a circle, counter-clockwise from `start` radians.
    #[staticmethod]
    #[pyo3(signature = (count, radius=2.0, *, center=(0.0, 0.0), start=0.0, orient=false))]
    fn circle(
        count: usize,
        radius: f64,
        center: (f64, f64),
        start: f64,
        orient: bool,
    ) -> PyResult<Self> {
        checked(gaanim_api::canvas::Distribution::Circle {
            count,
            radius,
            center: DVec2::new(center.0, center.1),
            start,
            orient,
        })
    }

    /// `count` points evenly spaced along a polyline, both ends included.
    #[staticmethod]
    #[pyo3(signature = (points, count, *, orient=false))]
    fn along(points: Vec<(f64, f64)>, count: usize, orient: bool) -> PyResult<Self> {
        checked(gaanim_api::canvas::Distribution::Along {
            points: points.into_iter().map(|(x, y)| DVec2::new(x, y)).collect(),
            count,
            orient,
        })
    }

    /// `count` seeded uniform points inside `(xmin, ymin, xmax, ymax)`.
    #[staticmethod]
    #[pyo3(signature = (count, bounds=(-6.0, -3.0, 6.0, 3.0), *, seed=0))]
    fn random(count: usize, bounds: (f64, f64, f64, f64), seed: u64) -> PyResult<Self> {
        checked(gaanim_api::canvas::Distribution::Random {
            count,
            min: DVec2::new(bounds.0, bounds.1),
            max: DVec2::new(bounds.2, bounds.3),
            seed,
        })
    }

    /// `count` points on a sunflower spiral, `spacing * sqrt(i)` from `center`.
    #[staticmethod]
    #[pyo3(signature = (count, spacing=0.2, *, center=(0.0, 0.0)))]
    fn phyllotaxis(count: usize, spacing: f64, center: (f64, f64)) -> PyResult<Self> {
        checked(gaanim_api::canvas::Distribution::Phyllotaxis {
            count,
            spacing,
            center: DVec2::new(center.0, center.1),
        })
    }

    /// Positions of the copies, in order.
    #[getter]
    fn points(&self) -> Vec<(f64, f64)> {
        self.0
            .placements()
            .into_iter()
            .map(|(point, _)| (point.x, point.y))
            .collect()
    }

    fn __len__(&self) -> usize {
        self.0.placements().len()
    }

    fn __repr__(&self) -> String {
        format!("Distribution({} points)", self.0.placements().len())
    }
}

pub(crate) fn repeat(
    canvas: &std::sync::Arc<std::sync::Mutex<gaanim_api::canvas::SceneModel>>,
    shape: &PyDrawable,
    count: usize,
    step: gaanim_api::canvas::RepeatStep,
) -> PyResult<PyDrawable> {
    canvas
        .lock()
        .expect("scene canvas poisoned")
        .repeat(&shape.0, count, step)
        .map(PyDrawable)
        .map_err(PyValueError::new_err)
}

pub(crate) fn duplicate(
    canvas: &std::sync::Arc<std::sync::Mutex<gaanim_api::canvas::SceneModel>>,
    shape: &PyDrawable,
    distribution: &PyDistribution,
) -> PyResult<PyDrawable> {
    canvas
        .lock()
        .expect("scene canvas poisoned")
        .duplicate(&shape.0, &distribution.0)
        .map(PyDrawable)
        .map_err(PyValueError::new_err)
}
