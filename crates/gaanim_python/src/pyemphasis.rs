//! `Drawable.animated_boundary`: a live frame whose stroke cycles through colors.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::color::PyColor;
use crate::pydrawable::PyDrawable;

#[pymethods]
impl PyDrawable {
    /// A live frame around this drawable whose stroke cycles through
    /// `colors`, `cycle_rate` turns through the list per second from the
    /// timeline cursor on. The frame is a new drawable, visible right away.
    #[pyo3(signature = (colors, *, cycle_rate=0.5, width=0.04, padding=None, corner_radius=0.08))]
    fn animated_boundary(
        &self,
        colors: Vec<PyColor>,
        cycle_rate: f64,
        width: f64,
        padding: Option<Bound<'_, PyAny>>,
        corner_radius: f64,
    ) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let padding = crate::pycanvas::surrounding_padding(padding)?;
        let frame = self
            .0
            .animated_boundary(
                colors.into_iter().map(|color| color.0).collect(),
                cycle_rate,
                width,
                padding,
                corner_radius,
            )
            .map_err(PyValueError::new_err)?;
        Ok(PyDrawable(frame.drawable))
    }
}
