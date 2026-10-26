//! The active theme's colors by role: `scene.colors.accent`.
use std::sync::{Arc, Mutex};

use gaanim_api::canvas::SceneModel as ApiCanvas;
use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;

use crate::color::{PyColor, PyPalette};
use crate::pycanvas::PyScene;

/// The colors of the scene's active theme, by role, from `scene.colors`.
#[pyclass(name = "ThemeColors", module = "gaanim_core", frozen)]
pub struct PyThemeColors {
    inner: Arc<Mutex<ApiCanvas>>,
}

impl PyThemeColors {
    /// The color of `role` in the theme active now.
    fn role(&self, role: &str) -> PyResult<PyColor> {
        crate::custom::ensure_authoring_allowed()?;
        let canvas = self.inner.lock().expect("scene canvas poisoned");
        let theme = canvas
            .theme_style
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("no theme is active on this scene"))?;
        theme
            .colors
            .get(role)
            .copied()
            .map(PyColor)
            .ok_or_else(|| PyKeyError::new_err(format!("the theme has no color {role:?}")))
    }

    /// The theme's color roles, in name order; none without a theme.
    fn roles(&self) -> Vec<String> {
        let canvas = self.inner.lock().expect("scene canvas poisoned");
        let mut roles: Vec<String> = canvas
            .theme_style
            .as_ref()
            .map(|theme| theme.colors.keys().cloned().collect())
            .unwrap_or_default();
        roles.sort();
        roles
    }
}

#[pymethods]
impl PyThemeColors {
    /// The frame's background.
    #[getter]
    fn background(&self) -> PyResult<PyColor> {
        self.role("background")
    }

    /// Text and the default ink; `primary` and `body` are the same color.
    #[getter]
    fn foreground(&self) -> PyResult<PyColor> {
        self.role("foreground")
    }

    #[getter]
    fn primary(&self) -> PyResult<PyColor> {
        self.role("primary")
    }

    #[getter]
    fn body(&self) -> PyResult<PyColor> {
        self.role("body")
    }

    /// Secondary text and quiet marks; `secondary` is the same color.
    #[getter]
    fn muted(&self) -> PyResult<PyColor> {
        self.role("muted")
    }

    #[getter]
    fn secondary(&self) -> PyResult<PyColor> {
        self.role("secondary")
    }

    /// Titles.
    #[getter]
    fn title(&self) -> PyResult<PyColor> {
        self.role("title")
    }

    /// The color that draws the eye.
    #[getter]
    fn accent(&self) -> PyResult<PyColor> {
        self.role("accent")
    }

    /// Chart marks without a color of their own.
    #[getter]
    fn chart(&self) -> PyResult<PyColor> {
        self.role("chart")
    }

    /// Panels and cards behind content.
    #[getter]
    fn panel(&self) -> PyResult<PyColor> {
        self.role("panel")
    }

    /// Headers of tables and panels.
    #[getter]
    fn header(&self) -> PyResult<PyColor> {
        self.role("header")
    }

    /// Rules, grids and borders; `border` is the same color.
    #[getter]
    fn rule(&self) -> PyResult<PyColor> {
        self.role("rule")
    }

    #[getter]
    fn border(&self) -> PyResult<PyColor> {
        self.role("border")
    }

    /// Good news.
    #[getter]
    fn success(&self) -> PyResult<PyColor> {
        self.role("success")
    }

    /// Caution.
    #[getter]
    fn warning(&self) -> PyResult<PyColor> {
        self.role("warning")
    }

    /// Bad news and errors.
    #[getter]
    fn danger(&self) -> PyResult<PyColor> {
        self.role("danger")
    }

    /// Drop shadows.
    #[getter]
    fn shadow(&self) -> PyResult<PyColor> {
        self.role("shadow")
    }

    /// The categorical palette that colors chart categories.
    #[getter]
    fn palette(&self) -> PyResult<PyPalette> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyPalette(
            self.inner
                .lock()
                .expect("scene canvas poisoned")
                .categorical_palette(),
        ))
    }

    /// The color of any role, the theme's own included: `colors["brand"]`.
    fn __getitem__(&self, role: &str) -> PyResult<PyColor> {
        self.role(role)
    }

    fn __contains__(&self, role: &str) -> bool {
        self.roles().iter().any(|name| name == role)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::custom::ensure_authoring_allowed()?;
        let roles = pyo3::types::PyList::new(py, self.roles())?;
        Ok(roles.try_iter()?.into_any().unbind())
    }

    fn __len__(&self) -> usize {
        self.roles().len()
    }

    fn __repr__(&self) -> String {
        format!("ThemeColors({})", self.roles().join(", "))
    }
}

#[pymethods]
impl PyScene {
    /// The active theme's colors by role: `scene.colors.accent`.
    #[getter]
    fn colors(&self) -> PyResult<PyThemeColors> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyThemeColors {
            inner: self.inner.clone(),
        })
    }
}
