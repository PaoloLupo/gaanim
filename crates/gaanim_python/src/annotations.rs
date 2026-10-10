//! `Drawable.annotate` and `TextSelection.annotate`: hand-drawn notations
//! (AN-01) around a drawable or a part of a text.

use gaanim_api::canvas::{BoundsTarget, DrawableHandle};
use gaanim_math::{BracketSides, NotationShape, RoughNotation};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::color::PyColor;
use crate::pydrawable::PyDrawable;
use crate::pytext::PyTextSelection;

/// Hand-drawn notations around one drawable or text selection.
#[pyclass(name = "Annotations", module = "gaanim_core", frozen)]
pub struct PyAnnotations {
    owner: DrawableHandle,
    target: BoundsTarget,
}

impl PyAnnotations {
    #[allow(clippy::too_many_arguments)]
    fn mark(
        &self,
        shape: NotationShape,
        default_padding: [f64; 4],
        color: Option<PyColor>,
        width: Option<f64>,
        roughness: f64,
        passes: u32,
        seed: u64,
        padding: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyDrawable> {
        crate::custom::ensure_authoring_allowed()?;
        let padding = match padding {
            Some(padding) => crate::pycanvas::surrounding_padding(Some(padding))?,
            None => default_padding,
        };
        self.owner
            .rough_notation(
                self.target.clone(),
                RoughNotation {
                    shape,
                    roughness,
                    passes,
                    seed,
                },
                padding,
                color.map(|color| color.0),
                width,
            )
            .map(PyDrawable)
            .map_err(PyValueError::new_err)
    }
}

#[pymethods]
impl PyAnnotations {
    /// A hand-drawn line under the target.
    #[pyo3(signature = (*, color=None, width=None, roughness=1.0, passes=2, seed=0, padding=None))]
    #[allow(clippy::too_many_arguments)]
    fn underline(
        &self,
        color: Option<PyColor>,
        width: Option<f64>,
        roughness: f64,
        passes: u32,
        seed: u64,
        padding: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyDrawable> {
        self.mark(
            NotationShape::Underline,
            [0.0, 0.04, 0.08, 0.04],
            color,
            width,
            roughness,
            passes,
            seed,
            padding,
        )
    }

    /// A hand-drawn box around the target.
    #[pyo3(signature = (*, color=None, width=None, roughness=1.0, passes=2, seed=0, padding=None))]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(name = "box")]
    fn box_(
        &self,
        color: Option<PyColor>,
        width: Option<f64>,
        roughness: f64,
        passes: u32,
        seed: u64,
        padding: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyDrawable> {
        self.mark(
            NotationShape::Box,
            [0.1; 4],
            color,
            width,
            roughness,
            passes,
            seed,
            padding,
        )
    }

    /// A hand-drawn ellipse around the target.
    #[pyo3(signature = (*, color=None, width=None, roughness=1.0, passes=2, seed=0, padding=None))]
    #[allow(clippy::too_many_arguments)]
    fn circle(
        &self,
        color: Option<PyColor>,
        width: Option<f64>,
        roughness: f64,
        passes: u32,
        seed: u64,
        padding: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyDrawable> {
        self.mark(
            NotationShape::Circle,
            [0.25, 0.35, 0.25, 0.35],
            color,
            width,
            roughness,
            passes,
            seed,
            padding,
        )
    }

    /// A hand-drawn line across the middle of the target.
    #[pyo3(signature = (*, color=None, width=None, roughness=1.0, passes=2, seed=0, padding=None))]
    #[allow(clippy::too_many_arguments)]
    fn strike_through(
        &self,
        color: Option<PyColor>,
        width: Option<f64>,
        roughness: f64,
        passes: u32,
        seed: u64,
        padding: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyDrawable> {
        self.mark(
            NotationShape::StrikeThrough,
            [0.0, 0.06, 0.0, 0.06],
            color,
            width,
            roughness,
            passes,
            seed,
            padding,
        )
    }

    /// A hand-drawn cross over the target.
    #[pyo3(signature = (*, color=None, width=None, roughness=1.0, passes=2, seed=0, padding=None))]
    #[allow(clippy::too_many_arguments)]
    fn crossed_off(
        &self,
        color: Option<PyColor>,
        width: Option<f64>,
        roughness: f64,
        passes: u32,
        seed: u64,
        padding: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyDrawable> {
        self.mark(
            NotationShape::CrossedOff,
            [0.05; 4],
            color,
            width,
            roughness,
            passes,
            seed,
            padding,
        )
    }

    /// Hand-drawn square brackets on the chosen sides of the target.
    #[pyo3(signature = (sides=None, *, color=None, width=None, roughness=1.0, passes=2, seed=0, padding=None))]
    #[allow(clippy::too_many_arguments)]
    fn bracket(
        &self,
        sides: Option<Bound<'_, PyAny>>,
        color: Option<PyColor>,
        width: Option<f64>,
        roughness: f64,
        passes: u32,
        seed: u64,
        padding: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyDrawable> {
        let names: Vec<String> = match sides {
            None => vec!["left".to_owned(), "right".to_owned()],
            Some(sides) => match sides.extract::<String>() {
                Ok(name) => vec![name],
                Err(_) => sides.extract()?,
            },
        };
        let mut chosen = BracketSides::default();
        for name in &names {
            match name.as_str() {
                "left" => chosen.left = true,
                "right" => chosen.right = true,
                "top" => chosen.top = true,
                "bottom" => chosen.bottom = true,
                other => {
                    return Err(PyValueError::new_err(format!(
                        "bracket sides are left, right, top, or bottom, got {other:?}"
                    )));
                }
            }
        }
        self.mark(
            NotationShape::Bracket(chosen),
            [0.05, 0.1, 0.05, 0.1],
            color,
            width,
            roughness,
            passes,
            seed,
            padding,
        )
    }
}

#[pymethods]
impl PyDrawable {
    /// Hand-drawn notations around this drawable.
    #[getter]
    fn annotate(&self) -> PyResult<PyAnnotations> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyAnnotations {
            owner: self.0.clone(),
            target: self.0.bounds_target(),
        })
    }
}

#[pymethods]
impl PyTextSelection {
    /// Hand-drawn notations around this part of the text.
    #[getter]
    fn annotate(&self) -> PyResult<PyAnnotations> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(PyAnnotations {
            owner: self.owner().clone(),
            target: self.bounds_target(),
        })
    }
}
