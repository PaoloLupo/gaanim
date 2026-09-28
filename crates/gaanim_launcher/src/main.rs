//! Lightweight Gaanim launcher.
//!
//! It handles commands that do not need Python, discovers a compatible runtime
//! for project/script launches, and then starts the `gaanim-core` binary.
//! Playback bundles (`.gaanim`) play and export through `gaanim-play`, which
//! does not link Python, so they need no Python installation at all; without
//! Python, Home opens there too.

use gaanim_core::console;
use gaanim_project::help::{self, Topic};
use gaanim_project::{
    CreateProjectOptions, EnvironmentProbe, ProjectKind, activate_environment, core_environment,
    create_project, python_requirement,
};
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if handle_no_python_commands(&args) {
        return;
    }
    if bundle_input(&args) {
        run_player(&args[1..]);
    }

    // The core is linked against Python, including for the Home screen, so
    // prepare the runtime before spawning it even when no script argument was
    // supplied. This keeps `python3.dll` (Windows) or `libpython3.<minor>.so`
    // (Linux) resolvable while the editor can still show its environment
    // review before opening a project.
    let hint = find_script_hint(&args);
    let probe = EnvironmentProbe::detect(hint.as_deref());
    if let Err(error) = activate_environment(&probe) {
        // Home still opens without Python: it plays .gaanim files and
        // explains how to install Python for projects.
        if args.len() == 1 {
            run_player(&[]);
        }
        console::error("python", error);
        console::hint(format!(
            "Install {} (for example `uv python install 3.14`) and retry, or run `gaanim --help`.",
            python_requirement()
        ));
        std::process::exit(2);
    }

    let core_exe = sibling_binary("gaanim-core");
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

/// Run `gaanim-play`, which needs no Python, and exit with its status.
fn run_player(args: &[String]) -> ! {
    let player = sibling_binary("gaanim-play");
    let status = Command::new(&player)
        .args(args)
        .status()
        .unwrap_or_else(|error| {
            console::error(
                "launch",
                format!("failed to start {}: {error}", player.display()),
            );
            std::process::exit(1);
        });
    std::process::exit(status.code().unwrap_or(1));
}

/// The binary `name` installed next to this launcher.
fn sibling_binary(name: &str) -> PathBuf {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let path = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|parent| parent.join(&file)))
        .unwrap_or_else(|| PathBuf::from(&file));
    if !path.is_file() {
        console::error(
            "launch",
            format!("{name} binary not found at {}", path.display()),
        );
        std::process::exit(1);
    }
    path
}

/// Whether the command plays or exports a playback bundle: its input (not
/// its `--output`) is a `.gaanim` file.
fn bundle_input(args: &[String]) -> bool {
    if args.get(1).map(String::as_str) == Some("--diff") {
        return args
            .windows(2)
            .find(|pair| matches!(pair[0].as_str(), "--example" | "-e"))
            .is_some_and(|pair| is_bundle(&pair[1]));
    }
    let (rest, takes_value): (&[String], &[&str]) =
        if args.get(1).map(String::as_str) == Some("check") {
            (&args[2..], &[])
        } else if args.get(1).map(String::as_str) == Some("export") {
            (
                &args[2..],
                &[
                    "--output",
                    "-o",
                    "--quality",
                    "--encoder",
                    "--width",
                    "--height",
                    "--fit",
                    "--from",
                    "--to",
                    "--fps",
                ],
            )
        } else {
            (
                args.get(1..).unwrap_or_default(),
                &["--monitor", "--sections", "--from"],
            )
        };
    let mut index = 0;
    while index < rest.len() {
        let arg = rest[index].as_str();
        if takes_value.contains(&arg) {
            index += 2;
            continue;
        }
        if !arg.starts_with('-') {
            return is_bundle(arg);
        }
        index += 1;
    }
    false
}

fn is_bundle(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gaanim"))
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
    match args.get(1).map(String::as_str) {
        Some("thumbnail") => run_thumbnail(&args[2..]),
        Some("register") => run_association(true),
        Some("unregister") => run_association(false),
        Some("init") => {}
        _ => return false,
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

/// `gaanim thumbnail <BUNDLE> <OUTPUT.png> [SIZE]`: write the cover image of
/// a bundle, scaled so its longest edge is at most SIZE pixels. Needs no GPU
/// or Python, so file managers can run it for every file they list.
fn run_thumbnail(args: &[String]) -> ! {
    let usage = || -> ! {
        console::error(
            "thumbnail",
            "usage: gaanim thumbnail <BUNDLE.gaanim> <OUTPUT.png> [SIZE]",
        );
        std::process::exit(2);
    };
    let (input, output) = match args {
        [input, output] | [input, output, _] => (Path::new(input), Path::new(output)),
        _ => usage(),
    };
    let size = match args.get(2) {
        Some(size) => match size.parse::<u32>() {
            Ok(size) if size > 0 => size,
            _ => usage(),
        },
        None => gaanim_thumbnail::SIZE,
    };
    if let Err(error) = gaanim_thumbnail::extract(input, output, size) {
        console::error("thumbnail", error);
        std::process::exit(1);
    }
    std::process::exit(0);
}

/// `gaanim register` / `gaanim unregister`: associate `.gaanim` files with
/// this Gaanim for the current user, or undo it.
fn run_association(register: bool) -> ! {
    let result = if register {
        std::env::current_exe()
            .map_err(|error| error.to_string())
            .and_then(|exe| gaanim_project::association::register(&exe))
    } else {
        gaanim_project::association::unregister()
    };
    let label = if register { "register" } else { "unregister" };
    match result {
        Ok(report) => {
            for line in &report.done {
                console::detail("Done", line);
            }
            for warning in &report.warnings {
                console::warn(label, warning);
            }
            console::success(
                label,
                if register {
                    ".gaanim files open with Gaanim; right-click one to present it"
                } else {
                    ".gaanim files are no longer associated with Gaanim"
                },
            );
            std::process::exit(0);
        }
        Err(error) => {
            console::error(label, error);
            std::process::exit(1);
        }
    }
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
    fn bundles_play_and_export_without_python() {
        let args = |values: &[&str]| -> Vec<String> {
            std::iter::once("gaanim")
                .chain(values.iter().copied())
                .map(str::to_string)
                .collect()
        };
        assert!(bundle_input(&args(&["talk.gaanim"])));
        assert!(bundle_input(&args(&[
            "--present",
            "--monitor",
            "1",
            "talk.GAANIM"
        ])));
        assert!(bundle_input(&args(&[
            "export",
            "-o",
            "talk.mp4",
            "talk.gaanim"
        ])));
        // Recording a bundle runs the script.
        assert!(!bundle_input(&args(&[
            "export",
            "talk.py",
            "--output",
            "talk.gaanim"
        ])));
        assert!(!bundle_input(&args(&["--from", "intro.gaanim", "talk.py"])));
        assert!(bundle_input(&args(&["check", "talk.gaanim"])));
        assert!(bundle_input(&args(&[
            "--diff",
            "--example",
            "talk.gaanim",
            "--bless"
        ])));
        assert!(!bundle_input(&args(&["--diff", "-e", "talk.py"])));
        assert!(!bundle_input(&args(&["--diff", "-b", "a", "-c", "b"])));
        assert!(!bundle_input(&args(&["check", "talk.py"])));
        assert!(!bundle_input(&args(&[])));
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
