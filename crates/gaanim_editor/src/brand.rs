//! Gaanim's horizontal logo for dark backgrounds, drawn from the geometry
//! `tools/generate_brand.py` writes (`brand_logo.rs`): the symbol's pixel
//! frames as rectangles and the wordmark's curves rasterized once per size,
//! at the screen's pixels, so it stays sharp at any display scale.

use bevy_egui::egui::{self, Color32, Pos2, Rect, Vec2, pos2, vec2};

include!("brand_logo.rs");

/// Samples per pixel side when rasterizing the wordmark.
const SUPERSAMPLE: usize = 4;

/// Size of the logo drawn at `unit` points per unit.
pub(crate) fn logo_size(unit: f32) -> Vec2 {
    vec2(LOGO_SIZE[0], LOGO_SIZE[1]) * unit
}

/// Paint the logo with its top-left corner at `origin`, `unit` points per
/// unit (the brand asks for whole pixels per unit: 2 makes it 142 × 32).
pub(crate) fn paint_logo(ui: &egui::Ui, origin: Pos2, unit: f32) {
    let painter = ui.painter();
    let ppp = ui.ctx().pixels_per_point();
    // Whole physical pixels, so the frames' edges stay crisp.
    let origin = pos2(
        (origin.x * ppp).round() / ppp,
        (origin.y * ppp).round() / ppp,
    );
    let at = |x: f32, y: f32| origin + vec2(x, y) * unit;
    let color = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);

    for (index, rows) in LOGO_FRAME_ROWS.iter().enumerate() {
        let left = index as f32 * LOGO_FRAME_STEP;
        let side = rows.len() as f32;
        for (row, &width) in rows.iter().enumerate() {
            let x = left + (side - f32::from(width)) / 2.0;
            let y = LOGO_FRAME_TOP + row as f32;
            painter.rect_filled(
                Rect::from_min_max(at(x, y), at(x + f32::from(width), y + 1.0)),
                0.0,
                color(LOGO_DARK_FRAMES[index]),
            );
        }
    }

    let scale = unit * ppp;
    let texture = wordmark_texture(ui.ctx(), scale);
    let size = texture.size_vec2() / ppp;
    painter.image(
        texture.id(),
        Rect::from_min_size(origin, size),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    let [x0, y0, x1, y1] = LOGO_DOT;
    painter.rect_filled(
        Rect::from_min_max(at(x0, y0), at(x1, y1)),
        0.0,
        color(LOGO_DARK_DOT),
    );
}

/// The wordmark at `scale` physical pixels per unit, made once per scale.
fn wordmark_texture(ctx: &egui::Context, scale: f32) -> egui::TextureHandle {
    let id = egui::Id::new("gaanim-wordmark");
    let key = (scale * 100.0).round() as u32;
    if let Some((cached, texture)) =
        ctx.data(|data| data.get_temp::<(u32, egui::TextureHandle)>(id))
        && cached == key
    {
        return texture;
    }
    let (width, height, coverage) = rasterize(LOGO_WORDMARK, scale);
    let [r, g, b, _] = LOGO_DARK_TEXT;
    let pixels: Vec<u8> = coverage
        .iter()
        .flat_map(|&alpha| [r, g, b, (alpha * 255.0).round() as u8])
        .collect();
    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels);
    let texture = ctx.load_texture("gaanim-wordmark", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(id, (key, texture.clone())));
    texture
}

/// Coverage of `shapes` on a grid of `scale` pixels per unit: each shape
/// filled even-odd, and the shapes joined.
fn rasterize(shapes: &[&[&[[f32; 2]]]], scale: f32) -> (usize, usize, Vec<f32>) {
    let width = (LOGO_SIZE[0] * scale).ceil() as usize;
    let height = (LOGO_SIZE[1] * scale).ceil() as usize;
    let mut coverage = vec![0.0f32; width * height];
    let mut samples = vec![0u16; width * height];
    let mut crossings = Vec::new();
    let full = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    for shape in shapes {
        samples.fill(0);
        for row in 0..height * SUPERSAMPLE {
            let y = (row as f32 + 0.5) / (SUPERSAMPLE as f32 * scale);
            crossings.clear();
            for contour in shape.iter() {
                for (index, &[ax, ay]) in contour.iter().enumerate() {
                    let [bx, by] = contour[(index + 1) % contour.len()];
                    if (ay <= y) != (by <= y) {
                        let x = ax + (y - ay) / (by - ay) * (bx - ax);
                        crossings.push(x * scale * SUPERSAMPLE as f32);
                    }
                }
            }
            crossings.sort_by(f32::total_cmp);
            for span in crossings.chunks_exact(2) {
                // Sample columns whose centres fall inside the span.
                let first = (span[0] - 0.5).ceil().max(0.0) as usize;
                let last = ((span[1] - 0.5).ceil().max(0.0) as usize).min(width * SUPERSAMPLE);
                for column in first..last {
                    samples[(row / SUPERSAMPLE) * width + column / SUPERSAMPLE] += 1;
                }
            }
        }
        for (total, &count) in coverage.iter_mut().zip(&samples) {
            *total = total.max(f32::from(count) / full);
        }
    }
    (width, height, coverage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wordmark_fills_its_strokes_and_leaves_its_counters() {
        // Two pixels per unit: the lockup is 142 × 32.
        let (width, height, coverage) = rasterize(LOGO_WORDMARK, 2.0);
        assert_eq!((width, height), (142, 32));
        let at = |x: f32, y: f32| coverage[(y * 2.0) as usize * width + (x * 2.0) as usize];
        // The first a's stem (x 31.5..33.5) is ink; its counter, 2 units
        // into the bowl, is not; the i's stem is ink.
        assert!(at(32.5, 8.0) > 0.99);
        assert!(at(29.5, 8.0) < 0.01);
        assert!(at(56.0, 8.0) > 0.99);
        // Above the x-height, away from the dot, there is nothing.
        assert!(at(40.0, 2.0) < 0.01);
    }
}
