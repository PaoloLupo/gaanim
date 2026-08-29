//! File-system watcher that triggers a script re-run on save.

use notify::{EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
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
        let scope = WatchScope::for_script(script_path);
        let (changed_tx, changed_rx) = mpsc::channel::<()>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        std::thread::Builder::new()
            .name("gaanim-watcher".into())
            .spawn(move || {
                watch_loop(scope, stop_clone, changed_tx);
            })
            .expect("failed to spawn watcher thread");

        Self { changed_rx, stop }
    }
}

#[derive(Debug)]
struct WatchScope {
    script_path: PathBuf,
    root: PathBuf,
}

impl WatchScope {
    fn for_script(script_path: PathBuf) -> Self {
        let script_path = script_path.canonicalize().unwrap_or(script_path);
        let root = gaanim_project::find_project_for_script(&script_path)
            .map(|project| project.root)
            .or_else(|| script_path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| script_path.clone());
        Self { script_path, root }
    }

    fn contains_reloadable_source(&self, path: &Path) -> bool {
        if path == self.script_path {
            return true;
        }
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        if relative.components().any(|component| {
            component.as_os_str().to_str().is_some_and(|name| {
                matches!(
                    name,
                    ".git"
                        | ".venv"
                        | "venv"
                        | "env"
                        | "__pycache__"
                        | "exports"
                        | "snapshots"
                        | "target"
                )
            })
        }) {
            return false;
        }
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
    }
}

fn watch_loop(scope: WatchScope, stop: Arc<AtomicBool>, changed_tx: mpsc::Sender<()>) {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[gaanim] failed to start file watcher: {e}");
            return;
        }
    };

    if let Err(e) = watcher.watch(&scope.root, RecursiveMode::Recursive) {
        eprintln!(
            "[gaanim] failed to watch project sources under {}: {e}",
            scope.root.display()
        );
        return;
    }
    eprintln!(
        "[gaanim] watching Python sources under: {}",
        scope.root.display()
    );

    let debounce = Duration::from_millis(200);
    let poll_interval = Duration::from_millis(250);
    let mut reload_deadline = None;

    while !stop.load(Ordering::SeqCst) {
        let timeout = reload_deadline
            .map(|deadline: Instant| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(poll_interval)
            .min(poll_interval);
        match rx.recv_timeout(timeout) {
            Ok(Ok(event)) => {
                let relevant = matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                );
                if !relevant {
                    continue;
                }
                let touches_script = event_touches_script(&event.paths, &scope);
                if !touches_script {
                    continue;
                }
                reload_deadline = Some(Instant::now() + debounce);
            }
            Ok(Err(e)) => {
                eprintln!("[gaanim] watcher error: {e}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if reload_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    reload_deadline = None;
                    eprintln!("[gaanim] Python source changed, reloading...");
                    let _ = changed_tx.send(());
                }
            }
            Err(_) => break,
        }
    }
}

fn event_touches_script(paths: &[PathBuf], scope: &WatchScope) -> bool {
    paths
        .iter()
        .any(|path| scope.contains_reloadable_source(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_python_modules_trigger_hot_reload() {
        let temp = tempfile::tempdir().unwrap();
        let entry = temp.path().join("main.py");
        let section = temp.path().join("src/tesis/sections/title.py");
        std::fs::create_dir_all(section.parent().unwrap()).unwrap();
        std::fs::write(&entry, "").unwrap();
        std::fs::write(&section, "").unwrap();

        let scope = WatchScope {
            script_path: entry,
            root: temp.path().to_path_buf(),
        };
        assert!(event_touches_script(&[section], &scope));
        assert!(!event_touches_script(
            &[temp.path().join(".venv/Lib/site-packages/dependency.py")],
            &scope
        ));
        assert!(!event_touches_script(
            &[temp.path().join("exports/generated.py")],
            &scope
        ));
    }
}
