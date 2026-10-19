//! Shared project and environment support for the Gaanim launcher and editor.

use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

pub mod association;
pub mod describe;
pub mod help;
pub mod relay;
pub mod settings;
pub mod workspace;

pub use workspace::{
    MemberPattern, Workspace, WorkspaceMember, is_within, natural_cmp, resolve_workspace,
    workspace_around,
};

const VIDEO_PROJECT_TEMPLATE: &str = include_str!("../../../templates/video_project.py");
const SLIDES_PROJECT_TEMPLATE: &str = include_str!("../../../templates/slides_project.py");
const PROJECT_AGENTS: &str = include_str!("../../../templates/project_agents.md");
const WORKSPACE_AGENTS: &str = include_str!("../../../templates/workspace_agents.md");
/// What a workspace's repository leaves out: the environment and what any
/// of its projects renders, wherever its `output_dir` puts it. Media
/// extensions are not ignored, so images and videos in `assets/` stay.
const WORKSPACE_GITIGNORE: &str = r#"# The Python environment
.venv/
__pycache__/

# What Gaanim renders: each project's exports and captures, and the
# snapshots of visual checks
**/exports/*
!**/exports/.gitkeep
**/snapshots/
"#;
const PROJECT_GITIGNORE: &str = r#"exports/*
!exports/.gitkeep
snapshots/
__pycache__/
*.mp4
*.webm
*.webp
*.gif
"#;
/// The Python projects use: `gaanim init` pins it and uv installs it.
pub const PYTHON_VERSION: &str = "3.15";
/// Minor version of the Python 3 runtime the Python plugin is built against.
pub const PYTHON_MINOR: u16 = 15;
const RECENTS_LIMIT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Video,
    Slides,
}

impl ProjectKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "video" => Ok(Self::Video),
            "slides" => Ok(Self::Slides),
            "presentation" | "thesis" => Err(format!(
                "project kind `{value}` is no longer supported; change kind = \"{value}\" to kind = \"slides\""
            )),
            _ => Err(format!(
                "unknown project kind `{value}`; available kinds: video, slides"
            )),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Slides => "slides",
        }
    }

    pub const fn default_directory(self) -> &'static str {
        match self {
            Self::Video => "gaanim-video",
            Self::Slides => "gaanim-slides",
        }
    }

    pub const fn source(self) -> &'static str {
        match self {
            Self::Video => VIDEO_PROJECT_TEMPLATE,
            Self::Slides => SLIDES_PROJECT_TEMPLATE,
        }
    }

    pub const fn is_slides(self) -> bool {
        matches!(self, Self::Slides)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateWorkspaceOptions {
    pub directory: PathBuf,
    pub force: bool,
}

/// The folder a new workspace goes in when none is given.
pub const DEFAULT_WORKSPACE_DIRECTORY: &str = "gaanim-workspace";

/// The shared Python package a new workspace gets in `src/`: the same name
/// in every workspace, so it does not follow a rename of the folder.
const SHARED_PACKAGE: &str = "common";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProjectOptions {
    pub kind: ProjectKind,
    pub directory: PathBuf,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectManifest {
    pub name: String,
    pub kind: ProjectKind,
    pub entry: PathBuf,
    pub assets_dir: PathBuf,
    pub output_dir: PathBuf,
    /// Base URL of the relay that carries audience votes (`[polls] relay`),
    /// deployed from `gaanim relay init`.
    pub poll_relay: Option<String>,
    /// The language the project's talk is spoken in (`language`), as a
    /// BCP 47 tag such as `es-ES` or `es`.
    pub language: Option<String>,
    /// Keys whose names look like credentials (`token`, `api_key`, …),
    /// which have no place in a project that may end up in a repository.
    pub secret_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawProjectManifest {
    name: Option<String>,
    kind: Option<String>,
    entry: Option<PathBuf>,
    assets_dir: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    language: Option<String>,
    #[serde(default)]
    polls: RawPolls,
    workspace: Option<workspace::RawWorkspace>,
}

impl RawProjectManifest {
    /// Whether the manifest describes a project: a workspace's may not,
    /// and one that only sets `assets_dir` for `load_project` does not.
    fn declares_project(&self) -> bool {
        self.kind.is_some() || self.entry.is_some()
    }
}

/// The manifest at `manifest_path`, read and parsed but not validated.
fn read_manifest(manifest_path: &Path) -> Result<(String, RawProjectManifest), String> {
    let source = std::fs::read_to_string(manifest_path).map_err(|error| {
        format!(
            "could not read project manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    let raw = toml::from_str(&source).map_err(|error| {
        format!(
            "invalid project manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    Ok((source, raw))
}

/// Words that make a key's name look like a credential.
const SECRET_WORDS: [&str; 6] = [
    "token", "secret", "password", "apikey", "api_key", "api-key",
];

/// The dotted names of the keys of `table` that look like credentials.
fn secret_keys(table: &toml::Table, prefix: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (key, value) in table {
        let name = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let lowered = key.to_lowercase();
        if SECRET_WORDS.iter().any(|word| lowered.contains(word)) {
            found.push(name.clone());
        }
        if let toml::Value::Table(inner) = value {
            found.extend(secret_keys(inner, &name));
        }
    }
    found
}

/// Pin the language of the project whose manifest is `manifest_path`
/// (`language = "es-ES"`), or remove it with `None`. The rest of the file
/// stays as written, comments included.
pub fn set_project_language(manifest_path: &Path, language: Option<&str>) -> Result<(), String> {
    let source = std::fs::read_to_string(manifest_path).map_err(|error| {
        format!(
            "could not read project manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    let mut document: toml_edit::DocumentMut = source.parse().map_err(|error| {
        format!(
            "invalid project manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    match language
        .map(str::trim)
        .filter(|language| !language.is_empty())
    {
        Some(language) => document["language"] = toml_edit::value(language),
        None => {
            document.remove("language");
        }
    }
    // Through a sibling the file watcher ignores, so the manifest is never
    // seen half written.
    let temporary = manifest_path.with_extension("toml.tmp");
    std::fs::write(&temporary, document.to_string())
        .and_then(|()| std::fs::rename(&temporary, manifest_path))
        .map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            format!("could not write {}: {error}", manifest_path.display())
        })
}

#[derive(Debug, Default, Deserialize)]
struct RawPolls {
    relay: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProject {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: ProjectManifest,
    pub entry: PathBuf,
    /// The workspace the project is a member of, or the one its own
    /// manifest declares.
    pub workspace: Option<Workspace>,
}

impl ResolvedProject {
    /// The project's name with its workspace's in front (`canal › ep1`)
    /// when it is a member of one, for titles and lists.
    pub fn display_name(&self) -> String {
        match &self.workspace {
            Some(workspace) if workspace.root != self.root => {
                format!("{} › {}", workspace.name, self.manifest.name)
            }
            _ => self.manifest.name.clone(),
        }
    }

    /// Where the project's Python environment lives: its own folder when it
    /// has a virtual environment, else its workspace's root, else its own
    /// folder, where one is created.
    pub fn environment_root(&self) -> &Path {
        let own = VENV_NAMES
            .iter()
            .any(|name| venv_python(&self.root.join(name)).is_some());
        match &self.workspace {
            Some(workspace) if !own => &workspace.root,
            _ => &self.root,
        }
    }
}

/// What a script or folder belongs to: its project and the workspace that
/// project is a member of, or only a workspace, for a file in a workspace's
/// own folders (a shared module, a scratch script).
#[derive(Debug, Clone, Default)]
pub struct Location {
    pub project: Option<ResolvedProject>,
    pub workspace: Option<Workspace>,
}

/// A folder `gaanim` can open: a project, or a workspace without one.
// One at a time, briefly: boxing the project would only add indirection.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ProjectFolder {
    Project(ResolvedProject),
    Workspace(Workspace),
}

/// The project or workspace whose `gaanim.toml` is in the folder `path`.
pub fn resolve_folder(path: &Path) -> Result<ProjectFolder, String> {
    if path.is_dir() {
        let (_, raw) = read_manifest(&path.join("gaanim.toml"))?;
        if !raw.declares_project() && raw.workspace.is_some() {
            return resolve_workspace(path).map(ProjectFolder::Workspace);
        }
    }
    resolve_project(path).map(ProjectFolder::Project)
}

pub fn create_project(options: &CreateProjectOptions) -> Result<ResolvedProject, String> {
    if options.directory.is_file() {
        return Err(format!(
            "{} is a file; choose a project directory",
            options.directory.display()
        ));
    }
    let project_name = options
        .directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(options.kind.default_directory());
    let manifest = format!(
        "name = \"{}\"\nkind = \"{}\"\nentry = \"main.py\"\nassets_dir = \"assets\"\noutput_dir = \"exports\"\n",
        escape_manifest_value(project_name),
        options.kind.name(),
    );
    let readme = project_readme(project_name, options.kind);
    let pyproject = uv_bare_pyproject(project_name);
    let mut files = vec![
        (options.directory.join("main.py"), options.kind.source()),
        (options.directory.join("gaanim.toml"), manifest.as_str()),
        (options.directory.join("assets").join(".gitkeep"), ""),
    ];
    // Inside a workspace, the Python environment, the ignore rules and the
    // agent notes are the workspace's.
    if workspace_around(&options.directory).is_none() {
        files.extend([
            (options.directory.join("pyproject.toml"), pyproject.as_str()),
            (options.directory.join(".python-version"), PYTHON_VERSION),
            (options.directory.join(".gitignore"), PROJECT_GITIGNORE),
            (options.directory.join("README.md"), readme.as_str()),
            (options.directory.join("AGENTS.md"), PROJECT_AGENTS),
            (options.directory.join("exports").join(".gitkeep"), ""),
        ]);
    }
    if !options.force
        && let Some((path, _)) = files.iter().find(|(path, _)| path.exists())
    {
        return Err(format!(
            "{} already exists (use --force to update scaffold files)",
            path.display()
        ));
    }
    for (path, source) in files {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, source)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    }
    resolve_project(&options.directory)
}

/// Create a workspace ready for a repository: a `gaanim.toml` whose
/// members are `videos/*` and `talks/*`, a shared Python package in `src/`,
/// shared `assets/`, the Python environment's `pyproject.toml`, a
/// `.gitignore`, a README and agent notes, and a first member,
/// `videos/intro`. The names are only a start: the manifest says where the
/// members are.
pub fn create_workspace(options: &CreateWorkspaceOptions) -> Result<Workspace, String> {
    if options.directory.is_file() {
        return Err(format!(
            "{} is a file; choose a workspace directory",
            options.directory.display()
        ));
    }
    let name = options
        .directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(DEFAULT_WORKSPACE_DIRECTORY);
    let manifest = workspace_manifest(name);
    let readme = workspace_readme(name);
    let pyproject = uv_bare_pyproject(name);
    let shared = format!(
        "\"\"\"Code every project of the {name} workspace shares.\n\n\
         Import it from any of them (`import {SHARED_PACKAGE}`): Gaanim puts the\n\
         workspace's `src/` folder on `sys.path` and reloads this package when\n\
         it changes.\n\"\"\"\n"
    );
    let directory = &options.directory;
    let files = [
        (directory.join("gaanim.toml"), manifest.as_str()),
        (
            directory
                .join("src")
                .join(SHARED_PACKAGE)
                .join("__init__.py"),
            shared.as_str(),
        ),
        (directory.join("assets").join(".gitkeep"), ""),
        (directory.join("pyproject.toml"), pyproject.as_str()),
        (directory.join(".python-version"), PYTHON_VERSION),
        (directory.join(".gitignore"), WORKSPACE_GITIGNORE),
        (directory.join("README.md"), readme.as_str()),
        (directory.join("AGENTS.md"), WORKSPACE_AGENTS),
    ];
    if !options.force
        && let Some((path, _)) = files.iter().find(|(path, _)| path.exists())
    {
        return Err(format!(
            "{} already exists (use --force to update scaffold files)",
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
    let first = directory.join("videos").join("intro");
    if !first.join("gaanim.toml").exists() {
        create_project(&CreateProjectOptions {
            kind: ProjectKind::Video,
            directory: first,
            force: false,
        })?;
    }
    resolve_workspace(directory)
}

/// A workspace's `gaanim.toml`, explained for whoever edits it next.
fn workspace_manifest(name: &str) -> String {
    format!(
        r#"name = "{}"

# A Gaanim workspace: projects that share code, assets and Python.
# https://paololupo.github.io/gaanim/referencia/gaanim-toml/#workspace
[workspace]
# Where the projects are, relative to this folder. `*` stands for any part
# of a folder name, at any depth ("season-1/*", "events/*/talk"), and an
# entry can name its group: {{ path = "season-1/*", group = "Season 1" }}.
# A folder is a member only if it has its own gaanim.toml.
members = ["videos/*", "talks/*"]

[workspace.shared]
# Python on every member's sys.path, after the member's own folders.
python = ["src"]
# Where a member looks for an asset it does not have itself.
assets = ["assets"]
"#,
        escape_manifest_value(name)
    )
}

fn workspace_readme(name: &str) -> String {
    format!(
        "# {name}\n\n\
         Workspace de Gaanim: varios proyectos (vídeos, charlas) que comparten código, \
         recursos y entorno de Python.\n\n\
         ## Estructura\n\n\
         ```text\n\
         gaanim.toml        # el workspace: dónde están los proyectos y qué comparten\n\
         src/{SHARED_PACKAGE}/        # código de Python compartido: `import {SHARED_PACKAGE}`\n\
         assets/            # recursos compartidos: imágenes, SVG, fuentes, sonidos\n\
         videos/intro/      # un proyecto: gaanim.toml, main.py y sus assets/\n\
         talks/             # charlas y presentaciones\n\
         ```\n\n\
         La estructura es tuya: cambia `members` en `gaanim.toml` (por ejemplo \
         `\"season-1/*\"`) y mueve las carpetas. Un proyecto busca primero en sus \
         propias carpetas y después en las compartidas. Otro paquete junto a \
         `src/{SHARED_PACKAGE}/`, como `src/charts/`, se importa igual desde \
         todos.\n\n\
         ## Trabajar\n\n\
         ```bash\n\
         gaanim .                          # el Inicio, con los proyectos del workspace\n\
         gaanim videos/intro               # vista previa con recarga al guardar\n\
         gaanim init video videos/nuevo    # un vídeo nuevo\n\
         gaanim init slides talks/charla   # una presentación nueva\n\
         ```\n\n\
         ## Validar\n\n\
         ```bash\n\
         gaanim check .                    # todos los proyectos; útil en CI\n\
         gaanim check . --members \"videos/*\"\n\
         ```\n\n\
         ## Exportar\n\n\
         ```bash\n\
         gaanim export videos/intro        # a videos/intro/exports/intro.mp4\n\
         gaanim export . --quality production   # todos los proyectos\n\
         gaanim export . -o renders/{{member}}.mp4\n\
         ```\n\n\
         Las exportaciones van a la carpeta `exports/` de cada proyecto, que Git \
         ignora. El entorno de Python está en `.venv`, en la raíz; si falta, \
         `uv sync` lo crea.\n"
    )
}

/// Make `directory` a Git repository unless it is inside one already or
/// Git is not installed. Whether it made one.
pub fn init_git_repository(directory: &Path) -> bool {
    let inside = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(directory)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match inside {
        // Inside a repository already.
        Ok(status) if status.success() => false,
        Ok(_) => Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(directory)
            .stdin(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success()),
        // No Git.
        Err(_) => false,
    }
}

pub fn resolve_project(path: &Path) -> Result<ResolvedProject, String> {
    if !path.exists() {
        return Err(format!("project not found: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("expected a project directory: {}", path.display()));
    }
    let root = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let manifest_path = root.join("gaanim.toml");
    let (source, raw) = read_manifest(&manifest_path)?;
    if !raw.declares_project() && raw.workspace.is_some() {
        return Err(format!(
            "{} is a workspace, not a project: open one of its members",
            root.display()
        ));
    }
    let missing = |field: &str| {
        format!(
            "invalid project manifest {}: missing field `{field}`",
            manifest_path.display()
        )
    };
    let raw_kind = raw.kind.as_deref().ok_or_else(|| missing("kind"))?;
    let raw_entry = raw.entry.clone().ok_or_else(|| missing("entry"))?;
    let secret_keys = toml::from_str::<toml::Table>(&source)
        .map(|table| secret_keys(&table, ""))
        .unwrap_or_default();
    // A manifest found above a script is named, so it can be fixed.
    let invalid = |error: String| {
        format!(
            "invalid project manifest {}: {error}",
            manifest_path.display()
        )
    };
    let kind = ProjectKind::parse(raw_kind).map_err(invalid)?;
    validate_relative_path(&raw_entry, "entry").map_err(invalid)?;
    let entry = root.join(&raw_entry);
    if !entry.is_file() {
        return Err(format!(
            "project entry does not exist or is not a file: {}",
            entry.display()
        ));
    }
    let workspace = match raw.workspace {
        Some(workspace) => Some(Workspace::from_raw(
            &root,
            &manifest_path,
            raw.name.as_deref(),
            workspace,
        )?),
        None => workspace::find_workspace_for(&root)?,
    };
    let name = raw
        .name
        .filter(|name| !name.trim().is_empty())
        .or_else(|| root.file_name().and_then(OsStr::to_str).map(str::to_owned))
        .unwrap_or_else(|| kind.default_directory().to_string());
    Ok(ResolvedProject {
        root,
        manifest_path,
        workspace,
        manifest: ProjectManifest {
            name,
            kind,
            entry: raw_entry,
            assets_dir: raw.assets_dir.unwrap_or_else(|| PathBuf::from("assets")),
            output_dir: raw.output_dir.unwrap_or_else(|| PathBuf::from("exports")),
            poll_relay: raw
                .polls
                .relay
                .map(|relay| relay.trim().to_string())
                .filter(|relay| !relay.is_empty()),
            language: raw
                .language
                .map(|language| language.trim().to_string())
                .filter(|language| !language.is_empty()),
            secret_keys,
        },
        entry: entry.canonicalize().unwrap_or(entry),
    })
}

pub fn resolve_entry(path: &Path) -> Result<PathBuf, String> {
    if path.is_file() {
        return Ok(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()));
    }
    resolve_project(path).map(|project| project.entry)
}

/// The project `script` belongs to, if any; see [`locate_script`], which
/// also reports an invalid manifest instead of passing it over.
pub fn find_project_for_script(script: &Path) -> Option<ResolvedProject> {
    locate_script(script).ok()?.project
}

/// What `script` belongs to, from the nearest `gaanim.toml` in its folder or
/// up to five folders above.
pub fn locate_script(script: &Path) -> Result<Location, String> {
    match script.parent() {
        Some(folder) => locate_folder(folder),
        None => Ok(Location::default()),
    }
}

/// What `folder` belongs to, from the nearest `gaanim.toml` in it or up to
/// five folders above. A manifest that declares a project or a workspace
/// and is invalid is an error naming it; one that declares neither, such as
/// a manifest that only sets `assets_dir` for `load_project`, is passed over.
pub fn locate_folder(folder: &Path) -> Result<Location, String> {
    let mut current = folder.to_path_buf();
    for _ in 0..=5 {
        let manifest = current.join("gaanim.toml");
        if manifest.is_file() {
            let (_, raw) = read_manifest(&manifest)?;
            if raw.declares_project() {
                let project = resolve_project(&current)?;
                return Ok(Location {
                    workspace: project.workspace.clone(),
                    project: Some(project),
                });
            }
            if raw.workspace.is_some() {
                return Ok(Location {
                    project: None,
                    workspace: Some(resolve_workspace(&current)?),
                });
            }
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
    Ok(Location::default())
}

fn validate_relative_path(path: &Path, field: &str) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "project {field} must stay inside the project directory: {:?}",
            path.to_string_lossy()
        ));
    }
    Ok(())
}

fn escape_manifest_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The minimal project metadata generated by `uv init --bare --python`
/// [`PYTHON_VERSION`], plus Gaanim as the project's authoring dependency.
fn uv_bare_pyproject(name: &str) -> String {
    let normalized_name = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let normalized_name = normalized_name.trim_matches('-');
    let normalized_name = if normalized_name.is_empty() {
        "gaanim-project"
    } else {
        normalized_name
    };
    format!(
        "[project]\nname = \"{normalized_name}\"\nversion = \"0.1.0\"\n\
requires-python = \">={PYTHON_VERSION}\"\ndependencies = [\"gaanim\"]\n"
    )
}

fn project_readme(name: &str, kind: ProjectKind) -> String {
    let extra = if kind.is_slides() {
        "\n## Presentar\n\n```powershell\ngaanim --present --monitor 1 .\n```\n"
    } else {
        "\n## Exportar\n\n```powershell\ngaanim export . --output exports/video.mp4 --quality production\n```\n"
    };
    format!(
        "# {name}\n\nProyecto `{}` generado por Gaanim.\n\n## Editar y previsualizar\n\n\
         Edita `main.py` y ejecuta:\n\n```powershell\ngaanim .\n```\n\n\
         Los recursos van en `assets/`; las salidas generadas van en `exports/`.\n\
         {extra}\n## Validar\n\n```powershell\ngaanim check .\n```\n\n\
         ## Compartir\n\nGraba la escena en un solo archivo que se reproduce, presenta y \
         exporta a video sin Python:\n\n```powershell\n\
         gaanim export . --output exports/escena.gaanim\n\
         gaanim --present exports/escena.gaanim\n```\n",
        kind.name()
    )
}

/// Recently opened projects and playback bundles, newest first.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecentProjects {
    /// Project roots and `.gaanim` files in one list, so both keep a common
    /// order. Versions that only know projects skip the bundle entries.
    #[serde(default)]
    paths: Vec<PathBuf>,
    /// Entries pinned to the top, in the order they were pinned. They stay
    /// past the limit of recent entries and through *Clear list*.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pinned: Vec<PathBuf>,
}

/// A recent entry as Home lists it: a project shows the newest bundle
/// exported inside its folder instead of listing it apart.
#[derive(Debug, Clone)]
pub struct RecentGroup {
    pub entry: RecentEntry,
    pub bundle: Option<RecentBundle>,
    pub pinned: bool,
}

/// Whether `path` lies inside `folder`, ignoring case on Windows.
fn is_inside(path: &Path, folder: &Path) -> bool {
    if cfg!(windows) {
        // Canonical paths carry a verbatim prefix plain ones lack.
        let lower = |path: &Path| {
            let text = path.to_string_lossy().to_lowercase().replace('/', "\\");
            text.strip_prefix(r"\\?\")
                .map(str::to_owned)
                .unwrap_or(text)
        };
        let folder = lower(folder);
        let folder = folder.trim_end_matches('\\');
        let path = lower(path);
        path.len() > folder.len()
            && path.starts_with(folder)
            && path[folder.len()..].starts_with('\\')
    } else {
        path.starts_with(folder) && path != folder
    }
}

/// A playback bundle (`.gaanim`) opened recently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentBundle {
    pub path: PathBuf,
    /// The file name without its extension.
    pub name: String,
}

/// One entry of [`RecentProjects`].
// A few dozen entries at most: boxing projects would only add indirection.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum RecentEntry {
    Project(ResolvedProject),
    Bundle(RecentBundle),
    /// A workspace opened in Home. Versions without workspaces cannot
    /// resolve it as a project and skip it.
    Workspace(Workspace),
}

impl RecentEntry {
    /// The path that identifies the entry: a project or workspace root, or
    /// a bundle file.
    pub fn path(&self) -> &Path {
        match self {
            Self::Project(project) => &project.root,
            Self::Bundle(bundle) => &bundle.path,
            Self::Workspace(workspace) => &workspace.root,
        }
    }
}

/// Whether `path` names a playback bundle by its extension.
pub fn is_bundle_path(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gaanim"))
}

/// The bundle at `path`, if it still exists.
fn resolve_bundle(path: &Path) -> Option<RecentBundle> {
    if !is_bundle_path(path) || !path.is_file() {
        return None;
    }
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let name = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("Paquete")
        .to_owned();
    Some(RecentBundle { path, name })
}

fn resolve_entry_path(path: &Path) -> Option<RecentEntry> {
    if is_bundle_path(path) {
        return resolve_bundle(path).map(RecentEntry::Bundle);
    }
    match resolve_folder(path).ok()? {
        ProjectFolder::Project(project) => Some(RecentEntry::Project(project)),
        ProjectFolder::Workspace(workspace) => Some(RecentEntry::Workspace(workspace)),
    }
}

impl RecentProjects {
    pub fn load() -> Self {
        recent_projects_path()
            .and_then(|path| Self::load_from(&path).ok())
            .unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let stored: Self = serde_json::from_str(&source).unwrap_or_default();
        let mut seen = HashSet::new();
        let pinned: Vec<PathBuf> = stored
            .pinned
            .into_iter()
            .filter_map(|candidate| resolve_entry_path(&candidate))
            .map(|entry| entry.path().to_path_buf())
            .filter(|path| seen.insert(path.clone()))
            .collect();
        let mut valid = Vec::new();
        for candidate in stored.paths {
            if valid.len() == RECENTS_LIMIT {
                break;
            }
            if let Some(entry) = resolve_entry_path(&candidate)
                && seen.insert(entry.path().to_path_buf())
            {
                valid.push(entry.path().to_path_buf());
            }
        }
        Ok(Self {
            paths: valid,
            pinned,
        })
    }

    /// Projects and bundles that still exist: the pinned ones, then the
    /// rest newest first.
    pub fn entries(&self) -> Vec<RecentEntry> {
        self.pinned
            .iter()
            .chain(self.paths.iter().filter(|path| !self.pinned.contains(path)))
            .filter_map(|path| resolve_entry_path(path))
            .collect()
    }

    /// [`Self::entries`] as Home lists them: a bundle inside a listed
    /// project's folder goes with that project, the newest one only, and is
    /// not listed apart.
    pub fn groups(&self) -> Vec<RecentGroup> {
        group_entries(self.entries(), &self.pinned)
    }

    pub fn is_pinned(&self, path: &Path) -> bool {
        self.pinned.iter().any(|pinned| pinned == path)
    }

    /// Pin `path` to the top of the list, or unpin it.
    pub fn set_pinned(&mut self, path: &Path, pinned: bool) {
        self.pinned.retain(|existing| existing != path);
        if pinned {
            self.pinned.push(path.to_path_buf());
        }
    }

    /// The recent projects, newest first, without bundles.
    pub fn projects(&self) -> Vec<ResolvedProject> {
        self.entries()
            .into_iter()
            .filter_map(|entry| match entry {
                RecentEntry::Project(project) => Some(project),
                RecentEntry::Bundle(_) | RecentEntry::Workspace(_) => None,
            })
            .collect()
    }

    pub fn record(&mut self, project: &ResolvedProject) {
        self.record_path(project.root.clone());
    }

    /// Record the workspace opened in Home as the newest entry.
    pub fn record_workspace(&mut self, workspace: &Workspace) {
        self.record_path(workspace.root.clone());
    }

    /// Record the playback bundle at `path` as the newest entry.
    pub fn record_bundle(&mut self, path: &Path) {
        if let Some(bundle) = resolve_bundle(path) {
            self.record_path(bundle.path);
        }
    }

    fn record_path(&mut self, path: PathBuf) {
        self.paths.retain(|existing| existing != &path);
        self.paths.insert(0, path);
        self.paths.truncate(RECENTS_LIMIT);
    }

    /// Forget a project root or bundle file, pinned or not.
    pub fn remove(&mut self, path: &Path) {
        self.paths.retain(|existing| existing != path);
        self.pinned.retain(|existing| existing != path);
    }

    /// Forget every entry that is not pinned.
    pub fn clear(&mut self) {
        self.paths.clear();
    }

    pub fn save(&self) -> Result<(), String> {
        let path = recent_projects_path()
            .ok_or_else(|| "could not determine the Gaanim data directory".to_string())?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        let source = serde_json::to_string_pretty(self)
            .map_err(|error| format!("could not serialize recent projects: {error}"))?;
        std::fs::write(path, source)
            .map_err(|error| format!("could not write {}: {error}", path.display()))
    }
}

/// Fold each bundle inside a listed project's folder into that project:
/// the newest such bundle (the first in `entries`) goes with the first
/// project that holds it, and no bundle that went with a project is listed
/// apart.
fn group_entries(entries: Vec<RecentEntry>, pinned: &[PathBuf]) -> Vec<RecentGroup> {
    let roots: Vec<PathBuf> = entries
        .iter()
        .filter_map(|entry| match entry {
            RecentEntry::Project(project) => Some(project.root.clone()),
            RecentEntry::Bundle(_) | RecentEntry::Workspace(_) => None,
        })
        .collect();
    let mut newest: HashMap<PathBuf, RecentBundle> = HashMap::new();
    let mut folded = HashSet::new();
    for entry in &entries {
        if let RecentEntry::Bundle(bundle) = entry
            && let Some(root) = roots.iter().find(|root| is_inside(&bundle.path, root))
        {
            folded.insert(bundle.path.clone());
            newest.entry(root.clone()).or_insert_with(|| bundle.clone());
        }
    }
    entries
        .into_iter()
        .filter(|entry| !folded.contains(entry.path()))
        .map(|entry| {
            let bundle = match &entry {
                RecentEntry::Project(project) => newest.remove(&project.root),
                RecentEntry::Bundle(_) | RecentEntry::Workspace(_) => None,
            };
            let pinned = pinned.iter().any(|path| path == entry.path());
            RecentGroup {
                entry,
                bundle,
                pinned,
            }
        })
        .collect()
}

/// Record the playback bundle at `path` in the saved recent list.
pub fn record_recent_bundle(path: &Path) {
    let mut recents = RecentProjects::load();
    recents.record_bundle(path);
    let _ = recents.save();
}

fn recent_projects_path() -> Option<PathBuf> {
    user_data_dir().map(|dir| dir.join("recent-projects.json"))
}

/// Per-user directory for Gaanim state such as recent projects and editor
/// preferences.
pub fn user_data_dir() -> Option<PathBuf> {
    ProjectDirs::from("dev", "Gaanim", "Gaanim").map(|dirs| dirs.data_local_dir().to_path_buf())
}

/// Where the editor keeps the cover Home shows for the project at `root`: a
/// PNG in Gaanim's data folder, never in the project, so repositories stay
/// clean.
pub fn project_thumbnail_path(root: &Path) -> Option<PathBuf> {
    user_data_dir().map(|dir| thumbnail_in(&dir, root))
}

fn thumbnail_in(dir: &Path, root: &Path) -> PathBuf {
    dir.join("thumbnails")
        .join(format!("{:016x}.png", project_key(root)))
}

/// A stable name for the project at `root`, the same however the path is
/// written: FNV-1a of its canonical form (lowercase on Windows, whose paths
/// ignore case).
fn project_key(root: &Path) -> u64 {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut text = canonical.to_string_lossy().into_owned();
    if cfg!(windows) {
        text = text.to_lowercase();
    }
    let text = text.trim_end_matches(['/', '\\']);
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

pub fn default_project_parent() -> PathBuf {
    UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PythonVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl PythonVersion {
    /// On Windows the core links the version-independent `python3.dll`, so any
    /// newer runtime works. Elsewhere it links `libpython3.<minor>.so`, which
    /// only the exact minor version the core was built against provides.
    pub const fn is_supported(self) -> bool {
        if cfg!(windows) {
            self.major > 3 || (self.major == 3 && self.minor >= PYTHON_MINOR)
        } else {
            self.major == 3 && self.minor == PYTHON_MINOR
        }
    }
}

/// Human-readable requirement used in diagnostics.
pub fn python_requirement() -> String {
    if cfg!(windows) {
        format!("Python >=3.{PYTHON_MINOR}")
    } else {
        format!("Python 3.{PYTHON_MINOR}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PythonSource {
    ActiveVenv,
    ProjectVenv,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedPython {
    pub executable: PathBuf,
    pub home: PathBuf,
    pub version: PythonVersion,
    pub source: PythonSource,
    pub venv_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UvInfo {
    pub executable: PathBuf,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentProbe {
    pub python: Option<DetectedPython>,
    pub uv: Option<UvInfo>,
}

impl EnvironmentProbe {
    pub fn detect(project_hint: Option<&Path>) -> Self {
        detect_environment_with(project_hint, &SystemRunner)
    }

    /// The Python runtime [`Self::detect`] would select, without probing
    /// uv: all that loading Python needs.
    pub fn detect_python(project_hint: Option<&Path>) -> Self {
        Self {
            python: detect_python_with(project_hint, &SystemRunner),
            uv: None,
        }
    }

    pub fn has_supported_python(&self) -> bool {
        self.python
            .as_ref()
            .is_some_and(|python| python.version.is_supported())
    }

    pub fn has_venv(&self) -> bool {
        self.python
            .as_ref()
            .is_some_and(|python| python.venv_root.is_some())
    }
}

trait CommandRunner {
    fn output(&self, program: &OsStr, args: &[OsString]) -> Option<Output>;
}

struct SystemRunner;

/// How long one probe may run. Probes answer in well under a second; a probe
/// that blocks (a launcher prompting or downloading a runtime, a uv lock held
/// by another process) is abandoned so detection never hangs.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

impl CommandRunner for SystemRunner {
    fn output(&self, program: &OsStr, args: &[OsString]) -> Option<Output> {
        let mut command = python_command(program);
        command
            .args(args)
            // The Windows Python install manager's `py` otherwise offers to
            // install a missing runtime, which downloads or waits for input.
            .env("PYTHON_MANAGER_AUTOMATIC_INSTALL", "false")
            .env("PYTHON_MANAGER_CONFIRM", "false");
        output_with_timeout(command, PROBE_TIMEOUT)
    }
}

/// `Command::output` with a deadline: the child is killed once it expires.
fn output_with_timeout(mut command: Command, timeout: std::time::Duration) -> Option<Output> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    // Drain both pipes on threads so a chatty child cannot block on a full pipe.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(child.stdout.take().map(|pipe| Box::new(pipe) as _));
    let stderr = drain(child.stderr.take().map(|pipe| Box::new(pipe) as _));
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                // Grandchildren may still hold the pipes open; do not wait on them.
                return None;
            }
        }
    };
    Some(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

fn detect_environment_with(
    project_hint: Option<&Path>,
    runner: &impl CommandRunner,
) -> EnvironmentProbe {
    let python = detect_python_with(project_hint, runner);
    let uv = probe_uv(runner);
    EnvironmentProbe { python, uv }
}

fn detect_python_with(
    project_hint: Option<&Path>,
    runner: &impl CommandRunner,
) -> Option<DetectedPython> {
    active_venv()
        .and_then(|root| probe_venv(&root, PythonSource::ActiveVenv, runner))
        .or_else(|| {
            find_project_venv(project_hint)
                .and_then(|root| probe_venv(&root, PythonSource::ProjectVenv, runner))
        })
        .or_else(|| probe_system_python(runner))
}

fn probe_uv(runner: &impl CommandRunner) -> Option<UvInfo> {
    runner
        .output(OsStr::new("uv"), &[OsString::from("--version")])
        .filter(|output| output.status.success())
        .map(|output| UvInfo {
            executable: PathBuf::from("uv"),
            version: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        })
}

fn active_venv() -> Option<PathBuf> {
    std::env::var_os("VIRTUAL_ENV")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}

fn find_project_venv(project_hint: Option<&Path>) -> Option<PathBuf> {
    let mut bases = Vec::new();
    if let Some(path) = project_hint {
        bases.push(if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(path).to_path_buf()
        });
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    bases.iter().find_map(|base| venv_for_folder(base))
}

/// The environment scripts in `base` run in: the folder's own, then its
/// workspace's however far up, then those of the three folders above.
fn venv_for_folder(base: &Path) -> Option<PathBuf> {
    let workspace = locate_folder(base)
        .ok()
        .and_then(|location| location.workspace)
        .map(|workspace| workspace.root);
    std::iter::once(base.to_path_buf())
        .chain(workspace)
        .chain(base.ancestors().skip(1).take(3).map(Path::to_path_buf))
        .flat_map(|folder| VENV_NAMES.iter().map(move |name| folder.join(name)))
        .find(|candidate| venv_python(candidate).is_some())
}

/// Folder names a project's virtual environment goes by.
const VENV_NAMES: &[&str] = &[".venv", "venv", "env", ".venv312"];

fn venv_python(root: &Path) -> Option<PathBuf> {
    let candidates = if cfg!(windows) {
        vec![root.join("Scripts").join("python.exe")]
    } else {
        vec![
            root.join("bin").join("python3"),
            root.join("bin").join("python"),
        ]
    };
    candidates.into_iter().find(|path| path.is_file())
}

fn probe_venv(
    root: &Path,
    source: PythonSource,
    runner: &impl CommandRunner,
) -> Option<DetectedPython> {
    let executable = venv_python(root)?;
    if let Some((home, version)) = venv_base_runtime(root) {
        return Some(DetectedPython {
            executable,
            home,
            version,
            source,
            venv_root: Some(root.to_path_buf()),
        });
    }
    probe_python_command(
        executable.as_os_str(),
        &[],
        source,
        Some(root.to_path_buf()),
        runner,
    )
}

/// The runtime a virtual environment was made from, read from its
/// `pyvenv.cfg` instead of starting its interpreter, which takes up to
/// seconds the first time Windows sees a new environment: the runtime's
/// folder (`sys.base_prefix`) and version. Windows only, where that folder
/// holds the `python3.dll` loading Python needs; elsewhere, or when the
/// file lacks either value, the interpreter answers.
fn venv_base_runtime(root: &Path) -> Option<(PathBuf, PythonVersion)> {
    if !cfg!(windows) {
        return None;
    }
    let config = std::fs::read_to_string(root.join("pyvenv.cfg")).ok()?;
    let (mut home, mut version, mut version_info) = (None, None, None);
    for line in config.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "home" => home = Some(PathBuf::from(value)),
            // uv and virtualenv write `version_info`, the standard library
            // `version`.
            "version_info" => version_info = parse_python_version(value),
            "version" => version = parse_python_version(value),
            _ => {}
        }
    }
    let home = home.filter(|home| home.join("python3.dll").is_file())?;
    Some((home, version_info.or(version)?))
}

fn probe_system_python(runner: &impl CommandRunner) -> Option<DetectedPython> {
    #[cfg(windows)]
    let candidates: Vec<(&OsStr, Vec<OsString>)> = vec![
        (
            OsStr::new("py"),
            vec![OsString::from(format!("-{PYTHON_VERSION}"))],
        ),
        (OsStr::new("python"), vec![]),
        (OsStr::new("python3"), vec![]),
    ];
    #[cfg(not(windows))]
    let versioned = format!("python{PYTHON_VERSION}");
    #[cfg(not(windows))]
    let candidates: Vec<(&OsStr, Vec<OsString>)> = vec![
        (OsStr::new(&versioned), vec![]),
        (OsStr::new("python3"), vec![]),
        (OsStr::new("python"), vec![]),
    ];
    candidates
        .into_iter()
        .find_map(|(program, args)| {
            probe_python_command(program, &args, PythonSource::System, None, runner)
                .filter(|python| python.version.is_supported())
        })
        .or_else(|| probe_uv_managed_python(runner))
}

/// Fall back to a uv-managed interpreter, which is not on `PATH` by default.
fn probe_uv_managed_python(runner: &impl CommandRunner) -> Option<DetectedPython> {
    let output = runner
        .output(
            OsStr::new("uv"),
            &[
                OsString::from("python"),
                OsString::from("find"),
                OsString::from(PYTHON_VERSION),
            ],
        )
        .filter(|output| output.status.success())?;
    let executable = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if executable.is_empty() {
        return None;
    }
    probe_python_command(
        OsStr::new(&executable),
        &[],
        PythonSource::System,
        None,
        runner,
    )
    .filter(|python| python.version.is_supported())
}

/// Build a command for an external Python or uv process. The core runs with
/// `PYTHONHOME` pointing at its embedded runtime, which must not leak into
/// other interpreters (it would override their own prefix and venv).
fn python_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.env_remove("PYTHONHOME");
    command
}

fn probe_python_command(
    program: &OsStr,
    prefix_args: &[OsString],
    source: PythonSource,
    venv_root: Option<PathBuf>,
    runner: &impl CommandRunner,
) -> Option<DetectedPython> {
    let code = "import sys; print(sys.executable); print(sys.base_prefix); print(f'{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}')";
    let mut args = prefix_args.to_vec();
    args.extend([OsString::from("-c"), OsString::from(code)]);
    let output = runner.output(program, &args)?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let executable = PathBuf::from(lines.next()?.trim());
    let home = PathBuf::from(lines.next()?.trim());
    let version = parse_python_version(lines.next()?.trim())?;
    Some(DetectedPython {
        executable,
        home,
        version,
        source,
        venv_root,
    })
}

fn parse_python_version(value: &str) -> Option<PythonVersion> {
    let mut parts = value.split('.');
    Some(PythonVersion {
        major: parts.next()?.parse().ok()?,
        minor: parts.next()?.parse().ok()?,
        patch: parts.next().unwrap_or("0").parse().ok()?,
    })
}

pub fn activate_environment(probe: &EnvironmentProbe) -> Result<Option<PathBuf>, String> {
    let python = match &probe.python {
        Some(python) if python.version.is_supported() => python,
        Some(python) => {
            let version = python.version;
            let mut error = format!(
                "Gaanim requires {}, but found Python {}.{}.{} at {}",
                python_requirement(),
                version.major,
                version.minor,
                version.patch,
                python.executable.display()
            );
            // A project made for an older Gaanim keeps its environment.
            if let Some(root) = &python.venv_root {
                error.push_str(&format!(
                    "; set `.python-version` to {PYTHON_VERSION} and run \
                     `uv venv --python {PYTHON_VERSION} --clear` in {}",
                    root.parent().unwrap_or(root).display()
                ));
            }
            return Err(error);
        }
        None => return Err(format!("{} was not found", python_requirement())),
    };
    #[cfg(windows)]
    {
        prepend_to_path(&python.home);
        if let Some(root) = &python.venv_root {
            prepend_to_path(root.join("Scripts"));
        }
    }
    Ok(python.venv_root.clone())
}

/// The `PYTHONHOME` the embedded interpreter needs for the selected runtime
/// (Linux only). It cannot derive its prefix from the `gaanim` executable,
/// and relocated builds such as uv's report a stale compile-time prefix.
pub fn embedded_python_home(probe: &EnvironmentProbe) -> Option<PathBuf> {
    let python = probe
        .python
        .as_ref()
        .filter(|python| python.version.is_supported())?;
    cfg!(target_os = "linux").then(|| python.home.clone())
}

/// The shared `libpython` of the selected runtime, in the order to try
/// (Linux and macOS). The plugin links it, but the dynamic loader does not
/// search `<base_prefix>/lib` of uv, pyenv, and other non-system installs,
/// so `gaanim` loads it first. The bare file names come last, for the
/// loader's default search (a distribution's own Python).
pub fn python_library_candidates(probe: &EnvironmentProbe) -> Vec<PathBuf> {
    let Some(python) = probe
        .python
        .as_ref()
        .filter(|python| python.version.is_supported())
    else {
        return Vec::new();
    };
    let names: Vec<String> = if cfg!(target_os = "linux") {
        vec![
            format!("libpython3.{PYTHON_MINOR}.so.1.0"),
            format!("libpython3.{PYTHON_MINOR}.so"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![format!("libpython3.{PYTHON_MINOR}.dylib")]
    } else {
        return Vec::new();
    };
    let lib_dir = python.home.join("lib");
    names
        .iter()
        .map(|name| lib_dir.join(name))
        .chain(names.iter().map(PathBuf::from))
        .collect()
}

/// Create a project virtual environment with uv and install the bundled
/// authoring-only Gaanim wheel when it is missing or out of date.
pub fn provision_authoring_package(project_root: &Path) -> Result<PathBuf, String> {
    let wheel = bundled_authoring_wheel().ok_or_else(|| {
        format!(
            "bundled gaanim {} authoring wheel was not found next to the application",
            env!("CARGO_PKG_VERSION")
        )
    })?;
    let venv = project_root.join(".venv");
    if venv_python(&venv).is_none() {
        let status = python_command("uv")
            .current_dir(project_root)
            .args(["venv", "--python", PYTHON_VERSION, ".venv"])
            .status()
            .map_err(|error| format!("could not start uv: {error}"))?;
        if !status.success() {
            return Err(format!("uv could not create {}", venv.display()));
        }
    }
    let python = venv_python(&venv).ok_or_else(|| {
        format!(
            "uv did not create a usable environment at {}",
            venv.display()
        )
    })?;
    let installed = if authoring_package_installed(&venv) {
        Some(env!("CARGO_PKG_VERSION").to_string())
    } else {
        python_command(&python)
            .args([
                "-c",
                "import importlib.metadata as m; print(m.version('gaanim'))",
            ])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    if installed.as_deref() != Some(env!("CARGO_PKG_VERSION")) {
        let status = python_command("uv")
            .args([
                OsStr::new("pip"),
                OsStr::new("install"),
                OsStr::new("--python"),
            ])
            .arg(&python)
            .arg("--reinstall")
            .arg("--no-deps")
            .arg(&wheel)
            .status()
            .map_err(|error| format!("could not start uv: {error}"))?;
        if !status.success() {
            return Err(format!(
                "uv could not install the Gaanim authoring package into {}",
                venv.display()
            ));
        }
    }
    Ok(venv)
}

/// Whether the environment at `venv` holds this version's authoring
/// package, read from its installed metadata without starting Python.
fn authoring_package_installed(venv: &Path) -> bool {
    let metadata =
        Path::new(&format!("gaanim-{}.dist-info", env!("CARGO_PKG_VERSION"))).join("METADATA");
    let site_packages: Vec<PathBuf> = if cfg!(windows) {
        vec![venv.join("Lib").join("site-packages")]
    } else {
        std::fs::read_dir(venv.join("lib"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join("site-packages"))
            .collect()
    };
    site_packages
        .iter()
        .any(|folder| folder.join(&metadata).is_file())
}

fn bundled_authoring_wheel() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GAANIM_AUTHORING_WHEEL").map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    let filename = format!("gaanim-{}-py3-none-any.whl", env!("CARGO_PKG_VERSION"));
    let mut directories = vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("target/wheels"),
    ];
    if let Ok(executable) = std::env::current_exe()
        && let Some(bin_dir) = executable.parent()
    {
        directories.push(bin_dir.to_path_buf());
        if let Some(prefix) = bin_dir.parent() {
            directories.push(prefix.join("share/gaanim"));
        }
    }
    directories
        .into_iter()
        .map(|directory| directory.join(&filename))
        .find(|path| path.is_file())
}

#[cfg(windows)]
fn prepend_to_path(path: impl AsRef<Path>) {
    let path = path.as_ref();
    if !path.is_dir() {
        return;
    }
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut paths: Vec<PathBuf> = std::env::split_paths(&current).collect();
    if paths.iter().any(|candidate| candidate == path) {
        return;
    }
    paths.insert(0, path.to_path_buf());
    if let Ok(joined) = std::env::join_paths(paths) {
        unsafe { std::env::set_var("PATH", joined) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_keeps_its_thumbnail_however_its_path_is_written() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let path = thumbnail_in(data.path(), project.path());
        assert_eq!(
            path.parent(),
            Some(data.path().join("thumbnails").as_path())
        );
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("png"));
        let trailing = PathBuf::from(format!("{}/", project.path().display()));
        assert_eq!(thumbnail_in(data.path(), &trailing), path);
        let other = tempfile::tempdir().unwrap();
        assert_ne!(thumbnail_in(data.path(), other.path()), path);
    }

    #[test]
    fn probes_that_block_are_abandoned() {
        let sleeper = |seconds: &str| {
            if cfg!(windows) {
                let mut command = Command::new("powershell");
                command.args([
                    "-NoProfile",
                    "-Command",
                    &format!("Start-Sleep {seconds}; 'done'"),
                ]);
                command
            } else {
                let mut command = Command::new("sh");
                command.args(["-c", &format!("sleep {seconds}; echo done")]);
                command
            }
        };
        let started = std::time::Instant::now();
        assert!(
            output_with_timeout(sleeper("30"), std::time::Duration::from_millis(500)).is_none()
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        let output = output_with_timeout(sleeper("0"), std::time::Duration::from_secs(30)).unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "done");
    }

    #[test]
    fn the_language_is_read_pinned_and_unpinned_keeping_the_manifest_as_written() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("main.py"), "").unwrap();
        let manifest = temp.path().join("gaanim.toml");
        let written = "# Mi charla
name = \"charla\"  # visible en el Inicio
kind = \"slides\"
entry = \"main.py\"

[polls]
relay = \"https://relay.example\"
";
        std::fs::write(&manifest, written).unwrap();
        let project = resolve_project(temp.path()).unwrap();
        assert_eq!(project.manifest.language, None);
        assert!(project.manifest.secret_keys.is_empty());

        set_project_language(&manifest, Some("es-ES")).unwrap();
        let pinned = std::fs::read_to_string(&manifest).unwrap();
        assert!(
            pinned.starts_with(
                "# Mi charla
name = \"charla\"  # visible en el Inicio
"
            ),
            "{pinned}"
        );
        assert!(
            pinned.contains(
                "language = \"es-ES\"

[polls]"
            ),
            "{pinned}"
        );
        let project = resolve_project(temp.path()).unwrap();
        assert_eq!(project.manifest.language.as_deref(), Some("es-ES"));
        assert_eq!(
            project.manifest.poll_relay.as_deref(),
            Some("https://relay.example")
        );

        set_project_language(&manifest, None).unwrap();
        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), written);
        assert!(!temp.path().join("gaanim.toml.tmp").exists());
    }

    #[test]
    fn keys_that_look_like_credentials_are_reported() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("main.py"), "").unwrap();
        std::fs::write(
            temp.path().join("gaanim.toml"),
            "kind = \"slides\"
entry = \"main.py\"
scribe_token = \"x\"

[zoom]
api_key = \"y\"
language = \"es\"
",
        )
        .unwrap();
        let project = resolve_project(temp.path()).unwrap();
        assert_eq!(
            project.manifest.secret_keys,
            ["scribe_token", "zoom.api_key"]
        );
    }

    #[test]
    fn only_video_and_slides_are_project_kinds() {
        assert_eq!(ProjectKind::parse("video"), Ok(ProjectKind::Video));
        assert_eq!(ProjectKind::parse("slides"), Ok(ProjectKind::Slides));
        assert!(
            ProjectKind::parse("presentation")
                .unwrap_err()
                .contains("kind = \"slides\"")
        );
        assert!(ProjectKind::parse("thesis").is_err());
    }

    #[test]
    fn creates_and_resolves_both_project_kinds() {
        for kind in [ProjectKind::Video, ProjectKind::Slides] {
            let temp = tempfile::tempdir().unwrap();
            let directory = temp.path().join(kind.name());
            let project = create_project(&CreateProjectOptions {
                kind,
                directory: directory.clone(),
                force: false,
            })
            .unwrap();
            assert_eq!(project.manifest.kind, kind);
            assert!(directory.join("main.py").is_file());
            assert_eq!(
                std::fs::read_to_string(directory.join(".python-version")).unwrap(),
                PYTHON_VERSION
            );
            let pyproject = std::fs::read_to_string(directory.join("pyproject.toml")).unwrap();
            assert!(pyproject.contains("requires-python = \">=3.15\""));
            assert!(pyproject.contains("dependencies = [\"gaanim\"]"));
            assert!(project.entry.is_file());
            let source = std::fs::read_to_string(&project.entry).unwrap();
            assert!(source.contains("scene.render()"));
            assert!(!source.contains("scene.export("));
            let readme = std::fs::read_to_string(directory.join("README.md")).unwrap();
            let agents = std::fs::read_to_string(directory.join("AGENTS.md")).unwrap();
            assert!(agents.contains("gaanim/_docs"));
            assert!(agents.contains("gaanim check ."));
            if kind == ProjectKind::Video {
                assert!(readme.contains("gaanim export"));
            }
            assert!(readme.contains("\n## Compartir\n"));
            assert!(readme.contains("\ngaanim --present exports/escena.gaanim\n"));
        }
    }

    #[test]
    fn rejects_unsafe_entry_and_legacy_kind() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("outside.py"), "").unwrap();
        std::fs::write(
            temp.path().join("gaanim.toml"),
            "name = \"old\"\nkind = \"presentation\"\nentry = \"../outside.py\"\n",
        )
        .unwrap();
        assert!(resolve_project(temp.path()).is_err());
    }

    #[test]
    fn a_script_belongs_to_the_nearest_project_or_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let write = |relative: &str, text: &str| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "gaanim.toml",
            "name = \"canal\"\n[workspace]\nmembers = [\"shows/*\"]\n[workspace.shared]\npython = [\"common\"]\n",
        );
        write("common/helpers.py", "");
        write("shows/one/main.py", "");
        write(
            "shows/one/gaanim.toml",
            "kind = \"video\"\nentry = \"main.py\"\n",
        );
        write("shows/one/scenes/intro.py", "");

        let location = locate_script(&root.join("shows/one/scenes/intro.py")).unwrap();
        let project = location.project.expect("the member");
        assert_eq!(project.manifest.name, "one");
        let workspace = location.workspace.expect("its workspace");
        assert_eq!(workspace.name, "canal");
        assert_eq!(project.workspace.as_ref(), Some(&workspace));

        // A shared module belongs to the workspace alone.
        let location = locate_script(&root.join("common/helpers.py")).unwrap();
        assert!(location.project.is_none());
        assert_eq!(
            location.workspace.map(|ws| ws.name).as_deref(),
            Some("canal")
        );
        assert!(matches!(
            resolve_folder(root).unwrap(),
            ProjectFolder::Workspace(ws) if ws.name == "canal"
        ));
        assert!(
            resolve_project(root)
                .unwrap_err()
                .contains("is a workspace")
        );

        // A manifest that only sets `assets_dir` is passed over; one that
        // declares a project and is invalid is an error naming it.
        write("shows/one/scenes/gaanim.toml", "assets_dir = \"media\"\n");
        let location = locate_script(&root.join("shows/one/scenes/intro.py")).unwrap();
        assert_eq!(location.project.unwrap().manifest.name, "one");
        write(
            "shows/one/scenes/gaanim.toml",
            "kind = \"film\"\nentry = \"intro.py\"\n",
        );
        let error = locate_script(&root.join("shows/one/scenes/intro.py")).unwrap_err();
        assert!(error.contains("unknown project kind"), "{error}");
        assert!(error.contains("scenes"), "{error}");
        assert!(find_project_for_script(&root.join("shows/one/scenes/intro.py")).is_none());
        write("shows/one/scenes/gaanim.toml", "kind = \"video\"\n");
        let error = locate_script(&root.join("shows/one/scenes/intro.py")).unwrap_err();
        assert!(error.contains("missing field `entry`"), "{error}");
        assert!(error.contains("gaanim.toml"), "{error}");
    }

    #[test]
    fn a_member_uses_its_own_environment_or_its_workspace_one() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::write(
            root.join("gaanim.toml"),
            "[workspace]\nmembers = [\"a/b/c/*\"]\n",
        )
        .unwrap();
        let member = root.join("a/b/c/talk");
        std::fs::create_dir_all(&member).unwrap();
        std::fs::write(member.join("main.py"), "").unwrap();
        std::fs::write(
            member.join("gaanim.toml"),
            "kind = \"slides\"\nentry = \"main.py\"\n",
        )
        .unwrap();
        let project = resolve_project(&member).unwrap();
        assert_eq!(project.environment_root(), root.as_path());

        let make_venv = |folder: &Path| {
            let scripts = folder
                .join(".venv")
                .join(if cfg!(windows) { "Scripts" } else { "bin" });
            std::fs::create_dir_all(&scripts).unwrap();
            for name in ["python.exe", "python3"] {
                std::fs::write(scripts.join(name), "").unwrap();
            }
        };
        // The workspace's environment is found past four folders up.
        make_venv(&root);
        assert_eq!(find_project_venv(Some(&member)), Some(root.join(".venv")));
        make_venv(&member);
        let project = resolve_project(&member).unwrap();
        assert_eq!(project.environment_root(), member.as_path());
        assert_eq!(find_project_venv(Some(&member)), Some(member.join(".venv")));
    }

    #[test]
    fn a_new_project_inside_a_workspace_gets_only_its_own_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(
            root.join("gaanim.toml"),
            "[workspace]\nmembers = [\"shows/*\"]\n",
        )
        .unwrap();
        let directory = root.join("shows/pilot");
        let project = create_project(&CreateProjectOptions {
            kind: ProjectKind::Video,
            directory: directory.clone(),
            force: false,
        })
        .unwrap();
        assert!(project.workspace.is_some());
        assert!(directory.join("main.py").is_file());
        assert!(directory.join("assets/.gitkeep").is_file());
        for absent in [
            "pyproject.toml",
            ".python-version",
            ".gitignore",
            "README.md",
            "AGENTS.md",
            "exports",
        ] {
            assert!(!directory.join(absent).exists(), "{absent}");
        }
    }

    #[test]
    fn a_new_workspace_is_ready_for_its_repository() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("My Channel");
        let workspace = create_workspace(&CreateWorkspaceOptions {
            directory: directory.clone(),
            force: false,
        })
        .unwrap();
        assert_eq!(workspace.name, "My Channel");
        assert_eq!(workspace.python_roots, [workspace.root.join("src")]);
        assert_eq!(workspace.asset_roots, [workspace.root.join("assets")]);
        assert!(directory.join("src/common/__init__.py").is_file());
        for file in [
            ".gitignore",
            "pyproject.toml",
            ".python-version",
            "README.md",
            "AGENTS.md",
        ] {
            assert!(directory.join(file).is_file(), "{file}");
        }
        let ignore = std::fs::read_to_string(directory.join(".gitignore")).unwrap();
        assert!(ignore.contains(".venv/") && ignore.contains("**/exports/*"));
        assert!(!ignore.contains("*.webp"), "assets may be WebP images");
        let readme = std::fs::read_to_string(directory.join("README.md")).unwrap();
        assert!(readme.contains("`import common`"));
        // The first member is one, with only its own files.
        let members = workspace.members();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].path, "videos/intro");
        assert!(!directory.join("videos/intro/pyproject.toml").exists());
        // The manifest documents itself and stays valid TOML.
        let manifest = std::fs::read_to_string(directory.join("gaanim.toml")).unwrap();
        assert!(manifest.contains("members = [\"videos/*\", \"talks/*\"]"));
        assert!(
            create_workspace(&CreateWorkspaceOptions {
                directory: directory.clone(),
                force: false,
            })
            .unwrap_err()
            .contains("--force")
        );
        create_workspace(&CreateWorkspaceOptions {
            directory,
            force: true,
        })
        .unwrap();
    }

    #[test]
    fn recents_are_deduplicated_capped_and_persisted() {
        let temp = tempfile::tempdir().unwrap();
        let mut recents = RecentProjects::default();
        for index in 0..12 {
            let project = create_project(&CreateProjectOptions {
                kind: ProjectKind::Video,
                directory: temp.path().join(format!("p{index}")),
                force: false,
            })
            .unwrap();
            recents.record(&project);
        }
        let newest = recents.projects()[0].root.clone();
        recents.record(&resolve_project(&newest).unwrap());
        let storage = temp.path().join("recent.json");
        recents.save_to(&storage).unwrap();
        let loaded = RecentProjects::load_from(&storage).unwrap();
        assert_eq!(loaded.projects().len(), RECENTS_LIMIT);
        assert_eq!(loaded.projects()[0].root, newest);
    }

    #[test]
    fn recents_keep_bundles_and_projects_in_one_order() {
        let temp = tempfile::tempdir().unwrap();
        let project = create_project(&CreateProjectOptions {
            kind: ProjectKind::Slides,
            directory: temp.path().join("talk"),
            force: false,
        })
        .unwrap();
        let bundle = temp.path().join("talk.gaanim");
        std::fs::write(&bundle, b"bundle").unwrap();
        let not_a_bundle = temp.path().join("notes.txt");
        std::fs::write(&not_a_bundle, b"notes").unwrap();

        let mut recents = RecentProjects::default();
        recents.record(&project);
        recents.record_bundle(&bundle);
        recents.record_bundle(&not_a_bundle);
        recents.record_bundle(&temp.path().join("missing.gaanim"));
        let storage = temp.path().join("recent.json");
        recents.save_to(&storage).unwrap();

        let loaded = RecentProjects::load_from(&storage).unwrap();
        let entries = loaded.entries();
        assert_eq!(entries.len(), 2);
        match &entries[0] {
            RecentEntry::Bundle(recent) => assert_eq!(recent.name, "talk"),
            other => panic!("expected the bundle first, got {other:?}"),
        }
        assert!(matches!(&entries[1], RecentEntry::Project(recent) if recent.root == project.root));
        assert_eq!(loaded.projects().len(), 1);

        // A bundle that disappears drops out of the list.
        std::fs::remove_file(&bundle).unwrap();
        assert_eq!(
            RecentProjects::load_from(&storage).unwrap().entries().len(),
            1
        );
    }

    #[test]
    fn a_project_lists_the_newest_bundle_exported_inside_it() {
        let temp = tempfile::tempdir().unwrap();
        let project = create_project(&CreateProjectOptions {
            kind: ProjectKind::Slides,
            directory: temp.path().join("talk"),
            force: false,
        })
        .unwrap();
        let exports = project.root.join("exports");
        std::fs::create_dir_all(&exports).unwrap();
        let (old, new) = (exports.join("old.gaanim"), exports.join("new.gaanim"));
        let outside = temp.path().join("shared.gaanim");
        for bundle in [&old, &new, &outside] {
            std::fs::write(bundle, b"bundle").unwrap();
        }

        let mut recents = RecentProjects::default();
        recents.record_bundle(&old);
        recents.record(&project);
        recents.record_bundle(&outside);
        recents.record_bundle(&new);
        let groups = recents.groups();
        let names: Vec<String> = groups
            .iter()
            .map(|group| group.entry.path().display().to_string())
            .collect();
        assert_eq!(groups.len(), 2, "{names:?}");
        // Recorded bundle paths are canonical: compare their names.
        assert!(groups[0].entry.path().ends_with("shared.gaanim"));
        assert!(
            matches!(&groups[1].entry, RecentEntry::Project(found) if found.root == project.root)
        );
        assert!(
            groups[1]
                .bundle
                .as_ref()
                .is_some_and(|bundle| bundle.path.ends_with("new.gaanim"))
        );
        // A canonical path (`\\?\` on Windows) is inside its plain folder.
        let canonical = std::fs::canonicalize(&new).unwrap();
        let plain = PathBuf::from(
            project
                .root
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_owned(),
        );
        assert!(is_inside(&canonical, &plain));
        assert!(is_inside(&exports.join("x.gaanim"), &project.root));
        assert!(!is_inside(&project.root, &project.root));
        assert!(!is_inside(
            &temp.path().join("talk2/x.gaanim"),
            &project.root
        ));
    }

    #[test]
    fn pinned_entries_lead_and_outlast_the_limit_and_clearing() {
        let temp = tempfile::tempdir().unwrap();
        let mut recents = RecentProjects::default();
        for index in 0..=RECENTS_LIMIT {
            let bundle = temp.path().join(format!("b{index}.gaanim"));
            std::fs::write(&bundle, b"bundle").unwrap();
            recents.record_bundle(&bundle);
        }
        // The oldest fell past the limit; pinning keeps the second oldest,
        // by the path its entry carries, as Home pins it.
        let pinned = recents.entries().last().unwrap().path().to_path_buf();
        assert!(pinned.ends_with("b1.gaanim"));
        recents.set_pinned(&pinned, true);
        assert!(recents.is_pinned(&pinned));
        let more = temp.path().join("more.gaanim");
        std::fs::write(&more, b"bundle").unwrap();
        recents.record_bundle(&more);
        let storage = temp.path().join("recent.json");
        recents.save_to(&storage).unwrap();
        let mut loaded = RecentProjects::load_from(&storage).unwrap();
        let entries = loaded.entries();
        assert_eq!(entries[0].path(), pinned.as_path());
        assert!(entries[1].path().ends_with("more.gaanim"));
        assert!(loaded.groups()[0].pinned);
        loaded.clear();
        assert_eq!(loaded.entries().len(), 1);
        loaded.set_pinned(&pinned, false);
        assert!(loaded.entries().is_empty());
    }

    #[test]
    fn recents_keep_workspaces_and_name_members_after_them() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canal");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("gaanim.toml"),
            "name = \"canal\"\n[workspace]\nmembers = [\"shows/*\"]\n",
        )
        .unwrap();
        let member = create_project(&CreateProjectOptions {
            kind: ProjectKind::Video,
            directory: root.join("shows/pilot"),
            force: false,
        })
        .unwrap();
        assert_eq!(member.display_name(), "canal › pilot");
        let workspace = resolve_workspace(&root).unwrap();

        let mut recents = RecentProjects::default();
        recents.record(&member);
        recents.record_workspace(&workspace);
        let storage = temp.path().join("recent.json");
        recents.save_to(&storage).unwrap();
        let entries = RecentProjects::load_from(&storage).unwrap().entries();
        assert!(matches!(&entries[0], RecentEntry::Workspace(found) if found.name == "canal"));
        assert!(matches!(&entries[1], RecentEntry::Project(found) if found.root == member.root));
        assert_eq!(recents.projects().len(), 1);
    }

    #[test]
    fn corrupt_recent_file_recovers_as_empty() {
        let temp = tempfile::tempdir().unwrap();
        let storage = temp.path().join("recent.json");
        std::fs::write(&storage, "not json").unwrap();
        assert!(
            RecentProjects::load_from(&storage)
                .unwrap()
                .projects()
                .is_empty()
        );
    }

    #[test]
    fn supported_python_version_starts_at_3_15() {
        assert!(!parse_python_version("3.13.5").unwrap().is_supported());
        assert!(!parse_python_version("3.14.2").unwrap().is_supported());
        assert!(parse_python_version("3.15.0").unwrap().is_supported());
        assert!(parse_python_version("3.15.1").unwrap().is_supported());
        // Only Windows links the version-independent `python3.dll`.
        assert_eq!(
            parse_python_version("3.16.0").unwrap().is_supported(),
            cfg!(windows)
        );
    }

    #[test]
    fn uv_bare_metadata_uses_a_safe_python_project_name() {
        let pyproject = uv_bare_pyproject("Mi video 2026!");
        assert!(pyproject.contains("name = \"mi-video-2026\""));
        assert!(pyproject.contains("requires-python = \">=3.15\""));
        assert!(pyproject.contains("dependencies = [\"gaanim\"]"));
    }

    #[test]
    fn embedded_runtime_points_at_the_selected_python() {
        let home = PathBuf::from("/opt/python-3.15");
        let supported = EnvironmentProbe {
            python: Some(DetectedPython {
                executable: home.join("bin").join("python3"),
                home: home.clone(),
                version: parse_python_version("3.15.2").unwrap(),
                source: PythonSource::System,
                venv_root: None,
            }),
            uv: None,
        };
        let libraries = python_library_candidates(&supported);
        if cfg!(target_os = "linux") {
            assert_eq!(embedded_python_home(&supported), Some(home.clone()));
            assert_eq!(
                libraries.first(),
                Some(&home.join("lib").join("libpython3.15.so.1.0"))
            );
            assert_eq!(libraries.last(), Some(&PathBuf::from("libpython3.15.so")));
        } else {
            assert_eq!(embedded_python_home(&supported), None);
        }
        if cfg!(windows) {
            assert!(libraries.is_empty());
        }
        let missing = EnvironmentProbe::default();
        assert_eq!(embedded_python_home(&missing), None);
        assert!(python_library_candidates(&missing).is_empty());
    }

    struct FakeRunner {
        python: bool,
        uv: bool,
    }

    impl CommandRunner for FakeRunner {
        fn output(&self, program: &OsStr, _args: &[OsString]) -> Option<Output> {
            let enabled = if program == OsStr::new("uv") {
                self.uv
            } else {
                self.python
            };
            enabled.then(|| Output {
                status: success_status(),
                stdout: if program == OsStr::new("uv") {
                    b"uv 0.8.0\n".to_vec()
                } else {
                    b"C:\\Python315\\python.exe\nC:\\Python315\n3.15.2\n".to_vec()
                },
                stderr: Vec::new(),
            })
        }
    }

    #[cfg(windows)]
    fn success_status() -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }

    #[cfg(not(windows))]
    fn success_status() -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }

    #[test]
    fn a_virtual_environment_is_read_without_starting_python_on_windows() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("pythoncore-3.15-64");
        let venv = temp.path().join(".venv");
        let scripts = venv.join(if cfg!(windows) { "Scripts" } else { "bin" });
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(home.join("python3.dll"), "").unwrap();
        for name in ["python.exe", "python3", "python"] {
            std::fs::write(scripts.join(name), "").unwrap();
        }
        let write_config = |version_line: &str| {
            std::fs::write(
                venv.join("pyvenv.cfg"),
                format!(
                    "home = {}\ninclude-system-site-packages = false\n{version_line}\n",
                    home.display()
                ),
            )
            .unwrap();
        };
        let silent = FakeRunner {
            python: false,
            uv: false,
        };
        // uv writes `version_info`, the standard library `version`.
        for line in ["version_info = 3.15.2", "version = 3.15.2"] {
            write_config(line);
            let detected = probe_venv(&venv, PythonSource::ProjectVenv, &silent);
            if cfg!(windows) {
                let detected = detected.expect("read from pyvenv.cfg");
                assert_eq!(detected.home, home);
                assert_eq!(detected.venv_root.as_deref(), Some(venv.as_path()));
                assert_eq!(
                    detected.version,
                    PythonVersion {
                        major: 3,
                        minor: 15,
                        patch: 2
                    }
                );
            } else {
                assert!(detected.is_none());
            }
        }
        // Without the runtime library in `home`, the interpreter answers.
        std::fs::remove_file(home.join("python3.dll")).unwrap();
        assert!(probe_venv(&venv, PythonSource::ProjectVenv, &silent).is_none());
        let answering = FakeRunner {
            python: true,
            uv: false,
        };
        assert_eq!(
            probe_venv(&venv, PythonSource::ProjectVenv, &answering)
                .unwrap()
                .home,
            PathBuf::from("C:\\Python315")
        );
    }

    #[test]
    fn the_authoring_package_is_found_by_its_installed_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let venv = temp.path().join(".venv");
        assert!(!authoring_package_installed(&venv));
        let site_packages = if cfg!(windows) {
            venv.join("Lib").join("site-packages")
        } else {
            venv.join("lib").join("python3.15").join("site-packages")
        };
        let other = site_packages.join("gaanim-0.0.1.dist-info");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("METADATA"), "").unwrap();
        assert!(!authoring_package_installed(&venv));
        let this = site_packages.join(format!("gaanim-{}.dist-info", env!("CARGO_PKG_VERSION")));
        std::fs::create_dir_all(&this).unwrap();
        std::fs::write(this.join("METADATA"), "").unwrap();
        assert!(authoring_package_installed(&venv));
    }

    #[test]
    fn command_probe_reports_python_and_uv_without_mutating_the_environment() {
        let runner = FakeRunner {
            python: true,
            uv: true,
        };
        let python = probe_python_command(
            OsStr::new("python"),
            &[],
            PythonSource::System,
            None,
            &runner,
        )
        .unwrap();
        assert_eq!(
            python.version,
            PythonVersion {
                major: 3,
                minor: 15,
                patch: 2
            }
        );
        assert_eq!(probe_uv(&runner).unwrap().version, "uv 0.8.0");
        assert!(
            probe_uv(&FakeRunner {
                python: true,
                uv: false
            })
            .is_none()
        );
    }
}
