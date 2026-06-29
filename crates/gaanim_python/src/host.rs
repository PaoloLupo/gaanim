//! Host bridge for embedded execution.
//!
//! When Gaanim runs as a host application embedding a Python interpreter, the
//! user's script builds a `Scene`/`Engine` and calls `.render()`. Instead of
//! opening a window (the old standalone behaviour), the drained `DeferredOp`
//! queue is packaged into a [`ReloadPayload`] and pushed through a
//! process-local channel to the host's Bevy event loop, which rebuilds the
//! scene in place — enabling hot-reload without restarting the window.

use crate::scene::DeferredOp;
use crossbeam_channel::Sender;
use gaanim_core::peniko;
use std::sync::{Mutex, OnceLock};

/// A complete, self-contained description of a scene ready to be replayed
/// into a Bevy [`World`](bevy::prelude::World).
#[derive(Debug, Clone)]
pub struct ReloadPayload {
    pub ops: Vec<DeferredOp>,
    pub width: u32,
    pub height: u32,
    pub background: Option<peniko::Color>,
}

static HOST_TX: OnceLock<Mutex<Option<Sender<ReloadPayload>>>> = OnceLock::new();

fn tx_slot() -> &'static Mutex<Option<Sender<ReloadPayload>>> {
    HOST_TX.get_or_init(|| Mutex::new(None))
}

/// Called by the host (the `gaanim` binary) to install the channel endpoint
/// that receives scene payloads from the embedded script.
pub fn set_host_sender(tx: Option<Sender<ReloadPayload>>) {
    *tx_slot().lock().expect("host tx poisoned") = tx;
}

/// Called from `Scene::render()` / `Engine::render()` inside the embedded
/// script. Returns `false` when no host is attached (i.e. the script is run
/// with plain `python`), so the Python method can raise a helpful error.
pub fn send_to_host(payload: ReloadPayload) -> bool {
    let guard = tx_slot().lock().expect("host tx poisoned");
    match &*guard {
        Some(tx) => tx.send(payload).is_ok(),
        None => false,
    }
}
