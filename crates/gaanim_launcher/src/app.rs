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
    reload_ready_system, reload_status_overlay_system, script_error_listener_system,
    script_error_overlay_system,
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
    if crate::inspect::dispatch_inspect_mode() {
        return;
    }
    if dispatch_diff_mode() {
        return;
    }

    let launch = parse_args();
    // The window's GPU backends are found while Python loads.
    gaanim_renderer::adapter::start_window_backends_probe();
    // Load Python before the app starts its threads (see `python::runtime`),
    // and run the script while the app and its GPU start.
    let script = launch
        .script_path
        .as_deref()
        .filter(|path| !gaanim_editor::bundle_player::is_bundle_path(path))
        .map(|script| {
            let python = load_python(script, launch.project.as_ref()).unwrap_or_else(|error| {
                console::error("python", error);
                console::hint(crate::python::install_hint());
                std::process::exit(2);
            });
            ScriptLaunch::start(python, script.to_path_buf())
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
        duration: launch.duration,
        ..Default::default()
    });
    crate::startup::mark("app setup");
    app.insert_resource(ReloadStatus::default())
        .insert_resource(ScriptError::default())
        .add_systems(Last, crate::startup::first_frame_system)
        .add_systems(
            Update,
            (
                script_error_listener_system.in_set(gaanim_scene::hierarchy::SceneSet::Input),
                reload_listener_system.in_set(gaanim_scene::hierarchy::SceneSet::Input),
                // After the timeline seek of the Animation phase.
                reload_ready_system.in_set(gaanim_scene::hierarchy::SceneSet::Camera),
            ),
        )
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            (reload_status_overlay_system, script_error_overlay_system),
        )
        .add_systems(Update, open_project_request_system)
        .add_systems(
            Update,
            (
                crate::control::control_command_system
                    .in_set(gaanim_scene::hierarchy::SceneSet::Input)
                    .after(reload_listener_system),
                crate::control::playhead_event_system
                    .in_set(gaanim_scene::hierarchy::SceneSet::Camera),
            ),
        );
    if let Some(address) = launch.listen
        && let Err(error) =
            crate::control::listen(app.world_mut(), address, launch.script_path.as_deref())
    {
        console::error("listen", error);
        std::process::exit(2);
    }

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
    } else if let Some(script) = script {
        if let Err(error) = attach_script_session(app.world_mut(), script, launch.project) {
            console::error("project", error);
            std::process::exit(2);
        }
    } else {
        let mut hub = app
            .world_mut()
            .resource_mut::<gaanim_editor::project_hub::ProjectHubState>();
        match launch.workspace {
            Some(workspace) => hub.show_workspace(workspace),
            None => hub.show(),
        }
    }

    app.run();
}

/// The command line after the program, without `--message-format
/// <FORMAT>`, which `check`, `export` and the preview take: it sets how the
/// console writes (see [`console::set_message_format_json`]).
fn command_args() -> Vec<String> {
    match take_message_format(std::env::args().skip(1).collect()) {
        Ok((args, json)) => {
            console::set_message_format_json(json);
            args
        }
        Err(error) => {
            console::error("usage", error);
            std::process::exit(2);
        }
    }
}

/// `args` without its `--message-format <FORMAT>` (or
/// `--message-format=<FORMAT>`), and whether the format is JSON.
pub(crate) fn take_message_format(mut args: Vec<String>) -> Result<(Vec<String>, bool), String> {
    let mut json = false;
    while let Some(index) = args
        .iter()
        .position(|arg| arg == "--message-format" || arg.starts_with("--message-format="))
    {
        let flag = args.remove(index);
        let value = match flag.split_once('=') {
            Some((_, value)) => Some(value.to_string()),
            None => (index < args.len()).then(|| args.remove(index)),
        };
        json = console::parse_message_format(value.as_deref())?;
    }
    Ok((args, json))
}

fn dispatch_export_mode() -> bool {
    let args = command_args();
    if args.first().map(String::as_str) != Some("export") {
        return false;
    }
    let mut command = ExportCommand::parse(&args[1..]).unwrap_or_else(|error| {
        console::error("export", error);
        std::process::exit(2);
    });
    if let Err(error) = command.validate_placeholders() {
        console::error("export", error);
        std::process::exit(2);
    }
    let output_has = |placeholder: &str| {
        command
            .output
            .as_deref()
            .is_some_and(|output| output.contains(placeholder))
    };
    // A playback bundle exports to video without running Python.
    if command
        .input
        .as_deref()
        .is_some_and(gaanim_editor::bundle_player::is_bundle_path)
    {
        if output_has("{member}")
            || output_has("{segment}")
            || command.members.is_some()
            || command.sections.is_some()
        {
            console::error(
                "export",
                "a playback bundle exports as a whole; {member}, {segment}, --members and \
                 --sections do not apply",
            );
            std::process::exit(2);
        }
        console::banner("Export");
        let pdf = command
            .output_format()
            .is_ok_and(|(_, format)| format == gaanim_editor::cli::PDF_EXTENSION);
        if pdf {
            match gaanim_editor::cli::export_bundle_pdf(&command, None) {
                Ok(pages) => report_deck(&command, pages),
                Err(error) => error.exit("export"),
            }
        } else if let Err(error) = gaanim_editor::cli::export_bundle_video(&command) {
            error.exit("export");
        }
        return true;
    }
    // A workspace exports each of its members, in a process of its own.
    if let Some(workspace) = exported_workspace(&command) {
        std::process::exit(export_members(&workspace, &command, &args[1..]));
    }
    if output_has("{member}") || command.members.is_some() {
        console::error(
            "export",
            "{member} and --members export the members of a workspace; pass its folder",
        );
        std::process::exit(2);
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
    let project = gaanim_project::locate_script(&script)
        .unwrap_or_else(|error| {
            console::error("export", error);
            std::process::exit(2);
        })
        .project;
    // Without --output, a project exports to its output_dir.
    if command.output.is_none()
        && let Some(project) = &project
    {
        command.output = Some(gaanim_editor::cli::default_output(
            project, &script, &command,
        ));
    }
    let (output, format) = command.output_format().unwrap_or_else(|error| {
        console::error("export", error);
        std::process::exit(2);
    });
    let segments = output.contains("{segment}");
    if segments
        && (format == gaanim_bundle::EXTENSION || format == gaanim_editor::cli::PDF_EXTENSION)
    {
        console::error(
            "export",
            format!("a .{format} holds the whole scene; {{segment}} exports videos or PNGs"),
        );
        std::process::exit(2);
    }
    if format == gaanim_bundle::EXTENSION {
        if command.from.is_some()
            || command.to.is_some()
            || !command.frames.is_empty()
            || command.transparent
            || command.notes
            || command.encoder != VideoEncoder::Auto
        {
            console::error(
                "export",
                "a playback bundle records the whole scene; --from, --to, --frame, --transparent, --notes and --encoder do not apply",
            );
            std::process::exit(2);
        }
        let fps = command.recording_fps().unwrap_or_else(|error| {
            console::error("export", error);
            std::process::exit(2);
        });
        console::banner("Bundle");
        if let Err(error) = run_bundle_export(&script, &output, fps) {
            console::error("export", error);
            std::process::exit(1);
        }
        return true;
    }
    if format == gaanim_editor::cli::PDF_EXTENSION {
        if let Err(error) = command.validate_pdf() {
            console::error("export", error);
            std::process::exit(2);
        }
        console::banner("Export");
        let pages = crate::python::runtime(&script)
            .and_then(|python| (python.load_script_canvas)(&script))
            .and_then(|canvas| {
                gaanim_editor::cli::export_slides_pdf(
                    canvas,
                    &command,
                    &output,
                    Some(&script),
                    None,
                )
            });
        match pages {
            Ok(pages) => report_deck(&command, pages),
            Err(error) => {
                console::error("export", error);
                std::process::exit(1);
            }
        }
        return true;
    }
    if let Err(error) = command.validate_video(&format) {
        console::error("export", error);
        std::process::exit(2);
    }
    console::banner("Export");
    if !command.frames.is_empty() {
        let written = crate::python::runtime(&script)
            .and_then(|python| (python.load_script_canvas)(&script))
            .and_then(|canvas| gaanim_editor::cli::export_frames(canvas, &command, &output));
        match written {
            Ok(paths) => {
                for path in paths {
                    console::success("export", format!("wrote {}", path.display()));
                }
            }
            Err(error) => {
                console::error("export", error);
                std::process::exit(1);
            }
        }
        return true;
    }
    if segments {
        match export_segments(&script, &command, &output, &format) {
            Ok(count) => console::success(
                "export",
                format!(
                    "exported {count} segment{}",
                    if count == 1 { "" } else { "s" }
                ),
            ),
            Err(error) => {
                console::error("export", error);
                std::process::exit(1);
            }
        }
        return true;
    }
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
        fps: command.fps,
        from: command.from,
        to: command.to,
        notes: false,
    }) {
        console::error("export", error);
        std::process::exit(1);
    }
    true
}

/// The workspace whose members `gaanim export` exports: the input's own,
/// or the one a project at the workspace root declares when `--members` or
/// `{member}` ask for members.
fn exported_workspace(command: &ExportCommand) -> Option<gaanim_project::Workspace> {
    let input = command.input.as_deref().filter(|input| input.is_dir())?;
    let members = command.members.is_some()
        || command
            .output
            .as_deref()
            .is_some_and(|output| output.contains("{member}"));
    match gaanim_project::resolve_folder(input).ok()? {
        gaanim_project::ProjectFolder::Workspace(workspace) => Some(workspace),
        gaanim_project::ProjectFolder::Project(project) if members => project
            .workspace
            .filter(|workspace| workspace.root == project.root),
        gaanim_project::ProjectFolder::Project(_) => None,
    }
}

/// Export every member of `workspace` (or those `--members` names), each
/// in a `gaanim export` of its own with the same options: `{member}` in
/// `--output` becomes the member's path, and without `--output` each
/// member writes to its own output_dir. The exit code: 1 when one fails.
fn export_members(
    workspace: &gaanim_project::Workspace,
    command: &ExportCommand,
    args: &[String],
) -> i32 {
    let members = workspace
        .select_members(command.members.as_deref())
        .unwrap_or_else(|error| {
            console::error("export", error);
            std::process::exit(2);
        });
    if members.is_empty() {
        console::error(
            "export",
            format!("workspace {} has no members", workspace.name),
        );
        return 2;
    }
    if let Some(output) = &command.output
        && !output.contains("{member}")
        && members.len() > 1
    {
        console::error(
            "export",
            format!(
                "--output names one file for {} members; put {{member}} in it \
                 (exports/{{member}}.mp4), or leave it out to export each member to its \
                 output_dir",
                members.len()
            ),
        );
        return 2;
    }
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            console::error("export", format!("could not find gaanim: {error}"));
            return 2;
        }
    };
    let json = console::message_format_json();
    let mut failed = Vec::new();
    for member in &members {
        if let Err(error) = &member.project {
            console::error("export", format!("{}: {error}", member.path));
            failed.push(member.path.as_str());
            continue;
        }
        let output = command
            .output
            .as_deref()
            .map(|output| gaanim_editor::cli::fill_placeholder(output, "{member}", &member.path));
        console::info(
            "export",
            format!(
                "{} · {}",
                member.path,
                output.as_deref().unwrap_or("to its output_dir")
            ),
        );
        let mut child = std::process::Command::new(&executable);
        child.arg("export").args(gaanim_editor::cli::member_args(
            args,
            &member.root,
            output.as_deref(),
        ));
        if json {
            child.args(["--message-format", "json"]);
        }
        if !child.status().is_ok_and(|status| status.success()) {
            failed.push(member.path.as_str());
        }
    }
    let exported = members.len() - failed.len();
    if json {
        console::emit_json(&[
            ("type", "workspace".into()),
            ("name", workspace.name.as_str().into()),
            ("members", members.len().into()),
            ("passed", exported.into()),
            ("failed", failed.len().into()),
        ]);
    } else if failed.is_empty() {
        console::success(
            "export",
            format!(
                "workspace {} · exported {exported} of {} members",
                workspace.name,
                members.len()
            ),
        );
    } else {
        console::error(
            "export",
            format!(
                "workspace {} · exported {exported} of {} members · failed: {}",
                workspace.name,
                members.len(),
                failed.join(", ")
            ),
        );
    }
    i32::from(!failed.is_empty())
}

/// Export each segment `{segment}` asks for (all, or those of
/// `--sections`) to `output` with its name, running the script once.
fn export_segments(
    script: &Path,
    command: &ExportCommand,
    output: &str,
    format: &str,
) -> Result<usize, String> {
    let python = crate::python::runtime(script)?;
    let canvas = (python.load_script_canvas)(script)?;
    let segments = gaanim_editor::cli::segment_exports(&canvas, command.sections.as_deref())?;
    let format = gaanim_editor::cli::export_format(format).expect("validated export format");
    for (name, start, end) in &segments {
        let path = gaanim_editor::cli::fill_placeholder(output, "{segment}", name);
        console::info("export", format!("segment {name} · {path}"));
        let config =
            gaanim_editor::cli::video_config(command, &path, format, (Some(*start), Some(*end)));
        gaanim_api::export::export_canvas(canvas.clone(), config)
            .map_err(|error| format!("segment {name}: {error}"))?;
    }
    Ok(segments.len())
}

/// Report the PDF deck `command` wrote, with its number of pages.
fn report_deck(command: &ExportCommand, pages: usize) {
    let output = command.output.as_deref().unwrap_or_default();
    let noun = if pages == 1 { "page" } else { "pages" };
    console::success("export", format!("wrote {output} ({pages} {noun})"));
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
    /// Frame rate of the video (or whole rate of the bundle); `None` takes
    /// the quality preset's.
    fps: Option<gaanim_export::prelude::FrameRate>,
    /// Exported time range in seconds or names (markers, segments, stops);
    /// `None` means the scene start/end.
    from: Option<ExportBound>,
    to: Option<ExportBound>,
    /// A PDF deck prints each slide's speaker notes.
    notes: bool,
}

fn parse_export_worker_args(args: &[String]) -> Result<ExportWorkerArgs, String> {
    if args.len() < 4 {
        return Err(
            "expected: --export-worker <script.py> <output> <draft|standard|production> <mp4|webm|mov|webp|gif|png|gaanim|pdf> [--encoder auto|libx264|nvenc|amf|qsv|vaapi] [--transparent] [--fps <rate>] [--notes]"
                .to_string(),
        );
    }
    if !matches!(args[2].as_str(), "draft" | "standard" | "production") {
        return Err(format!("unknown export quality '{}'", args[2]));
    }
    if !gaanim_editor::cli::EXPORT_EXTENSIONS.contains(&args[3].as_str()) {
        return Err(format!("unknown export format '{}'", args[3]));
    }
    let mut encoder = VideoEncoder::Auto;
    let mut transparent = false;
    let mut width = 1920_u32;
    let mut height = 1080_u32;
    let mut fit = gaanim_export::prelude::OutputFit::Error;
    let mut fps = None;
    let mut from = None;
    let mut to = None;
    let mut notes = false;
    let mut index = 4;
    while index < args.len() {
        match args[index].as_str() {
            "--transparent" => transparent = true,
            "--notes" => notes = true,
            "--fps" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--fps requires a value".to_string())?;
                fps = Some(value.parse().map_err(|error| format!("--fps: {error}"))?);
            }
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
        fps,
        from,
        to,
        notes,
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
    if worker.format == gaanim_editor::cli::PDF_EXTENSION {
        let command = ExportCommand {
            quality: worker.quality.clone(),
            width: worker.width,
            height: worker.height,
            fit: worker.fit,
            notes: worker.notes,
            ..Default::default()
        };
        return gaanim_editor::cli::export_slides_pdf(
            canvas,
            &command,
            &worker.output,
            Some(&worker.script),
            None,
        )
        .map(|_| ());
    }
    if worker.format == gaanim_bundle::EXTENSION {
        let fps = match worker.fps {
            Some(fps) => fps
                .as_whole()
                .ok_or_else(|| format!("a playback bundle records whole rates, not {fps} fps"))?,
            None => gaanim_editor::cli::bundle_fps(&worker.quality),
        };
        let config = gaanim_editor::cli::bundle_config(
            Some(&worker.script),
            &canvas,
            &worker.output,
            Some(fps),
        );
        return gaanim_api::export::record_canvas(canvas, config)
            .map_err(|error| error.to_string());
    }
    let names = gaanim_editor::cli::TimelineNames::of_canvas(&canvas);
    let range =
        gaanim_editor::cli::resolve_export_range(worker.from.as_ref(), worker.to.as_ref(), &names)?;
    let format =
        gaanim_editor::cli::export_format(&worker.format).expect("validated export format");
    let command = ExportCommand {
        quality: worker.quality.clone(),
        encoder: worker.encoder,
        transparent: worker.transparent,
        width: worker.width,
        height: worker.height,
        fit: worker.fit,
        fps: worker.fps,
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
        && let Err(error) = gaanim_project::provision_authoring_package(project.environment_root())
    {
        console::warn(
            "python",
            format!("authoring environment not ready: {error}"),
        );
    }
    crate::startup::mark("authoring package");
    let hint = project
        .map(|project| project.root.as_path())
        .unwrap_or(script_path);
    crate::python::runtime(hint)
}

/// A script running on the plugin's thread. Its scenes and tracebacks wait
/// in their channels until [`attach_script_session`] hands them to an app.
struct ScriptLaunch {
    runner: Box<dyn gaanim_editor::python_plugin::ScriptSession>,
    script_path: PathBuf,
    payload_rx: crossbeam_channel::Receiver<ReloadPayload>,
    error_rx: crossbeam_channel::Receiver<String>,
}

impl ScriptLaunch {
    fn start(python: &gaanim_editor::python_plugin::PythonPlugin, script_path: PathBuf) -> Self {
        // The editor's console and inspector show the log and the script
        // lines; nothing else records them.
        gaanim_core::console::keep_log(true);
        gaanim_api::canvas::track_script_lines(true);
        let (payload_tx, payload_rx) = crossbeam_channel::unbounded::<ReloadPayload>();
        let (error_tx, error_rx) = crossbeam_channel::unbounded::<String>();
        let runner = (python.spawn_script)(script_path.clone(), payload_tx, error_tx);
        crate::startup::mark("script started");
        Self {
            runner,
            script_path,
            payload_rx,
            error_rx,
        }
    }
}

/// Start `script_path` and attach it to the app in `world` at once, as
/// opening a project from Home does.
fn start_script_session(
    world: &mut World,
    python: &gaanim_editor::python_plugin::PythonPlugin,
    script_path: PathBuf,
    project: Option<gaanim_project::ResolvedProject>,
) -> Result<(), String> {
    attach_script_session(world, ScriptLaunch::start(python, script_path), project)
}

/// With JSON messages, say that a change made the script run again.
fn reload_started(cause: &str) {
    if console::message_format_json() {
        console::emit_json(&[
            ("type", "reload".into()),
            ("phase", "start".into()),
            ("cause", cause.into()),
        ]);
    }
}

/// Install the persistent primary camera before the Bevy event loop begins.
///
/// Canvas replay reuses this Vello camera; creating it during replay is too
/// late for `bevy_egui` to attach its primary context on script launches.
fn attach_script_session(
    world: &mut World,
    script: ScriptLaunch,
    project: Option<gaanim_project::ResolvedProject>,
) -> Result<(), String> {
    let ScriptLaunch {
        runner,
        script_path,
        payload_rx,
        error_rx,
    } = script;
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
                    Ok(crate::file_watcher::ProjectChange::Source) => {
                        reload_started("source");
                        runner.request_rerun()
                    }
                    Ok(crate::file_watcher::ProjectChange::Assets(changed)) => {
                        reload_started("assets");
                        runner.request_asset_reload(changed)
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
            }
        })
        .map_err(|error| format!("failed to spawn watcher bridge: {error}"))?;

    let project_paths = resolve_project_paths(&script_path, project.as_ref());
    world.insert_resource(project_paths);
    // A resting window takes a reload or a traceback as soon as it arrives.
    let payload_rx = gaanim_editor::idle::waking_receiver(world, payload_rx, "gaanim-reload-wake");
    let error_rx = gaanim_editor::idle::waking_receiver(world, error_rx, "gaanim-error-wake");
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
            window.title = gaanim_editor::host::window_title(Some(&project.display_name()));
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

/// Report a script that failed to load and exit. A JSON message carries the
/// exception and the script line its traceback names; text keeps the whole
/// traceback.
pub(crate) fn exit_could_not_load(label: &str, error: &str) -> ! {
    report_could_not_load(label, error);
    std::process::exit(2);
}

/// Report a script that failed to load, as [`exit_could_not_load`] does,
/// without exiting.
fn report_could_not_load(label: &str, error: &str) {
    if console::message_format_json() {
        let (headline, location) = console::traceback_summary(error);
        console::print_status_at(
            console::Level::Error,
            label,
            &format!("could not load project: {headline}"),
            location.as_ref(),
        );
    } else {
        console::error(label, format!("could not load project: {error}"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckArgs {
    script: PathBuf,
    strict: bool,
    /// `--members`: the workspace members to check, as patterns.
    members: Option<String>,
}

fn dispatch_check_mode() -> bool {
    let mut args = command_args().into_iter();
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
    if let Some(workspace) = checked_workspace(&parsed) {
        std::process::exit(check_workspace(&workspace, &parsed));
    }
    if parsed.members.is_some() {
        console::error(
            "check",
            format!(
                "--members needs a workspace, and {} is not one",
                console::display_path(&parsed.script)
            ),
        );
        std::process::exit(2);
    }
    let script = gaanim_project::resolve_entry(&parsed.script).unwrap_or_else(|error| {
        console::error("check", error);
        std::process::exit(2);
    });
    // An invalid manifest above the script stops the check, as it stops
    // opening the script.
    if let Err(error) = gaanim_project::locate_script(&script) {
        console::error("check", error);
        std::process::exit(2);
    }

    let python = crate::python::runtime(&script).unwrap_or_else(|error| {
        console::error("check", error);
        console::hint(crate::python::install_hint());
        std::process::exit(2);
    });
    let json = console::message_format_json();
    // A diagnostic about a segment points at the line that made it.
    gaanim_api::canvas::track_script_lines(json);
    match check_script(python, &script, None, parsed.strict, json) {
        Err(error) => exit_could_not_load("check", &error),
        Ok(true) => std::process::exit(1),
        Ok(false) => true,
    }
}

/// The workspace `gaanim check` checks the members of: the folder's own
/// workspace, or the one a project at the workspace root declares when
/// `--members` asks for members.
fn checked_workspace(parsed: &CheckArgs) -> Option<gaanim_project::Workspace> {
    if !parsed.script.is_dir() {
        return None;
    }
    match gaanim_project::resolve_folder(&parsed.script).ok()? {
        gaanim_project::ProjectFolder::Workspace(workspace) => Some(workspace),
        gaanim_project::ProjectFolder::Project(project) if parsed.members.is_some() => project
            .workspace
            .filter(|workspace| workspace.root == project.root),
        gaanim_project::ProjectFolder::Project(_) => None,
    }
}

/// Check the entry of every member of `workspace` (or of those `--members`
/// names), one after another in one interpreter, and summarize. The exit
/// code: 0 when all pass, 1 when one fails, 2 when none can be checked.
fn check_workspace(workspace: &gaanim_project::Workspace, parsed: &CheckArgs) -> i32 {
    let members = workspace
        .select_members(parsed.members.as_deref())
        .unwrap_or_else(|error| {
            console::error("check", error);
            std::process::exit(2);
        });
    if members.is_empty() {
        console::error(
            "check",
            format!("workspace {} has no members", workspace.name),
        );
        console::hint("Add the folders of its projects to `members` in its gaanim.toml.");
        return 2;
    }
    let hint = members
        .iter()
        .find_map(|member| member.project.as_ref().ok())
        .map_or(workspace.root.as_path(), |project| project.root.as_path());
    let python = crate::python::runtime(hint).unwrap_or_else(|error| {
        console::error("check", error);
        console::hint(crate::python::install_hint());
        std::process::exit(2);
    });
    let json = console::message_format_json();
    gaanim_api::canvas::track_script_lines(json);
    let mut failed = Vec::new();
    for member in &members {
        let outcome = match &member.project {
            Ok(project) => check_script(
                python,
                &project.entry,
                Some(&member.path),
                parsed.strict,
                json,
            ),
            Err(error) => Err(error.clone()),
        };
        match outcome {
            Ok(false) => {}
            Ok(true) => failed.push(member.path.as_str()),
            Err(error) => {
                report_could_not_load("check", &error);
                // Editors match each verdict to its member.
                if json {
                    console::emit_json(&[
                        ("type", "check".into()),
                        ("member", member.path.as_str().into()),
                        ("result", "error".into()),
                    ]);
                }
                failed.push(member.path.as_str());
            }
        }
    }
    let passed = members.len() - failed.len();
    if json {
        console::emit_json(&[
            ("type", "workspace".into()),
            ("name", workspace.name.as_str().into()),
            ("members", members.len().into()),
            ("passed", passed.into()),
            ("failed", failed.len().into()),
        ]);
    } else {
        let color = console::color_enabled(console::Stream::Stdout);
        let (level, label) = if failed.is_empty() {
            (console::Level::Success, "pass")
        } else {
            (console::Level::Error, "fail")
        };
        let mut message = format!(
            "workspace {} · {passed} of {} members passed",
            workspace.name,
            members.len()
        );
        if !failed.is_empty() {
            message.push_str(&format!(" · failed: {}", failed.join(", ")));
        }
        println!("{}", console::format_line(level, label, &message, color));
    }
    i32::from(!failed.is_empty())
}

/// Run `script` without a window and print its `check` report. Whether it
/// failed (errors, or warnings when `strict`), or the error that kept it
/// from loading.
fn check_script(
    python: &gaanim_editor::python_plugin::PythonPlugin,
    script: &Path,
    member: Option<&str>,
    strict: bool,
    json: bool,
) -> Result<bool, String> {
    let canvas = (python.load_script_canvas)(script)?;
    let source = std::fs::read_to_string(script).unwrap_or_default();
    let is_presentation = canvas.has_presentation_features();
    let mut report = if is_presentation {
        presentation_preflight(&canvas, &source)
    } else {
        scene_preflight(&canvas, &source)
    };
    // A manifest travels with the project, repositories included.
    if let Some(project) = gaanim_project::find_project_for_script(script) {
        report
            .warnings
            .extend(project.manifest.secret_keys.iter().map(|key| {
                format!(
                    "gaanim.toml: `{key}` looks like a credential; keep tokens and keys out of \
                     the project"
                )
            }));
    }

    let failed = !report.errors.is_empty() || (strict && !report.warnings.is_empty());
    if json {
        print_check_json(script, member, &canvas, &report, is_presentation);
        return Ok(failed);
    }
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
            console::display_path(script),
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
    Ok(failed)
}

/// The `check` report as JSON lines: a `diagnostic` per error and warning,
/// with the line of the segment it names when there is one, then the
/// `check` verdict, which names the workspace `member` it is about.
fn print_check_json(
    script: &Path,
    member: Option<&str>,
    canvas: &gaanim_api::canvas::SceneModel,
    report: &PreflightReport,
    is_presentation: bool,
) {
    let index = canvas.timeline_index();
    let diagnostics = report
        .errors
        .iter()
        .map(|message| (console::Level::Error, message))
        .chain(
            report
                .warnings
                .iter()
                .map(|message| (console::Level::Warn, message)),
        );
    for (level, message) in diagnostics {
        let location = segment_named_in(message)
            .and_then(|name| index.segments.iter().find(|segment| segment.name == name))
            .and_then(|segment| segment.location.clone())
            .or_else(|| script_line_in(message));
        let mut fields = vec![
            ("type", console::Json::from("diagnostic")),
            ("level", console::level_name(level).into()),
            ("message", message.as_str().into()),
        ];
        fields.extend(console::location_fields(location.as_ref()));
        console::emit_json(&fields);
    }
    let kind = if is_presentation {
        "presentation"
    } else {
        "scene"
    };
    let result = if report.errors.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let mut verdict = vec![
        ("type", console::Json::from("check")),
        (
            "script",
            console::plain_path(script)
                .to_string_lossy()
                .into_owned()
                .into(),
        ),
    ];
    if let Some(member) = member {
        verdict.push(("member", member.into()));
    }
    verdict.extend([
        ("kind", kind.into()),
        ("result", result.into()),
        ("errors", report.errors.len().into()),
        ("warnings", report.warnings.len().into()),
        ("segments", report.segment_count.into()),
        ("stops", report.stop_count.into()),
        ("duration", report.duration.into()),
        ("width", canvas.frame.width.into()),
        ("height", canvas.frame.height.into()),
    ]);
    console::emit_json(&verdict);
}

/// The segment a diagnostic is about, written as segment `name`.
fn segment_named_in(message: &str) -> Option<&str> {
    let start = message.find("segment `")? + "segment `".len();
    let length = message[start..].find('`')?;
    Some(&message[start..start + length])
}

/// The first script line a diagnostic names as `path.py:line`, made
/// absolute.
fn script_line_in(message: &str) -> Option<console::ScriptLocation> {
    message.split_whitespace().find_map(|word| {
        let word = word.trim_matches(|ch: char| matches!(ch, '(' | ')' | ',' | ';' | '`'));
        let (path, line) = word.rsplit_once(':')?;
        let line = line.parse::<u32>().ok()?;
        if !path.ends_with(".py") {
            return None;
        }
        let path = std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path));
        Some(console::ScriptLocation::new(path, line))
    })
}

fn parse_check_args(args: &[String]) -> Result<CheckArgs, String> {
    let mut script = None;
    let mut strict = false;
    let mut members = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--strict" => strict = true,
            "--members" => {
                members = Some(
                    args.next()
                        .ok_or_else(|| "--members requires member paths or patterns".to_string())?
                        .clone(),
                );
            }
            value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
            value if script.is_none() => script = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument `{value}`")),
        }
    }
    Ok(CheckArgs {
        script: script.ok_or_else(|| "missing <script.py>".to_string())?,
        strict,
        members,
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
    report.warnings.extend(canvas.measurement_warning());
    report.warnings.extend(canvas.covered_text_warnings());
    report.warnings.extend(
        canvas
            .compiled_layout_diagnostics()
            .into_iter()
            .map(|(_, message)| format!("layout: {message}")),
    );
    report.warnings.extend(canvas.script_warnings());

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
    report.warnings.extend(canvas.measurement_warning());
    report.warnings.extend(canvas.covered_text_warnings());
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
                parsed.height,
            );
        } else if let Err(error) =
            (python.capture_script_snapshots)(&script, capture_dir, parsed.height)
        {
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
    height: Option<u32>,
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
    let capture =
        gaanim_diff::capture_stops(canvas, capture_dir, stops, height).unwrap_or_else(|error| {
            console::error("diff", format!("stop capture failed: {error}"));
            std::process::exit(2);
        });
    gaanim_editor::diff_cli::print_stop_capture(&capture, capture_dir);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchArgs {
    script_path: Option<PathBuf>,
    project: Option<gaanim_project::ResolvedProject>,
    /// A workspace folder without a script of its own: Home shows its page.
    workspace: Option<gaanim_project::Workspace>,
    present: bool,
    /// Zero-based monitor index used only with `--present`.
    monitor: Option<usize>,
    /// Segments to rehearse with `--sections` / `--from`.
    selection: gaanim_timeline::selection::SegmentSelection,
    /// Planned length of the talk (`--duration`), which Presenter View
    /// counts down.
    duration: Option<std::time::Duration>,
    /// Where editors control the preview (`--listen`).
    listen: Option<crate::control::ListenAddress>,
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
            poll_relay: project.manifest.poll_relay.clone(),
        }
    } else {
        let proj = script_parent.canonicalize().unwrap_or(script_parent);
        let out = proj.join("exports");
        gaanim_editor::export::ProjectPaths {
            project_dir: proj.clone(),
            output_dir: out,
            script_path: script_path.to_path_buf(),
            poll_relay: None,
        }
    }
}

fn parse_args() -> LaunchArgs {
    let args = command_args();
    if args.is_empty() {
        return LaunchArgs {
            script_path: None,
            project: None,
            workspace: None,
            present: false,
            monitor: None,
            selection: Default::default(),
            duration: None,
            listen: None,
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
        let project = match gaanim_project::resolve_folder(raw_path) {
            Ok(gaanim_project::ProjectFolder::Project(project)) => project,
            // A workspace without a script of its own opens in Home.
            Ok(gaanim_project::ProjectFolder::Workspace(workspace)) => {
                if parsed.present || parsed.listen.is_some() || !parsed.selection.is_empty() {
                    console::error(
                        "usage",
                        format!(
                            "{} is the workspace {}; open one of its members",
                            console::display_path(&workspace.root),
                            workspace.name
                        ),
                    );
                    std::process::exit(2);
                }
                return LaunchArgs {
                    script_path: None,
                    workspace: Some(workspace),
                    ..parsed
                };
            }
            Err(error) => {
                console::error("project", error);
                std::process::exit(2);
            }
        };
        (project.entry.clone(), Some(project))
    } else {
        let path = gaanim_project::resolve_entry(raw_path).unwrap_or_else(|error| {
            console::error("project", error);
            std::process::exit(2);
        });
        // An invalid manifest above the script is reported, not passed over.
        let project = gaanim_project::locate_script(&path)
            .unwrap_or_else(|error| {
                console::error("project", error);
                std::process::exit(2);
            })
            .project;
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
    let mut duration = None;
    let mut listen = None;
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
            "--duration" => {
                let value = args
                    .get(index)
                    .ok_or_else(|| "--duration requires a length, e.g. 20m".to_string())?;
                index += 1;
                duration = Some(parse_talk_duration(value)?);
            }
            "--listen" => {
                let value = args.get(index).ok_or_else(|| {
                    "--listen requires stdio or a loopback address such as 127.0.0.1:7801"
                        .to_string()
                })?;
                index += 1;
                listen = Some(crate::control::ListenAddress::parse(value)?);
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
    if duration.is_some() && script_path.is_none() {
        return Err("--duration requires <SCRIPT_OR_PROJECT>".to_string());
    }
    if listen.is_some() && script_path.is_none() {
        return Err("--listen requires <SCRIPT_OR_PROJECT>".to_string());
    }
    Ok(LaunchArgs {
        script_path,
        project: None,
        workspace: None,
        present,
        monitor,
        selection,
        duration,
        listen,
    })
}

/// A talk's length for `--duration` (see [`gaanim_editor::parse_talk_duration`]).
fn parse_talk_duration(text: &str) -> Result<std::time::Duration, String> {
    gaanim_editor::parse_talk_duration(text)
        .map_err(|expected| format!("--duration takes {expected}, not `{text}`"))
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
                fps: None,
                from: None,
                to: None,
                notes: false,
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
                Some(ExportBound::Name("climax".into())),
                Some(ExportBound::Name("fin".into()))
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
        let deck = ["talk.py", "talk.pdf", "standard", "pdf", "--notes"].map(str::to_string);
        let deck = parse_export_worker_args(&deck).unwrap();
        assert_eq!(deck.format, "pdf");
        assert!(deck.notes);
        let prores = [
            "scene.py",
            "lower.mov",
            "standard",
            "mov",
            "--transparent",
            "--fps",
            "30000/1001",
        ]
        .map(str::to_string);
        let prores = parse_export_worker_args(&prores).unwrap();
        assert_eq!(prores.format, "mov");
        assert_eq!(
            prores.fps.map(|fps| fps.to_string()).as_deref(),
            Some("29.97")
        );
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
        let fps = ["scene.py", "out.mp4", "draft", "mp4", "--fps", "0"].map(str::to_string);
        assert!(parse_export_worker_args(&fps).is_err());
    }

    #[test]
    fn parses_presentation_launch_options_in_any_order() {
        let args = ["demo.py", "--monitor", "1", "--present"].map(str::to_string);
        assert_eq!(
            parse_launch_args(&args).unwrap(),
            LaunchArgs {
                script_path: Some(PathBuf::from("demo.py")),
                project: None,
                workspace: None,
                present: true,
                monitor: Some(1),
                selection: Default::default(),
                duration: None,
                listen: None,
            }
        );
    }

    #[test]
    fn message_format_leaves_the_command_line_wherever_it_is() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(
            take_message_format(args(&["check", "--message-format", "json", "."])),
            Ok((args(&["check", "."]), true))
        );
        assert_eq!(
            take_message_format(args(&["export", ".", "--message-format=human"])),
            Ok((args(&["export", "."]), false))
        );
        assert_eq!(take_message_format(args(&["."])), Ok((args(&["."]), false)));
        assert!(take_message_format(args(&["check", "--message-format"])).is_err());
        assert!(take_message_format(args(&["check", "--message-format", "xml"])).is_err());
    }

    #[test]
    fn diagnostics_name_the_segment_they_are_about() {
        assert_eq!(
            segment_named_in("segment `Problema`: mark 2 is [stop dto] but stop 2 is `dato`"),
            Some("Problema")
        );
        assert_eq!(
            segment_named_in("segment `intro` has no speaker notes"),
            Some("intro")
        );
        assert_eq!(
            segment_named_in("timeline is shorter than one second"),
            None
        );
        let measured = script_line_in(
            "measuring compiled the scene 1 time: examples/basics/segments.py:52 (1×, 0.13s)",
        )
        .unwrap();
        assert!(measured.file.ends_with("examples/basics/segments.py"));
        assert_eq!(measured.line, 52);
        assert_eq!(script_line_in("at 12:30 the clock: stops"), None);
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
    fn parses_the_talk_duration() {
        assert!(
            parse_talk_duration("20x")
                .unwrap_err()
                .starts_with("--duration takes a length")
        );
        let args = ["talk", "--present", "--duration", "20m"].map(str::to_string);
        assert_eq!(
            parse_launch_args(&args).unwrap().duration,
            Some(std::time::Duration::from_secs(1200))
        );
        assert!(parse_launch_args(&["--duration".into(), "20m".into()]).is_err());
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
                workspace: None,
                present: false,
                monitor: None,
                selection: Default::default(),
                duration: None,
                listen: None,
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
                members: None,
            }
        );
        let args = ["canal", "--members", "season-1/*,trailer"].map(str::to_string);
        assert_eq!(
            parse_check_args(&args).unwrap().members.as_deref(),
            Some("season-1/*,trailer")
        );
        assert!(parse_check_args(&["canal", "--members"].map(str::to_string)).is_err());
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
