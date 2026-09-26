//! Lightweight Gaanim launcher.
//!
//! It handles commands that do not need Python, discovers a compatible runtime
//! for project/script launches, and then starts the `gaanim-core` binary.

use gaanim_core::console;
use gaanim_project::help::{self, Topic};
use gaanim_project::{
    CreateProjectOptions, EnvironmentProbe, ProjectKind, activate_environment, core_environment,
    create_project, python_requirement,
};
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if handle_no_python_commands(&args) {
        return;
    }

    // The core is linked against Python, including for the Home screen, so
    // prepare the runtime before spawning it even when no script argument was
    // supplied. This keeps `python3.dll` (Windows) or `libpython3.<minor>.so`
    // (Linux) resolvable while the editor can still show its environment
    // review before opening a project.
    let hint = find_script_hint(&args);
    let probe = EnvironmentProbe::detect(hint.as_deref());
    if let Err(error) = activate_environment(&probe) {
        console::error("python", error);
        console::hint(format!(
            "Install {} (for example `uv python install 3.14`) and retry, or run `gaanim --help`.",
            python_requirement()
        ));
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
        console::error(
            "launch",
            format!("core binary not found at {}", core_exe.display()),
        );
        std::process::exit(1);
    }
    let status = Command::new(&core_exe)
        .args(&args[1..])
        .envs(core_environment(&probe))
        .status()
        .unwrap_or_else(|error| {
            console::error(
                "launch",
                format!("failed to start {}: {error}", core_exe.display()),
            );
            std::process::exit(1);
        });
    std::process::exit(status.code().unwrap_or(1));
}

fn handle_no_python_commands(args: &[String]) -> bool {
    if matches!(args.get(1).map(String::as_str), Some("--version" | "-V")) {
        println!("gaanim {}", env!("CARGO_PKG_VERSION"));
        return true;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        if args.iter().any(|arg| arg == "init") {
            help::print(Topic::Init);
        } else if args.iter().any(|arg| arg == "check") {
            help::print(Topic::Check);
        } else if args.get(1).map(String::as_str) == Some("export") {
            help::print(Topic::Export);
        } else if args.iter().any(|arg| arg == "--diff") {
            help::print(Topic::Diff);
        } else {
            help::print(Topic::General);
        }
        return true;
    }
    if args.get(1).map(String::as_str) != Some("init") {
        return false;
    }
    let parsed = parse_init_args(&args[2..]).unwrap_or_else(|error| {
        console::error("init", error);
        console::hint("Run `gaanim init --help` for usage.");
        std::process::exit(2);
    });
    let project = create_project(&parsed).unwrap_or_else(|error| {
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
    if matches!(args.get(1).map(String::as_str), Some("check" | "export")) {
        return args.get(2).map(PathBuf::from);
    }
    // Values of these options are names or numbers, never the script.
    let takes_value = |index: usize| {
        index > 0
            && matches!(
                args[index - 1].as_str(),
                "--monitor" | "--sections" | "--from"
            )
    };
    args.iter().enumerate().rev().find_map(|(index, arg)| {
        if arg.starts_with('-') || matches!(arg.as_str(), "check" | "init") || takes_value(index) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_hint_skips_option_values() {
        let args: Vec<String> = [
            "gaanim",
            "talk",
            "--from",
            "Resultados",
            "--sections",
            "a,b",
        ]
        .map(str::to_string)
        .into();
        assert_eq!(find_script_hint(&args), Some(PathBuf::from("talk")));
    }

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
    fn version_and_export_help_need_no_python() {
        assert!(handle_no_python_commands(&[
            "gaanim".into(),
            "--version".into()
        ]));
        assert!(handle_no_python_commands(&[
            "gaanim".into(),
            "export".into(),
            "--help".into()
        ]));
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

    #[test]
    fn export_uses_the_script_instead_of_option_values_as_hint() {
        let args = [
            "gaanim".into(),
            "export".into(),
            "demo.py".into(),
            "--output".into(),
            "video.mp4".into(),
            "--quality".into(),
            "standard".into(),
        ];
        assert_eq!(find_script_hint(&args), Some(PathBuf::from("demo.py")));
    }
}
