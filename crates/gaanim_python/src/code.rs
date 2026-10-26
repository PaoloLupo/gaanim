//! Code blocks whose source changes: `scene.text.code` returns a `Code`.
use std::sync::{Arc, Mutex};

use gaanim_api::canvas::{CodeError, CodeHandle, SceneModel};
use pyo3::{
    exceptions::{PyTypeError, PyValueError},
    prelude::*,
    pyclass_init::PyClassInitializer,
};

use crate::pydrawable::{PyCanvasAnim, PyDrawable};

/// A code block from `scene.text.code`: a `Drawable` whose source can change.
#[pyclass(name = "Code", module = "gaanim_core", extends = PyDrawable)]
pub struct PyCode {
    code: CodeHandle,
    scene: Arc<Mutex<SceneModel>>,
}

/// Animation proxy of a `Code`: an `Anim` with `edit`, `select` and
/// `highlight`.
#[pyclass(name = "CodeAnimation", module = "gaanim_core", extends = PyCanvasAnim)]
pub struct PyCodeAnimation {
    code: CodeHandle,
    scene: Arc<Mutex<SceneModel>>,
}

fn code_error(error: CodeError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// `lines` as `(first, last)`: one line number, or a pair of them.
fn line_span(lines: Option<&Bound<'_, PyAny>>) -> PyResult<Option<(usize, usize)>> {
    let Some(lines) = lines else {
        return Ok(None);
    };
    if let Ok(line) = lines.extract::<usize>() {
        return Ok(Some((line, line)));
    }
    lines.extract::<(usize, usize)>().map(Some).map_err(|_| {
        PyTypeError::new_err("lines must be a line number or a (first, last) pair of them")
    })
}

impl PyCode {
    pub(crate) fn create(
        py: Python<'_>,
        code: CodeHandle,
        scene: Arc<Mutex<SceneModel>>,
    ) -> PyResult<Py<Self>> {
        let drawable = PyDrawable(code.group.clone());
        Py::new(
            py,
            PyClassInitializer::from(drawable).add_subclass(Self { code, scene }),
        )
    }
}

#[pymethods]
impl PyCode {
    /// The source the block shows after its latest edit.
    #[getter]
    fn source(&self) -> PyResult<String> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.code.source())
    }

    /// The language the block highlights.
    #[getter]
    fn language(&self) -> PyResult<String> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.code.language().to_owned())
    }

    /// The number of lines of the current source.
    #[getter]
    fn line_count(&self) -> PyResult<usize> {
        crate::custom::ensure_authoring_allowed()?;
        Ok(self.code.line_count())
    }

    /// Show `source` from the cursor, at once, and return this Code.
    fn edit<'py>(slf: PyRef<'py, Self>, source: &str) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        {
            let mut scene = slf.scene.lock().expect("scene canvas poisoned");
            slf.code.edit(&mut scene, source).map_err(code_error)?;
        }
        Ok(slf)
    }

    /// Dim every line outside `lines` and the occurrences of `text` at once,
    /// and return this Code.
    #[pyo3(signature = (lines=None, *, text=None, dim=0.3))]
    fn select<'py>(
        slf: PyRef<'py, Self>,
        lines: Option<&Bound<'_, PyAny>>,
        text: Option<&str>,
        dim: f32,
    ) -> PyResult<PyRef<'py, Self>> {
        crate::custom::ensure_authoring_allowed()?;
        let ranges = slf
            .code
            .ranges(line_span(lines)?, text)
            .map_err(code_error)?;
        slf.code.select(ranges, dim).map_err(code_error)?;
        Ok(slf)
    }

    /// Animation proxy: the `Drawable.animate` vocabulary plus `edit`,
    /// `select` and `highlight`.
    #[getter]
    fn animate(&self, py: Python<'_>) -> PyResult<Py<PyCodeAnimation>> {
        crate::custom::ensure_authoring_allowed()?;
        let base = PyCanvasAnim {
            inner: self.code.group.animate(),
        };
        Py::new(
            py,
            PyClassInitializer::from(base).add_subclass(PyCodeAnimation {
                code: self.code.clone(),
                scene: self.scene.clone(),
            }),
        )
    }
}

#[pymethods]
impl PyCodeAnimation {
    /// Edit the block to `source`: kept tokens slide, the others fade.
    fn edit(&self, source: &str) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        let mut scene = self.scene.lock().expect("scene canvas poisoned");
        self.code
            .edit_anim(&mut scene, source, None)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(code_error)
    }

    /// Fade every line outside `lines` and the occurrences of `text` to
    /// `dim`, and what they cover back to full opacity.
    #[pyo3(signature = (lines=None, *, text=None, dim=0.3))]
    fn select(
        &self,
        lines: Option<&Bound<'_, PyAny>>,
        text: Option<&str>,
        dim: f32,
    ) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        let ranges = self
            .code
            .ranges(line_span(lines)?, text)
            .map_err(code_error)?;
        self.code
            .select_anim(ranges, dim, None)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(code_error)
    }

    /// Tint `lines` and the occurrences of `text` with the theme's accent
    /// and back.
    #[pyo3(signature = (text=None, *, lines=None))]
    fn highlight(
        &self,
        text: Option<&str>,
        lines: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyCanvasAnim> {
        crate::custom::ensure_authoring_allowed()?;
        let lines = line_span(lines)?;
        if text.is_none() && lines.is_none() {
            return Err(PyValueError::new_err("highlight() needs text or lines"));
        }
        let ranges = self.code.ranges(lines, text).map_err(code_error)?;
        self.code
            .effect_anim(
                ranges,
                gaanim_api::anim::TextSelectionEffect::Highlight,
                None,
            )
            .map(|inner| PyCanvasAnim { inner })
            .map_err(code_error)
    }
}
