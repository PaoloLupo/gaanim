//! Language-agnostic host bridge for script frontends.
//!
//! Scripting bindings should build canonical `gaanim_api` values (currently
//! [`Canvas`](crate::canvas::Canvas)) and submit them here. The editor/hot-reload
//! host listens for these payloads and replays them into Bevy.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::canvas::Canvas;

/// A complete, self-contained animation description ready to replay into Bevy.
#[derive(Debug, Clone)]
pub struct ReloadPayload {
    pub canvas: Canvas,
    /// Time spent executing the Python script before it submitted the scene.
    pub compile_duration: Duration,
}

static HOST_TX: OnceLock<Mutex<Option<Sender<ReloadPayload>>>> = OnceLock::new();
static HOST_COMPILE_STARTED_AT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

fn tx_slot() -> &'static Mutex<Option<Sender<ReloadPayload>>> {
    HOST_TX.get_or_init(|| Mutex::new(None))
}

fn compile_started_at_slot() -> &'static Mutex<Option<Instant>> {
    HOST_COMPILE_STARTED_AT.get_or_init(|| Mutex::new(None))
}

/// Install or clear the process-local host channel.
pub fn set_host_sender(tx: Option<Sender<ReloadPayload>>) {
    *tx_slot().lock().expect("host tx poisoned") = tx;
}

/// Mark the start of an embedded Python execution so the next scene payload
/// can report how long that hot reload took.
pub fn set_compile_started_at(started_at: Option<Instant>) {
    *compile_started_at_slot()
        .lock()
        .expect("host compile timer poisoned") = started_at;
}

/// Submit a canvas to the embedded host. Returns `false` when no host is
/// attached, e.g. when a script is run directly with plain Python.
pub fn send_to_host(canvas: Canvas) -> bool {
    let guard = tx_slot().lock().expect("host tx poisoned");
    let compile_duration = compile_started_at_slot()
        .lock()
        .expect("host compile timer poisoned")
        .map(|started_at| started_at.elapsed())
        .unwrap_or_default();
    match &*guard {
        Some(tx) => tx
            .send(ReloadPayload {
                canvas,
                compile_duration,
            })
            .is_ok(),
        None => false,
    }
}
