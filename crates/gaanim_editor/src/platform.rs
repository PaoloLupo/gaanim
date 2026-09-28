//! Services of the host platform that the web build does not have.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/// Open a file, folder or URL with the system's default application.
pub fn open(target: impl AsRef<OsStr>) -> io::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        open::that(target)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = target;
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// [`open`] without waiting for the application to start.
pub fn open_detached(target: impl AsRef<OsStr>) -> io::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        open::that_detached(target)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = target;
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// Ask for a file with the native dialog. `filters` are `(name, extensions)`
/// pairs. The web has no blocking dialog; it picks files with an `<input>`.
pub fn pick_file(
    title: &str,
    directory: Option<&Path>,
    filters: &[(&str, &[&str])],
) -> Option<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(directory) = directory {
            dialog = dialog.set_directory(directory);
        }
        for (name, extensions) in filters {
            dialog = dialog.add_filter(*name, extensions);
        }
        dialog.pick_file()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (title, directory, filters);
        None
    }
}

/// Ask for a folder with the native dialog; always `None` on the web.
pub fn pick_folder(title: &str, directory: Option<&Path>) -> Option<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(directory) = directory {
            dialog = dialog.set_directory(directory);
        }
        dialog.pick_folder()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (title, directory);
        None
    }
}
