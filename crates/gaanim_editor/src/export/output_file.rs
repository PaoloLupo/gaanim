//! The export dialog's file: a folder and a name, which the format completes
//! with its extension, or for a PNG sequence with a folder of frames.

use std::path::{Path, PathBuf};

use gaanim_export::prelude::ExportFormat;

/// File name of the frames of a PNG sequence, before their numbers.
pub(super) const PNG_SEQUENCE_STEM: &str = "frame";

/// What an export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputKind {
    /// One file with this extension.
    File(&'static str),
    /// A folder of numbered PNG frames.
    Frames,
}

impl OutputKind {
    pub(super) fn of(format: ExportFormat, bundle: bool) -> Self {
        if bundle {
            return Self::File(gaanim_bundle::EXTENSION);
        }
        match format {
            ExportFormat::PngSequence => Self::Frames,
            format => Self::File(super::export_format_arg(format)),
        }
    }

    /// What the format adds after the name, as the dialog shows it.
    pub(super) fn suffix(self) -> String {
        match self {
            Self::File(extension) => format!(".{extension}"),
            Self::Frames => format!("/{PNG_SEQUENCE_STEM}_00000.png…"),
        }
    }
}

/// The folder and the name an output path is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OutputParts {
    /// Relative to the project, or absolute; empty for the project itself.
    pub folder: String,
    /// Without the extension; for a PNG sequence, its folder's name.
    pub name: String,
}

/// `path` as a folder and a name, whatever format wrote it: a PNG
/// sequence's `name/frame.png` names its folder, any other path its file.
pub(super) fn split(path: &str) -> OutputParts {
    let path = Path::new(path);
    let frames = path.file_name().and_then(|name| name.to_str())
        == Some(&format!("{PNG_SEQUENCE_STEM}.png") as &str);
    let (named, name) = if frames {
        let folder = path.parent().unwrap_or(Path::new(""));
        (folder, folder.file_name())
    } else {
        (path, path.file_stem())
    };
    OutputParts {
        folder: named
            .parent()
            .map(|folder| folder.to_string_lossy().into_owned())
            .unwrap_or_default(),
        name: name
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

/// The output path `parts` name for `kind`.
pub(super) fn join(parts: &OutputParts, kind: OutputKind) -> String {
    let folder = Path::new(&parts.folder);
    let path = match kind {
        OutputKind::File(extension) => folder.join(format!("{}.{extension}", parts.name)),
        OutputKind::Frames => folder
            .join(&parts.name)
            .join(format!("{PNG_SEQUENCE_STEM}.png")),
    };
    path.to_string_lossy().into_owned()
}

/// `name` without the characters a file name cannot hold on some system;
/// folder separators become dashes so the name stays one file.
pub(super) fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            ch if ch.is_control() => '-',
            ch => ch,
        })
        .collect()
}

/// The name an export starts with: the script's, or the project's when the
/// script is its `main.py`; a bundle's own name; `output` otherwise.
pub(super) fn default_name(project: Option<(&Path, &Path)>, bundle: Option<&Path>) -> String {
    let stem = |path: &Path| {
        path.file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
    };
    let name = match (project, bundle) {
        (_, Some(bundle)) => stem(bundle),
        (Some((project_dir, script)), None) => match stem(script) {
            Some(stem) if stem == "main" || stem == "__main__" => project_dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            stem => stem,
        },
        (None, None) => None,
    };
    let name = sanitize_name(name.as_deref().unwrap_or("").trim());
    if name.is_empty() {
        "output".to_owned()
    } else {
        name
    }
}

/// Whether exporting to `path` (resolved) would replace what is there.
pub(super) fn exists(path: &Path, kind: OutputKind) -> bool {
    match kind {
        OutputKind::File(_) => path.exists(),
        OutputKind::Frames => path
            .parent()
            .and_then(|folder| std::fs::read_dir(folder).ok())
            .is_some_and(|mut entries| {
                entries.any(|entry| {
                    entry.is_ok_and(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with(&format!("{PNG_SEQUENCE_STEM}_"))
                    })
                })
            }),
    }
}

/// The first of `name-2`, `name-3`… that nothing in `folder` (resolved)
/// uses for `kind`.
pub(super) fn free_name(folder: &Path, name: &str, kind: OutputKind) -> String {
    let base = name.trim_end_matches(|ch: char| ch.is_ascii_digit());
    let base = base
        .strip_suffix('-')
        .filter(|_| base.len() < name.len())
        .unwrap_or(name);
    (2..)
        .map(|copy| format!("{base}-{copy}"))
        .find(|candidate| {
            let parts = OutputParts {
                folder: folder.to_string_lossy().into_owned(),
                name: candidate.clone(),
            };
            !exists(&PathBuf::from(join(&parts, kind)), kind)
        })
        .expect("some numbered name is free")
}

/// `folder` as the dialog shows it: relative to the project when inside it.
pub(super) fn relative_folder(folder: &Path, project_dir: Option<&Path>) -> String {
    project_dir
        .and_then(|project| folder.strip_prefix(project).ok())
        .map(|relative| relative.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MP4: OutputKind = OutputKind::File("mp4");

    #[test]
    fn a_path_splits_into_folder_and_name_and_back() {
        // Compared as paths: Windows joins with backslashes.
        let path = |text: String| PathBuf::from(text);
        let parts = split("exports/intro.mp4");
        assert_eq!(Path::new(&parts.folder), Path::new("exports"));
        assert_eq!(parts.name, "intro");
        assert_eq!(path(join(&parts, MP4)), Path::new("exports/intro.mp4"));
        assert_eq!(
            path(join(&parts, OutputKind::Frames)),
            Path::new("exports/intro/frame.png")
        );
        assert_eq!(
            path(join(&parts, OutputKind::File("gif"))),
            Path::new("exports/intro.gif")
        );

        let frames = split("exports/intro/frame.png");
        assert_eq!(Path::new(&frames.folder), Path::new("exports"));
        assert_eq!(frames.name, "intro");
        let bare = split("intro.mp4");
        assert_eq!((bare.folder.as_str(), bare.name.as_str()), ("", "intro"));
        assert_eq!(join(&bare, MP4), "intro.mp4");

        // Switching formats keeps the name, whichever the path came from.
        for path in [
            "exports/intro.mp4",
            "exports/intro/frame.png",
            "exports/intro.gaanim",
        ] {
            let parts = split(path);
            assert_eq!(parts.name, "intro", "{path}");
            assert_eq!(
                PathBuf::from(join(&parts, OutputKind::Frames)),
                Path::new("exports/intro/frame.png")
            );
        }
        // An MP4 named after the frames is still a file.
        assert_eq!(split("exports/frame.mp4").name, "frame");
    }

    #[test]
    fn names_lose_what_a_file_name_cannot_hold() {
        assert_eq!(
            sanitize_name("capítulo 1/intro: v2?"),
            "capítulo 1-intro- v2-"
        );
        assert_eq!(sanitize_name("ok-name_3"), "ok-name_3");
    }

    #[test]
    fn exports_are_named_after_the_project_script_or_bundle() {
        let project = Path::new("/work/mi-charla");
        assert_eq!(
            default_name(Some((project, &project.join("main.py"))), None),
            "mi-charla"
        );
        assert_eq!(
            default_name(Some((project, &project.join("escenas/intro.py"))), None),
            "intro"
        );
        assert_eq!(
            default_name(None, Some(Path::new("/descargas/demo.gaanim"))),
            "demo"
        );
        assert_eq!(default_name(None, None), "output");
    }

    #[test]
    fn a_free_name_numbers_after_what_exists() {
        let folder = std::env::temp_dir().join(format!("gaanim-output-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("intro.mp4"), b"").unwrap();
        std::fs::write(folder.join("intro-2.mp4"), b"").unwrap();
        assert!(exists(&folder.join("intro.mp4"), MP4));
        assert_eq!(free_name(&folder, "intro", MP4), "intro-3");
        assert_eq!(free_name(&folder, "intro-2", MP4), "intro-3");
        assert_eq!(free_name(&folder, "v1", MP4), "v1-2");

        std::fs::create_dir_all(folder.join("intro")).unwrap();
        std::fs::write(folder.join("intro/frame_00000.png"), b"").unwrap();
        assert!(exists(&folder.join("intro/frame.png"), OutputKind::Frames));
        assert!(!exists(&folder.join("otro/frame.png"), OutputKind::Frames));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn folders_show_relative_to_the_project() {
        let project = Path::new("/work/charla");
        assert_eq!(
            relative_folder(&project.join("exports"), Some(project)),
            "exports"
        );
        assert_eq!(
            relative_folder(Path::new("/videos"), Some(project)),
            "/videos"
        );
    }
}
