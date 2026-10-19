//! Embedded-Python path integration after `gaanim_project` selects a runtime.

use std::path::Path;

/// Inject the selected virtual environment's site-packages into the embedded
/// interpreter without requiring activation in the user's shell: first on
/// `sys.path`, ahead of the runtime's own packages, with its `.pth` files
/// processed as Python's startup does, so editable installs and the members
/// of a uv workspace import.
pub fn inject_venv_site_packages(venv_root: &Path) {
    use pyo3::prelude::*;

    let candidates = [
        venv_root.join("Lib").join("site-packages"),
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
        // Already on `sys.path`, the folder is not added again; its `.pth`
        // files add their folders and run their `import` lines.
        py.import("site")?.call_method1("addsitedir", (&site,))?;
        Ok(())
    });
}
