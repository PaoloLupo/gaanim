//! What an editor needs to know about a file or folder, without running
//! Python: the project and workspace it belongs to, the folders its scripts
//! see on `sys.path`, and the Python environment to complete them with
//! (`gaanim project <PATH> --json`).

use crate::{Location, ResolvedProject, Workspace, locate_folder, venv_for_folder, venv_python};
use gaanim_core::console::plain_path;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// `path` as editors write it, without Windows' `\\?\` prefix.
fn text(path: &Path) -> String {
    plain_path(path).to_string_lossy().into_owned()
}

/// What `path` (a script, any file, or a folder) belongs to, as JSON.
pub fn describe(path: &Path) -> Result<Value, String> {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    if !absolute.exists() {
        return Err(format!("{} does not exist", path.display()));
    }
    let folder = if absolute.is_dir() {
        absolute.clone()
    } else {
        absolute
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| absolute.clone())
    };
    let location = locate_folder(&folder)?;
    // A file runs from its folder; a project folder runs its entry.
    let script_folder = match (&location.project, absolute.is_dir()) {
        (Some(project), true) => project
            .entry
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| project.root.clone()),
        _ => folder.clone(),
    };
    let environment = location
        .project
        .as_ref()
        .map(|project| project.environment_root().to_path_buf())
        .or_else(|| {
            location
                .workspace
                .as_ref()
                .map(|workspace| workspace.root.clone())
        })
        .unwrap_or_else(|| folder.clone());
    let venv = venv_for_folder(&environment);
    let paths: Vec<String> = location
        .python_paths(&script_folder)
        .iter()
        .map(|path| text(path))
        .collect();
    Ok(json!({
        "path": text(&absolute),
        "project": location.project.as_ref().map(project_json),
        "workspace": location.workspace.as_ref().map(workspace_json),
        "python": {
            "paths": paths,
            "venv": venv.as_deref().map(text),
            "interpreter": venv.as_deref().and_then(venv_python).as_deref().map(text),
        },
    }))
}

fn project_json(project: &ResolvedProject) -> Value {
    let absolute = |folder: &Path| -> PathBuf {
        if folder.is_absolute() {
            folder.to_path_buf()
        } else {
            project.root.join(folder)
        }
    };
    json!({
        "name": project.manifest.name,
        "display_name": project.display_name(),
        "kind": project.manifest.kind.name(),
        "root": text(&project.root),
        "manifest": text(&project.manifest_path),
        "entry": text(&project.entry),
        "assets_dir": text(&absolute(&project.manifest.assets_dir)),
        "output_dir": text(&absolute(&project.manifest.output_dir)),
        "language": project.manifest.language,
    })
}

fn workspace_json(workspace: &Workspace) -> Value {
    let members: Vec<Value> = workspace
        .members()
        .iter()
        .map(|member| {
            let project = member.project.as_ref().ok();
            json!({
                "path": member.path,
                "name": member.name(),
                "group": member.group,
                "root": text(&member.root),
                "kind": project.map(|project| project.manifest.kind.name()),
                "entry": project.map(|project| text(&project.entry)),
                "error": member.project.as_ref().err(),
            })
        })
        .collect();
    json!({
        "name": workspace.name,
        "root": text(&workspace.root),
        "manifest": text(&workspace.manifest_path),
        "cover": workspace.cover.as_deref().map(text),
        "python": workspace.python_roots.iter().map(|path| text(path)).collect::<Vec<_>>(),
        "assets": workspace.asset_roots.iter().map(|path| text(path)).collect::<Vec<_>>(),
        "members": members,
    })
}

impl Location {
    /// The folders a script in `script_folder` finds on `sys.path`, first
    /// to last: its project's `src/` and folder (the script's folder
    /// outside a project), then its workspace's shared Python folders and
    /// root. The script's own folder comes before the project's.
    pub fn python_paths(&self, script_folder: &Path) -> Vec<PathBuf> {
        let root = self
            .project
            .as_ref()
            .map(|project| project.root.clone())
            .unwrap_or_else(|| script_folder.to_path_buf());
        let mut paths = Vec::new();
        let source = root.join("src");
        if source.is_dir() {
            paths.push(source);
        }
        paths.push(script_folder.to_path_buf());
        paths.push(root);
        if let Some(workspace) = &self.workspace {
            paths.extend(workspace.python_roots.iter().cloned());
            paths.push(workspace.root.clone());
        }
        let mut seen = Vec::new();
        paths.retain(|path| {
            let key = plain_path(path);
            let fresh = !seen.contains(&key);
            seen.push(key);
            fresh
        });
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_member_file_tells_its_project_workspace_and_python() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let write = |relative: &str, text: &str| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "gaanim.toml",
            "name = \"channel\"\n[workspace]\nmembers = [\"videos/*\"]\n",
        );
        write("src/channel/__init__.py", "");
        write(
            "videos/intro/gaanim.toml",
            "kind = \"video\"\nentry = \"main.py\"\n",
        );
        write("videos/intro/main.py", "");
        write("videos/intro/scenes/title.py", "");
        let scripts = root
            .join(".venv")
            .join(if cfg!(windows) { "Scripts" } else { "bin" });
        std::fs::create_dir_all(&scripts).unwrap();
        let interpreter = scripts.join(if cfg!(windows) {
            "python.exe"
        } else {
            "python3"
        });
        std::fs::write(&interpreter, "").unwrap();

        let described = describe(&root.join("videos/intro/scenes/title.py")).unwrap();
        assert_eq!(described["project"]["name"], "intro");
        assert_eq!(described["project"]["display_name"], "channel › intro");
        assert_eq!(described["project"]["kind"], "video");
        assert_eq!(described["workspace"]["name"], "channel");
        assert_eq!(described["workspace"]["members"][0]["path"], "videos/intro");
        assert_eq!(described["workspace"]["members"][0]["error"], Value::Null);
        // The one environment at the workspace's root.
        assert_eq!(described["python"]["venv"], text(&root.join(".venv")));
        assert_eq!(described["python"]["interpreter"], text(&interpreter));
        let paths: Vec<&str> = described["python"]["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| path.as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            [
                text(&root.join("videos/intro/scenes")),
                text(&root.join("videos/intro")),
                text(&root.join("src")),
                text(&root),
            ]
        );
        assert!(paths.iter().all(|path| !path.starts_with(r"\\?\")));

        // The workspace folder itself has no project.
        let workspace = describe(&root).unwrap();
        assert_eq!(workspace["project"], Value::Null);
        assert_eq!(workspace["workspace"]["name"], "channel");
        // A project folder runs its entry.
        let folder = describe(&root.join("videos/intro")).unwrap();
        assert_eq!(
            folder["python"]["paths"][0],
            text(&root.join("videos/intro"))
        );
        assert!(describe(&root.join("missing.py")).is_err());
    }
}
