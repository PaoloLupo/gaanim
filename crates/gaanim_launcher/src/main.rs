//! gaanim launcher - sets up Python DLL path before spawning gaanim-core.exe
//!
//! This binary does NOT link against python, so it can start without python3.dll on PATH.
//! It detects a nearby uv/.venv or system Python >=3.12, prepends its directory to PATH,
//! then execs `gaanim-core.exe` (or `gaanim-core` on unix) with the same args.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Handle commands that don't need Python at all (like --help, init)
    // so `gaanim --help` works even without Python installed, matching
    // the earlier behavior where `gaanim` could run without python.
    if handle_no_python_commands(&args) {
        return;
    }

    // Detect script hint: first non-flag arg that is not --diff etc? Simple: look for last arg that exists as file/dir
    let script_hint = find_script_hint(&args);

    let venv_root = ensure_python_available(script_hint.as_deref());

    // Find gaanim-core binary next to this launcher
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("gaanim-core.exe"));
    let core_exe = exe
        .parent()
        .map(|p| p.join(if cfg!(windows) { "gaanim-core.exe" } else { "gaanim-core" }))
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "gaanim-core.exe" } else { "gaanim-core" }));

    // If core doesn't exist, try `gaanim.exe` in same dir (dev fallback where launcher not used)
    let core_exe = if core_exe.is_file() {
        core_exe
    } else {
        // Fallback: try target/debug/gaanim.exe relative? Just use gaanim-core name
        core_exe
    };

    if !core_exe.is_file() {
        // If we're in dev and core is actually `gaanim.exe` itself (no launcher built), try to run with current PATH
        // Check if this launcher IS the core (avoid recursion)
        if exe.is_file() && is_core_binary(&exe) {
            eprintln!("gaanim launcher: core binary not found at {}", core_exe.display());
            std::process::exit(1);
        }
    }

    // Prepare PATH for child
    // venv injection already done via ensure_python_available which modified this process's PATH
    // Also ensure we propagate any added PATH to child
    let mut child = Command::new(&core_exe);
    child.args(&args[1..]);
    // Inherit modified PATH
    if let Some(venv) = venv_root {
        // Also ensure site-packages handling is done by core; launcher just ensures DLL path
        let _ = venv;
    }
    // Use same env (PATH already modified)
    let status = child.status().unwrap_or_else(|e| {
        eprintln!("gaanim: failed to spawn {}: {}", core_exe.display(), e);
        std::process::exit(1);
    });
    std::process::exit(status.code().unwrap_or(1));
}

fn is_core_binary(path: &Path) -> bool {
    // Heuristic: if binary contains python dependency, it's core. Launcher does not.
    // We can't easily check, so assume false.
    let _ = path;
    false
}

const THESIS_PRESENTATION_TEMPLATE: &str =
    include_str!("../../../templates/thesis_presentation.py");
const VIDEO_PROJECT_TEMPLATE: &str = include_str!("../../../templates/video_project.py");
const PRESENTATION_PROJECT_TEMPLATE: &str =
    include_str!("../../../templates/presentation_project.py");

const PROJECT_GITIGNORE: &str = r#"exports/*
!exports/.gitkeep
snapshots/
__pycache__/
*.mp4
*.webm
*.webp
*.gif
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitTemplate {
    Video,
    Presentation,
    Thesis,
}

impl InitTemplate {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "video" => Ok(Self::Video),
            "presentation" | "slides" => Ok(Self::Presentation),
            "thesis" => Ok(Self::Thesis),
            _ => Err(format!(
                "unknown template `{value}`; available templates: video, presentation, thesis"
            )),
        }
    }
    const fn name(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Presentation => "presentation",
            Self::Thesis => "thesis",
        }
    }
    const fn default_directory(self) -> &'static str {
        match self {
            Self::Video => "gaanim-video",
            Self::Presentation => "gaanim-presentation",
            Self::Thesis => "gaanim-thesis",
        }
    }
    const fn source(self) -> &'static str {
        match self {
            Self::Video => VIDEO_PROJECT_TEMPLATE,
            Self::Presentation => PRESENTATION_PROJECT_TEMPLATE,
            Self::Thesis => THESIS_PRESENTATION_TEMPLATE,
        }
    }
    const fn is_presentation(self) -> bool {
        matches!(self, Self::Presentation | Self::Thesis)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InitArgs {
    template: InitTemplate,
    directory: PathBuf,
    force: bool,
}

fn handle_no_python_commands(args: &[String]) -> bool {
    // No args -> show usage (no python needed)
    if args.len() <= 1 {
        eprintln!("usage:");
        eprintln!("  gaanim [--present] [--monitor <INDEX>] <SCRIPT_OR_PROJECT>");
        eprintln!("  gaanim init <video|presentation|thesis> [DIRECTORY] [--force]");
        eprintln!("  gaanim check <SCRIPT_OR_PROJECT> [--strict]");
        eprintln!("  gaanim --diff --baseline <DIR> --current <DIR> [OPTIONS]");
        std::process::exit(2);
    }
    // --help / -h anywhere -> show help without needing python
    if args.iter().any(|a| a == "--help" || a == "-h") {
        // Distinguish init help vs general help
        if args.iter().any(|a| a == "init") {
            print_init_help();
        } else if args.iter().any(|a| a == "check") {
            print_check_help();
        } else if args.iter().any(|a| a == "--diff") {
            print_diff_help();
        } else {
            eprintln!("gaanim — GPU-accelerated vector animation engine (hot-reload viewer)");
            eprintln!();
            eprintln!("usage:");
            eprintln!("  gaanim [--present] [--monitor <INDEX>] <SCRIPT_OR_PROJECT>");
            eprintln!("  gaanim init <video|presentation|thesis> [DIRECTORY] [--force]");
            eprintln!("  gaanim check <SCRIPT_OR_PROJECT> [--strict]");
            eprintln!("  gaanim --diff --example <SCRIPT_OR_PROJECT> [OPTIONS]");
        }
        std::process::exit(0);
    }
    // init command -> handle without python
    if args.get(1).map(|s| s.as_str()) == Some("init") {
        let init_args: Vec<String> = args.iter().skip(2).cloned().collect();
        // Check for --help in init args already handled above, but also handle here
        if init_args.iter().any(|a| a == "--help" || a == "-h") {
            print_init_help();
            std::process::exit(0);
        }
        let parsed = parse_init_args(&init_args).unwrap_or_else(|e| {
            eprintln!("gaanim init: {e}");
            eprintln!("Run `gaanim init --help` for usage.");
            std::process::exit(2);
        });
        if let Err(e) = create_project(&parsed) {
            eprintln!("gaanim init: {e}");
            std::process::exit(2);
        }
        println!(
            "Created {} project: {}",
            parsed.template.name(),
            parsed.directory.display()
        );
        println!("Edit: {}", parsed.directory.join("main.py").display());
        println!("Preview: gaanim {}", parsed.directory.display());
        println!("Check: gaanim check {}", parsed.directory.display());
        if parsed.template.is_presentation() {
            println!(
                "Present: gaanim --present --monitor 1 {}",
                parsed.directory.display()
            );
        } else {
            println!("Export: set GAANIM_EXPORT=exports/video.mp4, then run the project");
        }
        std::process::exit(0);
    }
    false
}

fn parse_init_args(args: &[String]) -> Result<InitArgs, String> {
    let template = args
        .first()
        .ok_or_else(|| {
            "missing template name; available templates: video, presentation, thesis".to_string()
        })
        .and_then(|v| InitTemplate::parse(v))?;
    let mut directory = None;
    let mut force = false;
    for arg in &args[1..] {
        match arg.as_str() {
            "--force" => force = true,
            v if v.starts_with('-') => return Err(format!("unknown option `{v}`")),
            v if directory.is_none() => directory = Some(PathBuf::from(v)),
            v => return Err(format!("unexpected argument `{v}`")),
        }
    }
    Ok(InitArgs {
        template,
        directory: directory.unwrap_or_else(|| PathBuf::from(template.default_directory())),
        force,
    })
}

fn create_project(args: &InitArgs) -> Result<(), String> {
    if args.directory.is_file() {
        return Err(format!(
            "{} is a file; choose a project directory",
            args.directory.display()
        ));
    }
    let project_name = args
        .directory
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(args.template.default_directory());
    let manifest = format!(
        "name = \"{}\"\nkind = \"{}\"\nentry = \"main.py\"\nassets_dir = \"assets\"\noutput_dir = \"exports\"\n",
        escape_manifest_value(project_name),
        args.template.name(),
    );
    let readme = project_readme(project_name, args.template);
    let files = [
        (args.directory.join("main.py"), args.template.source()),
        (args.directory.join("gaanim.toml"), manifest.as_str()),
        (args.directory.join(".gitignore"), PROJECT_GITIGNORE),
        (args.directory.join("README.md"), readme.as_str()),
        (args.directory.join("assets").join(".gitkeep"), ""),
        (args.directory.join("exports").join(".gitkeep"), ""),
    ];
    if !args.force
        && let Some((path, _)) = files.iter().find(|(p, _)| p.exists())
    {
        return Err(format!(
            "{} already exists (use --force to update scaffold files)",
            path.display()
        ));
    }
    for (path, source) in files {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, source)
            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    }
    Ok(())
}

fn escape_manifest_value(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

fn project_readme(name: &str, template: InitTemplate) -> String {
    let presentation = if template.is_presentation() {
        "\n## Presentar\n\n```powershell\ngaanim --present --monitor 1 .\n```\n"
    } else {
        "\n## Exportar\n\n```powershell\n$env:GAANIM_EXPORT = \"exports/video.mp4\"\ngaanim .\nRemove-Item Env:GAANIM_EXPORT\n```\n"
    };
    format!(
        "# {name}\n\nProyecto `{}` generado por Gaanim.\n\n## Editar y previsualizar\n\n\
         Edita `main.py` y ejecuta:\n\n```powershell\ngaanim .\n```\n\n\
         Los recursos van en `assets/`; las salidas generadas van en `exports/`.\n\
         {presentation}\n## Validar\n\n```powershell\ngaanim check .\n```\n",
        template.name()
    )
}

fn print_init_help() {
    println!(
        r#"gaanim init - create a runnable Gaanim project

USAGE:
    gaanim init <TEMPLATE> [DIRECTORY] [--force]

ARGUMENTS:
    video               Animated-video starter
    presentation        Semantic slide-deck starter
    thesis              Complete Spanish thesis-defense project
    DIRECTORY           Project directory (defaults to gaanim-<template>)

OPTIONS:
    --force             Update scaffold files without deleting user assets
    -h, --help          Print this help"#
    );
}

fn print_check_help() {
    println!(
        r#"gaanim check - validate a video or presentation project

USAGE:
    gaanim check <SCRIPT_OR_PROJECT> [--strict]

CHECKS:
    entry script and timeline duration
    semantic slides, notes and named reveal steps when present
    16:9 projector aspect ratio for presentations
    unresolved template placeholders

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
    -e, --example <SCRIPT_OR_PROJECT>
                                     Capture and compare one project automatically
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

fn find_script_hint(args: &[String]) -> Option<PathBuf> {
    // args[0] is exe, rest are flags. Look for last arg that looks like a path and exists.
    for arg in args.iter().rev() {
        if arg.starts_with('-') {
            continue;
        }
        let p = PathBuf::from(arg);
        if p.exists() {
            return Some(p);
        }
        // Also handle `gaanim check <path>` or `gaanim --diff --example <path>` patterns
        // So any non-flag arg is potential script hint
        if !arg.is_empty() && !arg.contains('=') {
            // Return it even if not exists yet (for init where project doesn't exist)
            // But for venv walk, we need existing dir. So check if parent exists?
            // For now return path if it looks like a file path
            if arg.ends_with(".py") || arg.contains('/') || arg.contains('\\') || !arg.contains('.') {
                return Some(p);
            }
        }
    }
    None
}

// --- Python detection (copied from gaanim_editor::python_home, no pyo3) ---

fn ensure_python_available(script_hint: Option<&Path>) -> Option<PathBuf> {
    #[cfg(not(windows))]
    {
        let _ = script_hint;
        return None;
    }
    #[cfg(windows)]
    {
        return ensure_windows(script_hint);
    }
}

#[cfg(windows)]
fn ensure_windows(script_hint: Option<&Path>) -> Option<PathBuf> {
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        let venv_path = PathBuf::from(venv);
        if venv_path.is_dir() {
            if let Some(home) = venv_python_home(&venv_path) {
                prepend_to_path(&home);
                prepend_to_path(venv_path.join("Scripts"));
                return Some(venv_path);
            }
            prepend_to_path(venv_path.join("Scripts"));
            return Some(venv_path);
        }
    }
    if let Some(venv) = find_venv_walk(script_hint) {
        if let Some(home) = venv_python_home(&venv) {
            prepend_to_path(&home);
        }
        prepend_to_path(venv.join("Scripts"));
        return Some(venv);
    }
    if let Some(home) = fallback_system_python_home() {
        prepend_to_path(&home);
        prepend_to_path(home.join("Scripts"));
    }
    None
}

#[cfg(windows)]
fn venv_python_home(venv_root: &Path) -> Option<PathBuf> {
    let cfg = venv_root.join("pyvenv.cfg");
    if !cfg.is_file() {
        let exe = venv_root.join("Scripts").join("python.exe");
        if exe.is_file() {
            if let Some(home) = probe_python_exe_home(&exe) {
                return Some(home);
            }
        }
        return None;
    }
    let content = std::fs::read_to_string(cfg).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("home") {
            let rest = rest.trim().trim_start_matches('=').trim();
            if !rest.is_empty() {
                let home = PathBuf::from(rest);
                if home.is_dir() {
                    if has_python_dll(&home) {
                        return Some(home);
                    }
                    if home.join("python.exe").is_file() {
                        return Some(home);
                    }
                }
            }
        }
        if let Some(rest) = line.strip_prefix("executable") {
            let rest = rest.trim().trim_start_matches('=').trim();
            if !rest.is_empty() {
                let exe = PathBuf::from(rest);
                if let Some(parent) = exe.parent() {
                    if has_python_dll(parent) {
                        return Some(parent.to_path_buf());
                    }
                }
            }
        }
    }
    let exe = venv_root.join("Scripts").join("python.exe");
    probe_python_exe_home(&exe)
}

#[cfg(windows)]
fn has_python_dll(dir: &Path) -> bool {
    for name in &[
        "python3.dll",
        "python312.dll",
        "python313.dll",
        "python314.dll",
        "python315.dll",
    ] {
        if dir.join(name).is_file() {
            return true;
        }
    }
    false
}

#[cfg(windows)]
fn probe_python_exe_home(exe: &Path) -> Option<PathBuf> {
    if !exe.is_file() {
        return None;
    }
    let output = std::process::Command::new(exe)
        .arg("-c")
        .arg("import sys; print(sys.base_prefix)")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        return None;
    }
    let p = PathBuf::from(s);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

#[cfg(windows)]
fn find_venv_walk(script_hint: Option<&Path>) -> Option<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(script) = script_hint {
        if let Some(parent) = script.parent() {
            bases.push(parent.to_path_buf());
        } else {
            bases.push(PathBuf::from("."));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            bases.push(parent.to_path_buf());
            if let Some(pp) = parent.parent() {
                bases.push(pp.to_path_buf());
            }
        }
    }
    bases.sort();
    bases.dedup();
    const VENV_NAMES: &[&str] = &[".venv", "venv", "env", ".venv312"];
    const MAX_DEPTH: usize = 4;
    for base in bases {
        let mut cur = base.clone();
        for _ in 0..MAX_DEPTH {
            for name in VENV_NAMES {
                let cand = cur.join(name);
                if cand.join("pyvenv.cfg").is_file() {
                    return Some(cand);
                }
                if cand.join("Scripts").join("python.exe").is_file() && cand.is_dir() {
                    return Some(cand);
                }
            }
            match cur.parent() {
                Some(parent) => cur = parent.to_path_buf(),
                None => break,
            }
        }
    }
    None
}

#[cfg(windows)]
fn fallback_system_python_home() -> Option<PathBuf> {
    for (prog, args) in [
        ("py", vec!["-3.14", "-c", "import sys; print(sys.base_prefix)"]),
        ("py", vec!["-3.13", "-c", "import sys; print(sys.base_prefix)"]),
        ("py", vec!["-3.12", "-c", "import sys; print(sys.base_prefix)"]),
        ("py", vec!["-3", "-c", "import sys; print(sys.base_prefix)"]),
        ("python", vec!["-c", "import sys; print(sys.base_prefix)"]),
        ("python3", vec!["-c", "import sys; print(sys.base_prefix)"]),
    ] {
        if let Ok(output) = std::process::Command::new(prog).args(&args).output() {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !s.is_empty() {
                    let p = PathBuf::from(&s);
                    if p.is_dir() && has_python_dll(&p) {
                        return Some(p);
                    }
                    if p.is_dir() {
                        return Some(p);
                    }
                }
            }
        }
    }
    if let Ok(output) = std::process::Command::new("where").arg("python").output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            for line in s.lines() {
                let exe = PathBuf::from(line.trim());
                if exe.is_file() {
                    if let Some(home) = probe_python_exe_home(&exe) {
                        return Some(home);
                    }
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn prepend_to_path(dir: impl AsRef<Path>) {
    let dir = dir.as_ref();
    if !dir.is_dir() && !dir.is_file() {
        return;
    }
    let dir_str = if dir.is_file() {
        dir.parent().unwrap_or(dir).to_string_lossy().to_string()
    } else {
        dir.to_string_lossy().to_string()
    };
    let current = std::env::var_os("PATH").unwrap_or_default();
    let paths: Vec<PathBuf> = std::env::split_paths(&current).collect();
    let needle = PathBuf::from(&dir_str);
    if paths.iter().any(|p| p == &needle) {
        return;
    }
    let mut new_paths = vec![PathBuf::from(&dir_str)];
    new_paths.extend(paths);
    if let Ok(new_var) = std::env::join_paths(new_paths) {
        unsafe { std::env::set_var("PATH", new_var); }
    }
}
