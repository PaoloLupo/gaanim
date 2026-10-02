//! Python support for `gaanim`, as a library loaded on demand.
//!
//! The executable does not link Python: it loads this library once a script
//! or project opens and it has found a runtime. The library embeds CPython,
//! registers the `gaanim_core` module, and runs scripts on their own thread;
//! see [`gaanim_editor::python_plugin`] for the interface.

// Take the engine crates from the shared engine library.
use gaanim_engine as _;

mod python_home;
mod script_runner;

use gaanim_editor::python_plugin::{PythonPlugin, ScriptSession};
use std::path::Path;

static PLUGIN: PythonPlugin = PythonPlugin {
    engine_marker: gaanim_editor::python_plugin::engine_marker,
    version: env!("CARGO_PKG_VERSION"),
    initialize,
    load_script_canvas: script_runner::load_script_canvas,
    capture_script_snapshots: script_runner::capture_script_snapshots,
    validate_python_api: script_runner::validate_python_api,
    spawn_script: |script, payloads, errors| {
        Box::new(script_runner::ScriptRunner::spawn(script, payloads, errors))
    },
};

/// The plugin's interface; see [`gaanim_editor::python_plugin::ENTRY_POINT`].
#[unsafe(no_mangle)]
pub extern "C" fn gaanim_python_plugin_v2() -> *const PythonPlugin {
    &PLUGIN
}

fn initialize(venv: Option<&Path>) {
    static START: std::sync::Once = std::sync::Once::new();
    START.call_once(|| {
        gaanim_python::register_inittab();
        pyo3::Python::initialize();
    });
    if let Some(venv) = venv {
        python_home::inject_venv_site_packages(venv);
    }
}

impl ScriptSession for script_runner::ScriptRunner {
    fn request_rerun(&self) {
        script_runner::ScriptRunner::request_rerun(self);
    }

    fn request_asset_reload(&self) {
        script_runner::ScriptRunner::request_asset_reload(self);
    }

    fn asset_reload_handle(&self) -> Box<dyn Fn() + Send + Sync> {
        Box::new(script_runner::ScriptRunner::asset_reload_handle(self))
    }
}
