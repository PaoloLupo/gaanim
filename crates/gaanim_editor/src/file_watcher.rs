//! File-system watcher that triggers a script re-run on save.

use notify::{EventKind, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Handle to the watcher thread. Exposes a [`Receiver`](mpsc::Receiver) that
/// fires whenever the watched script file (or its parent directory) changes.
pub struct FileWatcher {
    pub changed_rx: mpsc::Receiver<()>,
    pub stop: Arc<AtomicBool>,
}

impl FileWatcher {
    pub fn spawn(script_path: PathBuf) -> Self {
        // Canonicalize so event paths (always absolute) match.
        let script_path = script_path.canonicalize().unwrap_or(script_path);
        let (changed_tx, changed_rx) = mpsc::channel::<()>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        std::thread::Builder::new()
            .name("gaanim-watcher".into())
            .spawn(move || {
                watch_loop(script_path, stop_clone, changed_tx);
            })
            .expect("failed to spawn watcher thread");

        Self { changed_rx, stop }
    }
}

fn watch_loop(script_path: PathBuf, stop: Arc<AtomicBool>, changed_tx: mpsc::Sender<()>) {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[gaanim] failed to start file watcher: {e}");
            return;
        }
    };

    if let Err(e) = watcher.watch(&script_path, RecursiveMode::NonRecursive) {
        eprintln!("[gaanim] failed to watch {}: {e}", script_path.display());
    }
    if let Some(parent) = script_path.parent() {
        if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
            eprintln!("[gaanim] failed to watch dir {}: {e}", parent.display());
        }
    }
    eprintln!("[gaanim] watching for changes: {}", script_path.display());

    let debounce = Duration::from_millis(200);
    let mut last_fire = Instant::now() - debounce;

    while !stop.load(Ordering::SeqCst) {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(Ok(event)) => {
                let relevant = matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                );
                if !relevant {
                    continue;
                }
                let touches_script = event.paths.iter().any(|p| {
                    p == &script_path || script_path.parent().map(|d| p == d).unwrap_or(false)
                });
                if !touches_script {
                    continue;
                }
                let now = Instant::now();
                if now.duration_since(last_fire) < debounce {
                    continue;
                }
                last_fire = now;
                eprintln!("[gaanim] file changed, reloading...");
                let _ = changed_tx.send(());
            }
            Ok(Err(e)) => {
                eprintln!("[gaanim] watcher error: {e}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
    }
}
