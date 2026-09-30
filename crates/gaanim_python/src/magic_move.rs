//! `magic_move(before, after, key=..., unmatched=...)`: keyed morph between
//! two states of a drawable hierarchy.

use gaanim_api::canvas::{MagicMoveFailure, MagicMoveKey, MagicMoveUnmatched};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

use crate::pydrawable::{PyCanvasAnim, PyDrawable};

pub(crate) fn parse_key(key: &str) -> PyResult<MagicMoveKey> {
    MagicMoveKey::parse(key).ok_or_else(|| {
        PyValueError::new_err(format!(
            "invalid magic_move key {key:?}; use \"name\", \"id\" or a callable"
        ))
    })
}

fn parse_unmatched(unmatched: &str) -> PyResult<MagicMoveUnmatched> {
    MagicMoveUnmatched::parse(unmatched).ok_or_else(|| {
        PyValueError::new_err(format!(
            "invalid unmatched mode {unmatched:?}; use \"fade\" or \"cut\""
        ))
    })
}

/// Key returned by a Python callable: `None` (or an empty string) means no
/// key, strings are used as they are and other values through `str()`.
fn callable_key(value: &Bound<'_, PyAny>) -> PyResult<Option<String>> {
    if value.is_none() {
        return Ok(None);
    }
    let key = match value.extract::<String>() {
        Ok(key) => key,
        Err(_) => value.str()?.to_string(),
    };
    Ok((!key.is_empty()).then_some(key))
}

/// Keyed "magic move" from `before` to `after`.
#[pyfunction]
#[pyo3(signature = (before, after, *, key=None, unmatched="fade", duration=1.0))]
pub fn magic_move(
    py: Python<'_>,
    before: PyRef<'_, PyDrawable>,
    after: PyRef<'_, PyDrawable>,
    key: Option<&Bound<'_, PyAny>>,
    unmatched: &str,
    duration: f64,
) -> PyResult<PyCanvasAnim> {
    crate::custom::ensure_authoring_allowed()?;
    let unmatched = parse_unmatched(unmatched)?;
    let result = match key {
        None => before
            .0
            .magic_move_to(&after.0, MagicMoveKey::Name, unmatched, duration)
            .map_err(|error| PyValueError::new_err(error.to_string())),
        Some(key) if key.is_instance_of::<pyo3::types::PyString>() => {
            let key = parse_key(&key.extract::<String>()?)?;
            before
                .0
                .magic_move_to(&after.0, key, unmatched, duration)
                .map_err(|error| PyValueError::new_err(error.to_string()))
        }
        Some(key) if key.is_callable() => before
            .0
            .magic_move_to_by(
                &after.0,
                |member| {
                    let member = Py::new(py, PyDrawable(member.clone()))?;
                    callable_key(&key.call1((member,))?)
                },
                unmatched,
                duration,
            )
            .map_err(|failure| match failure {
                MagicMoveFailure::Invalid(error) => PyValueError::new_err(error.to_string()),
                MagicMoveFailure::Key(error) => error,
            }),
        Some(_) => Err(PyTypeError::new_err(
            "key must be \"name\", \"id\" or a callable taking a Drawable",
        )),
    };
    result.map(|inner| PyCanvasAnim { inner })
}
