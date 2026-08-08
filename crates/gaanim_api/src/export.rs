//! Canonical export helpers for `gaanim_api` scenes.
//!
//! Language bindings should construct a [`Canvas`](crate::canvas::Canvas) and
//! call these helpers instead of duplicating Bevy/world setup.

use std::collections::HashSet;

use gaanim_export::prelude::{ExportError, export_scene, export_scene_direct};

use crate::canvas::Canvas;
use crate::runtime::replay_canvas_into;

pub use gaanim_export::encoder::{EncodingSpeed, VideoEncoder, detect_best_encoder};
pub use gaanim_export::prelude::{
    AspectRatioPreset, AudioTrack, AudioTrackError, ExportConfig, ExportFormat, QualityPreset,
};

#[derive(Debug, thiserror::Error)]
pub enum SlideExportError {
    #[error("the canvas does not define any semantic slides")]
    NoSlides,
    #[error("unknown slide '{0}'")]
    UnknownSlide(String),
    #[error("slide '{name}' has an invalid or empty time range ({start:.3}s..{end:.3}s)")]
    InvalidRange { name: String, start: f64, end: f64 },
    #[error("exporting every slide requires '{{slide}}' or '{{index}}' in the output path")]
    MissingPathPlaceholder,
    #[error("the output template resolves more than one slide to '{0}'")]
    DuplicateOutput(String),
    #[error("could not create an export directory: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Export(#[from] ExportError),
}

/// Export a Canvas using the supplied `gaanim_export::ExportConfig`.
pub fn export_canvas(canvas: Canvas, mut config: ExportConfig) -> Result<(), ExportError> {
    config.audio_tracks.extend(canvas.audio_tracks.clone());
    if config.headless && !canvas.has_native_3d_content() {
        export_scene_direct(config, move |world| replay_canvas_into(world, canvas))
    } else {
        export_scene(config, move |world| replay_canvas_into(world, canvas))
    }
}

/// Export one semantic slide while preserving all other export settings.
pub fn export_canvas_slide(
    canvas: Canvas,
    slide_name: &str,
    mut config: ExportConfig,
) -> Result<(), SlideExportError> {
    let manifest = canvas.presentation_manifest();
    let slide = manifest
        .slides
        .iter()
        .find(|slide| slide.name == slide_name)
        .or_else(|| {
            manifest
                .slides
                .iter()
                .find(|slide| slide.name.eq_ignore_ascii_case(slide_name))
        })
        .ok_or_else(|| SlideExportError::UnknownSlide(slide_name.to_string()))?;
    apply_slide_range(&mut config, slide)?;
    create_output_parent(&config.output_path)?;
    export_canvas(canvas, config)?;
    Ok(())
}

/// Export every semantic slide using one output-path template.
///
/// `{slide}` expands to a filesystem-safe slide name and `{index}` to a
/// one-based, zero-padded index. At least one placeholder is required so
/// multiple slides can never silently overwrite the same file.
pub fn export_canvas_slides(
    canvas: Canvas,
    output_template: &str,
    config: ExportConfig,
) -> Result<Vec<String>, SlideExportError> {
    if !output_template.contains("{slide}") && !output_template.contains("{index}") {
        return Err(SlideExportError::MissingPathPlaceholder);
    }
    let manifest = canvas.presentation_manifest();
    if manifest.slides.is_empty() {
        return Err(SlideExportError::NoSlides);
    }

    let mut outputs = Vec::with_capacity(manifest.slides.len());
    let mut unique_outputs = HashSet::with_capacity(manifest.slides.len());
    for (index, slide) in manifest.slides.iter().enumerate() {
        let output = output_template
            .replace("{slide}", &safe_slide_filename(&slide.name))
            .replace("{index}", &format!("{:02}", index + 1));
        if !unique_outputs.insert(output.clone()) {
            return Err(SlideExportError::DuplicateOutput(output));
        }
        create_output_parent(&output)?;
        let mut slide_config = config.clone();
        slide_config.output_path = output.clone();
        apply_slide_range(&mut slide_config, slide)?;
        export_canvas(canvas.clone(), slide_config)?;
        outputs.push(output);
    }
    Ok(outputs)
}

fn create_output_parent(output: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = std::path::Path::new(output).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn apply_slide_range(
    config: &mut ExportConfig,
    slide: &crate::canvas::SlideSpec,
) -> Result<(), SlideExportError> {
    let start = slide.start_time;
    let end = slide.end_time.unwrap_or(start);
    if !start.is_finite() || !end.is_finite() || end <= start {
        return Err(SlideExportError::InvalidRange {
            name: slide.name.clone(),
            start,
            end,
        });
    }
    config.start_time = Some(start);
    config.end_time = Some(end);
    Ok(())
}

fn safe_slide_filename(name: &str) -> String {
    let mut filename = String::with_capacity(name.len());
    let mut previous_separator = false;
    for character in name.chars() {
        if character.is_alphanumeric() || matches!(character, '-' | '_') {
            filename.extend(character.to_lowercase());
            previous_separator = false;
        } else if !previous_separator && !filename.is_empty() {
            filename.push('-');
            previous_separator = true;
        }
    }
    while filename.ends_with('-') {
        filename.pop();
    }
    if filename.is_empty() {
        "slide".to_string()
    } else {
        filename
    }
}

/// Convenience helper for the common headless export path.
pub fn export_canvas_to_path(
    canvas: Canvas,
    output_path: &str,
    fps: Option<u32>,
    transparent: Option<bool>,
) -> Result<(), ExportError> {
    let mut config = ExportConfig::new(output_path);
    config.width = canvas.width;
    config.height = canvas.height;
    config.aspect_ratio = AspectRatioPreset::Custom;
    config.headless = true;
    if let Some(fps) = fps {
        config.fps = fps;
    }
    if let Some(transparent) = transparent {
        config.transparent = transparent;
    }
    export_canvas(canvas, config)
}

#[cfg(test)]
mod tests {
    use super::safe_slide_filename;

    #[test]
    fn slide_names_become_portable_filenames() {
        assert_eq!(
            safe_slide_filename("Marco teórico / 2026"),
            "marco-teórico-2026"
        );
        assert_eq!(safe_slide_filename("  Resultados: A+B  "), "resultados-a-b");
        assert_eq!(safe_slide_filename("///"), "slide");
    }
}
