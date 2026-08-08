//! Shared project and environment support for the Gaanim launcher and editor.

use directories::{ProjectDirs, UserDirs};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

const VIDEO_PROJECT_TEMPLATE: &str = include_str!("../../../templates/video_project.py");
const SLIDES_PROJECT_TEMPLATE: &str = include_str!("../../../templates/slides_project.py");
const PROJECT_GITIGNORE: &str = r#"exports/*
!exports/.gitkeep
snapshots/
__pycache__/
*.mp4
*.webm
*.webp
*.gif
"#;
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
}

#[derive(Debug, Deserialize)]
struct RawProjectManifest {
    name: Option<String>,
    kind: String,
    entry: PathBuf,
    assets_dir: Option<PathBuf>,
    output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProject {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: ProjectManifest,
    pub entry: PathBuf,
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
    let files = [
        (options.directory.join("main.py"), options.kind.source()),
        (options.directory.join("gaanim.toml"), manifest.as_str()),
        (options.directory.join(".gitignore"), PROJECT_GITIGNORE),
        (options.directory.join("README.md"), readme.as_str()),
        (options.directory.join("assets").join(".gitkeep"), ""),
        (options.directory.join("exports").join(".gitkeep"), ""),
    ];
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

pub fn resolve_project(path: &Path) -> Result<ResolvedProject, String> {
    if !path.exists() {
        return Err(format!("project not found: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("expected a project directory: {}", path.display()));
    }
    let root = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let manifest_path = root.join("gaanim.toml");
    let source = std::fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "could not read project manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    let raw: RawProjectManifest = toml::from_str(&source).map_err(|error| {
        format!(
            "invalid project manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    let kind = ProjectKind::parse(&raw.kind)?;
    validate_relative_path(&raw.entry, "entry")?;
    let entry = root.join(&raw.entry);
    if !entry.is_file() {
        return Err(format!(
            "project entry does not exist or is not a file: {}",
            entry.display()
        ));
    }
    let name = raw
        .name
        .filter(|name| !name.trim().is_empty())
        .or_else(|| root.file_name().and_then(OsStr::to_str).map(str::to_owned))
        .unwrap_or_else(|| kind.default_directory().to_string());
    Ok(ResolvedProject {
        root,
        manifest_path,
        manifest: ProjectManifest {
            name,
            kind,
            entry: raw.entry,
            assets_dir: raw.assets_dir.unwrap_or_else(|| PathBuf::from("assets")),
            output_dir: raw.output_dir.unwrap_or_else(|| PathBuf::from("exports")),
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

pub fn find_project_for_script(script: &Path) -> Option<ResolvedProject> {
    let mut current = script.parent()?.to_path_buf();
    for _ in 0..=5 {
        if current.join("gaanim.toml").is_file()
            && let Ok(project) = resolve_project(&current)
        {
            return Some(project);
        }
        current = current.parent()?.to_path_buf();
    }
    None
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

fn project_readme(name: &str, kind: ProjectKind) -> String {
    let extra = if kind.is_slides() {
        "\n## Presentar\n\n```powershell\ngaanim --present --monitor 1 .\n```\n"
    } else {
        "\n## Exportar\n\n```powershell\n$env:GAANIM_EXPORT = \"exports/video.mp4\"\ngaanim .\nRemove-Item Env:GAANIM_EXPORT\n```\n"
    };
    format!(
        "# {name}\n\nProyecto `{}` generado por Gaanim.\n\n## Editar y previsualizar\n\n\
         Edita `main.py` y ejecuta:\n\n```powershell\ngaanim .\n```\n\n\
         Los recursos van en `assets/`; las salidas generadas van en `exports/`.\n\
         {extra}\n## Validar\n\n```powershell\ngaanim check .\n```\n",
        kind.name()
    )
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecentProjects {
    #[serde(default)]
    paths: Vec<PathBuf>,
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
        let mut valid = Vec::new();
        let mut seen = HashSet::new();
        for candidate in stored.paths {
            if let Ok(project) = resolve_project(&candidate)
                && seen.insert(project.root.clone())
            {
                valid.push(project.root);
            }
            if valid.len() == RECENTS_LIMIT {
                break;
            }
        }
        Ok(Self { paths: valid })
    }

    pub fn projects(&self) -> Vec<ResolvedProject> {
        self.paths
            .iter()
            .filter_map(|path| resolve_project(path).ok())
            .collect()
    }

    pub fn record(&mut self, project: &ResolvedProject) {
        self.paths.retain(|path| path != &project.root);
        self.paths.insert(0, project.root.clone());
        self.paths.truncate(RECENTS_LIMIT);
    }

    pub fn remove(&mut self, root: &Path) {
        self.paths.retain(|path| path != root);
    }

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

fn recent_projects_path() -> Option<PathBuf> {
    ProjectDirs::from("dev", "Gaanim", "Gaanim")
        .map(|dirs| dirs.data_local_dir().join("recent-projects.json"))
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
    pub const fn is_supported(self) -> bool {
        self.major > 3 || (self.major == 3 && self.minor >= 12)
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

impl CommandRunner for SystemRunner {
    fn output(&self, program: &OsStr, args: &[OsString]) -> Option<Output> {
        Command::new(program).args(args).output().ok()
    }
}

fn detect_environment_with(
    project_hint: Option<&Path>,
    runner: &impl CommandRunner,
) -> EnvironmentProbe {
    let python = active_venv()
        .and_then(|root| probe_venv(&root, PythonSource::ActiveVenv, runner))
        .or_else(|| {
            find_project_venv(project_hint)
                .and_then(|root| probe_venv(&root, PythonSource::ProjectVenv, runner))
        })
        .or_else(|| probe_system_python(runner));
    let uv = probe_uv(runner);
    EnvironmentProbe { python, uv }
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
    const VENV_NAMES: &[&str] = &[".venv", "venv", "env", ".venv312"];
    for base in bases {
        let mut current = base;
        for _ in 0..4 {
            for name in VENV_NAMES {
                let candidate = current.join(name);
                if venv_python(&candidate).is_some() {
                    return Some(candidate);
                }
            }
            let Some(parent) = current.parent() else {
                break;
            };
            current = parent.to_path_buf();
        }
    }
    None
}

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
    probe_python_command(
        executable.as_os_str(),
        &[],
        source,
        Some(root.to_path_buf()),
        runner,
    )
}

fn probe_system_python(runner: &impl CommandRunner) -> Option<DetectedPython> {
    #[cfg(windows)]
    let candidates: Vec<(&OsStr, Vec<OsString>)> = vec![
        (OsStr::new("py"), vec![OsString::from("-3.14")]),
        (OsStr::new("py"), vec![OsString::from("-3.13")]),
        (OsStr::new("py"), vec![OsString::from("-3.12")]),
        (OsStr::new("python"), vec![]),
        (OsStr::new("python3"), vec![]),
    ];
    #[cfg(not(windows))]
    let candidates: Vec<(&OsStr, Vec<OsString>)> = vec![
        (OsStr::new("python3"), vec![]),
        (OsStr::new("python"), vec![]),
    ];
    candidates.into_iter().find_map(|(program, args)| {
        probe_python_command(program, &args, PythonSource::System, None, runner)
            .filter(|python| python.version.is_supported())
    })
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
    let python = probe
        .python
        .as_ref()
        .filter(|python| python.version.is_supported())
        .ok_or_else(|| "Python >=3.12 was not found".to_string())?;
    #[cfg(windows)]
    {
        prepend_to_path(&python.home);
        if let Some(root) = &python.venv_root {
            prepend_to_path(root.join("Scripts"));
        }
    }
    Ok(python.venv_root.clone())
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
            assert!(project.entry.is_file());
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
    fn supported_python_version_starts_at_3_12() {
        assert!(!parse_python_version("3.11.9").unwrap().is_supported());
        assert!(parse_python_version("3.12.0").unwrap().is_supported());
        assert!(parse_python_version("3.14.1").unwrap().is_supported());
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
                    b"C:\\Python312\\python.exe\nC:\\Python312\n3.12.8\n".to_vec()
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
                minor: 12,
                patch: 8
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
