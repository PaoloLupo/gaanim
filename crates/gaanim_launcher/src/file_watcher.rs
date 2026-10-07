//! File-system watcher that triggers a script re-run when project sources or
//! assets change.

use gaanim_core::console;
use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecursiveMode, Watcher};
use std::collections::BTreeSet;
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
        let ignored = relative
            .components()
            .any(|component| component.as_os_str().to_str().is_some_and(is_ignored_name));
        if ignored {
            return None;
        }
        let name = path.file_name()?.to_str()?;
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            extension.as_str(),
            "swp" | "swx" | "tmp" | "bak" | "pyc" | "lock"
        ) || is_save_temporary(name)
        {
            return None;
        }
        Some(if extension == "py" {
            ProjectChange::Source
        } else {
            ProjectChange::Assets
        })
    }

    /// The project's top-level folders worth watching recursively.
    fn watched_folders(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut folders: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .filter(|path| self.is_watched_folder(path))
            .collect();
        folders.sort();
        folders
    }

    /// Whether `path` is a top-level folder of the project that is watched.
    fn is_watched_folder(&self, path: &Path) -> bool {
        path.parent() == Some(self.root.as_path())
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !is_ignored_name(name))
    }
}

/// Editor backups and the temporaries of atomic saves, which write a new
/// file and rename it over the saved one: `main.py.tmp.<pid>.<hash>` (Node
/// tools such as Claude Code), `sedXXXXXX` (`sed -i`), JetBrains'
/// `___jb_tmp___`/`___jb_old___`, Emacs' `#main.py#`, Vim's `main.py~` and
/// numeric probe files.
fn is_save_temporary(name: &str) -> bool {
    let sed = name.len() == 9
        && name.starts_with("sed")
        && name[3..]
            .chars()
            .all(|character| character.is_ascii_alphanumeric());
    name.ends_with('~')
        || name.contains(".tmp.")
        || name.ends_with("___jb_tmp___")
        || name.ends_with("___jb_old___")
        || (name.len() > 1 && name.starts_with('#') && name.ends_with('#'))
        || name.chars().all(|character| character.is_ascii_digit())
        || sed
}

/// Paths reported during one debounce window, settled once it ends.
#[derive(Debug, Default)]
struct PendingChanges {
    paths: BTreeSet<PathBuf>,
    created: BTreeSet<PathBuf>,
}

impl PendingChanges {
    /// Note the project paths `event` touches. Returns whether any can
    /// affect the scene.
    fn record(&mut self, event: &notify::Event, scope: &WatchScope) -> bool {
        let relevant = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        );
        if !relevant {
            return false;
        }
        let mut recorded = false;
        for (index, path) in event.paths.iter().enumerate() {
            if scope.classify(path).is_none() {
                continue;
            }
            let created = match event.kind {
                EventKind::Create(_) => true,
                EventKind::Modify(ModifyKind::Name(RenameMode::To)) => true,
                // A rename reported as one event lists its source first.
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => index > 0,
                _ => false,
            };
            if created {
                self.created.insert(path.clone());
            }
            self.paths.insert(path.clone());
            recorded = true;
        }
        recorded
    }

    /// The strongest change the window left on disk. A saved file counts by
    /// its own name: a temporary created and renamed or removed within the
    /// window is gone, and a folder reports only that its entries changed,
    /// which they report themselves.
    fn settle(self, scope: &WatchScope) -> Option<ProjectChange> {
        let paths: Vec<PathBuf> = self
            .paths
            .iter()
            .filter(|path| !(self.created.contains(*path) && !path.exists()))
            .filter(|path| !path.is_dir())
            .cloned()
            .collect();
        event_change(&paths, scope)
    }
}

/// Entries whose changes never affect the scene. Hidden entries cover .git,
/// .venv and editor swap files.
fn is_ignored_name(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "venv" | "env" | "__pycache__" | "exports" | "snapshots" | "target"
        )
}

/// Watch the project root's own entries and each watched top-level folder
/// recursively. Watching the root recursively would also register every
/// folder of .venv and .git with the system, which can exhaust Linux's
/// inotify watches and leave hot reload off.
fn watch_project(watcher: &mut impl Watcher, scope: &WatchScope) -> notify::Result<()> {
    watcher.watch(&scope.root, RecursiveMode::NonRecursive)?;
    for folder in scope.watched_folders() {
        watch_folder(watcher, &folder);
    }
    Ok(())
}

fn watch_folder(watcher: &mut impl Watcher, folder: &Path) {
    if let Err(error) = watcher.watch(folder, RecursiveMode::Recursive) {
        console::warn(
            "watch",
            format!("could not watch {}: {error}", folder.display()),
        );
    }
}

fn watch_loop(scope: WatchScope, stop: Arc<AtomicBool>, changed_tx: mpsc::Sender<ProjectChange>) {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            console::error("watch", format!("could not start the file watcher: {e}"));
            return;
        }
    };

    if let Err(e) = watch_project(&mut watcher, &scope) {
        console::error(
            "watch",
            format!(
                "could not watch project sources under {}: {e}",
                scope.root.display()
            ),
        );
        return;
    }
    console::info(
        "watch",
        format!(
            "Watching {} · save a file to reload",
            match console::display_path(&scope.root).as_str() {
                "." => "the current folder".to_string(),
                path => path.to_string(),
            }
        ),
    );

    let debounce = Duration::from_millis(200);
    let poll_interval = Duration::from_millis(250);
    let mut reload_deadline = None;
    let mut pending = PendingChanges::default();

    while !stop.load(Ordering::SeqCst) {
        let timeout = reload_deadline
            .map(|deadline: Instant| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(poll_interval)
            .min(poll_interval);
        match rx.recv_timeout(timeout) {
            Ok(Ok(event)) => {
                // A folder created at the top of the project is watched too.
                if matches!(event.kind, EventKind::Create(_)) {
                    for path in &event.paths {
                        if path.is_dir() && scope.is_watched_folder(path) {
                            watch_folder(&mut watcher, path);
                        }
                    }
                }
                if pending.record(&event, &scope) {
                    reload_deadline = Some(Instant::now() + debounce);
                }
            }
            Ok(Err(e)) => {
                console::warn("watch", e);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if reload_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    reload_deadline = None;
                    if let Some(change) = std::mem::take(&mut pending).settle(&scope) {
                        console::info(
                            "reload",
                            format!(
                                "{} changed",
                                match change {
                                    ProjectChange::Source => "Python source",
                                    ProjectChange::Assets => "project asset",
                                }
                            ),
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
        let section = temp.path().join("src/talk/sections/title.py");
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
    fn only_project_folders_are_watched_recursively() {
        let temp = tempfile::tempdir().unwrap();
        for folder in [
            "src/sections",
            "assets",
            ".venv/Lib/site-packages",
            ".git/objects",
            "venv",
            "target/release",
            "exports",
            "__pycache__",
        ] {
            std::fs::create_dir_all(temp.path().join(folder)).unwrap();
        }
        std::fs::write(temp.path().join("main.py"), "").unwrap();
        let scope = WatchScope {
            script_path: temp.path().join("main.py"),
            root: temp.path().to_path_buf(),
        };
        assert_eq!(
            scope.watched_folders(),
            [temp.path().join("assets"), temp.path().join("src")]
        );
        // Nested folders are covered by their top-level folder's watch.
        assert!(!scope.is_watched_folder(&temp.path().join("src/sections")));
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
            "src/talk/notes.typ",
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
            "src/main.py.tmp.23124.4c0c6a4784a3",
            "assets/cover.png.tmp.8.1f",
            "sedWV991Q",
            "src/main.py___jb_tmp___",
            "src/main.py___jb_old___",
            "src/#main.py#",
        ] {
            assert_eq!(
                event_change(&[temp.path().join(ignored)], &scope),
                None,
                "{ignored}"
            );
        }
    }

    fn event(kind: EventKind, paths: &[&Path]) -> notify::Event {
        paths.iter().fold(notify::Event::new(kind), |event, path| {
            event.add_path(path.to_path_buf())
        })
    }

    fn settle(scope: &WatchScope, events: &[notify::Event]) -> Option<ProjectChange> {
        let mut pending = PendingChanges::default();
        for event in events {
            pending.record(event, scope);
        }
        pending.settle(scope)
    }

    /// Event sequences recorded on Windows for each way of saving
    /// `src/sections/title.py`; the files are left as each save leaves them.
    #[test]
    fn a_saved_source_reloads_as_source_however_it_was_written() {
        use notify::event::{CreateKind, RemoveKind};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let folder = root.join("src/sections");
        let section = folder.join("title.py");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(root.join("main.py"), "").unwrap();
        std::fs::write(&section, "").unwrap();
        let scope = WatchScope {
            script_path: root.join("main.py"),
            root: root.to_path_buf(),
        };
        let create = || EventKind::Create(CreateKind::Any);
        let modify = || EventKind::Modify(ModifyKind::Any);
        let remove = || EventKind::Remove(RemoveKind::Any);
        let renamed = |mode| EventKind::Modify(ModifyKind::Name(mode));

        // A temporary renamed over the file, as Node tools and editors do.
        let temporary = folder.join("title.py.tmp.24760.4c0c6a4784a3");
        let atomic = [
            event(create(), &[&temporary]),
            event(modify(), &[&temporary]),
            event(modify(), &[&folder]),
            event(remove(), &[&section]),
            event(renamed(RenameMode::From), &[&temporary]),
            event(renamed(RenameMode::To), &[&section]),
            event(modify(), &[&folder]),
        ];
        assert_eq!(settle(&scope, &atomic), Some(ProjectChange::Source));
        // The same, reported as one rename event.
        let atomic = [
            event(create(), &[&temporary]),
            event(renamed(RenameMode::Both), &[&temporary, &section]),
        ];
        assert_eq!(settle(&scope, &atomic), Some(ProjectChange::Source));

        // `sed -i` writes its temporary in the working folder.
        let sed = root.join("sedWV991Q");
        let sed_save = [
            event(create(), &[&sed]),
            event(modify(), &[&sed]),
            event(remove(), &[&section]),
            event(remove(), &[&sed]),
            event(create(), &[&section]),
            event(modify(), &[&folder]),
        ];
        assert_eq!(settle(&scope, &sed_save), Some(ProjectChange::Source));

        // `git checkout -- file` removes and creates the file.
        let git = root.join(".git");
        let checkout = [
            event(create(), &[&git.join("index.lock")]),
            event(remove(), &[&section]),
            event(modify(), &[&git]),
            event(modify(), &[&folder]),
            event(create(), &[&section]),
            event(modify(), &[&section]),
            event(modify(), &[&folder]),
        ];
        assert_eq!(settle(&scope, &checkout), Some(ProjectChange::Source));

        // A temporary still on disk when the window ends.
        std::fs::write(&temporary, "").unwrap();
        assert_eq!(settle(&scope, &[event(create(), &[&temporary])]), None);
    }

    #[test]
    fn assets_written_or_removed_still_reload_as_assets() {
        use notify::event::{CreateKind, RemoveKind};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let assets = root.join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        let scope = WatchScope {
            script_path: root.join("main.py"),
            root: root.to_path_buf(),
        };
        let cover = assets.join("cover.png");
        std::fs::write(&cover, "").unwrap();
        let added = [
            event(EventKind::Create(CreateKind::Any), &[&cover]),
            event(EventKind::Modify(ModifyKind::Any), &[&assets]),
        ];
        assert_eq!(settle(&scope, &added), Some(ProjectChange::Assets));

        // An asset deleted, or renamed away, is a change too.
        let removed = assets.join("logo.svg");
        assert_eq!(
            settle(
                &scope,
                &[event(EventKind::Remove(RemoveKind::Any), &[&removed])]
            ),
            Some(ProjectChange::Assets)
        );
        assert_eq!(
            settle(
                &scope,
                &[event(
                    EventKind::Modify(ModifyKind::Name(RenameMode::From)),
                    &[&removed]
                )]
            ),
            Some(ProjectChange::Assets)
        );
        // A folder alone reports only that its entries changed.
        assert_eq!(
            settle(
                &scope,
                &[event(EventKind::Modify(ModifyKind::Any), &[&assets])]
            ),
            None
        );
    }
}
