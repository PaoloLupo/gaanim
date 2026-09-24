//! File-system watcher that triggers a script re-run when project sources or
//! assets change.

use notify::{EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// What changed in the project since the last notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectChange {
    /// Only Python sources changed.
    Source,
    /// A non-Python project file changed (images, SVG, Lottie, WGSL, Typst,
    /// data, ...); cached assets must be read again.
    Assets,
}

/// Handle to the watcher thread. Exposes a [`Receiver`](mpsc::Receiver) that
/// fires whenever a Python source or project asset changes.
pub struct FileWatcher {
    pub changed_rx: mpsc::Receiver<ProjectChange>,
    pub stop: Arc<AtomicBool>,
}

impl FileWatcher {
    pub fn spawn(script_path: PathBuf) -> Self {
        let scope = WatchScope::for_script(script_path);
        let (changed_tx, changed_rx) = mpsc::channel::<ProjectChange>();
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

    /// How a change to `path` affects the scene, if at all.
    fn classify(&self, path: &Path) -> Option<ProjectChange> {
        if path == self.script_path {
            return Some(ProjectChange::Source);
        }
        let relative = path.strip_prefix(&self.root).ok()?;
        let ignored = relative.components().any(|component| {
            component.as_os_str().to_str().is_some_and(|name| {
                // Hidden entries cover .git, .venv, and editor swap files.
                name.starts_with('.')
                    || matches!(
                        name,
                        "venv" | "env" | "__pycache__" | "exports" | "snapshots" | "target"
                    )
            })
        });
        if ignored {
            return None;
        }
        let name = path.file_name()?.to_str()?;
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        // Editor backups and atomic-save temporaries are not assets.
        if name.ends_with('~')
            || matches!(
                extension.as_str(),
                "swp" | "swx" | "tmp" | "bak" | "pyc" | "lock"
            )
            || name.chars().all(|character| character.is_ascii_digit())
        {
            return None;
        }
        Some(if extension == "py" {
            ProjectChange::Source
        } else {
            ProjectChange::Assets
        })
    }
}

fn watch_loop(scope: WatchScope, stop: Arc<AtomicBool>, changed_tx: mpsc::Sender<ProjectChange>) {
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
        "[gaanim] watching Python sources and assets under: {}",
        scope.root.display()
    );

    let debounce = Duration::from_millis(200);
    let poll_interval = Duration::from_millis(250);
    let mut reload_deadline = None;
    let mut pending = None;

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
                let Some(change) = event_change(&event.paths, &scope) else {
                    continue;
                };
                pending = pending.max(Some(change));
                reload_deadline = Some(Instant::now() + debounce);
            }
            Ok(Err(e)) => {
                eprintln!("[gaanim] watcher error: {e}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if reload_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    reload_deadline = None;
                    if let Some(change) = pending.take() {
                        eprintln!(
                            "[gaanim] {} changed, reloading...",
                            match change {
                                ProjectChange::Source => "Python source",
                                ProjectChange::Assets => "project asset",
                            }
                        );
                        let _ = changed_tx.send(change);
                    }
                }
            }
            Err(_) => break,
        }
    }
}

/// The strongest change among `paths`: any asset outranks sources.
fn event_change(paths: &[PathBuf], scope: &WatchScope) -> Option<ProjectChange> {
    paths.iter().filter_map(|path| scope.classify(path)).max()
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
        assert_eq!(
            event_change(&[section], &scope),
            Some(ProjectChange::Source)
        );
        assert_eq!(
            event_change(
                &[temp.path().join(".venv/Lib/site-packages/dependency.py")],
                &scope
            ),
            None
        );
        assert_eq!(
            event_change(&[temp.path().join("exports/generated.py")], &scope),
            None
        );
    }

    #[test]
    fn project_assets_trigger_an_asset_reload() {
        let temp = tempfile::tempdir().unwrap();
        let scope = WatchScope {
            script_path: temp.path().join("main.py"),
            root: temp.path().to_path_buf(),
        };
        for asset in [
            "assets/cover.png",
            "assets/diagram.SVG",
            "assets/pulse.json",
            "assets/background.wgsl",
            "src/tesis/notes.typ",
            "data/results.csv",
            "gaanim.toml",
        ] {
            assert_eq!(
                event_change(&[temp.path().join(asset)], &scope),
                Some(ProjectChange::Assets),
                "{asset}"
            );
        }
        // An asset outranks a source saved in the same batch.
        assert_eq!(
            event_change(
                &[
                    temp.path().join("main.py"),
                    temp.path().join("assets/a.png")
                ],
                &scope
            ),
            Some(ProjectChange::Assets)
        );
        for ignored in [
            "assets/.cover.png.swp",
            "assets/cover.png~",
            "assets/4913",
            "exports/talk.mp4",
            "snapshots/seek_0000.png",
            ".git/index",
            "__pycache__/main.cpython-314.pyc",
            "uv.lock",
        ] {
            assert_eq!(
                event_change(&[temp.path().join(ignored)], &scope),
                None,
                "{ignored}"
            );
        }
    }
}
