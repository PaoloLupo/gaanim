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
    ReloadReceiver, ReloadStatus, reload_listener_system, reload_status_overlay_system,
};

fn main() {
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
    let script_path = launch.script_path.clone();
    let project_paths = resolve_project_paths(&script_path);

    // 1. Ensure python312.dll is on PATH by detecting a nearby uv/.venv
    //    (VIRTUAL_ENV, walk-up from script/cwd/exe) or system Python.
    //    No env vars required from the user.
    let venv_root = python_home::ensure_python_available(Some(&script_path));

    // 2. Register the `gaanim_core` module in the embedded interpreter's init
    //    table BEFORE initializing Python, so `import gaanim_core` resolves to
    //    our in-process module (no .pyd needed).
    gaanim_python::register_inittab();

    // 3. Initialize the embedded CPython interpreter.
    Python::initialize();
    if let Some(ref venv) = venv_root {
        python_home::inject_venv_site_packages(venv);
    }

    // 3. Set up the host<->script channel.
    let (payload_tx, payload_rx) = crossbeam_channel::unbounded::<ReloadPayload>();

    // 4. Spawn the script-runner thread (holds the GIL, runs the script).
    let runner = script_runner::ScriptRunner::spawn(script_path.clone(), payload_tx);

    // 5. Spawn the file watcher and extract its channel endpoints.
    let file_watcher::FileWatcher { changed_rx, stop } =
        file_watcher::FileWatcher::spawn(script_path.clone());

    // 6. Bridge watcher events -> script re-run requests in a dedicated thread.
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
        .expect("failed to spawn watcher bridge thread");

    // 7. Build the Bevy app with the editor + renderer + reload wiring.
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: if launch.present {
                "Gaanim — Presentation".to_string()
            } else {
                "Gaanim".to_string()
            },
            resolution: (1280, 720).into(),
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
    }))
    .add_plugins(gaanim_scene::GaanimScenePlugin)
    .add_plugins(gaanim_animation::GaanimAnimationPlugin)
    .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
    .add_plugins(gaanim_text::GaanimTextPlugin)
    .add_plugins(gaanim_api::GaanimApiPlugin)
    .add_plugins(gaanim_renderer::GaanimRendererPlugin)
    .add_plugins(gaanim_editor::GaanimEditorPlugin)
    .insert_resource(gaanim_editor::PresentationMode {
        active: launch.present,
    })
    .insert_resource(project_paths)
    .insert_resource(ReloadReceiver { rx: payload_rx })
    .insert_resource(ReloadStatus::default())
    .add_systems(
        Update,
        (
            reload_listener_system.in_set(gaanim_scene::hierarchy::SceneSet::Input),
            reload_status_overlay_system,
        ),
    );

    app.run();
}

const THESIS_PRESENTATION_TEMPLATE: &str =
    include_str!("../../../templates/thesis_presentation.py");
const VIDEO_PROJECT_TEMPLATE: &str = include_str!("../../../templates/video_project.py");
const PRESENTATION_PROJECT_TEMPLATE: &str =
    include_str!("../../../templates/presentation_project.py");

const PROJECT_GITIGNORE: &str = r#"exports/*
!exports/.gitkeep
snapshots/
__pycache__/
*.mp4
*.webm
*.webp
*.gif
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitTemplate {
    Video,
    Presentation,
    Thesis,
}

impl InitTemplate {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "video" => Ok(Self::Video),
            "presentation" | "slides" => Ok(Self::Presentation),
            "thesis" => Ok(Self::Thesis),
            _ => Err(format!(
                "unknown template `{value}`; available templates: video, presentation, thesis"
            )),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Presentation => "presentation",
            Self::Thesis => "thesis",
        }
    }

    const fn default_directory(self) -> &'static str {
        match self {
            Self::Video => "gaanim-video",
            Self::Presentation => "gaanim-presentation",
            Self::Thesis => "gaanim-thesis",
        }
    }

    const fn source(self) -> &'static str {
        match self {
            Self::Video => VIDEO_PROJECT_TEMPLATE,
            Self::Presentation => PRESENTATION_PROJECT_TEMPLATE,
            Self::Thesis => THESIS_PRESENTATION_TEMPLATE,
        }
    }

    const fn is_presentation(self) -> bool {
        matches!(self, Self::Presentation | Self::Thesis)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InitArgs {
    template: InitTemplate,
    directory: PathBuf,
    force: bool,
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

    if let Err(error) = create_project(&parsed) {
        eprintln!("gaanim init: {error}");
        std::process::exit(2);
    }

    println!(
        "Created {} project: {}",
        parsed.template.name(),
        parsed.directory.display()
    );
    println!("Edit: {}", parsed.directory.join("main.py").display());
    println!("Preview: gaanim {}", parsed.directory.display());
    println!("Check: gaanim check {}", parsed.directory.display());
    if parsed.template.is_presentation() {
        println!(
            "Present: gaanim --present --monitor 1 {}",
            parsed.directory.display()
        );
    } else {
        println!("Export: set GAANIM_EXPORT=exports/video.mp4, then run the project");
    }
    true
}

fn parse_init_args(args: &[String]) -> Result<InitArgs, String> {
    let template = args
        .first()
        .ok_or_else(|| {
            "missing template name; available templates: video, presentation, thesis".to_string()
        })
        .and_then(|value| InitTemplate::parse(value))?;

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

    Ok(InitArgs {
        template,
        directory: directory.unwrap_or_else(|| PathBuf::from(template.default_directory())),
        force,
    })
}

fn create_project(args: &InitArgs) -> Result<(), String> {
    if args.directory.is_file() {
        return Err(format!(
            "{} is a file; choose a project directory",
            args.directory.display()
        ));
    }

    let project_name = args
        .directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(args.template.default_directory());
    let manifest = format!(
        "name = \"{}\"\nkind = \"{}\"\nentry = \"main.py\"\nassets_dir = \"assets\"\noutput_dir = \"exports\"\n",
        escape_manifest_value(project_name),
        args.template.name(),
    );
    let readme = project_readme(project_name, args.template);
    let files = [
        (args.directory.join("main.py"), args.template.source()),
        (args.directory.join("gaanim.toml"), manifest.as_str()),
        (args.directory.join(".gitignore"), PROJECT_GITIGNORE),
        (args.directory.join("README.md"), readme.as_str()),
        (args.directory.join("assets").join(".gitkeep"), ""),
        (args.directory.join("exports").join(".gitkeep"), ""),
    ];

    if !args.force
        && let Some((path, _)) = files.iter().find(|(path, _)| path.exists())
    {
        return Err(format!(
            "{} already exists (use --force to update scaffold files)",
            path.display()
        ));
    }

    for (path, source) in files {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, source)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    }
    Ok(())
}

fn escape_manifest_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn project_readme(name: &str, template: InitTemplate) -> String {
    let presentation = if template.is_presentation() {
        "\n## Presentar\n\n```powershell\ngaanim --present --monitor 1 .\n```\n"
    } else {
        "\n## Exportar\n\n```powershell\n$env:GAANIM_EXPORT = \"exports/video.mp4\"\ngaanim .\nRemove-Item Env:GAANIM_EXPORT\n```\n"
    };
    format!(
        "# {name}\n\nProyecto `{}` generado por Gaanim.\n\n## Editar y previsualizar\n\n\
         Edita `main.py` y ejecuta:\n\n```powershell\ngaanim .\n```\n\n\
         Los recursos van en `assets/`; las salidas generadas van en `exports/`.\n\
         {presentation}\n## Validar\n\n```powershell\ngaanim check .\n```\n",
        template.name()
    )
}

fn print_init_help() {
    println!(
        r#"gaanim init - create a runnable Gaanim project

USAGE:
    gaanim init <TEMPLATE> [DIRECTORY] [--force]

ARGUMENTS:
    video               Animated-video starter
    presentation        Semantic slide-deck starter
    thesis              Complete Spanish thesis-defense project
    DIRECTORY           Project directory (defaults to gaanim-<template>)

OPTIONS:
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
    let script = resolve_project_entry(&parsed.script).unwrap_or_else(|error| {
        eprintln!("gaanim check: {error}");
        std::process::exit(2);
    });

    let venv_root = python_home::ensure_python_available(Some(&script));
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
    let is_presentation = !canvas.presentation_manifest().slides.is_empty();
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
            "  {} slides · {} stops · {:.1} seconds · {}x{}",
            report.slide_count, report.stop_count, report.duration, canvas.width, canvas.height
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
    slide_count: usize,
    stop_count: usize,
    duration: f64,
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn presentation_preflight(canvas: &gaanim_api::canvas::Canvas, source: &str) -> PreflightReport {
    let manifest = canvas.presentation_manifest();
    let mut report = PreflightReport {
        slide_count: manifest.slides.len(),
        stop_count: manifest
            .slides
            .iter()
            .map(|slide| 1 + slide.steps.len())
            .sum(),
        duration: canvas.current_time(),
        ..default()
    };

    if manifest.slides.is_empty() {
        report
            .errors
            .push("no semantic slides; use `scene.slide(...)`".to_string());
        return report;
    }
    let aspect_ratio = canvas.width as f64 / canvas.height.max(1) as f64;
    if (aspect_ratio - 16.0 / 9.0).abs() > 0.02 {
        report.warnings.push(format!(
            "canvas aspect ratio is {:.3}; 16:9 is recommended for projectors",
            aspect_ratio
        ));
    }

    for slide in &manifest.slides {
        if slide
            .notes
            .as_deref()
            .is_none_or(|notes| notes.trim().is_empty())
        {
            report
                .warnings
                .push(format!("slide `{}` has no speaker notes", slide.name));
        }
        if slide.steps.is_empty() {
            report
                .warnings
                .push(format!("slide `{}` has no `step()` pause", slide.name));
        }
        if slide.steps.iter().any(|step| step.name.is_none()) {
            report.warnings.push(format!(
                "slide `{}` contains unnamed steps; names improve Presenter View",
                slide.name
            ));
        }
        let end = slide.end_time.unwrap_or(report.duration);
        if end - slide.start_time <= 1e-5 {
            report
                .errors
                .push(format!("slide `{}` has zero duration", slide.name));
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
        r#"gaanim check - validate a video or presentation project

USAGE:
    gaanim check <SCRIPT_OR_PROJECT> [--strict]

CHECKS:
    entry script and timeline duration
    semantic slides, notes and named reveal steps when present
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
        let script = resolve_project_entry(example).unwrap_or_else(|error| {
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
        let venv_root = python_home::ensure_python_available(Some(&script));
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
            "--bless" => bless = true,
            "--help" | "-h" => return Ok(None),
            _ => return Err(format!("unknown option `{flag}`")),
        }
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
            bless,
        }));
    }

    if bless {
        return Err("--bless requires --example <SCRIPT_OR_PROJECT>".to_string());
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
    script_path: PathBuf,
    present: bool,
    /// Zero-based monitor index used only with `--present`.
    monitor: Option<usize>,
}

fn resolve_project_entry(path: &Path) -> Result<PathBuf, String> {
    if path.is_file() {
        return Ok(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()));
    }
    if !path.exists() {
        return Err(format!("script or project not found: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!(
            "expected a Python script or project directory: {}",
            path.display()
        ));
    }

    let manifest = path.join("gaanim.toml");
    let source = std::fs::read_to_string(&manifest).map_err(|error| {
        format!(
            "could not read project manifest {}: {error}",
            manifest.display()
        )
    })?;
    let entry = manifest_string_value(&source, "entry")
        .ok_or_else(|| format!("{} must declare entry = \"...\"", manifest.display()))?;
    let entry_path = PathBuf::from(&entry);
    if entry_path.is_absolute()
        || entry_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "project entry must stay inside the project directory: {entry:?}"
        ));
    }
    let script = path.join(entry_path);
    if !script.is_file() {
        return Err(format!(
            "project entry does not exist or is not a file: {}",
            script.display()
        ));
    }
    Ok(script.canonicalize().unwrap_or(script))
}

fn manifest_string_value(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let line = line.split('#').next()?.trim();
        let (candidate, value) = line.split_once('=')?;
        if candidate.trim() != key {
            return None;
        }
        let value = value.trim();
        value
            .strip_prefix('"')?
            .strip_suffix('"')
            .map(str::to_owned)
    })
}

fn resolve_project_paths(script_path: &Path) -> gaanim_editor::export::ProjectPaths {
    let script_parent = script_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    // Walk up to find gaanim.toml
    let mut cur = script_parent.clone();
    let mut project_dir: Option<PathBuf> = None;
    let mut manifest_path: Option<PathBuf> = None;
    for _ in 0..5 {
        let cand = cur.join("gaanim.toml");
        if cand.is_file() {
            project_dir = Some(cur.clone());
            manifest_path = Some(cand);
            break;
        }
        if let Some(parent) = cur.parent() {
            cur = parent.to_path_buf();
        } else {
            break;
        }
    }
    if let Some(dir) = project_dir {
        let output_dir_str = manifest_path
            .and_then(|p| {
                std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|s| manifest_string_value(&s, "output_dir"))
            })
            .unwrap_or_else(|| "exports".to_string());
        let output_dir = if Path::new(&output_dir_str).is_absolute() {
            PathBuf::from(&output_dir_str)
        } else {
            dir.join(&output_dir_str)
        };
        // Do not canonicalize output_dir if it doesn't exist yet; keep as absolute-ish
        let project_dir_abs = dir.canonicalize().unwrap_or(dir);
        let output_dir_abs = if output_dir.is_absolute() {
            output_dir
        } else {
            project_dir_abs.join(output_dir)
        };
        gaanim_editor::export::ProjectPaths {
            project_dir: project_dir_abs.clone(),
            output_dir: output_dir_abs,
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
        eprintln!("usage:");
        eprintln!("  gaanim [--present] [--monitor <INDEX>] <SCRIPT_OR_PROJECT>");
        eprintln!("  gaanim init <video|presentation|thesis> [DIRECTORY] [--force]");
        eprintln!("  gaanim check <SCRIPT_OR_PROJECT> [--strict]");
        eprintln!("  gaanim --diff --baseline <DIR> --current <DIR> [OPTIONS]");
        std::process::exit(2);
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        eprintln!("gaanim — GPU-accelerated vector animation engine (hot-reload viewer)");
        eprintln!();
        eprintln!("usage:");
        eprintln!("  gaanim [--present] [--monitor <INDEX>] <SCRIPT_OR_PROJECT>");
        eprintln!("  gaanim init <video|presentation|thesis> [DIRECTORY] [--force]");
        eprintln!("  gaanim check <SCRIPT_OR_PROJECT> [--strict]");
        eprintln!("  gaanim --diff --example <SCRIPT_OR_PROJECT> [OPTIONS]");
        std::process::exit(0);
    }
    let parsed = parse_launch_args(&args).unwrap_or_else(|error| {
        eprintln!("gaanim: {error}");
        std::process::exit(2);
    });
    let path = resolve_project_entry(&parsed.script_path).unwrap_or_else(|error| {
        eprintln!("gaanim: {error}");
        std::process::exit(2);
    });
    LaunchArgs {
        script_path: path,
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
    let script_path = script_path.ok_or_else(|| "missing <SCRIPT_OR_PROJECT>".to_string())?;
    Ok(LaunchArgs {
        script_path,
        present,
        monitor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_presentation_launch_options_in_any_order() {
        let args = ["demo.py", "--monitor", "1", "--present"].map(str::to_string);
        assert_eq!(
            parse_launch_args(&args).unwrap(),
            LaunchArgs {
                script_path: PathBuf::from("demo.py"),
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
    fn parses_thesis_template_with_safe_default_directory() {
        assert_eq!(
            parse_init_args(&["thesis".to_string()]).unwrap(),
            InitArgs {
                template: InitTemplate::Thesis,
                directory: PathBuf::from("gaanim-thesis"),
                force: false,
            }
        );
    }

    #[test]
    fn parses_custom_thesis_directory_and_force() {
        let args = ["thesis", "defense", "--force"].map(str::to_string);
        assert_eq!(
            parse_init_args(&args).unwrap(),
            InitArgs {
                template: InitTemplate::Thesis,
                directory: PathBuf::from("defense"),
                force: true,
            }
        );
        assert!(THESIS_PRESENTATION_TEMPLATE.contains("scene.slide("));
        assert!(THESIS_PRESENTATION_TEMPLATE.contains("notes="));
        assert!(THESIS_PRESENTATION_TEMPLATE.contains("ThesisTemplate("));
        assert!(THESIS_PRESENTATION_TEMPLATE.contains("background=\"#1601FC\""));
        assert!(THESIS_PRESENTATION_TEMPLATE.contains("design.cover("));
    }

    #[test]
    fn parses_video_and_presentation_project_templates() {
        assert_eq!(
            parse_init_args(&["video".to_string()]).unwrap().template,
            InitTemplate::Video
        );
        assert_eq!(
            parse_init_args(&["slides".to_string()]).unwrap().template,
            InitTemplate::Presentation
        );
        assert!(VIDEO_PROJECT_TEMPLATE.contains("scene.render()"));
        assert!(PRESENTATION_PROJECT_TEMPLATE.contains("scene.slide("));
    }

    #[test]
    fn reads_project_manifest_values() {
        let source = r#"
            name = "demo"
            entry = "src/presentation.py" # entry point
        "#;
        assert_eq!(
            manifest_string_value(source, "entry").as_deref(),
            Some("src/presentation.py")
        );
        assert_eq!(manifest_string_value(source, "missing"), None);
    }

    #[test]
    fn parses_strict_presentation_check() {
        let args = ["thesis.py", "--strict"].map(str::to_string);
        assert_eq!(
            parse_check_args(&args).unwrap(),
            CheckArgs {
                script: PathBuf::from("thesis.py"),
                strict: true,
            }
        );
    }

    #[test]
    fn thesis_template_preflight_finds_expected_risks() {
        let mut canvas = gaanim_api::canvas::Canvas::new(1920, 1080);
        let slide = canvas
            .slide(
                "Opening",
                Some("Introduce the topic".to_string()),
                gaanim_api::canvas::SlideTemplate::Title,
            )
            .unwrap();
        canvas.wait(1.0);
        canvas.slide_step(slide, Some("ready".to_string())).unwrap();
        let report = presentation_preflight(&canvas, "title = \"[TITLE]\"");

        assert!(report.errors.is_empty());
        assert_eq!(report.slide_count, 1);
        assert_eq!(report.stop_count, 2);
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
