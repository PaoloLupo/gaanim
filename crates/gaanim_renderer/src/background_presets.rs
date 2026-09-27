//! Built-in living backgrounds: WGSL generated for [`ShaderBackground`].
//!
//! Every preset bakes its colors and seed into the shader source, so the same
//! arguments always produce the same image at a given timeline time.

use crate::background::{ShaderBackground, ShaderBackgroundError};
use gaanim_core::peniko::Color;
use std::fmt::Write;

/// Most colors a gradient preset accepts.
pub const MAX_BACKGROUND_COLORS: usize = 8;

/// Hash, value noise, fBm and color conversions shared by the presets.
const HELPERS: &str = r#"
fn ga_hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + vec2<f32>(33.33));
    return fract((r.x + r.y) * r.x);
}

fn ga_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (vec2<f32>(3.0) - 2.0 * f);
    let a = ga_hash(cell);
    let b = ga_hash(cell + vec2<f32>(1.0, 0.0));
    let c = ga_hash(cell + vec2<f32>(0.0, 1.0));
    let d = ga_hash(cell + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Five octaves of value noise, normalized to [0, 1].
fn ga_fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var q = p;
    for (var octave = 0; octave < 5; octave++) {
        value += amplitude * ga_noise(q);
        q = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * q + vec2<f32>(17.0, 9.2);
        amplitude *= 0.5;
    }
    return value / 0.96875;
}

fn ga_to_srgb(linear: vec3<f32>) -> vec3<f32> {
    let c = clamp(linear, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(
        1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055),
        12.92 * c,
        c <= vec3<f32>(0.0031308),
    );
}
"#;

/// Why a living background preset rejected its arguments.
#[derive(Clone, Debug, thiserror::Error)]
pub enum BackgroundPresetError {
    #[error("{name} needs between {min} and {MAX_BACKGROUND_COLORS} colors, got {count}")]
    Colors {
        name: &'static str,
        min: usize,
        count: usize,
    },
    #[error("{name} must be {expected}, got {value}")]
    Value {
        name: &'static str,
        expected: &'static str,
        value: f64,
    },
    #[error(transparent)]
    Shader(#[from] ShaderBackgroundError),
}

/// Smooth color blobs drifting on seeded orbits, blended like a mesh gradient.
// The seeded layout uses 6.283, not `TAU`; the approved look depends on it.
#[allow(clippy::approx_constant)]
pub fn mesh_gradient(
    colors: &[Color],
    speed: f64,
    seed: u32,
) -> Result<ShaderBackground, BackgroundPresetError> {
    check_colors("mesh_gradient", colors, 2)?;
    check_finite("speed", speed)?;
    let mut points = String::new();
    for index in 0..colors.len() {
        let [x, y, phase_x, phase_y] = seeded(seed, index as u32);
        // Golden-angle spread keeps the blobs apart before they start drifting.
        let angle = index as f64 * 2.399_963 + x * 6.283;
        let radius = 0.18 + 0.22 * y;
        let _ = writeln!(
            points,
            "        vec4<f32>({}, {}, {}, {}),",
            wgsl(angle.cos() * radius * 1.4),
            wgsl(angle.sin() * radius),
            wgsl(phase_x * 6.283),
            wgsl(phase_y * 6.283),
        );
    }
    let count = colors.len();
    let source = format!(
        r#"{HELPERS}
var<private> GA_COLORS: array<vec3<f32>, {count}> = array<vec3<f32>, {count}>(
{colors});
var<private> GA_POINTS: array<vec4<f32>, {count}> = array<vec4<f32>, {count}>(
{points});

fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {{
    let aspect = resolution.x / resolution.y;
    let t = time * {speed};
    var p = (uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0);
    p += 0.12 * (vec2<f32>(
        ga_noise(p * 1.8 + vec2<f32>(t, {offset})),
        ga_noise(p * 1.8 + vec2<f32>({offset}, -t)),
    ) - vec2<f32>(0.5));
    var total = vec3<f32>(0.0);
    var weight = 0.0;
    for (var index = 0; index < {count}; index++) {{
        let point = GA_POINTS[index];
        let center = point.xy + vec2<f32>(
            0.22 * aspect * sin(t * 2.1 + point.z),
            0.18 * cos(t * 1.7 + point.w),
        );
        let offset = p - center;
        let w = 1.0 / pow(dot(offset, offset) + 0.015, 1.6);
        total += GA_COLORS[index] * w;
        weight += w;
    }}
    return vec4<f32>(ga_to_srgb(total / weight), 1.0);
}}
"#,
        colors = linear_colors(colors),
        speed = wgsl(speed),
        offset = wgsl(seeded::<1>(seed, 97)[0] * 40.0),
    );
    Ok(ShaderBackground::new(source, colors[0])?)
}

/// Domain-warped fBm noise mapped across a color ramp.
pub fn noise_gradient(
    colors: &[Color],
    scale: f64,
    speed: f64,
    seed: u32,
) -> Result<ShaderBackground, BackgroundPresetError> {
    check_colors("noise_gradient", colors, 2)?;
    check_positive("scale", scale)?;
    check_finite("speed", speed)?;
    let count = colors.len();
    let [ox, oy, wx, wy] = seeded(seed, 0);
    let source = format!(
        r#"{HELPERS}
var<private> GA_COLORS: array<vec3<f32>, {count}> = array<vec3<f32>, {count}>(
{colors});

fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {{
    let aspect = resolution.x / resolution.y;
    let t = time * {speed};
    let q = (uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0) * {scale}
        + vec2<f32>({ox}, {oy});
    let warp = vec2<f32>(
        ga_fbm(q + vec2<f32>(0.0, t)),
        ga_fbm(q + vec2<f32>({wx}, {wy} - t)),
    );
    let n = smoothstep(0.18, 0.82, ga_fbm(q + 2.2 * warp + vec2<f32>(0.5 * t, 0.0)));
    let position = n * f32({last});
    let index = min(u32(position), {last}u);
    let next = min(index + 1u, {last}u);
    let color = mix(GA_COLORS[index], GA_COLORS[next], position - f32(index));
    return vec4<f32>(ga_to_srgb(color), 1.0);
}}
"#,
        colors = linear_colors(colors),
        speed = wgsl(speed),
        scale = wgsl(scale),
        ox = wgsl(ox * 50.0),
        oy = wgsl(oy * 50.0),
        wx = wgsl(5.2 + wx * 10.0),
        wy = wgsl(1.3 + wy * 10.0),
        last = count - 1,
    );
    Ok(ShaderBackground::new(source, colors[0])?)
}

/// Northern-lights curtains, one per color, over a dark sky.
pub fn aurora(
    colors: &[Color],
    sky: Color,
    speed: f64,
    seed: u32,
) -> Result<ShaderBackground, BackgroundPresetError> {
    check_colors("aurora", colors, 1)?;
    check_finite("speed", speed)?;
    let count = colors.len();
    let mut offsets = String::new();
    for index in 0..count {
        let [a, b] = seeded(seed, index as u32);
        let _ = writeln!(
            offsets,
            "        vec2<f32>({}, {}),",
            wgsl(a * 30.0),
            wgsl(0.18 + 0.2 * b),
        );
    }
    let source = format!(
        r#"{HELPERS}
var<private> GA_COLORS: array<vec3<f32>, {count}> = array<vec3<f32>, {count}>(
{colors});
// Noise offset (x) and resting height (y, from the top) of each curtain.
var<private> GA_CURTAINS: array<vec2<f32>, {count}> = array<vec2<f32>, {count}>(
{offsets});
const GA_SKY = vec3<f32>({sky});

fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {{
    let aspect = resolution.x / resolution.y;
    let t = time * {speed};
    let x = uv.x * aspect;
    // The sky brightens slightly toward the horizon.
    var color = GA_SKY * (0.6 + 0.8 * uv.y);
    for (var index = 0; index < {count}; index++) {{
        let curtain = GA_CURTAINS[index];
        let fold = x * 1.3 + curtain.x;
        let top = curtain.y + 0.22 * (ga_fbm(vec2<f32>(fold * 0.7, t * 0.35 + curtain.x)) - 0.5);
        let below = uv.y - top;
        // A sharp upper edge and a long glow hanging down from it.
        let edge = exp(-below * below * 900.0);
        let hang = select(exp(-below * 5.5), 0.0, below < 0.0);
        let rays = 0.45 + 0.55 * ga_noise(vec2<f32>(fold * 22.0, t * 0.8 + f32(index) * 3.1));
        let shimmer = 0.75 + 0.25 * sin(fold * 3.0 + t * 1.4 + f32(index));
        color += GA_COLORS[index] * (0.9 * edge + 0.55 * hang * rays) * shimmer;
    }}
    return vec4<f32>(ga_to_srgb(color), 1.0);
}}
"#,
        colors = linear_colors(colors),
        sky = linear_rgb(sky),
        speed = wgsl(speed),
    );
    Ok(ShaderBackground::new(source, sky)?)
}

/// Round dots on a square grid measured in scene units, optionally drifting.
pub fn dot_grid(
    spacing: f64,
    radius: f64,
    color: Color,
    background: Color,
    drift: (f64, f64),
) -> Result<ShaderBackground, BackgroundPresetError> {
    check_positive("spacing", spacing)?;
    check_positive("radius", radius)?;
    if radius > spacing / 2.0 {
        return Err(BackgroundPresetError::Value {
            name: "radius",
            expected: "at most half the spacing",
            value: radius,
        });
    }
    check_finite("drift x", drift.0)?;
    check_finite("drift y", drift.1)?;
    let dot_alpha = color.components[3];
    let source = format!(
        r#"{HELPERS}
fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {{
    let frame = gaanim_frame_size(resolution);
    // World units, y-up, with a dot on the scene origin.
    let position = (uv - vec2<f32>(0.5)) * frame * vec2<f32>(1.0, -1.0)
        - vec2<f32>({drift_x}, {drift_y}) * time;
    let cell = fract(position / {spacing} + vec2<f32>(0.5)) - vec2<f32>(0.5);
    let dist = length(cell) * {spacing};
    let pixel = frame.y / resolution.y;
    let coverage = 1.0 - smoothstep({radius} - 0.75 * pixel, {radius} + 0.75 * pixel, dist);
    let color = mix(vec3<f32>({background}), vec3<f32>({dot}), coverage * {dot_alpha});
    return vec4<f32>(ga_to_srgb(color), 1.0);
}}
"#,
        spacing = wgsl(spacing),
        radius = wgsl(radius),
        drift_x = wgsl(drift.0),
        drift_y = wgsl(drift.1),
        background = linear_rgb(background),
        dot = linear_rgb(color),
        dot_alpha = wgsl(f64::from(dot_alpha)),
    );
    Ok(ShaderBackground::new(source, background)?)
}

fn check_colors(
    name: &'static str,
    colors: &[Color],
    min: usize,
) -> Result<(), BackgroundPresetError> {
    if colors.len() < min || colors.len() > MAX_BACKGROUND_COLORS {
        return Err(BackgroundPresetError::Colors {
            name,
            min,
            count: colors.len(),
        });
    }
    Ok(())
}

fn check_finite(name: &'static str, value: f64) -> Result<(), BackgroundPresetError> {
    if value.is_finite() && value.abs() <= 1.0e4 {
        return Ok(());
    }
    Err(BackgroundPresetError::Value {
        name,
        expected: "a finite number",
        value,
    })
}

fn check_positive(name: &'static str, value: f64) -> Result<(), BackgroundPresetError> {
    if value.is_finite() && value > 0.0 && value <= 1.0e4 {
        return Ok(());
    }
    Err(BackgroundPresetError::Value {
        name,
        expected: "a positive finite number",
        value,
    })
}

/// `N` deterministic values in [0, 1) for `seed` and `index`.
fn seeded<const N: usize>(seed: u32, index: u32) -> [f64; N] {
    let mut state = u64::from(seed) << 32 | u64::from(index);
    std::array::from_fn(|_| {
        // SplitMix64.
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1_u64 << 53) as f64
    })
}

/// A WGSL float literal.
fn wgsl(value: f64) -> String {
    format!("{value:.6}")
}

/// Linear-light RGB components, as WGSL arguments.
fn linear_rgb(color: Color) -> String {
    let [r, g, b, _] = color.components;
    let linear = |c: f32| {
        let c = f64::from(c.clamp(0.0, 1.0));
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    format!(
        "{}, {}, {}",
        wgsl(linear(r)),
        wgsl(linear(g)),
        wgsl(linear(b))
    )
}

fn linear_colors(colors: &[Color]) -> String {
    colors
        .iter()
        .map(|color| format!("        vec3<f32>({}),\n", linear_rgb(*color)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAVY: Color = Color::from_rgb8(0x0b, 0x10, 0x40);
    const PURPLE: Color = Color::from_rgb8(0x6a, 0x2c, 0xc8);
    const TEAL: Color = Color::from_rgb8(0x12, 0xb8, 0xa6);

    #[test]
    fn presets_build_valid_shaders() {
        let colors = [NAVY, PURPLE, TEAL];
        mesh_gradient(&colors, 0.1, 4).unwrap();
        noise_gradient(&colors, 1.5, 0.05, 1).unwrap();
        aurora(&colors[1..], NAVY, 0.2, 0).unwrap();
        dot_grid(0.4, 0.03, Color::WHITE, NAVY, (0.1, 0.0)).unwrap();
        mesh_gradient(&[NAVY; MAX_BACKGROUND_COLORS], 0.0, u32::MAX).unwrap();
    }

    #[test]
    fn presets_reject_invalid_arguments() {
        assert!(mesh_gradient(&[NAVY], 0.1, 0).is_err());
        assert!(noise_gradient(&[NAVY; MAX_BACKGROUND_COLORS + 1], 1.0, 0.0, 0).is_err());
        assert!(aurora(&[], NAVY, 0.2, 0).is_err());
        assert!(noise_gradient(&[NAVY, TEAL], 0.0, 0.0, 0).is_err());
        assert!(mesh_gradient(&[NAVY, TEAL], f64::NAN, 0).is_err());
        assert!(dot_grid(0.4, 0.3, Color::WHITE, NAVY, (0.0, 0.0)).is_err());
        assert!(dot_grid(-1.0, 0.1, Color::WHITE, NAVY, (0.0, 0.0)).is_err());
        assert!(dot_grid(0.4, 0.1, Color::WHITE, NAVY, (f64::INFINITY, 0.0)).is_err());
    }

    #[test]
    fn seeds_change_the_shader_deterministically() {
        let colors = [NAVY, PURPLE, TEAL];
        let a = mesh_gradient(&colors, 0.1, 1).unwrap();
        let b = mesh_gradient(&colors, 0.1, 1).unwrap();
        let c = mesh_gradient(&colors, 0.1, 2).unwrap();
        assert_eq!(a.source(), b.source());
        assert_ne!(a.source(), c.source());
    }
}
