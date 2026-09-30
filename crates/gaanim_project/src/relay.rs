//! The relay that carries audience votes to a presentation.
//!
//! Every user deploys their own relay, a Cloudflare Worker written by
//! `gaanim relay init`, and tells Gaanim its address with `gaanim relay use`
//! or a project's `[polls] relay`.

use std::path::{Path, PathBuf};

/// Environment variable that overrides every configured relay.
pub const RELAY_ENV: &str = "GAANIM_POLL_RELAY";

/// Files of the relay template, relative to the directory it is written to.
const TEMPLATE: [(&str, &str); 4] = [
    ("wrangler.toml", include_str!("../relay/wrangler.toml")),
    ("src/index.js", include_str!("../relay/src/index.js")),
    ("README.md", include_str!("../relay/README.md")),
    (".gitignore", ".wrangler/\nnode_modules/\n"),
];

/// Write the relay template into `directory`, refusing to replace an
/// existing file unless `force`. Returns the written files.
pub fn write_template(directory: &Path, force: bool) -> Result<Vec<PathBuf>, String> {
    let files: Vec<(PathBuf, &str)> = TEMPLATE
        .iter()
        .map(|(path, source)| (directory.join(path), *source))
        .collect();
    if !force && let Some((path, _)) = files.iter().find(|(path, _)| path.exists()) {
        return Err(format!(
            "{} already exists (use --force to replace the relay files)",
            path.display()
        ));
    }
    for (path, source) in &files {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        std::fs::write(path, source)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    }
    Ok(files.into_iter().map(|(path, _)| path).collect())
}

/// Check a relay address and drop its trailing slash.
pub fn normalize(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| format!("the relay address must start with https:// (got {url:?})"))?;
    if rest.is_empty() || rest.contains(char::is_whitespace) {
        return Err(format!("{url:?} is not a relay address"));
    }
    Ok(url.to_string())
}

fn saved_path() -> Option<PathBuf> {
    crate::user_data_dir().map(|dir| dir.join("poll-relay.txt"))
}

/// The relay saved with `gaanim relay use`.
pub fn saved() -> Option<String> {
    let source = std::fs::read_to_string(saved_path()?).ok()?;
    normalize(&source).ok()
}

/// Save `url` as this user's relay; `None` forgets it.
pub fn save(url: Option<&str>) -> Result<Option<String>, String> {
    let path = saved_path().ok_or("could not find the user data directory")?;
    let Some(url) = url else {
        match std::fs::remove_file(&path) {
            Ok(()) => return Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("could not remove {}: {error}", path.display())),
        }
    };
    let url = normalize(url)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    std::fs::write(&path, format!("{url}\n"))
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(Some(url))
}

/// Where a relay address came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelaySource {
    Environment,
    Project,
    User,
}

/// The relay a presentation uses: `GAANIM_POLL_RELAY`, then the project's
/// `[polls] relay`, then the one saved with `gaanim relay use`.
pub fn resolve(project: Option<&str>) -> Option<(String, RelaySource)> {
    resolve_from(std::env::var(RELAY_ENV).ok().as_deref(), project, saved)
}

fn resolve_from(
    environment: Option<&str>,
    project: Option<&str>,
    saved: impl FnOnce() -> Option<String>,
) -> Option<(String, RelaySource)> {
    let valid = |url: &str| normalize(url).ok();
    environment
        .and_then(valid)
        .map(|url| (url, RelaySource::Environment))
        .or_else(|| {
            project
                .and_then(valid)
                .map(|url| (url, RelaySource::Project))
        })
        .or_else(|| saved().map(|url| (url, RelaySource::User)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_addresses_need_a_web_scheme() {
        assert_eq!(
            normalize(" https://relay.example.workers.dev/ ").unwrap(),
            "https://relay.example.workers.dev"
        );
        assert_eq!(
            normalize("http://localhost:8787").unwrap(),
            "http://localhost:8787"
        );
        assert!(normalize("relay.example.workers.dev").is_err());
        assert!(normalize("https://").is_err());
    }

    #[test]
    fn the_environment_overrides_the_project_and_the_user() {
        let user = || Some("https://user.dev".to_string());
        assert_eq!(
            resolve_from(Some("https://env.dev"), Some("https://project.dev"), user),
            Some(("https://env.dev".into(), RelaySource::Environment))
        );
        assert_eq!(
            resolve_from(None, Some("https://project.dev/"), user),
            Some(("https://project.dev".into(), RelaySource::Project))
        );
        assert_eq!(
            resolve_from(Some("not a url"), None, user),
            Some(("https://user.dev".into(), RelaySource::User))
        );
        assert_eq!(resolve_from(None, None, || None), None);
    }

    #[test]
    fn the_template_is_written_once() {
        let dir = tempfile::tempdir().unwrap();
        let written = write_template(dir.path(), false).unwrap();
        assert!(written.iter().all(|path| path.is_file()));
        let wrangler = std::fs::read_to_string(dir.path().join("wrangler.toml")).unwrap();
        assert!(wrangler.contains("PollSession"));
        assert!(write_template(dir.path(), false).is_err());
        assert!(write_template(dir.path(), true).is_ok());
    }
}
