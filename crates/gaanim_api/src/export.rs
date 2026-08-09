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
pub enum SegmentExportError {
    #[error("the canvas does not define any segments")]
    NoSegments,
    #[error("unknown segment '{0}'")]
    UnknownSegment(String),
    #[error("segment '{name}' has an invalid or empty time range ({start:.3}s..{end:.3}s)")]
    InvalidRange { name: String, start: f64, end: f64 },
    #[error("exporting every segment requires '{{segment}}' or '{{index}}' in the output path")]
    MissingPathPlaceholder,
    #[error("the output template resolves more than one segment to '{0}'")]
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

/// Export one segment while preserving all other export settings.
pub fn export_canvas_segment(
    canvas: Canvas,
    segment_name: &str,
    mut config: ExportConfig,
) -> Result<(), SegmentExportError> {
    let manifest = canvas.segment_manifest();
    let normalized_name = segment_name.to_lowercase();
    let segment = manifest
        .segments
        .iter()
        .find(|segment| segment.name == segment_name)
        .or_else(|| {
            manifest
                .segments
                .iter()
                .find(|segment| segment.name.to_lowercase() == normalized_name)
        })
        .ok_or_else(|| SegmentExportError::UnknownSegment(segment_name.to_string()))?;
    apply_segment_range(&mut config, segment)?;
    create_output_parent(&config.output_path)?;
    export_canvas(canvas, config)?;
    Ok(())
}

/// Export every segment using one output-path template.
///
/// `{segment}` expands to a filesystem-safe segment name and `{index}` to a
/// one-based, zero-padded index.
pub fn export_canvas_segments(
    canvas: Canvas,
    output_template: &str,
    config: ExportConfig,
) -> Result<Vec<String>, SegmentExportError> {
    if !output_template.contains("{segment}") && !output_template.contains("{index}") {
        return Err(SegmentExportError::MissingPathPlaceholder);
    }
    let manifest = canvas.segment_manifest();
    if manifest.segments.is_empty() {
        return Err(SegmentExportError::NoSegments);
    }

    let mut jobs = Vec::with_capacity(manifest.segments.len());
    let mut unique_outputs = HashSet::with_capacity(manifest.segments.len());
    for (index, segment) in manifest.segments.iter().enumerate() {
        let output = segment_output_path(output_template, index, &segment.name);
        if !unique_outputs.insert(output.clone()) {
            return Err(SegmentExportError::DuplicateOutput(output));
        }
        let mut segment_config = config.clone();
        segment_config.output_path = output.clone();
        apply_segment_range(&mut segment_config, segment)?;
        jobs.push((output, segment_config));
    }

    let mut outputs = Vec::with_capacity(jobs.len());
    for (output, segment_config) in jobs {
        create_output_parent(&output)?;
        export_canvas(canvas.clone(), segment_config)?;
        outputs.push(output);
    }
    Ok(outputs)
}

fn segment_output_path(template: &str, index: usize, name: &str) -> String {
    template
        .replace("{segment}", &safe_segment_filename(name))
        .replace("{index}", &format!("{:02}", index + 1))
}

fn create_output_parent(output: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = std::path::Path::new(output).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn apply_segment_range(
    config: &mut ExportConfig,
    segment: &crate::canvas::SegmentSpec,
) -> Result<(), SegmentExportError> {
    let start = segment.start_time;
    let end = segment.end_time;
    if !start.is_finite() || !end.is_finite() || end <= start {
        return Err(SegmentExportError::InvalidRange {
            name: segment.name.clone(),
            start,
            end,
        });
    }
    config.start_time = Some(start);
    config.end_time = Some(end);
    Ok(())
}

fn safe_segment_filename(name: &str) -> String {
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
        "segment".to_string()
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
    use super::{
        ExportConfig, SegmentExportError, apply_segment_range, export_canvas_segment,
        export_canvas_segments, safe_segment_filename, segment_output_path,
    };
    use crate::canvas::Canvas;

    #[test]
    fn segment_names_become_portable_filenames() {
        assert_eq!(
            safe_segment_filename("Marco teórico / 2026"),
            "marco-teórico-2026"
        );
        assert_eq!(
            safe_segment_filename("  Resultados: A+B  "),
            "resultados-a-b"
        );
        assert_eq!(safe_segment_filename("///"), "segment");
        assert_eq!(
            segment_output_path("slides/{index}-{segment}.mp4", 2, "Marco teórico"),
            "slides/03-marco-teórico.mp4"
        );
    }

    #[test]
    fn segment_export_validation_happens_before_rendering() {
        let canvas = Canvas::new(640, 360);
        assert!(matches!(
            export_canvas_segment(
                canvas.clone(),
                "missing",
                ExportConfig::new("missing.mp4")
            ),
            Err(SegmentExportError::UnknownSegment(name)) if name == "missing"
        ));
        assert!(matches!(
            export_canvas_segment(canvas.clone(), "_default", ExportConfig::new("empty.mp4")),
            Err(SegmentExportError::InvalidRange { .. })
        ));
        assert!(matches!(
            export_canvas_segments(canvas, "segments.mp4", ExportConfig::new("segments.mp4")),
            Err(SegmentExportError::MissingPathPlaceholder)
        ));

        let mut colliding = Canvas::new(640, 360);
        colliding.segment("A+B", None).unwrap();
        colliding.wait(0.1);
        colliding.segment("A B", None).unwrap();
        colliding.wait(0.1);
        assert!(matches!(
            export_canvas_segments(
                colliding,
                "{segment}.mp4",
                ExportConfig::new("{segment}.mp4")
            ),
            Err(SegmentExportError::DuplicateOutput(path)) if path == "a-b.mp4"
        ));
    }

    #[test]
    fn explicit_stops_do_not_change_segment_export_ranges() {
        let mut canvas = Canvas::new(640, 360);
        canvas.segment("intro", None).unwrap();
        canvas.wait(1.0);
        canvas.stop(Some("ready".to_string())).unwrap();
        canvas.wait(0.5);
        canvas.segment("details", None).unwrap();
        canvas.wait(2.0);

        let manifest = canvas.segment_manifest();
        assert_eq!(manifest.duration(), 3.5);
        assert_eq!(manifest.segments[0].stops[0].time, 1.0);

        let mut config = ExportConfig::new("details.mp4");
        apply_segment_range(&mut config, &manifest.segments[1]).unwrap();
        assert_eq!(config.start_time, Some(1.5));
        assert_eq!(config.end_time, Some(3.5));
    }
}
