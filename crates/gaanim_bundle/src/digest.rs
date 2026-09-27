//! Content digests of composed Vello scenes.
//!
//! A bundle stores the digest of every frame as the scene composed it while
//! recording. Hashing the frame it decodes and composes again proves that
//! playback hands Vello exactly the same drawing, bit for bit.

use vello_encoding::Patch;

/// BLAKE3 digest of what `scene` draws: every encoded stream and the
/// late-bound gradients and images, the images by their pixels (their ids
/// differ between the recording and a replay).
pub fn scene_digest(scene: &vello::Scene) -> [u8; 32] {
    let encoding = scene.encoding();
    let mut hasher = blake3::Hasher::new();
    for count in [
        encoding.n_paths,
        encoding.n_path_segments,
        encoding.n_clips,
        encoding.n_open_clips,
    ] {
        hasher.update(&count.to_le_bytes());
    }
    let mut stream = |bytes: &[u8]| {
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    };
    stream(bytemuck::cast_slice(&encoding.path_tags));
    stream(bytemuck::cast_slice(&encoding.path_data));
    stream(bytemuck::cast_slice(&encoding.draw_tags));
    stream(bytemuck::cast_slice(&encoding.draw_data));
    stream(bytemuck::cast_slice(&encoding.transforms));
    stream(bytemuck::cast_slice(&encoding.styles));
    let resources = &encoding.resources;
    hasher.update(&(resources.color_stops.len() as u64).to_le_bytes());
    for stop in &resources.color_stops {
        hasher.update(&stop.offset.to_bits().to_le_bytes());
        hasher.update(&[stop.color.cs as u8]);
        for component in stop.color.components {
            hasher.update(&component.to_bits().to_le_bytes());
        }
    }
    hasher.update(&(resources.patches.len() as u64).to_le_bytes());
    for patch in &resources.patches {
        match patch {
            Patch::Ramp {
                draw_data_offset,
                stops,
                extend,
            } => {
                hasher.update(&[0]);
                hasher.update(&(*draw_data_offset as u64).to_le_bytes());
                hasher.update(&(stops.start as u64).to_le_bytes());
                hasher.update(&(stops.end as u64).to_le_bytes());
                hasher.update(&[*extend as u8]);
            }
            Patch::GlyphRun { index } => {
                hasher.update(&[1]);
                hasher.update(&(*index as u64).to_le_bytes());
            }
            Patch::Image {
                draw_data_offset,
                image,
            } => {
                hasher.update(&[2]);
                hasher.update(&(*draw_data_offset as u64).to_le_bytes());
                hasher.update(&[image.format as u8, image.alpha_type as u8]);
                hasher.update(&image.width.to_le_bytes());
                hasher.update(&image.height.to_le_bytes());
                hasher.update(image.data.data());
            }
        }
    }
    // Gaanim draws text as paths; a glyph run still changes the digest.
    hasher.update(&(resources.glyph_runs.len() as u64).to_le_bytes());
    hasher.update(&(resources.glyphs.len() as u64).to_le_bytes());
    *hasher.finalize().as_bytes()
}
