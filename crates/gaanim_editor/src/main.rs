//! `gaanim` application entry point.
//!
//! Gaanim is a native binary that embeds a CPython interpreter. The user's
//! animation script (a `.py` that imports `gaanim`) is executed inside this
//! interpreter; the script *describes* the scene via the fluent API and calls
//! `.render()`, which pushes the deferred-op queue to this host's Bevy event
//! loop instead of opening its own window.
//!
//! A file watcher observes the script: on save, the script is re-run in the
//! same interpreter and the scene is rebuilt in place — hot-reload without
//! restarting the window.

use bevy::prelude::*;
use gaanim_python::host::ReloadPayload;
use pyo3::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

mod file_watcher;
mod hot_reload;
mod script_runner;

use hot_reload::{
    ReloadReceiver, ReloadStatus, reload_listener_system, reload_status_overlay_system,
};

fn main() {
    let script_path = parse_args();

    // 1. Register the `gaanim_core` module in the embedded interpreter's init
    //    table BEFORE initializing Python, so `import gaanim_core` resolves to
    //    our in-process module (no .pyd needed).
    gaanim_python::register_inittab();

    // 2. Initialize the embedded CPython interpreter.
    Python::initialize();

    // 3. Set up the host<->script channel.
    let (payload_tx, payload_rx) = crossbeam_channel::unbounded::<ReloadPayload>();

    // 4. Spawn the script-runner thread (holds the GIL, runs the script).
    let runner = script_runner::ScriptRunner::spawn(script_path.clone(), payload_tx);

    // 5. Spawn the file watcher and extract its channel endpoints.
    let file_watcher::FileWatcher { changed_rx, stop } =
        file_watcher::FileWatcher::spawn(script_path.clone());

    // 6. Bridge watcher events -> script re-run requests in a dedicated thread.
    std::thread::Builder::new()
        .name("gaanim-watcher-bridge".into())
        .spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match changed_rx.recv_timeout(std::time::Duration::from_millis(250)) {
                    Ok(()) => runner.request_rerun(),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
            }
        })
        .expect("failed to spawn watcher bridge thread");

    // 7. Build the Bevy app with the editor + renderer + reload wiring.
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Gaanim".to_string(),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(gaanim_scene::GaanimScenePlugin)
    .add_plugins(gaanim_animation::GaanimAnimationPlugin)
    .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
    .add_plugins(gaanim_text::GaanimTextPlugin)
    .add_plugins(gaanim_api::GaanimApiPlugin)
    .add_plugins(gaanim_renderer::GaanimRendererPlugin)
    .add_plugins(gaanim_editor::GaanimEditorPlugin)
    .insert_resource(ReloadReceiver { rx: payload_rx })
    .insert_resource(ReloadStatus::default())
    .add_systems(
        Update,
        (
            reload_listener_system.in_set(gaanim_scene::hierarchy::SceneSet::Input),
            reload_status_overlay_system,
        ),
    );

    app.run();
}

fn parse_args() -> PathBuf {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        eprintln!("usage: gaanim <script.py>");
        std::process::exit(2);
    };
    if first == "--help" || first == "-h" {
        eprintln!("gaanim — GPU-accelerated vector animation engine (hot-reload viewer)");
        eprintln!();
        eprintln!("usage: gaanim <script.py>");
        std::process::exit(0);
    }
    let path = PathBuf::from(&first);
    if !path.exists() {
        eprintln!("gaanim: script not found: {}", path.display());
        std::process::exit(2);
    }
    // Canonicalize so the file watcher can match absolute event paths.
    path.canonicalize().unwrap_or(path)
}
