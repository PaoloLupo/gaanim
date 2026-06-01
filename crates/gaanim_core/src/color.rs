use peniko::Color;

/// Linearly interpolate between two `peniko::Color` values.
///
/// This performs component-wise interpolation in RGBA8 space for consistent,
/// perceptually-adequate results across the entire engine.
///
/// # Arguments
/// * `from` - The starting color.
/// * `to` - The ending color.
/// * `t` - Interpolation factor in `[0.0, 1.0]` (values are clamped).
pub fn interpolate_color(from: Color, to: Color, t: f64) -> Color {
    let from_rgba = from.to_rgba8();
    let to_rgba = to.to_rgba8();
    let t = t.clamp(0.0, 1.0) as f32;

    let r = (from_rgba.r as f32 + (to_rgba.r as f32 - from_rgba.r as f32) * t).clamp(0.0, 255.0) as u8;
    let g = (from_rgba.g as f32 + (to_rgba.g as f32 - from_rgba.g as f32) * t).clamp(0.0, 255.0) as u8;
    let b = (from_rgba.b as f32 + (to_rgba.b as f32 - from_rgba.b as f32) * t).clamp(0.0, 255.0) as u8;
    let a = (from_rgba.a as f32 + (to_rgba.a as f32 - from_rgba.a as f32) * t).clamp(0.0, 255.0) as u8;

    Color::from_rgba8(r, g, b, a)
}

/// Linearly interpolate between two RGBA8 colors represented as `(r, g, b, a)` tuples.
///
/// This is a lower-level helper used when you already have decomposed bytes.
pub fn interpolate_rgba8(
    from: (u8, u8, u8, u8),
    to: (u8, u8, u8, u8),
    t: f64,
) -> (u8, u8, u8, u8) {
    let t = t.clamp(0.0, 1.0) as f32;
    let r = (from.0 as f32 + (to.0 as f32 - from.0 as f32) * t).clamp(0.0, 255.0) as u8;
    let g = (from.1 as f32 + (to.1 as f32 - from.1 as f32) * t).clamp(0.0, 255.0) as u8;
    let b = (from.2 as f32 + (to.2 as f32 - from.2 as f32) * t).clamp(0.0, 255.0) as u8;
    let a = (from.3 as f32 + (to.3 as f32 - from.3 as f32) * t).clamp(0.0, 255.0) as u8;
    (r, g, b, a)
}
