//! The interface between the `gaanim` executable and its Python plugin.
//!
//! `gaanim` does not link Python, so it starts even where Python is missing:
//! Home and playback bundles need none. When a script or project opens, it
//! selects a runtime, loads the `gaanim_python_plugin` library that sits next
//! to it, and calls its [`ENTRY_POINT`] for the [`PythonPlugin`] below. Both
//! sides link the same `gaanim_engine` library, so they exchange Rust types
//! (scenes, channels, callbacks) directly; [`engine_marker`] confirms it.

use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;
use gaanim_api::canvas::SceneModel;
use gaanim_api::host::ReloadPayload;

/// File stem of the plugin library, as `libloading::library_filename` takes it.
pub const LIBRARY_NAME: &str = "gaanim_python_plugin";

/// Symbol of the plugin's [`EntryPointFn`]; its suffix is the interface version.
pub const ENTRY_POINT: &[u8] = b"gaanim_python_plugin_v2";

/// Returns the plugin's interface, which lives as long as the process.
pub type EntryPointFn = unsafe extern "C" fn() -> *const PythonPlugin;

/// What the plugin does for the executable. Every function expects
/// [`PythonPlugin::initialize`] to have run once first.
pub struct PythonPlugin {
    /// [`engine_marker`] as the plugin computes it.
    pub engine_marker: fn() -> usize,
    /// Version of the Gaanim build the plugin belongs to.
    pub version: &'static str,
    /// Register the `gaanim_core` module, start the interpreter, and add the
    /// site-packages of `venv`, if any, to `sys.path`.
    pub initialize: fn(venv: Option<&Path>),
    /// Run a script once and return the scene its `scene.render()` submitted.
    pub load_script_canvas: fn(script: &Path) -> Result<SceneModel, String>,
    /// Run a script that calls `scene.snapshots(...)` into `dir`.
    pub capture_script_snapshots:
        fn(script: &Path, dir: &Path, height: Option<u32>) -> Result<(), String>,
    /// Run an API contract validator against the builtin module.
    pub validate_python_api: fn(validator: &Path) -> Result<(), String>,
    /// Run a script on its own thread, sending each rendered scene to
    /// `payloads` and each traceback to `errors`, until the session drops.
    pub spawn_script: fn(
        script: PathBuf,
        payloads: Sender<ReloadPayload>,
        errors: Sender<String>,
    ) -> Box<dyn ScriptSession>,
}

/// A script running on the plugin's thread.
pub trait ScriptSession: Send + Sync {
    /// Run the script again after its source changed.
    fn request_rerun(&self);
    /// Run the script again after project assets changed, reading them anew.
    fn request_asset_reload(&self);
    /// A [`ScriptSession::request_asset_reload`] that other threads can keep.
    fn asset_reload_handle(&self) -> Box<dyn Fn() + Send + Sync>;
}

static ENGINE_MARKER: u8 = 0;

/// Address of a static in the engine. The executable and the plugin compute
/// the same value only when they share one loaded engine library.
pub fn engine_marker() -> usize {
    &raw const ENGINE_MARKER as usize
}
