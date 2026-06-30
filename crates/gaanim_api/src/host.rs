//! Language-agnostic host bridge for script frontends.
//!
//! Scripting bindings should build canonical `gaanim_api` values (currently
//! [`Canvas`](crate::canvas::Canvas)) and submit them here. The editor/hot-reload
//! host listens for these payloads and replays them into Bevy.

use std::sync::{Mutex, OnceLock};

use crossbeam_channel::Sender;

use crate::canvas::Canvas;

/// A complete, self-contained animation description ready to replay into Bevy.
#[derive(Debug, Clone)]
pub struct ReloadPayload {
    pub canvas: Canvas,
}

static HOST_TX: OnceLock<Mutex<Option<Sender<ReloadPayload>>>> = OnceLock::new();

fn tx_slot() -> &'static Mutex<Option<Sender<ReloadPayload>>> {
    HOST_TX.get_or_init(|| Mutex::new(None))
}

/// Install or clear the process-local host channel.
pub fn set_host_sender(tx: Option<Sender<ReloadPayload>>) {
    *tx_slot().lock().expect("host tx poisoned") = tx;
}

/// Submit a canvas to the embedded host. Returns `false` when no host is
/// attached, e.g. when a script is run directly with plain Python.
pub fn send_to_host(canvas: Canvas) -> bool {
    let guard = tx_slot().lock().expect("host tx poisoned");
    match &*guard {
        Some(tx) => tx.send(ReloadPayload { canvas }).is_ok(),
        None => false,
    }
}
