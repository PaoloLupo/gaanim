//! Python bindings for axonometric drawings (`scene.geometry.axonometric`).

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use gaanim_api::canvas::{Axonometric, AxonometricView};
use gaanim_core::glam::{DVec2, DVec3};

use crate::composition::PyComposition;
use crate::pydrawable::PyDrawable;

/// A model point given as `(x, y, z)`.
type ModelPoint = (f64, f64, f64);

fn model(points: &[ModelPoint]) -> Vec<DVec3> {
    points
        .iter()
        .map(|&(x, y, z)| DVec3::new(x, y, z))
        .collect()
}

/// The view `scene.geometry.axonometric` describes with these arguments.
pub(crate) fn axonometric_view(
    view: &str,
    azimuth: Option<f64>,
    elevation: Option<f64>,
    origin: (f64, f64),
    scale: f64,
) -> PyResult<AxonometricView> {
    let (preset_azimuth, preset_elevation) = AxonometricView::preset(view).ok_or_else(|| {
        PyValueError::new_err(format!(
            "unknown view {view:?}; use 'isometric', 'dimetric', 'plan', 'front' or 'side'"
        ))
    })?;
    AxonometricView::new(
        azimuth.unwrap_or(preset_azimuth),
        elevation.unwrap_or(preset_elevation),
        DVec2::new(origin.0, origin.1),
        scale,
    )
    .map_err(PyValueError::new_err)
}

/// An axonometric view of a model and the drawings made with it.
#[pyclass(
    name = "Axonometric",
    module = "gaanim_core",
    frozen,
    skip_from_py_object
)]
pub struct PyAxonometric {
    pub(crate) inner: Axonometric,
}

#[pymethods]
impl PyAxonometric {
    /// Radians the view turns about `z` from the front view.
    #[getter]
    fn azimuth(&self) -> f64 {
        self.inner.view().azimuth
    }

    /// Radians the view rises above the horizon.
    #[getter]
    fn elevation(&self) -> f64 {
        self.inner.view().elevation
    }

    /// Scene point where the model origin is drawn.
    #[getter]
    fn origin(&self) -> (f64, f64) {
        let origin = self.inner.view().origin;
        (origin.x, origin.y)
    }

    /// Scene units per model unit along the least shortened axis.
    #[getter]
    fn scale(&self) -> f64 {
        self.inner.view().scale
    }

    /// Where the model point `(x, y, z)` is drawn.
    fn point(&self, x: f64, y: f64, z: f64) -> (f64, f64) {
        let point = self.inner.point(DVec3::new(x, y, z));
        (point.x, point.y)
    }

    /// How near the viewer the model point `(x, y, z)` is.
    fn depth(&self, x: f64, y: f64, z: f64) -> f64 {
        self.inner.depth(DVec3::new(x, y, z))
    }

    fn polygon(&self, points: Vec<ModelPoint>) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner
            .polygon(&model(&points))
            .map(PyDrawable)
            .map_err(PyValueError::new_err)
    }

    #[pyo3(signature = (points, *, closed=false))]
    fn polyline(&self, points: Vec<ModelPoint>, closed: bool) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner
            .polyline(&model(&points), closed)
            .map(PyDrawable)
            .map_err(PyValueError::new_err)
    }

    fn line(&self, start: ModelPoint, end: ModelPoint) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let [start, end] = [start, end].map(|(x, y, z)| DVec3::new(x, y, z));
        self.inner
            .line(start, end)
            .map(PyDrawable)
            .map_err(PyValueError::new_err)
    }

    #[pyo3(signature = (drawables, *, z_index=0, view=None))]
    fn depth_sort(
        &self,
        drawables: Vec<PyDrawable>,
        z_index: i32,
        view: Option<PyRef<'_, PyAxonometric>>,
    ) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let drawables: Vec<_> = drawables.into_iter().map(|drawable| drawable.0).collect();
        let view = view.map_or_else(|| self.inner.view(), |other| other.inner.view());
        self.inner
            .depth_sort_in(&drawables, z_index, view)
            .map(drop)
            .map_err(PyValueError::new_err)
    }

    fn animate_to(&self, other: &PyAxonometric) -> PyResult<PyComposition> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner
            .animate_to(&other.inner)
            .map(|inner| PyComposition { inner })
            .map_err(PyValueError::new_err)
    }
}
