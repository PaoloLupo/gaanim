use gaanim_core::ObjectId;
use pyo3::prelude::*;

/// Stable handle to a Mobject allocated by `gaanim`.
///
/// These are allocated by the Python side (`Scene.next_id`) so the user can
/// keep references to mobjects across Python calls. Internally, the Rust
/// `SceneBuilder` re-allocates a separate `ObjectId` and a `py_id -> bevy_id`
/// map is built at replay time.
#[pyclass(name = "ObjectId", module = "gaanim_core", from_py_object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PyObjectId(pub ObjectId);

#[pymethods]
impl PyObjectId {
    #[new]
    fn new(index: u32, generation: u32) -> Self {
        Self(ObjectId::from_parts(index, generation))
    }

    #[getter]
    fn index(&self) -> u32 {
        self.0.index()
    }

    #[getter]
    fn generation(&self) -> u32 {
        self.0.generation()
    }

    fn __repr__(&self) -> String {
        format!("ObjectId({}v{})", self.0.index(), self.0.generation())
    }

    fn __eq__(&self, other: &PyObjectId) -> bool {
        self.0 == other.0
    }

    fn __hash__(&self) -> u64 {
        self.0.as_raw()
    }
}
