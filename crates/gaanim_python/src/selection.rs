use gaanim_api::anim::AnimationType;
use gaanim_core::glam::DVec3;
use gaanim_core::ObjectId;
use pyo3::prelude::*;

use crate::animation::{rate_func_from_name, PyAnimationSpec};
use crate::id::PyObjectId;
use crate::scene::PyScene;

/// A handle to a sub-selection of characters/glyphs inside a Text or Equation.
///
/// Created via `Scene.select(parent, query)`. Use the corresponding `Scene`
/// methods (`fill`, `set_stroke`, `play`) to operate on the selection.
#[pyclass(name = "Selection", module = "gaanim_core", from_py_object)]
#[derive(Clone, Debug)]
pub struct PySelection {
    pub parent: ObjectId,
    pub query: String,
    pub id: ObjectId,
}

#[pymethods]
impl PySelection {
    #[getter]
    fn parent(&self) -> PyObjectId {
        PyObjectId(self.parent)
    }

    #[getter]
    fn query(&self) -> String {
        self.query.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "Selection(parent=ObjectId({}v{}), query={:?})",
            self.parent.index(),
            self.parent.generation(),
            self.query
        )
    }
}

/// Fluent builder for selection animations. Built via
/// `Scene.build_selection_anim(selection, dx, dy)`. Pass `.build()` result
/// to `Scene.play()`.
#[pyclass(name = "SelectionAnim", module = "gaanim_core", from_py_object)]
#[derive(Clone, Debug)]
pub struct PySelectionAnim {
    pub selection: ObjectId,
    pub parent: ObjectId,
    pub dx: f64,
    pub dy: f64,
    pub duration: f64,
    pub rate_func: gaanim_math::RateFunc,
}

#[pymethods]
impl PySelectionAnim {
    fn __repr__(&self) -> String {
        format!(
            "SelectionAnim(parent=ObjectId({}v{}), shift=({:.1}, {:.1}), duration={}, rate={})",
            self.parent.index(),
            self.parent.generation(),
            self.dx,
            self.dy,
            self.duration,
            crate::animation::rate_func_name(&self.rate_func),
        )
    }

    #[getter]
    fn duration_val(&self) -> f64 {
        self.duration
    }

    fn duration(&self, d: f64) -> Self {
        Self {
            duration: d,
            ..self.clone()
        }
    }

    fn spring(&self) -> Self {
        Self {
            rate_func: gaanim_math::RateFunc::Spring {
                stiffness: 90.0,
                damping: 12.0,
            },
            ..self.clone()
        }
    }

    fn smooth(&self) -> Self {
        Self {
            rate_func: gaanim_math::RateFunc::Smooth,
            ..self.clone()
        }
    }

    fn linear(&self) -> Self {
        Self {
            rate_func: gaanim_math::RateFunc::Linear,
            ..self.clone()
        }
    }

    fn rate_func(&self, name: &str) -> PyResult<Self> {
        let rf = rate_func_from_name(name)?;
        Ok(Self {
            rate_func: rf,
            ..self.clone()
        })
    }

    /// Build the final selection shift spec. Pass to `Scene.play(spec)`.
    fn build(&self, scene: &PyScene) -> PyResult<PyAnimationSpec> {
        scene.push_selection_shift(
            self.selection,
            self.dx,
            self.dy,
            self.duration,
            self.rate_func.clone(),
        )?;
        Ok(PyAnimationSpec::from_kind(
            self.parent,
            AnimationType::TranslateBy {
                delta: DVec3::new(self.dx, self.dy, 0.0),
            },
        ))
    }
}
