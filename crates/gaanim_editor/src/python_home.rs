//! Auto-detection of the Python runtime for the embedded interpreter.
//!
//! Goal: `gaanim.exe` works when installed on `PATH` without the user
//! setting `PATH`/`VIRTUAL_ENV`/`PYTHONHOME` manually. It detects:
//! 1. an active `uv`/`venv` via `VIRTUAL_ENV` if present (respects activation),
//! 2. a nearby `.venv` by walking up from `script`, `cwd` and `exe` dir,
//! 3. system Python >=3.12 via `py -3.14`/`py -3.12` / `python` fallbacks.
//!
//! On Windows the directory containing `python3*.dll` must be on `PATH`
//! before `Python::initialize()` otherwise `LoadLibrary` fails.
//! `build.rs` sets `/DELAYLOAD` for all versioned dlls so this can be done
//! inside `main()` before the first Python API call.

use std::path::{Path, PathBuf};

/// Ensure `python3*.dll` can be found before `Python::initialize()`.
///
/// Supports any Python >=3.12 (3.12 is the minimum, 3.14 also works).
/// `script_hint` is `Some(script_path)` when launching a script, `None` for
/// bare `gaanim --help` / `check` without script.
/// Returns the detected venv root (if any) so the caller can inject its
/// `site-packages` after initialization.
pub fn ensure_python_available(script_hint: Option<&Path>) -> Option<PathBuf> {
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

/// Inject the venv's `site-packages` into `sys.path` after `Python::initialize()`.
pub fn inject_venv_site_packages(venv_root: &Path) {
    use pyo3::prelude::*;
    let candidates = [
        venv_root.join("Lib").join("site-packages"),
        venv_root.join("lib").join("python3.12").join("site-packages"),
        venv_root.join("lib").join("python3.13").join("site-packages"),
        venv_root.join("lib").join("python3.14").join("site-packages"),
        venv_root.join("lib").join("python3.15").join("site-packages"),
        venv_root.join("lib").join("site-packages"),
    ];
    let site_packages = candidates.iter().find(|p| p.is_dir());
    let Some(site) = site_packages else {
        return;
    };
    let site_str = site.to_string_lossy().to_string();
    let _ = Python::attach(|py| -> PyResult<()> {
        let sys = py.import("sys")?;
        let path = sys.getattr("path")?;
        // Avoid duplicates.
        let contains: bool = path
            .try_iter()?
            .filter_map(|v| v.ok())
            .filter_map(|v| v.extract::<String>().ok())
            .any(|s| s == site_str);
        if !contains {
            path.call_method1("insert", (0, &site_str))?;
        }
        Ok(())
    });
}

#[cfg(windows)]
fn ensure_windows(script_hint: Option<&Path>) -> Option<PathBuf> {
    // 1. Respect active venv if present (uv/venv activation sets VIRTUAL_ENV).
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        let venv_path = PathBuf::from(venv);
        if venv_path.is_dir() {
            if let Some(home) = venv_python_home(&venv_path) {
                prepend_to_path(&home);
                prepend_to_path(venv_path.join("Scripts"));
                // Also try base home's DLL dir.
                return Some(venv_path);
            }
            // Even without pyvenv.cfg, try Scripts on PATH.
            prepend_to_path(venv_path.join("Scripts"));
            return Some(venv_path);
        }
    }

    // 2. Walk-up detection for uv's `.venv` without activation.
    if let Some(venv) = find_venv_walk(script_hint) {
        if let Some(home) = venv_python_home(&venv) {
            prepend_to_path(&home);
        }
        prepend_to_path(venv.join("Scripts"));
        // Also ensure base home candidate probed via pyvenv.cfg is on PATH.
        return Some(venv);
    }

    // 3. Fallback: system Python >=3.12.
    if let Some(home) = fallback_system_python_home() {
        prepend_to_path(&home);
        prepend_to_path(home.join("Scripts"));
        // Home itself is DLL dir (python3.dll or versioned).
    }
    None
}

#[cfg(windows)]
fn venv_python_home(venv_root: &Path) -> Option<PathBuf> {
    let cfg = venv_root.join("pyvenv.cfg");
    if !cfg.is_file() {
        // Try to derive from executable's `pyvenv.cfg` parent? fallback: check if venv has python.exe that knows home.
        // Probe common base locations.
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
        // pyvenv.cfg: `home = C:\...\Python312` or `...\pythoncore-3.14-64`
        if let Some(rest) = line.strip_prefix("home") {
            let rest = rest.trim().trim_start_matches('=').trim();
            if !rest.is_empty() {
                let home = PathBuf::from(rest);
                if home.is_dir() {
                    if has_python_dll(&home) {
                        return Some(home);
                    }
                    // Some installs: home is parent of python.exe, dll beside it.
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
    // Last resort: try probing the venv's python.exe.
    let exe = venv_root.join("Scripts").join("python.exe");
    probe_python_exe_home(&exe)
}

#[cfg(windows)]
fn has_python_dll(dir: &Path) -> bool {
    // Accept any >=3.12 dll: python3.dll (stable ABI) or versioned 312..315
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
    // Run `<exe> -c "import sys; print(sys.base_prefix)"`
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
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            bases.push(parent.to_path_buf());
            // Also try exe's parent parent (e.g., if installed in bin/)
            if let Some(pp) = parent.parent() {
                bases.push(pp.to_path_buf());
            }
        }
    }
    // Deduplicate.
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
                // Also accept venv without pyvenv.cfg but with Scripts/python.exe (conda-like)
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
    // Try `py` launcher with versioned args first (3.12 is minimum, try newer first).
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
                    // Even if dll not in base_prefix, try base_prefix itself.
                    if p.is_dir() {
                        return Some(p);
                    }
                }
            }
        }
    }
    // Try `where python` to get exe path, then base_prefix from it.
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
    // Use dir itself (if file, use parent).
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
    // Prepend so our DLL is found first.
    let mut new_paths = vec![PathBuf::from(&dir_str)];
    new_paths.extend(paths);
    if let Ok(new_var) = std::env::join_paths(new_paths) {
        // SAFETY: single-threaded at startup before Python init.
        unsafe { std::env::set_var("PATH", new_var); }
    }
}
