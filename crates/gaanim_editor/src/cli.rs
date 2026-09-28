//! `gaanim export` arguments, shared by `gaanim-core` and `gaanim-play`.
//!
//! Scripts and projects export through the Python host; playback bundles
//! (`.gaanim`) export through [`export_bundle_video`] without Python.

use std::path::{Path, PathBuf};

use gaanim_core::console;
use gaanim_export::encoder::{ExportFormat, VideoEncoder};
use gaanim_export::prelude::{AspectRatioPreset, ExportConfig, OutputFit, QualityPreset};

/// One end of an export range: seconds, or a `scene.marker` name resolved
/// once the markers are known.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportBound {
    Seconds(f64),
    Marker(String),
}

impl std::fmt::Display for ExportBound {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Seconds(seconds) => write!(formatter, "{seconds}"),
            Self::Marker(name) => write!(formatter, "{name}"),
        }
    }
}

/// `--from` / `--to`: finite non-negative seconds, or a marker name.
pub fn parse_export_seconds(flag: &str, value: Option<&String>) -> Result<ExportBound, String> {
    let value = value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{flag} requires seconds or a marker name"))?;
    match value.parse::<f64>() {
        Ok(seconds) if seconds.is_finite() && seconds >= 0.0 => Ok(ExportBound::Seconds(seconds)),
        Ok(_) => Err(format!(
            "{flag} requires a non-negative number of seconds or a marker name"
        )),
        Err(_) if value.starts_with('-') => Err(format!(
            "{flag} requires a non-negative number of seconds or a marker name"
        )),
        Err(_) => Ok(ExportBound::Marker(value.to_string())),
    }
}

pub fn validate_export_range(
    from: Option<&ExportBound>,
    to: Option<&ExportBound>,
) -> Result<(), String> {
    match (from, to) {
        (Some(ExportBound::Seconds(from)), Some(ExportBound::Seconds(to))) if to <= from => {
            Err(format!("--to ({to}) must be greater than --from ({from})"))
        }
        _ => Ok(()),
    }
}

/// Resolve marker bounds against the authored `(name, time)` markers.
pub fn resolve_export_bound(
    flag: &str,
    bound: Option<&ExportBound>,
    markers: &[(&str, f64)],
) -> Result<Option<f64>, String> {
    match bound {
        None => Ok(None),
        Some(ExportBound::Seconds(seconds)) => Ok(Some(*seconds)),
        Some(ExportBound::Marker(name)) => markers
            .iter()
            .find(|(marker, _)| marker == name)
            .map(|(_, time)| Some(*time))
            .ok_or_else(|| {
                let known = markers
                    .iter()
                    .map(|(marker, _)| format!("{marker:?}"))
                    .collect::<Vec<_>>();
                format!(
                    "{flag}: unknown marker {name:?}; the scene defines {}",
                    if known.is_empty() {
                        "no markers (use scene.marker(\"name\"))".to_string()
                    } else {
                        known.join(", ")
                    }
                )
            }),
    }
}

/// Resolve both ends of an export range and check their order.
pub fn resolve_export_range(
    from: Option<&ExportBound>,
    to: Option<&ExportBound>,
    markers: &[(&str, f64)],
) -> Result<(Option<f64>, Option<f64>), String> {
    let start = resolve_export_bound("--from", from, markers)?;
    let end = resolve_export_bound("--to", to, markers)?;
    if let (Some(start), Some(end)) = (start, end)
        && end <= start
    {
        return Err(format!(
            "--to ({}) resolves to {end}s, which must be after --from ({}) at {start}s",
            to.map(ToString::to_string).unwrap_or_default(),
            from.map(ToString::to_string).unwrap_or_default(),
        ));
    }
    Ok((start, end))
}

/// Output formats `gaanim export` writes, by file extension.
pub const EXPORT_EXTENSIONS: [&str; 6] = ["mp4", "webm", "webp", "gif", "png", "gaanim"];

/// `gaanim export <INPUT> --output <FILE> [options]`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportCommand {
    /// Script, project directory, or playback bundle.
    pub input: Option<PathBuf>,
    pub output: Option<String>,
    pub quality: String,
    pub encoder: VideoEncoder,
    pub transparent: bool,
    pub width: u32,
    pub height: u32,
    pub fit: OutputFit,
    pub from: Option<ExportBound>,
    pub to: Option<ExportBound>,
    /// Recording rate of a playback bundle.
    pub fps: Option<u32>,
}

impl Default for ExportCommand {
    fn default() -> Self {
        Self {
            input: None,
            output: None,
            quality: "standard".to_string(),
            encoder: VideoEncoder::Auto,
            transparent: false,
            width: 1920,
            height: 1080,
            fit: OutputFit::Error,
            from: None,
            to: None,
            fps: None,
        }
    }
}

impl ExportCommand {
    /// Parse the arguments that follow `export`.
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut command = Self::default();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--fps" => {
                    index += 1;
                    command.fps = Some(
                        args.get(index)
                            .and_then(|value| value.parse::<u32>().ok())
                            .filter(|value| (1..=240).contains(value))
                            .ok_or_else(|| "--fps requires an integer from 1 to 240".to_string())?,
                    );
                }
                flag @ ("--from" | "--to") => {
                    index += 1;
                    let bound = parse_export_seconds(flag, args.get(index))?;
                    if flag == "--from" {
                        command.from = Some(bound);
                    } else {
                        command.to = Some(bound);
                    }
                }
                "--output" | "-o" => {
                    index += 1;
                    command.output = args.get(index).cloned();
                }
                "--quality" => {
                    index += 1;
                    command.quality = args.get(index).cloned().unwrap_or_default();
                }
                "--encoder" => {
                    index += 1;
                    command.encoder = args
                        .get(index)
                        .and_then(|value| VideoEncoder::parse_arg(value))
                        .ok_or_else(|| {
                            format!("encoder must be {}", VideoEncoder::ARG_VALUES.join(", "))
                        })?;
                }
                "--transparent" => command.transparent = true,
                flag @ ("--width" | "--height") => {
                    index += 1;
                    let value = args
                        .get(index)
                        .and_then(|value| value.parse().ok())
                        .filter(|value| *value > 0)
                        .ok_or_else(|| format!("{flag} requires a positive integer"))?;
                    if flag == "--width" {
                        command.width = value;
                    } else {
                        command.height = value;
                    }
                }
                "--fit" => {
                    index += 1;
                    command.fit = match args.get(index).map(String::as_str) {
                        Some("error") => OutputFit::Error,
                        Some("contain") => OutputFit::Contain,
                        Some("cover") => OutputFit::Cover,
                        _ => return Err("--fit must be error, contain, or cover".to_string()),
                    };
                }
                value if value.starts_with('-') => {
                    return Err(format!("unknown option `{value}`"));
                }
                value if command.input.is_none() => command.input = Some(PathBuf::from(value)),
                value => return Err(format!("unexpected argument `{value}`")),
            }
            index += 1;
        }
        if !matches!(
            command.quality.as_str(),
            "draft" | "standard" | "production"
        ) {
            return Err("quality must be draft, standard, or production".to_string());
        }
        Ok(command)
    }

    /// The output path and its lowercase format extension.
    pub fn output_format(&self) -> Result<(String, String), String> {
        let output = self
            .output
            .clone()
            .ok_or_else(|| "--output is required".to_string())?;
        let format = Path::new(&output)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .filter(|format| EXPORT_EXTENSIONS.contains(&format.as_str()))
            .ok_or_else(|| {
                "output extension must be mp4, webm, webp, gif, png, or gaanim".to_string()
            })?;
        Ok((output, format))
    }

    /// Check the options that apply to a video `format`.
    pub fn validate_video(&self, format: &str) -> Result<(), String> {
        if self.fps.is_some() {
            return Err("--fps applies to playback bundles (.gaanim)".to_string());
        }
        if self.transparent && !matches!(format, "webm" | "webp" | "png") {
            return Err("--transparent requires WebM, WebP, or PNG output".to_string());
        }
        if format != "mp4" && self.encoder != VideoEncoder::Auto {
            return Err("--encoder requires MP4 output".to_string());
        }
        validate_export_range(self.from.as_ref(), self.to.as_ref())
    }
}

pub fn quality_preset(quality: &str) -> QualityPreset {
    match quality {
        "draft" => QualityPreset::Draft,
        "production" => QualityPreset::Production,
        _ => QualityPreset::Standard,
    }
}

pub fn export_format(format: &str) -> Option<ExportFormat> {
    Some(match format {
        "mp4" => ExportFormat::Mp4,
        "webm" => ExportFormat::Webm,
        "webp" => ExportFormat::Webp,
        "gif" => ExportFormat::Gif,
        "png" => ExportFormat::PngSequence,
        _ => return None,
    })
}

/// The export configuration a video export of `command` uses.
pub fn video_config(
    command: &ExportCommand,
    output: &str,
    format: ExportFormat,
    range: (Option<f64>, Option<f64>),
) -> ExportConfig {
    let mut config = ExportConfig::new(output).with_quality(quality_preset(&command.quality));
    config.width = command.width;
    config.height = command.height;
    config.fit = command.fit;
    config.start_time = range.0;
    config.end_time = range.1;
    config.aspect_ratio = AspectRatioPreset::Custom;
    config.format = format;
    config.video_encoder = command.encoder;
    config.transparent = command.transparent;
    config.headless = true;
    config
}

/// Frames per second a bundle records for an export `quality`: the rate a
/// video of that quality has.
pub fn bundle_fps(quality: &str) -> u32 {
    match quality {
        "draft" => 30,
        _ => 60,
    }
}

/// The recording of `canvas` into a bundle at `output`, named after the
/// project `script` belongs to (or the script) and sized like the scene's
/// preview.
pub fn bundle_config(
    script: Option<&Path>,
    canvas: &gaanim_api::canvas::SceneModel,
    output: &str,
    fps: Option<u32>,
) -> gaanim_api::export::BundleConfig {
    let mut config = gaanim_api::export::BundleConfig::new(output);
    if let Some(title) = script.and_then(bundle_title) {
        config.title = title;
    }
    if let Some(fps) = fps {
        config.fps = fps;
    }
    (config.width, config.height) = canvas.frame.preview_pixel_size();
    // Verification switch: record a second world regardless of the scene, to
    // compare it with the single-world recording.
    config.force_second_world = std::env::var_os("GAANIM_BUNDLE_SECOND_WORLD").is_some();
    config
}

/// The title a bundle recorded from `script` carries: its project's name,
/// which the player shows in the window title, or the script's name outside
/// a project. Every project's entry is `main.py`, so its name says nothing.
fn bundle_title(script: &Path) -> Option<String> {
    let project = gaanim_project::find_project_for_script(script)
        .map(|project| project.manifest.name.trim().to_owned())
        .filter(|name| !name.is_empty());
    project.or_else(|| {
        script
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
    })
}

/// Why a command stopped: bad arguments (exit status 2) or a failed run (1).
#[derive(Debug, Clone, PartialEq)]
pub enum CommandError {
    Usage(String),
    Failed(String),
}

impl CommandError {
    /// Report the error and exit with its status.
    pub fn exit(self, scope: &str) -> ! {
        let (status, message) = match self {
            Self::Usage(message) => (2, message),
            Self::Failed(message) => (1, message),
        };
        console::error(scope, message);
        std::process::exit(status);
    }
}

/// Export a playback bundle to video, without Python: every frame is
/// composed from the recorded frame shown at that instant.
pub fn export_bundle_video(command: &ExportCommand) -> Result<(), CommandError> {
    let input = command
        .input
        .as_deref()
        .ok_or_else(|| CommandError::Usage("a bundle to export is required".to_string()))?;
    let (output, format) = command.output_format().map_err(CommandError::Usage)?;
    if format == gaanim_bundle::EXTENSION {
        return Err(CommandError::Usage(
            "the input is already a playback bundle; export it to mp4, webm, webp, gif, or png"
                .to_string(),
        ));
    }
    command
        .validate_video(&format)
        .map_err(CommandError::Usage)?;
    let bundle = gaanim_bundle::Bundle::open(input)
        .map_err(|error| CommandError::Failed(format!("{}: {error}", input.display())))?;
    let markers: Vec<(&str, f64)> = bundle
        .scene
        .markers
        .iter()
        .map(|marker| (marker.name.as_str(), marker.time))
        .collect();
    let range = resolve_export_range(command.from.as_ref(), command.to.as_ref(), &markers)
        .map_err(CommandError::Usage)?;
    let format = export_format(&format).expect("validated export format");
    drop(bundle);
    // The video runs at the bundle's recorded rate.
    gaanim_export::prelude::export_bundle(input, video_config(command, &output, format, range))
        .map_err(|error| CommandError::Failed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn bundles_are_titled_after_their_project() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project = gaanim_project::create_project(&gaanim_project::CreateProjectOptions {
            kind: gaanim_project::ProjectKind::Slides,
            directory: temp.path().join("tesis"),
            force: false,
        })
        .expect("project");
        assert_eq!(bundle_title(&project.entry).as_deref(), Some("tesis"));
        let loose = temp.path().join("demo.py");
        assert_eq!(bundle_title(&loose).as_deref(), Some("demo"));
    }

    #[test]
    fn export_bounds_resolve_scene_markers() {
        let markers = [("climax", 2.5)];
        let climax = ExportBound::Marker("climax".into());
        assert_eq!(
            resolve_export_bound("--from", Some(&climax), &markers),
            Ok(Some(2.5))
        );
        assert_eq!(
            resolve_export_bound("--to", Some(&ExportBound::Seconds(4.0)), &markers),
            Ok(Some(4.0))
        );
        let error =
            resolve_export_bound("--to", Some(&ExportBound::Marker("fin".into())), &markers)
                .unwrap_err();
        assert!(error.contains("unknown marker \"fin\"") && error.contains("\"climax\""));
        // Marker ranges are only ordered once the markers are known.
        assert!(validate_export_range(Some(&climax), Some(&ExportBound::Seconds(0.1))).is_ok());
        assert!(
            resolve_export_range(Some(&climax), Some(&ExportBound::Seconds(0.1)), &markers)
                .is_err()
        );
    }

    #[test]
    fn parses_export_commands() {
        let command = ExportCommand::parse(&args(&[
            "deck.gaanim",
            "-o",
            "talk.mp4",
            "--quality",
            "draft",
            "--from",
            "intro",
            "--to",
            "12",
            "--width",
            "1280",
            "--height",
            "720",
        ]))
        .unwrap();
        assert_eq!(command.input, Some(PathBuf::from("deck.gaanim")));
        assert_eq!(
            command.output_format().unwrap(),
            ("talk.mp4".to_string(), "mp4".to_string())
        );
        assert_eq!(command.from, Some(ExportBound::Marker("intro".into())));
        assert_eq!(command.to, Some(ExportBound::Seconds(12.0)));
        assert_eq!((command.width, command.height), (1280, 720));
        assert!(command.validate_video("mp4").is_ok());

        let bundle =
            ExportCommand::parse(&args(&["scene.py", "-o", "x.gaanim", "--fps", "30"])).unwrap();
        assert_eq!(bundle.fps, Some(30));
        assert!(bundle.validate_video("mp4").is_err());

        for invalid in [
            &["scene.py", "--fps", "0"][..],
            &["scene.py", "--quality", "ultra"],
            &["scene.py", "--fit", "stretch"],
            &["scene.py", "--from", "-1"],
            &["scene.py", "other.py"],
            &["scene.py", "--frobnicate"],
        ] {
            assert!(ExportCommand::parse(&args(invalid)).is_err(), "{invalid:?}");
        }
        let avi = ExportCommand::parse(&args(&["scene.py", "-o", "x.avi"])).unwrap();
        assert!(avi.output_format().is_err());
        let transparent =
            ExportCommand::parse(&args(&["scene.py", "-o", "x.mp4", "--transparent"])).unwrap();
        assert!(transparent.validate_video("mp4").is_err());
    }
}
