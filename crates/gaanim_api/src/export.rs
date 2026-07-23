//! Canonical export helpers for `gaanim_api` scenes.
//!
//! Language bindings should construct a [`Canvas`](crate::canvas::Canvas) and
//! call these helpers instead of duplicating Bevy/world setup.

use gaanim_export::prelude::{ExportError, export_scene, export_scene_direct};

use crate::canvas::Canvas;
use crate::runtime::replay_canvas_into;

pub use gaanim_export::encoder::{EncodingSpeed, VideoEncoder, detect_best_encoder};
pub use gaanim_export::prelude::{AspectRatioPreset, ExportConfig, ExportFormat, QualityPreset};

/// Export a Canvas using the supplied `gaanim_export::ExportConfig`.
pub fn export_canvas(canvas: Canvas, config: ExportConfig) -> Result<(), ExportError> {
    if config.headless {
        export_scene_direct(config, move |world| replay_canvas_into(world, canvas))
    } else {
        export_scene(config, move |world| replay_canvas_into(world, canvas))
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
