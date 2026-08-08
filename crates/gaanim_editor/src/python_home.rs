//! Embedded-Python path integration after `gaanim_project` selects a runtime.

use std::path::Path;

/// Inject the selected virtual environment's site-packages into the embedded
/// interpreter without requiring activation in the user's shell.
pub fn inject_venv_site_packages(venv_root: &Path) {
    use pyo3::prelude::*;

    let candidates = [
        venv_root.join("Lib").join("site-packages"),
        venv_root
            .join("lib")
            .join("python3.12")
            .join("site-packages"),
        venv_root
            .join("lib")
            .join("python3.13")
            .join("site-packages"),
        venv_root
            .join("lib")
            .join("python3.14")
            .join("site-packages"),
        venv_root
            .join("lib")
            .join("python3.15")
            .join("site-packages"),
        venv_root.join("lib").join("site-packages"),
    ];
    let Some(site) = candidates.iter().find(|path| path.is_dir()) else {
        return;
    };
    let site = site.to_string_lossy().to_string();
    let _ = Python::attach(|py| -> PyResult<()> {
        let sys = py.import("sys")?;
        let path = sys.getattr("path")?;
        let contains = path
            .try_iter()?
            .filter_map(Result::ok)
            .filter_map(|value| value.extract::<String>().ok())
            .any(|value| value == site);
        if !contains {
            path.call_method1("insert", (0, &site))?;
        }
        Ok(())
    });
}
