use std::path::PathBuf;

use gaanim_diff::{CompareOptions, FrameStatus, compare_directories};

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("gaanim-diff: {error}");
            eprintln!("Run `gaanim-diff --help` for usage.");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<i32, String> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        print_help();
        return Ok(2);
    };
    if matches!(command.as_str(), "-h" | "--help" | "help") {
        print_help();
        return Ok(0);
    }
    if command != "compare" {
        return Err(format!("unknown command `{command}`"));
    }

    let baseline = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "missing baseline directory".to_string())?;
    let current = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "missing current directory".to_string())?;
    let mut output = PathBuf::from("gaanim-diff-report");
    let mut options = CompareOptions::default();
    let mut open_gui = true;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--output" | "-o" => {
                output = PathBuf::from(
                    args.next()
                        .ok_or_else(|| format!("{flag} requires a directory"))?,
                );
            }
            "--pixel-threshold" => {
                options.pixel_threshold = args
                    .next()
                    .ok_or_else(|| format!("{flag} requires an integer"))?
                    .parse()
                    .map_err(|_| "pixel threshold must be between 0 and 255".to_string())?;
            }
            "--max-changed-ratio" => {
                options.max_changed_ratio = args
                    .next()
                    .ok_or_else(|| format!("{flag} requires a number"))?
                    .parse()
                    .map_err(|_| "max changed ratio must be between 0 and 1".to_string())?;
            }
            "--no-gui" => open_gui = false,
            "-h" | "--help" => {
                print_help();
                return Ok(0);
            }
            _ => return Err(format!("unknown option `{flag}`")),
        }
    }

    let report = compare_directories(&baseline, &current, &output, options)
        .map_err(|error| error.to_string())?;
    println!(
        "{}: {} compared, {} changed, {} missing",
        if report.passed { "PASS" } else { "FAIL" },
        report.compared,
        report.changed,
        report.missing
    );
    for frame in report
        .frames
        .iter()
        .filter(|frame| frame.status != FrameStatus::Unchanged)
        .take(12)
    {
        let time = frame
            .time_seconds
            .map(|time| format!(" at {time:.6}s"))
            .unwrap_or_default();
        println!(
            "  {:?}: {}{} ({:.4}% changed, max delta {})",
            frame.status,
            frame.id,
            time,
            frame.changed_ratio * 100.0,
            frame.max_channel_delta
        );
    }
    println!("HTML: {}", output.join("index.html").display());
    println!("JSON: {}", output.join("report.json").display());
    let exit_code = if report.passed { 0 } else { 1 };
    if open_gui {
        gaanim_diff::viewer::run(report, baseline, current, output, options)
            .map_err(|error| format!("could not open egui viewer: {error}"))?;
    }
    Ok(exit_code)
}

fn print_help() {
    println!(
        r#"gaanim-diff — exact-seek visual regression reports

USAGE:
    gaanim-diff compare <BASELINE_DIR> <CURRENT_DIR> [OPTIONS]

OPTIONS:
    -o, --output <DIR>              Report directory (default: gaanim-diff-report)
        --pixel-threshold <0..255>  Ignore per-channel noise up to this value (default: 2)
        --max-changed-ratio <0..1>  Allowed changed-pixel fraction (default: 0)
        --no-gui                     Generate reports without opening egui (CI)
    -h, --help                      Print help

The command opens the native egui viewer by default. It exits with 0 when all
frames pass, 1 when visual changes are found, and 2 for invalid input or errors."#
    );
}
