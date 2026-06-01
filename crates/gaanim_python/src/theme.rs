use crate::color::PyColor;
use gaanim_core::Theme;
use pyo3::prelude::*;

/// A role-based color theme that defines the visual aesthetic of the scene.
///
/// Contains premium, harmonized color palettes mapping to specific semantic roles
/// (e.g. background, primary text/shapes, secondary accents, muted information).
#[pyclass(name = "Theme", module = "gaanim_core", subclass, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyTheme(pub Theme);

#[pymethods]
#[allow(non_snake_case)]
impl PyTheme {
    /// Creates a custom color theme with specific role mappings.
    #[new]
    fn new(
        background: PyColor,
        primary: PyColor,
        secondary: PyColor,
        accent: PyColor,
        muted: PyColor,
    ) -> Self {
        Self(Theme::new(
            background.0,
            primary.0,
            secondary.0,
            accent.0,
            muted.0,
        ))
    }

    #[getter]
    fn background(&self) -> PyColor {
        PyColor(self.0.background)
    }

    #[getter]
    fn primary(&self) -> PyColor {
        PyColor(self.0.primary)
    }

    #[getter]
    fn secondary(&self) -> PyColor {
        PyColor(self.0.secondary)
    }

    #[getter]
    fn accent(&self) -> PyColor {
        PyColor(self.0.accent)
    }

    #[getter]
    fn muted(&self) -> PyColor {
        PyColor(self.0.muted)
    }

    /// Default premium dark theme (Catppuccin Mocha inspired).
    #[classattr]
    pub fn DARK() -> Self {
        Self(Theme::dark())
    }

    /// Premium high-contrast light theme (Catppuccin Latte inspired).
    #[classattr]
    pub fn LIGHT() -> Self {
        Self(Theme::light())
    }

    /// Classic premium Dracula dark theme.
    #[classattr]
    pub fn DRACULA() -> Self {
        Self(Theme::dracula())
    }

    /// Warm, retro Gruvbox dark theme.
    #[classattr]
    pub fn GRUVBOX() -> Self {
        Self(Theme::gruvbox())
    }
}
