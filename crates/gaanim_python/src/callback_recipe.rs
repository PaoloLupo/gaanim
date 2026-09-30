//! Recipes for Python callbacks, so hot reload can reuse the segments that
//! use them.
//!
//! A new run creates new callback objects, and a callback without a recipe
//! fingerprints by identity: every segment from the first one that uses it
//! would recompile on each reload. A callback's recipe (see
//! `callback_recipe.py`) is computed when the scene is rendered rather than
//! when the callback is passed in, because the globals and captured
//! variables it reads may still be reassigned until the script finishes.

use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;

type PendingRecipe = (Py<PyAny>, Arc<OnceLock<Arc<str>>>);

thread_local! {
    /// Callbacks passed in on this thread since its last render, with the
    /// recipe each awaits. A script authors and renders on one thread.
    static PENDING: RefCell<Vec<PendingRecipe>> = const { RefCell::new(Vec::new()) };
}

static RECIPE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

/// Register `callback` and return the recipe it will get at the next render.
pub(crate) fn deferred(py: Python<'_>, callback: &Py<PyAny>) -> Arc<OnceLock<Arc<str>>> {
    let recipe = Arc::new(OnceLock::new());
    PENDING.with_borrow_mut(|pending| pending.push((callback.clone_ref(py), recipe.clone())));
    recipe
}

/// Describe every callback registered since the last render.
pub(crate) fn resolve_pending(py: Python<'_>) -> PyResult<()> {
    let pending = PENDING.take();
    if pending.is_empty() {
        return Ok(());
    }
    let describe = RECIPE.get_or_try_init(py, || -> PyResult<Py<PyAny>> {
        let module = PyModule::from_code(
            py,
            &std::ffi::CString::new(include_str!("callback_recipe.py"))
                .expect("recipe source has no NUL"),
            c"gaanim/_callback_recipe.py",
            c"gaanim._callback_recipe",
        )?;
        Ok(module.getattr("recipe")?.unbind())
    })?;
    for (callback, recipe) in pending {
        let described = describe.bind(py).call1((callback,))?;
        if let Some(text) = described.extract::<Option<String>>()? {
            let _ = recipe.set(text.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyDict;

    fn recipes(py: Python<'_>, source: &str, names: &[&str]) -> PyResult<Vec<Option<String>>> {
        let globals = PyDict::new(py);
        py.run(
            &std::ffi::CString::new(source).unwrap(),
            Some(&globals),
            None,
        )?;
        let cells = names
            .iter()
            .map(|name| {
                let callback = globals.get_item(name)?.expect("callback defined").unbind();
                Ok(deferred(py, &callback))
            })
            .collect::<PyResult<Vec<_>>>()?;
        resolve_pending(py)?;
        Ok(cells
            .iter()
            .map(|cell| cell.get().map(|recipe| recipe.to_string()))
            .collect())
    }

    #[test]
    fn equal_callbacks_share_a_recipe_and_any_difference_changes_it() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let found = recipes(
                py,
                r#"
import math
OFFSET = 1.5
def make(k):
    return lambda v: OFFSET + k * math.cos(v)
def helper(v):
    return v * 2
same_a = lambda v, o=0.25: v + o
same_b = lambda v, o=0.25: v + o
other_default = lambda v, o=0.5: v + o
other_code = lambda v, o=0.25: v - o
closure_a = make(2)
closure_b = make(2)
closure_c = make(3)
calls_helper = lambda v: helper(v) + 1
"#,
                &[
                    "same_a",
                    "same_b",
                    "other_default",
                    "other_code",
                    "closure_a",
                    "closure_b",
                    "closure_c",
                    "calls_helper",
                ],
            )?;
            let [
                same_a,
                same_b,
                other_default,
                other_code,
                closure_a,
                closure_b,
                closure_c,
                helper,
            ] = found.try_into().unwrap();
            assert!(same_a.is_some() && helper.is_some());
            assert_eq!(same_a, same_b);
            assert_ne!(same_a, other_default);
            assert_ne!(same_a, other_code);
            assert_eq!(closure_a, closure_b);
            assert!(closure_a.is_some());
            assert_ne!(closure_a, closure_c);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn globals_read_the_value_at_render_time() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let globals = PyDict::new(py);
            py.run(
                c"SHIFT = 1.0\nshift = lambda v: v + SHIFT\n",
                Some(&globals),
                None,
            )?;
            let callback = globals.get_item("shift")?.unwrap().unbind();
            let early = deferred(py, &callback);
            py.run(c"SHIFT = 2.0\n", Some(&globals), None)?;
            resolve_pending(py)?;
            let late = deferred(py, &callback);
            resolve_pending(py)?;
            assert_eq!(early.get(), late.get());
            assert!(early.get().unwrap().contains("SHIFT=2.0"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn hidden_or_mutable_state_has_no_recipe() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let found = recipes(
                py,
                r#"
import types
values = [1.0]
project = types.ModuleType("tesis.theme")
project.__file__ = "/work/project/src/tesis/theme.py"
class Scale:
    def __call__(self, v):
        return v
reads_list = lambda v: v + values[0]
reads_project_module = lambda v: v + project.OFFSET
callable_object = Scale()
bound_builtin = values.append
"#,
                &[
                    "reads_list",
                    "reads_project_module",
                    "callable_object",
                    "bound_builtin",
                ],
            )?;
            assert_eq!(found, vec![None, None, None, None]);
            Ok(())
        })
        .unwrap();
    }
}
