//! `gaanim --diff`: capture snapshots of a scene or a playback bundle and
//! compare them with an approved baseline.

use std::path::{Path, PathBuf};

use gaanim_core::console;

#[derive(Debug)]
pub struct DiffModeArgs {
    pub baseline: PathBuf,
    pub current: PathBuf,
    pub output: PathBuf,
    pub options: gaanim_diff::CompareOptions,
    pub example: Option<PathBuf>,
    pub capture: bool,
    pub capture_only: bool,
    pub bless: bool,
    /// Capture every `scene.stop(...)` instead of the script's `scene.snapshots`.
    pub capture_stops: bool,
    /// 1-based stops to capture; `None` captures all of them.
    pub stops: Option<Vec<usize>>,
    /// With `--capture-stops`, only stops inside these segments or sections.
    pub selection: gaanim_timeline::selection::SegmentSelection,
    /// Capture frames this many pixels tall instead of the preview size.
    pub height: Option<u32>,
}

/// Run `gaanim --diff` with the arguments after `--diff` and exit.
///
/// A playback bundle (`--example deck.gaanim`) captures its stops here,
/// without Python; `capture_script` captures a script or project into the
/// directory it is given, or reports that this binary cannot.
pub fn run_diff(args: &[String], capture_script: impl FnOnce(&DiffModeArgs, &Path, &Path)) -> ! {
    let parsed = match parse_diff_mode_args(args) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => {
            gaanim_project::help::print(gaanim_project::help::Topic::Diff);
            std::process::exit(0);
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
        let capture_dir = if parsed.bless {
            &parsed.baseline
        } else {
            &parsed.current
        };
        if crate::bundle_player::is_bundle_path(example) {
            println!(
                "Capturing {} -> {}",
                console::display_path(example),
                capture_dir.display()
            );
            let capture = gaanim_diff::capture_bundle_stops(
                example,
                capture_dir,
                parsed.stops.as_deref(),
                &parsed.selection,
            )
            .unwrap_or_else(|error| {
                console::error("diff", format!("stop capture failed: {error}"));
                std::process::exit(2);
            });
            print_stop_capture(&capture, capture_dir);
        } else {
            capture_script(&parsed, example, capture_dir);
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

    if let Some(error) = size_mismatch(&parsed.baseline, &parsed.current) {
        console::error("diff", error);
        std::process::exit(2);
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

/// List the stops a capture wrote.
pub fn print_stop_capture(capture: &gaanim_diff::StopCapture, capture_dir: &Path) {
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

/// Why `--diff` ends after capturing instead of comparing with `baseline`:
/// `Ok` for a successful stop capture that has no stop baseline to compare
/// with (a `scene.snapshots` baseline shares none of its ids), `Err` when
/// there is no baseline at all.
pub fn comparison_blocker(baseline: &Path, capture_stops: bool) -> Option<Result<String, String>> {
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

/// Why `baseline` and `current` cannot be compared frame by frame: they
/// were captured at different sizes.
fn size_mismatch(baseline: &Path, current: &Path) -> Option<String> {
    let (baseline_size, current_size) = (
        gaanim_diff::snapshot_size(baseline)?,
        gaanim_diff::snapshot_size(current)?,
    );
    (baseline_size != current_size).then(|| {
        format!(
            "the baseline is {}x{} and this capture {}x{}; capture both at the same size \
             (--height {}) or pass --capture-only",
            baseline_size.0, baseline_size.1, current_size.0, current_size.1, baseline_size.1
        )
    })
}

pub fn parse_diff_mode_args(args: &[String]) -> Result<Option<DiffModeArgs>, String> {
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
    let mut height = None;
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
                selection.set_sections(value(&mut index)?)?;
            }
            "--from" => selection.from = Some(value(&mut index)?.to_string()),
            "--height" => {
                height = Some(
                    value(&mut index)?
                        .parse::<u32>()
                        .ok()
                        .filter(|height| (16..=8192).contains(height))
                        .ok_or_else(|| {
                            "--height must be a whole number of pixels from 16 to 8192".to_string()
                        })?,
                );
            }
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
    let bundle = example
        .as_deref()
        .is_some_and(crate::bundle_player::is_bundle_path);
    if stops.is_some() && !capture_stops && !bundle {
        return Err("--stops requires --capture-stops".to_string());
    }
    if !selection.is_empty() && !capture_stops && !bundle {
        return Err(
            "--sections and --from require --capture-stops; scene.snapshots times are chosen by the script"
                .to_string(),
        );
    }
    if (capture_stops || bundle) && capture == Some(false) {
        return Err("--capture-stops cannot be combined with --no-capture".to_string());
    }
    if height.is_some() && bundle {
        return Err(
            "--height captures scripts; a bundle's frames have the size it was recorded at"
                .to_string(),
        );
    }
    if height.is_some() && (example.is_none() || capture == Some(false)) {
        return Err("--height needs a capture: --example without --no-capture".to_string());
    }

    if let Some(example) = example {
        // A bundle records no `scene.snapshots` request: its stops are the
        // exact frames it captures.
        let capture_stops = capture_stops || crate::bundle_player::is_bundle_path(&example);
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
            height,
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
        height: None,
    }))
}

pub fn visual_test_case_dir(tests_root: &Path, example: &Path) -> Result<PathBuf, String> {
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

    /// `--height` sizes a script's capture; bundles keep their size, and
    /// captures of different sizes are not compared (#305).
    #[test]
    fn height_sizes_script_captures_only() {
        let parse = |args: &[&str]| {
            parse_diff_mode_args(&args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>())
        };
        let parsed = parse(&["--example", "examples/demo.py", "--height", "540"])
            .unwrap()
            .unwrap();
        assert_eq!(parsed.height, Some(540));
        assert!(parse(&["--example", "examples/demo.py", "--height", "0"]).is_err());
        assert!(parse(&["--example", "examples/demo.py", "--height", "big"]).is_err());
        assert!(parse(&["--example", "deck.gaanim", "--height", "540"]).is_err());
        assert!(parse(&["--baseline", "a", "--current", "b", "--height", "540"]).is_err());

        let root = std::env::temp_dir().join(format!("gaanim-diff-size-{}", std::process::id()));
        let manifest = |dir: &str, width: u32, height: u32| {
            let dir = root.join(dir);
            std::fs::create_dir_all(&dir).unwrap();
            let manifest = gaanim_diff::SnapshotManifest {
                schema_version: 1,
                width,
                height,
                snapshots: Vec::new(),
            };
            std::fs::write(
                dir.join(gaanim_diff::MANIFEST_FILE),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
            dir
        };
        let baseline = manifest("baseline", 1920, 1080);
        let same = manifest("same", 1920, 1080);
        let small = manifest("small", 960, 540);
        assert_eq!(size_mismatch(&baseline, &same), None);
        let error = size_mismatch(&baseline, &small).unwrap();
        assert!(
            error.contains("1920x1080") && error.contains("--height 1080"),
            "{error}"
        );
        std::fs::remove_dir_all(&root).unwrap();
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

    #[test]
    fn a_bundle_example_captures_its_stops() {
        let args = [
            "--example",
            "talks/deck.gaanim",
            "--stops",
            "2-3",
            "--from",
            "demo",
        ]
        .map(str::to_string);
        let parsed = parse_diff_mode_args(&args).unwrap().unwrap();
        assert!(parsed.capture_stops);
        assert_eq!(parsed.stops, Some(vec![2, 3]));
        assert_eq!(
            parsed.baseline,
            PathBuf::from("tests/visual/talks/deck/baseline")
        );
        let no_capture = ["--example", "deck.gaanim", "--no-capture"].map(str::to_string);
        assert!(parse_diff_mode_args(&no_capture).is_err());
    }
}
