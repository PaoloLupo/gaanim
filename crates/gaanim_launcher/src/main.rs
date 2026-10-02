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

/// `gaanim relay [init [DIR] [--force] | use <URL> | forget | reset [PATH] |
/// results [PATH] | key <FILE.gaanim|CODE> [--src URL]]`: set up the relay
/// that carries audience poll votes to a presentation, start a new game on
/// a project's session, or hand its key to the web player.
/// Say whether the relay at `url` answers and speaks this Gaanim's protocol.
fn report_relay_version(url: &str) {
    match gaanim_editor::relay_version(url) {
        Ok(version) => match gaanim_project::relay::version_advice(version) {
            None => console::info("relay", format!("version {version}, up to date")),
            Some(advice) => console::warn("relay", advice),
        },
        Err(error) => console::warn("relay", error),
    }
}

fn run_relay(args: &[String]) -> ! {
    use gaanim_project::relay::{self, RelaySource};
    let usage = || -> ! {
        console::error(
            "relay",
            "usage: gaanim relay [init [DIR] [--force] | use <URL> | forget | reset [PATH] | results [PATH] [--output DIR] | key <FILE.gaanim|CODE> [--src URL]]",
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
                report_relay_version(&url);
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
            report_relay_version(&url);
        }
        ["forget"] => {
            relay::save(None).unwrap_or_else(|error| fail(error));
            console::success("relay", "Forgot the saved relay");
        }
        ["reset", rest @ ..] => {
            let path = match rest {
                [] => PathBuf::from("."),
                [path] => PathBuf::from(path),
                _ => usage(),
            };
            let code =
                gaanim_editor::reset_relay_session(&path).unwrap_or_else(|error| fail(error));
            console::success(
                "relay",
                format!("Started a new game on {code}: every vote, answer and player is gone"),
            );
        }
        ["results", rest @ ..] => {
            let mut path = None;
            let mut output = None;
            let mut rest = rest.iter();
            while let Some(arg) = rest.next() {
                match *arg {
                    "--output" | "-o" => {
                        output = Some(PathBuf::from(rest.next().unwrap_or_else(|| usage())))
                    }
                    arg if path.is_none() && !arg.starts_with('-') => {
                        path = Some(PathBuf::from(arg))
                    }
                    _ => usage(),
                }
            }
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            let folder = gaanim_editor::save_relay_results(&path, output.as_deref())
                .unwrap_or_else(|error| fail(error));
            console::success(
                "relay",
                format!("Saved the game's results in {}", folder.display()),
            );
        }
        ["key", target, rest @ ..] => {
            let src = match rest {
                [] => None,
                ["--src", url] => Some(*url),
                _ => usage(),
            };
            let code = if target.to_ascii_lowercase().ends_with(".gaanim") {
                gaanim_bundle::Bundle::open(Path::new(target))
                    .map_err(|error| format!("{target}: {error}"))
                    .and_then(|bundle| {
                        bundle
                            .scene
                            .poll_session
                            .map(|session| session.code)
                            .ok_or_else(|| format!("{target} has no audience polls"))
                    })
                    .unwrap_or_else(|error| fail(error))
            } else {
                target.to_ascii_uppercase()
            };
            let key = relay::key_for_code(&code).unwrap_or_else(|error| fail(error));
            console::info("relay", format!("presenter key of session {code}: {key}"));
            match src {
                Some(src) => {
                    console::detail("Present", format!("{WEB_PLAYER}?src={src}#clave={key}"))
                }
                None => console::detail(
                    "Present",
                    format!(
                        "add #clave={key} to the web player's link to present this session there"
                    ),
                ),
            }
            console::hint(
                "Anyone with the key controls the session's polls: share it only with whoever presents.",
            );
        }
        _ => usage(),
    }
    std::process::exit(0);
}

/// The web player, which `gaanim relay key --src` links to.
const WEB_PLAYER: &str = "https://paololupo.github.io/gaanim/reproductor/";

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
