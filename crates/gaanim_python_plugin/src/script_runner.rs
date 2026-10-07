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
use pyo3::types::{PyBytes, PyDict};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use gaanim_api::host::{self, ReloadPayload};

/// Handle to the script-running thread.
pub struct ScriptRunner {
    /// Ask the thread to re-run the script.
    rerun_tx: Sender<Rerun>,
    /// Set when the thread has exited (e.g. after a fatal error).
    _exited: Arc<AtomicBool>,
}

struct SnapshotHandlerGuard;

impl Drop for SnapshotHandlerGuard {
    fn drop(&mut self) {
        host::set_snapshot_handler(None);
    }
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
        let (rerun_tx, rerun_rx) = crossbeam_channel::unbounded::<Rerun>();
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
        let _ = self.rerun_tx.send(Rerun::Source);
    }

    /// Re-run the script after the project files `changed` changed: the
    /// cached images and Lottie compositions read from them are read again,
    /// and the replay recompiles from the first segment whose content
    /// changed with them.
    pub fn request_asset_reload(&self, changed: Vec<PathBuf>) {
        let _ = self.rerun_tx.send(Rerun::Assets(changed));
    }

    /// A request that other threads and systems can keep to re-run the
    /// script reading every cached asset (images, Lottie, Typst layouts)
    /// anew and replaying the whole scene, e.g. after narration takes.
    pub fn asset_reload_handle(&self) -> impl Fn() + Send + Sync + 'static {
        let rerun_tx = self.rerun_tx.clone();
        move || {
            let _ = rerun_tx.send(Rerun::AllAssets);
        }
    }
}

/// Why the script runs again.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Rerun {
    Source,
    /// These project files changed.
    Assets(Vec<PathBuf>),
    /// Any asset may have changed.
    AllAssets,
}

impl Rerun {
    /// One request standing for `self` and `next`, queued while the script
    /// ran.
    fn merge(self, next: Self) -> Self {
        match (self, next) {
            (Rerun::AllAssets, _) | (_, Rerun::AllAssets) => Rerun::AllAssets,
            (Rerun::Assets(mut paths), Rerun::Assets(more)) => {
                paths.extend(more);
                Rerun::Assets(paths)
            }
            (Rerun::Assets(paths), Rerun::Source) | (Rerun::Source, Rerun::Assets(paths)) => {
                Rerun::Assets(paths)
            }
            (Rerun::Source, Rerun::Source) => Rerun::Source,
        }
    }
}

fn format_py_traceback(py: Python<'_>, err: &PyErr) -> String {
    // Intentar traceback.format_exception para obtener el traceback completo
    if let Ok(tb_mod) = py.import("traceback")
        && let Ok(formatted) = tb_mod.call_method1(
            "format_exception",
            (err.get_type(py), err.value(py), err.traceback(py)),
        )
        && let Ok(list) = formatted.extract::<Vec<String>>()
    {
        let joined = list.join("");
        if !joined.trim().is_empty() {
            return joined;
        }
    }
    // Fallback: valor de la excepción + tipo
    if let Ok(val) = err.value(py).extract::<String>()
        && !val.trim().is_empty()
    {
        let type_name = err
            .get_type(py)
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "PythonError".to_string());
        return format!("{type_name}: {val}");
    }
    err.to_string()
}

/// Drops the frames of Gaanim's script runner (`<string>`) and of Python's
/// own machinery (`<frozen runpy>`, `<frozen importlib._bootstrap>`) from a
/// formatted traceback, so it starts at the person's code.
fn without_runner_frames(traceback: &str) -> String {
    let mut kept = String::with_capacity(traceback.len());
    let mut skipping = false;
    for line in traceback.split_inclusive('\n') {
        if let Some(frame) = line.strip_prefix("  File \"") {
            skipping = frame.starts_with("<string>") || frame.starts_with("<frozen ");
        } else if !line.starts_with("    ") {
            skipping = false;
        }
        if !skipping {
            kept.push_str(line);
        }
    }
    kept
}

fn run_script_thread(
    script_path: PathBuf,
    payload_tx: Sender<ReloadPayload>,
    error_tx: Sender<String>,
    rerun_rx: Receiver<Rerun>,
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
        gaanim_core::console::error("python", "could not load the in-memory `gaanim` package");
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
                let tb = without_runner_frames(&format_py_traceback(py, &e));
                let header = format!("{} — traceback:", script_path.display());
                let full = format!("{}\n{}", header, tb);
                let _ = error_tx.send(full);
                let color =
                    gaanim_core::console::color_enabled(gaanim_core::console::Stream::Stderr);
                eprint!("{}", gaanim_core::console::format_traceback(&tb, color));
            });
            gaanim_core::console::error("python", "Script failed · fix it and save to reload");
        }

        // Block until the next re-run request (or channel closed).
        match rerun_rx.recv() {
            Ok(first) => {
                // Coalesce requests queued while the script ran.
                match rerun_rx.try_iter().fold(first, Rerun::merge) {
                    Rerun::Source => {}
                    // What the forgotten files fed the scene reaches the
                    // replay's fingerprints, so it can still reuse segments.
                    Rerun::Assets(changed) => gaanim_api::canvas::forget_assets(&changed),
                    Rerun::AllAssets => {
                        gaanim_api::canvas::clear_asset_caches();
                        gaanim_api::host::mark_assets_changed();
                    }
                }
            }
            Err(_) => break,
        }
    }

    host::set_host_sender(None);
    exited.store(true, Ordering::SeqCst);
}

const GAANIM_PACKAGE_INIT: &str = include_str!("../../gaanim_python/gaanim/__init__.py");
const GAANIM_COLORS: &str = include_str!("../../gaanim_python/gaanim/colors.py");
const GAANIM_TEMPLATES: &str = include_str!("../../gaanim_python/gaanim/templates.py");
const GAANIM_SECTIONS: &str = include_str!("../../gaanim_python/gaanim/sections.py");
const GAANIM_MATRIX: &str = include_str!("../../gaanim_python/gaanim/matrix.py");
const GAANIM_AUDIO_VIZ: &str = include_str!("../../gaanim_python/gaanim/audio_viz.py");
const GAANIM_ANIMATION_TYPES: &str = include_str!("../../gaanim_python/gaanim/animation_types.py");
const GAANIM_TYPE_ALIASES: &str = include_str!("../../gaanim_python/gaanim/_type_aliases.py");
const GAANIM_LIVE: &str = include_str!("../../gaanim_python/gaanim/live.py");
const GAANIM_QUESTIONS: &str = include_str!("../../gaanim_python/gaanim/questions.py");

/// Build the public `gaanim` package around the builtin `gaanim_core` module.
///
/// The installed wheel only carries authoring helpers and stubs. The package
/// initializer and pure-Python helpers are compiled into the editor and executed with
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

    // Register before execution so dataclasses can resolve their defining module.
    let sections = PyModule::new(py, "gaanim.sections")?;
    sections.setattr("__package__", "gaanim")?;
    modules.set_item("gaanim.sections", &sections)?;
    let sections_source = std::ffi::CString::new(GAANIM_SECTIONS).unwrap();
    py.run(&sections_source, Some(&sections.dict()), None)?;

    let matrix_source = std::ffi::CString::new(GAANIM_MATRIX).unwrap();
    let matrix_file = std::ffi::CString::new("gaanim/matrix.py").unwrap();
    let matrix_name = std::ffi::CString::new("gaanim.matrix").unwrap();
    let matrix = PyModule::from_code(py, &matrix_source, &matrix_file, &matrix_name)?;
    modules.set_item("gaanim.matrix", &matrix)?;

    let audio_viz_source = std::ffi::CString::new(GAANIM_AUDIO_VIZ).unwrap();
    let audio_viz_file = std::ffi::CString::new("gaanim/audio_viz.py").unwrap();
    let audio_viz_name = std::ffi::CString::new("gaanim.audio_viz").unwrap();
    let audio_viz = PyModule::from_code(py, &audio_viz_source, &audio_viz_file, &audio_viz_name)?;
    modules.set_item("gaanim.audio_viz", &audio_viz)?;

    let types_source = std::ffi::CString::new(GAANIM_ANIMATION_TYPES).unwrap();
    let types_file = std::ffi::CString::new("gaanim/animation_types.py").unwrap();
    let types_name = std::ffi::CString::new("gaanim.animation_types").unwrap();
    let animation_types = PyModule::from_code(py, &types_source, &types_file, &types_name)?;
    modules.set_item("gaanim.animation_types", &animation_types)?;

    // The stub's type aliases (Paint, ColorLike...), which the package
    // initializer installs on `gaanim_core`.
    let aliases_source = std::ffi::CString::new(GAANIM_TYPE_ALIASES).unwrap();
    let aliases_file = std::ffi::CString::new("gaanim/_type_aliases.py").unwrap();
    let aliases_name = std::ffi::CString::new("gaanim._type_aliases").unwrap();
    let aliases = PyModule::from_code(py, &aliases_source, &aliases_file, &aliases_name)?;
    modules.set_item("gaanim._type_aliases", &aliases)?;

    // Live behaviors: the compiler scene.live_zone calls. Registered before
    // execution, like sections, so its dataclasses resolve their module.
    let live = PyModule::new(py, "gaanim.live")?;
    live.setattr("__package__", "gaanim")?;
    live.setattr("__file__", "gaanim/live.py")?;
    modules.set_item("gaanim.live", &live)?;
    // The compiler reads its helpers' source with `inspect`, so the embedded
    // file goes in `linecache` under the name its code objects carry.
    let loader = pyo3::types::PyDict::new(py);
    loader.set_item("source", GAANIM_LIVE)?;
    loader.set_item("namespace", live.dict())?;
    let load = std::ffi::CString::new(concat!(
        "import linecache\n",
        "linecache.cache['gaanim/live.py'] = ",
        "(len(source), None, source.splitlines(True), 'gaanim/live.py')\n",
        "exec(compile(source, 'gaanim/live.py', 'exec'), namespace)\n",
    ))
    .unwrap();
    py.run(&load, Some(&loader), None)?;

    // Questions written outside Python; dataclasses, so registered first.
    let questions = PyModule::new(py, "gaanim.questions")?;
    questions.setattr("__package__", "gaanim")?;
    modules.set_item("gaanim.questions", &questions)?;
    let questions_source = std::ffi::CString::new(GAANIM_QUESTIONS).unwrap();
    py.run(&questions_source, Some(&questions.dict()), None)?;

    let init_source = std::ffi::CString::new(GAANIM_PACKAGE_INIT).unwrap();
    py.run(&init_source, Some(&package.dict()), None)
}

/// Execute one script solely to produce `Scene.snapshots()` artifacts.
///
/// A host channel is installed so a trailing `scene.render()` remains valid,
/// but its payload is intentionally discarded: this command is headless.
/// Frames are `height` pixels tall when given, or the preview size.
pub fn capture_script_snapshots(
    script_path: &Path,
    snapshot_dir: &Path,
    height: Option<u32>,
) -> Result<(), String> {
    let snapshot_dir = snapshot_dir
        .to_str()
        .ok_or_else(|| "snapshot directory is not UTF-8".to_string())?;
    let (sender, _receiver) = crossbeam_channel::unbounded::<ReloadPayload>();
    host::set_host_sender(Some(sender));

    let handler_dir = PathBuf::from(snapshot_dir);
    host::set_snapshot_handler(Some(Arc::new(move |canvas, requested, times| {
        if Path::new(requested) != handler_dir {
            return Err(format!(
                "snapshot directory must match the path supplied by the Gaanim host: {}",
                handler_dir.display()
            ));
        }
        gaanim_diff::capture_canvas_sized(canvas, &handler_dir, times, height)
            .map(|manifest| manifest.snapshots.len())
            .map_err(|error| error.to_string())
    })));
    let _snapshot_handler = SnapshotHandlerGuard;

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
pub fn load_script_canvas(script_path: &Path) -> Result<gaanim_api::canvas::SceneModel, String> {
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

/// Run a Python API-contract validator against the builtin PyO3 module.
pub fn validate_python_api(script_path: &Path) -> Result<(), String> {
    Python::attach(|py| {
        bootstrap_gaanim_package(py)?;
        let path = python_path(script_path);
        let path = path.to_string_lossy();
        let code = format!(
            "import runpy\ntry:\n    runpy.run_path(r'{path}', run_name='__main__')\nexcept SystemExit as exc:\n    if exc.code not in (None, 0):\n        raise\n"
        );
        let result = py.run(&std::ffi::CString::new(code).unwrap(), None, None);
        flush_python_output(py);
        result
    })
    .map_err(|error| Python::attach(|py| format_py_traceback(py, &error)))
}

/// The path Python code should see for a script. Canonical Windows paths carry
/// the `\\?\` verbatim prefix, which breaks string comparisons and
/// `Path.relative_to` against ordinary paths in user code (`__file__`,
/// `sys.path`). The prefix is dropped when the plain form is still valid.
fn python_path(path: &Path) -> PathBuf {
    gaanim_core::console::plain_path(path)
}

/// Execute a Python file by path inside the given interpreter, in a fresh
/// `__main__` namespace so each re-run is isolated from the previous one.
fn run_script_file(py: Python<'_>, path: &Path) -> PyResult<()> {
    let path = &python_path(path);
    let path_str = path
        .to_str()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("script path is not UTF-8"))?;

    prepare_script_execution(py, path)?;

    // Build a tiny bootstrap that runs the file as __main__.
    // Using runpy.run_path executes the file with a fresh __main__ module,
    // which gives each reload a clean global namespace.
    let code = format!(
        "import runpy, sys\n\
         sys.argv = [r'{path_str}']\n\
         runpy.run_path(r'{path_str}', run_name='__main__')\n"
    );
    let result = py.run(&std::ffi::CString::new(code).unwrap(), None, None);
    flush_python_output(py);
    result
}

/// Piped stdout is block-buffered and the embedded interpreter is never
/// finalized, so script output would be lost at process exit without this.
fn flush_python_output(py: Python<'_>) {
    let Ok(sys) = py.import("sys") else {
        return;
    };
    for name in ["stdout", "stderr"] {
        if let Ok(stream) = sys.getattr(name)
            && !stream.is_none()
        {
            let _ = stream.call_method0("flush");
        }
    }
}

fn prepare_script_execution(py: Python<'_>, path: &Path) -> PyResult<()> {
    let root = gaanim_project::find_project_for_script(path)
        .map(|project| project.root)
        .or_else(|| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf());
    evict_project_modules(py, &root)?;

    let mut import_paths = Vec::new();
    let source_root = root.join("src");
    if source_root.is_dir() {
        import_paths.push(source_root);
    }
    if let Some(parent) = path.parent() {
        import_paths.push(parent.to_path_buf());
    }
    import_paths.push(root);
    import_paths.dedup();

    let sys_path = py.import("sys")?.getattr("path")?;
    for import_path in import_paths.iter().rev() {
        let import_path = import_path.to_string_lossy().into_owned();
        while sys_path.contains(&import_path)? {
            sys_path.call_method1("remove", (&import_path,))?;
        }
        sys_path.call_method1("insert", (0, import_path))?;
    }
    py.import("importlib")?.call_method0("invalidate_caches")?;
    Ok(())
}

/// Remove the project's modules from `sys.modules`, so the next run imports
/// them again, keeping the bytecode of those that did not change.
fn evict_project_modules(py: Python<'_>, root: &Path) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    let modules = modules.cast::<PyDict>()?;
    let mut module_names = Vec::new();
    let mut bytecode_paths = Vec::new();

    for (name, module) in modules.iter() {
        let Ok(file) = module.getattr("__file__") else {
            continue;
        };
        let Ok(file) = file.extract::<String>() else {
            continue;
        };
        let file = PathBuf::from(file);
        if !is_reloadable_project_module(&file, root) {
            continue;
        }
        module_names.push(name.unbind());
        let is_source = file
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("py"));
        if is_source
            && let Ok(cached) = module.getattr("__cached__")
            && let Ok(cached) = cached.extract::<String>()
        {
            bytecode_paths.push((file, PathBuf::from(cached)));
        }
    }

    for name in module_names {
        modules.del_item(name.bind(py))?;
    }
    for (source, bytecode) in bytecode_paths {
        if checked_bytecode(py, &source, &bytecode).is_none() {
            let _ = std::fs::remove_file(bytecode);
        }
    }
    Ok(())
}

/// Flags of a pyc that Python validates against its source's hash.
const CHECKED_HASH_PYC: u32 = 0b11;

/// Make `bytecode`, Python's cache of `source`, one that it validates by the
/// source's hash, when it provably holds the current source. `None` when it
/// may not and must be removed.
///
/// Python validates a timestamp pyc by the source's size and mtime in whole
/// seconds, so an edit that keeps the size within the second it was cached
/// would run stale code. A timestamp pyc written after the source last
/// changed holds the current source, and becomes a hash-checked one; Python
/// then writes hash-checked pycs for that module, so unchanged modules
/// never compile again.
fn checked_bytecode(py: Python<'_>, source: &Path, bytecode: &Path) -> Option<()> {
    let util = py.import("importlib.util").ok()?;
    let magic = util.getattr("MAGIC_NUMBER").ok()?;
    let magic = magic.cast::<PyBytes>().ok()?.as_bytes();
    let mut pyc = std::fs::read(bytecode).ok()?;
    if pyc.len() < 16 || pyc[..4] != *magic {
        return None;
    }
    let word = |at: usize| u32::from_le_bytes(pyc[at..at + 4].try_into().unwrap());
    match word(4) {
        CHECKED_HASH_PYC => return Some(()),
        0 => {}
        // Python never checks an unchecked hash pyc against its source.
        _ => return None,
    }
    let stamp = |metadata: &std::fs::Metadata| {
        let modified = metadata.modified().ok()?;
        let seconds = modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        // Python records both in 32 bits.
        Some((modified, seconds as u32, metadata.len() as u32))
    };
    let (modified, seconds, size) = stamp(&std::fs::metadata(source).ok()?)?;
    let cached_at = std::fs::metadata(bytecode).ok()?.modified().ok()?;
    if (word(8), word(12)) != (seconds, size) || cached_at <= modified {
        return None;
    }
    let content = std::fs::read(source).ok()?;
    // The source must not change while it is hashed.
    if stamp(&std::fs::metadata(source).ok()?)?.0 != modified {
        return None;
    }
    let hash = util
        .call_method1("source_hash", (PyBytes::new(py, &content),))
        .ok()?;
    let hash = hash.cast::<PyBytes>().ok()?.as_bytes();
    if hash.len() != 8 {
        return None;
    }
    pyc[4..8].copy_from_slice(&CHECKED_HASH_PYC.to_le_bytes());
    pyc[8..16].copy_from_slice(hash);
    // Replace the file whole, as Python writes it, so no import reads half.
    let staged = bytecode.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&staged, &pyc).ok()?;
    if std::fs::rename(&staged, bytecode).is_err() {
        let _ = std::fs::remove_file(&staged);
        return None;
    }
    Some(())
}

fn is_reloadable_project_module(path: &Path, root: &Path) -> bool {
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let Ok(relative) = absolute.strip_prefix(root) else {
        return false;
    };
    if relative.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| matches!(name, ".venv" | "venv" | "env" | "__pycache__" | "target"))
    }) {
        return false;
    }
    absolute
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("py") || extension.eq_ignore_ascii_case("pyc")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracebacks_start_at_the_script() {
        let traceback = concat!(
            "Traceback (most recent call last):\n",
            "  File \"<string>\", line 3, in <module>\n",
            "  File \"<frozen runpy>\", line 287, in run_path\n",
            "  File \"/work/main.py\", line 4, in <module>\n",
            "    undefined_name\n",
            "  File \"<frozen importlib._bootstrap>\", line 1, in _find_and_load\n",
            "    ^^^^\n",
            "NameError: name 'undefined_name' is not defined\n",
        );
        assert_eq!(
            without_runner_frames(traceback),
            concat!(
                "Traceback (most recent call last):\n",
                "  File \"/work/main.py\", line 4, in <module>\n",
                "    undefined_name\n",
                "NameError: name 'undefined_name' is not defined\n",
            )
        );
    }

    /// Scripts share one interpreter and its global `sys` state (modules,
    /// path caches, streams); run them one at a time.
    fn python_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn python_sees_script_paths_without_the_windows_verbatim_prefix() {
        assert_eq!(
            python_path(Path::new(r"\\?\C:\proj\main.py")),
            PathBuf::from(r"C:\proj\main.py")
        );
        assert_eq!(
            python_path(Path::new(r"\\?\UNC\server\share\main.py")),
            PathBuf::from(r"\\server\share\main.py")
        );
        assert_eq!(
            python_path(Path::new(r"C:\proj\main.py")),
            PathBuf::from(r"C:\proj\main.py")
        );
        let long = format!(r"\\?\C:\{}\main.py", "d".repeat(300));
        assert_eq!(python_path(Path::new(&long)), PathBuf::from(&long));
    }

    fn write_project_manifest(root: &Path) {
        std::fs::write(
            root.join("gaanim.toml"),
            "name = \"reload-test\"\nkind = \"video\"\nentry = \"main.py\"\n",
        )
        .unwrap();
    }

    #[test]
    fn project_src_is_available_without_entrypoint_path_hacks() {
        let _python = python_lock();
        Python::initialize();
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("src/reload_src_case");
        std::fs::create_dir_all(&source).unwrap();
        write_project_manifest(temp.path());
        std::fs::write(source.join("__init__.py"), "VALUE = 'from-src'\n").unwrap();
        let output = temp.path().join("result.txt");
        let entry = temp.path().join("main.py");
        std::fs::write(
            &entry,
            format!(
                "from pathlib import Path\nfrom reload_src_case import VALUE\nPath({:?}).write_text(VALUE)\n",
                output
            ),
        )
        .unwrap();

        Python::attach(|py| run_script_file(py, &entry)).unwrap();
        assert_eq!(std::fs::read_to_string(output).unwrap(), "from-src");
    }

    #[test]
    fn rerun_reimports_changed_project_modules() {
        let _python = python_lock();
        Python::initialize();
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("src");
        let package = source_root.join("reload_cache_case");
        std::fs::create_dir_all(&package).unwrap();
        write_project_manifest(temp.path());
        std::fs::write(package.join("__init__.py"), "VALUE = 'first'\n").unwrap();
        let output = temp.path().join("result.txt");
        let entry = temp.path().join("main.py");
        std::fs::write(
            &entry,
            format!(
                "import sys\nfrom pathlib import Path\nsys.path.insert(0, {:?})\nfrom reload_cache_case import VALUE\nPath({:?}).write_text(VALUE)\n",
                source_root, output
            ),
        )
        .unwrap();

        Python::attach(|py| run_script_file(py, &entry)).unwrap();
        std::fs::write(package.join("__init__.py"), "VALUE = 'second-value'\n").unwrap();
        Python::attach(|py| run_script_file(py, &entry)).unwrap();

        assert_eq!(std::fs::read_to_string(output).unwrap(), "second-value");
    }

    #[test]
    fn reruns_keep_unchanged_bytecode_and_see_edits_of_the_same_size() {
        let _python = python_lock();
        Python::initialize();
        if Python::attach(|py| {
            py.import("sys")
                .and_then(|sys| sys.getattr("dont_write_bytecode"))
                .and_then(|flag| flag.extract::<bool>())
                .unwrap_or(true)
        }) {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        write_project_manifest(temp.path());
        let module = temp.path().join("same_size_case.py");
        std::fs::write(&module, "VALUE = 'first'\n").unwrap();
        let output = temp.path().join("result.txt");
        let entry = temp.path().join("main.py");
        std::fs::write(
            &entry,
            format!(
                "from pathlib import Path\nimport same_size_case\n\
                 Path({output:?}).write_text(same_size_case.VALUE)\n"
            ),
        )
        .unwrap();
        let run = || {
            Python::attach(|py| run_script_file(py, &entry)).unwrap();
            std::fs::read_to_string(&output).unwrap()
        };
        let flags = || {
            let cache = std::fs::read_dir(temp.path().join("__pycache__"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("same_size_case."))
                })
                .unwrap();
            let pyc = std::fs::read(cache).unwrap();
            u32::from_le_bytes(pyc[4..8].try_into().unwrap())
        };

        assert_eq!(run(), "first");
        assert_eq!(flags(), 0, "Python caches a new module by timestamp");
        // Same size, most likely within the second the bytecode records.
        std::fs::write(&module, "VALUE = 'other'\n").unwrap();
        assert_eq!(run(), "other");
        // Unchanged since: kept, and checked by the source's hash.
        assert_eq!(run(), "other");
        assert_eq!(flags(), CHECKED_HASH_PYC);
        std::fs::write(&module, "VALUE = 'third'\n").unwrap();
        assert_eq!(run(), "third");
        assert_eq!(
            flags(),
            CHECKED_HASH_PYC,
            "Python keeps checking it by hash"
        );
    }

    #[test]
    fn script_output_is_flushed_after_success_and_failure() {
        let _python = python_lock();
        Python::initialize();
        let temp = tempfile::tempdir().unwrap();
        write_project_manifest(temp.path());
        for (name, tail) in [("ok", ""), ("error", "raise RuntimeError('boom')\n")] {
            let output = temp.path().join(format!("{name}.txt"));
            let entry = temp.path().join(format!("{name}.py"));
            // A buffered stream only publishes its text when flushed. It binds
            // its writer eagerly because runpy may clear the script globals.
            std::fs::write(
                &entry,
                format!(
                    "import sys\nfrom pathlib import Path\n\
                     class Buffered:\n    parts = []\n\
                     \x20   def write(self, text):\n        self.parts.append(text)\n        return len(text)\n\
                     \x20   def flush(self, publish=Path({output:?}).write_text):\n        publish(''.join(self.parts))\n\
                     sys.stdout = Buffered()\nprint('{name}-output')\n{tail}"
                ),
            )
            .unwrap();

            let result = Python::attach(|py| {
                let result = run_script_file(py, &entry);
                let sys = py.import("sys").unwrap();
                sys.setattr("stdout", sys.getattr("__stdout__").unwrap())
                    .unwrap();
                result
            });
            assert_eq!(result.is_ok(), tail.is_empty());
            assert_eq!(
                std::fs::read_to_string(output).unwrap().trim_end(),
                format!("{name}-output")
            );
        }
    }
}
