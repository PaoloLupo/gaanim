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
use std::time::Instant;

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
    /// * `error_tx` — channel end that receives formatted tracebacks when the
    ///   script raises.
    pub fn spawn(
        script_path: PathBuf,
        payload_tx: Sender<ReloadPayload>,
        error_tx: Sender<String>,
    ) -> Self {
        let (rerun_tx, rerun_rx) = crossbeam_channel::unbounded::<bool>();
        let exited = Arc::new(AtomicBool::new(false));
        let exited_clone = exited.clone();

        std::thread::Builder::new()
            .name("gaanim-script".into())
            .spawn(move || {
                run_script_thread(script_path, payload_tx, error_tx, rerun_rx, exited_clone);
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

fn format_py_traceback(py: Python<'_>, err: &PyErr) -> String {
    // Intentar traceback.format_exception para obtener el traceback completo
    if let Ok(tb_mod) = py.import("traceback") {
        if let Ok(formatted) = tb_mod.call_method1(
            "format_exception",
            (err.get_type(py), err.value(py), err.traceback(py)),
        ) {
            if let Ok(list) = formatted.extract::<Vec<String>>() {
                let joined = list.join("");
                if !joined.trim().is_empty() {
                    return joined;
                }
            }
        }
    }
    // Fallback: valor de la excepción + tipo
    if let Ok(val) = err.value(py).extract::<String>() {
        if !val.trim().is_empty() {
            let type_name = err
                .get_type(py)
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "PythonError".to_string());
            return format!("{type_name}: {val}");
        }
    }
    err.to_string()
}

fn run_script_thread(
    script_path: PathBuf,
    payload_tx: Sender<ReloadPayload>,
    error_tx: Sender<String>,
    rerun_rx: Receiver<bool>,
    exited: Arc<AtomicBool>,
) {
    // Install the host channel so `Canvas.render()` inside the script can push
    // payloads to us. This is done once; the channel persists across re-runs.
    host::set_host_sender(Some(payload_tx));

    // One-time bootstrap: expose the exact same public package surface as the
    // wheel while keeping the in-process `gaanim_core` module that owns the host
    // channel. Pure-Python helpers such as colors and layout templates are
    // embedded below.
    let bootstrap_err = Python::attach(bootstrap_gaanim_package);
    if let Err(e) = bootstrap_err {
        Python::attach(|py| {
            let msg = format_py_traceback(py, &e);
            let _ = error_tx.send(format!("[bootstrap] {}", msg));
            e.print(py);
        });
        eprintln!("[gaanim] failed to bootstrap in-memory `gaanim` package");
    }

    // Run immediately on first iteration, then block for re-run signals.
    loop {
        if exited.load(Ordering::SeqCst) {
            break;
        }

        host::set_compile_started_at(Some(Instant::now()));
        let result = Python::attach(|py| run_script_file(py, &script_path));
        host::set_compile_started_at(None);
        if let Err(e) = result {
            Python::attach(|py| {
                let tb = format_py_traceback(py, &e);
                let header = format!("{} — traceback:", script_path.display());
                let full = format!("{}\n{}", header, tb);
                let _ = error_tx.send(full);
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

const GAANIM_PACKAGE_INIT: &str = include_str!("../../gaanim_python/gaanim/__init__.py");
const GAANIM_COLORS: &str = include_str!("../../gaanim_python/gaanim/colors.py");
const GAANIM_TEMPLATES: &str = include_str!("../../gaanim_python/gaanim/templates.py");

/// Build the public `gaanim` package around the builtin `gaanim_core` module.
///
/// Loading the installed wheel here would create a second native extension and
/// therefore a second host-channel static. Instead, the package initializer and
/// pure-Python helpers are compiled into the editor and executed with
/// `gaanim.gaanim_core` explicitly aliased to the builtin module.
fn bootstrap_gaanim_package(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    if modules.contains("gaanim")? {
        return Ok(());
    }

    let core = py.import("gaanim_core")?;
    let package = PyModule::new(py, "gaanim")?;
    package.setattr("__package__", "gaanim")?;
    package.setattr("__path__", Vec::<String>::new())?;
    modules.set_item("gaanim", &package)?;
    modules.set_item("gaanim.gaanim_core", &core)?;

    let colors_source = std::ffi::CString::new(GAANIM_COLORS).unwrap();
    let colors_file = std::ffi::CString::new("gaanim/colors.py").unwrap();
    let colors_name = std::ffi::CString::new("gaanim.colors").unwrap();
    let colors = PyModule::from_code(py, &colors_source, &colors_file, &colors_name)?;
    modules.set_item("gaanim.colors", &colors)?;

    let templates_source = std::ffi::CString::new(GAANIM_TEMPLATES).unwrap();
    let templates_file = std::ffi::CString::new("gaanim/templates.py").unwrap();
    let templates_name = std::ffi::CString::new("gaanim.templates").unwrap();
    let templates = PyModule::from_code(py, &templates_source, &templates_file, &templates_name)?;
    modules.set_item("gaanim.templates", &templates)?;

    let init_source = std::ffi::CString::new(GAANIM_PACKAGE_INIT).unwrap();
    py.run(&init_source, Some(&package.dict()), None)
}

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
        bootstrap_gaanim_package(py)?;
        let os = py.import("os")?;
        os.getattr("environ")?
            .set_item("GAANIM_SNAPSHOTS", snapshot_dir)?;
        run_script_file(py, script_path)
    });

    host::set_host_sender(None);
    result.map_err(|error| Python::attach(|py| format_py_traceback(py, &error)))
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
        bootstrap_gaanim_package(py)?;
        let environment = py.import("os")?.getattr("environ")?;
        let _ = environment.del_item("GAANIM_SNAPSHOTS");
        let _ = environment.del_item("GAANIM_EXPORT");
        run_script_file(py, script_path)
    });

    host::set_host_sender(None);
    result.map_err(|error| Python::attach(|py| format_py_traceback(py, &error)))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_package_exports_tailwind_colors() {
        gaanim_python::register_inittab();
        Python::initialize();

        Python::attach(|py| {
            bootstrap_gaanim_package(py).expect("embedded gaanim package should bootstrap");
            let package = py.import("gaanim").expect("gaanim should be importable");
            let colors = package
                .getattr("colors")
                .expect("gaanim should export its colors module");
            let blue = colors
                .getattr("tailwind")
                .and_then(|tailwind| tailwind.getattr("blue"))
                .and_then(|family| family.get_item(500))
                .expect("Tailwind blue[500] should be available in the embedded host");
            assert_eq!(blue.to_string(), "Color(#2B7FFF)");
        });
    }
}
