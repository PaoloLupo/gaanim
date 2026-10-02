//! The relay that carries audience votes to a presentation.
//!
//! Every user deploys their own relay, a Cloudflare Worker written by
//! `gaanim relay init`, and tells Gaanim its address with `gaanim relay use`
//! or a project's `[polls] relay`.
//!
//! A project votes under one session: a six-character code, fixed so the QR
//! code a scene draws is ordinary content (the same in previews, exports and
//! bundles), and a secret key that only this computer holds. Both are kept in
//! the user data directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Environment variable that overrides every configured relay.
pub const RELAY_ENV: &str = "GAANIM_POLL_RELAY";
/// Environment variable that fixes the session code, e.g. for tests or for
/// a project several people present.
pub const SESSION_ENV: &str = "GAANIM_POLL_SESSION";
/// Session code characters: no 0/O or 1/I to confuse. 32 of them, so a
/// random byte maps to one without bias. The relay accepts the same set.
pub const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const CODE_LENGTH: usize = 6;
/// The relay protocol this Gaanim speaks: `API_VERSION` in the template's
/// `src/index.js`, which a relay reports on `/health`.
pub const API_VERSION: u64 = 11;

/// Files of the relay template, relative to the directory it is written to.
const TEMPLATE: [(&str, &[u8]); 20] = [
    ("wrangler.toml", include_bytes!("../relay/wrangler.toml")),
    ("package.json", include_bytes!("../relay/package.json")),
    ("README.md", include_bytes!("../relay/README.md")),
    ("src/index.js", include_bytes!("../relay/src/index.js")),
    (
        "public/_headers",
        include_bytes!("../relay/public/_headers"),
    ),
    ("public/app.css", include_bytes!("../relay/public/app.css")),
    (
        "public/avatar-parts.json",
        include_bytes!("../relay/public/avatar-parts.json"),
    ),
    (
        "public/avatar.js",
        include_bytes!("../relay/public/avatar.js"),
    ),
    (
        "public/apple-touch-icon.png",
        include_bytes!("../relay/public/apple-touch-icon.png"),
    ),
    ("public/i18n.js", include_bytes!("../relay/public/i18n.js")),
    (
        "public/icon.svg",
        include_bytes!("../relay/public/icon.svg"),
    ),
    (
        "public/index.html",
        include_bytes!("../relay/public/index.html"),
    ),
    ("public/join.js", include_bytes!("../relay/public/join.js")),
    (
        "public/manifest.webmanifest",
        include_bytes!("../relay/public/manifest.webmanifest"),
    ),
    (
        "public/symbol-dark.svg",
        include_bytes!("../relay/public/symbol-dark.svg"),
    ),
    (
        "public/symbol-light.svg",
        include_bytes!("../relay/public/symbol-light.svg"),
    ),
    (
        "public/vote.html",
        include_bytes!("../relay/public/vote.html"),
    ),
    ("public/vote.js", include_bytes!("../relay/public/vote.js")),
    (
        "test/relay.test.js",
        include_bytes!("../relay/test/relay.test.js"),
    ),
    (".gitignore", b"node_modules/\n.wrangler/\n.dev.vars\n"),
];

/// Write the relay template into `directory`, refusing to replace an
/// existing file unless `force`. Returns the written files.
pub fn write_template(directory: &Path, force: bool) -> Result<Vec<PathBuf>, String> {
    let files: Vec<(PathBuf, &[u8])> = TEMPLATE
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

/// What to tell the user about a relay whose `/health` reports `version`,
/// or `None` when it speaks this Gaanim's protocol.
pub fn version_advice(version: u64) -> Option<String> {
    use std::cmp::Ordering;
    match version.cmp(&API_VERSION) {
        Ordering::Equal => None,
        Ordering::Less => Some(format!(
            "the relay is version {version} and this Gaanim needs {API_VERSION}: update it with              `gaanim relay init --force <its folder>` and deploy it again (`npx wrangler deploy`)"
        )),
        Ordering::Greater => Some(format!(
            "the relay is version {version}, newer than this Gaanim's {API_VERSION}:              update Gaanim if polls misbehave"
        )),
    }
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

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// A project's session on the relay: the code phones use and the key that
/// lets only this computer open questions and read votes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollSession {
    pub code: String,
    pub key: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SessionStore {
    #[serde(default)]
    sessions: Vec<StoredSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSession {
    /// The project folder, or the script, the session belongs to; empty for
    /// a session only known by its code (a bundle recorded elsewhere).
    #[serde(default)]
    scope: String,
    code: String,
    key: String,
}

/// Whether `code` is a valid session code.
pub fn is_code(code: &str) -> bool {
    code.len() == CODE_LENGTH && code.bytes().all(|byte| CODE_ALPHABET.contains(&byte))
}

#[cfg(not(target_arch = "wasm32"))]
fn random_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("could not create a poll session: {error}"))?;
    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
fn random_bytes<const N: usize>() -> Result<[u8; N], String> {
    Err("poll sessions are not available in the web player".to_string())
}

fn random_code() -> Result<String, String> {
    Ok(random_bytes::<CODE_LENGTH>()?
        .iter()
        .map(|byte| char::from(CODE_ALPHABET[usize::from(*byte) % CODE_ALPHABET.len()]))
        .collect())
}

fn random_key() -> Result<String, String> {
    Ok(random_bytes::<32>()?
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn sessions_path() -> Result<PathBuf, String> {
    crate::user_data_dir()
        .map(|dir| dir.join("poll-sessions.json"))
        .ok_or_else(|| "could not find the user data directory".to_string())
}

fn scope_name(scope: &Path) -> String {
    scope
        .canonicalize()
        .unwrap_or_else(|_| scope.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// Where the polls of a script in `directory` belong: the nearest folder
/// with a `gaanim.toml` (searched upward, as `git` finds `.git`), else
/// `directory` itself; and that project's `[polls] relay`.
pub fn scope_of(directory: &Path) -> (PathBuf, Option<String>) {
    let project = directory
        .ancestors()
        .find(|folder| folder.join("gaanim.toml").is_file())
        .map(Path::to_path_buf);
    let relay = project
        .as_deref()
        .and_then(|root| crate::resolve_project(root).ok())
        .and_then(|project| project.manifest.poll_relay);
    (project.unwrap_or_else(|| directory.to_path_buf()), relay)
}

/// The session of the project folder (or script) `scope`, created on first
/// use. `GAANIM_POLL_SESSION` fixes its code.
pub fn session_for(scope: &Path) -> Result<PollSession, String> {
    let fixed = std::env::var(SESSION_ENV).ok();
    session_in(&sessions_path()?, &scope_name(scope), fixed.as_deref())
}

/// The key for session `code`, creating one when this computer has none,
/// as when presenting a bundle recorded on another computer.
pub fn key_for_code(code: &str) -> Result<String, String> {
    Ok(session_in(&sessions_path()?, "", Some(code))?.key)
}

fn session_in(file: &Path, scope: &str, fixed_code: Option<&str>) -> Result<PollSession, String> {
    if let Some(code) = fixed_code
        && !is_code(code)
    {
        return Err(format!(
            "{code:?} is not a poll session code: 6 characters from {}",
            String::from_utf8_lossy(CODE_ALPHABET)
        ));
    }
    let mut store: SessionStore = match std::fs::read_to_string(file) {
        Ok(source) => serde_json::from_str(&source).unwrap_or_default(),
        Err(_) => SessionStore::default(),
    };
    let found = match fixed_code {
        Some(code) => store.sessions.iter().find(|session| session.code == code),
        None => store.sessions.iter().find(|session| session.scope == scope),
    };
    if let Some(session) = found {
        return Ok(PollSession {
            code: session.code.clone(),
            key: session.key.clone(),
        });
    }
    let session = StoredSession {
        scope: scope.to_string(),
        code: match fixed_code {
            Some(code) => code.to_string(),
            None => random_code()?,
        },
        key: random_key()?,
    };
    store.sessions.push(session.clone());
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&store).map_err(|error| error.to_string())?;
    std::fs::write(file, json)
        .map_err(|error| format!("could not write {}: {error}", file.display()))?;
    Ok(PollSession {
        code: session.code,
        key: session.key,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_template_speaks_this_gaanims_protocol() {
        let source = std::str::from_utf8(include_bytes!("../relay/src/index.js")).unwrap();
        let declared = format!("const API_VERSION = {};", super::API_VERSION);
        assert!(
            source.contains(&declared),
            "the relay must declare `{declared}`"
        );
        assert_eq!(super::version_advice(super::API_VERSION), None);
        assert!(
            super::version_advice(super::API_VERSION - 1)
                .unwrap()
                .contains("--force")
        );
    }

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
        assert!(dir.path().join("public/vote.html").is_file());
        assert!(write_template(dir.path(), false).is_err());
        assert!(write_template(dir.path(), true).is_ok());
    }

    #[test]
    fn a_project_keeps_its_session_and_projects_get_different_ones() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("poll-sessions.json");
        let first = session_in(&file, "C:/talks/calculus", None).unwrap();
        assert!(is_code(&first.code));
        assert_eq!(first.key.len(), 64);
        assert_eq!(session_in(&file, "C:/talks/calculus", None).unwrap(), first);
        let other = session_in(&file, "C:/talks/physics", None).unwrap();
        assert_ne!(other.key, first.key);
    }

    #[test]
    fn a_fixed_code_reuses_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("poll-sessions.json");
        let project = session_in(&file, "C:/talks/calculus", None).unwrap();
        // Presenting that project's bundle finds the same key by its code.
        assert_eq!(session_in(&file, "", Some(&project.code)).unwrap(), project);
        let fixed = session_in(&file, "C:/talks/shared", Some("TEST23")).unwrap();
        assert_eq!(fixed.code, "TEST23");
        assert_eq!(session_in(&file, "", Some("TEST23")).unwrap(), fixed);
        assert!(session_in(&file, "", Some("BAD0O1")).is_err());
    }
}
