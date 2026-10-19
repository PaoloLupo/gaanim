//! Home: the workspace people see when they run `gaanim` without a script.
//!
//! Nothing asks for typed paths or environment knowledge. New projects go in a
//! folder chosen with the system picker, Python is prepared in the background
//! with uv, recent projects are one click away, and anything missing from the
//! computer (uv, Python, FFmpeg) is explained in plain words with the exact
//! command to install it. The look follows the playback bar: dark panels,
//! painted icons, seek tracks with a playhead, stops and keyframes, all with
//! square corners like the pixel mark.

use crate::app_icon::{ICON_GRID, ICON_PALETTE, ICON_PIXELS, ICON_TILE_RADIUS};
use crate::ui_kit::{
    self, ButtonTone, Icon, caption, icon_button, palette, primary_button, secondary_button,
};
use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use bevy_egui::{EguiPrimaryContextPass, egui};
use crossbeam_channel::{Receiver, TryRecvError};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use gaanim_project::{
    CreateProjectOptions, CreateWorkspaceOptions, EnvironmentProbe, PYTHON_VERSION, ProjectFolder,
    ProjectKind, PythonSource, RecentBundle, RecentEntry, RecentGroup, RecentProjects,
    ResolvedProject, Workspace, WorkspaceMember, create_project, create_workspace,
    default_project_parent, is_bundle_path, locate_script, resolve_folder, resolve_project,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DOCS_URL: &str = "https://paololupo.github.io/gaanim/";
const SIDEBAR_W: f32 = 236.0;
const CONTENT_MAX_W: f32 = 980.0;
/// Gap between chapter pieces of a track, as in the playback bar.
const PIECE_GAP: f32 = 4.0;

pub struct ProjectHubPlugin;

impl Plugin for ProjectHubPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProjectHubState>()
            .init_resource::<PendingProjectOpen>()
            .add_systems(Update, (hub_file_drop_system, open_bundle_request_system))
            .add_systems(EguiPrimaryContextPass, project_hub_ui_system);
    }
}

#[derive(Resource, Default)]
pub struct PendingProjectOpen(pub Option<ResolvedProject>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Projects,
    Environment,
    Learn,
}

/// What the computer offers: Python and uv as the launcher sees them, and
/// whether FFmpeg is on `PATH` (the exporter runs `ffmpeg` by name).
#[derive(Debug, Clone, Default)]
struct Tools {
    probe: EnvironmentProbe,
    ffmpeg: bool,
}

impl Tools {
    fn readiness(&self) -> Readiness {
        if self.probe.uv.is_some() {
            Readiness::Ready
        } else if self.probe.has_supported_python() {
            Readiness::WithoutUv
        } else {
            Readiness::NeedsUv
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Readiness {
    /// uv prepares Python and each project's environment.
    Ready,
    /// Projects open with the system Python, without isolation or autocomplete.
    WithoutUv,
    /// Nothing can run a project yet.
    NeedsUv,
}

/// A project whose environment is being prepared on a worker thread.
struct Preparation {
    project: ResolvedProject,
    created: bool,
    started: Option<f64>,
    result: Receiver<Result<(), String>>,
}

/// The Home's offer of a newer Gaanim: the daily check and the install run on
/// worker threads (see [`crate::update`]).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct UpdateOffer {
    checking: Option<Receiver<Result<Option<crate::update::Release>, String>>>,
    checked: bool,
    /// The last check was asked for in Settings, so its answer is shown
    /// even when there is nothing new or it failed.
    manual: bool,
    /// Why the last check asked for in Settings failed.
    check_error: Option<String>,
    /// A newer release with a package for this system.
    available: Option<crate::update::Release>,
    installing: Option<Receiver<InstallEvent>>,
    step: Option<crate::update::Step>,
    /// The new `gaanim`, once installed.
    installed: Option<PathBuf>,
    error: Option<String>,
    /// Closed for this session.
    dismissed: bool,
    /// Start this executable and close; set by *Restart*.
    restart: Option<PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
enum InstallEvent {
    Step(crate::update::Step),
    Done(Result<PathBuf, String>),
}

#[cfg(not(target_arch = "wasm32"))]
impl UpdateOffer {
    /// Ask GitHub once per session; [`crate::update::check`] reuses a check
    /// made less than a day ago. A failed check stays silent.
    fn check(&mut self) {
        if self.checked || crate::update::check_disabled() {
            return;
        }
        self.checked = true;
        let (tx, rx) = crossbeam_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("gaanim-update-check".into())
            .spawn(move || {
                let _ = tx.send(crate::update::check(false));
            });
        if spawned.is_ok() {
            self.checking = Some(rx);
        }
    }

    /// Ask GitHub now, whatever the last check said and even when the
    /// daily check is off; Settings shows the answer.
    fn check_now(&mut self) {
        if self.working() {
            return;
        }
        self.checked = true;
        self.manual = true;
        self.dismissed = false;
        self.check_error = None;
        let (tx, rx) = crossbeam_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("gaanim-update-check".into())
            .spawn(move || {
                let _ = tx.send(crate::update::check(true));
            });
        match spawned {
            Ok(_) => self.checking = Some(rx),
            Err(error) => self.check_error = Some(error.to_string()),
        }
    }

    fn install(&mut self) {
        let Some(release) = self.available.clone() else {
            return;
        };
        if self.installing.is_some() {
            return;
        }
        self.error = None;
        let (tx, rx) = crossbeam_channel::unbounded();
        let spawned = std::thread::Builder::new()
            .name("gaanim-update-install".into())
            .spawn(move || {
                let result = crate::update::install(&release, |step| {
                    let _ = tx.send(InstallEvent::Step(step));
                });
                let _ = tx.send(InstallEvent::Done(result));
            });
        match spawned {
            Ok(_) => self.installing = Some(rx),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn poll(&mut self) {
        if let Some(rx) = &self.checking {
            match rx.try_recv() {
                // A failed daily check stays silent; one asked for says why.
                Ok(Ok(release)) => {
                    self.available = release;
                    self.checking = None;
                }
                Ok(Err(error)) => {
                    self.check_error = Some(error);
                    self.checking = None;
                }
                Err(TryRecvError::Disconnected) => self.checking = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let mut finished = None;
        if let Some(rx) = &self.installing {
            loop {
                match rx.try_recv() {
                    Ok(InstallEvent::Step(step)) => self.step = Some(step),
                    Ok(InstallEvent::Done(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        finished = Some(Err("the update ended without a response".into()));
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
        }
        if let Some(result) = finished {
            self.installing = None;
            self.step = None;
            match result {
                Ok(executable) => self.installed = Some(executable),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn working(&self) -> bool {
        self.checking.is_some() || self.installing.is_some()
    }

    /// What the offer of `release` says, in its colour and icon.
    fn status(&self, release: &crate::update::Release) -> (Color32, Icon, String) {
        if let Some(error) = &self.error {
            return (
                palette::DANGER,
                Icon::Warning,
                format!("Could not update to Gaanim {}: {error}", release.version),
            );
        }
        if self.installed.is_some() {
            return (
                palette::LOOP,
                Icon::Check,
                format!(
                    "Gaanim {} is installed. Restart to use it.",
                    release.version
                ),
            );
        }
        let doing = match self.step {
            Some(crate::update::Step::Downloading { done, total }) if total > 0 => format!(
                "Downloading Gaanim {}… {}%",
                release.version,
                done * 100 / total
            ),
            Some(crate::update::Step::Downloading { .. }) => {
                format!("Downloading Gaanim {}…", release.version)
            }
            Some(crate::update::Step::Verifying) => "Verifying the download…".to_owned(),
            Some(crate::update::Step::Unpacking) => "Unpacking…".to_owned(),
            Some(crate::update::Step::Installing) => "Installing…".to_owned(),
            None => format!(
                "Gaanim {} is available; you have {}.",
                release.version,
                env!("CARGO_PKG_VERSION")
            ),
        };
        (palette::ACCENT, Icon::Package, doing)
    }
}

/// Settings → Updates: check now, without waiting for the daily check, and
/// update and restart as Home's banner does.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn update_settings_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    let offer = &mut state.update;
    offer.poll();
    let checking = offer.checking.is_some();
    let installing = offer.installing.is_some();
    ui.horizontal(|ui| {
        let label = if checking { "Checking…" } else { "Check now" };
        if secondary_button(ui, label, !offer.working()).clicked() {
            offer.check_now();
        }
        ui.label(
            egui::RichText::new(concat!("You have Gaanim ", env!("CARGO_PKG_VERSION")))
                .size(13.0)
                .color(palette::TEXT_MUTED),
        );
    });
    let (mut notes, mut install, mut restart) = (false, false, false);
    if let Some(release) = offer.available.clone() {
        let (color, icon, text) = offer.status(&release);
        ui.horizontal_wrapped(|ui| {
            let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
            ui_kit::paint_icon(ui.painter(), icon_rect, icon, color);
            ui.label(egui::RichText::new(text).size(13.0).color(palette::TEXT));
        });
        ui.horizontal(|ui| {
            if offer.installed.is_some() {
                restart = primary_button(ui, "Restart", None, true).clicked();
            } else if !installing {
                let label = if offer.error.is_some() {
                    "Retry"
                } else {
                    "Update"
                };
                install = primary_button(ui, label, None, true).clicked();
            }
            if !release.page.is_empty() {
                notes = secondary_button(ui, "What's new", true).clicked();
            }
        });
    } else if let Some(error) = &offer.check_error {
        ui.add(
            egui::Label::new(
                egui::RichText::new(format!("Could not check: {error}"))
                    .size(13.0)
                    .color(palette::DANGER),
            )
            .wrap(),
        );
    } else if offer.manual && !checking {
        ui.label(
            egui::RichText::new("This is the latest version.")
                .size(13.0)
                .color(palette::LOOP),
        );
    }
    if offer.working() {
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    if notes {
        let _ = crate::platform::open_detached(release_page(offer));
    }
    if install {
        offer.install();
    }
    if restart {
        offer.restart = offer.installed.clone();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn release_page(offer: &UpdateOffer) -> String {
    offer
        .available
        .as_ref()
        .map(|release| release.page.clone())
        .unwrap_or_default()
}

enum Notice {
    Error(String),
    Info(String),
}

#[derive(Resource)]
pub struct ProjectHubState {
    pub active: bool,
    section: Section,
    recents: RecentProjects,
    tools: Option<Tools>,
    checking: Option<Receiver<Tools>>,
    preparing: Option<Preparation>,
    /// Shown as "opening" for a frame before the blocking Python start-up.
    opening: Option<ResolvedProject>,
    opening_shown: bool,
    /// A project to open as soon as the environment allows it.
    waiting: Option<ResolvedProject>,
    notice: Option<Notice>,
    drop_hover: bool,
    dropped: Option<PathBuf>,
    was_focused: bool,
    /// A playback bundle to open on the next frame; see
    /// [`open_bundle_request_system`].
    bundle_request: Option<PathBuf>,
    /// Whether `.gaanim` files open with Gaanim; read when the environment
    /// page first shows it.
    associated: Option<bool>,
    #[cfg(not(target_arch = "wasm32"))]
    update: UpdateOffer,
    /// Go back to Home from a project: start Gaanim again without one.
    #[cfg(not(target_arch = "wasm32"))]
    home_requested: bool,
    /// A recent project or bundle to switch to: Gaanim starts again with it.
    #[cfg(not(target_arch = "wasm32"))]
    switch_requested: Option<PathBuf>,
    /// Leaving was asked for while recording: where to go once confirmed
    /// (`None` is Home).
    #[cfg(not(target_arch = "wasm32"))]
    leave_pending: Option<Option<PathBuf>>,
    /// Recent entries as rows or as cards with their covers.
    recent_view: RecentView,
    thumbnails: Thumbnails,
    /// Settings is open over Home, which then ignores the pointer.
    covered: bool,
    /// What the recent list is filtered by: part of a name or a path.
    recent_query: String,
    /// The workspace whose members the Projects page shows instead.
    workspace_page: Option<WorkspacePage>,
}

/// A workspace opened in Home: its members, grouped as its manifest says.
struct WorkspacePage {
    workspace: Workspace,
    members: Vec<WorkspaceMember>,
    /// What the member list is filtered by: part of a name, path or group.
    query: String,
    /// The last *Check all*, while it runs and after.
    #[cfg(not(target_arch = "wasm32"))]
    check: Option<WorkspaceCheck>,
}

impl WorkspacePage {
    fn new(workspace: Workspace) -> Self {
        Self {
            members: workspace.members(),
            workspace,
            query: String::new(),
            #[cfg(not(target_arch = "wasm32"))]
            check: None,
        }
    }

    /// List the members again: one may have been added or removed.
    fn refresh(&mut self) {
        self.members = self.workspace.members();
    }
}

/// *Check all*: `gaanim check` run on each member in turn, in its own
/// process, so Home loads no Python.
#[cfg(not(target_arch = "wasm32"))]
struct WorkspaceCheck {
    results: HashMap<PathBuf, CheckOutcome>,
    receiver: Receiver<(PathBuf, CheckOutcome)>,
    running: bool,
}

/// How a member's `gaanim check` ended.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum CheckOutcome {
    Passed { warnings: usize },
    Failed { errors: usize, warnings: usize },
    NotLoaded(String),
}

#[cfg(not(target_arch = "wasm32"))]
impl CheckOutcome {
    /// What a member's card says about it, in what color.
    fn label(&self) -> (String, Color32) {
        let plural = |count: usize, word: &str| {
            format!("{count} {word}{}", if count == 1 { "" } else { "s" })
        };
        match self {
            Self::Passed { warnings: 0 } => ("Passed".into(), palette::LOOP),
            Self::Passed { warnings } => (
                format!("Passed · {}", plural(*warnings, "warning")),
                palette::STOP,
            ),
            Self::Failed { errors, .. } => (
                format!("Failed · {}", plural(*errors, "error")),
                palette::DANGER,
            ),
            Self::NotLoaded(_) => ("Could not load".into(), palette::DANGER),
        }
    }

    /// The outcome of a `gaanim check --message-format json` run that
    /// exited with `code` and wrote `stdout`.
    fn read(code: Option<i32>, stdout: &str) -> Self {
        let lines: Vec<serde_json::Value> = stdout
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let count = |line: &serde_json::Value, key: &str| {
            line.get(key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize
        };
        let verdict = lines
            .iter()
            .find(|line| line.get("type").and_then(|kind| kind.as_str()) == Some("check"));
        match (code, verdict) {
            (Some(0), Some(line)) => Self::Passed {
                warnings: count(line, "warnings"),
            },
            (Some(1), Some(line)) => Self::Failed {
                errors: count(line, "errors"),
                warnings: count(line, "warnings"),
            },
            _ => Self::NotLoaded(
                lines
                    .iter()
                    .filter(|line| {
                        line.get("level").and_then(|level| level.as_str()) == Some("error")
                    })
                    .find_map(|line| line.get("message").and_then(|message| message.as_str()))
                    .unwrap_or("gaanim check did not finish")
                    .to_owned(),
            ),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WorkspaceCheck {
    /// Start checking `members` on a worker thread.
    fn start(members: &[WorkspaceMember]) -> Option<Self> {
        let executable = std::env::current_exe().ok()?;
        let roots: Vec<PathBuf> = members
            .iter()
            .filter(|member| member.project.is_ok())
            .map(|member| member.root.clone())
            .collect();
        let (sender, receiver) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("gaanim-hub-check".into())
            .spawn(move || {
                for root in roots {
                    let output = std::process::Command::new(&executable)
                        .arg("check")
                        .arg(gaanim_core::console::plain_path(&root))
                        .args(["--message-format", "json"])
                        .stdin(std::process::Stdio::null())
                        .output();
                    let outcome = match output {
                        Ok(output) => CheckOutcome::read(
                            output.status.code(),
                            &String::from_utf8_lossy(&output.stdout),
                        ),
                        Err(error) => CheckOutcome::NotLoaded(error.to_string()),
                    };
                    if sender.send((root, outcome)).is_err() {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Self {
            results: HashMap::new(),
            receiver,
            running: true,
        })
    }

    fn poll(&mut self) {
        loop {
            match self.receiver.try_recv() {
                Ok((root, outcome)) => {
                    self.results.insert(root, outcome);
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.running = false;
                    return;
                }
            }
        }
    }
}

/// How Home shows its recent entries; saved in the `home` section of the
/// user's settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum RecentView {
    #[default]
    List,
    Grid,
}

/// The `home` section of the user's settings.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct HomePreferences {
    #[serde(default)]
    recent_view: RecentView,
}

impl HomePreferences {
    const SECTION: &str = "home";

    fn load() -> Self {
        gaanim_project::settings::load(Self::SECTION)
    }

    fn save(&self) -> Result<(), String> {
        gaanim_project::settings::save(Self::SECTION, self)
    }
}

/// Covers of recent entries: a project's from the editor's cache, a
/// bundle's from its `thumbnail.png`. Each is decoded on a worker thread
/// and kept as a texture for the session, or as `None` when there is none.
#[derive(Default)]
struct Thumbnails {
    textures: HashMap<PathBuf, Option<egui::TextureHandle>>,
    loading: HashMap<PathBuf, Receiver<Option<egui::ColorImage>>>,
}

/// Longest side covers are decoded at, in pixels.
const COVER_EDGE: u32 = 480;

impl Thumbnails {
    /// The cover of `entry`, once loaded; the first call starts loading it.
    fn get(&mut self, ctx: &egui::Context, group: &RecentGroup) -> Option<egui::TextureHandle> {
        let entry = &group.entry;
        let key = entry.path().to_path_buf();
        if let Some(texture) = self.textures.get(&key) {
            return texture.clone();
        }
        if let Some(receiver) = self.loading.get(&key) {
            let image = match receiver.try_recv() {
                Ok(image) => image,
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100));
                    return None;
                }
                Err(TryRecvError::Disconnected) => None,
            };
            self.loading.remove(&key);
            let texture = image.map(|image| {
                ctx.load_texture(
                    format!("recent-cover:{}", key.display()),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
            self.textures.insert(key, texture.clone());
            return texture;
        }
        let cover = match entry {
            // A project the editor has not covered yet shows its bundle's.
            RecentEntry::Project(project) => Some(Cover::Project(
                gaanim_project::project_thumbnail_path(&project.root),
                group.bundle.as_ref().map(|bundle| bundle.path.clone()),
            )),
            RecentEntry::Bundle(bundle) => Some(Cover::Bundle(bundle.path.clone())),
            RecentEntry::Workspace(workspace) => Some(Cover::Workspace(workspace.clone())),
        };
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("gaanim-recent-cover".into())
            .spawn(move || {
                let _ = sender.send(cover.and_then(Cover::decode));
            });
        match spawned {
            Ok(_) => {
                self.loading.insert(key, receiver);
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(_) => {
                self.textures.insert(key, None);
            }
        }
        None
    }
}

/// Where a cover comes from.
enum Cover {
    /// A project's: the editor's cache, else its newest bundle's cover.
    Project(Option<PathBuf>, Option<PathBuf>),
    Bundle(PathBuf),
    /// A workspace's: its image, else its members' covers in a mosaic.
    Workspace(Workspace),
}

impl Cover {
    fn decode(self) -> Option<egui::ColorImage> {
        let bundle_cover = |path: &Path| gaanim_thumbnail::read_file(path).ok().flatten();
        let png = match self {
            Self::Project(file, bundle) => file
                .and_then(|file| std::fs::read(file).ok())
                .or_else(|| bundle.as_deref().and_then(bundle_cover))?,
            Self::Bundle(path) => bundle_cover(&path)?,
            Self::Workspace(workspace) => return workspace_cover(&workspace).map(color_image),
        };
        gaanim_thumbnail::scaled(&png, COVER_EDGE)
            .ok()
            .map(color_image)
    }
}

fn color_image(image: image::RgbaImage) -> egui::ColorImage {
    let (width, height) = image.dimensions();
    egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], image.as_raw())
}

/// A workspace's cover: its image (`cover`, or a `cover.png` at its root),
/// else the covers of up to four members, the most recently edited first,
/// in a mosaic.
fn workspace_cover(workspace: &Workspace) -> Option<image::RgbaImage> {
    if let Some(file) = &workspace.cover {
        let image = image::load_from_memory(&std::fs::read(file).ok()?)
            .ok()?
            .into_rgba8();
        let (width, height) = image.dimensions();
        if width.max(height) <= COVER_EDGE {
            return Some(image);
        }
        let (width, height) = gaanim_thumbnail::fit(width, height, COVER_EDGE);
        return Some(image::imageops::resize(
            &image,
            width,
            height,
            image::imageops::FilterType::Triangle,
        ));
    }
    let mut members: Vec<(SystemTime, PathBuf)> = workspace
        .members()
        .into_iter()
        .filter_map(|member| {
            let project = member.project.ok()?;
            let edited = std::fs::metadata(&project.entry).ok()?.modified().ok()?;
            Some((edited, project.root))
        })
        .collect();
    members.sort_by_key(|(edited, _)| std::cmp::Reverse(*edited));
    let tiles: Vec<image::RgbaImage> = members
        .iter()
        .filter_map(|(_, root)| {
            let png = std::fs::read(gaanim_project::project_thumbnail_path(root)?).ok()?;
            gaanim_thumbnail::scaled(&png, COVER_EDGE).ok()
        })
        .take(4)
        .collect();
    mosaic(&tiles)
}

/// `tiles` in a 16:9 image [`COVER_EDGE`] wide: one fills it, more share a
/// two-by-two grid in reading order, each contained in its cell.
fn mosaic(tiles: &[image::RgbaImage]) -> Option<image::RgbaImage> {
    const GAP: u32 = 4;
    let (width, height) = (COVER_EDGE, COVER_EDGE * 9 / 16);
    let columns = match tiles.len() {
        0 => return None,
        1 => 1,
        _ => 2,
    };
    let cell = (
        (width - GAP * (columns - 1)) / columns,
        (height - GAP * (columns - 1)) / columns,
    );
    let mut canvas = image::RgbaImage::new(width, height);
    for (index, tile) in tiles.iter().enumerate() {
        let (column, row) = (index as u32 % columns, index as u32 / columns);
        let (tile_width, tile_height) = tile.dimensions();
        let scale = (f64::from(cell.0) / f64::from(tile_width.max(1)))
            .min(f64::from(cell.1) / f64::from(tile_height.max(1)));
        let size = |edge: u32| ((f64::from(edge) * scale).round() as u32).max(1);
        let (fitted_width, fitted_height) =
            (size(tile_width).min(cell.0), size(tile_height).min(cell.1));
        let fitted = image::imageops::resize(
            tile,
            fitted_width,
            fitted_height,
            image::imageops::FilterType::Triangle,
        );
        let x = column * (cell.0 + GAP) + (cell.0 - fitted_width) / 2;
        let y = row * (cell.1 + GAP) + (cell.1 - fitted_height) / 2;
        image::imageops::overlay(&mut canvas, &fitted, i64::from(x), i64::from(y));
    }
    Some(canvas)
}

impl Default for ProjectHubState {
    fn default() -> Self {
        Self {
            active: false,
            section: Section::Projects,
            recents: RecentProjects::load(),
            tools: None,
            checking: None,
            preparing: None,
            opening: None,
            opening_shown: false,
            waiting: None,
            notice: None,
            drop_hover: false,
            dropped: None,
            was_focused: true,
            bundle_request: None,
            associated: None,
            #[cfg(not(target_arch = "wasm32"))]
            update: UpdateOffer::default(),
            #[cfg(not(target_arch = "wasm32"))]
            home_requested: false,
            #[cfg(not(target_arch = "wasm32"))]
            switch_requested: None,
            #[cfg(not(target_arch = "wasm32"))]
            leave_pending: None,
            recent_view: HomePreferences::load().recent_view,
            thumbnails: Thumbnails::default(),
            covered: false,
            recent_query: String::new(),
            workspace_page: None,
        }
    }
}

impl ProjectHubState {
    pub fn show(&mut self) {
        self.active = true;
        self.check_tools();
        #[cfg(not(target_arch = "wasm32"))]
        self.update.check();
    }

    /// Leave the open project or bundle for Home. A project loads Python and
    /// its scene into this process, so Gaanim starts again without one, as
    /// after an update, rather than unloading it in place.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn request_home(&mut self) {
        self.home_requested = true;
    }

    /// Leave the open project for another one, or a bundle, as
    /// [`Self::request_home`] leaves it for Home.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn request_switch(&mut self, path: PathBuf) {
        self.switch_requested = Some(path);
    }

    /// Show `workspace`'s page, as opening its folder from Home does.
    pub fn show_workspace(&mut self, workspace: Workspace) {
        self.show();
        self.open_workspace(workspace);
    }

    fn open_workspace(&mut self, workspace: Workspace) {
        self.notice = None;
        self.recents.record_workspace(&workspace);
        let _ = self.recents.save();
        self.section = Section::Projects;
        self.workspace_page = Some(WorkspacePage::new(workspace));
    }

    pub fn report_open_error(&mut self, error: String) {
        self.active = true;
        self.opening = None;
        self.preparing = None;
        self.notice = Some(Notice::Error(format!(
            "Could not open the project: {error}"
        )));
    }

    /// Re-detect the tools on a worker thread; process spawns take a moment.
    fn check_tools(&mut self) {
        if self.checking.is_some() {
            return;
        }
        adopt_installed_tools();
        let (tx, rx) = crossbeam_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("gaanim-hub-probe".into())
            .spawn(move || {
                let _ = tx.send(Tools {
                    probe: EnvironmentProbe::detect(None),
                    ffmpeg: has_ffmpeg(),
                });
            });
        if spawned.is_ok() {
            self.checking = Some(rx);
        }
    }

    fn poll_workers(&mut self, pending: &mut PendingProjectOpen) {
        #[cfg(not(target_arch = "wasm32"))]
        self.update.poll();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(check) = self
            .workspace_page
            .as_mut()
            .and_then(|page| page.check.as_mut())
        {
            check.poll();
        }
        if let Some(rx) = &self.checking {
            match rx.try_recv() {
                Ok(tools) => {
                    self.tools = Some(tools);
                    self.checking = None;
                    self.retry_waiting();
                }
                Err(TryRecvError::Disconnected) => self.checking = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let finished =
            self.preparing
                .as_ref()
                .and_then(|preparation| match preparation.result.try_recv() {
                    Ok(result) => Some(result),
                    Err(TryRecvError::Disconnected) => {
                        Some(Err("the preparation ended without a response".into()))
                    }
                    Err(TryRecvError::Empty) => None,
                });
        if let Some(result) = finished
            && let Some(preparation) = self.preparing.take()
        {
            match result {
                Ok(()) => self.opening = Some(preparation.project),
                Err(error) => {
                    self.waiting = Some(preparation.project);
                    self.section = Section::Environment;
                    self.notice = Some(Notice::Error(format!(
                        "Could not prepare the Python environment: {error}"
                    )));
                }
            }
        }
        // Keep the "opening" card on screen for a frame before Python starts.
        if let Some(project) = self.opening.as_ref()
            && pending.0.is_none()
            && self.opening_shown
        {
            pending.0 = Some(project.clone());
        }
    }

    fn retry_waiting(&mut self) {
        let ready = self
            .tools
            .as_ref()
            .is_some_and(|tools| tools.readiness() != Readiness::NeedsUv);
        if ready && let Some(project) = self.waiting.take() {
            self.notice = None;
            self.open_project(project, false);
        }
    }

    /// Open a project, preparing its environment first when it needs one.
    fn open_project(&mut self, project: ResolvedProject, created: bool) {
        self.notice = None;
        self.recents.record(&project);
        let _ = self.recents.save();
        let probe = EnvironmentProbe::detect(Some(&project.root));
        if probe.has_supported_python() && probe.has_venv() {
            self.opening = Some(project);
        } else if probe.uv.is_some() {
            self.start_preparation(project, created);
        } else if probe.has_supported_python() {
            // No uv: the preview works with the system Python, without isolation.
            self.opening = Some(project);
        } else {
            self.notice = Some(Notice::Info(format!(
                "Gaanim needs uv to open “{}”. Install it with the command below; the \
                 project opens on its own as soon as Gaanim detects it.",
                project.manifest.name
            )));
            self.waiting = Some(project);
            self.section = Section::Environment;
        }
    }

    fn start_preparation(&mut self, project: ResolvedProject, created: bool) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let root = project.environment_root().to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("gaanim-hub-prepare".into())
            .spawn(move || {
                let result = gaanim_project::provision_authoring_package(&root).map(|_| ());
                let _ = tx.send(result);
            });
        match spawned {
            Ok(_) => {
                self.preparing = Some(Preparation {
                    project,
                    created,
                    started: None,
                    result: rx,
                })
            }
            Err(error) => {
                self.notice = Some(Notice::Error(format!(
                    "Could not start the preparation: {error}"
                )))
            }
        }
    }

    fn busy(&self) -> bool {
        self.preparing.is_some() || self.opening.is_some() || self.bundle_request.is_some()
    }

    /// Whether a worker or a request will change the hub without input: a
    /// bundle about to open or, while the hub shows (only then are its
    /// workers polled), the tool probe, an environment being prepared, a
    /// project about to open or a dropped file not yet taken.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn working(&self) -> bool {
        let checking_members = self
            .workspace_page
            .as_ref()
            .and_then(|page| page.check.as_ref())
            .is_some_and(|check| check.running);
        self.bundle_request.is_some()
            || (self.active
                && (self.checking.is_some()
                    || self.busy()
                    || self.dropped.is_some()
                    || checking_members
                    || self.update.working()))
    }

    /// Ask for a playback bundle with the native picker and open it.
    fn open_bundle_with_picker(&mut self) {
        let directory = self
            .recents
            .entries()
            .into_iter()
            .find_map(|entry| match entry {
                RecentEntry::Bundle(bundle) => bundle.path.parent().map(Path::to_path_buf),
                RecentEntry::Project(_) | RecentEntry::Workspace(_) => None,
            })
            .unwrap_or_else(default_project_parent);
        if let Some(path) = crate::platform::pick_file(
            "Play a .gaanim file",
            Some(&directory),
            &[("Gaanim (.gaanim)", &[gaanim_bundle::EXTENSION])],
        ) {
            self.open_bundle(&path);
        }
    }

    /// Play a bundle in this window. It needs neither Python nor its project.
    fn open_bundle(&mut self, path: &Path) {
        if !path.is_file() {
            self.notice = Some(Notice::Error(format!(
                "“{}” was not found.",
                display_path(path)
            )));
            return;
        }
        self.notice = None;
        self.bundle_request = Some(path.to_path_buf());
    }

    /// Ask for a folder with the native picker and create the project there.
    fn create_with_picker(&mut self, kind: ProjectKind) {
        self.create_with_picker_in(kind, &default_project_parent());
    }

    /// [`Self::create_with_picker`], with the picker starting in `start`.
    fn create_with_picker_in(&mut self, kind: ProjectKind, start: &Path) {
        let title = match kind {
            ProjectKind::Video => "Choose or create a folder for the new video",
            ProjectKind::Slides => "Choose or create a folder for the new presentation",
        };
        let Some(picked) = crate::platform::pick_folder(title, Some(start)) else {
            return;
        };
        match new_project_target(&picked, kind) {
            NewProjectTarget::Existing(root) => match resolve_project(&root) {
                Ok(project) => self.open_project(project, false),
                Err(error) => self.notice = Some(Notice::Error(error)),
            },
            NewProjectTarget::Create(directory) => match create_project(&CreateProjectOptions {
                kind,
                directory,
                force: false,
            }) {
                Ok(project) => self.open_project(project, true),
                Err(error) => {
                    self.notice = Some(Notice::Error(format!(
                        "Could not create the project: {error}"
                    )))
                }
            },
        }
    }

    /// Ask for a folder with the native picker and create a workspace there,
    /// in a Git repository of its own unless it is inside one, then show it.
    fn create_workspace_with_picker(&mut self) {
        let Some(picked) = crate::platform::pick_folder(
            "Choose or create a folder for the new workspace",
            Some(&default_project_parent()),
        ) else {
            return;
        };
        let directory = match new_folder_target(&picked, "new-workspace") {
            NewProjectTarget::Existing(root) => {
                self.open_path(&root);
                return;
            }
            NewProjectTarget::Create(directory) => directory,
        };
        match create_workspace(&CreateWorkspaceOptions {
            directory,
            force: false,
        }) {
            Ok(workspace) => {
                gaanim_project::init_git_repository(&workspace.root);
                self.open_workspace(workspace);
            }
            Err(error) => {
                self.notice = Some(Notice::Error(format!(
                    "Could not create the workspace: {error}"
                )))
            }
        }
    }

    fn open_with_picker(&mut self) {
        if let Some(path) =
            crate::platform::pick_folder("Open a Gaanim project", Some(&default_project_parent()))
        {
            self.open_path(&path);
        }
    }

    /// Open a dropped or picked path: a playback bundle, a project or
    /// workspace folder, or any file inside one.
    fn open_path(&mut self, path: &Path) {
        if is_bundle_path(path) {
            self.open_bundle(path);
            return;
        }
        let opened = if path.is_dir() {
            if path.join("gaanim.toml").is_file() {
                resolve_folder(path).map(Some)
            } else {
                Ok(None)
            }
        } else {
            locate_script(path).map(|location| match location {
                gaanim_project::Location {
                    project: Some(project),
                    ..
                } => Some(ProjectFolder::Project(project)),
                gaanim_project::Location {
                    workspace: Some(workspace),
                    ..
                } => Some(ProjectFolder::Workspace(workspace)),
                _ => None,
            })
        };
        match opened {
            Ok(Some(ProjectFolder::Project(project))) => self.open_project(project, false),
            Ok(Some(ProjectFolder::Workspace(workspace))) => self.open_workspace(workspace),
            Err(error) => {
                self.notice = Some(Notice::Error(format!("Could not open it: {error}")));
            }
            Ok(None) => {
                self.notice = Some(Notice::Error(format!(
                    "“{}” is neither a Gaanim project (it has no gaanim.toml) nor a \
                     .gaanim file. Use “New video” or “New presentation” to create one.",
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string())
                )))
            }
        }
    }
}

/// Where a new project goes for the folder picked in the dialog.
#[derive(Debug, PartialEq, Eq)]
enum NewProjectTarget {
    /// The folder is already a project: open it instead.
    Existing(PathBuf),
    Create(PathBuf),
}

/// An empty folder becomes the project; any other folder gets a new subfolder
/// with the first free default name, so picking "Documents" just works.
fn new_project_target(picked: &Path, kind: ProjectKind) -> NewProjectTarget {
    new_folder_target(
        picked,
        match kind {
            ProjectKind::Video => "new-video",
            ProjectKind::Slides => "new-presentation",
        },
    )
}

/// [`new_project_target`] for any new folder: one named `base`, or
/// `base-2` and so on, when the picked folder is not empty.
fn new_folder_target(picked: &Path, base: &str) -> NewProjectTarget {
    if picked.join("gaanim.toml").is_file() {
        return NewProjectTarget::Existing(picked.to_path_buf());
    }
    let empty = std::fs::read_dir(picked).is_ok_and(|mut entries| entries.next().is_none());
    if empty {
        return NewProjectTarget::Create(picked.to_path_buf());
    }
    let candidate = (1..)
        .map(|n| {
            if n == 1 {
                picked.join(base)
            } else {
                picked.join(format!("{base}-{n}"))
            }
        })
        .find(|candidate| !candidate.exists())
        .expect("an unused folder name exists");
    NewProjectTarget::Create(candidate)
}

fn kind_label(kind: ProjectKind) -> &'static str {
    match kind {
        ProjectKind::Video => "Video",
        ProjectKind::Slides => "Presentation",
    }
}

fn kind_color(kind: ProjectKind) -> Color32 {
    match kind {
        ProjectKind::Video => palette::ACCENT,
        ProjectKind::Slides => palette::STOP,
    }
}

/// "3 h ago": how long ago a project was last edited.
fn relative_age(elapsed: Duration) -> String {
    let minutes = elapsed.as_secs() / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    match () {
        _ if minutes < 1 => "just now".into(),
        _ if hours < 1 => format!("{minutes} min ago"),
        _ if days < 1 => format!("{hours} h ago"),
        _ if days == 1 => "yesterday".into(),
        _ if days < 7 => format!("{days} days ago"),
        _ if days < 30 => format!("{} wk ago", days / 7),
        _ if days < 60 => "1 month ago".into(),
        _ if days < 365 => format!("{} months ago", days / 30),
        _ => "over a year ago".into(),
    }
}

fn recorded_ago(bundle: &RecentBundle) -> Option<String> {
    let modified = std::fs::metadata(&bundle.path).ok()?.modified().ok()?;
    let elapsed = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    Some(format!("Recorded {}", relative_age(elapsed)))
}

/// "11 h ago": how long ago a bundle was recorded, for a project's line.
fn bundle_age(bundle: &RecentBundle) -> Option<String> {
    let modified = std::fs::metadata(&bundle.path).ok()?.modified().ok()?;
    let elapsed = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    Some(relative_age(elapsed))
}

fn edited_ago(project: &ResolvedProject) -> Option<String> {
    let modified = std::fs::metadata(&project.entry).ok()?.modified().ok()?;
    let elapsed = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    Some(format!("Edited {}", relative_age(elapsed)))
}

/// Paths as people write them, without Windows' `\\?\` verbatim prefix.
fn display_path(path: &Path) -> String {
    gaanim_core::console::plain_path(path).display().to_string()
}

// ---- Tools on the computer -------------------------------------------------------------

/// Install help for a tool Gaanim does not ship.
struct ToolHelp {
    terminal: &'static str,
    command: &'static str,
    more_url: &'static str,
}

fn uv_help() -> ToolHelp {
    ToolHelp {
        terminal: TERMINAL,
        command: if cfg!(windows) {
            "winget install --id=astral-sh.uv -e"
        } else if cfg!(target_os = "macos") {
            "brew install uv"
        } else {
            "curl -LsSf https://astral.sh/uv/install.sh | sh"
        },
        more_url: "https://docs.astral.sh/uv/getting-started/installation/",
    }
}

fn ffmpeg_help() -> ToolHelp {
    ToolHelp {
        terminal: TERMINAL,
        command: if cfg!(windows) {
            "winget install --id=Gyan.FFmpeg -e"
        } else if cfg!(target_os = "macos") {
            "brew install ffmpeg"
        } else {
            "sudo apt install ffmpeg"
        },
        more_url: "https://ffmpeg.org/download.html",
    }
}

const TERMINAL: &str = if cfg!(windows) {
    "PowerShell (search for it in the Start menu)"
} else {
    "a terminal"
};

fn has_ffmpeg() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Folders where the uv and FFmpeg installers leave their executables.
fn tool_install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
    }
    if cfg!(windows)
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        dirs.push(
            PathBuf::from(local)
                .join("Microsoft")
                .join("WinGet")
                .join("Links"),
        );
    }
    dirs
}

/// A process keeps the `PATH` it started with, so a tool installed while the
/// Home is open would stay invisible until a restart. Append the folders
/// where installers put uv and FFmpeg when they contain one of them.
fn adopt_installed_tools() {
    let executable = |name: &str| {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        }
    };
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut paths: Vec<PathBuf> = std::env::split_paths(&current).collect();
    let mut changed = false;
    for dir in tool_install_dirs() {
        let has_tool = ["uv", "ffmpeg"]
            .iter()
            .any(|tool| dir.join(executable(tool)).is_file());
        if has_tool && !paths.contains(&dir) {
            paths.push(dir);
            changed = true;
        }
    }
    if changed && let Ok(joined) = std::env::join_paths(paths) {
        // SAFETY: as in gaanim_project::activate_environment, PATH is updated
        // from the main thread before the probe and uv processes are spawned.
        unsafe { std::env::set_var("PATH", joined) };
    }
}

// ---- Systems ---------------------------------------------------------------------------

fn hub_file_drop_system(
    mut drops: MessageReader<FileDragAndDrop>,
    mut state: ResMut<ProjectHubState>,
) {
    for event in drops.read() {
        if !state.active {
            continue;
        }
        match event {
            FileDragAndDrop::HoveredFile { .. } => state.drop_hover = true,
            FileDragAndDrop::HoveredFileCanceled { .. } => state.drop_hover = false,
            FileDragAndDrop::DroppedFile { path_buf, .. } => {
                state.drop_hover = false;
                state.dropped = Some(path_buf.clone());
            }
        }
    }
}

/// Opens the bundle the hub asked for: [`crate::bundle_player::open_bundle`]
/// needs the whole world.
fn open_bundle_request_system(world: &mut World) {
    let Some(path) = world
        .get_resource_mut::<ProjectHubState>()
        .and_then(|mut state| state.bundle_request.take())
    else {
        return;
    };
    let opened = crate::bundle_player::open_bundle(world, &path);
    let mut state = world.resource_mut::<ProjectHubState>();
    match opened {
        Ok(()) => {
            state.recents.record_bundle(&path);
            let _ = state.recents.save();
            state.active = false;
        }
        Err(error) => {
            state.notice = Some(Notice::Error(format!("Could not play the file: {error}")));
        }
    }
}

fn project_hub_ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut state: ResMut<ProjectHubState>,
    mut pending: ResMut<PendingProjectOpen>,
    mut settings: ResMut<crate::settings::SettingsWindow>,
    #[cfg(not(target_arch = "wasm32"))] narration: Option<Res<crate::narration::NarrationSession>>,
    #[cfg(not(target_arch = "wasm32"))] mut exit: MessageWriter<AppExit>,
) {
    // Leaving starts Gaanim again; a recording or dictation in progress
    // would be lost, so that asks first.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let asked = std::mem::take(&mut state.home_requested);
        let switch = state.switch_requested.take();
        if asked || switch.is_some() {
            state.leave_pending = Some(switch);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let mut leave = None;
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(target) = state.leave_pending.clone() {
        let recording = narration.is_some_and(|session| session.capturing());
        if !recording {
            leave = Some(target);
            state.leave_pending = None;
        } else if let Ok(ctx) = contexts.ctx_mut() {
            match confirm_leave(ctx) {
                Some(true) => {
                    leave = Some(target);
                    state.leave_pending = None;
                }
                Some(false) => state.leave_pending = None,
                None => {}
            }
        }
    }
    // The new version starts before this one closes, so the window
    // seems to restart. Settings can ask for it from the editor too.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(switch) = leave {
        let started = std::env::current_exe().and_then(|executable| {
            let mut command = std::process::Command::new(executable);
            // Not the editor's stdin: a plugin's `--listen stdio` pipe
            // belongs to this process, not the one that replaces it.
            command
                .args(switch.iter())
                .stdin(std::process::Stdio::null());
            command.spawn()
        });
        match started {
            Ok(_) => {
                exit.write(AppExit::Success);
            }
            Err(error) => {
                gaanim_core::console::error("home", format!("could not open Home: {error}"));
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(executable) = state.update.restart.take() {
        match std::process::Command::new(&executable)
            .stdin(std::process::Stdio::null())
            .spawn()
        {
            Ok(_) => {
                exit.write(AppExit::Success);
            }
            Err(error) => {
                state.notice = Some(Notice::Error(format!(
                    "Could not open the new version ({}): {error}. Open it yourself.",
                    executable.display()
                )));
            }
        }
    }
    if !state.active {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let ctx = ctx.clone();
    let state = &mut *state;
    state.covered = settings.is_open();
    state.poll_workers(&mut pending);

    // Re-check the tools when the window comes back into focus: people install
    // uv or FFmpeg in a terminal and return here.
    let focused = ctx.input(|input| input.focused);
    if focused && !state.was_focused {
        state.check_tools();
        // A member may have been added outside Gaanim.
        if let Some(page) = state.workspace_page.as_mut() {
            page.refresh();
        }
    }
    state.was_focused = focused;

    if let Some(path) = state.dropped.take()
        && !state.busy()
    {
        state.open_path(&path);
    }
    if !state.busy() {
        handle_shortcuts(&ctx, state);
    }

    let screen = ctx.viewport_rect();
    let mut root = Ui::new(
        ctx.clone(),
        "project-hub".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(screen),
    );
    root.painter().rect_filled(screen, 0.0, palette::GRAPHITE);
    let sidebar = Rect::from_min_max(screen.min, pos2(screen.min.x + SIDEBAR_W, screen.max.y));
    let content = Rect::from_min_max(pos2(sidebar.max.x, screen.min.y), screen.max);
    paint_backdrop_grid(root.painter(), content);
    root.scope_builder(egui::UiBuilder::new().max_rect(sidebar), |ui| {
        sidebar_ui(ui, state, &mut settings)
    });
    root.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
        content_ui(ui, state)
    });

    if state.drop_hover {
        paint_drop_overlay(&ctx, content);
    }
    if state.preparing.is_some() || state.opening.is_some() {
        preparation_modal(&ctx, state);
        ctx.request_repaint();
    }
    state.opening_shown = state.opening.is_some();
}

/// Asks whether to leave while a recording runs: `Some(true)` leaves,
/// `Some(false)` stays, `None` while it waits for an answer.
#[cfg(not(target_arch = "wasm32"))]
fn confirm_leave(ctx: &egui::Context) -> Option<bool> {
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::new("gaanim-confirm-leave"))
        .frame(ui_kit::card_frame())
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(
                egui::RichText::new("A recording is in progress")
                    .size(17.0)
                    .strong()
                    .color(palette::TEXT),
            );
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "Leaving closes this project and loses the recording or dictation \
                         that has not finished. Finish it with Enter first to keep it.",
                    )
                    .size(13.5)
                    .color(palette::TEXT_MUTED),
                )
                .wrap(),
            );
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_button(ui, "Stay", None, true).clicked() {
                    answer = Some(false);
                }
                if secondary_button(ui, "Leave anyway", true).clicked() {
                    answer = Some(true);
                }
            });
        });
    if modal.should_close() && answer.is_none() {
        answer = Some(false);
    }
    answer
}

fn handle_shortcuts(ctx: &egui::Context, state: &mut ProjectHubState) {
    use egui::{Key, KeyboardShortcut, Modifiers};
    let (slides, video, bundle, open) = ctx.input_mut(|input| {
        (
            input.consume_shortcut(&KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                Key::N,
            )),
            input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::N)),
            input.consume_shortcut(&KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                Key::O,
            )),
            input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::O)),
        )
    });
    if slides {
        state.create_with_picker(ProjectKind::Slides);
    } else if video {
        state.create_with_picker(ProjectKind::Video);
    } else if bundle {
        state.open_bundle_with_picker();
    } else if open {
        state.open_with_picker();
    }
}

// ---- Sidebar ---------------------------------------------------------------------------

fn sidebar_ui(
    ui: &mut Ui,
    state: &mut ProjectHubState,
    settings: &mut crate::settings::SettingsWindow,
) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, palette::SURFACE);
    painter.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_white_alpha(10)),
    );

    let inner = rect.shrink2(vec2(16.0, 22.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        // Brand: the logo at 2 px per unit (142 × 32), as the brand asks.
        let (brand, _) = ui.allocate_exact_size(vec2(inner.width(), 40.0), Sense::hover());
        let logo = crate::brand::logo_size(2.0);
        crate::brand::paint_logo(
            ui,
            pos2(brand.min.x + 4.0, brand.center().y - logo.y / 2.0),
            2.0,
        );
        ui.painter().text(
            pos2(brand.max.x, brand.center().y + 4.0),
            Align2::RIGHT_CENTER,
            concat!("v", env!("CARGO_PKG_VERSION")),
            FontId::monospace(10.5),
            palette::TEXT_FAINT,
        );
        ui.add_space(26.0);

        let status_dot = state.tools.as_ref().map(|tools| match tools.readiness() {
            Readiness::Ready => palette::LOOP,
            Readiness::WithoutUv | Readiness::NeedsUv => palette::STOP,
        });
        for (section, icon, label) in [
            (Section::Projects, Icon::Grid, "Projects"),
            (Section::Environment, Icon::Package, "Environment"),
            (Section::Learn, Icon::Book, "Learn"),
        ] {
            let dot = (section == Section::Environment)
                .then_some(status_dot)
                .flatten();
            if nav_item(ui, icon, label, state.section == section, dot).clicked() {
                state.section = section;
            }
            ui.add_space(2.0);
        }
        if nav_item(ui, Icon::Settings, "Settings", false, None)
            .on_hover_text("Ctrl+,")
            .clicked()
        {
            settings.open();
        }

        // Environment status, pinned to the bottom.
        let status_h = 76.0;
        let status = Rect::from_min_max(pos2(inner.min.x, inner.max.y - status_h), inner.max);
        let response = ui
            .interact(status, ui.id().with("hub-status"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            state.section = Section::Environment;
        }
        paint_status_card(ui, status, state, response.hovered());
    });
}

fn nav_item(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    dot: Option<Color32>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 0.0, palette::SELECTED);
        painter.rect_filled(
            Rect::from_min_size(pos2(rect.min.x, rect.center().y - 8.0), vec2(3.0, 16.0)),
            0.0,
            palette::ACCENT,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, palette::HOVER);
    }
    let color = if selected || response.hovered() {
        palette::TEXT
    } else {
        palette::TEXT_MUTED
    };
    ui_kit::paint_icon(
        painter,
        Rect::from_center_size(pos2(rect.min.x + 22.0, rect.center().y), Vec2::splat(16.0)),
        icon,
        if selected { palette::ACCENT } else { color },
    );
    painter.text(
        pos2(rect.min.x + 42.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        color,
    );
    if let Some(dot) = dot {
        painter.rect_filled(
            Rect::from_center_size(pos2(rect.max.x - 14.0, rect.center().y), Vec2::splat(7.0)),
            0.0,
            dot,
        );
    }
    response
}

fn paint_status_card(ui: &Ui, rect: Rect, state: &ProjectHubState, hovered: bool) {
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        0.0,
        if hovered {
            Color32::from_white_alpha(10)
        } else {
            palette::FIELD
        },
    );
    let (color, title, detail) = match (&state.tools, state.checking.is_some()) {
        (None, _) => (palette::TEXT_FAINT, "Checking…".to_string(), String::new()),
        (Some(tools), _) => {
            let python =
                tools.probe.python.as_ref().map(|python| {
                    format!("Python {}.{}", python.version.major, python.version.minor)
                });
            match tools.readiness() {
                Readiness::Ready => (
                    palette::LOOP,
                    "All set".to_string(),
                    match python {
                        Some(python) => format!("{python} · uv"),
                        None => "uv will set up Python".to_string(),
                    },
                ),
                Readiness::WithoutUv => (
                    palette::STOP,
                    "Almost ready".to_string(),
                    "Install uv for isolated environments".to_string(),
                ),
                Readiness::NeedsUv => (
                    palette::STOP,
                    "uv is not installed".to_string(),
                    "Click here to see how".to_string(),
                ),
            }
        }
    };
    let left = rect.min.x + 14.0;
    painter.text(
        pos2(left, rect.min.y + 16.0),
        Align2::LEFT_CENTER,
        "ENVIRONMENT",
        FontId::proportional(10.0),
        palette::TEXT_FAINT,
    );
    painter.rect_filled(
        Rect::from_center_size(pos2(left + 4.0, rect.min.y + 40.0), Vec2::splat(8.0)),
        0.0,
        color,
    );
    painter.text(
        pos2(left + 16.0, rect.min.y + 40.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.5),
        palette::TEXT,
    );
    painter.text(
        pos2(left, rect.min.y + 60.0),
        Align2::LEFT_CENTER,
        detail,
        FontId::proportional(11.5),
        palette::TEXT_MUTED,
    );
}

// ---- Content ---------------------------------------------------------------------------

fn content_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let available = ui.available_width();
            let width = (available - 80.0).clamp(320.0, CONTENT_MAX_W);
            let margin = ((available - width) / 2.0).max(24.0);
            ui.add_space(36.0);
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    #[cfg(not(target_arch = "wasm32"))]
                    update_ui(ui, state);
                    notice_ui(ui, state);
                    match state.section {
                        Section::Projects => projects_ui(ui, state),
                        Section::Environment => environment_ui(ui, state),
                        Section::Learn => learn_ui(ui),
                    }
                    ui.add_space(48.0);
                });
            });
        });
}

fn page_header(ui: &mut Ui, eyebrow: &str, title: &str, subtitle: &str) {
    ui.label(caption(eyebrow));
    ui.add_space(8.0);
    ui.label(egui::RichText::new(title).size(28.0).color(palette::TEXT));
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(subtitle)
            .size(14.0)
            .color(palette::TEXT_MUTED),
    );
    ui.add_space(26.0);
}

/// The offer of a newer Gaanim, above the page: the version and its notes,
/// the install's progress, then the restart into it.
#[cfg(not(target_arch = "wasm32"))]
fn update_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    let offer = &mut state.update;
    let Some(release) = offer.available.clone() else {
        return;
    };
    if offer.dismissed {
        return;
    }
    let installing = offer.installing.is_some();
    let (color, icon, text) = offer.status(&release);
    let (mut notes, mut install, mut restart, mut dismiss) = (false, false, false, false);
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // As tall as the buttons, so the text centers beside them.
                ui.set_min_height(34.0);
                ui.spacing_mut().item_spacing.x = 10.0;
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                ui_kit::paint_icon(ui.painter(), icon_rect, icon, color);
                ui.label(egui::RichText::new(text).size(13.0).color(palette::TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !installing
                        && icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                            .on_hover_text("Close")
                            .clicked()
                    {
                        dismiss = true;
                    }
                    if offer.installed.is_some() {
                        restart = primary_button(ui, "Restart", None, true).clicked();
                    } else if !installing {
                        let label = if offer.error.is_some() {
                            "Retry"
                        } else {
                            "Update"
                        };
                        install = primary_button(ui, label, None, true).clicked();
                    }
                    if !release.page.is_empty() {
                        notes = secondary_button(ui, "What's new", true).clicked();
                    }
                });
            });
        });
    if installing {
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    if notes {
        let _ = crate::platform::open_detached(&release.page);
    }
    if install {
        offer.install();
    }
    if restart {
        offer.restart = offer.installed.clone();
    }
    if dismiss {
        offer.dismissed = true;
    }
    ui.add_space(20.0);
}

fn notice_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    let Some(notice) = &state.notice else {
        return;
    };
    let (color, icon, text) = match notice {
        Notice::Error(text) => (palette::DANGER, Icon::Warning, text.clone()),
        Notice::Info(text) => (palette::ACCENT, Icon::Check, text.clone()),
    };
    let fix = matches!(notice, Notice::Error(_))
        .then(|| notice_fix(&text))
        .flatten();
    let is_error = matches!(notice, Notice::Error(_));
    let mut dismiss = false;
    let mut go_fix = false;
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                ui_kit::paint_icon(ui.painter(), icon_rect, icon, color);
                let text_w = ui.available_width() - 40.0;
                ui.vertical(|ui| {
                    ui.set_width(text_w);
                    ui.label(egui::RichText::new(&text).size(13.0).color(palette::TEXT));
                    // An error says what to do about it.
                    if is_error {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if let Some(fix) = fix
                                && secondary_button(ui, fix.label(), true).clicked()
                            {
                                go_fix = true;
                            }
                            if secondary_button(ui, "Copy", true).clicked() {
                                ui.ctx().copy_text(text.clone());
                            }
                        });
                    }
                });
                if icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                    .on_hover_text("Close")
                    .clicked()
                {
                    dismiss = true;
                }
            });
        });
    if dismiss {
        state.notice = None;
    }
    if go_fix {
        state.section = Section::Environment;
        state.check_tools();
        state.notice = None;
    }
    ui.add_space(20.0);
}

/// What a Home error can lead to: the tool it is missing, which the
/// Environment page explains how to install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoticeFix {
    Python,
    Uv,
    Ffmpeg,
}

impl NoticeFix {
    fn label(self) -> &'static str {
        match self {
            Self::Python => "Set up Python",
            Self::Uv => "Install uv",
            Self::Ffmpeg => "Install FFmpeg",
        }
    }
}

/// The tool an error is about, if it names one.
fn notice_fix(text: &str) -> Option<NoticeFix> {
    let lower = text.to_lowercase();
    let word = |name: &str| {
        lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|token| token == name)
    };
    if word("ffmpeg") || word("ffprobe") {
        Some(NoticeFix::Ffmpeg)
    } else if word("uv") {
        Some(NoticeFix::Uv)
    } else if word("python") || word("pyo3") || word("venv") {
        Some(NoticeFix::Python)
    } else {
        None
    }
}

// ---- Projects --------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Start {
    Video,
    Slides,
    Open,
    Bundle,
}

fn projects_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    if state.workspace_page.is_some() {
        workspace_ui(ui, state);
        return;
    }
    page_header(
        ui,
        "PROJECTS",
        "What do you want to animate today?",
        "Create a project or open a recent one. Gaanim sets up Python for you.",
    );

    let busy = state.busy();
    let gap = 16.0;
    let columns = match ui.available_width() {
        width if width >= 900.0 => 4,
        width if width >= 460.0 => 2,
        _ => 1,
    };
    let card_w = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
    let starts = [
        (
            Start::Video,
            "New video",
            "A 16:9 animation ready to export to MP4, WebM or GIF.",
            "Ctrl N",
        ),
        (
            Start::Slides,
            "New presentation",
            "Slides with steps, notes and Presenter View.",
            "Ctrl Shift N",
        ),
        (
            Start::Open,
            "Open project",
            "Choose its folder or drag it onto this window.",
            "Ctrl O",
        ),
        (
            Start::Bundle,
            "Play .gaanim",
            "Play, present or export a .gaanim file.",
            "Ctrl Shift O",
        ),
    ];
    let mut chosen = None;
    for row in starts.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &(start, title, detail, shortcut) in row {
                if start_card(ui, start, title, detail, shortcut, card_w, !busy).clicked() {
                    chosen = Some(start);
                }
            }
        });
        ui.add_space(gap);
    }
    match chosen {
        Some(Start::Video) => state.create_with_picker(ProjectKind::Video),
        Some(Start::Slides) => state.create_with_picker(ProjectKind::Slides),
        Some(Start::Open) => state.open_with_picker(),
        Some(Start::Bundle) => state.open_bundle_with_picker(),
        None => {}
    }

    // Several projects in one repository are a workspace.
    if secondary_button(ui, "New workspace", !busy)
        .on_hover_text("Videos and talks in one repository that share code, assets and Python")
        .clicked()
    {
        state.create_workspace_with_picker();
    }

    ui.add_space(22.0);
    recents_ui(ui, state);
}

fn start_card(
    ui: &mut Ui,
    start: Start,
    title: &str,
    detail: &str,
    shortcut: &str,
    width: f32,
    enabled: bool,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 192.0), sense);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hovered = enabled && response.hovered();
    let rect = ui_kit::frame_hover(ui, response.id, rect, hovered);
    let painter = ui.painter();
    let accent = match start {
        Start::Video => palette::ACCENT,
        Start::Slides => palette::STOP,
        Start::Open => palette::TEXT,
        Start::Bundle => palette::LOOP,
    };
    painter.rect_filled(
        rect,
        0.0,
        if hovered {
            Color32::from_rgb(30, 30, 30)
        } else {
            palette::SURFACE
        },
    );
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(
            1.0,
            if hovered {
                accent.gamma_multiply(0.45)
            } else {
                Color32::from_white_alpha(12)
            },
        ),
        egui::StrokeKind::Inside,
    );

    let stage = Rect::from_min_max(
        rect.min + vec2(14.0, 14.0),
        pos2(rect.max.x - 14.0, rect.min.y + 110.0),
    );
    painter.rect_filled(stage, 0.0, Color32::from_rgb(14, 14, 14));
    let time = ui.input(|input| input.time);
    // Narrow cards must not let a scene spill onto their neighbours.
    let stage_painter = painter.with_clip_rect(stage.intersect(painter.clip_rect()));
    match start {
        Start::Video => paint_video_scene(&stage_painter, stage, hovered, time),
        Start::Slides => paint_slides_scene(&stage_painter, stage, hovered, time),
        Start::Open => paint_open_scene(&stage_painter, stage, hovered),
        Start::Bundle => paint_bundle_scene(&stage_painter, stage, hovered, time),
    }

    let text_left = rect.min.x + 18.0;
    let title_rect = painter.text(
        pos2(text_left, stage.max.y + 22.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(16.0),
        palette::TEXT,
    );
    let galley = painter.layout(
        detail.to_owned(),
        FontId::proportional(12.5),
        palette::TEXT_MUTED,
        rect.width() - 36.0,
    );
    painter.galley(
        pos2(text_left, stage.max.y + 36.0),
        galley,
        palette::TEXT_MUTED,
    );
    // Beside the title when it fits, otherwise in the stage's corner.
    let shortcut_galley = painter.layout_no_wrap(
        shortcut.to_owned(),
        FontId::monospace(10.5),
        palette::TEXT_FAINT,
    );
    let beside_title = title_rect.max.x + 12.0 + shortcut_galley.size().x <= rect.max.x - 16.0;
    let shortcut_at = if beside_title {
        pos2(rect.max.x - 16.0, stage.max.y + 22.0)
    } else {
        pos2(stage.max.x - 8.0, stage.min.y + 10.0)
    };
    painter.galley(
        Align2::RIGHT_CENTER
            .anchor_size(shortcut_at, shortcut_galley.size())
            .min,
        shortcut_galley,
        palette::TEXT_FAINT,
    );
    if hovered {
        ui.ctx().request_repaint();
    }
    response
}

/// A square that slides across the frame turning a quarter turn, trailed by
/// onion-skin frames, over a seek track: the playback bar in miniature.
fn paint_video_scene(painter: &egui::Painter, stage: Rect, playing: bool, time: f64) {
    let t = if playing {
        ((time * 0.45).fract()) as f32
    } else {
        0.62
    };
    let frame_h = stage.height() - 36.0;
    let frame = Rect::from_center_size(
        pos2(stage.center().x, stage.min.y + 10.0 + frame_h / 2.0),
        vec2(frame_h * 16.0 / 9.0, frame_h),
    );
    painter.rect_stroke(
        frame,
        0.0,
        Stroke::new(1.0, Color32::from_white_alpha(18)),
        egui::StrokeKind::Inside,
    );
    let half = frame.height() * 0.21;
    let x0 = frame.min.x + half * 1.8;
    let x1 = frame.max.x - half * 1.8;
    let square_at = |p: f32| {
        let eased = p * p * (3.0 - 2.0 * p);
        let center = pos2(x0 + (x1 - x0) * eased, frame.center().y);
        let (sin, cos) = (eased * std::f32::consts::FRAC_PI_2).sin_cos();
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(x, y): (f32, f32)| center + vec2(x * cos - y * sin, x * sin + y * cos) * half)
            .to_vec()
    };
    for (lag, alpha) in [(0.24, 0.18), (0.12, 0.36)] {
        painter.add(egui::Shape::convex_polygon(
            square_at((t - lag).max(0.0)),
            palette::ACCENT.gamma_multiply(alpha),
            Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::convex_polygon(
        square_at(t),
        palette::ACCENT,
        Stroke::NONE,
    ));

    let track = Rect::from_center_size(
        pos2(stage.center().x, stage.max.y - 14.0),
        vec2(stage.width() - 40.0, 5.0),
    );
    paint_seek_track(painter, track, &[], t, palette::ACCENT);
    paint_knob(
        painter,
        pos2(track.min.x + track.width() * t, track.center().y),
        5.0,
    );
}

/// Three slides and a chaptered track with stops; the playhead steps from one
/// slide to the next, as a presentation does.
fn paint_slides_scene(painter: &egui::Painter, stage: Rect, playing: bool, time: f64) {
    let current = if playing {
        ((time * 1.2) as usize) % 3
    } else {
        1
    };
    let slide_h = stage.height() - 48.0;
    let slide_w = slide_h * 16.0 / 9.0;
    let gap = 10.0;
    let total_w = slide_w * 3.0 + gap * 2.0;
    for index in 0..3 {
        let slide = Rect::from_min_size(
            pos2(
                stage.center().x - total_w / 2.0 + index as f32 * (slide_w + gap),
                stage.min.y + 12.0,
            ),
            vec2(slide_w, slide_h),
        );
        let active = index == current;
        painter.rect_filled(
            slide,
            0.0,
            Color32::from_white_alpha(if active { 12 } else { 5 }),
        );
        painter.rect_stroke(
            slide,
            0.0,
            Stroke::new(
                if active { 1.5 } else { 1.0 },
                if active {
                    palette::STOP
                } else {
                    Color32::from_white_alpha(16)
                },
            ),
            egui::StrokeKind::Inside,
        );
        let line_color = Color32::from_white_alpha(if active { 60 } else { 26 });
        let inset = slide.shrink2(vec2(slide.width() * 0.16, slide.height() * 0.3));
        painter.rect_filled(
            Rect::from_min_size(inset.min, vec2(inset.width() * 0.8, 3.0)),
            0.0,
            line_color,
        );
        painter.rect_filled(
            Rect::from_min_size(inset.min + vec2(0.0, 8.0), vec2(inset.width() * 0.55, 3.0)),
            0.0,
            line_color,
        );
    }
    let track = Rect::from_center_size(
        pos2(stage.center().x, stage.max.y - 16.0),
        vec2(stage.width() - 40.0, 5.0),
    );
    let playhead = (current as f32 + 0.5) / 3.0;
    paint_seek_track(
        painter,
        track,
        &[1.0 / 3.0, 2.0 / 3.0],
        playhead,
        palette::ACCENT,
    );
    for index in 0..3 {
        let x = track.min.x + track.width() * (index as f32 + 0.5) / 3.0;
        painter.rect_filled(
            Rect::from_center_size(pos2(x, track.max.y + 6.0), Vec2::splat(3.5)),
            0.0,
            palette::STOP.gamma_multiply(if index <= current { 0.45 } else { 0.9 }),
        );
    }
    paint_knob(
        painter,
        pos2(track.min.x + track.width() * playhead, track.center().y),
        5.0,
    );
}

fn paint_open_scene(painter: &egui::Painter, stage: Rect, hovered: bool) {
    let zone = stage.shrink(12.0);
    let color = if hovered {
        palette::TEXT_MUTED
    } else {
        Color32::from_white_alpha(40)
    };
    let corners = [
        zone.left_top(),
        zone.right_top(),
        zone.right_bottom(),
        zone.left_bottom(),
        zone.left_top(),
    ];
    painter.extend(egui::Shape::dashed_line(
        &corners,
        Stroke::new(1.0, color),
        5.0,
        4.0,
    ));
    ui_kit::paint_icon(
        painter,
        Rect::from_center_size(zone.center(), Vec2::splat(34.0)),
        Icon::Folder,
        if hovered {
            palette::TEXT
        } else {
            palette::TEXT_MUTED
        },
    );
}

/// A sealed package over a finished track: every frame is already recorded,
/// and the playhead only plays them back.
fn paint_bundle_scene(painter: &egui::Painter, stage: Rect, playing: bool, time: f64) {
    let t = if playing {
        ((time * 0.35).fract()) as f32
    } else {
        0.38
    };
    let color = if playing {
        palette::LOOP
    } else {
        palette::TEXT_MUTED
    };
    let icon = Rect::from_center_size(
        pos2(
            stage.center().x,
            stage.min.y + (stage.height() - 26.0) / 2.0,
        ),
        Vec2::splat(38.0),
    );
    ui_kit::paint_icon(painter, icon, Icon::Package, color);
    let track = Rect::from_center_size(
        pos2(stage.center().x, stage.max.y - 14.0),
        vec2(stage.width() - 40.0, 5.0),
    );
    paint_seek_track(
        painter,
        track,
        &[0.3, 0.62],
        1.0,
        color.gamma_multiply(0.35),
    );
    paint_knob(
        painter,
        pos2(track.min.x + track.width() * t, track.center().y),
        5.0,
    );
}

/// What the workspace page asked for this frame.
enum PageAction {
    Back,
    Create(ProjectKind),
    #[cfg(not(target_arch = "wasm32"))]
    Check,
    Member(RowAction, Box<WorkspaceMember>),
}

/// A workspace's page: its members grouped as its manifest says, with
/// ways to add members and to check them all.
fn workspace_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    let busy = state.busy() || state.covered;
    let view = state.recent_view;
    let ctx = ui.ctx().clone();
    let ProjectHubState {
        workspace_page,
        thumbnails,
        ..
    } = state;
    let Some(page) = workspace_page.as_mut() else {
        return;
    };
    let mut chosen = None;

    if ui
        .add(
            egui::Label::new(
                egui::RichText::new("‹ Projects")
                    .size(12.5)
                    .color(palette::TEXT_MUTED),
            )
            .sense(Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        chosen = Some(PageAction::Back);
    }
    ui.add_space(16.0);
    let workspace = &page.workspace;
    let count = page.members.len();
    let mut subtitle = format!(
        "{} · {count} member{}",
        display_path(&workspace.root),
        if count == 1 { "" } else { "s" }
    );
    let shared: Vec<String> = workspace
        .python_roots
        .iter()
        .chain(&workspace.asset_roots)
        .filter_map(|folder| workspace.relative_path(folder))
        .collect();
    if !shared.is_empty() {
        subtitle.push_str(&format!(" · shares {}", shared.join(", ")));
    }
    page_header(ui, "WORKSPACE", &workspace.name, &subtitle);

    ui.horizontal(|ui| {
        if primary_button(ui, "New video", Some(Icon::Plus), !busy).clicked() {
            chosen = Some(PageAction::Create(ProjectKind::Video));
        }
        ui.add_space(8.0);
        if secondary_button(ui, "New presentation", !busy).clicked() {
            chosen = Some(PageAction::Create(ProjectKind::Slides));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.add_space(8.0);
            let checkable = page
                .members
                .iter()
                .filter(|member| member.project.is_ok())
                .count();
            let running = page.check.as_ref().filter(|check| check.running);
            let label = match running {
                Some(check) => format!("Checking… {}/{checkable}", check.results.len()),
                None => "Check all".to_owned(),
            };
            if secondary_button(ui, &label, !busy && running.is_none() && checkable > 0)
                .on_hover_text("Run gaanim check on every member")
                .clicked()
            {
                chosen = Some(PageAction::Check);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui_kit::field_frame()
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut page.query)
                            .hint_text("Search")
                            .desired_width(150.0)
                            .frame(egui::Frame::NONE),
                    );
                });
        });
    });
    ui.add_space(22.0);

    let query = page.query.trim().to_lowercase();
    let mut groups: Vec<(&str, Vec<&WorkspaceMember>)> = Vec::new();
    for member in &page.members {
        let matches = query.is_empty()
            || member.name().to_lowercase().contains(&query)
            || member.path.to_lowercase().contains(&query)
            || member.group.to_lowercase().contains(&query);
        if !matches {
            continue;
        }
        match groups.iter_mut().find(|(group, _)| *group == member.group) {
            Some((_, members)) => members.push(member),
            None => groups.push((&member.group, vec![member])),
        }
    }
    if page.members.is_empty() {
        ui.add(
            egui::Label::new(
                egui::RichText::new(
                    "This workspace has no members yet. Create one here, or add the folders \
                     of its projects to `members` in its gaanim.toml.",
                )
                .size(13.5)
                .color(palette::TEXT_FAINT),
            )
            .wrap(),
        );
    } else if groups.is_empty() {
        ui.label(
            egui::RichText::new(format!("No member matches \"{}\".", page.query.trim()))
                .size(13.5)
                .color(palette::TEXT_FAINT),
        );
    }
    for (group, members) in &groups {
        if !group.is_empty() {
            ui.label(caption(&group.to_uppercase()));
            ui.add_space(10.0);
        }
        let rows: Vec<RecentRow> = members
            .iter()
            .map(|member| {
                #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
                let mut row = RecentRow::member(member);
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(outcome) = page
                    .check
                    .as_ref()
                    .and_then(|check| check.results.get(&member.root))
                {
                    row.status = Some(outcome.label());
                }
                row
            })
            .collect();
        match view {
            RecentView::List => {
                for (member, row) in members.iter().zip(&rows) {
                    if let Some(action) = recent_row(ui, row, !busy) {
                        chosen = Some(PageAction::Member(action, Box::new((*member).clone())));
                    }
                    ui.add_space(8.0);
                }
            }
            RecentView::Grid => {
                const GAP: f32 = 14.0;
                const MIN_CARD: f32 = 220.0;
                let width = ui.available_width();
                let columns = (((width + GAP) / (MIN_CARD + GAP)).floor() as usize).max(1);
                let card = (width - GAP * (columns - 1) as f32) / columns as f32;
                let pairs: Vec<(&&WorkspaceMember, &RecentRow)> =
                    members.iter().zip(&rows).collect();
                for chunk in pairs.chunks(columns) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for (member, row) in chunk {
                            let cover = member.project.as_ref().ok().and_then(|project| {
                                thumbnails.get(
                                    &ctx,
                                    &RecentGroup {
                                        entry: RecentEntry::Project(project.clone()),
                                        bundle: None,
                                        pinned: false,
                                    },
                                )
                            });
                            if let Some(action) = recent_card(ui, row, cover.as_ref(), card, !busy)
                            {
                                chosen =
                                    Some(PageAction::Member(action, Box::new((**member).clone())));
                            }
                        }
                    });
                    ui.add_space(GAP);
                }
            }
        }
        ui.add_space(14.0);
    }

    // New members start where the first one is, or at the workspace root.
    let start = page
        .members
        .first()
        .and_then(|member| member.root.parent())
        .unwrap_or(&page.workspace.root)
        .to_path_buf();
    match chosen {
        Some(PageAction::Back) => *workspace_page = None,
        Some(PageAction::Create(kind)) => {
            state.create_with_picker_in(kind, &start);
            if let Some(page) = state.workspace_page.as_mut() {
                page.refresh();
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        Some(PageAction::Check) => page.check = WorkspaceCheck::start(&page.members),
        Some(PageAction::Member(RowAction::Open, member)) => match member.project {
            Ok(project) => state.open_project(project, false),
            Err(error) => state.notice = Some(Notice::Error(error)),
        },
        Some(PageAction::Member(RowAction::ShowFolder, member)) => {
            let _ = crate::platform::open_detached(&member.root);
        }
        Some(PageAction::Member(RowAction::PlayBundle(path), _)) => state.open_bundle(&path),
        Some(PageAction::Member(_, _)) | None => {}
    }
}

fn recents_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    let all = state.recents.groups();
    let query = state.recent_query.trim().to_lowercase();
    let entries: Vec<RecentGroup> = all
        .iter()
        .filter(|group| query.is_empty() || recent_matches(group, &query))
        .cloned()
        .collect();
    ui.horizontal(|ui| {
        ui.label(caption("RECENT"));
        if !all.is_empty() {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(all.len().to_string())
                    .monospace()
                    .size(11.0)
                    .color(palette::TEXT_FAINT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (view, icon, hint) in [
                    (RecentView::Grid, Icon::Grid, "Cards with their covers"),
                    (RecentView::List, Icon::List, "A list"),
                ] {
                    let tone = if state.recent_view == view {
                        ButtonTone::On(ui_kit::ToggleColor::Accent)
                    } else {
                        ButtonTone::Ghost
                    };
                    if icon_button(ui, icon, tone, true)
                        .on_hover_text(hint)
                        .clicked()
                        && state.recent_view != view
                    {
                        state.recent_view = view;
                        let saved = HomePreferences { recent_view: view }.save();
                        if let Err(error) = saved {
                            state.notice =
                                Some(Notice::Error(format!("Could not save the view: {error}")));
                        }
                    }
                }
                ui.add_space(10.0);
                ui_kit::field_frame()
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut state.recent_query)
                                .hint_text("Search")
                                .desired_width(150.0)
                                .frame(egui::Frame::NONE),
                        );
                    });
                ui.add_space(10.0);
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new("Clear list")
                                .size(12.0)
                                .color(palette::TEXT_MUTED),
                        )
                        .sense(Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("Forgets every entry that is not pinned")
                    .clicked()
                {
                    state.recents.clear();
                    let _ = state.recents.save();
                }
            });
        }
    });
    ui.add_space(12.0);

    if all.is_empty() {
        empty_recents(ui, state);
        return;
    }
    if entries.is_empty() {
        ui.label(
            egui::RichText::new(format!(
                "Nothing recent matches \"{}\".",
                state.recent_query.trim()
            ))
            .size(13.5)
            .color(palette::TEXT_FAINT),
        );
        return;
    }
    let busy = state.busy() || state.covered;
    let mut action = None;
    match state.recent_view {
        RecentView::List => {
            for group in &entries {
                if let Some(chosen) = recent_row(ui, &RecentRow::of(group), !busy) {
                    action = Some((chosen, group.entry.clone()));
                }
                ui.add_space(8.0);
            }
        }
        RecentView::Grid => {
            const GAP: f32 = 14.0;
            const MIN_CARD: f32 = 220.0;
            let width = ui.available_width();
            let columns = (((width + GAP) / (MIN_CARD + GAP)).floor() as usize).max(1);
            let card = (width - GAP * (columns - 1) as f32) / columns as f32;
            let ctx = ui.ctx().clone();
            for row in entries.chunks(columns) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    for group in row {
                        let cover = state.thumbnails.get(&ctx, group);
                        if let Some(chosen) =
                            recent_card(ui, &RecentRow::of(group), cover.as_ref(), card, !busy)
                        {
                            action = Some((chosen, group.entry.clone()));
                        }
                    }
                });
                ui.add_space(GAP);
            }
        }
    }
    match action {
        Some((RowAction::Open, RecentEntry::Project(project))) => {
            state.open_project(project, false)
        }
        Some((RowAction::Open, RecentEntry::Bundle(bundle))) => state.open_bundle(&bundle.path),
        Some((RowAction::Open, RecentEntry::Workspace(workspace))) => {
            state.open_workspace(workspace)
        }
        Some((RowAction::ShowFolder, entry)) => {
            let folder = match &entry {
                RecentEntry::Project(project) => Some(project.root.as_path()),
                RecentEntry::Bundle(bundle) => bundle.path.parent(),
                RecentEntry::Workspace(workspace) => Some(workspace.root.as_path()),
            };
            if let Some(folder) = folder {
                let _ = crate::platform::open_detached(folder);
            }
        }
        Some((RowAction::Remove, entry)) => {
            state.recents.remove(entry.path());
            let _ = state.recents.save();
        }
        Some((RowAction::Pin(pinned), entry)) => {
            state.recents.set_pinned(entry.path(), pinned);
            let _ = state.recents.save();
        }
        Some((RowAction::PlayBundle(path), _)) => state.open_bundle(&path),
        None => {}
    }
}

/// Whether `group` matches the lowercase `query`: its name, its path, or
/// its bundle's name.
fn recent_matches(group: &RecentGroup, query: &str) -> bool {
    let row = RecentRow::of(group);
    row.name.to_lowercase().contains(query)
        || row.path.to_string_lossy().to_lowercase().contains(query)
        || group
            .bundle
            .as_ref()
            .is_some_and(|bundle| bundle.name.to_lowercase().contains(query))
}

enum RowAction {
    Open,
    ShowFolder,
    Remove,
    /// Pin to the top, or unpin.
    Pin(bool),
    /// Play the project's newest bundle.
    PlayBundle(PathBuf),
}

/// What a recent row shows for a project or a bundle.
struct RecentRow {
    name: String,
    path: PathBuf,
    icon: Icon,
    color: Color32,
    label: &'static str,
    age: Option<String>,
    pinned: bool,
    /// The project's newest bundle, and how long ago it was recorded.
    bundle: Option<(PathBuf, Option<String>)>,
    /// A workspace member, which is listed but not pinned or removed.
    member: bool,
    /// Why the member's manifest is invalid, shown instead of its path.
    error: Option<String>,
    /// What the last *Check all* said, shown instead of the kind.
    status: Option<(String, Color32)>,
}

impl RecentRow {
    fn of(group: &RecentGroup) -> Self {
        let pinned = group.pinned;
        let bundle = group
            .bundle
            .as_ref()
            .map(|bundle| (bundle.path.clone(), bundle_age(bundle)));
        match &group.entry {
            RecentEntry::Project(project) => Self {
                name: project.display_name(),
                path: project.root.clone(),
                icon: match project.manifest.kind {
                    ProjectKind::Video => Icon::Play,
                    ProjectKind::Slides => Icon::Present,
                },
                color: kind_color(project.manifest.kind),
                label: kind_label(project.manifest.kind),
                age: edited_ago(project),
                pinned,
                bundle,
                member: false,
                error: None,
                status: None,
            },
            RecentEntry::Bundle(bundle) => Self {
                name: bundle.name.clone(),
                path: bundle.path.clone(),
                icon: Icon::Package,
                color: palette::LOOP,
                label: ".gaanim",
                age: recorded_ago(bundle),
                pinned,
                bundle: None,
                member: false,
                error: None,
                status: None,
            },
            RecentEntry::Workspace(workspace) => Self {
                name: workspace.name.clone(),
                path: workspace.root.clone(),
                icon: Icon::Layout,
                color: palette::TEXT_MUTED,
                label: "Workspace",
                age: None,
                pinned,
                bundle: None,
                member: false,
                error: None,
                status: None,
            },
        }
    }

    /// A member on its workspace's page, under its own name.
    fn member(member: &WorkspaceMember) -> Self {
        match &member.project {
            Ok(project) => Self {
                name: project.manifest.name.clone(),
                member: true,
                ..Self::of(&RecentGroup {
                    entry: RecentEntry::Project(project.clone()),
                    bundle: None,
                    pinned: false,
                })
            },
            Err(error) => Self {
                name: member.name().to_owned(),
                path: member.root.clone(),
                icon: Icon::Warning,
                color: palette::DANGER,
                label: "Invalid",
                age: None,
                pinned: false,
                bundle: None,
                member: true,
                error: Some(error.clone()),
                status: None,
            },
        }
    }

    /// The kind, or what the last check said, and its color.
    fn meta(&self) -> (&str, Color32) {
        match &self.status {
            Some((status, color)) => (status, *color),
            None => (self.label, self.color),
        }
    }

    /// How many hover buttons [`recent_actions`] draws.
    fn action_count(&self) -> usize {
        let base = if self.member { 1 } else { 3 };
        base + usize::from(self.bundle.is_some())
    }

    /// The age line: when it was edited, and its bundle's age.
    fn ages(&self) -> Option<String> {
        let bundle = self.bundle.as_ref().map(|(_, age)| match age {
            Some(age) => format!("bundle {age}"),
            None => "bundle".to_owned(),
        });
        match (&self.age, bundle) {
            (Some(age), Some(bundle)) => Some(format!("{age} · {bundle}")),
            (Some(age), None) => Some(age.clone()),
            (None, bundle) => bundle,
        }
    }
}

/// The hover buttons every recent entry has, right to left: remove, show
/// the folder, pin, and play the project's bundle.
fn recent_actions(ui: &mut Ui, row: &RecentRow, action: &mut Option<RowAction>) {
    if !row.member
        && icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
            .on_hover_text("Remove from recent (does not delete files)")
            .clicked()
    {
        *action = Some(RowAction::Remove);
    }
    if icon_button(ui, Icon::Folder, ButtonTone::Ghost, true)
        .on_hover_text("Show folder")
        .clicked()
    {
        *action = Some(RowAction::ShowFolder);
    }
    if row.member {
        if let Some((bundle, _)) = &row.bundle
            && icon_button(ui, Icon::Package, ButtonTone::Ghost, true)
                .on_hover_text("Play its newest bundle")
                .clicked()
        {
            *action = Some(RowAction::PlayBundle(bundle.clone()));
        }
        return;
    }
    let pin_tone = if row.pinned {
        ButtonTone::On(ui_kit::ToggleColor::Stop)
    } else {
        ButtonTone::Ghost
    };
    if icon_button(ui, Icon::Pin, pin_tone, true)
        .on_hover_text(if row.pinned {
            "Unpin"
        } else {
            "Pin to the top"
        })
        .clicked()
    {
        *action = Some(RowAction::Pin(!row.pinned));
    }
    if let Some((bundle, _)) = &row.bundle
        && icon_button(ui, Icon::Package, ButtonTone::Ghost, true)
            .on_hover_text("Play its newest bundle")
            .clicked()
    {
        *action = Some(RowAction::PlayBundle(bundle.clone()));
    }
}

/// A pinned entry's mark: a small gold pin.
fn paint_pin(painter: &egui::Painter, center: Pos2) {
    ui_kit::paint_icon(
        painter,
        Rect::from_center_size(center, Vec2::splat(12.0)),
        Icon::Pin,
        palette::STOP,
    );
}

fn recent_row(ui: &mut Ui, row: &RecentRow, enabled: bool) -> Option<RowAction> {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), 66.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && ui.rect_contains_pointer(rect);
    let color = row.color;
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        0.0,
        if hovered {
            Color32::from_rgb(30, 30, 30)
        } else {
            palette::SURFACE
        },
    );
    if hovered {
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, color.gamma_multiply(0.35)),
            egui::StrokeKind::Inside,
        );
    }

    let tile = Rect::from_center_size(pos2(rect.min.x + 33.0, rect.center().y), Vec2::splat(38.0));
    painter.rect_filled(tile, 0.0, color.gamma_multiply(0.16));
    ui_kit::paint_icon(
        painter,
        Rect::from_center_size(tile.center(), Vec2::splat(16.0)),
        row.icon,
        color,
    );

    let text_left = tile.max.x + 14.0;
    let actions_w = if hovered { 290.0 } else { 220.0 };
    let text_w = (rect.max.x - actions_w - text_left).max(80.0);
    let name = painter.layout_no_wrap(row.name.clone(), FontId::proportional(15.0), palette::TEXT);
    let name_width = name.size().x;
    painter.galley(pos2(text_left, rect.center().y - 19.0), name, palette::TEXT);
    if row.pinned {
        paint_pin(
            painter,
            pos2(text_left + name_width + 12.0, rect.center().y - 10.0),
        );
    }
    let (path_text, path_color) = match &row.error {
        Some(error) => (error.clone(), palette::DANGER),
        None => (display_path(&row.path), palette::TEXT_FAINT),
    };
    let path = painter.layout(
        path_text,
        FontId::proportional(11.5),
        path_color,
        f32::INFINITY,
    );
    let path_clip = Rect::from_min_size(pos2(text_left, rect.center().y + 2.0), vec2(text_w, 16.0));
    painter
        .with_clip_rect(path_clip)
        .galley(path_clip.min, path, palette::TEXT_FAINT);

    let mut action = None;
    if hovered {
        let actions = Rect::from_min_max(
            pos2(rect.max.x - actions_w, rect.min.y),
            rect.max - vec2(14.0, 0.0),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(actions)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if primary_button(ui, "Open", Some(Icon::Play), true).clicked() {
                    action = Some(RowAction::Open);
                }
                ui.add_space(6.0);
                recent_actions(ui, row, &mut action);
            },
        );
    } else {
        let meta_x = rect.max.x - 18.0;
        let (meta, meta_color) = row.meta();
        painter.text(
            pos2(meta_x, rect.center().y - 9.0),
            Align2::RIGHT_CENTER,
            meta,
            FontId::proportional(12.0),
            meta_color,
        );
        if let Some(ages) = row.ages() {
            painter.text(
                pos2(meta_x, rect.center().y + 10.0),
                Align2::RIGHT_CENTER,
                ages,
                FontId::proportional(11.5),
                palette::TEXT_FAINT,
            );
        }
    }
    if action.is_none()
        && response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    {
        action = Some(RowAction::Open);
    }
    action
}

/// A recent entry as a card: its cover, or its kind's icon while it has
/// none, over its name, kind and age.
fn recent_card(
    ui: &mut Ui,
    row: &RecentRow,
    cover: Option<&egui::TextureHandle>,
    width: f32,
    enabled: bool,
) -> Option<RowAction> {
    let stage_h = (width * 9.0 / 16.0).round();
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, stage_h + 56.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && ui.rect_contains_pointer(rect);
    let rect = ui_kit::frame_hover(ui, response.id, rect, hovered);
    let color = row.color;
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        0.0,
        if hovered {
            Color32::from_rgb(30, 30, 30)
        } else {
            palette::SURFACE
        },
    );
    let stage = Rect::from_min_size(rect.min, vec2(width, stage_h));
    painter.rect_filled(stage, 0.0, Color32::from_rgb(14, 14, 14));
    match cover {
        Some(texture) => {
            // Contained in the 16:9 stage, whatever the scene's shape.
            let size = texture.size_vec2();
            let scale = (stage.width() / size.x).min(stage.height() / size.y);
            painter.image(
                texture.id(),
                Rect::from_center_size(stage.center(), size * scale),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            let tile = Rect::from_center_size(stage.center(), Vec2::splat(44.0));
            painter.rect_filled(tile, 0.0, color.gamma_multiply(0.16));
            ui_kit::paint_icon(
                painter,
                Rect::from_center_size(tile.center(), Vec2::splat(18.0)),
                row.icon,
                color,
            );
        }
    }
    if hovered {
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, color.gamma_multiply(0.45)),
            egui::StrokeKind::Inside,
        );
    }

    let text_left = rect.min.x + 12.0;
    let text_w = width - 24.0;
    let name = painter.layout_no_wrap(row.name.clone(), FontId::proportional(14.5), palette::TEXT);
    let name_clip = Rect::from_min_size(pos2(text_left, stage.max.y + 10.0), vec2(text_w, 20.0));
    painter
        .with_clip_rect(name_clip)
        .galley(name_clip.min, name, palette::TEXT);
    let meta_y = stage.max.y + 38.0;
    let (meta, meta_color) = row.meta();
    let label = painter.text(
        pos2(text_left, meta_y),
        Align2::LEFT_CENTER,
        meta,
        FontId::proportional(11.5),
        meta_color,
    );
    if let Some(ages) = row.ages() {
        painter
            .with_clip_rect(Rect::from_min_max(
                pos2(label.max.x + 8.0, stage.max.y),
                pos2(rect.max.x - 10.0, rect.max.y),
            ))
            .text(
                pos2(label.max.x + 8.0, meta_y),
                Align2::LEFT_CENTER,
                ages,
                FontId::proportional(11.5),
                palette::TEXT_FAINT,
            );
    }
    if row.pinned {
        let mark = Rect::from_min_size(stage.min + vec2(6.0, 6.0), Vec2::splat(22.0));
        painter.rect_filled(mark, 0.0, palette::PANEL);
        paint_pin(painter, mark.center());
    }

    let mut action = None;
    if hovered {
        let buttons = row.action_count() as f32;
        let actions = Rect::from_min_max(
            pos2(stage.max.x - 10.0 - buttons * 34.0, stage.min.y + 6.0),
            pos2(stage.max.x - 6.0, stage.min.y + 40.0),
        );
        // A child that takes no room in the row, so the cards beside this
        // one stay where they are.
        let ui = &mut ui.new_child(
            egui::UiBuilder::new()
                .max_rect(actions)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.painter().rect_filled(ui.max_rect(), 0.0, palette::PANEL);
        recent_actions(ui, row, &mut action);
    }
    if enabled
        && action.is_none()
        && response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(row.error.clone().unwrap_or_else(|| display_path(&row.path)))
            .clicked()
    {
        action = Some(RowAction::Open);
    }
    action
}

fn empty_recents(ui: &mut Ui, state: &mut ProjectHubState) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 180.0), Sense::hover());
    let painter = ui.painter();
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    painter.extend(egui::Shape::dashed_line(
        &corners,
        Stroke::new(1.0, Color32::from_white_alpha(22)),
        6.0,
        5.0,
    ));
    paint_pixel_mark(
        painter,
        pos2(rect.center().x - 24.0, rect.min.y + 26.0).round(),
        3.0,
    );
    painter.text(
        pos2(rect.center().x, rect.min.y + 98.0),
        Align2::CENTER_CENTER,
        "Your projects will appear here",
        FontId::proportional(15.0),
        palette::TEXT,
    );
    painter.text(
        pos2(rect.center().x, rect.min.y + 120.0),
        Align2::CENTER_CENTER,
        "Create your first video: choose a folder and Gaanim sets up everything else.",
        FontId::proportional(12.5),
        palette::TEXT_MUTED,
    );
    let button =
        Rect::from_center_size(pos2(rect.center().x, rect.min.y + 154.0), vec2(170.0, 34.0));
    let clicked = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(button)
                .layout(egui::Layout::top_down(egui::Align::Center)),
            |ui| primary_button(ui, "New video", Some(Icon::Plus), !state.busy()).clicked(),
        )
        .inner;
    if clicked {
        state.create_with_picker(ProjectKind::Video);
    }
}

// ---- Environment -----------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolState {
    Ok,
    Missing,
    Optional,
    Handled,
}

fn environment_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    page_header(
        ui,
        "ENVIRONMENT",
        "What Gaanim needs on your computer",
        "Each project has its own isolated Python, which Gaanim sets up on its own. For that \
         it needs uv; FFmpeg is optional.",
    );

    ui.horizontal(|ui| {
        let checking = state.checking.is_some();
        if primary_button(
            ui,
            if checking {
                "Checking…"
            } else {
                "Check again"
            },
            Some(Icon::Loop),
            !checking,
        )
        .clicked()
        {
            state.check_tools();
        }
        if let Some(project) = &state.waiting {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(format!(
                    "“{}” will open as soon as the environment is ready.",
                    project.manifest.name
                ))
                .size(12.5)
                .color(palette::TEXT_MUTED),
            );
        }
    });
    ui.add_space(20.0);

    let Some(tools) = state.tools.clone() else {
        ui.label(
            egui::RichText::new("Looking for Python, uv and FFmpeg…")
                .size(13.0)
                .color(palette::TEXT_MUTED),
        );
        return;
    };

    let uv_state = if tools.probe.uv.is_some() {
        ToolState::Ok
    } else {
        ToolState::Missing
    };
    let uv_detail = match &tools.probe.uv {
        Some(uv) => format!("Installed · {}", uv.version),
        None => "Not installed · needed to create and set up projects".into(),
    };
    tool_card(
        ui,
        "uv",
        "Downloads Python and creates each project's environment. You use it once; Gaanim does the rest.",
        uv_state,
        &uv_detail,
        (uv_state == ToolState::Missing).then(uv_help),
        &[
            "When it finishes, come back to this window: Gaanim detects it on its own.",
            "If it does not show up, click “Check again”.",
        ],
    );
    ui.add_space(12.0);

    let (python_state, python_detail) = match &tools.probe.python {
        Some(python) if python.version.is_supported() => (
            ToolState::Ok,
            format!(
                "Python {}.{}.{} · {}",
                python.version.major,
                python.version.minor,
                python.version.patch,
                match python.source {
                    PythonSource::ActiveVenv => "active environment",
                    PythonSource::ProjectVenv => "project environment",
                    PythonSource::System => "system",
                }
            ),
        ),
        Some(python) if tools.probe.uv.is_some() => (
            ToolState::Handled,
            format!(
                "You have Python {}.{}; uv will download Python {PYTHON_VERSION} for your projects",
                python.version.major, python.version.minor
            ),
        ),
        None if tools.probe.uv.is_some() => (
            ToolState::Handled,
            "uv will download it when you create your first project".into(),
        ),
        Some(python) => (
            ToolState::Missing,
            format!(
                "Python {}.{} is not supported · install uv and it will bring Python {PYTHON_VERSION}",
                python.version.major, python.version.minor
            ),
        ),
        None => (
            ToolState::Missing,
            format!("Not found · install uv and it will bring Python {PYTHON_VERSION}"),
        ),
    };
    tool_card(
        ui,
        &format!("Python {PYTHON_VERSION}"),
        "The language you write scenes in. You do not have to install it by hand.",
        python_state,
        &python_detail,
        None,
        &[],
    );
    ui.add_space(12.0);

    let ffmpeg_state = if tools.ffmpeg {
        ToolState::Ok
    } else {
        ToolState::Optional
    };
    tool_card(
        ui,
        "FFmpeg",
        "Optional. Exports to MP4 and WebM and lets scenes use video and audio. Without it \
         you can export WebP, GIF and PNG.",
        ffmpeg_state,
        if tools.ffmpeg {
            "Installed"
        } else {
            "Not installed"
        },
        (!tools.ffmpeg).then(ffmpeg_help),
        &["When it finishes, close and reopen Gaanim if exporting still cannot find it."],
    );
    if !crate::WEB {
        ui.add_space(12.0);
        association_card(ui, state);
    }
}

/// Open `.gaanim` files with a double click, as `gaanim register` does.
fn association_card(ui: &mut Ui, state: &mut ProjectHubState) {
    use gaanim_project::association;
    let associated = *state
        .associated
        .get_or_insert_with(association::is_registered);
    let (color, icon, badge) = if associated {
        (palette::LOOP, Icon::Check, "Associated")
    } else {
        (palette::TEXT_MUTED, Icon::Warning, "Optional")
    };
    let mut clicked = false;
    egui::Frame::new()
        .fill(palette::SURFACE)
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(12)))
        .inner_margin(egui::Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.horizontal(|ui| {
                ui_kit::status_badge(ui, icon, color);
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(".gaanim files")
                                .size(16.0)
                                .color(palette::TEXT),
                        );
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new(badge).size(11.5).color(color));
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Open them with a double click, present them from the context menu \
                             and see their cover in the file manager. Only for your user.",
                        )
                        .size(12.5)
                        .color(palette::TEXT_MUTED),
                    );
                    ui.add_space(10.0);
                    clicked = ui_kit::secondary_button(
                        ui,
                        if associated {
                            "Remove the association"
                        } else {
                            "Associate .gaanim files"
                        },
                        true,
                    )
                    .clicked();
                });
            });
        });
    if !clicked {
        return;
    }
    let result = if associated {
        association::unregister()
    } else {
        std::env::current_exe()
            .map_err(|error| error.to_string())
            .and_then(|exe| association::register(&association::launcher_next_to(&exe)))
    };
    state.associated = Some(association::is_registered());
    state.notice = Some(match result {
        Ok(report) if report.warnings.is_empty() => Notice::Info(
            if associated {
                ".gaanim files no longer open with Gaanim."
            } else {
                "Done: .gaanim files open with Gaanim."
            }
            .into(),
        ),
        Ok(_) if associated => Notice::Info(".gaanim files no longer open with Gaanim.".into()),
        Ok(report)
            if report
                .warnings
                .iter()
                .any(|warning| warning.contains(association::THUMBNAIL_HANDLER_DLL)) =>
        {
            Notice::Info(format!(
                ".gaanim files open with Gaanim, but Explorer will not show their \
                 cover: {} is missing next to gaanim.exe. It comes in each release's \
                 .zip; copy it to that folder and associate again.",
                association::THUMBNAIL_HANDLER_DLL
            ))
        }
        Ok(_) => Notice::Info(
            "Done: .gaanim files open with Gaanim. If the file manager does not show it \
             yet, log out and back in."
                .into(),
        ),
        Err(error) => Notice::Error(format!("Could not change the association: {error}")),
    });
}

fn tool_card(
    ui: &mut Ui,
    name: &str,
    purpose: &str,
    tool_state: ToolState,
    detail: &str,
    help: Option<ToolHelp>,
    after: &[&str],
) {
    let (color, icon, badge) = match tool_state {
        ToolState::Ok => (palette::LOOP, Icon::Check, "Ready"),
        ToolState::Handled => (palette::ACCENT, Icon::Check, "Automatic"),
        ToolState::Missing => (palette::STOP, Icon::Warning, "Missing"),
        ToolState::Optional => (palette::TEXT_MUTED, Icon::Warning, "Optional"),
    };
    egui::Frame::new()
        .fill(palette::SURFACE)
        .stroke(Stroke::new(
            1.0,
            if tool_state == ToolState::Missing {
                color.gamma_multiply(0.4)
            } else {
                Color32::from_white_alpha(12)
            },
        ))
        .inner_margin(egui::Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.horizontal(|ui| {
                ui_kit::status_badge(ui, icon, color);
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(name).size(16.0).color(palette::TEXT));
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new(badge).size(11.5).color(color));
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(purpose)
                            .size(12.5)
                            .color(palette::TEXT_MUTED),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(detail)
                            .size(12.0)
                            .monospace()
                            .color(palette::TEXT_FAINT),
                    );
                });
            });
            if let Some(help) = help {
                ui.add_space(16.0);
                install_steps(ui, name, &help, after);
            }
        });
}

/// Numbered steps with the exact command, a copy button and a link to more
/// options.
fn install_steps(ui: &mut Ui, name: &str, help: &ToolHelp, after: &[&str]) {
    ui.label(caption("HOW TO INSTALL IT"));
    ui.add_space(10.0);
    let step = |ui: &mut Ui, number: usize, text: &str| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, palette::FIELD);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                number.to_string(),
                FontId::monospace(11.0),
                palette::TEXT_MUTED,
            );
            ui.add_space(10.0);
            ui.label(egui::RichText::new(text).size(13.0).color(palette::TEXT));
        });
        ui.add_space(8.0);
    };
    step(ui, 1, &format!("Open {}.", help.terminal));
    step(ui, 2, "Copy this command, paste it and press Enter:");
    egui::Frame::new()
        .fill(Color32::from_rgb(14, 14, 14))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(help.command)
                        .monospace()
                        .size(13.0)
                        .color(palette::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let copied_id = ui.id().with(("copied", name));
                    let copied_at: Option<f64> = ui.data(|data| data.get_temp(copied_id));
                    let now = ui.input(|input| input.time);
                    let recently = copied_at.is_some_and(|at| now - at < 1.6);
                    if ui_kit::secondary_button(ui, if recently { "Copied" } else { "Copy" }, true)
                        .clicked()
                    {
                        ui.ctx().copy_text(help.command.to_owned());
                        ui.data_mut(|data| data.insert_temp(copied_id, now));
                    }
                    if recently {
                        ui.ctx().request_repaint();
                    }
                });
            });
        });
    ui.add_space(8.0);
    for (index, text) in after.iter().enumerate() {
        step(ui, 3 + index, text);
    }
    if ui
        .add(
            egui::Label::new(
                egui::RichText::new(format!("Other ways to install {name} ↗"))
                    .size(12.0)
                    .color(palette::ACCENT),
            )
            .sense(Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        let _ = crate::platform::open_detached(help.more_url);
    }
}

// ---- Learn -----------------------------------------------------------------------------

fn learn_ui(ui: &mut Ui) {
    page_header(
        ui,
        "LEARN",
        "Learn Gaanim step by step",
        "The documentation opens in your browser.",
    );
    let links = [
        (
            Icon::Book,
            "Quick start",
            "Your first scene in five minutes.",
            "empezar/primera-animacion/",
        ),
        (
            Icon::Play,
            "Hands-on project",
            "From circle to sine wave in eight chapters.",
            "tutorial/antes-de-empezar/",
        ),
        (
            Icon::Grid,
            "Examples",
            "Scenes ready to copy and modify.",
            "ejemplos/",
        ),
        (
            Icon::Present,
            "Presentations",
            "Steps, notes and Presenter View.",
            "guias/presentaciones/",
        ),
        (
            Icon::Code,
            "API reference",
            "Every function, with animated examples.",
            "referencia/",
        ),
        (
            Icon::Export,
            "Installation and export",
            "Formats, FFmpeg and updates.",
            "empezar/instalacion/",
        ),
    ];
    let gap = 12.0;
    let columns = if ui.available_width() >= 640.0 { 2 } else { 1 };
    let card_w = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
    for row in links.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &(icon, title, detail, route) in row {
                if link_card(ui, icon, title, detail, card_w).clicked() {
                    let _ = crate::platform::open_detached(format!("{DOCS_URL}{route}"));
                }
            }
        });
        ui.add_space(gap);
    }

    ui.add_space(22.0);
    ui.label(caption("PREVIEW SHORTCUTS"));
    ui.add_space(12.0);
    egui::Frame::new()
        .fill(palette::SURFACE)
        .inner_margin(egui::Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (keys, action) in [
                ("Space", "Play or pause"),
                ("← →", "Previous or next scene"),
                ("Home  End", "Go to the start or the end"),
                ("L", "Repeat the current scene"),
                ("F11", "Full screen"),
                ("O", "Show composition guides"),
                ("Save the file", "Reloads the scene instantly"),
            ] {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(170.0, 28.0), Sense::hover());
                    let galley = ui.painter().layout_no_wrap(
                        keys.to_owned(),
                        FontId::monospace(12.0),
                        palette::TEXT,
                    );
                    let key_rect = Rect::from_min_size(
                        pos2(rect.min.x, rect.center().y - 11.0),
                        vec2(galley.size().x + 16.0, 22.0),
                    );
                    ui.painter().rect_filled(key_rect, 0.0, palette::FIELD);
                    ui.painter().galley(
                        key_rect.center() - galley.size() / 2.0,
                        galley,
                        palette::TEXT,
                    );
                    ui.label(
                        egui::RichText::new(action)
                            .size(13.0)
                            .color(palette::TEXT_MUTED),
                    );
                });
            }
        });
}

fn link_card(ui: &mut Ui, icon: Icon, title: &str, detail: &str, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 72.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hovered = response.hovered();
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        0.0,
        if hovered {
            Color32::from_rgb(30, 30, 30)
        } else {
            palette::SURFACE
        },
    );
    let tile = Rect::from_center_size(pos2(rect.min.x + 34.0, rect.center().y), Vec2::splat(38.0));
    painter.rect_filled(tile, 0.0, palette::ACCENT.gamma_multiply(0.14));
    ui_kit::paint_icon(
        painter,
        Rect::from_center_size(tile.center(), Vec2::splat(16.0)),
        icon,
        palette::ACCENT,
    );
    painter.text(
        pos2(tile.max.x + 14.0, rect.center().y - 10.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(14.5),
        palette::TEXT,
    );
    painter.text(
        pos2(tile.max.x + 14.0, rect.center().y + 11.0),
        Align2::LEFT_CENTER,
        detail,
        FontId::proportional(12.0),
        palette::TEXT_MUTED,
    );
    ui_kit::paint_icon(
        painter,
        Rect::from_center_size(pos2(rect.max.x - 24.0, rect.center().y), Vec2::splat(13.0)),
        Icon::ExternalLink,
        if hovered {
            palette::TEXT
        } else {
            palette::TEXT_FAINT
        },
    );
    response
}

// ---- Overlays --------------------------------------------------------------------------

fn preparation_modal(ctx: &egui::Context, state: &mut ProjectHubState) {
    let time = ctx.input(|input| input.time);
    let (name, root, created, started, opening) = match (&mut state.preparing, &state.opening) {
        (Some(preparation), _) => {
            let started = *preparation.started.get_or_insert(time);
            (
                preparation.project.manifest.name.clone(),
                preparation.project.root.clone(),
                preparation.created,
                started,
                false,
            )
        }
        (None, Some(project)) => (
            project.manifest.name.clone(),
            project.root.clone(),
            false,
            time,
            true,
        ),
        (None, None) => return,
    };
    egui::Modal::new(egui::Id::new("hub-preparation"))
        .frame(ui_kit::card_frame())
        .backdrop_color(Color32::from_black_alpha(170))
        .show(ctx, |ui| {
            ui.set_width(460.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.label(caption(if opening { "OPENING" } else { "PREPARING" }));
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("“{name}”"))
                    .size(20.0)
                    .color(palette::TEXT),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(display_path(&root))
                    .size(11.5)
                    .color(palette::TEXT_FAINT),
            );
            ui.add_space(22.0);
            let steps = [
                if created {
                    "Project created"
                } else {
                    "Project"
                },
                "Python and autocomplete",
                "Preview",
            ];
            let current = if opening { 2 } else { 1 };
            paint_steps(ui, &steps, current, time);
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new(if opening {
                    "Loading the scene…".to_owned()
                } else {
                    format!(
                        "uv is creating the project's Python environment and installing \
                         autocomplete. The first time can take a minute while it downloads \
                         Python {PYTHON_VERSION}."
                    )
                })
                .size(12.5)
                .color(palette::TEXT_MUTED),
            );
            if !opening {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if secondary_button(ui, "Show folder", true).clicked() {
                        let _ = crate::platform::open_detached(&root);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let elapsed = (time - started).max(0.0) as u64;
                        ui.label(
                            egui::RichText::new(format!("{}:{:02}", elapsed / 60, elapsed % 60))
                                .monospace()
                                .size(13.0)
                                .color(palette::TEXT_MUTED),
                        );
                    });
                });
            }
        });
}

/// Steps as chapters of a seek track: finished pieces filled, the current one
/// sweeping, keyframes between them and labels underneath.
fn paint_steps(ui: &mut Ui, steps: &[&str], current: usize, time: f64) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    let painter = ui.painter();
    let track = Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(rect.width(), 6.0));
    let count = steps.len() as f32;
    let sweep = ((time * 0.6).fract()) as f32;
    let playhead = (current as f32 + sweep.min(0.96)) / count;
    let cuts: Vec<f32> = (1..steps.len()).map(|i| i as f32 / count).collect();
    paint_seek_track(painter, track, &cuts, playhead, palette::ACCENT);
    for (index, label) in steps.iter().enumerate() {
        let center_x = track.min.x + track.width() * (index as f32 + 0.5) / count;
        let done = index < current;
        let color = if index <= current {
            palette::TEXT
        } else {
            palette::TEXT_FAINT
        };
        let text = if done {
            format!("✓ {label}")
        } else {
            (*label).to_owned()
        };
        painter.text(
            pos2(center_x, track.max.y + 18.0),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(12.0),
            color,
        );
    }
    paint_knob(
        painter,
        pos2(track.min.x + track.width() * playhead, track.center().y),
        6.0,
    );
}

fn paint_drop_overlay(ctx: &egui::Context, area: Rect) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hub-drop"),
    ));
    painter.rect_filled(area, 0.0, Color32::from_black_alpha(150));
    let zone = area.shrink(28.0);
    painter.rect_stroke(
        zone,
        0.0,
        Stroke::new(2.0, palette::ACCENT),
        egui::StrokeKind::Inside,
    );
    ui_kit::paint_icon(
        &painter,
        Rect::from_center_size(zone.center() - vec2(0.0, 24.0), Vec2::splat(40.0)),
        Icon::Folder,
        palette::ACCENT,
    );
    painter.text(
        zone.center() + vec2(0.0, 22.0),
        Align2::CENTER_CENTER,
        "Drop a project folder or a .gaanim file to open it",
        FontId::proportional(16.0),
        palette::TEXT,
    );
}

// ---- Shared painting -------------------------------------------------------------------

/// A seek track split into chapter pieces, filled up to `fill` (0..=1).
fn paint_seek_track(painter: &egui::Painter, track: Rect, cuts: &[f32], fill: f32, color: Color32) {
    let x_at = |f: f32| track.min.x + track.width() * f;
    let fill_x = x_at(fill.clamp(0.0, 1.0));
    let bounds: Vec<f32> = std::iter::once(0.0)
        .chain(cuts.iter().copied())
        .chain(std::iter::once(1.0))
        .collect();
    for piece in bounds.windows(2) {
        let x0 = x_at(piece[0]) + if piece[0] > 0.0 { PIECE_GAP / 2.0 } else { 0.0 };
        let x1 = x_at(piece[1]) - if piece[1] < 1.0 { PIECE_GAP / 2.0 } else { 0.0 };
        let piece_rect = Rect::from_min_max(pos2(x0, track.min.y), pos2(x1, track.max.y));
        painter.rect_filled(piece_rect, 0.0, Color32::from_white_alpha(26));
        if fill_x > x0 {
            let clip = Rect::from_min_max(piece_rect.min, pos2(fill_x.min(x1), piece_rect.max.y));
            painter
                .with_clip_rect(clip.intersect(painter.clip_rect()))
                .rect_filled(piece_rect, 0.0, color);
        }
    }
}

/// The square playhead knob of the playback bar, `half` points from its
/// center to each side.
fn paint_knob(painter: &egui::Painter, center: Pos2, half: f32) {
    let knob = Rect::from_center_size(center, Vec2::splat(half * 2.0));
    painter.rect_filled(
        knob.translate(vec2(0.0, 1.0)).expand(1.0),
        0.0,
        Color32::from_black_alpha(90),
    );
    painter.rect_filled(knob, 0.0, Color32::WHITE);
}

/// The Gaanim mark at `unit` points per grid pixel: the rounded tile, then
/// its pixel frames. A mesh keeps neighbouring pixels seamless (shapes would
/// anti-alias every edge).
fn paint_pixel_mark(painter: &egui::Painter, origin: Pos2, unit: f32) {
    let [r, g, b, a] = ICON_PALETTE[0];
    painter.rect_filled(
        Rect::from_min_size(origin, Vec2::splat(ICON_GRID as f32 * unit)),
        ICON_TILE_RADIUS * unit,
        Color32::from_rgba_unmultiplied(r, g, b, a),
    );
    let mut mesh = egui::Mesh::default();
    for (y, row) in ICON_PIXELS.iter().enumerate() {
        for (x, &pixel) in row.iter().enumerate() {
            let index = usize::from(pixel - b'0');
            // 0 is transparent and 1 the tile, drawn above.
            if index <= 1 {
                continue;
            }
            let [r, g, b, a] = ICON_PALETTE[index - 1];
            mesh.add_colored_rect(
                Rect::from_min_size(
                    origin + vec2(x as f32 * unit, y as f32 * unit),
                    Vec2::splat(unit),
                ),
                Color32::from_rgba_unmultiplied(r, g, b, a),
            );
        }
    }
    debug_assert_eq!(ICON_PIXELS.len(), ICON_GRID);
    painter.add(mesh);
}

/// A faint unit grid behind the content, fading towards the bottom.
fn paint_backdrop_grid(painter: &egui::Painter, area: Rect) {
    let step = 48.0;
    let mut y = area.min.y + step;
    while y < area.max.y {
        let fade = 1.0 - ((y - area.min.y) / area.height()).clamp(0.0, 1.0);
        let color = Color32::from_white_alpha((7.0 * fade) as u8);
        painter.line_segment(
            [pos2(area.min.x, y), pos2(area.max.x, y)],
            Stroke::new(1.0, color),
        );
        y += step;
    }
    let mut x = area.min.x + step;
    while x < area.max.x {
        painter.line_segment(
            [
                pos2(x, area.min.y),
                pos2(x, area.min.y + area.height() * 0.7),
            ],
            Stroke::new(1.0, Color32::from_white_alpha(5)),
        );
        x += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_names_the_tool_to_install() {
        assert_eq!(
            notice_fix("Could not open the project: uv was not found"),
            Some(NoticeFix::Uv)
        );
        assert_eq!(
            notice_fix("ffprobe failed: no such file"),
            Some(NoticeFix::Ffmpeg)
        );
        assert_eq!(
            notice_fix("Python 3.12 is required"),
            Some(NoticeFix::Python)
        );
        // "uv" inside another word is not uv.
        assert_eq!(notice_fix("Could not move the file"), None);
    }

    #[test]
    fn empty_folders_become_the_project() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            new_project_target(dir.path(), ProjectKind::Video),
            NewProjectTarget::Create(dir.path().to_path_buf())
        );
    }

    #[test]
    fn busy_folders_get_a_free_subfolder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("new-video")).unwrap();
        assert_eq!(
            new_project_target(dir.path(), ProjectKind::Video),
            NewProjectTarget::Create(dir.path().join("new-video-2"))
        );
        assert_eq!(
            new_project_target(dir.path(), ProjectKind::Slides),
            NewProjectTarget::Create(dir.path().join("new-presentation"))
        );
    }

    #[test]
    fn existing_projects_open_instead_of_nesting() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("gaanim.toml"), "").unwrap();
        assert_eq!(
            new_project_target(dir.path(), ProjectKind::Video),
            NewProjectTarget::Existing(dir.path().to_path_buf())
        );
    }

    #[test]
    fn paths_drop_the_windows_verbatim_prefix() {
        assert_eq!(
            display_path(Path::new(r"\\?\C:\Users\demo")),
            r"C:\Users\demo"
        );
        assert_eq!(display_path(Path::new("/home/demo")), "/home/demo");
    }

    #[test]
    fn ages_read_naturally() {
        let minutes = |m: u64| Duration::from_secs(m * 60);
        assert_eq!(relative_age(Duration::from_secs(20)), "just now");
        assert_eq!(relative_age(minutes(5)), "5 min ago");
        assert_eq!(relative_age(minutes(180)), "3 h ago");
        assert_eq!(relative_age(minutes(60 * 30)), "yesterday");
        assert_eq!(relative_age(minutes(60 * 24 * 3)), "3 days ago");
        assert_eq!(relative_age(minutes(60 * 24 * 14)), "2 wk ago");
        assert_eq!(relative_age(minutes(60 * 24 * 90)), "3 months ago");
    }

    #[test]
    fn install_help_names_a_command_for_this_platform() {
        for help in [uv_help(), ffmpeg_help()] {
            assert!(!help.command.is_empty());
            assert!(help.more_url.starts_with("https://"));
        }
        assert!(uv_help().command.contains("uv"));
        assert!(ffmpeg_help().command.to_lowercase().contains("ffmpeg"));
    }

    /// A workspace in a temporary folder: `shows/pilot` (valid),
    /// `shows/broken` (an invalid manifest) and `extras/trailer`.
    fn workspace() -> (tempfile::TempDir, Workspace) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let write = |relative: &str, text: &str| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "gaanim.toml",
            "name = \"canal\"\n[workspace]\nmembers = [{ path = \"shows/*\", group = \"Shows\" }, \"extras/*\"]\n",
        );
        for member in ["shows/pilot", "extras/trailer"] {
            write(&format!("{member}/main.py"), "");
            write(
                &format!("{member}/gaanim.toml"),
                "kind = \"video\"\nentry = \"main.py\"\n",
            );
        }
        write(
            "shows/broken/gaanim.toml",
            "kind = \"film\"\nentry = \"main.py\"\n",
        );
        let workspace = gaanim_project::resolve_workspace(root).unwrap();
        (temp, workspace)
    }

    #[test]
    fn members_are_listed_by_their_own_name_and_an_invalid_one_says_why() {
        let (_temp, workspace) = workspace();
        let page = WorkspacePage::new(workspace);
        let rows: Vec<RecentRow> = page.members.iter().map(RecentRow::member).collect();
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["broken", "pilot", "trailer"]);
        assert!(rows.iter().all(|row| row.member));
        assert!(
            rows[0]
                .error
                .as_deref()
                .is_some_and(|error| error.contains("film"))
        );
        assert_eq!(rows[0].label, "Invalid");
        assert!(rows[1].error.is_none());
        // Members keep the folder button; pinning and removing are for recents.
        assert_eq!(rows[1].action_count(), 1);
        let groups: Vec<&str> = page
            .members
            .iter()
            .map(|member| member.group.as_str())
            .collect();
        assert_eq!(groups, ["Shows", "Shows", "extras"]);
        // In recents, a member carries its workspace's name.
        let recent = RecentRow::of(&RecentGroup {
            entry: RecentEntry::Project(page.members[1].project.clone().unwrap()),
            bundle: None,
            pinned: false,
        });
        assert_eq!(recent.name, "canal › pilot");
        assert_eq!(recent.action_count(), 3);
    }

    #[test]
    fn a_member_check_is_read_from_its_json_report() {
        let verdict = |result: &str, errors: usize, warnings: usize| {
            format!(
                "{{\"type\":\"check\",\"result\":\"{result}\",\"errors\":{errors},\"warnings\":{warnings}}}"
            )
        };
        assert_eq!(
            CheckOutcome::read(Some(0), &format!("noise\n{}", verdict("pass", 0, 2))),
            CheckOutcome::Passed { warnings: 2 }
        );
        assert_eq!(
            CheckOutcome::read(Some(1), &verdict("fail", 3, 1)),
            CheckOutcome::Failed {
                errors: 3,
                warnings: 1
            }
        );
        let error = "{\"type\":\"message\",\"level\":\"error\",\"label\":\"check\",\"message\":\"could not load project: boom\"}";
        assert_eq!(
            CheckOutcome::read(Some(2), error),
            CheckOutcome::NotLoaded("could not load project: boom".into())
        );
        assert_eq!(
            CheckOutcome::read(None, ""),
            CheckOutcome::NotLoaded("gaanim check did not finish".into())
        );
        assert_eq!(CheckOutcome::Passed { warnings: 0 }.label().0, "Passed");
        assert_eq!(
            CheckOutcome::Failed {
                errors: 1,
                warnings: 0
            }
            .label()
            .0,
            "Failed · 1 error"
        );
    }

    #[test]
    fn a_workspace_cover_is_its_image_or_a_mosaic_of_its_members() {
        let tile = |shade: u8| {
            image::RgbaImage::from_pixel(160, 90, image::Rgba([shade, shade, shade, 255]))
        };
        assert!(mosaic(&[]).is_none());
        let one = mosaic(&[tile(200)]).unwrap();
        assert_eq!(one.dimensions(), (COVER_EDGE, COVER_EDGE * 9 / 16));
        assert_eq!(one.get_pixel(COVER_EDGE / 2, 100)[0], 200);
        let three = mosaic(&[tile(10), tile(20), tile(30)]).unwrap();
        let (width, height) = three.dimensions();
        assert_eq!(three.get_pixel(width / 4, height / 4)[0], 10);
        assert_eq!(three.get_pixel(3 * width / 4, height / 4)[0], 20);
        assert_eq!(three.get_pixel(width / 4, 3 * height / 4)[0], 30);
        // The fourth cell stays empty, as does the gap between cells.
        assert_eq!(three.get_pixel(3 * width / 4, 3 * height / 4)[3], 0);
        assert_eq!(three.get_pixel(width / 2, height / 4)[3], 0);

        // An image at the workspace's root wins, scaled down to a cover.
        let (temp, _) = workspace();
        let png = gaanim_thumbnail::encode_png(&image::RgbaImage::from_pixel(
            1920,
            1080,
            image::Rgba([1, 2, 3, 255]),
        ))
        .unwrap();
        std::fs::write(temp.path().join("cover.png"), png).unwrap();
        let workspace = gaanim_project::resolve_workspace(temp.path()).unwrap();
        let cover = workspace_cover(&workspace).unwrap();
        assert_eq!(cover.dimensions(), (COVER_EDGE, COVER_EDGE * 9 / 16));
        assert_eq!(cover.get_pixel(0, 0).0, [1, 2, 3, 255]);
    }
}
