//! `gaanim`: the Gaanim application and command line, in one executable.
//!
//! The engine is a shared library (`gaanim_engine`) and Python support a
//! plugin loaded when a script runs (see `python.rs`), so this executable
//! starts without Python: Home, playback bundles (`.gaanim`), and commands
//! such as `init`, `thumbnail`, `register` and `relay` need none.

// Take the engine crates from the shared engine library.
use gaanim_engine as _;

mod app;
mod file_watcher;
mod hot_reload;
mod python;
mod runtime_benchmark;

use gaanim_core::console;
use gaanim_project::help::{self, Topic};
use gaanim_project::{CreateProjectOptions, ProjectKind, create_project};
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if handle_no_python_commands(&args) {
        return;
    }
    app::run();
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
        Some("relay") => run_relay(&args[2..]),
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

/// `gaanim relay [init [DIR] [--force] | use <URL> | forget]`: set up the
/// relay that carries audience poll votes to a presentation.
fn run_relay(args: &[String]) -> ! {
    use gaanim_project::relay::{self, RelaySource};
    let usage = || -> ! {
        console::error(
            "relay",
            "usage: gaanim relay [init [DIR] [--force] | use <URL> | forget]",
        );
        std::process::exit(2);
    };
    let fail = |error: String| -> ! {
        console::error("relay", error);
        std::process::exit(1);
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => match relay::resolve(None) {
            Some((url, source)) => {
                let from = match source {
                    RelaySource::Environment => relay::RELAY_ENV,
                    RelaySource::Project => "gaanim.toml",
                    RelaySource::User => "gaanim relay use",
                };
                console::info("relay", format!("{url} (from {from})"));
                console::hint("A project's `[polls] relay` in gaanim.toml overrides it.");
            }
            None => {
                console::info("relay", "no relay is set; audience polls cannot take votes");
                console::hint("Run `gaanim relay init` to write one you can deploy for free.");
            }
        },
        ["init", rest @ ..] => {
            let force = rest.contains(&"--force");
            let paths: Vec<&&str> = rest.iter().filter(|arg| **arg != "--force").collect();
            let directory = match paths.as_slice() {
                [] => PathBuf::from("gaanim-relay"),
                [directory] => PathBuf::from(directory),
                _ => usage(),
            };
            relay::write_template(&directory, force).unwrap_or_else(|error| fail(error));
            console::success(
                "relay",
                format!("Wrote the relay to {}", directory.display()),
            );
            console::detail(
                "Deploy",
                format!("cd {} && npx wrangler deploy", directory.display()),
            );
            console::detail(
                "Then",
                "gaanim relay use <the https address wrangler prints>",
            );
        }
        ["use", url] => {
            let url = relay::save(Some(url))
                .unwrap_or_else(|error| fail(error))
                .expect("a saved relay has an address");
            console::success("relay", format!("Audience polls use {url}"));
        }
        ["forget"] => {
            relay::save(None).unwrap_or_else(|error| fail(error));
            console::success("relay", "Forgot the saved relay");
        }
        _ => usage(),
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
}
