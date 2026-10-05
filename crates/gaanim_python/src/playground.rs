//! Entry points of the web playground, which runs scripts in Pyodide
//! (wasm32-unknown-emscripten, built by `crates/gaanim_pyodide`) and shows
//! their scene in the web player.
//!
//! Pyodide has no Gaanim application to submit a scene to, so the playground
//! is the host: `_playground_begin` installs the channel `scene.render()`
//! submits to, the script runs, and `_playground_record` records the last
//! submitted scene into a playback bundle, which the web player opens. The
//! recording evaluates every frame, Python callbacks included, in this
//! interpreter.

use std::path::PathBuf;
use std::sync::Mutex;

use gaanim_api::export::{BundleConfig, record_canvas};
use gaanim_api::host::{self, ReloadPayload};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// Where scenes submitted since `_playground_begin` arrive.
static SUBMITTED: Mutex<Option<crossbeam_channel::Receiver<ReloadPayload>>> = Mutex::new(None);

/// Accept the scenes the next script submits. `assets_changed` drops the
/// cached images and fonts, after files were added or replaced.
#[pyfunction]
#[pyo3(name = "_playground_begin")]
fn begin(assets_changed: bool) {
    if assets_changed {
        gaanim_api::canvas::clear_asset_caches();
        host::mark_assets_changed();
    }
    // Unbounded: a script that renders twice must not block on its own
    // thread; the last scene wins, as in the application.
    let (sender, receiver) = crossbeam_channel::unbounded();
    host::set_host_sender(Some(sender));
    *SUBMITTED.lock().expect("playground channel poisoned") = Some(receiver);
}

/// Record the scene the script submitted into a bundle at `path`, at `fps`
/// frames per second.
#[pyfunction]
#[pyo3(name = "_playground_record", signature = (path, fps = 30))]
fn record(path: PathBuf, fps: u32) -> PyResult<()> {
    host::set_host_sender(None);
    let receiver = SUBMITTED
        .lock()
        .expect("playground channel poisoned")
        .take()
        .ok_or_else(|| PyRuntimeError::new_err("_playground_begin was not called"))?;
    let canvas = receiver
        .try_iter()
        .last()
        .map(|payload| payload.canvas)
        .ok_or_else(|| {
            PyRuntimeError::new_err(
                "the script did not submit a scene; finish it with `scene.render()`",
            )
        })?;
    let (width, height) = canvas.frame.preview_pixel_size();
    let mut config = BundleConfig::new(path);
    config.fps = fps.max(1);
    config.width = width;
    config.height = height;
    // The cover image needs a GPU; the playground has none in Python.
    config.thumbnail = false;
    record_canvas(canvas, config).map_err(|error| PyRuntimeError::new_err(error.to_string()))
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(begin, module)?)?;
    module.add_function(wrap_pyfunction!(record, module)?)?;
    Ok(())
}
