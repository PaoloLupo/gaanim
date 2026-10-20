//! Writing a bundle again from another one, in this build's format: the
//! frames it decodes to, its static data, media and cover.

use std::collections::HashMap;
use std::io::{Seek, Write};
use std::path::Path;
use std::sync::Arc;

use gaanim_core::kurbo;

use crate::{Bundle, BundleError, BundleWriter, Result};

/// Width, in pixels, of the export a bundle packed for the web is drawn
/// within a fraction of a pixel of (`model::GRID_PIXELS`): 4K.
const GRID_OUTPUT_WIDTH: u32 = 3840;

/// How [`repack_with`] writes a bundle again.
#[derive(Clone, Debug, Default)]
pub struct RepackOptions {
    /// Pack for publishing in other pages (`gaanim pack --web`): store each
    /// path's points on a grid fine enough that a 4K export of any frame
    /// moves no point more than 1/64 of a pixel, the vertices of 3D meshes
    /// and line lists on one of 2^-17 of their size (as fine for a mesh
    /// that fills a 4K frame), and leave out the frame
    /// digests and the sub-frames of motion blur, which players do not use.
    /// The result plays and exports as the original within that tolerance,
    /// but no longer composes bit for bit.
    pub web: bool,
}

/// Write `bundle` again into `output` in the current format, frame for frame
/// with the digests it recorded, so the result composes exactly what the
/// original did. A bundle in an older format this build reads comes out in
/// the current one, with its tables in segments.
pub fn repack<W: Write + Seek>(
    bundle: &mut Bundle,
    output: W,
    generator: impl Into<String>,
) -> Result<W> {
    repack_with(bundle, output, generator, &RepackOptions::default())
}

/// [`repack`] with `options`.
pub fn repack_with<W: Write + Seek>(
    bundle: &mut Bundle,
    output: W,
    generator: impl Into<String>,
    options: &RepackOptions,
) -> Result<W> {
    let mut writer = BundleWriter::new(output, generator);
    // The frames name drawables by the keys the original recorded, and so
    // does the static data (poll bars, live text).
    writer.keys.keep = true;
    if options.web {
        writer.tables.grid = Some(drawn_scales(bundle)?);
        writer.approximate = true;
    }
    let mut last = f64::NEG_INFINITY;
    for index in 0..bundle.frame_count() {
        let mut frame = bundle.frame(index)?;
        // A recording in two passes stores the instants between grid frames
        // after the grid.
        if frame.time <= last {
            writer.start_pass()?;
        }
        last = frame.time;
        let digest = if options.web {
            frame.motion_blur.clear();
            [0; 32]
        } else {
            bundle
                .digest(index)
                .ok_or_else(|| BundleError::Corrupt("frame digests are missing".into()))?
        };
        writer.push_frame(&frame, digest)?;
    }
    let mut scene = bundle.scene.clone();
    for track in &mut scene.audio {
        let bytes = bundle.media(&track.media)?;
        let extension = Path::new(&track.media)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_owned();
        track.media = writer.add_media_bytes(&bytes, &extension)?;
    }
    if let Some(png) = bundle.thumbnail()? {
        writer.set_thumbnail(&png)?;
    }
    writer.finish(&scene)
}

/// The most pixels of a 4K export per unit of its own coordinates that each
/// path a drawable draws reaches over the bundle, by the path's address in
/// the bundle's tables (decoded segments stay, so addresses do).
fn drawn_scales(bundle: &mut Bundle) -> Result<HashMap<usize, f64>> {
    let mut scales: HashMap<usize, f64> = HashMap::new();
    for index in 0..bundle.frame_count() {
        let frame = bundle.frame(index)?;
        let camera = &frame.camera;
        let per_unit = gaanim_renderer::pipeline::output_pixels_per_unit(camera, GRID_OUTPUT_WIDTH)
            .unwrap_or(f64::from(GRID_OUTPUT_WIDTH) / camera.frame_width.max(f64::EPSILON));
        for element in &frame.capture.elements {
            let scale = per_unit * largest_scale(element.transform);
            for path in [&element.recipe.path, &element.recipe.source]
                .into_iter()
                .flatten()
            {
                let entry = scales.entry(Arc::as_ptr(path) as usize).or_insert(0.0);
                *entry = entry.max(scale);
            }
        }
    }
    Ok(scales)
}

/// How much `transform` stretches a length at most: its largest singular
/// value.
fn largest_scale(transform: kurbo::Affine) -> f64 {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let (p, q, r) = (a * a + b * b, c * c + d * d, a * c + b * d);
    (((p + q) + ((p - q).powi(2) + 4.0 * r * r).sqrt()) / 2.0).sqrt()
}
