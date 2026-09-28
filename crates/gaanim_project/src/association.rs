//! Associate `.gaanim` files with Gaanim for the current user: a double
//! click plays them, a "Present" action presents them, and file managers
//! show their cover image. Nothing needs administrator rights.
//!
//! - Windows: keys under `HKCU\Software\Classes`, including the thumbnail
//!   handler DLL when it is installed next to `gaanim.exe`.
//! - Linux: a MIME type, a `.desktop` entry, a thumbnailer and icons under
//!   the XDG data directory, then the desktop's own tools refresh them.

use std::path::{Path, PathBuf};

/// The Windows thumbnail handler DLL `register` looks for next to the launcher.
pub use gaanim_thumbnail::WINDOWS_HANDLER_DLL as THUMBNAIL_HANDLER_DLL;

/// MIME type of playback bundles.
pub const MIME_TYPE: &str = "application/x-gaanim";
/// Windows programmatic identifier of the file type.
pub const PROG_ID: &str = "Gaanim.Bundle";
/// Name shown for the file type.
const TYPE_NAME: &str = "Paquete de Gaanim";
/// Shell extension slot of thumbnail providers (`IThumbnailProvider`).
#[cfg_attr(not(windows), allow(dead_code))]
const THUMBNAIL_PROVIDER_SLOT: &str = "{e357fccd-a995-4576-b01f-234630154e96}";

/// What `register` or `unregister` did, one line per step, and what it
/// could not do.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub done: Vec<String>,
    pub warnings: Vec<String>,
}

/// The `gaanim` launcher installed next to `exe`, or `exe` itself when
/// there is none (a development build).
pub fn launcher_next_to(exe: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "gaanim.exe"
    } else {
        "gaanim"
    };
    exe.parent()
        .map(|dir| dir.join(name))
        .filter(|launcher| launcher.is_file())
        .unwrap_or_else(|| exe.to_path_buf())
}

/// Open `.gaanim` files with `launcher` for the current user.
pub fn register(launcher: &Path) -> Result<Report, String> {
    let launcher = std::fs::canonicalize(launcher)
        .map_err(|error| format!("{}: {error}", launcher.display()))?;
    platform::register(&plain_path(launcher))
}

/// `path` without the `\\?\` prefix Windows canonical paths carry, which
/// shell commands and icon references do not accept; UNC paths keep it.
fn plain_path(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

/// Undo [`register`]. Associations another program owns are kept.
pub fn unregister() -> Result<Report, String> {
    platform::unregister()
}

/// Whether `.gaanim` files are associated with Gaanim for this user.
pub fn is_registered() -> bool {
    platform::is_registered()
}

// ---------------------------------------------------------------------------
// Linux (and other XDG desktops)
// ---------------------------------------------------------------------------

// The XDG files are written on Linux and tested everywhere.
#[cfg_attr(windows, allow(dead_code))]
/// Quote `arg` for the `Exec` key of a desktop entry or thumbnailer.
fn exec_arg(arg: &str) -> String {
    const RESERVED: &[char] = &[
        ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(',
        ')', '`',
    ];
    if !arg.is_empty() && !arg.contains(RESERVED) {
        return arg.replace('%', "%%");
    }
    let mut quoted = String::from("\"");
    for character in arg.chars() {
        match character {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(character);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg_attr(windows, allow(dead_code))]
fn mime_package() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="{MIME_TYPE}">
    <comment>Gaanim bundle</comment>
    <comment xml:lang="es">{TYPE_NAME}</comment>
    <sub-class-of type="application/zip"/>
    <icon name="application-x-gaanim"/>
    <generic-icon name="video-x-generic"/>
    <glob pattern="*.gaanim"/>
  </mime-type>
</mime-info>
"#
    )
}

#[cfg_attr(windows, allow(dead_code))]
fn desktop_entry(launcher: &Path) -> String {
    let exe = exec_arg(&launcher.to_string_lossy());
    format!(
        "[Desktop Entry]
Type=Application
Name=Gaanim
Comment=Play, present and export Gaanim bundles
Comment[es]=Reproduce, presenta y exporta paquetes de Gaanim
Exec={exe} %f
Icon=gaanim
Terminal=false
MimeType={MIME_TYPE};
Categories=Graphics;AudioVideo;Education;
Actions=present;

[Desktop Action present]
Name=Present
Name[es]=Presentar
Exec={exe} --present %f
"
    )
}

#[cfg_attr(windows, allow(dead_code))]
fn thumbnailer_entry(launcher: &Path) -> String {
    let exe = exec_arg(&launcher.to_string_lossy());
    format!(
        "[Thumbnailer Entry]
TryExec={exe}
Exec={exe} thumbnail %i %o %s
MimeType={MIME_TYPE};
"
    )
}

#[cfg_attr(windows, allow(dead_code))]
/// Files a registration writes, relative to the XDG data directory.
fn linux_files(launcher: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    const ICON: &[u8] = include_bytes!("../../../docs/assets/brand/gaanim-icon-512.png");
    vec![
        (
            PathBuf::from("mime/packages/gaanim.xml"),
            mime_package().into_bytes(),
        ),
        (
            PathBuf::from("applications/gaanim.desktop"),
            desktop_entry(launcher).into_bytes(),
        ),
        (
            PathBuf::from("thumbnailers/gaanim.thumbnailer"),
            thumbnailer_entry(launcher).into_bytes(),
        ),
        (
            PathBuf::from("icons/hicolor/512x512/apps/gaanim.png"),
            ICON.to_vec(),
        ),
        (
            PathBuf::from("icons/hicolor/512x512/mimetypes/application-x-gaanim.png"),
            ICON.to_vec(),
        ),
    ]
}

#[cfg_attr(windows, allow(dead_code))]
/// Write the association files under `data` (the XDG data directory).
fn install_linux_files(data: &Path, launcher: &Path) -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    for (relative, bytes) in linux_files(launcher) {
        let path = data.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|error| format!("{}: {error}", path.display()))?;
        done.push(format!("wrote {}", path.display()));
    }
    Ok(done)
}

#[cfg_attr(windows, allow(dead_code))]
/// Remove the association files under `data`; missing ones are skipped.
fn remove_linux_files(data: &Path) -> Vec<String> {
    let mut done = Vec::new();
    for (relative, _) in linux_files(Path::new("gaanim")) {
        let path = data.join(relative);
        if std::fs::remove_file(&path).is_ok() {
            done.push(format!("removed {}", path.display()));
        }
    }
    done
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    use std::process::Command;

    fn data_dir() -> Result<PathBuf, String> {
        directories::BaseDirs::new()
            .map(|dirs| dirs.data_dir().to_path_buf())
            .ok_or_else(|| "could not find the home directory".to_string())
    }

    /// Run a desktop tool; a missing or failing tool is a warning, since the
    /// files already work once the desktop rescans them.
    fn refresh(report: &mut Report, program: &str, args: &[&str]) {
        match Command::new(program).args(args).output() {
            Ok(output) if output.status.success() => {
                report.done.push(format!("ran {program}"));
            }
            Ok(output) => report.warnings.push(format!(
                "{program} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(_) => report.warnings.push(format!(
                "{program} is not installed; log out and back in to apply"
            )),
        }
    }

    fn refresh_desktop(report: &mut Report, data: &Path) {
        let mime = data.join("mime");
        let applications = data.join("applications");
        refresh(report, "update-mime-database", &[&mime.to_string_lossy()]);
        refresh(
            report,
            "update-desktop-database",
            &[&applications.to_string_lossy()],
        );
    }

    pub(super) fn register(launcher: &Path) -> Result<Report, String> {
        let data = data_dir()?;
        let mut report = Report {
            done: install_linux_files(&data, launcher)?,
            ..Default::default()
        };
        refresh_desktop(&mut report, &data);
        refresh(
            &mut report,
            "xdg-mime",
            &["default", "gaanim.desktop", MIME_TYPE],
        );
        Ok(report)
    }

    pub(super) fn unregister() -> Result<Report, String> {
        let data = data_dir()?;
        let mut report = Report {
            done: remove_linux_files(&data),
            ..Default::default()
        };
        refresh_desktop(&mut report, &data);
        Ok(report)
    }

    pub(super) fn is_registered() -> bool {
        data_dir().is_ok_and(|data| data.join("applications/gaanim.desktop").is_file())
    }
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

/// A registry value under `HKCU\Software\Classes`; `name` `None` is the
/// key's default value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
struct RegistryValue {
    key: String,
    name: Option<&'static str>,
    data: String,
}

#[cfg_attr(not(windows), allow(dead_code))]
fn value(
    key: impl Into<String>,
    name: Option<&'static str>,
    data: impl Into<String>,
) -> RegistryValue {
    RegistryValue {
        key: key.into(),
        name,
        data: data.into(),
    }
}

/// Registry values of the association; `handler` is the thumbnail handler
/// DLL, when installed.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_values(launcher: &Path, handler: Option<&Path>) -> Vec<RegistryValue> {
    let exe = launcher.display().to_string();
    let mut values = vec![
        value(".gaanim", None, PROG_ID),
        value(".gaanim", Some("Content Type"), MIME_TYPE),
        value(r".gaanim\OpenWithProgids", Some(PROG_ID), ""),
        value(PROG_ID, None, TYPE_NAME),
        value(
            format!(r"{PROG_ID}\DefaultIcon"),
            None,
            format!("\"{exe}\",0"),
        ),
        value(format!(r"{PROG_ID}\shell"), None, "open"),
        value(format!(r"{PROG_ID}\shell\open"), None, "Reproducir"),
        value(
            format!(r"{PROG_ID}\shell\open\command"),
            None,
            format!("\"{exe}\" \"%1\""),
        ),
        value(format!(r"{PROG_ID}\shell\present"), None, "Presentar"),
        value(
            format!(r"{PROG_ID}\shell\present\command"),
            None,
            format!("\"{exe}\" --present \"%1\""),
        ),
    ];
    if let Some(handler) = handler {
        let clsid = gaanim_thumbnail::WINDOWS_HANDLER_CLSID;
        values.extend([
            value(
                format!(r".gaanim\ShellEx\{THUMBNAIL_PROVIDER_SLOT}"),
                None,
                clsid,
            ),
            value(format!(r"CLSID\{clsid}"), None, "Gaanim thumbnail handler"),
            value(
                format!(r"CLSID\{clsid}\InprocServer32"),
                None,
                handler.display().to_string(),
            ),
            value(
                format!(r"CLSID\{clsid}\InprocServer32"),
                Some("ThreadingModel"),
                "Apartment",
            ),
        ]);
    }
    values
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW,
        RegSetValueExW,
    };
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};

    const CLASSES: &str = r"Software\Classes";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn classes(key: &str) -> Vec<u16> {
        wide(&format!(r"{CLASSES}\{key}"))
    }

    fn set(entry: &RegistryValue) -> Result<(), String> {
        let path = classes(&entry.key);
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: every pointer is a live, NUL-terminated buffer or a valid
        // out-pointer, and the opened key is closed below.
        unsafe {
            let status = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            );
            if status != ERROR_SUCCESS {
                return Err(format!(
                    "could not create HKCU\\{CLASSES}\\{} ({status})",
                    entry.key
                ));
            }
            let name = entry.name.map(wide);
            let data = wide(&entry.data);
            let status = RegSetValueExW(
                key,
                name.as_ref().map_or(std::ptr::null(), |name| name.as_ptr()),
                0,
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            );
            RegCloseKey(key);
            if status != ERROR_SUCCESS {
                return Err(format!(
                    "could not write HKCU\\{CLASSES}\\{} ({status})",
                    entry.key
                ));
            }
        }
        Ok(())
    }

    fn get(key: &str, name: Option<&str>) -> Option<String> {
        let path = classes(key);
        let name = name.map(wide);
        let mut buffer = vec![0u16; 1024];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: the buffer holds `size` bytes and the strings are NUL-terminated.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                name.as_ref().map_or(std::ptr::null(), |name| name.as_ptr()),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then(|| {
            let length = (size as usize / 2).saturating_sub(1);
            String::from_utf16_lossy(&buffer[..length.min(buffer.len())])
        })
    }

    fn delete_tree(key: &str) -> bool {
        let path = classes(key);
        // SAFETY: `path` is NUL-terminated.
        let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, path.as_ptr()) };
        status == ERROR_SUCCESS
    }

    fn delete_value(key: &str, name: &str) -> bool {
        let path = classes(key);
        let name = wide(name);
        // SAFETY: both strings are NUL-terminated.
        unsafe {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, path.as_ptr(), name.as_ptr()) == ERROR_SUCCESS
        }
    }

    fn notify() {
        // SAFETY: SHCNE_ASSOCCHANGED takes no items.
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
    }

    pub(super) fn register(launcher: &Path) -> Result<Report, String> {
        let mut report = Report::default();
        let handler = launcher
            .parent()
            .map(|dir| dir.join(gaanim_thumbnail::WINDOWS_HANDLER_DLL))
            .filter(|dll| dll.is_file());
        if handler.is_none() {
            report.warnings.push(format!(
                "{} is not next to gaanim.exe; files open with Gaanim but show no cover image",
                gaanim_thumbnail::WINDOWS_HANDLER_DLL
            ));
        }
        for entry in windows_values(launcher, handler.as_deref()) {
            set(&entry)?;
        }
        report.done.push(format!(
            "associated .gaanim with {} (HKCU\\{CLASSES}\\{PROG_ID})",
            launcher.display()
        ));
        if handler.is_some() {
            report.done.push("registered the thumbnail handler".into());
        }
        notify();
        Ok(report)
    }

    pub(super) fn unregister() -> Result<Report, String> {
        let mut report = Report::default();
        let clsid = gaanim_thumbnail::WINDOWS_HANDLER_CLSID;
        if get(".gaanim", None).as_deref() == Some(PROG_ID) {
            if delete_tree(".gaanim") {
                report
                    .done
                    .push("removed HKCU\\Software\\Classes\\.gaanim".into());
            }
        } else {
            // Another program owns the extension: only take Gaanim out of it.
            delete_value(r".gaanim\OpenWithProgids", PROG_ID);
            let slot = format!(r".gaanim\ShellEx\{THUMBNAIL_PROVIDER_SLOT}");
            if get(&slot, None).as_deref() == Some(clsid) {
                delete_tree(&slot);
            }
        }
        for key in [PROG_ID.to_string(), format!(r"CLSID\{clsid}")] {
            if delete_tree(&key) {
                report.done.push(format!("removed HKCU\\{CLASSES}\\{key}"));
            }
        }
        notify();
        Ok(report)
    }

    pub(super) fn is_registered() -> bool {
        get(&format!(r"{PROG_ID}\shell\open\command"), None).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_files_quote_the_launcher_and_offer_present() {
        let launcher = Path::new("/home/ana/My Tools/gaanim");
        let desktop = desktop_entry(launcher);
        assert!(
            desktop.contains("Exec=\"/home/ana/My Tools/gaanim\" %f\n"),
            "{desktop}"
        );
        assert!(desktop.contains("Exec=\"/home/ana/My Tools/gaanim\" --present %f\n"));
        assert!(desktop.contains("MimeType=application/x-gaanim;"));
        assert!(desktop.contains("Actions=present;"));
        let thumbnailer = thumbnailer_entry(Path::new("/opt/gaanim/gaanim"));
        assert!(thumbnailer.contains("Exec=/opt/gaanim/gaanim thumbnail %i %o %s\n"));
        assert!(thumbnailer.contains("MimeType=application/x-gaanim;"));
        assert_eq!(exec_arg("/a/100%/g$x"), "\"/a/100%%/g\\$x\"");
        assert_eq!(exec_arg("/plain/path"), "/plain/path");
    }

    #[test]
    fn verbatim_windows_paths_lose_their_prefix() {
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\C:\Tools\gaanim.exe")),
            PathBuf::from(r"C:\Tools\gaanim.exe")
        );
        let unc = PathBuf::from(r"\\?\UNC\server\share\gaanim.exe");
        assert_eq!(plain_path(unc.clone()), unc);
        assert_eq!(
            plain_path(PathBuf::from("/opt/gaanim")),
            PathBuf::from("/opt/gaanim")
        );
    }

    #[test]
    fn the_mime_type_wins_over_zip_detection() {
        let xml = mime_package();
        assert!(xml.contains(r#"<mime-type type="application/x-gaanim">"#));
        assert!(xml.contains(r#"<sub-class-of type="application/zip"/>"#));
        assert!(xml.contains(r#"<glob pattern="*.gaanim"/>"#));
    }

    #[test]
    fn linux_files_install_and_uninstall_under_the_data_directory() {
        let data = tempfile::tempdir().unwrap();
        let launcher = Path::new("/opt/gaanim/gaanim");
        let done = install_linux_files(data.path(), launcher).unwrap();
        assert_eq!(done.len(), 5);
        for relative in [
            "mime/packages/gaanim.xml",
            "applications/gaanim.desktop",
            "thumbnailers/gaanim.thumbnailer",
            "icons/hicolor/512x512/apps/gaanim.png",
            "icons/hicolor/512x512/mimetypes/application-x-gaanim.png",
        ] {
            assert!(data.path().join(relative).is_file(), "{relative}");
        }
        let icon =
            std::fs::read(data.path().join("icons/hicolor/512x512/apps/gaanim.png")).unwrap();
        assert_eq!(&icon[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(remove_linux_files(data.path()).len(), 5);
        assert!(!data.path().join("applications/gaanim.desktop").exists());
        assert!(remove_linux_files(data.path()).is_empty());
    }

    #[test]
    fn windows_values_open_present_and_add_the_handler_when_installed() {
        let launcher = Path::new(r"C:\Tools\gaanim\gaanim.exe");
        let values = windows_values(launcher, None);
        let find = |key: &str, name: Option<&str>| {
            values
                .iter()
                .find(|value| value.key == key && value.name == name)
                .map(|value| value.data.clone())
        };
        assert_eq!(find(".gaanim", None).as_deref(), Some(PROG_ID));
        assert_eq!(
            find(r"Gaanim.Bundle\shell\open\command", None).as_deref(),
            Some(r#""C:\Tools\gaanim\gaanim.exe" "%1""#)
        );
        assert_eq!(
            find(r"Gaanim.Bundle\shell\present\command", None).as_deref(),
            Some(r#""C:\Tools\gaanim\gaanim.exe" --present "%1""#)
        );
        assert_eq!(
            find(r".gaanim\OpenWithProgids", Some(PROG_ID)).as_deref(),
            Some("")
        );
        assert!(values.iter().all(|value| !value.key.starts_with("CLSID")));

        let dll = Path::new(r"C:\Tools\gaanim\gaanim_thumbnail_handler.dll");
        let values = windows_values(launcher, Some(dll));
        let clsid = gaanim_thumbnail::WINDOWS_HANDLER_CLSID;
        assert!(values.iter().any(|value| value.key
            == format!(r".gaanim\ShellEx\{THUMBNAIL_PROVIDER_SLOT}")
            && value.data == clsid));
        assert!(values.iter().any(
            |value| value.key == format!(r"CLSID\{clsid}\InprocServer32")
                && value.name == Some("ThreadingModel")
                && value.data == "Apartment"
        ));
    }
}
