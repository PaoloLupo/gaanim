//! Lightweight Gaanim launcher.
//!
//! It handles commands that do not need Python, discovers a compatible runtime
//! for project/script launches, and then starts the `gaanim-core` binary.

use gaanim_project::{
    CreateProjectOptions, EnvironmentProbe, ProjectKind, activate_environment, create_project,
};
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if handle_no_python_commands(&args) {
        return;
    }

    // The core is linked against Python on Windows, including for the Home
    // screen, so prepare the runtime before spawning it even when no script
    // argument was supplied. This keeps `python3.dll` resolvable while the
    // editor can still show its environment review before opening a project.
    let hint = find_script_hint(&args);
    let probe = EnvironmentProbe::detect(hint.as_deref());
    if let Err(error) = activate_environment(&probe) {
        eprintln!("gaanim: {error}");
        eprintln!("Run `gaanim --help` for usage, or install Python >=3.12 and retry.");
        std::process::exit(2);
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("gaanim.exe"));
    let core_exe = exe
        .parent()
        .map(|parent| {
            parent.join(if cfg!(windows) {
                "gaanim-core.exe"
            } else {
                "gaanim-core"
            })
        })
        .unwrap_or_else(|| {
            PathBuf::from(if cfg!(windows) {
                "gaanim-core.exe"
            } else {
                "gaanim-core"
            })
        });
    if !core_exe.is_file() {
        eprintln!(
            "gaanim launcher: core binary not found at {}",
            core_exe.display()
        );
        std::process::exit(1);
    }
    let status = Command::new(&core_exe)
        .args(&args[1..])
        .status()
        .unwrap_or_else(|error| {
            eprintln!("gaanim: failed to spawn {}: {error}", core_exe.display());
            std::process::exit(1);
        });
    std::process::exit(status.code().unwrap_or(1));
}

fn handle_no_python_commands(args: &[String]) -> bool {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        if args.iter().any(|arg| arg == "init") {
            print_init_help();
        } else if args.iter().any(|arg| arg == "check") {
            print_check_help();
        } else if args.iter().any(|arg| arg == "--diff") {
            print_diff_help();
        } else {
            print_general_help();
        }
        return true;
    }
    if args.get(1).map(String::as_str) != Some("init") {
        return false;
    }
    let parsed = parse_init_args(&args[2..]).unwrap_or_else(|error| {
        eprintln!("gaanim init: {error}");
        eprintln!("Run `gaanim init --help` for usage.");
        std::process::exit(2);
    });
    let project = create_project(&parsed).unwrap_or_else(|error| {
        eprintln!("gaanim init: {error}");
        std::process::exit(2);
    });
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
        println!("Export: set GAANIM_EXPORT=exports/video.mp4, then run the project");
    }
    true
}

fn parse_init_args(args: &[String]) -> Result<CreateProjectOptions, String> {
    let kind = args
        .first()
        .ok_or_else(|| "missing project kind; available kinds: video, slides".to_string())
        .and_then(|value| ProjectKind::parse(value))?;
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
    Ok(CreateProjectOptions {
        kind,
        directory: directory.unwrap_or_else(|| PathBuf::from(kind.default_directory())),
        force,
    })
}

fn find_script_hint(args: &[String]) -> Option<PathBuf> {
    args.iter().rev().find_map(|arg| {
        if arg.starts_with('-') || matches!(arg.as_str(), "check" | "init") {
            return None;
        }
        let path = PathBuf::from(arg);
        if path.exists()
            || arg.ends_with(".py")
            || arg.contains('/')
            || arg.contains('\\')
            || !arg.contains('.')
        {
            Some(path)
        } else {
            None
        }
    })
}

fn print_general_help() {
    println!("gaanim — GPU-accelerated vector animation engine");
    println!();
    println!("usage:");
    println!("  gaanim");
    println!("  gaanim [--present] [--monitor <INDEX>] <SCRIPT_OR_PROJECT>");
    println!("  gaanim init <video|slides> [DIRECTORY] [--force]");
    println!("  gaanim check <SCRIPT_OR_PROJECT> [--strict]");
    println!("  gaanim --diff --example <SCRIPT_OR_PROJECT> [OPTIONS]");
}

fn print_init_help() {
    println!(
        r#"gaanim init - create a runnable Gaanim project

USAGE:
    gaanim init <KIND> [DIRECTORY] [--force]

ARGUMENTS:
    video               Animated-video starter
    slides              Semantic slides starter
    DIRECTORY           Project directory (defaults to gaanim-<kind>)

OPTIONS:
    --force             Update scaffold files without deleting user assets
    -h, --help          Print this help"#
    );
}

fn print_check_help() {
    println!(
        r#"gaanim check - validate a video or slides project

USAGE:
    gaanim check <SCRIPT_OR_PROJECT> [--strict]

OPTIONS:
    --strict            Return failure when warnings are present
    -h, --help          Print this help"#
    );
}

fn print_diff_help() {
    println!(
        r#"gaanim --diff - native egui visual regression viewer

USAGE:
    gaanim --diff --example <SCRIPT_OR_PROJECT> [OPTIONS]
    gaanim --diff --baseline <DIR> --current <DIR> [OPTIONS]

OPTIONS:
    -e, --example <SCRIPT_OR_PROJECT>  Capture and compare one project
        --tests-root <DIR>             Snapshot root (default: tests/visual)
        --bless                        Capture as baseline and exit
        --no-capture                   Reuse existing current snapshots
    -b, --baseline <DIR>               Known-good snapshot directory
    -c, --current <DIR>                Candidate snapshot directory
    -o, --output <DIR>                 Override report directory
        --pixel-threshold <0..255>      Ignored per-channel difference
        --max-changed-ratio <0..1>      Allowed changed-pixel fraction
        --no-gui                        Generate reports without egui
    -h, --help                          Print this help"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_video_and_slides_init_kinds() {
        assert_eq!(
            parse_init_args(&["video".into()]).unwrap().kind,
            ProjectKind::Video
        );
        assert_eq!(
            parse_init_args(&["slides".into()]).unwrap().kind,
            ProjectKind::Slides
        );
        assert!(parse_init_args(&["presentation".into()]).is_err());
        assert!(parse_init_args(&["thesis".into()]).is_err());
    }

    #[test]
    fn bare_launch_is_not_consumed_by_no_python_dispatch() {
        assert!(!handle_no_python_commands(&["gaanim".into()]));
    }

    #[test]
    fn script_hint_ignores_command_words() {
        let args = ["gaanim".into(), "check".into(), "demo.py".into()];
        assert_eq!(find_script_hint(&args), Some(PathBuf::from("demo.py")));
    }
}
