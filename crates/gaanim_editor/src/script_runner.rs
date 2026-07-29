//! Embedded-Python script runner.
//!
//! The host owns a dedicated OS thread that holds the GIL and executes the
//! user's animation script. The script imports `gaanim` (which, because the
//! host registered `gaanim_core` via `append_to_inittab!`, resolves to the
//! in-process module) and builds a `Canvas`; calling `.render()` pushes the
//! canonical `gaanim_api` payload through the host channel instead of opening a
//! window.

use crossbeam_channel::{Receiver, Sender};
use pyo3::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gaanim_api::host::{self, ReloadPayload};

/// Handle to the script-running thread.
pub struct ScriptRunner {
    /// Send `true` here to ask the thread to re-run the script.
    rerun_tx: Sender<bool>,
    /// Set when the thread has exited (e.g. after a fatal error).
    _exited: Arc<AtomicBool>,
}

impl ScriptRunner {
    /// Spawn the script-runner thread.
    ///
    /// * `script_path` — absolute path to the user's `.py` file.
    /// * `payload_tx` — channel end that receives scene payloads from the
    ///   embedded script (i.e. the host-side receiver of `host::send_to_host`).
    pub fn spawn(script_path: PathBuf, payload_tx: Sender<ReloadPayload>) -> Self {
        let (rerun_tx, rerun_rx) = crossbeam_channel::unbounded::<bool>();
        let exited = Arc::new(AtomicBool::new(false));
        let exited_clone = exited.clone();

        std::thread::Builder::new()
            .name("gaanim-script".into())
            .spawn(move || {
                run_script_thread(script_path, payload_tx, rerun_rx, exited_clone);
            })
            .expect("failed to spawn script thread");

        Self {
            rerun_tx,
            _exited: exited,
        }
    }

    /// Request a re-run of the script (used by the file watcher and the `R` key).
    pub fn request_rerun(&self) {
        let _ = self.rerun_tx.send(true);
    }
}

fn run_script_thread(
    script_path: PathBuf,
    payload_tx: Sender<ReloadPayload>,
    rerun_rx: Receiver<bool>,
    exited: Arc<AtomicBool>,
) {
    // Install the host channel so `Canvas.render()` inside the script can push
    // payloads to us. This is done once; the channel persists across re-runs.
    host::set_host_sender(Some(payload_tx));

    // One-time bootstrap: the embedded interpreter has `gaanim_core` registered
    // as a top-level builtin module (via `append_to_inittab!` in main). User
    // scripts, however, do `from gaanim import Canvas`. There is no pip-installed
    // `gaanim` package in the embedded environment, so we synthesize one in
    // memory that re-exports every public attribute of the builtin `gaanim_core`.
    let bootstrap_err = Python::attach(|py| {
        py.run(
            &std::ffi::CString::new(BOOTSTRAP_GAANIM_PACKAGE).unwrap(),
            None,
            None,
        )
    });
    if let Err(e) = bootstrap_err {
        Python::attach(|py| {
            e.print(py);
        });
        eprintln!("[gaanim] failed to bootstrap in-memory `gaanim` package");
    }

    // Run immediately on first iteration, then block for re-run signals.
    loop {
        if exited.load(Ordering::SeqCst) {
            break;
        }

        let result = Python::attach(|py| run_script_file(py, &script_path));
        if let Err(e) = result {
            Python::attach(|py| {
                e.print(py);
            });
            eprintln!("[gaanim] script error (waiting for next save to retry)");
        }

        // Block until the next re-run request (or channel closed).
        match rerun_rx.recv() {
            Ok(true) => continue,
            _ => break,
        }
    }

    host::set_host_sender(None);
    exited.store(true, Ordering::SeqCst);
}

/// Python bootstrap that creates an in-memory `gaanim` package aliasing the
/// builtin `gaanim_core` module, so `from gaanim import Canvas` works without a
/// pip-installed package.
const BOOTSTRAP_GAANIM_PACKAGE: &str = "import sys, types\nimport gaanim_core\nif 'gaanim' not in sys.modules:\n    _pkg = types.ModuleType('gaanim')\n    _pkg.__path__ = []\n    for _n in dir(gaanim_core):\n        if not _n.startswith('_'):\n            setattr(_pkg, _n, getattr(gaanim_core, _n))\n    sys.modules['gaanim'] = _pkg\n";

/// Execute one script solely to produce `Scene.snapshots()` artifacts.
///
/// A host channel is installed so a trailing `scene.render()` remains valid,
/// but its payload is intentionally discarded: this command is headless.
pub fn capture_script_snapshots(script_path: &Path, snapshot_dir: &Path) -> Result<(), String> {
    let snapshot_dir = snapshot_dir
        .to_str()
        .ok_or_else(|| "snapshot directory is not UTF-8".to_string())?;
    let (sender, _receiver) = crossbeam_channel::unbounded::<ReloadPayload>();
    host::set_host_sender(Some(sender));

    let result = Python::attach(|py| -> PyResult<()> {
        py.run(
            &std::ffi::CString::new(BOOTSTRAP_GAANIM_PACKAGE).unwrap(),
            None,
            None,
        )?;
        let os = py.import("os")?;
        os.getattr("environ")?
            .set_item("GAANIM_SNAPSHOTS", snapshot_dir)?;
        run_script_file(py, script_path)
    });

    host::set_host_sender(None);
    result.map_err(|error| error.to_string())
}

/// Execute a script once and return the canvas submitted by `scene.render()`.
///
/// Used by non-interactive CLI tooling such as `gaanim check`. Export and
/// snapshot environment switches are removed so validation can never start a
/// render job as a side effect.
pub fn load_script_canvas(script_path: &Path) -> Result<gaanim_api::canvas::Canvas, String> {
    let (sender, receiver) = crossbeam_channel::bounded::<ReloadPayload>(1);
    host::set_host_sender(Some(sender));

    let result = Python::attach(|py| -> PyResult<()> {
        py.run(
            &std::ffi::CString::new(BOOTSTRAP_GAANIM_PACKAGE).unwrap(),
            None,
            None,
        )?;
        let environment = py.import("os")?.getattr("environ")?;
        let _ = environment.del_item("GAANIM_SNAPSHOTS");
        let _ = environment.del_item("GAANIM_EXPORT");
        run_script_file(py, script_path)
    });

    host::set_host_sender(None);
    result.map_err(|error| error.to_string())?;
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .map(|payload| payload.canvas)
        .map_err(|_| "script did not submit a scene; finish it with `scene.render()`".to_string())
}

/// Execute a Python file by path inside the given interpreter, in a fresh
/// `__main__` namespace so each re-run is isolated from the previous one.
fn run_script_file(py: Python<'_>, path: &Path) -> PyResult<()> {
    let path_str = path
        .to_str()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("script path is not UTF-8"))?;

    // Build a tiny bootstrap that runs the file as __main__.
    // Using runpy.run_path executes the file with a fresh __main__ module,
    // which gives each reload a clean global namespace.
    let code = format!(
        "import runpy, sys\n\
         sys.argv = [r'{path_str}']\n\
         runpy.run_path(r'{path_str}', run_name='__main__')\n"
    );
    py.run(&std::ffi::CString::new(code).unwrap(), None, None)?;
    Ok(())
}
