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
use gaanim_api::host::ReloadPayload;
use gaanim_core::console;
use gaanim_export::encoder::VideoEncoder;
use pyo3::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc;

mod file_watcher;
mod hot_reload;
mod python_home;
mod runtime_benchmark;
mod script_runner;

use hot_reload::{
    ReloadReceiver, ReloadStatus, ScriptError, ScriptErrorReceiver, reload_listener_system,
    reload_status_overlay_system, script_error_listener_system, script_error_overlay_system,
};

fn main() {
    if runtime_benchmark::dispatch_reload_benchmark_mode() {
        return;
    }
    if dispatch_python_api_validation_mode() {
        return;
    }
    if dispatch_export_worker_mode() {
        return;
    }
    if dispatch_export_mode() {
        return;
    }
    if dispatch_init_mode() {
        return;
    }
    if dispatch_check_mode() {
        return;
    }
    if dispatch_diff_mode() {
        return;
    }

    let launch = parse_args();
    #[cfg(target_os = "linux")]
    gaanim_editor::alsa_errors::route_alsa_errors();
    console::banner(if launch.present {
        "Presentation"
    } else {
        "GPU-accelerated vector animation engine"
    });
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: if launch.present {
                        "Gaanim — Presentation".to_string()
                    } else {
                        "Gaanim".to_string()
                    },
                    resolution: (1280, 720).into(),
                    present_mode: bevy::window::PresentMode::AutoVsync,
                    mode: if launch.present {
                        bevy::window::WindowMode::BorderlessFullscreen(
                            launch
                                .monitor
                                .map(bevy::window::MonitorSelection::Index)
                                .unwrap_or(bevy::window::MonitorSelection::Primary),
                        )
                    } else {
                        bevy::window::WindowMode::Windowed
                    },
                    ..default()
                }),
                ..default()
            })
            .set(gaanim_scene::gaanim_asset_plugin())
            .set(gaanim_scene::logging::log_plugin()),
    )
    .add_plugins(gaanim_scene::GaanimScenePlugin)
    .add_plugins(gaanim_animation::GaanimAnimationPlugin)
    .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
    .add_plugins(gaanim_media::GaanimMediaPlugin)
    .add_plugins(gaanim_text::GaanimTextPlugin)
    .add_plugins(gaanim_api::GaanimApiPlugin)
    .add_plugins(gaanim_renderer::GaanimRendererPlugin)
    .add_plugins(gaanim_editor::GaanimEditorPlugin)
    .insert_resource(gaanim_media::VideoSamplingMode::Realtime)
    .insert_resource(gaanim_media::PreviewAudioEnabled(true))
    .insert_resource(gaanim_editor::PresentationMode {
        active: launch.present,
    })
    .insert_resource(launch.selection.clone())
    .insert_resource(ReloadStatus::default())
    .insert_resource(ScriptError::default())
    .add_systems(
        Update,
        (
            script_error_listener_system.in_set(gaanim_scene::hierarchy::SceneSet::Input),
            reload_listener_system.in_set(gaanim_scene::hierarchy::SceneSet::Input),
        ),
    )
    .add_systems(
        bevy_egui::EguiPrimaryContextPass,
        (reload_status_overlay_system, script_error_overlay_system),
    )
    .add_systems(Update, open_project_request_system);
    if gaanim_editor::frame_profile::enabled() {
        app.add_plugins(gaanim_editor::frame_profile::FrameProfilePlugin);
    }

    // bevy_egui creates its primary context when the application starts. Keep
    // this camera alive for both the project hub and script launches, so a
    // script payload can reuse it instead of creating the egui camera after
    // the first frame.
    spawn_host_camera(app.world_mut());

    if let Some(script_path) = launch.script_path {
        if let Err(error) = start_script_session(app.world_mut(), script_path, launch.project) {
            console::error("project", error);
            std::process::exit(2);
        }
    } else {
        app.world_mut()
            .resource_mut::<gaanim_editor::project_hub::ProjectHubState>()
            .show();
    }

    app.run();
}

fn dispatch_export_mode() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("export") {
        return false;
    }
    let mut script = None;
    let mut output = None;
    let mut quality = "standard".to_string();
    let mut encoder = VideoEncoder::Auto;
    let mut transparent = false;
    let mut width = 1920_u32;
    let mut height = 1080_u32;
    let mut fit = gaanim_export::prelude::OutputFit::Error;
    let mut from = None;
    let mut to = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            flag @ ("--from" | "--to") => {
                index += 1;
                let seconds = parse_export_seconds(flag, args.get(index)).unwrap_or_else(|error| {
                    console::error("export", error);
                    std::process::exit(2);
                });
                if flag == "--from" {
                    from = Some(seconds);
                } else {
                    to = Some(seconds);
                }
            }
            "--output" | "-o" => {
                index += 1;
                output = args.get(index).cloned();
            }
            "--quality" => {
                index += 1;
                quality = args.get(index).cloned().unwrap_or_default();
            }
            "--encoder" => {
                index += 1;
                encoder = args
                    .get(index)
                    .and_then(|value| VideoEncoder::parse_arg(value))
                    .unwrap_or_else(|| {
                        console::error(
                            "export",
                            format!("encoder must be {}", VideoEncoder::ARG_VALUES.join(", ")),
                        );
                        std::process::exit(2);
                    });
            }
            "--transparent" => transparent = true,
            "--width" => {
                index += 1;
                width = args
                    .get(index)
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or_else(|| {
                        console::error("export", "--width requires a positive integer");
                        std::process::exit(2);
                    });
            }
            "--height" => {
                index += 1;
                height = args
                    .get(index)
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or_else(|| {
                        console::error("export", "--height requires a positive integer");
                        std::process::exit(2);
                    });
            }
            "--fit" => {
                index += 1;
                fit = match args.get(index).map(String::as_str) {
                    Some("error") => gaanim_export::prelude::OutputFit::Error,
                    Some("contain") => gaanim_export::prelude::OutputFit::Contain,
                    Some("cover") => gaanim_export::prelude::OutputFit::Cover,
                    _ => {
                        console::error("export", "--fit must be error, contain, or cover");
                        std::process::exit(2);
                    }
                };
            }
            value if value.starts_with('-') => {
                console::error("export", format!("unknown option `{value}`"));
                std::process::exit(2);
            }
            value if script.is_none() => script = Some(PathBuf::from(value)),
            value => {
                console::error("export", format!("unexpected argument `{value}`"));
                std::process::exit(2);
            }
        }
        index += 1;
    }
    let script = script
        .and_then(|path| gaanim_project::resolve_entry(&path).ok())
        .unwrap_or_else(|| {
            console::error("export", "a script or project to export is required");
            console::hint("Run `gaanim export --help` for the options.");
            std::process::exit(2);
        });
    let output = output.unwrap_or_else(|| {
        console::error("export", "--output is required");
        std::process::exit(2);
    });
    if !matches!(quality.as_str(), "draft" | "standard" | "production") {
        console::error("export", "quality must be draft, standard, or production");
        std::process::exit(2);
    }
    let format = Path::new(&output)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|format| matches!(format.as_str(), "mp4" | "webm" | "webp" | "gif" | "png"))
        .unwrap_or_else(|| {
            console::error(
                "export",
                "output extension must be mp4, webm, webp, gif, or png",
            );
            std::process::exit(2);
        });
    if transparent && !matches!(format.as_str(), "webm" | "webp" | "png") {
        console::error("export", "--transparent requires WebM, WebP, or PNG output");
        std::process::exit(2);
    }
    if format != "mp4" && encoder != VideoEncoder::Auto {
        console::error("export", "--encoder requires MP4 output");
        std::process::exit(2);
    }
    if let Err(error) = validate_export_range(from.as_ref(), to.as_ref()) {
        console::error("export", error);
        std::process::exit(2);
    }
    console::banner("Export");
    if let Err(error) = run_export_worker(ExportWorkerArgs {
        script,
        output,
        quality,
        format,
        encoder,
        transparent,
        width,
        height,
        fit,
        from,
        to,
    }) {
        console::error("export", error);
        std::process::exit(1);
    }
    true
}

fn dispatch_python_api_validation_mode() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--validate-python-api") {
        return false;
    }
    if args.len() != 2 {
        eprintln!("usage: gaanim-core --validate-python-api <validator.py>");
        std::process::exit(2);
    }
    gaanim_python::register_inittab();
    Python::initialize();
    if let Err(error) = script_runner::validate_python_api(Path::new(&args[1])) {
        console::error("python", format!("API validation failed: {error}"));
        std::process::exit(1);
    }
    true
}

/// One end of an export range: seconds, or a `scene.marker` name resolved
/// after the script runs.
#[derive(Debug, Clone, PartialEq)]
enum ExportBound {
    Seconds(f64),
    Marker(String),
}

impl std::fmt::Display for ExportBound {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Seconds(seconds) => write!(formatter, "{seconds}"),
            Self::Marker(name) => write!(formatter, "{name}"),
        }
    }
}

/// `--from` / `--to`: finite non-negative seconds, or a marker name.
fn parse_export_seconds(flag: &str, value: Option<&String>) -> Result<ExportBound, String> {
    let value = value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{flag} requires seconds or a marker name"))?;
    match value.parse::<f64>() {
        Ok(seconds) if seconds.is_finite() && seconds >= 0.0 => Ok(ExportBound::Seconds(seconds)),
        Ok(_) => Err(format!(
            "{flag} requires a non-negative number of seconds or a marker name"
        )),
        Err(_) if value.starts_with('-') => Err(format!(
            "{flag} requires a non-negative number of seconds or a marker name"
        )),
        Err(_) => Ok(ExportBound::Marker(value.to_string())),
    }
}

fn validate_export_range(
    from: Option<&ExportBound>,
    to: Option<&ExportBound>,
) -> Result<(), String> {
    match (from, to) {
        (Some(ExportBound::Seconds(from)), Some(ExportBound::Seconds(to))) if to <= from => {
            Err(format!("--to ({to}) must be greater than --from ({from})"))
        }
        _ => Ok(()),
    }
}

/// Resolve marker bounds against the markers the script authored.
fn resolve_export_bound(
    flag: &str,
    bound: Option<&ExportBound>,
    markers: &[gaanim_api::canvas::SceneMarker],
) -> Result<Option<f64>, String> {
    match bound {
        None => Ok(None),
        Some(ExportBound::Seconds(seconds)) => Ok(Some(*seconds)),
        Some(ExportBound::Marker(name)) => markers
            .iter()
            .find(|marker| marker.name == *name)
            .map(|marker| Some(marker.time))
            .ok_or_else(|| {
                let known = markers
                    .iter()
                    .map(|marker| format!("{:?}", marker.name))
                    .collect::<Vec<_>>();
                format!(
                    "{flag}: unknown marker {name:?}; the script defines {}",
                    if known.is_empty() {
                        "no markers (use scene.marker(\"name\"))".to_string()
                    } else {
                        known.join(", ")
                    }
                )
            }),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ExportWorkerArgs {
    script: PathBuf,
    output: String,
    quality: String,
    format: String,
    encoder: VideoEncoder,
    transparent: bool,
    width: u32,
    height: u32,
    fit: gaanim_export::prelude::OutputFit,
    /// Exported time range in seconds or marker names; `None` means the
    /// scene start/end.
    from: Option<ExportBound>,
    to: Option<ExportBound>,
}

fn parse_export_worker_args(args: &[String]) -> Result<ExportWorkerArgs, String> {
    if args.len() < 4 {
        return Err(
            "expected: --export-worker <script.py> <output> <draft|standard|production> <mp4|webm|webp|gif|png> [--encoder auto|libx264|nvenc|amf|qsv|vaapi] [--transparent]"
                .to_string(),
        );
    }
    if !matches!(args[2].as_str(), "draft" | "standard" | "production") {
        return Err(format!("unknown export quality '{}'", args[2]));
    }
    if !matches!(args[3].as_str(), "mp4" | "webm" | "webp" | "gif" | "png") {
        return Err(format!("unknown export format '{}'", args[3]));
    }
    let mut encoder = VideoEncoder::Auto;
    let mut transparent = false;
    let mut width = 1920_u32;
    let mut height = 1080_u32;
    let mut fit = gaanim_export::prelude::OutputFit::Error;
    let mut from = None;
    let mut to = None;
    let mut index = 4;
    while index < args.len() {
        match args[index].as_str() {
            "--transparent" => transparent = true,
            "--from" => {
                index += 1;
                from = Some(parse_export_seconds("--from", args.get(index))?);
            }
            "--to" => {
                index += 1;
                to = Some(parse_export_seconds("--to", args.get(index))?);
            }
            "--encoder" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--encoder requires a value".to_string())?;
                encoder = VideoEncoder::parse_arg(value)
                    .ok_or_else(|| format!("unknown export encoder '{value}'"))?;
            }
            "--width" => {
                index += 1;
                width = args
                    .get(index)
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0)
                    .ok_or_else(|| "--width requires a positive integer".to_string())?;
            }
            "--height" => {
                index += 1;
                height = args
                    .get(index)
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0)
                    .ok_or_else(|| "--height requires a positive integer".to_string())?;
            }
            "--fit" => {
                index += 1;
                fit = match args.get(index).map(String::as_str) {
                    Some("error") => gaanim_export::prelude::OutputFit::Error,
                    Some("contain") => gaanim_export::prelude::OutputFit::Contain,
                    Some("cover") => gaanim_export::prelude::OutputFit::Cover,
                    _ => return Err("--fit must be error, contain, or cover".to_string()),
                };
            }
            value => return Err(format!("unknown export worker option '{value}'")),
        }
        index += 1;
    }
    if args[3] != "mp4" && encoder != VideoEncoder::Auto {
        return Err("--encoder requires MP4 output".to_string());
    }
    validate_export_range(from.as_ref(), to.as_ref())?;
    Ok(ExportWorkerArgs {
        script: PathBuf::from(&args[0]),
        output: args[1].clone(),
        quality: args[2].clone(),
        format: args[3].clone(),
        encoder,
        transparent,
        width,
        height,
        fit,
        from,
        to,
    })
}

fn dispatch_export_worker_mode() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--export-worker") {
        return false;
    }
    let worker = parse_export_worker_args(&args[1..]).unwrap_or_else(|error| {
        console::error("export", error);
        std::process::exit(2);
    });
    if let Err(error) = run_export_worker(worker) {
        console::error("export", error);
        std::process::exit(1);
    }
    true
}

fn run_export_worker(worker: ExportWorkerArgs) -> Result<(), String> {
    let probe = gaanim_project::EnvironmentProbe::detect(Some(&worker.script));
    let venv_root = gaanim_project::activate_environment(&probe)?;
    gaanim_python::register_inittab();
    Python::initialize();
    if let Some(ref venv) = venv_root {
        python_home::inject_venv_site_packages(venv);
    }

    let canvas = script_runner::load_script_canvas(&worker.script)?;
    let markers = canvas.markers();
    let from = resolve_export_bound("--from", worker.from.as_ref(), &markers)?;
    let to = resolve_export_bound("--to", worker.to.as_ref(), &markers)?;
    if let (Some(from), Some(to)) = (from, to)
        && to <= from
    {
        return Err(format!(
            "--to ({}) resolves to {to}s, which must be after --from ({}) at {from}s",
            worker
                .to
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            worker
                .from
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ));
    }
    let quality = match worker.quality.as_str() {
        "draft" => gaanim_export::prelude::QualityPreset::Draft,
        "standard" => gaanim_export::prelude::QualityPreset::Standard,
        "production" => gaanim_export::prelude::QualityPreset::Production,
        _ => unreachable!("validated export quality"),
    };
    let format = match worker.format.as_str() {
        "mp4" => gaanim_export::encoder::ExportFormat::Mp4,
        "webm" => gaanim_export::encoder::ExportFormat::Webm,
        "webp" => gaanim_export::encoder::ExportFormat::Webp,
        "gif" => gaanim_export::encoder::ExportFormat::Gif,
        "png" => gaanim_export::encoder::ExportFormat::PngSequence,
        _ => unreachable!("validated export format"),
    };
    let mut config =
        gaanim_export::prelude::ExportConfig::new(&worker.output).with_quality(quality);
    config.width = worker.width;
    config.height = worker.height;
    config.fit = worker.fit;
    config.start_time = from;
    config.end_time = to;
    config.aspect_ratio = gaanim_export::prelude::AspectRatioPreset::Custom;
    config.format = format;
    config.video_encoder = worker.encoder;
    if worker.transparent {
        config.transparent = true;
    }
    config.headless = true;
    gaanim_api::export::export_canvas(canvas, config).map_err(|error| error.to_string())
}

/// Install the persistent primary camera before the Bevy event loop begins.
///
/// Canvas replay reuses this Vello camera; creating it during replay is too
/// late for `bevy_egui` to attach its primary context on script launches.
fn spawn_host_camera(world: &mut World) {
    world.spawn((
        Camera2d,
        gaanim_renderer::prelude::VelloView,
        bevy::prelude::Camera {
            order: 1,
            clear_color: bevy::camera::ClearColorConfig::None,
            ..default()
        },
        bevy::core_pipeline::tonemapping::Tonemapping::None,
    ));
}

fn start_script_session(
    world: &mut World,
    script_path: PathBuf,
    project: Option<gaanim_project::ResolvedProject>,
) -> Result<(), String> {
    if let Some(project) = &project
        && let Err(error) = gaanim_project::provision_authoring_package(&project.root)
    {
        console::warn(
            "python",
            format!("authoring environment not ready: {error}"),
        );
    }
    let hint = project
        .as_ref()
        .map(|project| project.root.as_path())
        .unwrap_or(script_path.as_path());
    let probe = gaanim_project::EnvironmentProbe::detect(Some(hint));
    let venv_root = gaanim_project::activate_environment(&probe)?;
    gaanim_python::register_inittab();
    Python::initialize();
    if let Some(ref venv) = venv_root {
        python_home::inject_venv_site_packages(venv);
    }

    let (payload_tx, payload_rx) = crossbeam_channel::unbounded::<ReloadPayload>();
    let (error_tx, error_rx) = crossbeam_channel::unbounded::<String>();
    let runner = script_runner::ScriptRunner::spawn(script_path.clone(), payload_tx, error_tx);
    world.insert_resource(gaanim_editor::narration::ScriptReload(std::sync::Arc::new(
        runner.asset_reload_handle(),
    )));
    let file_watcher::FileWatcher { changed_rx, stop } =
        file_watcher::FileWatcher::spawn(script_path.clone());
    std::thread::Builder::new()
        .name("gaanim-watcher-bridge".into())
        .spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match changed_rx.recv_timeout(std::time::Duration::from_millis(250)) {
                    Ok(file_watcher::ProjectChange::Source) => runner.request_rerun(),
                    Ok(file_watcher::ProjectChange::Assets) => runner.request_asset_reload(),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
            }
        })
        .map_err(|error| format!("failed to spawn watcher bridge: {error}"))?;

    let project_paths = resolve_project_paths(&script_path, project.as_ref());
    world.insert_resource(project_paths);
    world.insert_resource(ReloadReceiver { rx: payload_rx });
    world.insert_resource(ScriptErrorReceiver { rx: error_rx });
    if let Some(project) = project {
        let mut recents = gaanim_project::RecentProjects::load();
        recents.record(&project);
        let _ = recents.save();
        if let Some(mut window) = world
            .query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>()
            .iter_mut(world)
            .next()
        {
            window.title = format!("Gaanim — {}", project.manifest.name);
        }
    }
    world
        .resource_mut::<gaanim_editor::project_hub::ProjectHubState>()
        .active = false;
    Ok(())
}

fn open_project_request_system(world: &mut World) {
    if world.contains_resource::<gaanim_editor::export::ProjectPaths>() {
        return;
    }
    let request = world
        .resource_mut::<gaanim_editor::project_hub::PendingProjectOpen>()
        .0
        .take();
    let Some(project) = request else {
        return;
    };
    if let Err(error) = start_script_session(world, project.entry.clone(), Some(project)) {
        world
            .resource_mut::<gaanim_editor::project_hub::ProjectHubState>()
            .report_open_error(error);
    }
}

/// Generate a runnable project starter without initializing Python or Bevy.
fn dispatch_init_mode() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("init") {
        return false;
    }

    let args: Vec<_> = args.collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        gaanim_project::help::print(gaanim_project::help::Topic::Init);
        return true;
    }
    let parsed = parse_init_args(&args).unwrap_or_else(|error| {
        console::error("init", error);
        console::hint("Run `gaanim init --help` for usage.");
        std::process::exit(2);
    });

    let project = gaanim_project::create_project(&parsed).unwrap_or_else(|error| {
        console::error("init", error);
        std::process::exit(2);
    });

    let venv = gaanim_project::provision_authoring_package(&project.root);
    console::success(
        "init",
        format!(
            "Created {} project: {}",
            parsed.kind.name(),
            project.root.display()
        ),
    );
    console::detail("Edit", project.entry.display());
    console::detail("Preview", format!("gaanim {}", project.root.display()));
    console::detail("Check", format!("gaanim check {}", project.root.display()));
    if parsed.kind.is_slides() {
        console::detail(
            "Present",
            format!("gaanim --present --monitor 1 {}", project.root.display()),
        );
    } else {
        console::detail(
            "Export",
            format!(
                "gaanim export {} --output exports/video.mp4 --quality production",
                project.root.display()
            ),
        );
    }
    match venv {
        Ok(venv) => console::detail("Python", venv.display()),
        Err(error) => console::warn(
            "python",
            format!("authoring environment not ready: {error}"),
        ),
    }
    true
}

fn parse_init_args(args: &[String]) -> Result<gaanim_project::CreateProjectOptions, String> {
    let kind = args
        .first()
        .ok_or_else(|| "missing project kind; available kinds: video, slides".to_string())
        .and_then(|value| gaanim_project::ProjectKind::parse(value))?;

    let mut directory = None;
    let mut force = false;
    for arg in &args[1..] {
        match arg.as_str() {
            "--force" => force = true,
            value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
            value if directory.is_none() => directory = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument `{value}`")),
        }
    }

    Ok(gaanim_project::CreateProjectOptions {
        kind,
        directory: directory.unwrap_or_else(|| PathBuf::from(kind.default_directory())),
        force,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckArgs {
    script: PathBuf,
    strict: bool,
}

fn dispatch_check_mode() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("check") {
        return false;
    }

    let args: Vec<_> = args.collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        gaanim_project::help::print(gaanim_project::help::Topic::Check);
        return true;
    }
    let parsed = parse_check_args(&args).unwrap_or_else(|error| {
        console::error("check", error);
        console::hint("Run `gaanim check --help` for usage.");
        std::process::exit(2);
    });
    let script = gaanim_project::resolve_entry(&parsed.script).unwrap_or_else(|error| {
        console::error("check", error);
        std::process::exit(2);
    });

    let probe = gaanim_project::EnvironmentProbe::detect(Some(&script));
    let venv_root = gaanim_project::activate_environment(&probe).unwrap_or_else(|error| {
        console::error("check", error);
        std::process::exit(2);
    });
    gaanim_python::register_inittab();
    Python::initialize();
    if let Some(ref venv) = venv_root {
        python_home::inject_venv_site_packages(venv);
    }
    let canvas = script_runner::load_script_canvas(&script).unwrap_or_else(|error| {
        console::error("check", format!("could not load project: {error}"));
        std::process::exit(2);
    });
    let source = std::fs::read_to_string(&script).unwrap_or_default();
    let is_presentation = canvas.has_presentation_features();
    let report = if is_presentation {
        presentation_preflight(&canvas, &source)
    } else {
        scene_preflight(&canvas, &source)
    };

    // The report goes to stdout so it can be captured. Its first line, the
    // `check` status line, marks where it starts (the docs builder relies on it).
    let color = console::color_enabled(console::Stream::Stdout);
    let line = |level, label: &str, message: &str| {
        println!("{}", console::format_line(level, label, message, color));
    };
    line(
        console::Level::Info,
        "check",
        &format!(
            "{} · {}",
            console::display_path(&script),
            if is_presentation {
                "presentation"
            } else {
                "scene"
            }
        ),
    );
    let summary = if is_presentation {
        format!(
            "{} segments · {} stops · {:.1} seconds · {}×{}",
            report.segment_count,
            report.stop_count,
            report.duration,
            canvas.frame.width,
            canvas.frame.height
        )
    } else {
        format!(
            "{:.1} seconds · {}×{}",
            report.duration, canvas.frame.width, canvas.frame.height
        )
    };
    println!("{}", console::format_hint(&summary, color));
    for error in &report.errors {
        line(console::Level::Error, "error", error);
    }
    for warning in &report.warnings {
        line(console::Level::Warn, "warning", warning);
    }
    let plural =
        |count: usize, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
    if report.errors.is_empty() && report.warnings.is_empty() {
        let verdict = if is_presentation {
            "Ready to present"
        } else {
            "No problems found"
        };
        line(console::Level::Success, "pass", verdict);
    } else if report.errors.is_empty() {
        line(
            console::Level::Success,
            "pass",
            &format!("with {}", plural(report.warnings.len(), "warning")),
        );
    } else {
        line(
            console::Level::Error,
            "fail",
            &plural(report.errors.len(), "error"),
        );
    }

    if !report.errors.is_empty() || (parsed.strict && !report.warnings.is_empty()) {
        std::process::exit(1);
    }
    true
}

fn parse_check_args(args: &[String]) -> Result<CheckArgs, String> {
    let mut script = None;
    let mut strict = false;
    for arg in args {
        match arg.as_str() {
            "--strict" => strict = true,
            value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
            value if script.is_none() => script = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument `{value}`")),
        }
    }
    Ok(CheckArgs {
        script: script.ok_or_else(|| "missing <script.py>".to_string())?,
        strict,
    })
}

#[derive(Debug, Default, PartialEq)]
struct PreflightReport {
    segment_count: usize,
    stop_count: usize,
    duration: f64,
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn presentation_preflight(
    canvas: &gaanim_api::canvas::SceneModel,
    source: &str,
) -> PreflightReport {
    let manifest = canvas.segment_manifest();
    let mut report = PreflightReport {
        segment_count: manifest.segments.len(),
        stop_count: manifest
            .segments
            .iter()
            .map(|segment| segment.stops.len())
            .sum(),
        duration: manifest.duration(),
        ..default()
    };
    report.warnings.extend(canvas.unthemed_contrast_warning());

    if manifest.segments.is_empty() {
        report
            .errors
            .push("no segments; use `scene.segment(...)`".to_string());
        return report;
    }
    let aspect_ratio = canvas.frame.aspect_ratio();
    if (aspect_ratio - 16.0 / 9.0).abs() > 0.02 {
        report.warnings.push(format!(
            "canvas aspect ratio is {:.3}; 16:9 is recommended for projectors",
            aspect_ratio
        ));
    }

    for segment in &manifest.segments {
        if segment
            .notes
            .as_deref()
            .is_none_or(|notes| notes.trim().is_empty())
        {
            report
                .warnings
                .push(format!("segment `{}` has no speaker notes", segment.name));
        }
        if segment.stops.iter().any(|stop| stop.name.is_none()) {
            report.warnings.push(format!(
                "segment `{}` contains unnamed stops; names improve Presenter View",
                segment.name
            ));
        }
        if segment.end_time - segment.start_time <= 1e-5 {
            report
                .errors
                .push(format!("segment `{}` has zero duration", segment.name));
        }
    }

    let placeholders = source
        .lines()
        .filter(|line| line.contains("\"[") || line.contains("'["))
        .count();
    if placeholders > 0 {
        report.warnings.push(format!(
            "{placeholders} placeholder line{} still contain text beginning with `[`",
            if placeholders == 1 { "" } else { "s" }
        ));
    }
    if report.duration < 1.0 {
        report
            .warnings
            .push("timeline is shorter than one second".to_string());
    }

    report
}

fn scene_preflight(canvas: &gaanim_api::canvas::SceneModel, source: &str) -> PreflightReport {
    let mut report = PreflightReport {
        duration: canvas.current_time(),
        ..default()
    };
    report.warnings.extend(canvas.unthemed_contrast_warning());
    if canvas.frame.validate().is_err() {
        report
            .errors
            .push("canvas width and height must be positive".to_string());
    }
    if report.duration < 1e-3 {
        report
            .warnings
            .push("timeline has no visible duration".to_string());
    }
    let placeholders = source
        .lines()
        .filter(|line| line.contains("\"[") || line.contains("'["))
        .count();
    if placeholders > 0 {
        report.warnings.push(format!(
            "{placeholders} placeholder line{} still contain text beginning with `[`",
            if placeholders == 1 { "" } else { "s" }
        ));
    }
    report
}

#[derive(Debug)]
struct DiffModeArgs {
    baseline: PathBuf,
    current: PathBuf,
    output: PathBuf,
    options: gaanim_diff::CompareOptions,
    example: Option<PathBuf>,
    capture: bool,
    capture_only: bool,
    bless: bool,
    /// Capture every `scene.stop(...)` instead of the script's `scene.snapshots`.
    capture_stops: bool,
    /// 1-based stops to capture; `None` captures all of them.
    stops: Option<Vec<usize>>,
    /// With `--capture-stops`, only stops inside these segments or sections.
    selection: gaanim_timeline::selection::SegmentSelection,
}

/// Handle `gaanim --diff ...` before Python, Bevy, or the editor are initialized.
fn dispatch_diff_mode() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--diff") {
        return false;
    }

    let args: Vec<_> = args.collect();
    let parsed = match parse_diff_mode_args(&args) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => {
            gaanim_project::help::print(gaanim_project::help::Topic::Diff);
            return true;
        }
        Err(error) => {
            console::error("diff", error);
            console::hint("Run `gaanim --diff --help` for usage.");
            std::process::exit(2);
        }
    };

    if let Some(example) = &parsed.example
        && (parsed.capture || parsed.bless)
    {
        let script = gaanim_project::resolve_entry(example).unwrap_or_else(|error| {
            console::error("diff", error);
            std::process::exit(2);
        });
        let capture_dir = if parsed.bless {
            &parsed.baseline
        } else {
            &parsed.current
        };
        println!(
            "Capturing {} -> {}",
            console::display_path(&script),
            capture_dir.display()
        );
        let probe = gaanim_project::EnvironmentProbe::detect(Some(&script));
        let venv_root = gaanim_project::activate_environment(&probe).unwrap_or_else(|error| {
            console::error("diff", error);
            std::process::exit(2);
        });
        gaanim_python::register_inittab();
        Python::initialize();
        if let Some(ref venv) = venv_root {
            python_home::inject_venv_site_packages(venv);
        }
        if parsed.capture_stops {
            capture_stop_snapshots(
                &script,
                capture_dir,
                parsed.stops.as_deref(),
                &parsed.selection,
            );
        } else if let Err(error) = script_runner::capture_script_snapshots(&script, capture_dir) {
            console::error("diff", format!("snapshot capture failed: {error}"));
            std::process::exit(2);
        }
        if !capture_dir.join(gaanim_diff::MANIFEST_FILE).is_file() {
            console::error(
                "diff",
                format!("{} did not call scene.snapshots(...)", script.display()),
            );
            std::process::exit(2);
        }
    }

    if parsed.bless {
        println!("Baseline updated: {}", parsed.baseline.display());
        std::process::exit(0);
    }

    if parsed.capture_only {
        println!("Snapshots captured: {}", parsed.current.display());
        std::process::exit(0);
    }

    match comparison_blocker(&parsed.baseline, parsed.capture_stops) {
        Some(Ok(note)) => {
            println!("Snapshots captured: {}", parsed.current.display());
            println!("{note}");
            std::process::exit(0);
        }
        Some(Err(error)) => {
            console::error("diff", error);
            std::process::exit(2);
        }
        None => {}
    }

    let report = match gaanim_diff::compare_directories(
        &parsed.baseline,
        &parsed.current,
        &parsed.output,
        parsed.options,
    ) {
        Ok(report) => report,
        Err(error) => {
            console::error("diff", error);
            std::process::exit(2);
        }
    };

    println!(
        "{}: {} compared, {} changed, {} missing",
        if report.passed { "PASS" } else { "FAIL" },
        report.compared,
        report.changed,
        report.missing
    );
    println!("Report: {}", parsed.output.join("index.html").display());
    println!(
        "JSON: {}",
        parsed.output.join(gaanim_diff::REPORT_FILE).display()
    );

    std::process::exit(if report.passed { 0 } else { 1 });
}

/// Why `--diff` ends after capturing instead of comparing with `baseline`:
/// `Ok` for a successful stop capture that has no stop baseline to compare
/// with (a `scene.snapshots` baseline shares none of its ids), `Err` when
/// there is no baseline at all.
fn comparison_blocker(baseline: &Path, capture_stops: bool) -> Option<Result<String, String>> {
    if capture_stops && !baseline.join(gaanim_diff::STOPS_FILE).is_file() {
        return Some(Ok(format!(
            "No stop baseline in {}; nothing to compare. Pass --capture-only to skip this check.",
            baseline.display()
        )));
    }
    (!baseline.is_dir()).then(|| {
        Err(format!(
            "baseline {} does not exist; capture it with --bless first",
            baseline.display()
        ))
    })
}

/// Run the script like `gaanim check` and capture the frame shown at each stop.
fn capture_stop_snapshots(
    script: &Path,
    capture_dir: &Path,
    stops: Option<&[usize]>,
    selection: &gaanim_timeline::selection::SegmentSelection,
) {
    let canvas = script_runner::load_script_canvas(script).unwrap_or_else(|error| {
        console::error("diff", error);
        std::process::exit(2);
    });
    let selected;
    let stops = if selection.is_empty() {
        stops
    } else {
        selected = gaanim_diff::stops_in_selection(&canvas.segment_manifest(), selection, stops)
            .unwrap_or_else(|error| {
                console::error("diff", error);
                std::process::exit(2);
            });
        Some(selected.as_slice())
    };
    let capture = gaanim_diff::capture_stops(canvas, capture_dir, stops).unwrap_or_else(|error| {
        console::error("diff", format!("stop capture failed: {error}"));
        std::process::exit(2);
    });
    for stop in &capture.stops.stops {
        let name = stop
            .name
            .as_deref()
            .map(|name| format!(" · {name}"))
            .unwrap_or_default();
        println!(
            "  stop {}/{} · {}{name} · {:.3}s -> {}",
            stop.index, capture.stops.total, stop.segment, stop.time_seconds, stop.file
        );
    }
    println!(
        "Stops: {}",
        capture_dir.join(gaanim_diff::STOPS_FILE).display()
    );
}

fn parse_diff_mode_args(args: &[String]) -> Result<Option<DiffModeArgs>, String> {
    let mut baseline = None;
    let mut current = None;
    let mut output = None;
    let mut example = None;
    let mut tests_root = PathBuf::from("tests/visual");
    let mut options = gaanim_diff::CompareOptions::default();
    let mut capture = None;
    let mut capture_only = false;
    let mut bless = false;
    let mut capture_stops = false;
    let mut stops = None;
    let mut selection = gaanim_timeline::selection::SegmentSelection::default();
    let mut index = 0;

    while index < args.len() {
        let flag = &args[index];
        index += 1;
        let value = |index: &mut usize| -> Result<&str, String> {
            let value = args
                .get(*index)
                .ok_or_else(|| format!("{flag} requires a value"))?;
            *index += 1;
            Ok(value)
        };

        match flag.as_str() {
            "--baseline" | "-b" => baseline = Some(PathBuf::from(value(&mut index)?)),
            "--current" | "-c" => current = Some(PathBuf::from(value(&mut index)?)),
            "--output" | "-o" => output = Some(PathBuf::from(value(&mut index)?)),
            "--example" | "-e" => example = Some(PathBuf::from(value(&mut index)?)),
            "--tests-root" => tests_root = PathBuf::from(value(&mut index)?),
            "--pixel-threshold" => {
                options.pixel_threshold = value(&mut index)?
                    .parse()
                    .map_err(|_| "--pixel-threshold must be between 0 and 255".to_string())?;
            }
            "--max-changed-ratio" => {
                options.max_changed_ratio = value(&mut index)?
                    .parse()
                    .map_err(|_| "--max-changed-ratio must be between 0 and 1".to_string())?;
            }
            "--no-capture" => capture = Some(false),
            "--capture-only" => capture_only = true,
            "--bless" => bless = true,
            "--capture-stops" => capture_stops = true,
            "--stops" => {
                stops = Some(
                    gaanim_diff::parse_stop_selection(value(&mut index)?)
                        .map_err(|error| format!("--stops: {error}"))?,
                );
            }
            "--sections" => {
                selection.sections =
                    gaanim_timeline::selection::SegmentSelection::parse_list(value(&mut index)?)?;
            }
            "--from" => selection.from = Some(value(&mut index)?.to_string()),
            "--help" | "-h" => return Ok(None),
            _ => return Err(format!("unknown option `{flag}`")),
        }
    }

    if capture_only && bless {
        return Err("--capture-only cannot be combined with --bless".to_string());
    }
    if capture_only && capture == Some(false) {
        return Err("--capture-only cannot be combined with --no-capture".to_string());
    }
    if stops.is_some() && !capture_stops {
        return Err("--stops requires --capture-stops".to_string());
    }
    if !selection.is_empty() && !capture_stops {
        return Err(
            "--sections and --from require --capture-stops; scene.snapshots times are chosen by the script"
                .to_string(),
        );
    }
    if capture_stops && capture == Some(false) {
        return Err("--capture-stops cannot be combined with --no-capture".to_string());
    }

    if let Some(example) = example {
        let case_dir = visual_test_case_dir(&tests_root, &example)?;
        return Ok(Some(DiffModeArgs {
            baseline: baseline.unwrap_or_else(|| case_dir.join("baseline")),
            current: current.unwrap_or_else(|| case_dir.join("current")),
            output: output.unwrap_or_else(|| case_dir.join("report")),
            options,
            example: Some(example),
            capture: capture.unwrap_or(true),
            capture_only,
            bless,
            capture_stops,
            stops,
            selection,
        }));
    }

    if bless {
        return Err("--bless requires --example <SCRIPT_OR_PROJECT>".to_string());
    }
    if capture_only {
        return Err("--capture-only requires --example <SCRIPT_OR_PROJECT>".to_string());
    }
    if capture_stops {
        return Err("--capture-stops requires --example <SCRIPT_OR_PROJECT>".to_string());
    }

    Ok(Some(DiffModeArgs {
        baseline: baseline.ok_or_else(|| {
            "missing --baseline <DIR> or --example <SCRIPT_OR_PROJECT>".to_string()
        })?,
        current: current.ok_or_else(|| {
            "missing --current <DIR> or --example <SCRIPT_OR_PROJECT>".to_string()
        })?,
        output: output.unwrap_or_else(|| PathBuf::from("tests/visual/report")),
        options,
        example: None,
        capture: false,
        capture_only: false,
        bless: false,
        capture_stops: false,
        stops: None,
        selection,
    }))
}

fn visual_test_case_dir(tests_root: &Path, example: &Path) -> Result<PathBuf, String> {
    // `.` or `project/..` name a project directory without a final component;
    // resolve them like `gaanim .` does so the case is named after the folder.
    let resolved;
    let example = if example.file_stem().is_none() {
        resolved = example
            .canonicalize()
            .map_err(|error| format!("cannot resolve example {}: {error}", example.display()))?;
        resolved.as_path()
    } else {
        example
    };
    let stem = example
        .file_stem()
        .ok_or_else(|| format!("example has no file stem: {}", example.display()))?;
    if example.is_absolute() {
        return Ok(tests_root.join(stem));
    }

    let relative = example.strip_prefix("examples").unwrap_or(example);
    let mut case_dir = PathBuf::new();
    for component in relative.components() {
        if let std::path::Component::Normal(component) = component {
            case_dir.push(component);
        }
    }
    case_dir.set_extension("");
    if case_dir.as_os_str().is_empty() {
        case_dir.push(stem);
    }
    Ok(tests_root.join(case_dir))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchArgs {
    script_path: Option<PathBuf>,
    project: Option<gaanim_project::ResolvedProject>,
    present: bool,
    /// Zero-based monitor index used only with `--present`.
    monitor: Option<usize>,
    /// Segments to rehearse with `--sections` / `--from`.
    selection: gaanim_timeline::selection::SegmentSelection,
}

fn resolve_project_paths(
    script_path: &Path,
    project: Option<&gaanim_project::ResolvedProject>,
) -> gaanim_editor::export::ProjectPaths {
    let script_parent = script_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    if let Some(project) = project {
        let output_dir = if project.manifest.output_dir.is_absolute() {
            project.manifest.output_dir.clone()
        } else {
            project.root.join(&project.manifest.output_dir)
        };
        gaanim_editor::export::ProjectPaths {
            project_dir: project.root.clone(),
            output_dir,
            script_path: script_path.to_path_buf(),
        }
    } else {
        let proj = script_parent.canonicalize().unwrap_or(script_parent);
        let out = proj.join("exports");
        gaanim_editor::export::ProjectPaths {
            project_dir: proj.clone(),
            output_dir: out,
            script_path: script_path.to_path_buf(),
        }
    }
}

fn parse_args() -> LaunchArgs {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return LaunchArgs {
            script_path: None,
            project: None,
            present: false,
            monitor: None,
            selection: Default::default(),
        };
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        gaanim_project::help::print(gaanim_project::help::Topic::General);
        std::process::exit(0);
    }
    let parsed = parse_launch_args(&args).unwrap_or_else(|error| {
        console::error("usage", error);
        std::process::exit(2);
    });
    let Some(raw_path) = parsed.script_path.as_ref() else {
        return parsed;
    };
    let (path, project) = if raw_path.is_dir() {
        let project = gaanim_project::resolve_project(raw_path).unwrap_or_else(|error| {
            console::error("project", error);
            std::process::exit(2);
        });
        (project.entry.clone(), Some(project))
    } else {
        let path = gaanim_project::resolve_entry(raw_path).unwrap_or_else(|error| {
            console::error("project", error);
            std::process::exit(2);
        });
        let project = gaanim_project::find_project_for_script(&path);
        (path, project)
    };
    LaunchArgs {
        script_path: Some(path),
        project,
        ..parsed
    }
}

fn parse_launch_args(args: &[String]) -> Result<LaunchArgs, String> {
    let mut script_path = None;
    let mut present = false;
    let mut monitor = None;
    let mut selection = gaanim_timeline::selection::SegmentSelection::default();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        index += 1;
        match arg.as_str() {
            "--present" => present = true,
            "--sections" | "--from" => {
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{arg} requires a segment or section name"))?;
                index += 1;
                if arg == "--from" {
                    selection.from = Some(value.clone());
                } else {
                    selection.sections =
                        gaanim_timeline::selection::SegmentSelection::parse_list(value)?;
                }
            }
            "--monitor" => {
                let value = args
                    .get(index)
                    .ok_or_else(|| "--monitor requires a zero-based monitor index".to_string())?;
                index += 1;
                monitor = Some(
                    value
                        .parse()
                        .map_err(|_| "--monitor must be a non-negative integer".to_string())?,
                );
            }
            value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
            value if script_path.is_none() => script_path = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument `{value}`")),
        }
    }

    if monitor.is_some() && !present {
        return Err("--monitor requires --present".to_string());
    }
    if present && script_path.is_none() {
        return Err("--present requires <SCRIPT_OR_PROJECT>".to_string());
    }
    if !selection.is_empty() && script_path.is_none() {
        return Err("--sections and --from require <SCRIPT_OR_PROJECT>".to_string());
    }
    Ok(LaunchArgs {
        script_path,
        project: None,
        present,
        monitor,
        selection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_case_dir_resolves_current_and_parent_directories() {
        // Regression for #20: `--example .` used to fail with "no file stem".
        let root = Path::new("tests/visual");
        let cwd = std::env::current_dir().unwrap();
        let name = cwd.file_name().unwrap();
        assert_eq!(
            visual_test_case_dir(root, Path::new(".")).unwrap(),
            root.join(name)
        );
        let parent = cwd.join("src").join("..");
        assert_eq!(
            visual_test_case_dir(root, &parent).unwrap(),
            root.join(name)
        );
        assert_eq!(
            visual_test_case_dir(root, Path::new("examples/nested/demo.py")).unwrap(),
            root.join("nested").join("demo")
        );
    }

    #[test]
    fn diff_compares_only_against_a_baseline_of_the_same_capture_kind() {
        let root = std::env::temp_dir().join(format!("gaanim_diff_blocker_{}", std::process::id()));
        let baseline = root.join("baseline");
        let _ = std::fs::remove_dir_all(&root);

        // No baseline: stop captures succeed, snapshot diffs explain the fix.
        assert!(matches!(comparison_blocker(&baseline, true), Some(Ok(_))));
        let missing = comparison_blocker(&baseline, false).unwrap().unwrap_err();
        assert!(missing.contains("--bless"), "{missing}");

        // A scene.snapshots baseline has no stops.json.
        std::fs::create_dir_all(&baseline).unwrap();
        std::fs::write(baseline.join(gaanim_diff::MANIFEST_FILE), "{}").unwrap();
        assert!(matches!(comparison_blocker(&baseline, true), Some(Ok(_))));
        assert_eq!(comparison_blocker(&baseline, false), None);

        std::fs::write(baseline.join(gaanim_diff::STOPS_FILE), "{}").unwrap();
        assert_eq!(comparison_blocker(&baseline, true), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn export_bounds_resolve_scene_markers() {
        let markers = vec![gaanim_api::canvas::SceneMarker {
            name: "climax".into(),
            time: 2.5,
            segment: "_default".into(),
        }];
        let climax = ExportBound::Marker("climax".into());
        assert_eq!(
            resolve_export_bound("--from", Some(&climax), &markers),
            Ok(Some(2.5))
        );
        assert_eq!(
            resolve_export_bound("--to", Some(&ExportBound::Seconds(4.0)), &markers),
            Ok(Some(4.0))
        );
        let error =
            resolve_export_bound("--to", Some(&ExportBound::Marker("fin".into())), &markers)
                .unwrap_err();
        assert!(error.contains("unknown marker \"fin\"") && error.contains("\"climax\""));
        // Marker ranges are only ordered once the script has run.
        assert!(validate_export_range(Some(&climax), Some(&ExportBound::Seconds(0.1))).is_ok());
    }

    #[test]
    fn parses_isolated_export_worker_arguments() {
        let args = ["scene.py", "exports/output.mp4", "standard", "mp4"].map(str::to_string);
        assert_eq!(
            parse_export_worker_args(&args).unwrap(),
            ExportWorkerArgs {
                script: PathBuf::from("scene.py"),
                output: "exports/output.mp4".to_string(),
                quality: "standard".to_string(),
                format: "mp4".to_string(),
                encoder: VideoEncoder::Auto,
                transparent: false,
                width: 1920,
                height: 1080,
                fit: gaanim_export::prelude::OutputFit::Error,
                from: None,
                to: None,
            }
        );
        let range = [
            "scene.py", "clip.mp4", "draft", "mp4", "--from", "12.5", "--to", "15",
        ]
        .map(str::to_string);
        let range = parse_export_worker_args(&range).unwrap();
        assert_eq!(
            (range.from, range.to),
            (
                Some(ExportBound::Seconds(12.5)),
                Some(ExportBound::Seconds(15.0))
            )
        );
        let markers = [
            "scene.py", "clip.mp4", "draft", "mp4", "--from", "climax", "--to", "fin",
        ]
        .map(str::to_string);
        let markers = parse_export_worker_args(&markers).unwrap();
        assert_eq!(
            (markers.from, markers.to),
            (
                Some(ExportBound::Marker("climax".into())),
                Some(ExportBound::Marker("fin".into()))
            )
        );
        for invalid in [
            [
                "scene.py", "clip.mp4", "draft", "mp4", "--from", "3", "--to", "3",
            ],
            [
                "scene.py", "clip.mp4", "draft", "mp4", "--from", "-1", "--to", "3",
            ],
            [
                "scene.py", "clip.mp4", "draft", "mp4", "--from", "nan", "--to", "3",
            ],
        ] {
            assert!(parse_export_worker_args(&invalid.map(str::to_string)).is_err());
        }

        let transparent =
            ["scene.py", "overlay.webm", "draft", "webm", "--transparent"].map(str::to_string);
        assert!(parse_export_worker_args(&transparent).unwrap().transparent);
        let hardware = [
            "scene.py",
            "output.mp4",
            "draft",
            "mp4",
            "--encoder",
            "nvenc",
        ]
        .map(str::to_string);
        assert_eq!(
            parse_export_worker_args(&hardware).unwrap().encoder,
            VideoEncoder::H264Nvenc
        );
    }

    #[test]
    fn rejects_invalid_export_worker_quality_and_format() {
        let quality = ["scene.py", "out.mp4", "ultra", "mp4"].map(str::to_string);
        assert!(parse_export_worker_args(&quality).is_err());
        let format = ["scene.py", "out.avi", "draft", "avi"].map(str::to_string);
        assert!(parse_export_worker_args(&format).is_err());
        let encoder =
            ["scene.py", "out.mp4", "draft", "mp4", "--encoder", "magic"].map(str::to_string);
        assert!(parse_export_worker_args(&encoder).is_err());
    }

    #[test]
    fn parses_capture_only_diff_without_requiring_a_baseline() {
        let args = [
            "--example",
            "examples/performance_benchmark.py",
            "--current",
            "target/performance/seek",
            "--capture-only",
        ]
        .map(str::to_string);
        let parsed = parse_diff_mode_args(&args).unwrap().unwrap();

        assert!(parsed.capture);
        assert!(parsed.capture_only);
        assert!(!parsed.bless);
        assert_eq!(parsed.current, PathBuf::from("target/performance/seek"));
    }

    #[test]
    fn capture_only_diff_rejects_non_capture_combinations() {
        let no_example = ["--capture-only"].map(str::to_string);
        assert!(parse_diff_mode_args(&no_example).is_err());

        let no_capture = [
            "--example",
            "examples/performance_benchmark.py",
            "--capture-only",
            "--no-capture",
        ]
        .map(str::to_string);
        assert!(parse_diff_mode_args(&no_capture).is_err());

        let bless = [
            "--example",
            "examples/performance_benchmark.py",
            "--capture-only",
            "--bless",
        ]
        .map(str::to_string);
        assert!(parse_diff_mode_args(&bless).is_err());
    }

    #[test]
    fn parses_stop_capture_with_a_selection() {
        let args = [
            "--example",
            ".",
            "--capture-stops",
            "--stops",
            "12,30",
            "--capture-only",
        ]
        .map(str::to_string);
        let parsed = parse_diff_mode_args(&args).unwrap().unwrap();

        assert!(parsed.capture_stops);
        assert_eq!(parsed.stops, Some(vec![12, 30]));
        assert!(parsed.capture_only);
    }

    #[test]
    fn stop_capture_rejects_invalid_combinations() {
        let reject = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            parse_diff_mode_args(&args).unwrap_err()
        };

        assert!(reject(&["--example", ".", "--stops", "1"]).contains("--capture-stops"));
        assert!(reject(&["--capture-stops"]).contains("--example"));
        assert!(
            reject(&["--example", ".", "--capture-stops", "--no-capture"]).contains("--no-capture")
        );
        assert!(reject(&["--example", ".", "--capture-stops", "--stops", "0"]).contains("--stops"));
    }

    #[test]
    fn parses_presentation_launch_options_in_any_order() {
        let args = ["demo.py", "--monitor", "1", "--present"].map(str::to_string);
        assert_eq!(
            parse_launch_args(&args).unwrap(),
            LaunchArgs {
                script_path: Some(PathBuf::from("demo.py")),
                project: None,
                present: true,
                monitor: Some(1),
                selection: Default::default(),
            }
        );
    }

    #[test]
    fn parses_rehearsal_sections_and_start() {
        let args = [".", "--sections", "results, close", "--from", "Results"].map(str::to_string);
        let parsed = parse_launch_args(&args).unwrap();
        assert_eq!(parsed.script_path, Some(PathBuf::from(".")));
        assert_eq!(parsed.selection.sections, vec!["results", "close"]);
        assert_eq!(parsed.selection.from.as_deref(), Some("Results"));
        for args in [
            &["--from", "x"][..],
            &[".", "--sections"],
            &[".", "--sections", "a,,b"],
        ] {
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            assert!(parse_launch_args(&args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn rejects_monitor_without_presentation_mode() {
        let args = ["demo.py", "--monitor", "0"].map(str::to_string);
        assert!(parse_launch_args(&args).is_err());
    }

    #[test]
    fn parses_only_video_and_slides_project_kinds() {
        assert_eq!(
            parse_init_args(&["video".to_string()]).unwrap().kind,
            gaanim_project::ProjectKind::Video
        );
        assert_eq!(
            parse_init_args(&["slides".to_string()]).unwrap().kind,
            gaanim_project::ProjectKind::Slides
        );
        assert!(parse_init_args(&["presentation".to_string()]).is_err());
        assert!(parse_init_args(&["thesis".to_string()]).is_err());
    }

    #[test]
    fn bare_launch_opens_home() {
        assert_eq!(
            parse_launch_args(&[]).unwrap(),
            LaunchArgs {
                script_path: None,
                project: None,
                present: false,
                monitor: None,
                selection: Default::default(),
            }
        );
    }

    #[test]
    fn host_installs_a_primary_2d_camera_for_egui_before_script_replay() {
        let mut world = World::new();
        spawn_host_camera(&mut world);
        assert!(
            world
                .query_filtered::<Entity, With<Camera2d>>()
                .iter(&world)
                .next()
                .is_some()
        );
    }

    #[test]
    fn parses_strict_presentation_check() {
        let args = ["slides.py", "--strict"].map(str::to_string);
        assert_eq!(
            parse_check_args(&args).unwrap(),
            CheckArgs {
                script: PathBuf::from("slides.py"),
                strict: true,
            }
        );
    }

    #[test]
    fn segment_preflight_finds_expected_risks() {
        let mut canvas = gaanim_api::canvas::SceneModel::new(1920, 1080);
        canvas
            .segment_with(
                "Opening",
                None,
                Some("Introduce the topic".to_string()),
                Some("title_slide".to_string()),
            )
            .unwrap();
        canvas.wait(1.0);
        canvas.stop(Some("ready".to_string())).unwrap();
        let report = presentation_preflight(&canvas, "title = \"[TITLE]\"");

        assert!(report.errors.is_empty());
        assert_eq!(report.segment_count, 1);
        assert_eq!(report.stop_count, 1);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("placeholder"))
        );
    }

    #[test]
    fn scene_preflight_warns_about_white_on_white_defaults() {
        let mut canvas = gaanim_api::canvas::SceneModel::new(16.0, 9.0);
        canvas.wait(1.0);
        assert!(
            scene_preflight(&canvas, "").warnings.is_empty(),
            "new scenes use the default theme"
        );
        canvas.clear_theme();
        let report = scene_preflight(&canvas, "");
        assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
        assert!(report.warnings[0].contains("no theme is active"));

        canvas.set_theme("technical").unwrap();
        assert!(scene_preflight(&canvas, "").warnings.is_empty());
    }

    #[test]
    fn diff_rejects_the_removed_no_gui_flag() {
        let args =
            ["--baseline", "baseline", "--current", "current", "--no-gui"].map(str::to_string);
        assert!(parse_diff_mode_args(&args).is_err());
    }

    #[test]
    fn parses_named_diff_flags() {
        let args = [
            "--baseline",
            "baseline",
            "--current",
            "current",
            "--output",
            "report",
            "--pixel-threshold",
            "4",
            "--max-changed-ratio",
            "0.001",
        ]
        .map(str::to_string);

        let parsed = parse_diff_mode_args(&args).unwrap().unwrap();
        assert_eq!(parsed.baseline, PathBuf::from("baseline"));
        assert_eq!(parsed.current, PathBuf::from("current"));
        assert_eq!(parsed.output, PathBuf::from("report"));
        assert_eq!(parsed.options.pixel_threshold, 4);
        assert_eq!(parsed.options.max_changed_ratio, 0.001);
    }

    #[test]
    fn diff_mode_requires_both_inputs() {
        let args = ["--baseline".to_string(), "baseline".to_string()];
        let error = parse_diff_mode_args(&args).unwrap_err();
        assert!(error.contains("--current"));
    }

    #[test]
    fn example_derives_global_snapshot_paths() {
        let args = [
            "--example".to_string(),
            "examples/visual_diff_demo.py".to_string(),
        ];
        let parsed = parse_diff_mode_args(&args).unwrap().unwrap();
        assert_eq!(
            parsed.baseline,
            PathBuf::from("tests/visual/visual_diff_demo/baseline")
        );
        assert_eq!(
            parsed.current,
            PathBuf::from("tests/visual/visual_diff_demo/current")
        );
        assert_eq!(
            parsed.output,
            PathBuf::from("tests/visual/visual_diff_demo/report")
        );
        assert!(parsed.capture);
    }
}
