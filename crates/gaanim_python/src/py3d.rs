//! Friendly Python bindings for native PBR primitives.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;

use crate::color::PyColor;
use crate::pydrawable::PyDrawable;
use crate::pylayout::PyAnchor;

#[pyclass(name = "Material3D", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Copy)]
pub struct PyMaterial3D(pub gaanim_scene::Material3D);

#[pymethods]
impl PyMaterial3D {
    #[new]
    #[pyo3(signature = (color=None, roughness=0.55, metallic=0.0, emissive=None, emissive_strength=0.0))]
    fn new(
        color: Option<PyColor>,
        roughness: f32,
        metallic: f32,
        emissive: Option<PyColor>,
        emissive_strength: f32,
    ) -> PyResult<Self> {
        gaanim_scene::Material3D::new(
            color
                .map(|value| value.0)
                .unwrap_or(gaanim_core::peniko::Color::WHITE),
            roughness,
            metallic,
            emissive.map(|value| value.0),
            emissive_strength,
        )
        .map(Self)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    #[staticmethod]
    #[pyo3(signature = (color=None))]
    fn matte(color: Option<PyColor>) -> Self {
        Self(gaanim_scene::Material3D::matte(
            color
                .map(|value| value.0)
                .unwrap_or(gaanim_core::peniko::Color::WHITE),
        ))
    }

    #[staticmethod]
    #[pyo3(signature = (color=None))]
    fn metal(color: Option<PyColor>) -> Self {
        Self(gaanim_scene::Material3D::metal(
            color
                .map(|value| value.0)
                .unwrap_or(gaanim_core::peniko::Color::WHITE),
        ))
    }

    #[staticmethod]
    #[pyo3(signature = (color=None, strength=1.0))]
    fn emissive(color: Option<PyColor>, strength: f32) -> PyResult<Self> {
        gaanim_scene::Material3D::emissive(
            color
                .map(|value| value.0)
                .unwrap_or(gaanim_core::peniko::Color::WHITE),
            strength,
        )
        .map(Self)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
    }

    #[getter]
    fn color(&self) -> PyColor {
        PyColor(self.0.color)
    }

    #[getter]
    fn roughness(&self) -> f32 {
        self.0.roughness
    }

    #[getter]
    fn metallic(&self) -> f32 {
        self.0.metallic
    }

    #[getter]
    fn emissive_color(&self) -> PyColor {
        PyColor(self.0.emissive)
    }

    #[getter]
    fn emissive_strength(&self) -> f32 {
        self.0.emissive_strength
    }
}

#[pyclass(name = "Primitive3D", module = "gaanim_core", extends = PyDrawable, skip_from_py_object)]
#[derive(Clone)]
pub struct PyPrimitive3D {
    handle: gaanim_api::canvas::DrawableHandle,
}

impl PyPrimitive3D {
    pub(crate) fn initializer(
        handle: gaanim_api::canvas::DrawableHandle,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyDrawable(handle.clone())).add_subclass(Self { handle })
    }
}

#[pymethods]
impl PyPrimitive3D {
    fn opacity<'py>(slf: PyRef<'py, Self>, value: &Bound<'_, PyAny>) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).opacity(value)?;
        Ok(slf)
    }

    fn z_index(slf: PyRef<'_, Self>, value: i32) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().z_index(value);
            slf
        })
    }

    #[pyo3(signature = (x, y=None, anchor=None))]
    fn move_to<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: Option<&Bound<'_, PyAny>>,
        anchor: Option<&PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).move_to(x, y, anchor)?;
        Ok(slf)
    }

    fn move_to_3d<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).move_to_3d(x, y, z)?;
        Ok(slf)
    }

    fn scale_to<'py>(
        slf: PyRef<'py, Self>,
        factor: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).scale_to(factor)?;
        Ok(slf)
    }

    fn scale_to_3d<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).scale_to_3d(x, y, z)?;
        Ok(slf)
    }

    fn rotate_to<'py>(
        slf: PyRef<'py, Self>,
        radians: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).rotate_to(radians)?;
        Ok(slf)
    }

    fn rotate_to_3d<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: &Bound<'_, PyAny>,
        z: &Bound<'_, PyAny>,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        PyDrawable(slf.handle.clone()).rotate_to_3d(x, y, z)?;
        Ok(slf)
    }

    fn with_pivot_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        Ok({
            slf.handle.clone().with_pivot_3d(x, y, z);
            slf
        })
    }

    fn material(slf: PyRef<'_, Self>, material: PyMaterial3D) -> PyResult<PyRef<'_, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        slf.handle
            .clone()
            .material(material.0)
            .map_err(|_| PyTypeError::new_err("material() requires a native Primitive3D"))?;
        Ok(slf)
    }
}
