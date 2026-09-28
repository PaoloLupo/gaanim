//! The Gaanim icon on every native window: title bar, taskbar and window
//! switcher.
//!
//! The 16×16 pixel mark from `tools/generate_brand.py` is scaled by a whole
//! factor so its pixels stay square, over a tile whose rounded corners are
//! antialiased at the icon's size. The executables also embed the icon as a
//! Windows resource (see `build.rs`) for Explorer and pinned shortcuts; windows
//! do not load it back because winit resolves resources in the module that
//! contains winit, which is Bevy's DLL in dynamically linked dev builds.

use bevy::prelude::*;
use bevy::window::WindowCreated;
use bevy::winit::WINIT_WINDOWS;
use winit::window::Icon;

include!("app_icon_pixels.rs");

/// Scale of the single icon on platforms without separate sizes: 64 px halves
/// exactly to 32 and 16 px.
#[cfg(not(windows))]
const RGBA_SCALE: usize = 4;

pub(crate) struct AppIconPlugin;

impl Plugin for AppIconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            apply_app_icon_system.run_if(on_message::<WindowCreated>),
        );
    }
}

/// Exclusive, so it runs on the main thread that owns the winit windows. Icons
/// are idempotent, so every open window is refreshed when one is created.
fn apply_app_icon_system(_world: &mut World) {
    WINIT_WINDOWS.with_borrow(|windows| {
        for window in windows.windows.values() {
            apply_icon(window);
        }
    });
}

fn icon(scale: usize) -> Option<Icon> {
    let side = (ICON_GRID * scale) as u32;
    Icon::from_rgba(icon_rgba(scale), side, side).ok()
}

#[cfg(windows)]
fn apply_icon(window: &winit::window::Window) {
    use winit::platform::windows::WindowExtWindows;

    // Windows asks for a 16 px small icon (title bar) and a 32 px large icon
    // (taskbar, Alt+Tab), both multiplied by the display scale; use the nearest
    // whole scale of the grid.
    let scale = (window.scale_factor().round() as usize).max(1);
    window.set_window_icon(icon(scale));
    window.set_taskbar_icon(icon(2 * scale));
}

#[cfg(not(windows))]
fn apply_icon(window: &winit::window::Window) {
    window.set_window_icon(icon(RGBA_SCALE));
}

/// The icon as straight RGBA, each grid pixel drawn as a `scale`×`scale` block
/// except the tile's rounded corners.
fn icon_rgba(scale: usize) -> Vec<u8> {
    let side = ICON_GRID * scale;
    let mut rgba = Vec::with_capacity(side * side * 4);
    for y in 0..side {
        for x in 0..side {
            let index = usize::from(ICON_PIXELS[y / scale][x / scale] - b'0');
            if index == 0 {
                rgba.extend_from_slice(&[0; 4]);
                continue;
            }
            let [r, g, b, a] = ICON_PALETTE[index - 1];
            let coverage = if index == 1 {
                tile_coverage(x, y, side, ICON_TILE_RADIUS * scale as f32)
            } else {
                1.0
            };
            rgba.extend_from_slice(&[r, g, b, (f32::from(a) * coverage).round() as u8]);
        }
    }
    rgba
}

/// How much of pixel (`x`, `y`) a `side`-pixel square with corners of
/// `radius` covers, from 4×4 samples.
fn tile_coverage(x: usize, y: usize, side: usize, radius: f32) -> f32 {
    const SAMPLES: usize = 4;
    let side = side as f32;
    let mut inside = 0;
    for sy in 0..SAMPLES {
        for sx in 0..SAMPLES {
            let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
            let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
            // Distance past the rounded corner's centre on each axis.
            let dx = (radius - px).max(px - (side - radius)).max(0.0);
            let dy = (radius - py).max(py - (side - radius)).max(0.0);
            if dx * dx + dy * dy <= radius * radius {
                inside += 1;
            }
        }
    }
    inside as f32 / (SAMPLES * SAMPLES) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pixel_names_a_palette_colour() {
        for row in ICON_PIXELS {
            for pixel in row {
                assert!(pixel.is_ascii_digit());
                assert!(usize::from(pixel - b'0') <= ICON_PALETTE.len());
            }
        }
    }

    #[test]
    fn scaled_icon_keeps_whole_pixels() {
        let scale = 4;
        let side = ICON_GRID * scale;
        let rgba = icon_rgba(scale);
        assert_eq!(rgba.len(), side * side * 4);
        let at = |x: usize, y: usize| &rgba[(y * side + x) * 4..(y * side + x) * 4 + 4];
        // The rounded corner stays transparent and its edge is partly covered;
        // the tile and the current frame are opaque.
        assert_eq!(at(0, 0)[3], 0);
        let edge = at(3, 4)[3];
        assert!(edge > 0 && edge < 255, "{edge}");
        assert_eq!(at(side / 2, 0), ICON_PALETTE[0]);
        assert_eq!(
            at(10 * scale, side / 2),
            ICON_PALETTE[ICON_PALETTE.len() - 1]
        );
        // Every grid pixel is a uniform block.
        assert_eq!(
            at(10 * scale, side / 2),
            at(10 * scale + scale - 1, side / 2 + scale - 1)
        );
    }
}
