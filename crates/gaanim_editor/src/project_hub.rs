//! Home: the workspace people see when they run `gaanim` without a script.
//!
//! Nothing asks for typed paths or environment knowledge. New projects go in a
//! folder chosen with the system picker, Python is prepared in the background
//! with uv, recent projects are one click away, and anything missing from the
//! computer (uv, Python, FFmpeg) is explained in plain words with the exact
//! command to install it. The look follows the playback bar: dark panels,
//! painted icons, seek tracks with a playhead, stops and keyframes, all with
//! square corners like the pixel mark.

use crate::app_icon::{ICON_GRID, ICON_PALETTE, ICON_PIXELS};
use crate::ui_kit::{
    self, ButtonTone, Icon, caption, icon_button, palette, primary_button, secondary_button,
};
use bevy::prelude::*;
use bevy::window::FileDragAndDrop;
use bevy_egui::{EguiPrimaryContextPass, egui};
use crossbeam_channel::{Receiver, TryRecvError};
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use gaanim_project::{
    CreateProjectOptions, EnvironmentProbe, ProjectKind, PythonSource, RecentProjects,
    ResolvedProject, create_project, default_project_parent, find_project_for_script,
    resolve_project,
};
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
            .add_systems(Update, hub_file_drop_system)
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
        }
    }
}

impl ProjectHubState {
    pub fn show(&mut self) {
        self.active = true;
        self.check_tools();
    }

    pub fn report_open_error(&mut self, error: String) {
        self.active = true;
        self.opening = None;
        self.preparing = None;
        self.notice = Some(Notice::Error(format!(
            "No se pudo abrir el proyecto: {error}"
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
                        Some(Err("la preparación terminó sin respuesta".into()))
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
                        "No se pudo preparar el entorno de Python: {error}"
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
                "Para abrir «{}» Gaanim necesita uv. Instálalo con el comando de abajo; el \
                 proyecto se abrirá solo en cuanto lo detecte.",
                project.manifest.name
            )));
            self.waiting = Some(project);
            self.section = Section::Environment;
        }
    }

    fn start_preparation(&mut self, project: ResolvedProject, created: bool) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let root = project.root.clone();
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
                    "No se pudo iniciar la preparación: {error}"
                )))
            }
        }
    }

    fn busy(&self) -> bool {
        self.preparing.is_some() || self.opening.is_some()
    }

    /// Ask for a folder with the native picker and create the project there.
    fn create_with_picker(&mut self, kind: ProjectKind) {
        let title = match kind {
            ProjectKind::Video => "Elige o crea una carpeta para el nuevo video",
            ProjectKind::Slides => "Elige o crea una carpeta para la nueva presentación",
        };
        let Some(picked) = rfd::FileDialog::new()
            .set_title(title)
            .set_directory(default_project_parent())
            .pick_folder()
        else {
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
                        "No se pudo crear el proyecto: {error}"
                    )))
                }
            },
        }
    }

    fn open_with_picker(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Abrir un proyecto de Gaanim")
            .set_directory(default_project_parent())
            .pick_folder()
        {
            self.open_path(&path);
        }
    }

    /// Open a dropped or picked path: a project folder or any file inside one.
    fn open_path(&mut self, path: &Path) {
        let project = if path.is_dir() {
            resolve_project(path).ok()
        } else {
            find_project_for_script(path)
        };
        match project {
            Some(project) => self.open_project(project, false),
            None => {
                self.notice = Some(Notice::Error(format!(
                    "«{}» no es un proyecto de Gaanim (no tiene gaanim.toml). Usa «Nuevo \
                     video» o «Nueva presentación» para crear uno.",
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
    if picked.join("gaanim.toml").is_file() {
        return NewProjectTarget::Existing(picked.to_path_buf());
    }
    let empty = std::fs::read_dir(picked).is_ok_and(|mut entries| entries.next().is_none());
    if empty {
        return NewProjectTarget::Create(picked.to_path_buf());
    }
    let base = match kind {
        ProjectKind::Video => "nuevo-video",
        ProjectKind::Slides => "nueva-presentacion",
    };
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
        ProjectKind::Slides => "Presentación",
    }
}

fn kind_color(kind: ProjectKind) -> Color32 {
    match kind {
        ProjectKind::Video => palette::ACCENT,
        ProjectKind::Slides => palette::STOP,
    }
}

/// "hace 3 h": how long ago a project was last edited.
fn relative_age(elapsed: Duration) -> String {
    let minutes = elapsed.as_secs() / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    match () {
        _ if minutes < 1 => "ahora mismo".into(),
        _ if hours < 1 => format!("hace {minutes} min"),
        _ if days < 1 => format!("hace {hours} h"),
        _ if days == 1 => "ayer".into(),
        _ if days < 7 => format!("hace {days} días"),
        _ if days < 30 => format!("hace {} sem.", days / 7),
        _ if days < 60 => "hace 1 mes".into(),
        _ if days < 365 => format!("hace {} meses", days / 30),
        _ => "hace más de un año".into(),
    }
}

fn edited_ago(project: &ResolvedProject) -> Option<String> {
    let modified = std::fs::metadata(&project.entry).ok()?.modified().ok()?;
    let elapsed = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    Some(format!("Editado {}", relative_age(elapsed)))
}

/// Paths as people write them, without Windows' `\\?\` verbatim prefix.
fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => rest.to_owned(),
        None => text,
    }
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
    "PowerShell (búscalo en el menú Inicio)"
} else {
    "una terminal"
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

fn project_hub_ui_system(
    mut contexts: bevy_egui::EguiContexts,
    mut state: ResMut<ProjectHubState>,
    mut pending: ResMut<PendingProjectOpen>,
) {
    if !state.active {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let ctx = ctx.clone();
    let state = &mut *state;
    state.poll_workers(&mut pending);

    // Re-check the tools when the window comes back into focus: people install
    // uv or FFmpeg in a terminal and return here.
    let focused = ctx.input(|input| input.focused);
    if focused && !state.was_focused {
        state.check_tools();
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
    root.painter().rect_filled(screen, 0.0, palette::INK);
    let sidebar = Rect::from_min_max(screen.min, pos2(screen.min.x + SIDEBAR_W, screen.max.y));
    let content = Rect::from_min_max(pos2(sidebar.max.x, screen.min.y), screen.max);
    paint_backdrop_grid(root.painter(), content);
    root.scope_builder(egui::UiBuilder::new().max_rect(sidebar), |ui| {
        sidebar_ui(ui, state)
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

fn handle_shortcuts(ctx: &egui::Context, state: &mut ProjectHubState) {
    use egui::{Key, KeyboardShortcut, Modifiers};
    let (slides, video, open) = ctx.input_mut(|input| {
        (
            input.consume_shortcut(&KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                Key::N,
            )),
            input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::N)),
            input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::O)),
        )
    });
    if slides {
        state.create_with_picker(ProjectKind::Slides);
    } else if video {
        state.create_with_picker(ProjectKind::Video);
    } else if open {
        state.open_with_picker();
    }
}

// ---- Sidebar ---------------------------------------------------------------------------

fn sidebar_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, palette::SURFACE);
    painter.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_white_alpha(10)),
    );

    let inner = rect.shrink2(vec2(16.0, 22.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        // Brand: the pixel mark at 2 px per unit, then the name.
        let (brand, _) = ui.allocate_exact_size(vec2(inner.width(), 40.0), Sense::hover());
        let mark = Rect::from_min_size(
            pos2(brand.min.x + 4.0, brand.center().y - 16.0).round(),
            Vec2::splat(32.0),
        );
        paint_pixel_mark(ui.painter(), mark.min, 2.0);
        ui.painter().text(
            pos2(mark.max.x + 12.0, brand.center().y - 1.0),
            Align2::LEFT_CENTER,
            "gaanim",
            FontId::proportional(22.0),
            palette::TEXT,
        );
        ui.painter().text(
            pos2(brand.max.x, brand.center().y),
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
            (Section::Projects, Icon::Grid, "Proyectos"),
            (Section::Environment, Icon::Package, "Entorno"),
            (Section::Learn, Icon::Book, "Aprender"),
        ] {
            let dot = (section == Section::Environment)
                .then_some(status_dot)
                .flatten();
            if nav_item(ui, icon, label, state.section == section, dot).clicked() {
                state.section = section;
            }
            ui.add_space(2.0);
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
        (None, _) => (
            palette::TEXT_FAINT,
            "Comprobando…".to_string(),
            String::new(),
        ),
        (Some(tools), _) => {
            let python =
                tools.probe.python.as_ref().map(|python| {
                    format!("Python {}.{}", python.version.major, python.version.minor)
                });
            match tools.readiness() {
                Readiness::Ready => (
                    palette::LOOP,
                    "Todo listo".to_string(),
                    match python {
                        Some(python) => format!("{python} · uv"),
                        None => "uv preparará Python".to_string(),
                    },
                ),
                Readiness::WithoutUv => (
                    palette::STOP,
                    "Casi listo".to_string(),
                    "Instala uv para entornos aislados".to_string(),
                ),
                Readiness::NeedsUv => (
                    palette::STOP,
                    "Falta instalar uv".to_string(),
                    "Toca aquí para ver cómo".to_string(),
                ),
            }
        }
    };
    let left = rect.min.x + 14.0;
    painter.text(
        pos2(left, rect.min.y + 16.0),
        Align2::LEFT_CENTER,
        "ENTORNO",
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

fn notice_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    let Some(notice) = &state.notice else {
        return;
    };
    let (color, icon, text) = match notice {
        Notice::Error(text) => (palette::DANGER, Icon::Warning, text.clone()),
        Notice::Info(text) => (palette::ACCENT, Icon::Check, text.clone()),
    };
    let mut dismiss = false;
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
                    ui.label(egui::RichText::new(text).size(13.0).color(palette::TEXT));
                });
                if icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                    .on_hover_text("Cerrar")
                    .clicked()
                {
                    dismiss = true;
                }
            });
        });
    if dismiss {
        state.notice = None;
    }
    ui.add_space(20.0);
}

// ---- Projects --------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Start {
    Video,
    Slides,
    Open,
}

fn projects_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    page_header(
        ui,
        "PROYECTOS",
        "¿Qué quieres animar hoy?",
        "Crea un proyecto o abre uno reciente. Gaanim prepara Python por ti.",
    );

    let busy = state.busy();
    let gap = 16.0;
    let columns = if ui.available_width() >= 700.0 { 3 } else { 1 };
    let card_w = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
    let starts = [
        (
            Start::Video,
            "Nuevo video",
            "Animación 16:9 lista para exportar a MP4, WebM o GIF.",
            "Ctrl N",
        ),
        (
            Start::Slides,
            "Nueva presentación",
            "Diapositivas con pasos, notas y modo presentador.",
            "Ctrl Shift N",
        ),
        (
            Start::Open,
            "Abrir proyecto",
            "Elige su carpeta o arrástrala a esta ventana.",
            "Ctrl O",
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
        None => {}
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
    let painter = ui.painter();
    let accent = match start {
        Start::Video => palette::ACCENT,
        Start::Slides => palette::STOP,
        Start::Open => palette::TEXT,
    };
    if hovered {
        painter.rect_filled(
            rect.translate(vec2(0.0, 6.0)),
            0.0,
            Color32::from_black_alpha(60),
        );
    }
    painter.rect_filled(
        rect,
        0.0,
        if hovered {
            Color32::from_rgb(27, 28, 36)
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
    painter.rect_filled(stage, 0.0, Color32::from_rgb(11, 12, 16));
    let time = ui.input(|input| input.time);
    match start {
        Start::Video => paint_video_scene(painter, stage, hovered, time),
        Start::Slides => paint_slides_scene(painter, stage, hovered, time),
        Start::Open => paint_open_scene(painter, stage, hovered),
    }

    let text_left = rect.min.x + 18.0;
    painter.text(
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
    painter.text(
        pos2(rect.max.x - 16.0, stage.max.y + 22.0),
        Align2::RIGHT_CENTER,
        shortcut,
        FontId::monospace(10.5),
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

fn recents_ui(ui: &mut Ui, state: &mut ProjectHubState) {
    let projects = state.recents.projects();
    ui.horizontal(|ui| {
        ui.label(caption("RECIENTES"));
        if !projects.is_empty() {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(projects.len().to_string())
                    .monospace()
                    .size(11.0)
                    .color(palette::TEXT_FAINT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new("Limpiar lista")
                                .size(12.0)
                                .color(palette::TEXT_MUTED),
                        )
                        .sense(Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    state.recents.clear();
                    let _ = state.recents.save();
                }
            });
        }
    });
    ui.add_space(12.0);

    if projects.is_empty() {
        empty_recents(ui, state);
        return;
    }
    let busy = state.busy();
    let mut action = None;
    for project in &projects {
        if let Some(chosen) = recent_row(ui, project, !busy) {
            action = Some((chosen, project.clone()));
        }
        ui.add_space(8.0);
    }
    match action {
        Some((RowAction::Open, project)) => state.open_project(project, false),
        Some((RowAction::ShowFolder, project)) => {
            let _ = open::that_detached(&project.root);
        }
        Some((RowAction::Remove, project)) => {
            state.recents.remove(&project.root);
            let _ = state.recents.save();
        }
        None => {}
    }
}

enum RowAction {
    Open,
    ShowFolder,
    Remove,
}

fn recent_row(ui: &mut Ui, project: &ResolvedProject, enabled: bool) -> Option<RowAction> {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), 66.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && ui.rect_contains_pointer(rect);
    let kind = project.manifest.kind;
    let color = kind_color(kind);
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        0.0,
        if hovered {
            Color32::from_rgb(27, 28, 36)
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
        match kind {
            ProjectKind::Video => Icon::Play,
            ProjectKind::Slides => Icon::Present,
        },
        color,
    );

    let text_left = tile.max.x + 14.0;
    let actions_w = if hovered { 200.0 } else { 150.0 };
    let text_w = (rect.max.x - actions_w - text_left).max(80.0);
    let name = painter.layout_no_wrap(
        project.manifest.name.clone(),
        FontId::proportional(15.0),
        palette::TEXT,
    );
    painter.galley(pos2(text_left, rect.center().y - 19.0), name, palette::TEXT);
    let path = painter.layout(
        display_path(&project.root),
        FontId::proportional(11.5),
        palette::TEXT_FAINT,
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
                if primary_button(ui, "Abrir", Some(Icon::Play), true).clicked() {
                    action = Some(RowAction::Open);
                }
                ui.add_space(6.0);
                if icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                    .on_hover_text("Quitar de la lista (no borra archivos)")
                    .clicked()
                {
                    action = Some(RowAction::Remove);
                }
                if icon_button(ui, Icon::Folder, ButtonTone::Ghost, true)
                    .on_hover_text("Mostrar la carpeta")
                    .clicked()
                {
                    action = Some(RowAction::ShowFolder);
                }
            },
        );
    } else {
        let meta_x = rect.max.x - 18.0;
        painter.text(
            pos2(meta_x, rect.center().y - 9.0),
            Align2::RIGHT_CENTER,
            kind_label(kind),
            FontId::proportional(12.0),
            color,
        );
        if let Some(age) = edited_ago(project) {
            painter.text(
                pos2(meta_x, rect.center().y + 10.0),
                Align2::RIGHT_CENTER,
                age,
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
        "Tus proyectos aparecerán aquí",
        FontId::proportional(15.0),
        palette::TEXT,
    );
    painter.text(
        pos2(rect.center().x, rect.min.y + 120.0),
        Align2::CENTER_CENTER,
        "Crea tu primer video: elige una carpeta y Gaanim prepara todo lo demás.",
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
            |ui| primary_button(ui, "Nuevo video", Some(Icon::Plus), !state.busy()).clicked(),
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
        "ENTORNO",
        "Lo que Gaanim necesita en tu equipo",
        "Cada proyecto tiene su propio Python aislado, que Gaanim prepara solo. Para eso \
         necesita uv; FFmpeg es opcional.",
    );

    ui.horizontal(|ui| {
        let checking = state.checking.is_some();
        if primary_button(
            ui,
            if checking {
                "Comprobando…"
            } else {
                "Comprobar de nuevo"
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
                    "«{}» se abrirá en cuanto el entorno esté listo.",
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
            egui::RichText::new("Buscando Python, uv y FFmpeg…")
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
        Some(uv) => format!("Instalado · {}", uv.version),
        None => "No instalado · necesario para crear y preparar proyectos".into(),
    };
    tool_card(
        ui,
        "uv",
        "Descarga Python y crea el entorno de cada proyecto. Lo usas una vez; Gaanim hace el resto.",
        uv_state,
        &uv_detail,
        (uv_state == ToolState::Missing).then(uv_help),
        &[
            "Cuando termine, vuelve a esta ventana: Gaanim lo detecta solo.",
            "Si no aparece, pulsa «Comprobar de nuevo».",
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
                    PythonSource::ActiveVenv => "entorno activo",
                    PythonSource::ProjectVenv => "entorno del proyecto",
                    PythonSource::System => "sistema",
                }
            ),
        ),
        Some(python) if tools.probe.uv.is_some() => (
            ToolState::Handled,
            format!(
                "Tienes Python {}.{}; uv descargará Python 3.14 para tus proyectos",
                python.version.major, python.version.minor
            ),
        ),
        None if tools.probe.uv.is_some() => (
            ToolState::Handled,
            "uv lo descargará al crear tu primer proyecto".into(),
        ),
        Some(python) => (
            ToolState::Missing,
            format!(
                "Python {}.{} no es compatible · instala uv y él traerá Python 3.14",
                python.version.major, python.version.minor
            ),
        ),
        None => (
            ToolState::Missing,
            "No encontrado · instala uv y él traerá Python 3.14".into(),
        ),
    };
    tool_card(
        ui,
        "Python 3.14",
        "El lenguaje en el que escribes las escenas. No tienes que instalarlo a mano.",
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
        "Opcional. Exporta a MP4 y WebM y permite usar video y audio en las escenas. Sin él \
         puedes exportar WebP, GIF y PNG.",
        ffmpeg_state,
        if tools.ffmpeg {
            "Instalado"
        } else {
            "No instalado"
        },
        (!tools.ffmpeg).then(ffmpeg_help),
        &["Cuando termine, cierra y vuelve a abrir Gaanim si al exportar sigue sin encontrarlo."],
    );
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
        ToolState::Ok => (palette::LOOP, Icon::Check, "Listo"),
        ToolState::Handled => (palette::ACCENT, Icon::Check, "Automático"),
        ToolState::Missing => (palette::STOP, Icon::Warning, "Falta"),
        ToolState::Optional => (palette::TEXT_MUTED, Icon::Warning, "Opcional"),
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
    ui.label(caption("CÓMO INSTALARLO"));
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
    step(ui, 1, &format!("Abre {}.", help.terminal));
    step(ui, 2, "Copia este comando, pégalo y pulsa Enter:");
    egui::Frame::new()
        .fill(Color32::from_rgb(11, 12, 16))
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
                    if ui_kit::secondary_button(
                        ui,
                        if recently { "Copiado" } else { "Copiar" },
                        true,
                    )
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
                egui::RichText::new(format!("Otras formas de instalar {name} ↗"))
                    .size(12.0)
                    .color(palette::ACCENT),
            )
            .sense(Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        let _ = open::that_detached(help.more_url);
    }
}

// ---- Learn -----------------------------------------------------------------------------

fn learn_ui(ui: &mut Ui) {
    page_header(
        ui,
        "APRENDER",
        "Aprende Gaanim paso a paso",
        "La documentación se abre en tu navegador.",
    );
    let links = [
        (
            Icon::Book,
            "Guía rápida",
            "Tu primera escena en cinco minutos.",
            "manual/guia-rapida/",
        ),
        (
            Icon::Play,
            "Proyecto práctico",
            "Del círculo al seno en ocho capítulos.",
            "guia/antes-de-empezar/",
        ),
        (
            Icon::Grid,
            "Ejemplos",
            "Escenas listas para copiar y modificar.",
            "examples/basic/",
        ),
        (
            Icon::Present,
            "Presentaciones",
            "Pasos, notas y modo presentador.",
            "guides/slides/",
        ),
        (
            Icon::Code,
            "Referencia de la API",
            "Cada función, con ejemplos animados.",
            "api/",
        ),
        (
            Icon::Export,
            "Instalación y exportación",
            "Formatos, FFmpeg y actualizaciones.",
            "getting-started/installation/",
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
                    let _ = open::that_detached(format!("{DOCS_URL}{route}"));
                }
            }
        });
        ui.add_space(gap);
    }

    ui.add_space(22.0);
    ui.label(caption("ATAJOS EN LA VISTA PREVIA"));
    ui.add_space(12.0);
    egui::Frame::new()
        .fill(palette::SURFACE)
        .inner_margin(egui::Margin::symmetric(18, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (keys, action) in [
                ("Espacio", "Reproducir o pausar"),
                ("← →", "Escena anterior o siguiente"),
                ("Inicio  Fin", "Ir al principio o al final"),
                ("L", "Repetir la escena actual"),
                ("F11", "Pantalla completa"),
                ("O", "Mostrar guías de composición"),
                ("Guardar el archivo", "Recarga la escena al instante"),
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
            Color32::from_rgb(27, 28, 36)
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
            ui.label(caption(if opening { "ABRIENDO" } else { "PREPARANDO" }));
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("«{name}»"))
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
                    "Proyecto creado"
                } else {
                    "Proyecto"
                },
                "Python y autocompletado",
                "Vista previa",
            ];
            let current = if opening { 2 } else { 1 };
            paint_steps(ui, &steps, current, time);
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new(if opening {
                    "Cargando la escena…"
                } else {
                    "uv está creando el entorno de Python del proyecto e instalando el \
                     autocompletado. La primera vez puede tardar un minuto mientras descarga \
                     Python 3.14."
                })
                .size(12.5)
                .color(palette::TEXT_MUTED),
            );
            if !opening {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if secondary_button(ui, "Mostrar carpeta", true).clicked() {
                        let _ = open::that_detached(&root);
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
        "Suelta la carpeta del proyecto para abrirla",
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

/// The Gaanim pixel mark at `unit` points per grid pixel. A mesh keeps
/// neighbouring pixels seamless (shapes would anti-alias every edge).
fn paint_pixel_mark(painter: &egui::Painter, origin: Pos2, unit: f32) {
    let mut mesh = egui::Mesh::default();
    for (y, row) in ICON_PIXELS.iter().enumerate() {
        for (x, &pixel) in row.iter().enumerate() {
            let index = usize::from(pixel - b'0');
            if index == 0 {
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
        std::fs::create_dir(dir.path().join("nuevo-video")).unwrap();
        assert_eq!(
            new_project_target(dir.path(), ProjectKind::Video),
            NewProjectTarget::Create(dir.path().join("nuevo-video-2"))
        );
        assert_eq!(
            new_project_target(dir.path(), ProjectKind::Slides),
            NewProjectTarget::Create(dir.path().join("nueva-presentacion"))
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
        assert_eq!(relative_age(Duration::from_secs(20)), "ahora mismo");
        assert_eq!(relative_age(minutes(5)), "hace 5 min");
        assert_eq!(relative_age(minutes(180)), "hace 3 h");
        assert_eq!(relative_age(minutes(60 * 30)), "ayer");
        assert_eq!(relative_age(minutes(60 * 24 * 3)), "hace 3 días");
        assert_eq!(relative_age(minutes(60 * 24 * 14)), "hace 2 sem.");
        assert_eq!(relative_age(minutes(60 * 24 * 90)), "hace 3 meses");
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
}
