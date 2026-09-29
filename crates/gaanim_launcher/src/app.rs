//! Everything `gaanim` does beyond the commands in `main.rs`: the editor and
//! Home, and the commands that run scripts (`export`, `check`, `--diff`).
//!
//! A script runs in an embedded CPython interpreter, loaded with the Python
//! plugin (see `python.rs`) the first time one runs. The script *describes*
//! the scene through the fluent API and calls `.render()`, which sends the
//! scene to this process's Bevy event loop instead of opening a window of its
//! own. A file watcher observes the script: on save, the script runs again in
//! the same interpreter and the scene is rebuilt in place, without restarting
//! the window.

use bevy::prelude::*;
use gaanim_api::host::ReloadPayload;
use gaanim_core::console;
use gaanim_editor::cli::{ExportBound, ExportCommand, parse_export_seconds, validate_export_range};
use gaanim_export::encoder::VideoEncoder;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc;

use crate::hot_reload::{
    ReloadReceiver, ReloadStatus, ScriptError, ScriptErrorReceiver, reload_listener_system,
    reload_status_overlay_system, script_error_listener_system, script_error_overlay_system,
};

pub fn run() {
    if crate::runtime_benchmark::dispatch_reload_benchmark_mode() {
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
    if dispatch_check_mode() {
        return;
    }
    if dispatch_diff_mode() {
        return;
    }

    let launch = parse_args();
    // Load Python before the app starts its threads (see `python::runtime`).
    let python = launch
        .script_path
        .as_deref()
        .filter(|path| !gaanim_editor::bundle_player::is_bundle_path(path))
        .map(|script| {
            load_python(script, launch.project.as_ref()).unwrap_or_else(|error| {
                console::error("python", error);
                console::hint(crate::python::install_hint());
                std::process::exit(2);
            })
        });
    #[cfg(target_os = "linux")]
    gaanim_editor::alsa_errors::route_alsa_errors();
    console::banner(if launch.present {
        "Presentation"
    } else {
        "GPU-accelerated vector animation engine"
    });
    let mut app = gaanim_editor::host::host_app(&gaanim_editor::host::HostOptions {
        present: launch.present,
        monitor: launch.monitor,
        selection: launch.selection.clone(),
        ..Default::default()
    });
    app.insert_resource(ReloadStatus::default())
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

    if let Some(bundle) = launch
        .script_path
        .as_deref()
        .filter(|path| gaanim_editor::bundle_player::is_bundle_path(path))
    {
        if let Err(error) = gaanim_editor::bundle_player::open_bundle(app.world_mut(), bundle) {
            console::error("bundle", error);
            std::process::exit(2);
        }
        gaanim_project::record_recent_bundle(bundle);
        app.world_mut()
            .resource_mut::<gaanim_editor::project_hub::ProjectHubState>()
            .active = false;
    } else if let (Some(script_path), Some(python)) = (launch.script_path, python) {
        if let Err(error) =
            start_script_session(app.world_mut(), python, script_path, launch.project)
        {
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
    let command = ExportCommand::parse(&args[1..]).unwrap_or_else(|error| {
        console::error("export", error);
        std::process::exit(2);
    });
    // A playback bundle exports to video without running Python.
    if command
        .input
        .as_deref()
        .is_some_and(gaanim_editor::bundle_player::is_bundle_path)
    {
        console::banner("Export");
        if let Err(error) = gaanim_editor::cli::export_bundle_video(&command) {
            error.exit("export");
        }
        return true;
    }
    let script = command
        .input
        .as_deref()
        .and_then(|path| gaanim_project::resolve_entry(path).ok())
        .unwrap_or_else(|| {
            console::error("export", "a script or project to export is required");
            console::hint("Run `gaanim export --help` for the options.");
            std::process::exit(2);
        });
    let (output, format) = command.output_format().unwrap_or_else(|error| {
        console::error("export", error);
        std::process::exit(2);
    });
    if format == gaanim_bundle::EXTENSION {
        if command.from.is_some()
            || command.to.is_some()
            || command.transparent
            || command.encoder != VideoEncoder::Auto
        {
            console::error(
                "export",
                "a playback bundle records the whole scene; --from, --to, --transparent and --encoder do not apply",
            );
            std::process::exit(2);
        }
        console::banner("Bundle");
        if let Err(error) = run_bundle_export(&script, &output, command.fps) {
            console::error("export", error);
            std::process::exit(1);
        }
        return true;
    }
    if let Err(error) = command.validate_video(&format) {
        console::error("export", error);
        std::process::exit(2);
    }
    console::banner("Export");
    if let Err(error) = run_export_worker(ExportWorkerArgs {
        script,
        output,
        quality: command.quality,
        format,
        encoder: command.encoder,
        transparent: command.transparent,
        width: command.width,
        height: command.height,
        fit: command.fit,
        from: command.from,
        to: command.to,
    }) {
        console::error("export", error);
        std::process::exit(1);
    }
    true
}

/// Record `script` into a playback bundle at `output`.
fn run_bundle_export(script: &Path, output: &str, fps: Option<u32>) -> Result<(), String> {
    let python = crate::python::runtime(script)?;
    let canvas = (python.load_script_canvas)(script)?;
    let config = gaanim_editor::cli::bundle_config(Some(script), &canvas, output, fps);
    gaanim_api::export::record_canvas(canvas, config).map_err(|error| error.to_string())
}

fn dispatch_python_api_validation_mode() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--validate-python-api") {
        return false;
    }
    if args.len() != 2 {
        eprintln!("usage: gaanim --validate-python-api <validator.py>");
        std::process::exit(2);
    }
    let validator = Path::new(&args[1]);
    let python = crate::python::runtime(validator).unwrap_or_else(|error| {
        console::error("python", error);
        console::hint(crate::python::install_hint());
        std::process::exit(2);
    });
    if let Err(error) = (python.validate_python_api)(validator) {
        console::error("python", format!("API validation failed: {error}"));
        std::process::exit(1);
    }
    true
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
            "expected: --export-worker <script.py> <output> <draft|standard|production> <mp4|webm|webp|gif|png|gaanim> [--encoder auto|libx264|nvenc|amf|qsv|vaapi] [--transparent]"
                .to_string(),
        );
    }
    if !matches!(args[2].as_str(), "draft" | "standard" | "production") {
        return Err(format!("unknown export quality '{}'", args[2]));
    }
    if !matches!(
        args[3].as_str(),
        "mp4" | "webm" | "webp" | "gif" | "png" | "gaanim"
    ) {
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
    let python = crate::python::runtime(&worker.script)?;
    let canvas = (python.load_script_canvas)(&worker.script)?;
    if worker.format == gaanim_bundle::EXTENSION {
        let fps = gaanim_editor::cli::bundle_fps(&worker.quality);
        let config = gaanim_editor::cli::bundle_config(
            Some(&worker.script),
            &canvas,
            &worker.output,
            Some(fps),
        );
        return gaanim_api::export::record_canvas(canvas, config)
            .map_err(|error| error.to_string());
    }
    let markers = canvas.markers();
    let markers: Vec<(&str, f64)> = markers
        .iter()
        .map(|marker| (marker.name.as_str(), marker.time))
        .collect();
    let range = gaanim_editor::cli::resolve_export_range(
        worker.from.as_ref(),
        worker.to.as_ref(),
        &markers,
    )?;
    let format =
        gaanim_editor::cli::export_format(&worker.format).expect("validated export format");
    let command = ExportCommand {
        quality: worker.quality.clone(),
        encoder: worker.encoder,
        transparent: worker.transparent,
        width: worker.width,
        height: worker.height,
        fit: worker.fit,
        ..Default::default()
    };
    let config = gaanim_editor::cli::video_config(&command, &worker.output, format, range);
    gaanim_api::export::export_canvas(canvas, config).map_err(|error| error.to_string())
}

/// Prepare a project's authoring environment and load Python for a script.
fn load_python(
    script_path: &Path,
    project: Option<&gaanim_project::ResolvedProject>,
) -> Result<&'static gaanim_editor::python_plugin::PythonPlugin, String> {
    if let Some(project) = project
        && let Err(error) = gaanim_project::provision_authoring_package(&project.root)
    {
        console::warn(
            "python",
            format!("authoring environment not ready: {error}"),
        );
    }
    let hint = project
        .map(|project| project.root.as_path())
        .unwrap_or(script_path);
    crate::python::runtime(hint)
}

/// Install the persistent primary camera before the Bevy event loop begins.
///
/// Canvas replay reuses this Vello camera; creating it during replay is too
/// late for `bevy_egui` to attach its primary context on script launches.
fn start_script_session(
    world: &mut World,
    python: &gaanim_editor::python_plugin::PythonPlugin,
    script_path: PathBuf,
    project: Option<gaanim_project::ResolvedProject>,
) -> Result<(), String> {
    let (payload_tx, payload_rx) = crossbeam_channel::unbounded::<ReloadPayload>();
    let (error_tx, error_rx) = crossbeam_channel::unbounded::<String>();
    let runner = (python.spawn_script)(script_path.clone(), payload_tx, error_tx);
    world.insert_resource(gaanim_editor::narration::ScriptReload(
        runner.asset_reload_handle().into(),
    ));
    let crate::file_watcher::FileWatcher { changed_rx, stop } =
        crate::file_watcher::FileWatcher::spawn(script_path.clone());
    std::thread::Builder::new()
        .name("gaanim-watcher-bridge".into())
        .spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match changed_rx.recv_timeout(std::time::Duration::from_millis(250)) {
                    Ok(crate::file_watcher::ProjectChange::Source) => runner.request_rerun(),
                    Ok(crate::file_watcher::ProjectChange::Assets) => runner.request_asset_reload(),
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
    let session = load_python(&project.entry, Some(&project)).and_then(|python| {
        start_script_session(world, python, project.entry.clone(), Some(project))
    });
    if let Err(error) = session {
        world
            .resource_mut::<gaanim_editor::project_hub::ProjectHubState>()
            .report_open_error(error);
    }
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
    // A playback bundle is checked against its recording, without Python.
    if gaanim_editor::bundle_player::is_bundle_path(&parsed.script) {
        check_bundle(&parsed.script).unwrap_or_else(|error| error.exit("check"));
        return true;
    }
    let script = gaanim_project::resolve_entry(&parsed.script).unwrap_or_else(|error| {
        console::error("check", error);
        std::process::exit(2);
    });

    let python = crate::python::runtime(&script).unwrap_or_else(|error| {
        console::error("check", error);
        console::hint(crate::python::install_hint());
        std::process::exit(2);
    });
    let canvas = (python.load_script_canvas)(&script).unwrap_or_else(|error| {
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

/// Open `path`, describe it, and recompose every frame against its digest.
fn check_bundle(path: &Path) -> Result<(), gaanim_editor::cli::CommandError> {
    let mut bundle = gaanim_bundle::Bundle::open(path).map_err(|error| {
        gaanim_editor::cli::CommandError::Failed(format!("{}: {error}", path.display()))
    })?;
    let scene = &bundle.scene;
    let stops: usize = scene
        .segments
        .iter()
        .map(|segment| segment.stops.len())
        .sum();
    console::info("check", format!("{} · {}", path.display(), scene.title));
    console::detail(
        "Frames",
        format!(
            "{} at {} fps · {:.2} seconds · {}×{}",
            bundle.frame_count(),
            scene.fps,
            scene.duration,
            scene.output_size.0,
            scene.output_size.1
        ),
    );
    console::detail(
        "Structure",
        format!(
            "{} segments · {stops} stops · {} markers · {} audio tracks",
            scene.segments.len(),
            scene.markers.len(),
            scene.audio.len()
        ),
    );
    let mismatched = bundle
        .verify()
        .map_err(|error| gaanim_editor::cli::CommandError::Failed(error.to_string()))?;
    if let Some((index, time)) = mismatched.first() {
        return Err(gaanim_editor::cli::CommandError::Failed(format!(
            "{} of {} frames differ from the recording, first frame {index} at {time:.3}s",
            mismatched.len(),
            bundle.frame_count()
        )));
    }
    console::success("pass", "every frame composes as it was recorded");
    Ok(())
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
    report.warnings.extend(canvas.launched_past_end_warning());
    report.warnings.extend(
        canvas
            .compiled_layout_diagnostics()
            .into_iter()
            .map(|(_, message)| format!("layout: {message}")),
    );

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
    report.warnings.extend(canvas.launched_past_end_warning());
    report.warnings.extend(
        canvas
            .compiled_layout_diagnostics()
            .into_iter()
            .map(|(_, message)| format!("layout: {message}")),
    );
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

/// Handle `gaanim --diff ...` before Python, Bevy, or the editor are initialized.
fn dispatch_diff_mode() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--diff") {
        return false;
    }
    let args: Vec<_> = args.collect();
    gaanim_editor::diff_cli::run_diff(&args, |parsed, example, capture_dir| {
        let script = gaanim_project::resolve_entry(example).unwrap_or_else(|error| {
            console::error("diff", error);
            std::process::exit(2);
        });
        println!(
            "Capturing {} -> {}",
            console::display_path(&script),
            capture_dir.display()
        );
        let python = crate::python::runtime(&script).unwrap_or_else(|error| {
            console::error("diff", error);
            console::hint(crate::python::install_hint());
            std::process::exit(2);
        });
        if parsed.capture_stops {
            capture_stop_snapshots(
                python,
                &script,
                capture_dir,
                parsed.stops.as_deref(),
                &parsed.selection,
            );
        } else if let Err(error) = (python.capture_script_snapshots)(&script, capture_dir) {
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
    })
}

/// Run the script like `gaanim check` and capture the frame shown at each stop.
fn capture_stop_snapshots(
    python: &gaanim_editor::python_plugin::PythonPlugin,
    script: &Path,
    capture_dir: &Path,
    stops: Option<&[usize]>,
    selection: &gaanim_timeline::selection::SegmentSelection,
) {
    let canvas = (python.load_script_canvas)(script).unwrap_or_else(|error| {
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
    gaanim_editor::diff_cli::print_stop_capture(&capture, capture_dir);
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
    // A playback bundle replays without Python or a project.
    if gaanim_editor::bundle_player::is_bundle_path(raw_path) {
        return parsed;
    }
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
                    selection.set_sections(value)?;
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
}
