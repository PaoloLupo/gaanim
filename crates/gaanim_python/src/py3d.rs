//! Friendly Python bindings for native PBR primitives.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;

use crate::color::PyColor;
use crate::pydrawable::{resolve_at_target, PyAtTarget, PyCanvasAnim, PyDrawable};
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

    fn require_free_position(&self, operation: &str) -> PyResult<()> {
        if self.handle.layout_owner().is_some() {
            Err(crate::LayoutOwnershipError::new_err(format!(
                "layout owns this Primitive3D's translation; operation: {operation}"
            )))
        } else {
            Ok(())
        }
    }
}

#[pymethods]
impl PyPrimitive3D {
    fn opacity(slf: PyRef<'_, Self>, value: f32) -> PyRef<'_, Self> {
        slf.handle.clone().opacity(value);
        slf
    }

    fn z_index(slf: PyRef<'_, Self>, value: i32) -> PyRef<'_, Self> {
        slf.handle.clone().z_index(value);
        slf
    }

    #[pyo3(signature = (x, y=None, anchor=None))]
    fn at<'py>(
        slf: PyRef<'py, Self>,
        x: &Bound<'_, PyAny>,
        y: Option<f64>,
        anchor: Option<&PyAnchor>,
    ) -> PyResult<PyRef<'py, Self>> {
        slf.require_free_position("at")?;
        match resolve_at_target(x, y, anchor.is_some())? {
            PyAtTarget::Coordinates { x, y } => {
                slf.handle.clone().at_anchor(
                    x,
                    y,
                    anchor.map(|anchor| anchor.0).unwrap_or_default(),
                );
            }
            PyAtTarget::Drawable(reference) => {
                slf.handle.clone().align_to(
                    &reference,
                    gaanim_api::canvas::Anchor::Center,
                    gaanim_api::canvas::Anchor::Center,
                );
            }
        }
        Ok(slf)
    }

    fn at_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyResult<PyRef<'_, Self>> {
        slf.require_free_position("at_3d")?;
        slf.handle.clone().at_3d(x, y, z);
        Ok(slf)
    }

    fn scaled(slf: PyRef<'_, Self>, factor: f64) -> PyRef<'_, Self> {
        slf.handle.clone().scaled(factor);
        slf
    }

    fn scaled_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyRef<'_, Self> {
        slf.handle.clone().scaled_3d(x, y, z);
        slf
    }

    fn rotated(slf: PyRef<'_, Self>, radians: f64) -> PyRef<'_, Self> {
        slf.handle.clone().rotated(radians);
        slf
    }

    fn rotated_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyRef<'_, Self> {
        slf.handle.clone().rotated_3d(x, y, z);
        slf
    }

    fn with_pivot_3d(slf: PyRef<'_, Self>, x: f64, y: f64, z: f64) -> PyRef<'_, Self> {
        slf.handle.clone().with_pivot_3d(x, y, z);
        slf
    }

    fn material(slf: PyRef<'_, Self>, material: PyMaterial3D) -> PyResult<PyRef<'_, Self>> {
        slf.handle
            .clone()
            .material(material.0)
            .map_err(|_| PyTypeError::new_err("material() requires a native Primitive3D"))?;
        Ok(slf)
    }

    fn material_to(&self, material: PyMaterial3D) -> PyResult<PyCanvasAnim> {
        self.handle
            .material_to(material.0)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(|_| PyTypeError::new_err("material_to() requires a native Primitive3D"))
    }

    #[pyo3(signature = (duration=None))]
    fn write(&self, duration: Option<f64>) -> PyResult<PyCanvasAnim> {
        let _ = duration;
        Err(PyTypeError::new_err(
            "write() is a vector-path animation and is not supported by Primitive3D; use create()",
        ))
    }
}
