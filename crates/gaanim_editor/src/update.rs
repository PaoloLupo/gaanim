//! New versions of Gaanim: find the newest release with a package for this
//! system, and install it over the running installation.
//!
//! The Home checks at most once a day ([`check`]) and offers the update;
//! `gaanim update` installs it from the command line. An installation is the
//! folder of the release package (`gaanim`, its libraries, the authoring
//! wheel), so an update downloads the package, checks its SHA-256, unpacks it
//! next to the installation, makes sure the new `gaanim` starts, and then
//! swaps the files. Windows does not let a running program or a loaded
//! library be overwritten, but it lets them be renamed, so the old files move
//! aside into [`STAGING`] and are deleted the next time Gaanim starts
//! ([`clean_previous`]). A project's authoring wheel follows on its own: the
//! next time it opens, Gaanim installs the wheel the new version ships.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use semver::Version;
use serde::{Deserialize, Serialize};

/// The releases GitHub lists, newest first.
const RELEASES_API: &str = "https://api.github.com/repos/PaoloLupo/gaanim/releases?per_page=10";
/// How long a check is reused before GitHub is asked again.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Set to anything but `0` to turn the Home's check off (classrooms without
/// network, CI); `gaanim update` still checks when asked.
pub const DISABLE_VARIABLE: &str = "GAANIM_NO_UPDATE_CHECK";
/// Folder inside the installation where an update unpacks and where the
/// files it replaced wait to be deleted.
pub const STAGING: &str = ".gaanim-update";

/// A release with this system's package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    /// Its page on GitHub, with the release notes.
    pub page: String,
    pub package: Asset,
    /// The package's `.sha256` file.
    pub checksum: Asset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

impl Release {
    fn version(&self) -> Option<Version> {
        Version::parse(&self.version).ok()
    }

    /// Whether this release is newer than the running Gaanim.
    pub fn is_newer(&self) -> bool {
        let running = Version::parse(env!("CARGO_PKG_VERSION")).ok();
        matches!((self.version(), running), (Some(release), Some(running)) if release > running)
    }
}

/// End of the package name for this system, or `None` where Gaanim has none.
pub fn package_suffix() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("-windows-x64.zip"),
        ("linux", "x86_64") => Some("-linux-x64.tar.gz"),
        _ => None,
    }
}

/// The newest published release in `releases` (GitHub's list, newest first)
/// with a package ending in `suffix` and its checksum. Drafts and
/// prereleases are skipped, as `releases/latest` skips them.
pub fn newest_release(releases: &serde_json::Value, suffix: &str) -> Option<Release> {
    releases.as_array()?.iter().find_map(|release| {
        if release["draft"].as_bool() == Some(true) || release["prerelease"].as_bool() == Some(true)
        {
            return None;
        }
        let version = release["tag_name"].as_str()?.trim_start_matches('v');
        Version::parse(version).ok()?;
        let assets = release["assets"].as_array()?;
        let asset = |wanted: &dyn Fn(&str) -> bool| {
            assets.iter().find_map(|asset| {
                let name = asset["name"].as_str().filter(|name| wanted(name))?;
                Some(Asset {
                    name: name.to_owned(),
                    url: asset["browser_download_url"].as_str()?.to_owned(),
                    size: asset["size"].as_u64().unwrap_or(0),
                })
            })
        };
        let package = asset(&|name| name.ends_with(suffix))?;
        let checksum_name = format!("{}.sha256", package.name);
        let checksum = asset(&|name| name == checksum_name)?;
        Some(Release {
            version: version.to_owned(),
            page: release["html_url"].as_str().unwrap_or_default().to_owned(),
            package,
            checksum,
        })
    })
}

/// Whether the Home's daily check is turned off with [`DISABLE_VARIABLE`].
pub fn check_disabled() -> bool {
    std::env::var_os(DISABLE_VARIABLE).is_some_and(|value| !value.is_empty() && value != "0")
}

/// What the last check found, saved in Gaanim's data folder.
#[derive(Debug, Serialize, Deserialize)]
struct SavedCheck {
    /// Seconds since the Unix epoch.
    checked_at: u64,
    release: Option<Release>,
}

fn saved_check_path() -> Option<PathBuf> {
    gaanim_project::user_data_dir().map(|dir| dir.join("update-check.json"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The newest release with a package for this system when it is newer than
/// the running Gaanim. Reuses the answer of a check made less than a day
/// ago unless `fresh`; a failed request is not saved, so the next check
/// tries again.
pub fn check(fresh: bool) -> Result<Option<Release>, String> {
    let Some(suffix) = package_suffix() else {
        return Ok(None);
    };
    let path = saved_check_path();
    if !fresh
        && let Some(saved) = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<SavedCheck>(&bytes).ok())
        && now().saturating_sub(saved.checked_at) < CHECK_INTERVAL.as_secs()
    {
        return Ok(saved.release.filter(Release::is_newer));
    }
    let releases: serde_json::Value = agent()?
        .get(RELEASES_API)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("could not ask GitHub for releases: {}", describe(error)))?
        .into_json()
        .map_err(|error| format!("GitHub answered something unexpected: {error}"))?;
    let release = newest_release(&releases, suffix);
    if let Some(path) = path {
        let saved = SavedCheck {
            checked_at: now(),
            release: release.clone(),
        };
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&saved) {
            let _ = std::fs::write(path, bytes);
        }
    }
    Ok(release.filter(Release::is_newer))
}

fn agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new()
        .map_err(|error| format!("could not set up TLS: {error}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(tls))
        .user_agent(concat!("gaanim/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build())
}

fn describe(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(403 | 429, _) => {
            "GitHub limits how often it is asked; try again in an hour".to_owned()
        }
        ureq::Error::Status(code, _) => format!("GitHub answered {code}"),
        ureq::Error::Transport(transport) => transport.to_string(),
    }
}

/// The folder of the running installation, or why it cannot be updated: a
/// build from source lives in Cargo's `target` folder and updates with git.
pub fn install_dir() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the Gaanim executable: {error}"))?;
    let folder = executable
        .parent()
        .ok_or("the Gaanim executable has no folder")?
        .to_path_buf();
    if is_cargo_build(&folder) {
        return Err(format!(
            "this Gaanim was built from source ({}); update it with git pull and cargo",
            folder.display()
        ));
    }
    Ok(folder)
}

/// Whether `folder` is a Cargo profile folder such as `target/debug`, which
/// Cargo marks with its lock file and fingerprints.
fn is_cargo_build(folder: &Path) -> bool {
    folder.join(".cargo-lock").exists() || folder.join(".fingerprint").is_dir()
}

/// What an update is doing, for a progress line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Bytes downloaded of the package's size.
    Downloading {
        done: u64,
        total: u64,
    },
    Verifying,
    Unpacking,
    Installing,
}

/// Install `release` over the running installation and return the path of
/// the new `gaanim`. Nothing changes until the package is downloaded, its
/// checksum matches and the new `gaanim` starts; a failure while the files
/// are swapped puts the old ones back.
pub fn install(release: &Release, mut progress: impl FnMut(Step)) -> Result<PathBuf, String> {
    let install = install_dir()?;
    let staging = install.join(STAGING);
    // A previous update's leftovers: the files it replaced, if Gaanim has
    // not started since, or a download it did not finish.
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .map_err(|error| format!("could not write to {}: {error}", install.display()))?;
    let package = fetch_verified(release, &staging, &mut progress)?;

    progress(Step::Unpacking);
    let unpacked = staging.join("new");
    unpack(&package, &unpacked)?;
    let _ = std::fs::remove_file(&package);
    let executable = executable_name();
    let reported = Command::new(unpacked.join(executable))
        .arg("--version")
        .output()
        .map_err(|error| format!("the new Gaanim does not start: {error}"))?;
    let reported = String::from_utf8_lossy(&reported.stdout);
    if reported.trim() != format!("gaanim {}", release.version) {
        return Err(format!(
            "the new Gaanim reports {:?} instead of gaanim {}",
            reported.trim(),
            release.version
        ));
    }

    progress(Step::Installing);
    swap(&install, &unpacked, &staging.join("old"))?;
    let _ = std::fs::remove_dir_all(&unpacked);
    Ok(install.join(executable))
}

/// Delete what a finished update left in `install`: the files it replaced,
/// which a running Gaanim kept until it closed. An update another Gaanim is
/// still downloading stays. Errors are ignored; the next start tries again.
pub fn clean_previous(install: &Path) {
    let staging = install.join(STAGING);
    let old = staging.join("old");
    if old.is_dir() {
        let _ = std::fs::remove_dir_all(old);
        // Removed only when empty: nothing else is in progress.
        let _ = std::fs::remove_dir(staging);
    }
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "gaanim.exe"
    } else {
        "gaanim"
    }
}

/// Download `release`'s package into `folder` and check it against the
/// release's SHA-256; a damaged download is deleted.
fn fetch_verified(
    release: &Release,
    folder: &Path,
    progress: &mut impl FnMut(Step),
) -> Result<PathBuf, String> {
    let package = folder.join(&release.package.name);
    download(&release.package, &package, progress)?;
    progress(Step::Verifying);
    let expected = agent()?
        .get(&release.checksum.url)
        .call()
        .map_err(|error| format!("could not download the checksum: {}", describe(error)))?
        .into_string()
        .map_err(|error| format!("could not read the checksum: {error}"))?;
    let expected = parse_checksum(&expected).ok_or("the release's checksum file is malformed")?;
    let actual = sha256_file(&package)?;
    if actual != expected {
        let _ = std::fs::remove_file(&package);
        return Err(format!(
            "the download is damaged: its SHA-256 is {actual}, the release says {expected}"
        ));
    }
    Ok(package)
}

fn download(asset: &Asset, path: &Path, progress: &mut impl FnMut(Step)) -> Result<(), String> {
    let response = agent()?
        .get(&asset.url)
        .call()
        .map_err(|error| format!("could not download {}: {}", asset.name, describe(error)))?;
    let total = response
        .header("Content-Length")
        .and_then(|length| length.parse().ok())
        .unwrap_or(asset.size);
    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(path)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    let mut buffer = vec![0; 1 << 16];
    let mut done = 0;
    progress(Step::Downloading { done, total });
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("the download of {} stopped: {error}", asset.name))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
        done += read as u64;
        progress(Step::Downloading { done, total });
    }
    file.flush()
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    if total > 0 && done != total {
        return Err(format!(
            "the download of {} stopped at {done} of {total} bytes",
            asset.name
        ));
    }
    Ok(())
}

/// The hash in a `.sha256` file: `<hex>  <file name>`, as `sha256sum` and
/// the release workflow write it.
fn parse_checksum(text: &str) -> Option<String> {
    let hash = text
        .trim_start_matches('\u{feff}')
        .split_whitespace()
        .next()?
        .to_ascii_lowercase();
    (hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(hash)
}

/// SHA-256 of a file, in lowercase hex, from the system's own tool:
/// PowerShell's `Get-FileHash` on Windows, `sha256sum` elsewhere.
fn sha256_file(path: &Path) -> Result<String, String> {
    let output = if cfg!(windows) {
        // The path goes through the environment, which needs no quoting.
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-FileHash -Algorithm SHA256 -LiteralPath $env:GAANIM_HASHED_FILE).Hash",
            ])
            .env("GAANIM_HASHED_FILE", path)
            .output()
    } else {
        Command::new("sha256sum").arg("--").arg(path).output()
    }
    .map_err(|error| format!("could not compute the download's SHA-256: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "could not compute the SHA-256 of {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_checksum(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| format!("could not compute the SHA-256 of {}", path.display()))
}

/// Unpack a release package into `folder`: a zip on Windows, a gzipped tar
/// elsewhere. Entries that would land outside `folder` are refused.
fn unpack(package: &Path, folder: &Path) -> Result<(), String> {
    std::fs::create_dir_all(folder)
        .map_err(|error| format!("could not create {}: {error}", folder.display()))?;
    let file = std::fs::File::open(package)
        .map_err(|error| format!("could not open {}: {error}", package.display()))?;
    let name = package.to_string_lossy();
    if name.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|error| format!("the package is not a valid zip: {error}"))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| format!("the package is damaged: {error}"))?;
            let relative = entry
                .enclosed_name()
                .ok_or_else(|| format!("the package holds an unsafe path: {}", entry.name()))?;
            let target = folder.join(relative);
            if entry.is_dir() {
                std::fs::create_dir_all(&target)
                    .map_err(|error| format!("could not create {}: {error}", target.display()))?;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
            }
            let mut out = std::fs::File::create(&target)
                .map_err(|error| format!("could not write {}: {error}", target.display()))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|error| format!("could not unpack {}: {error}", target.display()))?;
        }
        Ok(())
    } else if name.ends_with(".tar.gz") {
        unpack_tar_gz(file, folder)
    } else {
        Err(format!("unknown package format: {}", package.display()))
    }
}

#[cfg(target_os = "linux")]
fn unpack_tar_gz(file: std::fs::File, folder: &Path) -> Result<(), String> {
    // `unpack` keeps every entry inside `folder` and restores permissions.
    tar::Archive::new(flate2::read::GzDecoder::new(file))
        .unpack(folder)
        .map_err(|error| format!("could not unpack the package: {error}"))
}

#[cfg(not(target_os = "linux"))]
fn unpack_tar_gz(_file: std::fs::File, _folder: &Path) -> Result<(), String> {
    Err("tar.gz packages are only unpacked on Linux".to_owned())
}

/// Whether `name` belongs to one version of Gaanim only: Rust's standard
/// library (named after the toolchain that built it) and the authoring
/// wheel. A new version brings its own, so the old ones leave.
fn versioned_file(name: &str) -> bool {
    let library = |extension: &str| {
        (name.starts_with("std-") || name.starts_with("libstd-")) && name.ends_with(extension)
    };
    library(".dll")
        || library(".so")
        || (name.starts_with("gaanim-") && name.ends_with("-py3-none-any.whl"))
}

/// Put every entry of `new` in `install`, moving what it replaces, and the
/// files of the old version that the new one does not ship, into `old`.
/// Renaming works on a running program and its loaded libraries, which
/// cannot be overwritten. If a step fails, the moves made so far are undone.
fn swap(install: &Path, new: &Path, old: &Path) -> Result<(), String> {
    std::fs::create_dir_all(old)
        .map_err(|error| format!("could not create {}: {error}", old.display()))?;
    let incoming: Vec<std::ffi::OsString> = std::fs::read_dir(new)
        .map_err(|error| format!("could not read {}: {error}", new.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
        .collect();
    let leaving: Vec<std::ffi::OsString> = std::fs::read_dir(install)
        .map_err(|error| format!("could not read {}: {error}", install.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
        .filter(|name| !incoming.contains(name))
        .filter(|name| name.to_str().is_some_and(versioned_file))
        .collect();
    // Undo log: (from, to) of every rename made, to reverse on failure.
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    let rename = |from: PathBuf, to: PathBuf, done: &mut Vec<(PathBuf, PathBuf)>| {
        std::fs::rename(&from, &to)
            .map(|()| done.push((from.clone(), to.clone())))
            .map_err(|error| {
                format!(
                    "could not move {} to {}: {error}",
                    from.display(),
                    to.display()
                )
            })
    };
    let mut result = Ok(());
    for name in &leaving {
        result = rename(install.join(name), old.join(name), &mut done);
        if result.is_err() {
            break;
        }
    }
    if result.is_ok() {
        for name in &incoming {
            let target = install.join(name);
            if target.exists() {
                result = rename(target.clone(), old.join(name), &mut done);
                if result.is_err() {
                    break;
                }
            }
            result = rename(new.join(name), target, &mut done);
            if result.is_err() {
                break;
            }
        }
    }
    if result.is_err() {
        for (from, to) in done.into_iter().rev() {
            let _ = std::fs::rename(&to, &from);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, assets: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/PaoloLupo/gaanim/releases/tag/{tag}"),
            "assets": assets.iter().map(|name| serde_json::json!({
                "name": name,
                "browser_download_url": format!("https://example.invalid/{name}"),
                "size": 10,
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn the_newest_release_with_a_package_and_its_checksum_wins() {
        let zip = "-windows-x64.zip";
        let mut draft = release("v0.13.0", &["gaanim-v0.13.0-windows-x64.zip"]);
        draft["draft"] = true.into();
        let mut prerelease = release("v0.13.0-rc.1", &["gaanim-v0.13.0-rc.1-windows-x64.zip"]);
        prerelease["prerelease"] = true.into();
        let releases = serde_json::json!([
            draft,
            prerelease,
            // Still uploading: its package without its checksum.
            release("v0.12.1", &["gaanim-v0.12.1-windows-x64.zip"]),
            release("v0.12.0", &["gaanim-v0.12.0-linux-x64.tar.gz"]),
            release(
                "v0.11.1",
                &[
                    "gaanim-v0.11.1-windows-x64.zip",
                    "gaanim-v0.11.1-windows-x64.zip.sha256",
                ],
            ),
        ]);
        let found = newest_release(&releases, zip).unwrap();
        assert_eq!(found.version, "0.11.1");
        assert_eq!(found.package.name, "gaanim-v0.11.1-windows-x64.zip");
        assert_eq!(found.checksum.name, "gaanim-v0.11.1-windows-x64.zip.sha256");
        assert!(found.page.ends_with("/v0.11.1"));
        assert_eq!(newest_release(&serde_json::json!([]), zip), None);
        assert_eq!(
            newest_release(&serde_json::json!({"message": "rate limited"}), zip),
            None
        );
    }

    #[test]
    fn only_a_higher_version_is_newer() {
        let at = |version: &str| Release {
            version: version.to_owned(),
            page: String::new(),
            package: Asset {
                name: String::new(),
                url: String::new(),
                size: 0,
            },
            checksum: Asset {
                name: String::new(),
                url: String::new(),
                size: 0,
            },
        };
        let running = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        let next = Version::new(running.major, running.minor, running.patch + 1);
        assert!(at(&next.to_string()).is_newer());
        assert!(!at(env!("CARGO_PKG_VERSION")).is_newer());
        assert!(!at("0.0.1").is_newer());
        assert!(!at("not a version").is_newer());
    }

    #[test]
    fn checksum_files_give_their_hash() {
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(
            parse_checksum(&format!("{hash}  gaanim-v0.11.0-linux-x64.tar.gz\n")).as_deref(),
            Some(hash)
        );
        // PowerShell's Out-File can start the file with a byte order mark,
        // and Get-FileHash prints uppercase.
        assert_eq!(
            parse_checksum(&format!("\u{feff}{}  x.zip\r\n", hash.to_uppercase())).as_deref(),
            Some(hash)
        );
        assert_eq!(parse_checksum("not a hash"), None);
        assert_eq!(parse_checksum(""), None);
    }

    #[test]
    fn the_system_tool_computes_sha256() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("abc.txt");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(
            sha256_file(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn zip_packages_unpack_and_refuse_paths_outside_their_folder() {
        let folder = tempfile::tempdir().unwrap();
        let package = folder.path().join("gaanim-v9.9.9-windows-x64.zip");
        {
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&package).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("gaanim.exe", options).unwrap();
            writer.write_all(b"new program").unwrap();
            writer.start_file("docs/README.txt", options).unwrap();
            writer.write_all(b"read me").unwrap();
            writer.finish().unwrap();
        }
        let unpacked = folder.path().join("new");
        unpack(&package, &unpacked).unwrap();
        assert_eq!(
            std::fs::read(unpacked.join("gaanim.exe")).unwrap(),
            b"new program"
        );
        assert_eq!(
            std::fs::read(unpacked.join("docs/README.txt")).unwrap(),
            b"read me"
        );

        let unsafe_package = folder.path().join("unsafe.zip");
        {
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&unsafe_package).unwrap());
            writer
                .start_file("../escaped.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"outside").unwrap();
            writer.finish().unwrap();
        }
        assert!(unpack(&unsafe_package, &folder.path().join("unsafe")).is_err());
        assert!(!folder.path().join("escaped.txt").exists());
    }

    #[test]
    fn swapping_replaces_files_and_sets_the_old_version_aside() {
        let folder = tempfile::tempdir().unwrap();
        let (install, new, old) = (
            folder.path().join("install"),
            folder.path().join("new"),
            folder.path().join("old"),
        );
        for dir in [&install, &new] {
            std::fs::create_dir_all(dir).unwrap();
        }
        for (name, text) in [
            ("gaanim.exe", "old program"),
            ("std-1111.dll", "old std"),
            ("gaanim-0.11.0-py3-none-any.whl", "old wheel"),
            ("notes.txt", "the user's own file"),
        ] {
            std::fs::write(install.join(name), text).unwrap();
        }
        for (name, text) in [
            ("gaanim.exe", "new program"),
            ("std-2222.dll", "new std"),
            ("gaanim-0.11.1-py3-none-any.whl", "new wheel"),
        ] {
            std::fs::write(new.join(name), text).unwrap();
        }
        swap(&install, &new, &old).unwrap();

        let read = |path: PathBuf| std::fs::read_to_string(path).unwrap();
        assert_eq!(read(install.join("gaanim.exe")), "new program");
        assert_eq!(read(install.join("std-2222.dll")), "new std");
        assert_eq!(
            read(install.join("gaanim-0.11.1-py3-none-any.whl")),
            "new wheel"
        );
        assert_eq!(read(install.join("notes.txt")), "the user's own file");
        assert!(!install.join("std-1111.dll").exists());
        assert!(!install.join("gaanim-0.11.0-py3-none-any.whl").exists());
        assert_eq!(read(old.join("gaanim.exe")), "old program");
        assert_eq!(read(old.join("std-1111.dll")), "old std");
    }

    #[test]
    fn a_failed_swap_puts_the_old_files_back() {
        let folder = tempfile::tempdir().unwrap();
        let (install, new, old) = (
            folder.path().join("install"),
            folder.path().join("new"),
            folder.path().join("old"),
        );
        for dir in [&install, &new] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(install.join("a.dll"), "old a").unwrap();
        std::fs::write(install.join("b.dll"), "old b").unwrap();
        std::fs::write(new.join("a.dll"), "new a").unwrap();
        std::fs::write(new.join("b.dll"), "new b").unwrap();
        // A folder in the way of `old/b.dll` makes its move fail.
        std::fs::create_dir_all(old.join("b.dll").join("blocked")).unwrap();
        assert!(swap(&install, &new, &old).is_err());
        let read = |path: PathBuf| std::fs::read_to_string(path).unwrap();
        assert_eq!(read(install.join("a.dll")), "old a");
        assert_eq!(read(install.join("b.dll")), "old b");
        assert_eq!(read(new.join("a.dll")), "new a");
    }

    /// Serve `files` (request path and body) over plain HTTP on this
    /// computer, one request each, and return the server's address.
    fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
        use std::io::BufRead;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().take(files.len()) {
                let mut stream = stream.unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap() == 0 || header == "\r\n" {
                        break;
                    }
                }
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                match files.iter().find(|(name, _)| *name == path) {
                    Some((_, body)) => {
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .unwrap();
                        stream.write_all(body).unwrap();
                    }
                    None => write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap(),
                }
            }
        });
        address
    }

    #[test]
    fn a_download_stays_only_when_its_checksum_matches() {
        let folder = tempfile::tempdir().unwrap();
        let body = b"the package's bytes".to_vec();
        let reference = folder.path().join("reference");
        std::fs::write(&reference, &body).unwrap();
        let hash = sha256_file(&reference).unwrap();
        for (checksum, matches) in [(hash, true), ("0".repeat(64), false)] {
            let server = serve(vec![
                ("/package.zip", body.clone()),
                (
                    "/package.zip.sha256",
                    format!("{checksum}  package.zip\n").into_bytes(),
                ),
            ]);
            let release = Release {
                version: "9.9.9".to_owned(),
                page: String::new(),
                package: Asset {
                    name: "package.zip".to_owned(),
                    url: format!("{server}/package.zip"),
                    size: body.len() as u64,
                },
                checksum: Asset {
                    name: "package.zip.sha256".to_owned(),
                    url: format!("{server}/package.zip.sha256"),
                    size: 0,
                },
            };
            let staging = folder
                .path()
                .join(if matches { "matching" } else { "damaged" });
            std::fs::create_dir_all(&staging).unwrap();
            let mut steps = Vec::new();
            let result = fetch_verified(&release, &staging, &mut |step| steps.push(step));
            let total = body.len() as u64;
            assert!(steps.contains(&Step::Downloading { done: total, total }));
            assert!(steps.contains(&Step::Verifying));
            if matches {
                assert_eq!(std::fs::read(result.unwrap()).unwrap(), body);
            } else {
                assert!(result.unwrap_err().contains("damaged"));
                assert!(!staging.join("package.zip").exists());
            }
        }
    }

    /// Windows does not let a running program be overwritten, but lets it be
    /// renamed, which is how an update replaces the running `gaanim`.
    #[cfg(windows)]
    #[test]
    fn a_running_program_is_swapped_out() {
        let folder = tempfile::tempdir().unwrap();
        let (install, new, old) = (
            folder.path().join("install"),
            folder.path().join("new"),
            folder.path().join("old"),
        );
        for dir in [&install, &new] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let ping = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32")
            .join("PING.EXE");
        let program = install.join("gaanim.exe");
        std::fs::copy(ping, &program).unwrap();
        std::fs::write(new.join("gaanim.exe"), "new program").unwrap();
        let mut running = Command::new(&program)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let overwritten = std::fs::write(&program, "overwritten");
        let swapped = swap(&install, &new, &old);
        let _ = running.kill();
        let _ = running.wait();
        assert!(overwritten.is_err(), "a running program was overwritten");
        swapped.unwrap();
        assert_eq!(std::fs::read_to_string(&program).unwrap(), "new program");
        assert!(old.join("gaanim.exe").is_file());
    }

    #[test]
    fn builds_from_source_are_not_updated_in_place() {
        let folder = tempfile::tempdir().unwrap();
        assert!(!is_cargo_build(folder.path()));
        std::fs::write(folder.path().join(".cargo-lock"), "").unwrap();
        assert!(is_cargo_build(folder.path()));
    }

    #[test]
    fn only_per_version_files_leave_with_the_old_version() {
        assert!(versioned_file("std-0123abcd.dll"));
        assert!(versioned_file("libstd-0123abcd.so"));
        assert!(versioned_file("gaanim-0.11.0-py3-none-any.whl"));
        assert!(!versioned_file("gaanim.exe"));
        assert!(!versioned_file("gaanim_engine.dll"));
        assert!(!versioned_file("README.txt"));
        assert!(!versioned_file("mystd-1.dll"));
    }
}
