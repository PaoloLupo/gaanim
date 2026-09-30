//! `scene.character`: the audience's kind of character in a scene.

use gaanim_api::canvas::{CharacterError, CharacterHandle};
use pyo3::{exceptions::PyValueError, prelude::*, pyclass_init::PyClassInitializer};

use crate::pydrawable::PyDrawable;

pub(crate) fn character_error(error: CharacterError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// A character: a drawable that breathes, blinks and plays expressions,
/// made of the parts phones pick their characters from.
#[pyclass(name = "Character", module = "gaanim_core", extends = PyDrawable)]
pub struct PyCharacter {
    pub(crate) inner: CharacterHandle,
}

impl PyCharacter {
    pub(crate) fn create(py: Python<'_>, inner: CharacterHandle) -> PyResult<Py<Self>> {
        Py::new(
            py,
            PyClassInitializer::from(PyDrawable(inner.group.clone())).add_subclass(Self { inner }),
        )
    }
}

#[pymethods]
impl PyCharacter {
    /// Its parts: [body, color, eyes, mouth, extra].
    #[getter]
    fn avatar(&self) -> Vec<usize> {
        self.inner.character().to_vec()
    }

    /// Play `expression` from the cursor: once, or with `loop=True` until
    /// the next one. `None` goes back to the character's own face.
    #[pyo3(signature = (expression=None, *, r#loop=false))]
    fn express(&self, expression: Option<&str>, r#loop: bool) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner
            .express(expression, r#loop)
            .map_err(character_error)
    }

    /// The expressions characters can play.
    #[staticmethod]
    fn expressions() -> Vec<String> {
        gaanim_objects::character::catalog()
            .expressions()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("Character(avatar={:?})", self.inner.character())
    }
}
