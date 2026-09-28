//! Python support, loaded the first time a script needs it.
//!
//! `gaanim` does not link Python: Home, playback bundles, and every command
//! that does not run a script work without it. Running a script selects a
//! runtime (the project's `.venv`, else a system Python), prepares this
//! process to load it, and loads the `gaanim_python_plugin` library installed
//! next to the executable. One process runs one interpreter, so the first
//! runtime selected stays for the rest of the process.

use gaanim_editor::python_plugin::{self, EntryPointFn, PythonPlugin};
use gaanim_project::EnvironmentProbe;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The loaded plugin, its interpreter started for `hint`'s environment.
///
/// `hint` is the script or project about to run; it selects the runtime the
/// first time only.
pub fn runtime(hint: &Path) -> Result<&'static PythonPlugin, String> {
    static LOADED: Mutex<Option<&'static PythonPlugin>> = Mutex::new(None);
    let mut loaded = LOADED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(plugin) = *loaded {
        return Ok(plugin);
    }
    let probe = EnvironmentProbe::detect(Some(hint));
    let venv = gaanim_project::activate_environment(&probe)?;
    prepare_loader(&probe)?;
    let plugin = load_plugin()?;
    (plugin.initialize)(venv.as_deref());
    *loaded = Some(plugin);
    Ok(plugin)
}

/// What to tell the user when [`runtime`] fails for want of Python.
pub fn install_hint() -> String {
    format!(
        "Install {} (for example `uv python install 3.14`) and retry, or run `gaanim --help`.",
        gaanim_project::python_requirement()
    )
}

/// Make the selected runtime loadable and startable in this process.
///
/// On Windows `activate_environment` already put its directory on `PATH`,
/// where the loader finds `python3.dll`. Elsewhere `libpython` is loaded
/// here, globally: the plugin's reference to it then resolves to this copy,
/// and extension modules (NumPy and others), which leave Python's symbols to
/// the process, find them.
fn prepare_loader(probe: &EnvironmentProbe) -> Result<(), String> {
    if let Some(home) = gaanim_project::embedded_python_home(probe) {
        // SAFETY: the interpreter reads it when it starts, after this. Command
        // line runs get here before starting any thread; a project opened
        // from Home gets here with the app's threads running and, like the
        // `PATH` update in `activate_environment`, relies on them not reading
        // the environment at that moment.
        unsafe { std::env::set_var("PYTHONHOME", home) };
    }
    #[cfg(unix)]
    load_python_library(probe)?;
    Ok(())
}

#[cfg(unix)]
fn load_python_library(probe: &EnvironmentProbe) -> Result<(), String> {
    use libloading::os::unix::{Library, RTLD_GLOBAL, RTLD_NOW};

    let candidates = gaanim_project::python_library_candidates(probe);
    let mut errors = Vec::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| !candidate.is_absolute() || candidate.is_file())
    {
        // SAFETY: libpython's initializers only register the library; the
        // interpreter starts later, in the plugin.
        match unsafe { Library::open(Some(candidate), RTLD_NOW | RTLD_GLOBAL) } {
            Ok(library) => {
                // Python stays loaded for the life of the process.
                std::mem::forget(library);
                return Ok(());
            }
            Err(error) => errors.push(error.to_string()),
        }
    }
    if candidates.is_empty() {
        return Ok(());
    }
    Err(format!(
        "could not load the Python library of the selected runtime: {}",
        errors.join("; ")
    ))
}

/// Load the plugin next to the executable and check it belongs to this build.
fn load_plugin() -> Result<&'static PythonPlugin, String> {
    let path = plugin_path()?;
    if !path.is_file() {
        return Err(format!(
            "Python support ({}) is missing next to the Gaanim executable; reinstall Gaanim",
            path.display()
        ));
    }
    // SAFETY: the plugin is Gaanim's own library; its initializers only
    // register it. Its interface is checked against this build below.
    let library = unsafe { libloading::Library::new(&path) }.map_err(|error| {
        format!(
            "could not load {}: {error}. If Gaanim was updated or rebuilt, its files may \
                 come from different builds; reinstall Gaanim or rebuild it with `just build`",
            path.display()
        )
    })?;
    // SAFETY: `ENTRY_POINT` names a function of type `EntryPointFn`.
    let entry = unsafe { library.get::<EntryPointFn>(python_plugin::ENTRY_POINT) }
        .map_err(|error| format!("{} is not Gaanim's Python plugin: {error}", path.display()))?;
    // SAFETY: the entry point returns a pointer to a static.
    let plugin: &'static PythonPlugin = unsafe { &*entry() };
    // Script threads and the interpreter live until the process exits, so the
    // plugin is never unloaded.
    std::mem::forget(library);
    if plugin.version != env!("CARGO_PKG_VERSION")
        || (plugin.engine_marker)() != python_plugin::engine_marker()
    {
        return Err(format!(
            "{} belongs to another build of Gaanim ({}); reinstall Gaanim or rebuild it with `just build`",
            path.display(),
            plugin.version
        ));
    }
    Ok(plugin)
}

fn plugin_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("could not locate the Gaanim executable: {error}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", exe.display()))?;
    Ok(dir.join(libloading::library_filename(python_plugin::LIBRARY_NAME)))
}
