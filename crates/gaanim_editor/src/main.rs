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

mod export;
mod file_watcher;
mod hot_reload;
mod script_runner;

use hot_reload::{
    ReloadReceiver, ReloadStatus, reload_listener_system, reload_status_overlay_system,
};

fn main() {
    if dispatch_diff_mode() {
        return;
    }

    let script_path = parse_args();

    // 1. Register the `gaanim_core` module in the embedded interpreter's init
    //    table BEFORE initializing Python, so `import gaanim_core` resolves to
    //    our in-process module (no .pyd needed).
    gaanim_python::register_inittab();

    // 2. Initialize the embedded CPython interpreter.
    Python::initialize();

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
            title: "Gaanim".to_string(),
            resolution: (1280, 720).into(),
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
        if !example.is_file() {
            eprintln!(
                "gaanim --diff: example script does not exist: {}",
                example.display()
            );
            std::process::exit(2);
        }
        let capture_dir = if parsed.bless {
            &parsed.baseline
        } else {
            &parsed.current
        };
        println!(
            "Capturing {} -> {}",
            example.display(),
            capture_dir.display()
        );
        gaanim_python::register_inittab();
        Python::initialize();
        if let Err(error) = script_runner::capture_script_snapshots(example, capture_dir) {
            eprintln!("gaanim --diff: snapshot capture failed: {error}");
            std::process::exit(2);
        }
        if !capture_dir.join(gaanim_diff::MANIFEST_FILE).is_file() {
            eprintln!(
                "gaanim --diff: {} did not call scene.snapshots(...)",
                example.display()
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
        return Err("--bless requires --example <SCRIPT>".to_string());
    }

    Ok(Some(DiffModeArgs {
        baseline: baseline
            .ok_or_else(|| "missing --baseline <DIR> or --example <SCRIPT>".to_string())?,
        current: current
            .ok_or_else(|| "missing --current <DIR> or --example <SCRIPT>".to_string())?,
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
    gaanim --diff --example <SCRIPT> [OPTIONS]
    gaanim --diff --baseline <DIR> --current <DIR> [OPTIONS]

OPTIONS:
    -e, --example <SCRIPT>          Capture and compare one example automatically
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

fn parse_args() -> PathBuf {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        eprintln!("usage:");
        eprintln!("  gaanim <script.py>");
        eprintln!("  gaanim --diff --baseline <DIR> --current <DIR> [OPTIONS]");
        std::process::exit(2);
    };
    if first == "--help" || first == "-h" {
        eprintln!("gaanim — GPU-accelerated vector animation engine (hot-reload viewer)");
        eprintln!();
        eprintln!("usage:");
        eprintln!("  gaanim <script.py>");
        eprintln!("  gaanim --diff --example <SCRIPT> [OPTIONS]");
        std::process::exit(0);
    }
    let path = PathBuf::from(&first);
    if !path.exists() {
        eprintln!("gaanim: script not found: {}", path.display());
        std::process::exit(2);
    }
    // Canonicalize so the file watcher can match absolute event paths.
    path.canonicalize().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

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
