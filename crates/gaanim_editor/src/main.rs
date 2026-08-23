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
use pyo3::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc;

mod file_watcher;
mod hot_reload;
mod python_home;
mod script_runner;

use hot_reload::{
    ReloadReceiver, ReloadStatus, ScriptError, ScriptErrorReceiver, reload_listener_system,
    reload_status_overlay_system, script_error_listener_system, script_error_overlay_system,
};

fn main() {
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
            .set(gaanim_scene::gaanim_asset_plugin()),
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
    .insert_resource(gaanim_media::VideoPreviewAudioEnabled(true))
    .insert_resource(gaanim_editor::PresentationMode {
        active: launch.present,
    })
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

    // bevy_egui creates its primary context when the application starts. Keep
    // this camera alive for both the project hub and script launches, so a
    // script payload can reuse it instead of creating the egui camera after
    // the first frame.
    spawn_host_camera(app.world_mut());

    if let Some(script_path) = launch.script_path {
        if let Err(error) = start_script_session(app.world_mut(), script_path, launch.project) {
            eprintln!("gaanim: {error}");
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
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                index += 1;
                output = args.get(index).cloned();
            }
            "--quality" => {
                index += 1;
                quality = args.get(index).cloned().unwrap_or_default();
            }
            value if value.starts_with('-') => {
                eprintln!("gaanim export: unknown option `{value}`");
                std::process::exit(2);
            }
            value if script.is_none() => script = Some(PathBuf::from(value)),
            value => {
                eprintln!("gaanim export: unexpected argument `{value}`");
                std::process::exit(2);
            }
        }
        index += 1;
    }
    let script = script
        .and_then(|path| gaanim_project::resolve_entry(&path).ok())
        .unwrap_or_else(|| {
            eprintln!("usage: gaanim export <SCRIPT_OR_PROJECT> --output <FILE> [--quality draft|standard|production]");
            std::process::exit(2);
        });
    let output = output.unwrap_or_else(|| {
        eprintln!("gaanim export: --output is required");
        std::process::exit(2);
    });
    if !matches!(quality.as_str(), "draft" | "standard" | "production") {
        eprintln!("gaanim export: quality must be draft, standard, or production");
        std::process::exit(2);
    }
    let format = Path::new(&output)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|format| matches!(format.as_str(), "mp4" | "webm" | "webp" | "gif" | "png"))
        .unwrap_or_else(|| {
            eprintln!("gaanim export: output extension must be mp4, webm, webp, gif, or png");
            std::process::exit(2);
        });
    if let Err(error) = run_export_worker(ExportWorkerArgs {
        script,
        output,
        quality,
        format,
    }) {
        eprintln!("gaanim export: {error}");
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
        eprintln!("gaanim Python API validation failed: {error}");
        std::process::exit(1);
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExportWorkerArgs {
    script: PathBuf,
    output: String,
    quality: String,
    format: String,
}

fn parse_export_worker_args(args: &[String]) -> Result<ExportWorkerArgs, String> {
    if args.len() != 4 {
        return Err(
            "expected: --export-worker <script.py> <output> <draft|standard|production> <mp4|webm|webp|gif|png>"
                .to_string(),
        );
    }
    if !matches!(args[2].as_str(), "draft" | "standard" | "production") {
        return Err(format!("unknown export quality '{}'", args[2]));
    }
    if !matches!(args[3].as_str(), "mp4" | "webm" | "webp" | "gif" | "png") {
        return Err(format!("unknown export format '{}'", args[3]));
    }
    Ok(ExportWorkerArgs {
        script: PathBuf::from(&args[0]),
        output: args[1].clone(),
        quality: args[2].clone(),
        format: args[3].clone(),
    })
}

fn dispatch_export_worker_mode() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--export-worker") {
        return false;
    }
    let worker = parse_export_worker_args(&args[1..]).unwrap_or_else(|error| {
        eprintln!("gaanim export worker: {error}");
        std::process::exit(2);
    });
    if let Err(error) = run_export_worker(worker) {
        eprintln!("gaanim export worker: {error}");
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
    config.width = canvas.width;
    config.height = canvas.height;
    config.aspect_ratio = gaanim_export::prelude::AspectRatioPreset::Custom;
    config.format = format;
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
        eprintln!("gaanim: authoring environment not ready: {error}");
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
    let file_watcher::FileWatcher { changed_rx, stop } =
        file_watcher::FileWatcher::spawn(script_path.clone());
    std::thread::Builder::new()
        .name("gaanim-watcher-bridge".into())
        .spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match changed_rx.recv_timeout(std::time::Duration::from_millis(250)) {
                    Ok(()) => runner.request_rerun(),
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
        print_init_help();
        return true;
    }
    let parsed = parse_init_args(&args).unwrap_or_else(|error| {
        eprintln!("gaanim init: {error}");
        eprintln!("Run `gaanim init --help` for usage.");
        std::process::exit(2);
    });

    let project = gaanim_project::create_project(&parsed).unwrap_or_else(|error| {
        eprintln!("gaanim init: {error}");
        std::process::exit(2);
    });

    match gaanim_project::provision_authoring_package(&project.root) {
        Ok(venv) => println!("Python authoring environment: {}", venv.display()),
        Err(error) => eprintln!("gaanim init: authoring environment not ready: {error}"),
    }

    println!(
        "Created {} project: {}",
        parsed.kind.name(),
        project.root.display()
    );
    println!("Edit: {}", project.entry.display());
    println!("Preview: gaanim {}", project.root.display());
    println!("Check: gaanim check {}", project.root.display());
    if parsed.kind.is_slides() {
        println!(
            "Present: gaanim --present --monitor 1 {}",
            project.root.display()
        );
    } else {
        println!(
            "Export: gaanim export {} --output exports/video.mp4 --quality production",
            project.root.display()
        );
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

fn print_init_help() {
    println!(
        r#"gaanim init - create a runnable Gaanim project

USAGE:
    gaanim init <KIND> [DIRECTORY] [--force]

ARGUMENTS:
    video               Animated-video starter
    slides              Presentation segments starter
    DIRECTORY           Project directory (defaults to gaanim-<kind>)

OPTIONS:
    Creates a bare uv project pinned to Python 3.14
    --force             Update scaffold files without deleting user assets
    -h, --help          Print this help"#
    );
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
        print_check_help();
        return true;
    }
    let parsed = parse_check_args(&args).unwrap_or_else(|error| {
        eprintln!("gaanim check: {error}");
        eprintln!("Run `gaanim check --help` for usage.");
        std::process::exit(2);
    });
    let script = gaanim_project::resolve_entry(&parsed.script).unwrap_or_else(|error| {
        eprintln!("gaanim check: {error}");
        std::process::exit(2);
    });

    let probe = gaanim_project::EnvironmentProbe::detect(Some(&script));
    let venv_root = gaanim_project::activate_environment(&probe).unwrap_or_else(|error| {
        eprintln!("gaanim check: {error}");
        std::process::exit(2);
    });
    gaanim_python::register_inittab();
    Python::initialize();
    if let Some(ref venv) = venv_root {
        python_home::inject_venv_site_packages(venv);
    }
    let canvas = script_runner::load_script_canvas(&script).unwrap_or_else(|error| {
        eprintln!("gaanim check: could not load project: {error}");
        std::process::exit(2);
    });
    let source = std::fs::read_to_string(&script).unwrap_or_default();
    let is_presentation = canvas.has_presentation_features();
    let report = if is_presentation {
        presentation_preflight(&canvas, &source)
    } else {
        scene_preflight(&canvas, &source)
    };

    println!(
        "{} preflight: {}",
        if is_presentation {
            "Presentation"
        } else {
            "Scene"
        },
        script.display()
    );
    if is_presentation {
        println!(
            "  {} segments · {} stops · {:.1} seconds · {}x{}",
            report.segment_count, report.stop_count, report.duration, canvas.width, canvas.height
        );
    } else {
        println!(
            "  {:.1} seconds · {}x{}",
            report.duration, canvas.width, canvas.height
        );
    }
    for error in &report.errors {
        println!("  ERROR: {error}");
    }
    for warning in &report.warnings {
        println!("  WARN: {warning}");
    }
    if report.errors.is_empty() && report.warnings.is_empty() {
        println!("  PASS: ready to present");
    } else if report.errors.is_empty() {
        println!(
            "  PASS with {} warning{}",
            report.warnings.len(),
            if report.warnings.len() == 1 { "" } else { "s" }
        );
    } else {
        println!(
            "  FAIL: {} error{}",
            report.errors.len(),
            if report.errors.len() == 1 { "" } else { "s" }
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

fn presentation_preflight(canvas: &gaanim_api::canvas::Canvas, source: &str) -> PreflightReport {
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

    if manifest.segments.is_empty() {
        report
            .errors
            .push("no segments; use `scene.segment(...)`".to_string());
        return report;
    }
    let aspect_ratio = canvas.width as f64 / canvas.height.max(1) as f64;
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

fn scene_preflight(canvas: &gaanim_api::canvas::Canvas, source: &str) -> PreflightReport {
    let mut report = PreflightReport {
        duration: canvas.current_time(),
        ..default()
    };
    if canvas.width == 0 || canvas.height == 0 {
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

fn print_check_help() {
    println!(
        r#"gaanim check - validate a video or slides project

USAGE:
    gaanim check <SCRIPT_OR_PROJECT> [--strict]

CHECKS:
    entry script and timeline duration
    semantic segments, notes and named stops when present
    16:9 projector aspect ratio for presentations
    unresolved template placeholders

OPTIONS:
    --strict            Return failure when warnings are present
    -h, --help          Print this help"#
    );
}

#[derive(Debug)]
struct DiffModeArgs {
    baseline: PathBuf,
    current: PathBuf,
    output: PathBuf,
    options: gaanim_diff::CompareOptions,
    open_gui: bool,
    example: Option<PathBuf>,
    capture: bool,
    capture_only: bool,
    bless: bool,
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
            print_diff_help();
            return true;
        }
        Err(error) => {
            eprintln!("gaanim --diff: {error}");
            eprintln!("Run `gaanim --diff --help` for usage.");
            std::process::exit(2);
        }
    };

    if let Some(example) = &parsed.example
        && (parsed.capture || parsed.bless)
    {
        let script = gaanim_project::resolve_entry(example).unwrap_or_else(|error| {
            eprintln!("gaanim --diff: {error}");
            std::process::exit(2);
        });
        let capture_dir = if parsed.bless {
            &parsed.baseline
        } else {
            &parsed.current
        };
        println!(
            "Capturing {} -> {}",
            script.display(),
            capture_dir.display()
        );
        let probe = gaanim_project::EnvironmentProbe::detect(Some(&script));
        let venv_root = gaanim_project::activate_environment(&probe).unwrap_or_else(|error| {
            eprintln!("gaanim --diff: {error}");
            std::process::exit(2);
        });
        gaanim_python::register_inittab();
        Python::initialize();
        if let Some(ref venv) = venv_root {
            python_home::inject_venv_site_packages(venv);
        }
        if let Err(error) = script_runner::capture_script_snapshots(&script, capture_dir) {
            eprintln!("gaanim --diff: snapshot capture failed: {error}");
            std::process::exit(2);
        }
        if !capture_dir.join(gaanim_diff::MANIFEST_FILE).is_file() {
            eprintln!(
                "gaanim --diff: {} did not call scene.snapshots(...)",
                script.display()
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

    let report = match gaanim_diff::compare_directories(
        &parsed.baseline,
        &parsed.current,
        &parsed.output,
        parsed.options,
    ) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("gaanim --diff: {error}");
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
    println!("Report: {}", parsed.output.join("report.json").display());

    let passed = report.passed;
    if parsed.open_gui
        && let Err(error) = gaanim_diff::viewer::run(
            report,
            parsed.baseline,
            parsed.current,
            parsed.output,
            parsed.options,
        )
    {
        eprintln!("gaanim --diff: could not open egui viewer: {error}");
        std::process::exit(2);
    }

    std::process::exit(if passed { 0 } else { 1 });
}

fn parse_diff_mode_args(args: &[String]) -> Result<Option<DiffModeArgs>, String> {
    let mut baseline = None;
    let mut current = None;
    let mut output = None;
    let mut example = None;
    let mut tests_root = PathBuf::from("tests/visual");
    let mut options = gaanim_diff::CompareOptions::default();
    let mut open_gui = true;
    let mut capture = None;
    let mut capture_only = false;
    let mut bless = false;
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
            "--no-gui" => open_gui = false,
            "--no-capture" => capture = Some(false),
            "--capture-only" => capture_only = true,
            "--bless" => bless = true,
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

    if let Some(example) = example {
        let case_dir = visual_test_case_dir(&tests_root, &example)?;
        return Ok(Some(DiffModeArgs {
            baseline: baseline.unwrap_or_else(|| case_dir.join("baseline")),
            current: current.unwrap_or_else(|| case_dir.join("current")),
            output: output.unwrap_or_else(|| case_dir.join("report")),
            options,
            open_gui,
            example: Some(example),
            capture: capture.unwrap_or(true),
            capture_only,
            bless,
        }));
    }

    if bless {
        return Err("--bless requires --example <SCRIPT_OR_PROJECT>".to_string());
    }
    if capture_only {
        return Err("--capture-only requires --example <SCRIPT_OR_PROJECT>".to_string());
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
        open_gui,
        example: None,
        capture: false,
        capture_only: false,
        bless: false,
    }))
}

fn visual_test_case_dir(tests_root: &Path, example: &Path) -> Result<PathBuf, String> {
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

fn print_diff_help() {
    println!(
        r#"gaanim --diff - native egui visual regression viewer

USAGE:
    gaanim --diff --example <SCRIPT_OR_PROJECT> [OPTIONS]
    gaanim --diff --baseline <DIR> --current <DIR> [OPTIONS]

OPTIONS:
    -e, --example <SCRIPT_OR_PROJECT>
                                     Capture and compare one project automatically
        --tests-root <DIR>           Global snapshot root (default: tests/visual)
        --bless                      Capture this example as its baseline and exit
        --capture-only               Capture into current/ (or --current) and exit
        --no-capture                 Reuse the example's existing current snapshots
    -b, --baseline <DIR>            Known-good snapshot directory
    -c, --current <DIR>             Candidate snapshot directory
    -o, --output <DIR>              Override the generated report directory
        --pixel-threshold <0..255>  Ignored per-channel difference (default: 2)
        --max-changed-ratio <0..1>  Allowed changed-pixel fraction (default: 0)
        --no-gui                    Generate reports without opening egui
    -h, --help                      Print this help"#
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchArgs {
    script_path: Option<PathBuf>,
    project: Option<gaanim_project::ResolvedProject>,
    present: bool,
    /// Zero-based monitor index used only with `--present`.
    monitor: Option<usize>,
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
        };
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        eprintln!("gaanim — GPU-accelerated vector animation engine (hot-reload viewer)");
        eprintln!();
        eprintln!("usage:");
        eprintln!("  gaanim");
        eprintln!("  gaanim [--present] [--monitor <INDEX>] <SCRIPT_OR_PROJECT>");
        eprintln!("  gaanim init <video|slides> [DIRECTORY] [--force]");
        eprintln!("  gaanim export <SCRIPT_OR_PROJECT> --output <FILE>");
        eprintln!("  gaanim check <SCRIPT_OR_PROJECT> [--strict]");
        eprintln!("  gaanim --diff --example <SCRIPT_OR_PROJECT> [OPTIONS]");
        std::process::exit(0);
    }
    let parsed = parse_launch_args(&args).unwrap_or_else(|error| {
        eprintln!("gaanim: {error}");
        std::process::exit(2);
    });
    let Some(raw_path) = parsed.script_path.as_ref() else {
        return parsed;
    };
    let (path, project) = if raw_path.is_dir() {
        let project = gaanim_project::resolve_project(raw_path).unwrap_or_else(|error| {
            eprintln!("gaanim: {error}");
            std::process::exit(2);
        });
        (project.entry.clone(), Some(project))
    } else {
        let path = gaanim_project::resolve_entry(raw_path).unwrap_or_else(|error| {
            eprintln!("gaanim: {error}");
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
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        index += 1;
        match arg.as_str() {
            "--present" => present = true,
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
    Ok(LaunchArgs {
        script_path,
        project: None,
        present,
        monitor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            }
        );
    }

    #[test]
    fn rejects_invalid_export_worker_quality_and_format() {
        let quality = ["scene.py", "out.mp4", "ultra", "mp4"].map(str::to_string);
        assert!(parse_export_worker_args(&quality).is_err());
        let format = ["scene.py", "out.avi", "draft", "avi"].map(str::to_string);
        assert!(parse_export_worker_args(&format).is_err());
    }

    #[test]
    fn parses_capture_only_diff_without_requiring_a_baseline() {
        let args = [
            "--example",
            "examples/performance_benchmark.py",
            "--current",
            "target/performance/seek",
            "--capture-only",
            "--no-gui",
        ]
        .map(str::to_string);
        let parsed = parse_diff_mode_args(&args).unwrap().unwrap();

        assert!(parsed.capture);
        assert!(parsed.capture_only);
        assert!(!parsed.bless);
        assert!(!parsed.open_gui);
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
    fn parses_presentation_launch_options_in_any_order() {
        let args = ["demo.py", "--monitor", "1", "--present"].map(str::to_string);
        assert_eq!(
            parse_launch_args(&args).unwrap(),
            LaunchArgs {
                script_path: Some(PathBuf::from("demo.py")),
                project: None,
                present: true,
                monitor: Some(1),
            }
        );
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
        let mut canvas = gaanim_api::canvas::Canvas::new(1920, 1080);
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
            "--no-gui",
        ]
        .map(str::to_string);

        let parsed = parse_diff_mode_args(&args).unwrap().unwrap();
        assert_eq!(parsed.baseline, PathBuf::from("baseline"));
        assert_eq!(parsed.current, PathBuf::from("current"));
        assert_eq!(parsed.output, PathBuf::from("report"));
        assert_eq!(parsed.options.pixel_threshold, 4);
        assert_eq!(parsed.options.max_changed_ratio, 0.001);
        assert!(!parsed.open_gui);
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
