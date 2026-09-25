// Copyright 2026 the Gaanim Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Gaanim patch: size Vello's bump-allocated GPU buffers from the scene.
//!
//! Upstream `BufferSizes::new` uses fixed sizes. A scene that needs more
//! (for example a few hundred paths whose bounding boxes cover a 1080p frame)
//! makes every GPU stage bail out and the fine rasterizer write no pixels.
//!
//! [`crate::Resolver::resolve`] records the device-space bounding box of every
//! path in the packed scene, and [`crate::RenderConfig::new`], which Vello
//! calls next on the same thread, sizes the tile buffer to what
//! `tile_alloc` will allocate for them. Hosts that detect a failed frame can
//! also raise every bump buffer with [`set_bump_buffer_scale`]. No buffer
//! grows beyond [`max_bump_buffer_bytes`], which must not exceed the device's
//! `max_storage_buffer_binding_size`.
//!
//! The scale and the size limit belong to the calling thread, like the
//! render that reads them, so an exporter with its own device can raise them
//! without affecting a preview renderer on another thread.

use std::cell::{Cell, RefCell};

use crate::{
    BufferSize, BufferSizes, Layout, LineSoup, PathSegment, PathTag, SegmentCount, Style, Tile,
    Transform,
};

/// Words each tile reserves up front in the per-tile command list.
/// Mirrors `PTCL_INITIAL_ALLOC` in Vello's `shader/shared/ptcl.wgsl`.
const PTCL_INITIAL_ALLOC: u64 = 64;
/// Command-list words available beyond the up-front reservation, as in the
/// upstream default for a 1080p frame.
const PTCL_DYNAMIC_WORDS: u64 = 1 << 22;
const TILE_SIZE: f32 = 16.0;

thread_local! {
    /// Device-space `[x0, y0, x1, y1]` per path of the last resolved scene.
    static PATH_BOUNDS: RefCell<Option<Vec<[f32; 4]>>> = const { RefCell::new(None) };
    static SCALE: Cell<u32> = const { Cell::new(1) };
    static MAX_BYTES: Cell<u64> = const { Cell::new(128 << 20) };
}

/// Multiply every bump-allocated buffer by `scale` (at least 1) in later
/// renders on this thread. Retrying a failed frame with a larger scale lets
/// it render.
pub fn set_bump_buffer_scale(scale: u32) {
    SCALE.set(scale.max(1));
}

/// Multiplier of the bump-allocated buffers on this thread.
pub fn bump_buffer_scale() -> u32 {
    SCALE.get()
}

/// Largest size, in bytes, of any single bump-allocated buffer in renders on
/// this thread. It must not exceed the device's
/// `max_storage_buffer_binding_size`; the default is wgpu's default limit of
/// 128 MiB.
pub fn set_max_bump_buffer_bytes(bytes: u64) {
    MAX_BYTES.set(bytes.max(1 << 20));
}

/// Largest size, in bytes, of any single bump-allocated buffer on this thread.
pub fn max_bump_buffer_bytes() -> u64 {
    MAX_BYTES.get()
}

/// Record the device-space bounding box of every path in a packed scene.
///
/// Walks the path tag stream exactly as Vello's `flatten` stage does: a
/// segment reads its start point and `n` new points from the current path
/// data offset, uses the transform and style of the tags before it, and a
/// subpath end skips the next subpath's move-to point.
pub(crate) fn record_path_bounds(layout: &Layout, data: &[u8]) {
    let tags = layout.path_tags(data);
    // The packed scene is a `Vec<u8>`, which the allocator aligns in practice;
    // copy the words only if it is not.
    let path_bytes = layout.path_data(data);
    let path_data: std::borrow::Cow<'_, [u32]> = match bytemuck::try_cast_slice(path_bytes) {
        Ok(words) => std::borrow::Cow::Borrowed(words),
        Err(_) => std::borrow::Cow::Owned(
            path_bytes
                .chunks_exact(4)
                .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
                .collect(),
        ),
    };
    let path_data = &*path_data;
    let transform_bytes = &data[layout.transform_base as usize * 4..layout.style_base as usize * 4];
    let style_bytes = &data[layout.style_base as usize * 4..];
    let transform_at = |ix: usize| -> Option<Transform> {
        let size = size_of::<Transform>();
        Some(bytemuck::pod_read_unaligned(
            transform_bytes.get(ix * size..(ix + 1) * size)?,
        ))
    };
    let style_at = |ix: usize| -> Option<Style> {
        let size = size_of::<Style>();
        Some(bytemuck::pod_read_unaligned(
            style_bytes.get(ix * size..(ix + 1) * size)?,
        ))
    };
    const EMPTY: [f32; 4] = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    PATH_BOUNDS.with(|cell| {
        let mut cell = cell.borrow_mut();
        let bounds = cell.get_or_insert_with(Vec::new);
        bounds.clear();
        let mut n_transforms = 0_usize;
        let mut n_styles = 0_usize;
        let mut offset = 0_usize;
        // The f32 points of consecutive segments are contiguous in the path
        // data (a segment shares its start point with the previous end, and a
        // subpath end skips to the next move-to), so the tag loop only tracks
        // the word range of each run of one transform. The range is bounded in
        // local space and its corners are transformed: an affine map of a box
        // contains the mapped points, which keeps an upper bound.
        let mut run = offset..offset;
        let mut transform = Transform::IDENTITY;
        let mut local = EMPTY;
        let mut bbox = EMPTY;
        let mut reach = 0.0_f32;
        let mut style: Option<Style> = None;
        let mut style_reach = 0.0_f32;
        let bound_range = |local: &mut [f32; 4], words: &[u32]| {
            for point in words.chunks_exact(2) {
                let (x, y) = (f32::from_bits(point[0]), f32::from_bits(point[1]));
                *local = [
                    local[0].min(x),
                    local[1].min(y),
                    local[2].max(x),
                    local[3].max(y),
                ];
            }
        };
        let flush = |local: &mut [f32; 4], bbox: &mut [f32; 4], transform: &Transform| {
            if !(local[0] <= local[2]) {
                return;
            }
            let [a, b, c, d] = transform.matrix;
            let [tx, ty] = transform.translation;
            for (x, y) in [
                (local[0], local[1]),
                (local[2], local[1]),
                (local[0], local[3]),
                (local[2], local[3]),
            ] {
                let px = a * x + c * y + tx;
                let py = b * x + d * y + ty;
                if px.is_finite() && py.is_finite() {
                    *bbox = [
                        bbox[0].min(px),
                        bbox[1].min(py),
                        bbox[2].max(px),
                        bbox[3].max(py),
                    ];
                }
            }
            *local = EMPTY;
        };
        let words_in =
            |range: &std::ops::Range<usize>| path_data.get(range.clone()).unwrap_or_default();
        for tag in tags {
            let byte = tag.0;
            if byte == PathTag::TRANSFORM.0 {
                bound_range(&mut local, words_in(&run));
                run = offset..offset;
                flush(&mut local, &mut bbox, &transform);
                n_transforms += 1;
                transform = transform_at(n_transforms - 1).unwrap_or(Transform::IDENTITY);
                style_reach = style.map_or(0.0, |style| stroke_reach(&style, &transform));
                continue;
            }
            if byte == PathTag::STYLE.0 {
                n_styles += 1;
                style = style_at(n_styles - 1);
                style_reach = style.map_or(0.0, |style| stroke_reach(&style, &transform));
                continue;
            }
            if byte == PathTag::PATH.0 {
                bound_range(&mut local, words_in(&run));
                run = offset..offset;
                flush(&mut local, &mut bbox, &transform);
                bounds.push(if bbox[0] <= bbox[2] {
                    [
                        bbox[0] - reach,
                        bbox[1] - reach,
                        bbox[2] + reach,
                        bbox[3] + reach,
                    ]
                } else {
                    bbox
                });
                bbox = EMPTY;
                reach = 0.0;
                continue;
            }
            let new_points = usize::from(byte & 0x3);
            if new_points == 0 {
                continue;
            }
            if tag.is_f32() {
                // Start point plus the new points.
                if run.is_empty() {
                    run.start = offset;
                }
                run.end = offset + (new_points + 1) * 2;
                offset += (new_points + usize::from(tag.is_subpath_end())) * 2;
            } else {
                // Packed i16 points break the f32 range; bound them directly.
                bound_range(&mut local, words_in(&run));
                for word in path_data
                    .get(offset..offset + new_points + 1)
                    .unwrap_or_default()
                {
                    let x = ((*word << 16) as i32 >> 16) as f32;
                    let y = (*word as i32 >> 16) as f32;
                    local = [
                        local[0].min(x),
                        local[1].min(y),
                        local[2].max(x),
                        local[3].max(y),
                    ];
                }
                offset += new_points + usize::from(tag.is_subpath_end());
                run = offset..offset;
            }
            reach = reach.max(style_reach);
        }
    });
}

/// How far a stroke can extend beyond its control points, in device space:
/// half its width, up to the miter limit at joins and diagonally at caps,
/// stretched by the transform.
fn stroke_reach(style: &Style, transform: &Transform) -> f32 {
    let flags = style.flags_and_miter_limit;
    if flags & Style::FLAGS_STYLE_BIT == 0 {
        return 0.0;
    }
    let corner = if flags & Style::FLAGS_JOIN_MASK == Style::FLAGS_JOIN_BITS_MITER {
        f16_to_f32((flags & Style::MITER_LIMIT_MASK) as u16).max(std::f32::consts::SQRT_2)
    } else {
        std::f32::consts::SQRT_2
    };
    let [a, b, c, d] = transform.matrix;
    let stretch = (a * a + b * b + c * c + d * d).sqrt();
    let reach = 0.5 * style.line_width.abs() * corner * stretch;
    if reach.is_finite() { reach } else { 0.0 }
}

fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let mantissa = f32::from(bits & 0x3ff);
    match exponent {
        0 => sign * mantissa * 2.0_f32.powi(-24),
        31 => f32::INFINITY,
        _ => sign * (1.0 + mantissa / 1024.0) * 2.0_f32.powi(exponent - 15),
    }
}

/// Tiles `tile_alloc` allocates for the recorded paths in a viewport of
/// `width` by `height` tiles, consuming the record. Each path takes every
/// tile of its bounding box clipped to the viewport.
fn take_tile_count(width: u32, height: u32) -> Option<u64> {
    let bounds = PATH_BOUNDS.with(|cell| cell.borrow_mut().take())?;
    let span = |low: f32, high: f32, limit: u32| {
        let start = (low / TILE_SIZE).floor().clamp(0.0, limit as f32);
        let end = (high / TILE_SIZE).ceil().clamp(0.0, limit as f32);
        (end - start).max(0.0) as u64
    };
    let tiles = bounds
        .iter()
        .filter(|bbox| bbox[0] <= bbox[2] && bbox[1] <= bbox[3])
        .map(|bbox| span(bbox[0], bbox[2], width) * span(bbox[1], bbox[3], height))
        .sum();
    PATH_BOUNDS.with(|cell| {
        // Keep the allocation for the next frame.
        let mut bounds = bounds;
        bounds.clear();
        *cell.borrow_mut() = Some(bounds);
    });
    Some(tiles)
}

/// A buffer of at least `current` elements holding `wanted` when the size
/// limit allows it; never smaller than the upstream size.
fn grown<T>(current: u32, wanted: u64, max_bytes: u64) -> BufferSize<T> {
    let cap = (max_bytes / size_of::<T>() as u64).max(u64::from(current));
    let len = wanted
        .clamp(u64::from(current), cap)
        .min(u64::from(u32::MAX));
    BufferSize::new(len as u32)
}

/// Apply the scene estimate, the host scale and the size limit to the
/// bump-allocated buffers of one render.
pub(crate) fn fit(sizes: &mut BufferSizes, width_in_tiles: u32, height_in_tiles: u32) {
    let scale = u64::from(bump_buffer_scale());
    let max = max_bump_buffer_bytes();
    let tiles_needed = take_tile_count(width_in_tiles, height_in_tiles).unwrap_or(0);
    let viewport_tiles = u64::from(width_in_tiles) * u64::from(height_in_tiles);
    let scaled = |len: u32| u64::from(len) * scale;

    // Leave an eighth of headroom over the estimate for bounding boxes that
    // flattening widens by a pixel.
    let tiles = scaled(sizes.tiles.len()).max(tiles_needed + tiles_needed / 8);
    sizes.tiles = grown::<Tile>(sizes.tiles.len(), tiles, max);
    // Every tile reserves its first command-list words up front, so large
    // viewports need room beyond that reservation.
    let ptcl = scaled(sizes.ptcl.len())
        .max(viewport_tiles * PTCL_INITIAL_ALLOC + PTCL_DYNAMIC_WORDS * scale);
    sizes.ptcl = grown::<u32>(sizes.ptcl.len(), ptcl, max);
    sizes.lines = grown::<LineSoup>(sizes.lines.len(), scaled(sizes.lines.len()), max);
    sizes.seg_counts =
        grown::<SegmentCount>(sizes.seg_counts.len(), scaled(sizes.seg_counts.len()), max);
    sizes.segments = grown::<PathSegment>(sizes.segments.len(), scaled(sizes.segments.len()), max);
    sizes.bin_data = grown::<u32>(sizes.bin_data.len(), scaled(sizes.bin_data.len()), max);
    sizes.blend_spill = grown::<u32>(
        sizes.blend_spill.len(),
        scaled(sizes.blend_spill.len()),
        max,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Encoding, RenderConfig, Resolver};
    use peniko::kurbo::{Affine, BezPath, Rect, Stroke};
    use peniko::{Color, Fill};

    fn resolved_config(encoding: &Encoding, width: u32, height: u32) -> RenderConfig {
        let mut resolver = Resolver::default();
        let mut packed = Vec::new();
        let (layout, _, _) = resolver.resolve(encoding, &mut packed);
        RenderConfig::new(&layout, width, height, &Color::BLACK)
    }

    fn diagonal_lines(count: usize, width: f64, height: f64) -> Encoding {
        let mut encoding = Encoding::default();
        encoding.encode_transform(Transform::IDENTITY);
        for _ in 0..count {
            let _ = encoding.encode_stroke_style(&Stroke::new(2.0));
            let mut line = BezPath::new();
            line.move_to((0.0, 0.0));
            line.line_to((width, height));
            encoding.encode_shape(&line, false);
            encoding.encode_color(Color::WHITE);
        }
        encoding
    }

    #[test]
    fn frame_spanning_paths_get_one_tile_per_covered_tile() {
        // 1920x1080 is 120x68 tiles; each diagonal covers all of them.
        let config = resolved_config(&diagonal_lines(600, 1920.0, 1080.0), 1920, 1080);
        let needed = 600 * 120 * 68;
        assert!(u64::from(config.buffer_sizes.tiles.len()) >= needed);
        assert_eq!(config.gpu.tiles_size, config.buffer_sizes.tiles.len());
        assert!(u64::from(config.buffer_sizes.tiles.len()) <= needed * 2);
    }

    #[test]
    fn small_scenes_keep_the_upstream_sizes() {
        let mut encoding = Encoding::default();
        encoding.encode_transform(Transform::from_kurbo(&Affine::translate((100.0, 100.0))));
        encoding.encode_fill_style(Fill::NonZero);
        encoding.encode_shape(&Rect::new(0.0, 0.0, 50.0, 50.0), true);
        encoding.encode_color(Color::WHITE);
        let config = resolved_config(&encoding, 1920, 1080);
        assert_eq!(config.buffer_sizes.tiles.len(), 1 << 21);
        assert_eq!(config.buffer_sizes.ptcl.len(), 1 << 23);
        assert_eq!(config.buffer_sizes.lines.len(), 1 << 21);
    }

    #[test]
    fn path_bounds_follow_transforms_and_stroke_width() {
        let mut encoding = Encoding::default();
        encoding.encode_transform(Transform::from_kurbo(
            &(Affine::translate((320.0, 40.0)) * Affine::scale(2.0)),
        ));
        let _ =
            encoding.encode_stroke_style(&Stroke::new(10.0).with_join(peniko::kurbo::Join::Round));
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        encoding.encode_shape(&line, false);
        encoding.encode_color(Color::WHITE);
        let mut resolver = Resolver::default();
        let mut packed = Vec::new();
        resolver.resolve(&encoding, &mut packed);
        let bounds = PATH_BOUNDS
            .with(|cell| cell.borrow().clone())
            .expect("recorded bounds");
        assert_eq!(bounds.len(), 1);
        // Half the width, diagonally at the caps, stretched by the scale.
        let reach = 0.5 * 10.0 * std::f32::consts::SQRT_2 * 8.0_f32.sqrt();
        let [x0, y0, x1, y1] = bounds[0];
        assert!((x0 - (320.0 - reach)).abs() < 1e-3, "{x0}");
        assert!((x1 - (520.0 + reach)).abs() < 1e-3, "{x1}");
        assert!((y0 - (40.0 - reach)).abs() < 1e-3 && (y1 - (40.0 + reach)).abs() < 1e-3);
    }

    #[test]
    fn host_scale_and_limit_bound_every_bump_buffer() {
        set_bump_buffer_scale(4);
        set_max_bump_buffer_bytes(64 << 20);
        let config = resolved_config(&Encoding::default(), 1920, 1080);
        // Another thread keeps its own defaults.
        let other = std::thread::spawn(|| (bump_buffer_scale(), max_bump_buffer_bytes()));
        assert_eq!(other.join().unwrap(), (1, 128 << 20));
        for bytes in [
            config.buffer_sizes.tiles.size_in_bytes(),
            config.buffer_sizes.ptcl.size_in_bytes(),
            config.buffer_sizes.lines.size_in_bytes(),
            config.buffer_sizes.segments.size_in_bytes(),
        ] {
            assert!(u64::from(bytes) <= 64 << 20, "{bytes}");
        }
        assert_eq!(config.buffer_sizes.ptcl.len(), 1 << 24, "capped at 64 MiB");
        assert_eq!(config.buffer_sizes.tiles.len(), 1 << 23);
    }
}
