//! `Drawable.modifiers`: non-destructive path modifiers whose numbers
//! animate.

use gaanim_api::canvas::{OffsetJoin, PathModifierHandle};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::pydrawable::{PyCanvasAnim, PyDrawable};

/// The modifier stack of a drawable; each method adds one modifier.
#[pyclass(name = "PathModifiers", module = "gaanim_core", frozen)]
pub struct PyPathModifiers {
    drawable: gaanim_api::canvas::DrawableHandle,
}

fn added(result: Result<PathModifierHandle, String>) -> PyResult<PyPathModifier> {
    result
        .map(|inner| PyPathModifier { inner })
        .map_err(PyValueError::new_err)
}

#[pymethods]
impl PyPathModifiers {
    #[pyo3(signature = (size=0.1, ridges=4, *, smooth=false))]
    fn zigzag(&self, size: f64, ridges: u32, smooth: bool) -> PyResult<PyPathModifier> {
        crate::custom::ensure_authoring_allowed()?;
        added(self.drawable.zigzag(size, ridges, smooth))
    }

    fn round_corners(&self, radius: f64) -> PyResult<PyPathModifier> {
        crate::custom::ensure_authoring_allowed()?;
        added(self.drawable.round_corners(radius))
    }

    fn pucker_bloat(&self, amount: f64) -> PyResult<PyPathModifier> {
        crate::custom::ensure_authoring_allowed()?;
        added(self.drawable.pucker_bloat(amount))
    }

    fn twist(&self, angle: f64) -> PyResult<PyPathModifier> {
        crate::custom::ensure_authoring_allowed()?;
        added(self.drawable.twist(angle))
    }

    #[pyo3(signature = (size=0.1, *, detail=6, frequency=1.0, seed=0))]
    fn wiggle_path(
        &self,
        size: f64,
        detail: u32,
        frequency: f64,
        seed: u64,
    ) -> PyResult<PyPathModifier> {
        crate::custom::ensure_authoring_allowed()?;
        added(self.drawable.wiggle_path(size, detail, frequency, seed))
    }

    #[pyo3(signature = (amount, *, join="miter", copies=1))]
    fn offset(&self, amount: f64, join: &str, copies: u32) -> PyResult<PyPathModifier> {
        crate::custom::ensure_authoring_allowed()?;
        let join = match join {
            "miter" => OffsetJoin::Miter,
            "round" => OffsetJoin::Round,
            "bevel" => OffsetJoin::Bevel,
            _ => {
                return Err(PyValueError::new_err(
                    "join must be 'miter', 'round' or 'bevel'",
                ));
            }
        };
        added(self.drawable.offset_path(amount, join, copies))
    }
}

/// One modifier of a stack.
#[pyclass(name = "PathModifier", module = "gaanim_core", frozen)]
pub struct PyPathModifier {
    inner: PathModifierHandle,
}

#[pymethods]
impl PyPathModifier {
    /// Names of the numbers `animate` and `set` change.
    #[getter]
    fn names(&self) -> Vec<&'static str> {
        self.inner.names()
    }

    #[getter]
    fn animate(&self) -> PyPathModifierAnimation {
        PyPathModifierAnimation {
            inner: self.inner.clone(),
        }
    }

    /// Sets numbers from the cursor on: `zz.set(size=0.2)`.
    #[pyo3(signature = (**values))]
    fn set(&self, values: Option<std::collections::HashMap<String, f64>>) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        let mut values: Vec<_> = values.unwrap_or_default().into_iter().collect();
        values.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, value) in values {
            self.inner
                .set(&name, value)
                .map_err(PyValueError::new_err)?;
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!("PathModifier({:?})", self.inner.kind())
    }
}

/// Animations of a modifier's numbers.
#[pyclass(name = "PathModifierAnimation", module = "gaanim_core", frozen)]
pub struct PyPathModifierAnimation {
    inner: PathModifierHandle,
}

impl PyPathModifierAnimation {
    fn to(&self, name: &str, value: f64) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner
            .animate(name, value)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(PyValueError::new_err)
    }
}

#[pymethods]
impl PyPathModifierAnimation {
    fn size(&self, value: f64) -> PyResult<PyCanvasAnim> {
        self.to("size", value)
    }

    fn radius(&self, value: f64) -> PyResult<PyCanvasAnim> {
        self.to("radius", value)
    }

    fn amount(&self, value: f64) -> PyResult<PyCanvasAnim> {
        self.to("amount", value)
    }

    fn angle(&self, value: f64) -> PyResult<PyCanvasAnim> {
        self.to("angle", value)
    }

    fn frequency(&self, value: f64) -> PyResult<PyCanvasAnim> {
        self.to("frequency", value)
    }
}

#[pymethods]
impl PyDrawable {
    /// The drawable's path modifier stack.
    #[getter]
    fn modifiers(&self) -> PyResult<PyPathModifiers> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyPathModifiers {
            drawable: self.0.clone(),
        })
    }
}
