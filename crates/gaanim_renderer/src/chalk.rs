//! Chalk: a deterministic hand-drawn look for vector fragments.
//!
//! A chalk fragment draws its outline with a slight tremor and masks its
//! paint with a tileable grain, so a formula written with `write` looks
//! drawn on a blackboard. Both come from the [`ChalkBrush`] seed, so every
//! frame and every export draws the same chalk.

use std::sync::OnceLock;

use gaanim_core::kurbo::{self, Affine, BezPath, PathEl, Point, Vec2};
use gaanim_core::peniko;

use crate::effects::ChalkBrush;

/// Side of the grain texture, in pixels.
const GRAIN_PIXELS: u32 = 128;
/// Scene units one grain tile covers.
const GRAIN_TILE: f64 = 0.5;
/// Scene units between the points of a roughened outline.
const ROUGH_STEP: f64 = 0.02;
/// Scene units of the slow and fast tremor waves.
const SLOW_WAVE: f64 = 0.14;
const FAST_WAVE: f64 = 0.045;

fn mix(mut value: u64) -> u64 {
    // SplitMix64 finalizer.
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

/// A uniform value in `[0, 1)`.
fn uniform(hash: u64) -> f64 {
    (hash >> 11) as f64 / (1u64 << 53) as f64
}

/// Smooth value noise in `[-1, 1]` along one axis.
fn noise(seed: u64, t: f64) -> f64 {
    let cell = t.floor();
    let f = t - cell;
    let at = |i: i64| uniform(mix(seed ^ mix(i as u64))) * 2.0 - 1.0;
    let (a, b) = (at(cell as i64), at(cell as i64 + 1));
    a + (b - a) * f * f * (3.0 - 2.0 * f)
}

/// `path` with every point displaced along its normal by up to `amplitude`
/// local units. The tremor is a function of each point's arc length on its
/// sub-path, so a write reveal (a prefix of the full outline) trembles
/// exactly like the outline it reveals. `unit` is the local length of one
/// scene unit.
pub(crate) fn roughen(path: &BezPath, chalk: &ChalkBrush, unit: f64) -> BezPath {
    let amplitude = chalk.roughness * unit;
    if !(amplitude.is_finite() && amplitude > 0.0 && unit.is_finite() && unit > 0.0) {
        return path.clone();
    }
    let mut polylines: Vec<(Vec<Point>, bool)> = Vec::new();
    kurbo::flatten(path, (amplitude * 0.1).max(1e-5), |element| match element {
        PathEl::MoveTo(point) => polylines.push((vec![point], false)),
        PathEl::LineTo(point) => {
            if let Some((points, _)) = polylines.last_mut()
                && points.last() != Some(&point)
            {
                points.push(point);
            }
        }
        PathEl::ClosePath => {
            if let Some((_, closed)) = polylines.last_mut() {
                *closed = true;
            }
        }
        _ => {}
    });

    let step = ROUGH_STEP * unit;
    let mut rough = BezPath::new();
    for (index, (points, closed)) in polylines.iter().enumerate() {
        if points.len() < 2 {
            continue;
        }
        let seed = mix(chalk.seed ^ mix(index as u64 + 1));
        let offset = |length: f64| {
            amplitude
                * (0.7 * noise(seed, length / (SLOW_WAVE * unit))
                    + 0.3 * noise(seed ^ 0xC4A1_C0DE, length / (FAST_WAVE * unit)))
        };
        // Resample every `step`, keeping the original corners.
        let mut samples: Vec<(Point, Vec2, f64)> = Vec::with_capacity(points.len());
        let mut length = 0.0;
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let segment = b - a;
            let segment_length = segment.hypot();
            if segment_length <= 0.0 {
                continue;
            }
            let normal = Vec2::new(-segment.y, segment.x) / segment_length;
            let pieces = (segment_length / step).ceil().max(1.0) as usize;
            for piece in 0..pieces {
                let t = piece as f64 / pieces as f64;
                samples.push((a.lerp(b, t), normal, length + segment_length * t));
            }
            length += segment_length;
        }
        if let (Some(&last), Some(&(_, normal, _))) = (points.last(), samples.last()) {
            samples.push((last, normal, length));
        }
        for (position, (point, normal, length)) in samples.iter().enumerate() {
            let displaced = *point + *normal * offset(*length);
            if position == 0 {
                rough.move_to(displaced);
            } else {
                rough.line_to(displaced);
            }
        }
        if *closed {
            rough.close_path();
        }
    }
    rough
}

/// The shared chalk grain: white, with an alpha of mostly solid chalk
/// broken by streaks and specks of the board showing through.
fn grain() -> &'static peniko::ImageData {
    static GRAIN: OnceLock<peniko::ImageData> = OnceLock::new();
    GRAIN.get_or_init(|| {
        let size = GRAIN_PIXELS as usize;
        // Tileable value noise on a periodic lattice.
        let lattice = |seed: u64, cells: usize, x: f64, y: f64| {
            let fx = x * cells as f64 / size as f64;
            let fy = y * cells as f64 / size as f64;
            let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
            let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
            let at = |i: usize, j: usize| {
                uniform(mix(seed ^ mix(((i % cells) * 7919 + j % cells) as u64)))
            };
            let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
            let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * sx;
            let bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * sx;
            top + (bottom - top) * sy
        };
        let mut data = Vec::with_capacity(size * size * 4);
        for y in 0..size {
            for x in 0..size {
                let (fx, fy) = (x as f64, y as f64);
                // Streaks run along x, as chalk dragged across the board.
                let streak = lattice(11, 32, fx, fy * 4.0 % size as f64);
                let body = 0.6 * lattice(23, 8, fx, fy) + 0.4 * lattice(37, 16, fx, fy);
                let speck = uniform(mix(((y * size + x) as u64) ^ 0x5EED));
                let value = 0.5 * body + 0.3 * streak + 0.2 * speck;
                let alpha = ((value - 0.28) / 0.22).clamp(0.0, 1.0);
                let alpha = 0.18 + 0.82 * alpha * alpha * (3.0 - 2.0 * alpha);
                data.extend_from_slice(&[255, 255, 255, (alpha * 255.0).round() as u8]);
            }
        }
        peniko::ImageData {
            data: peniko::Blob::new(std::sync::Arc::new(data)),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: GRAIN_PIXELS,
            height: GRAIN_PIXELS,
        }
    })
}

/// Mask what was drawn since the matching `push_layer` with the chalk grain
/// over `bounds`, in local units where one scene unit spans `unit`.
pub(crate) fn mask_with_grain(
    scene: &mut vello::Scene,
    chalk: &ChalkBrush,
    unit: f64,
    bounds: kurbo::Rect,
) {
    let brush = peniko::ImageBrush::new(grain().clone()).with_extend(peniko::Extend::Repeat);
    let scale = GRAIN_TILE * unit / f64::from(GRAIN_PIXELS);
    let hash = mix(chalk.seed);
    let shift = Vec2::new(uniform(hash), uniform(mix(hash))) * GRAIN_TILE * unit;
    scene.push_layer(
        peniko::Fill::NonZero,
        peniko::BlendMode::new(peniko::Mix::Normal, peniko::Compose::DestIn),
        1.0,
        Affine::IDENTITY,
        &bounds,
    );
    scene.fill(
        peniko::Fill::NonZero,
        Affine::IDENTITY,
        &peniko::Brush::Image(brush),
        Some(Affine::translate(shift) * Affine::scale(scale)),
        &bounds,
    );
    scene.pop_layer();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chalk(seed: u64) -> ChalkBrush {
        ChalkBrush {
            seed,
            roughness: 0.01,
        }
    }

    #[test]
    fn roughening_is_deterministic_and_bounded() {
        let path = BezPath::from_svg("M 0 0 L 2 0 L 2 1 Z").unwrap();
        let a = roughen(&path, &chalk(3), 1.0);
        assert_eq!(a, roughen(&path, &chalk(3), 1.0));
        assert_ne!(a, roughen(&path, &chalk(4), 1.0));
        for element in a.elements() {
            if let PathEl::LineTo(point) | PathEl::MoveTo(point) = element {
                // Every point stays within the roughness of the triangle.
                let near = point.y.abs() <= 0.0101
                    || (point.x - 2.0).abs() <= 0.0101
                    || (point.y - point.x / 2.0).abs() <= 0.0101 * 1.2;
                assert!(near, "{point:?}");
            }
        }
    }

    #[test]
    fn a_revealed_prefix_trembles_like_the_full_outline() {
        let full = BezPath::from_svg("M 0 0 L 1 0 L 1 1").unwrap();
        let prefix = BezPath::from_svg("M 0 0 L 1 0 L 1 0.5").unwrap();
        let full = roughen(&full, &chalk(9), 1.0);
        let prefix = roughen(&prefix, &chalk(9), 1.0);
        let full: Vec<_> = full.elements().iter().take(60).collect();
        let prefix: Vec<_> = prefix.elements().iter().take(60).collect();
        assert_eq!(full, prefix);
    }

    #[test]
    fn grain_tiles_and_breaks_the_paint() {
        let grain = grain();
        let alphas: Vec<u8> = grain.data.data().chunks(4).map(|px| px[3]).collect();
        let mean = alphas.iter().map(|&a| f64::from(a)).sum::<f64>() / alphas.len() as f64;
        assert!(mean > 120.0 && mean < 240.0, "mean alpha {mean}");
        assert!(alphas.iter().any(|&a| a < 80));
    }
}
