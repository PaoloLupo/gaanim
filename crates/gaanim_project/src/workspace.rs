//! Workspaces: several projects in one repository that share Python code,
//! assets and a Python environment, described by a `[workspace]` table in
//! the `gaanim.toml` at the repository's root.
//!
//! No folder name means anything: `members` says where the projects are,
//! with `*` for any part of a folder name, and `[workspace.shared]` names
//! the shared folders by their role.

use crate::{ResolvedProject, read_manifest, resolve_project};
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RawWorkspace {
    members: Vec<RawMember>,
    #[serde(default)]
    exclude: Vec<String>,
    shared: Option<RawShared>,
    cover: Option<PathBuf>,
}

/// Images at a workspace's root Home takes as its cover without `cover`.
const COVER_FILES: [&str; 4] = ["cover.png", "cover.jpg", "cover.jpeg", "cover.webp"];

/// An entry of `members`: a path, or a table that also names its group.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawMember {
    Path(String),
    Table { path: String, group: Option<String> },
}

#[derive(Debug, Default, Deserialize)]
struct RawShared {
    python: Option<Vec<PathBuf>>,
    assets: Option<Vec<PathBuf>>,
}

/// One entry of `members`: a path relative to the workspace, in `/` form,
/// where `*` stands for any part of a folder name, and the group the
/// members it finds are listed under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberPattern {
    pub path: String,
    pub group: Option<String>,
}

/// A `[workspace]`: where its members are and what they share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub name: String,
    pub members: Vec<MemberPattern>,
    /// Paths and patterns `members` must not match, in `/` form.
    pub exclude: Vec<String>,
    /// Folders on every member's `sys.path`, after the member's own.
    pub python_roots: Vec<PathBuf>,
    /// Folders a member's relative asset paths are looked up in when the
    /// member's own folders lack them.
    pub asset_roots: Vec<PathBuf>,
    /// The image Home shows for the workspace: `cover`, else a `cover.png`
    /// (or `.jpg`, `.webp`) at its root. Without one, Home composes its
    /// members' covers.
    pub cover: Option<PathBuf>,
}

/// A project of a workspace, as [`Workspace::members`] finds it.
#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    /// Its folder relative to the workspace root, in `/` form.
    pub path: String,
    pub root: PathBuf,
    /// The group of the `members` entry that found it, or else its parent
    /// folder relative to the workspace (empty at the root).
    pub group: String,
    /// The project, or why its manifest is invalid.
    pub project: Result<ResolvedProject, String>,
}

impl WorkspaceMember {
    /// The name to show: the project's, or its folder's when its manifest
    /// is invalid.
    pub fn name(&self) -> &str {
        match &self.project {
            Ok(project) => &project.manifest.name,
            Err(_) => self.path.rsplit('/').next().unwrap_or(&self.path),
        }
    }
}

impl Workspace {
    pub(crate) fn from_raw(
        root: &Path,
        manifest_path: &Path,
        name: Option<&str>,
        raw: RawWorkspace,
    ) -> Result<Self, String> {
        let invalid = |message: String| {
            format!(
                "invalid workspace in {}: {message}",
                manifest_path.display()
            )
        };
        let members = raw
            .members
            .into_iter()
            .map(|member| {
                let (path, group) = match member {
                    RawMember::Path(path) => (path, None),
                    RawMember::Table { path, group } => (path, group),
                };
                Ok(MemberPattern {
                    path: normalize_pattern(&path).map_err(|error| format!("members: {error}"))?,
                    group: group
                        .map(|group| group.trim().to_owned())
                        .filter(|group| !group.is_empty()),
                })
            })
            .collect::<Result<Vec<_>, String>>()
            .map_err(invalid)?;
        let exclude = raw
            .exclude
            .iter()
            .map(|pattern| normalize_pattern(pattern).map_err(|error| format!("exclude: {error}")))
            .collect::<Result<Vec<_>, String>>()
            .map_err(invalid)?;
        let shared = raw.shared.unwrap_or_default();
        let python_roots =
            shared_folders(root, shared.python, "src", "shared.python").map_err(invalid)?;
        let asset_roots =
            shared_folders(root, shared.assets, "assets", "shared.assets").map_err(invalid)?;
        let cover = match raw.cover {
            Some(cover) => Some(cover_file(root, &cover).map_err(invalid)?),
            None => COVER_FILES
                .iter()
                .map(|name| root.join(name))
                .find(|path| path.is_file()),
        };
        let name = name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                root.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "workspace".to_owned());
        Ok(Self {
            root: root.to_path_buf(),
            manifest_path: manifest_path.to_path_buf(),
            name,
            members,
            exclude,
            python_roots,
            asset_roots,
            cover,
        })
    }

    /// The entry of `members` that makes the folder at `relative` (from
    /// the workspace root, in `/` form) a member, unless `exclude` removes
    /// it.
    pub fn member_pattern(&self, relative: &str) -> Option<&MemberPattern> {
        if self
            .exclude
            .iter()
            .any(|pattern| pattern_matches(pattern, relative))
        {
            return None;
        }
        self.members
            .iter()
            .find(|member| pattern_matches(&member.path, relative))
    }

    /// `folder` relative to the workspace root, in `/` form, when it lies
    /// inside it.
    pub fn relative_path(&self, folder: &Path) -> Option<String> {
        relative_to(folder, &self.root)
    }

    /// Whether the folder at `folder` is a member: inside the workspace
    /// and matched by `members`.
    pub fn has_member(&self, folder: &Path) -> bool {
        self.relative_path(folder)
            .is_some_and(|relative| self.member_pattern(&relative).is_some())
    }

    /// The group Home lists the member at `relative` under: its entry's
    /// `group`, or else its parent folder (empty at the root).
    pub fn group_of(&self, relative: &str) -> String {
        self.member_pattern(relative)
            .and_then(|member| member.group.clone())
            .unwrap_or_else(|| {
                relative
                    .rsplit_once('/')
                    .map(|(parent, _)| parent.to_owned())
                    .unwrap_or_default()
            })
    }

    /// Every member, in the order of `members` and in natural order within
    /// an entry (`ep2` before `ep10`). A folder an entry matches is a
    /// member only if it holds a `gaanim.toml`; a folder two entries match
    /// goes with the first.
    pub fn members(&self) -> Vec<WorkspaceMember> {
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        for pattern in &self.members {
            for (root, path) in expand(&self.root, &pattern.path) {
                if !root.join("gaanim.toml").is_file()
                    || self
                        .exclude
                        .iter()
                        .any(|excluded| pattern_matches(excluded, &path))
                    || !seen.insert(comparable(&path))
                {
                    continue;
                }
                let group = pattern.group.clone().unwrap_or_else(|| {
                    path.rsplit_once('/')
                        .map(|(parent, _)| parent.to_owned())
                        .unwrap_or_default()
                });
                found.push(WorkspaceMember {
                    project: resolve_project(&root),
                    root,
                    path,
                    group,
                });
            }
        }
        found
    }

    /// The members `patterns` names, as comma-separated paths or patterns
    /// written like `members` (`season-1/*,trailer`), or every member
    /// without patterns. A pattern that names no member is an error, since
    /// two members can share a folder name only their path tells apart.
    pub fn select_members(&self, patterns: Option<&str>) -> Result<Vec<WorkspaceMember>, String> {
        let members = self.members();
        let Some(patterns) = patterns else {
            return Ok(members);
        };
        let patterns = patterns
            .split(',')
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .map(normalize_pattern)
            .collect::<Result<Vec<_>, String>>()?;
        if patterns.is_empty() {
            return Err("no member paths or patterns given".to_owned());
        }
        if let Some(unmatched) = patterns.iter().find(|pattern| {
            !members
                .iter()
                .any(|member| pattern_matches(pattern, &member.path))
        }) {
            let paths: Vec<&str> = members.iter().map(|member| member.path.as_str()).collect();
            return Err(format!(
                "`{unmatched}` names no member of workspace {}; its members are {}",
                self.name,
                if paths.is_empty() {
                    "none".to_owned()
                } else {
                    paths.join(", ")
                }
            ));
        }
        Ok(members
            .into_iter()
            .filter(|member| {
                patterns
                    .iter()
                    .any(|pattern| pattern_matches(pattern, &member.path))
            })
            .collect())
    }

    /// The `members` line that would make `folder` a member, for a hint
    /// when it is not one.
    pub fn member_line(&self, folder: &Path) -> Option<String> {
        self.relative_path(folder)
            .map(|relative| format!("members = [..., \"{relative}\"]"))
    }
}

/// The folders `[workspace.shared]` lists under `key`, absolute. Without the
/// key, `default` if that folder exists: the name a lone project uses too.
fn shared_folders(
    root: &Path,
    listed: Option<Vec<PathBuf>>,
    default: &str,
    key: &str,
) -> Result<Vec<PathBuf>, String> {
    let Some(folders) = listed else {
        let folder = root.join(default);
        return Ok(folder.is_dir().then_some(folder).into_iter().collect());
    };
    folders
        .into_iter()
        .map(|folder| {
            let inside = !folder.as_os_str().is_empty()
                && folder
                    .components()
                    .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
            if !inside {
                return Err(format!(
                    "{key}: {:?} must be a folder inside the workspace",
                    folder.to_string_lossy()
                ));
            }
            let path = root.join(&folder);
            if !path.is_dir() {
                return Err(format!(
                    "{key}: folder {:?} does not exist",
                    folder.to_string_lossy()
                ));
            }
            Ok(path)
        })
        .collect()
}

/// The image `cover` names, which must be a file inside the workspace.
fn cover_file(root: &Path, cover: &Path) -> Result<PathBuf, String> {
    let inside = !cover.as_os_str().is_empty()
        && cover
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
    if !inside {
        return Err(format!(
            "cover: {:?} must be an image inside the workspace",
            cover.to_string_lossy()
        ));
    }
    let path = root.join(cover);
    if !path.is_file() {
        return Err(format!(
            "cover: file {:?} does not exist",
            cover.to_string_lossy()
        ));
    }
    Ok(path)
}

/// `pattern` in `/` form without a leading `./` or trailing `/`, or why it
/// cannot name folders inside the workspace.
fn normalize_pattern(pattern: &str) -> Result<String, String> {
    let normalized = pattern.trim().replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');
    let normalized = normalized.strip_prefix("./").unwrap_or(normalized);
    let segments: Vec<&str> = normalized.split('/').collect();
    let outside = normalized.starts_with('/')
        || segments.first().is_some_and(|first| first.contains(':'))
        || segments
            .iter()
            .any(|segment| matches!(*segment, "" | "." | ".."));
    if outside {
        return Err(format!(
            "{pattern:?} must be a path inside the workspace, such as \"episodes/*\""
        ));
    }
    if segments.iter().any(|segment| segment.contains("**")) {
        return Err(format!(
            "{pattern:?}: `**` is not supported; write one `*` per folder level"
        ));
    }
    Ok(normalized.to_owned())
}

/// Whether `relative` (in `/` form) matches `pattern` folder by folder.
fn pattern_matches(pattern: &str, relative: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let relative: Vec<&str> = relative.split('/').collect();
    pattern.len() == relative.len()
        && pattern
            .iter()
            .zip(&relative)
            .all(|(pattern, name)| wildcard_matches(pattern, name))
}

/// Whether the folder name `name` matches `pattern`, where `*` stands for
/// any run of characters. Case is ignored on Windows, as its paths do.
fn wildcard_matches(pattern: &str, name: &str) -> bool {
    let (pattern, name) = (comparable(pattern), comparable(name));
    let (pattern, name): (Vec<char>, Vec<char>) =
        (pattern.chars().collect(), name.chars().collect());
    // The last `*` seen and the name position it was tried at.
    let (mut p, mut n, mut star) = (0, 0, None);
    while n < name.len() {
        if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, n));
            p += 1;
        } else if p < pattern.len() && pattern[p] == name[n] {
            p += 1;
            n += 1;
        } else if let Some((star_p, star_n)) = star {
            p = star_p + 1;
            n = star_n + 1;
            star = Some((star_p, star_n + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|character| *character == '*')
}

/// `text` as paths compare on this system: lowercase on Windows.
fn comparable(text: &str) -> String {
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.to_owned()
    }
}

/// Folders [`expand`] never looks into for a `*`.
fn is_skipped_folder(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "venv" | "env" | "__pycache__" | "node_modules" | "target"
        )
}

/// The folders under `root` that `pattern` names, with their paths relative
/// to `root`, in natural order.
fn expand(root: &Path, pattern: &str) -> Vec<(PathBuf, String)> {
    let mut folders = vec![(root.to_path_buf(), String::new())];
    for segment in pattern.split('/') {
        let mut next = Vec::new();
        for (folder, relative) in folders {
            let child = |name: &str| {
                let relative = if relative.is_empty() {
                    name.to_owned()
                } else {
                    format!("{relative}/{name}")
                };
                (folder.join(name), relative)
            };
            if !segment.contains('*') {
                let (path, relative) = child(segment);
                if path.is_dir() {
                    next.push((path, relative));
                }
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
                .filter(|name| !is_skipped_folder(name) && wildcard_matches(segment, name))
                .collect();
            names.sort_by(|a, b| natural_cmp(a, b));
            next.extend(names.iter().map(|name| child(name)));
        }
        folders = next;
    }
    folders
}

/// `path` relative to `root` in `/` form, when it lies strictly inside it.
/// Either may be canonical (`\\?\` on Windows) or not, and `path` need not
/// exist yet.
pub(crate) fn relative_to(path: &Path, root: &Path) -> Option<String> {
    let (path, root) = (comparable_parts(path), comparable_parts(root));
    let inside = path.len() > root.len()
        && root
            .iter()
            .zip(&path)
            .all(|(root, path)| comparable(root) == comparable(path));
    inside.then(|| path[root.len()..].join("/"))
}

/// Whether `path` is `folder` or lies inside it, whichever form either is
/// written in (canonical or plain, any case on Windows).
pub fn is_within(path: &Path, folder: &Path) -> bool {
    let (path, folder) = (comparable_parts(path), comparable_parts(folder));
    path.len() >= folder.len()
        && folder
            .iter()
            .zip(&path)
            .all(|(folder, path)| comparable(folder) == comparable(path))
}

/// The names along `path` once resolved: canonical up to its deepest
/// existing folder, without the `\\?\` prefix.
fn comparable_parts(path: &Path) -> Vec<String> {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let resolved = absolute
        .ancestors()
        .find_map(|ancestor| {
            let canonical = ancestor.canonicalize().ok()?;
            Some(canonical.join(absolute.strip_prefix(ancestor).ok()?))
        })
        .unwrap_or(absolute);
    let text = resolved.to_string_lossy().replace('\\', "/");
    let text = match text.strip_prefix("//?/UNC/") {
        Some(rest) => format!("//{rest}"),
        None => text.strip_prefix("//?/").map(str::to_owned).unwrap_or(text),
    };
    text.trim_end_matches('/')
        .split('/')
        .filter(|part| *part != ".")
        .map(str::to_owned)
        .collect()
}

/// Names in natural order: digit runs compare as numbers (`ep2` before
/// `ep10`), and letters ignore case.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    let digits = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
        let mut run = String::new();
        while let Some(digit) = chars.next_if(char::is_ascii_digit) {
            run.push(digit);
        }
        run
    };
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (x, y) = (digits(&mut a), digits(&mut b));
                let (x_value, y_value) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = x_value
                    .len()
                    .cmp(&y_value.len())
                    .then_with(|| x_value.cmp(y_value))
                    .then_with(|| x.len().cmp(&y.len()));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// The workspace above the project at `project_root` that has it as a
/// member: the first `gaanim.toml` with a `[workspace]` whose `members`
/// include it, looking up to the folder holding `.git` or the root of the
/// file system.
pub(crate) fn find_workspace_for(project_root: &Path) -> Result<Option<Workspace>, String> {
    for folder in project_root.ancestors().skip(1) {
        let manifest = folder.join("gaanim.toml");
        if manifest.is_file() {
            let (_, raw) = read_manifest(&manifest)?;
            if let Some(workspace) = raw.workspace {
                let workspace =
                    Workspace::from_raw(folder, &manifest, raw.name.as_deref(), workspace)?;
                if workspace.has_member(project_root) {
                    return Ok(Some(workspace));
                }
            }
        }
        if folder.join(".git").exists() {
            break;
        }
    }
    Ok(None)
}

/// The nearest workspace above `folder`, whether or not it lists `folder`
/// as a member: where a new project there would land.
pub fn workspace_around(folder: &Path) -> Option<Workspace> {
    for folder in folder.ancestors() {
        let manifest = folder.join("gaanim.toml");
        if manifest.is_file()
            && let Ok((_, raw)) = read_manifest(&manifest)
            && let Some(workspace) = raw.workspace
        {
            let root = folder
                .canonicalize()
                .unwrap_or_else(|_| folder.to_path_buf());
            return Workspace::from_raw(
                &root,
                &root.join("gaanim.toml"),
                raw.name.as_deref(),
                workspace,
            )
            .ok();
        }
        if folder.join(".git").exists() {
            break;
        }
    }
    None
}

/// The workspace whose `gaanim.toml` is in the folder `path`.
pub fn resolve_workspace(path: &Path) -> Result<Workspace, String> {
    if !path.is_dir() {
        return Err(format!(
            "expected a workspace directory: {}",
            path.display()
        ));
    }
    let root = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let manifest_path = root.join("gaanim.toml");
    let (_, raw) = read_manifest(&manifest_path)?;
    let workspace = raw
        .workspace
        .ok_or_else(|| format!("{} has no [workspace] table", manifest_path.display()))?;
    Workspace::from_raw(&root, &manifest_path, raw.name.as_deref(), workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn member(root: &Path, relative: &str, kind: &str) {
        write(&root.join(relative).join("main.py"), "");
        write(
            &root.join(relative).join("gaanim.toml"),
            &format!("kind = \"{kind}\"\nentry = \"main.py\"\n"),
        );
    }

    #[test]
    fn patterns_match_folder_by_folder() {
        assert!(pattern_matches("season-1/*", "season-1/ep1"));
        assert!(!pattern_matches("season-1/*", "season-1/ep1/extra"));
        assert!(!pattern_matches("season-1/*", "season-2/ep1"));
        assert!(pattern_matches("events/*/talk", "events/2026/talk"));
        assert!(pattern_matches("ep-*", "ep-10"));
        assert!(pattern_matches("*-draft", "intro-draft"));
        assert!(pattern_matches("a*b*c", "aXbYc"));
        assert!(!pattern_matches("a*b*c", "aXbY"));
        assert!(pattern_matches("trailer", "trailer"));
        assert_eq!(pattern_matches("Trailer", "trailer"), cfg!(windows));
    }

    #[test]
    fn patterns_must_stay_inside_the_workspace() {
        assert_eq!(normalize_pattern("./season-1/*/").unwrap(), "season-1/*");
        assert_eq!(normalize_pattern("season-1\\*").unwrap(), "season-1/*");
        for bad in [
            "",
            ".",
            "../other",
            "a/../b",
            "/abs",
            "C:/videos",
            "a//b",
            "**/x",
        ] {
            assert!(normalize_pattern(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn names_sort_naturally() {
        let mut names = vec!["ep10", "ep2", "Ep1", "intro", "ep02"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["Ep1", "ep2", "ep02", "ep10", "intro"]);
    }

    #[test]
    fn members_follow_the_manifest_at_any_depth() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(
            &root.join("gaanim.toml"),
            r#"name = "canal"

[workspace]
members = [
  { path = "season-1/*", group = "Season 1" },
  "events/*/talk",
  "trailer",
]
exclude = ["season-1/draft"]

[workspace.shared]
python = ["common"]
assets = ["media", "brand/logos"]
"#,
        );
        for folder in ["common", "media", "brand/logos"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        member(root, "season-1/ep10", "video");
        member(root, "season-1/ep2", "video");
        member(root, "season-1/draft", "video");
        // A folder without a manifest is not a member.
        std::fs::create_dir_all(root.join("season-1/scripts")).unwrap();
        member(root, "events/2026/talk", "slides");
        member(root, "trailer", "video");
        // A project the workspace does not list stays on its own.
        member(root, "sandbox", "video");

        let workspace = resolve_workspace(root).unwrap();
        assert_eq!(workspace.name, "canal");
        let root = workspace.root.clone();
        assert_eq!(workspace.python_roots, [root.join("common")]);
        assert_eq!(
            workspace.asset_roots,
            [root.join("media"), root.join("brand/logos")]
        );
        let members = workspace.members();
        let listed: Vec<(&str, &str)> = members
            .iter()
            .map(|member| (member.path.as_str(), member.group.as_str()))
            .collect();
        assert_eq!(
            listed,
            [
                ("season-1/ep2", "Season 1"),
                ("season-1/ep10", "Season 1"),
                ("events/2026/talk", "events/2026"),
                ("trailer", ""),
            ]
        );
        assert!(members.iter().all(|member| member.project.is_ok()));
        assert_eq!(members[3].name(), "trailer");

        let project = resolve_project(&root.join("season-1/ep2")).unwrap();
        assert_eq!(project.workspace.as_ref().map(|ws| &ws.root), Some(&root));
        assert!(
            resolve_project(&root.join("sandbox"))
                .unwrap()
                .workspace
                .is_none()
        );
        assert!(
            resolve_project(&root.join("season-1/draft"))
                .unwrap()
                .workspace
                .is_none()
        );
        assert_eq!(workspace.group_of("events/2026/talk"), "events/2026");
        assert_eq!(workspace.group_of("season-1/ep2"), "Season 1");

        let paths = |selected: Vec<WorkspaceMember>| -> Vec<String> {
            selected.into_iter().map(|member| member.path).collect()
        };
        assert_eq!(
            paths(
                workspace
                    .select_members(Some("season-1/*, trailer"))
                    .unwrap()
            ),
            ["season-1/ep2", "season-1/ep10", "trailer"]
        );
        assert_eq!(paths(workspace.select_members(None).unwrap()).len(), 4);
        let error = workspace.select_members(Some("season-3/*")).unwrap_err();
        assert!(error.contains("names no member"), "{error}");
        assert!(error.contains("season-1/ep2"), "{error}");
        assert!(workspace.select_members(Some("../x")).is_err());
    }

    #[test]
    fn shared_folders_default_to_src_and_assets_when_present() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(
            &root.join("gaanim.toml"),
            "[workspace]\nmembers = [\"*\"]\n",
        );
        let workspace = resolve_workspace(root).unwrap();
        assert!(workspace.python_roots.is_empty());
        assert!(workspace.asset_roots.is_empty());
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let workspace = resolve_workspace(root).unwrap();
        assert_eq!(workspace.python_roots, [workspace.root.join("src")]);
        assert_eq!(workspace.asset_roots, [workspace.root.join("assets")]);
    }

    #[test]
    fn a_shared_folder_must_exist_inside_the_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (shared, message) in [
            ("python = [\"lib\"]", "does not exist"),
            ("assets = [\"../outside\"]", "inside the workspace"),
        ] {
            write(
                &root.join("gaanim.toml"),
                &format!("[workspace]\nmembers = [\"*\"]\n\n[workspace.shared]\n{shared}\n"),
            );
            let error = resolve_workspace(root).unwrap_err();
            assert!(error.contains(message), "{error}");
            assert!(error.contains("gaanim.toml"), "{error}");
        }
    }

    #[test]
    fn the_cover_is_the_chosen_image_or_one_at_the_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(
            &root.join("gaanim.toml"),
            "[workspace]\nmembers = [\"*\"]\n",
        );
        assert_eq!(resolve_workspace(root).unwrap().cover, None);
        write(&root.join("cover.jpg"), "");
        let workspace = resolve_workspace(root).unwrap();
        assert_eq!(workspace.cover, Some(workspace.root.join("cover.jpg")));
        write(&root.join("brand/channel.png"), "");
        write(
            &root.join("gaanim.toml"),
            "[workspace]\nmembers = [\"*\"]\ncover = \"brand/channel.png\"\n",
        );
        let workspace = resolve_workspace(root).unwrap();
        assert_eq!(
            workspace.cover,
            Some(workspace.root.join("brand/channel.png"))
        );
        write(
            &root.join("gaanim.toml"),
            "[workspace]\nmembers = [\"*\"]\ncover = \"missing.png\"\n",
        );
        let error = resolve_workspace(root).unwrap_err();
        assert!(error.contains("cover"), "{error}");
    }

    #[test]
    fn a_new_folder_is_relative_to_the_workspace_before_it_exists() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(
            &root.join("gaanim.toml"),
            "[workspace]\nmembers = [\"shows/*\"]\n",
        );
        let workspace = workspace_around(&root.join("shows/new")).unwrap();
        assert_eq!(
            workspace.relative_path(&root.join("shows/new")).as_deref(),
            Some("shows/new")
        );
        assert!(workspace.has_member(&root.join("shows/new")));
        assert!(!workspace.has_member(&root.join("other/new")));
        assert_eq!(
            workspace.member_line(&root.join("other/new")).as_deref(),
            Some("members = [..., \"other/new\"]")
        );
    }
}
