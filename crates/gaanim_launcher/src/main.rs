//! `gaanim`: the Gaanim application and command line, in one executable.
//!
//! The engine is a shared library (`gaanim_engine`) and Python support a
//! plugin loaded when a script runs (see `python.rs`), so this executable
//! starts without Python: Home, playback bundles (`.gaanim`), and commands
//! such as `init`, `thumbnail`, `register` and `relay` need none.

// Take the engine crates from the shared engine library.
use gaanim_engine as _;

mod app;
mod control;
mod file_watcher;
mod hot_reload;
mod inspect;
mod python;
mod runtime_benchmark;
mod startup;

use gaanim_core::console;
use gaanim_project::help::{self, Topic};
use gaanim_project::{
    CreateProjectOptions, CreateWorkspaceOptions, ProjectKind, create_project, create_workspace,
};
use std::path::{Path, PathBuf};

fn main() {
    startup::begin();
    // The files an update replaced, kept while the old Gaanim ran.
    if let Ok(install) = gaanim_editor::update::install_dir() {
        gaanim_editor::update::clean_previous(&install);
    }
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
        } else if args.get(1).map(String::as_str) == Some("inspect") {
            help::print(Topic::Inspect);
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
        Some("pack") => run_pack(&args[2..]),
        Some("register") => run_association(true),
        Some("unregister") => run_association(false),
        Some("relay") => run_relay(&args[2..]),
        Some("update") => run_update(&args[2..]),
        Some("init") => {}
        _ => return false,
    }
    let parsed = match parse_init_args(&args[2..]) {
        Ok(InitTarget::Project(parsed)) => parsed,
        Ok(InitTarget::Workspace(options)) => {
            init_workspace(&options);
            return true;
        }
        Err(error) => {
            console::error("init", error);
            console::hint("Run `gaanim init --help` for usage.");
            std::process::exit(2);
        }
    };
    let project = create_project(&parsed).unwrap_or_else(|error| {
        console::error("init", error);
        std::process::exit(2);
    });
    // A new project inside a workspace that does not list it gets the
    // workspace's files, not its own environment, until it is listed.
    let outside = project
        .workspace
        .is_none()
        .then(|| gaanim_project::workspace_around(&project.root))
        .flatten();
    if let Some(workspace) = &outside {
        console::warn(
            "init",
            format!(
                "{} is inside workspace {}, whose members do not include it",
                console::display_path(&project.root),
                workspace.name
            ),
        );
        if let Some(line) = workspace.member_line(&project.root) {
            console::hint(format!(
                "Add it in {}: {line}",
                console::display_path(&workspace.manifest_path)
            ));
        }
    }
    let venv = match outside {
        Some(_) => Err("the project is not a member of its workspace yet".to_string()),
        None => gaanim_project::provision_authoring_package(project.environment_root()),
    };
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

/// `gaanim update [--check]`: install the newest Gaanim release for this
/// system over this installation, or with `--check` only say whether there
/// is one. Needs no Python.
fn run_update(args: &[String]) -> ! {
    use gaanim_editor::update::{self, Step};
    let check_only = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => {
            console::error("update", "usage: gaanim update [--check]");
            std::process::exit(2);
        }
    };
    if update::package_suffix().is_none() {
        console::error(
            "update",
            "Gaanim has no package for this system yet; build it from source",
        );
        std::process::exit(1);
    }
    let release = match update::check(true) {
        Ok(Some(release)) => release,
        Ok(None) => {
            console::success(
                "update",
                format!("Gaanim {} is the newest version", env!("CARGO_PKG_VERSION")),
            );
            std::process::exit(0);
        }
        Err(error) => {
            console::error("update", error);
            std::process::exit(1);
        }
    };
    console::info(
        "update",
        format!(
            "Gaanim {} is available (this is {})",
            release.version,
            env!("CARGO_PKG_VERSION")
        ),
    );
    console::detail("Notes", &release.page);
    if check_only {
        console::hint("Run `gaanim update` to install it.");
        std::process::exit(0);
    }
    let mut shown = None;
    let installed = update::install(&release, |step| match step {
        Step::Downloading { done, total } if total > 0 => {
            // A line at each quarter.
            let quarter = done * 4 / total;
            if shown != Some(quarter) {
                shown = Some(quarter);
                console::info(
                    "download",
                    format!("{} % of {} MB", quarter * 25, total.div_ceil(1 << 20)),
                );
            }
        }
        Step::Downloading { .. } => {}
        Step::Verifying => console::info("verify", "Checking the download's SHA-256"),
        Step::Unpacking => console::info("unpack", "Unpacking the package"),
        Step::Installing => console::info("install", "Replacing this installation's files"),
    });
    match installed {
        Ok(executable) => {
            console::success("update", format!("Installed Gaanim {}", release.version));
            console::detail("Program", executable.display());
            console::hint(
                "Each project installs the new authoring package the next time it opens.",
            );
            std::process::exit(0);
        }
        Err(error) => {
            console::error("update", error);
            std::process::exit(1);
        }
    }
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

/// `gaanim pack <BUNDLE> <OUTPUT> [--web]`: write a bundle again in the
/// current format, whose players open it with its first chunk and download
/// the rest as they play; `--web` also stores paths on a grid and leaves out
/// what only `check` and motion-blurred videos use, for publishing in other
/// pages. Needs no GPU or Python.
fn run_pack(args: &[String]) -> ! {
    let usage = || -> ! {
        console::error(
            "pack",
            "usage: gaanim pack <BUNDLE.gaanim> <OUTPUT.gaanim> [--web]",
        );
        std::process::exit(2);
    };
    let web = args.iter().any(|arg| arg == "--web");
    let files: Vec<&String> = args.iter().filter(|arg| *arg != "--web").collect();
    let [input, output] = files[..] else { usage() };
    if input.starts_with("--") || output.starts_with("--") {
        usage();
    }
    let (input, output) = (Path::new(input), Path::new(output));
    let packed = (|| -> Result<(u64, u64, usize), String> {
        let before = std::fs::metadata(input)
            .map_err(|error| error.to_string())?
            .len();
        let mut bundle = gaanim_bundle::Bundle::open(input).map_err(|error| error.to_string())?;
        let mut temporary = output.as_os_str().to_owned();
        temporary.push(".partial");
        let temporary = std::path::PathBuf::from(temporary);
        let file = std::fs::File::create(&temporary).map_err(|error| error.to_string())?;
        let options = gaanim_bundle::RepackOptions { web };
        let written = gaanim_bundle::repack_with(
            &mut bundle,
            std::io::BufWriter::new(file),
            format!("gaanim {}", env!("CARGO_PKG_VERSION")),
            &options,
        )
        .and_then(|mut file| Ok(std::io::Write::flush(&mut file)?));
        if let Err(error) = written {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        std::fs::rename(&temporary, output).map_err(|error| error.to_string())?;
        let after = std::fs::metadata(output)
            .map_err(|error| error.to_string())?
            .len();
        Ok((before, after, bundle.frame_count()))
    })();
    match packed {
        Ok((before, after, frames)) => {
            console::success(
                "pack",
                format!(
                    "{} frames{}: {} → {} ({:.1} → {:.1} MB)",
                    frames,
                    if web { " for the web" } else { "" },
                    console::display_path(input),
                    console::display_path(output),
                    before as f64 / 1e6,
                    after as f64 / 1e6
                ),
            );
            std::process::exit(0);
        }
        Err(error) => {
            console::error("pack", error);
            std::process::exit(1);
        }
    }
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

/// What `gaanim init` creates.
#[derive(Debug, PartialEq, Eq)]
enum InitTarget {
    Project(CreateProjectOptions),
    Workspace(CreateWorkspaceOptions),
}

fn parse_init_args(args: &[String]) -> Result<InitTarget, String> {
    let kind = args.first().ok_or_else(|| {
        "missing project kind; available kinds: video, slides, workspace".to_string()
    })?;
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
    if kind == "workspace" {
        return Ok(InitTarget::Workspace(CreateWorkspaceOptions {
            directory: directory
                .unwrap_or_else(|| PathBuf::from(gaanim_project::DEFAULT_WORKSPACE_DIRECTORY)),
            force,
        }));
    }
    let kind = ProjectKind::parse(kind).map_err(|error| {
        if error.starts_with("unknown") {
            format!("{error}, workspace")
        } else {
            error
        }
    })?;
    Ok(InitTarget::Project(CreateProjectOptions {
        kind,
        directory: directory.unwrap_or_else(|| PathBuf::from(kind.default_directory())),
        force,
    }))
}

/// `gaanim init workspace`: the workspace, its Git repository and its
/// Python environment, then what to do next.
fn init_workspace(options: &CreateWorkspaceOptions) {
    let workspace = create_workspace(options).unwrap_or_else(|error| {
        console::error("init", error);
        std::process::exit(2);
    });
    let git = gaanim_project::init_git_repository(&workspace.root);
    let venv = gaanim_project::provision_authoring_package(&workspace.root);
    let root = console::display_path(&workspace.root);
    console::success(
        "init",
        format!("Created workspace {}: {root}", workspace.name),
    );
    console::detail(
        "Members",
        "videos/* and talks/* (edit `members` in gaanim.toml)",
    );
    console::detail(
        "Shared",
        "src/ for Python, assets/ for images, fonts and sounds",
    );
    let videos = workspace.root.join("videos");
    console::detail(
        "First",
        format!("gaanim {}", console::display_path(&videos.join("intro"))),
    );
    console::detail(
        "New",
        format!(
            "gaanim init video {}",
            console::display_path(&videos.join("<name>"))
        ),
    );
    console::detail("Home", format!("gaanim {root}"));
    console::detail("Check", format!("gaanim check {root}"));
    if git {
        console::detail("Git", "initialized a repository");
    }
    match venv {
        Ok(venv) => console::detail("Python", console::display_path(&venv)),
        Err(error) => console::warn(
            "python",
            format!("authoring environment not ready: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_video_slides_and_workspace_init_kinds() {
        let kind = |name: &str| match parse_init_args(&[name.into()]).unwrap() {
            InitTarget::Project(options) => Some(options.kind),
            InitTarget::Workspace(_) => None,
        };
        assert_eq!(kind("video"), Some(ProjectKind::Video));
        assert_eq!(kind("slides"), Some(ProjectKind::Slides));
        assert_eq!(
            parse_init_args(&["workspace".into(), "canal".into(), "--force".into()]).unwrap(),
            InitTarget::Workspace(CreateWorkspaceOptions {
                directory: PathBuf::from("canal"),
                force: true,
            })
        );
        assert!(parse_init_args(&["presentation".into()]).is_err());
        assert!(parse_init_args(&["thesis".into()]).is_err());
        assert!(
            parse_init_args(&["film".into()])
                .unwrap_err()
                .contains("workspace")
        );
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
