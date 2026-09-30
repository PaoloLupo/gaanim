//! `scene.live_zone`: a part of the scene where the audience's characters
//! play while presenting, posed by a behavior the scene writes in Python.

use gaanim_api::canvas::{LiveProgram, LiveZoneError, LiveZoneHandle};
use pyo3::{exceptions::PyValueError, prelude::*};

pub(crate) fn live_error(error: LiveZoneError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// Compile a behavior function with `gaanim.live.compile_behavior`. Its
/// `BehaviorError` (a `ValueError` pointing at the line) propagates.
pub(crate) fn compile_behavior(behavior: &Bound<'_, PyAny>) -> PyResult<LiveProgram> {
    let json: String = behavior
        .py()
        .import("gaanim.live")?
        .getattr("compile_behavior")?
        .call1((behavior,))?
        .extract()?;
    LiveProgram::from_json(&json).map_err(|error| PyValueError::new_err(error.to_string()))
}

/// A live zone: while presenting, each player arrives in it as their
/// character, posed every frame by the zone's behavior. Previews and
/// exports replay the scene's rehearsal. The behavior is compiled into the
/// scene, so a presented `.gaanim` runs it without Python.
#[pyclass(name = "LiveZone", module = "gaanim_core", frozen)]
pub struct PyLiveZone {
    pub(crate) inner: LiveZoneHandle,
}

#[pymethods]
impl PyLiveZone {
    /// How many instructions the compiled behavior runs per player.
    #[getter]
    fn instructions(&self) -> usize {
        self.inner.zone().behavior.code.len()
    }

    /// Stop the zone at the cursor instead of at the end of the segment
    /// where it opened.
    fn close(&self) -> PyResult<()> {
        crate::custom::ensure_authoring_allowed()?;
        self.inner.close().map_err(live_error)
    }

    fn __repr__(&self) -> String {
        let zone = self.inner.zone();
        format!(
            "LiveZone(behavior={}, bounds={:?}, size={}, instructions={})",
            zone.behavior.name,
            zone.bounds,
            zone.size,
            zone.behavior.code.len()
        )
    }
}
