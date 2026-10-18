//! Built-in finishing passes for the post-process chain: film grain,
//! vignette, chromatic aberration, color grading, tint and duotone, lookup
//! tables, halftone, ordered dithering, CRT, pixelation, glitch and bloom.
//!
//! Each preset is a [`PostProcessShader`] with named uniforms, so its
//! amounts can be constants or animated parameters. Sizes are measured in
//! pixels of a frame 1080 pixels tall, so a preset looks the same at any
//! export resolution. Noise is quantized to timeline frames of 1/24 s, so a
//! frame is a pure function of its time.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::post_process::{PostProcessError, PostProcessShader};

/// A built-in post-process with named uniforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostPreset {
    /// `amount`, `size` (pixels at 1080p), `animated` (0 or 1).
    Grain,
    /// `strength`, `softness`.
    Vignette,
    /// `amount` (fraction of the frame at its corners).
    ChromaticAberration,
    /// `exposure` (stops), `contrast`, `saturation`, `temperature`.
    ColorGrade,
    /// `red`, `green`, `blue` (the ink), `amount`.
    Tint,
    /// `shadow_red`, `shadow_green`, `shadow_blue`, `highlight_red`,
    /// `highlight_green`, `highlight_blue`, `amount`.
    Duotone,
    /// `dot` (cell size in pixels at 1080p).
    Halftone,
    /// `levels` (per channel, at least 2).
    Dither,
    /// `strength`.
    Crt,
    /// `size` (pixels at 1080p).
    Pixelate,
    /// `intensity`, `seed`.
    Glitch,
    /// `threshold` (sRGB brightness where glow starts), `intensity`,
    /// `radius` (0..1 spread).
    Bloom,
}

const HELPERS: &str = r#"
fn gaanim_preset_hash(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(0.1031, 0.1030));
    q = q + dot(q, q.yx + vec2<f32>(33.33));
    return fract((q.x + q.y) * q.x);
}

fn gaanim_preset_scale(resolution: vec2<f32>) -> f32 {
    return max(resolution.y / 1080.0, 1e-4);
}

fn gaanim_preset_luma(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
}
"#;

// Additive glow in linear light. Where the sum overflows white, the excess
// spills into the other channels, so hot cores turn white instead of clipping
// to a saturated hue.
const BLOOM: &str = r#"
fn gaanim_bloom_linear(color: vec3<f32>) -> vec3<f32> {
    return select(
        pow((color + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        color / 12.92,
        color <= vec3<f32>(0.04045),
    );
}

fn gaanim_bloom_srgb(linear: vec3<f32>) -> vec3<f32> {
    let c = clamp(linear, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(
        1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055),
        12.92 * c,
        c <= vec3<f32>(0.0031308),
    );
}

fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let scene = gaanim_scene(uv);
    let glow = gaanim_bloom(uv) * max(gaanim_uniforms.intensity, 0.0);
    var color = gaanim_bloom_linear(scene.rgb) * scene.a + glow;
    let alpha = clamp(scene.a + max(glow.r, max(glow.g, glow.b)), 0.0, 1.0);
    color = color / max(alpha, 1e-4);
    let over = max(max(color.r, max(color.g, color.b)) - 1.0, 0.0);
    color = min(color + vec3<f32>(0.6 * over), vec3<f32>(1.0));
    return vec4<f32>(gaanim_bloom_srgb(color), alpha);
}
"#;

const GRAIN: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    let size = max(gaanim_uniforms.size, 0.25) * gaanim_preset_scale(resolution);
    let cell = floor(uv * resolution / size);
    let frame = floor(time * 24.0) * clamp(gaanim_uniforms.animated, 0.0, 1.0);
    let noise = gaanim_preset_hash(cell + vec2<f32>(frame * 17.13, frame * 3.71)) - 0.5;
    return vec4<f32>(color.rgb + vec3<f32>(noise * gaanim_uniforms.amount), color.a);
}
"#;

const VIGNETTE: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    // 0 at the center and 1 at the corners.
    let distance = length(uv - vec2<f32>(0.5)) * 1.41421356;
    let softness = clamp(gaanim_uniforms.softness, 1e-3, 1.0);
    let t = clamp((distance - (1.0 - softness)) / softness, 0.0, 1.0);
    let shade = 1.0 - clamp(gaanim_uniforms.strength, 0.0, 1.0) * t * t * (3.0 - 2.0 * t);
    return vec4<f32>(color.rgb * shade, color.a);
}
"#;

const CHROMATIC_ABERRATION: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let offset = (uv - vec2<f32>(0.5)) * 2.0 * gaanim_uniforms.amount;
    let center = gaanim_scene(uv);
    let red = gaanim_scene(uv - offset).r;
    let blue = gaanim_scene(uv + offset).b;
    return vec4<f32>(red, center.g, blue, center.a);
}
"#;

const COLOR_GRADE: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    // Exposure and white balance in linear light.
    var linear = pow(max(color.rgb, vec3<f32>(0.0)), vec3<f32>(2.2));
    linear = linear * exp2(gaanim_uniforms.exposure);
    let warm = gaanim_uniforms.temperature * 0.1;
    linear = linear * vec3<f32>(1.0 + warm, 1.0, 1.0 - warm);
    var graded = pow(max(linear, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
    // Contrast about mid-gray and saturation about luma, in display values.
    graded = (graded - vec3<f32>(0.5)) * max(gaanim_uniforms.contrast, 0.0) + vec3<f32>(0.5);
    let luma = gaanim_preset_luma(graded);
    graded = mix(vec3<f32>(luma), graded, max(gaanim_uniforms.saturation, 0.0));
    return vec4<f32>(graded, color.a);
}
"#;

// Monochrome in one ink: black, the ink where the image is as bright as the
// ink, then white, so the image keeps its brightness.
const TINT: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    let ink = clamp(
        vec3<f32>(gaanim_uniforms.red, gaanim_uniforms.green, gaanim_uniforms.blue),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    let luma = clamp(gaanim_preset_luma(color.rgb), 0.0, 1.0);
    let middle = clamp(gaanim_preset_luma(ink), 1e-3, 1.0 - 1e-3);
    let tinted = select(
        mix(ink, vec3<f32>(1.0), (luma - middle) / (1.0 - middle)),
        ink * (luma / middle),
        luma <= middle,
    );
    return vec4<f32>(mix(color.rgb, tinted, clamp(gaanim_uniforms.amount, 0.0, 1.0)), color.a);
}
"#;

// Brightness mapped from the shadow color to the highlight color.
const DUOTONE: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    let shadows = vec3<f32>(
        gaanim_uniforms.shadow_red,
        gaanim_uniforms.shadow_green,
        gaanim_uniforms.shadow_blue,
    );
    let highlights = vec3<f32>(
        gaanim_uniforms.highlight_red,
        gaanim_uniforms.highlight_green,
        gaanim_uniforms.highlight_blue,
    );
    let luma = clamp(gaanim_preset_luma(color.rgb), 0.0, 1.0);
    let toned = mix(shadows, highlights, luma);
    return vec4<f32>(mix(color.rgb, toned, clamp(gaanim_uniforms.amount, 0.0, 1.0)), color.a);
}
"#;

const HALFTONE: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let size = max(gaanim_uniforms.dot, 1.0) * gaanim_preset_scale(resolution);
    let pixel = uv * resolution;
    let cell = floor(pixel / size);
    let center = (cell + vec2<f32>(0.5)) * size;
    let ink = gaanim_scene(center / resolution);
    // Brighter cells draw larger dots of their own color.
    let radius = sqrt(clamp(gaanim_preset_luma(ink.rgb), 0.0, 1.0)) * size * 0.7071;
    let coverage = clamp(radius - distance(pixel, center) + 0.5, 0.0, 1.0);
    return vec4<f32>(ink.rgb * coverage, ink.a);
}
"#;

const DITHER: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    let levels = max(floor(gaanim_uniforms.levels), 2.0) - 1.0;
    let p = vec2<u32>(floor(uv * resolution)) % vec2<u32>(4u);
    var bayer = array<f32, 16>(
        0.0, 8.0, 2.0, 10.0,
        12.0, 4.0, 14.0, 6.0,
        3.0, 11.0, 1.0, 9.0,
        15.0, 7.0, 13.0, 5.0,
    );
    let threshold = (bayer[p.y * 4u + p.x] + 0.5) / 16.0;
    let quantized = floor(color.rgb * levels + vec3<f32>(threshold)) / levels;
    return vec4<f32>(quantized, color.a);
}
"#;

const CRT: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let strength = clamp(gaanim_uniforms.strength, 0.0, 1.0);
    // Barrel distortion toward a curved screen.
    let centered = uv - vec2<f32>(0.5);
    let bent = centered * (1.0 + strength * 0.08 * dot(centered, centered) * 4.0);
    let screen = bent + vec2<f32>(0.5);
    if (any(screen < vec2<f32>(0.0)) || any(screen > vec2<f32>(1.0))) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let color = gaanim_scene(screen);
    let scale = gaanim_preset_scale(resolution);
    let row = screen.y * resolution.y / (3.0 * scale);
    let scanline = 1.0 - strength * 0.35 * (0.5 - 0.5 * cos(row * 6.28318530));
    let column = u32(floor(screen.x * resolution.x / scale)) % 3u;
    var mask = vec3<f32>(1.0 - strength * 0.2);
    mask[column] = 1.0;
    let edge = length(centered) * 1.41421356;
    let vignette = 1.0 - strength * 0.4 * edge * edge;
    return vec4<f32>(color.rgb * scanline * mask * vignette, color.a);
}
"#;

const PIXELATE: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let size = max(gaanim_uniforms.size, 1.0) * gaanim_preset_scale(resolution);
    let cell = floor(uv * resolution / size);
    return gaanim_scene((cell + vec2<f32>(0.5)) * size / resolution);
}
"#;

const GLITCH: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let intensity = clamp(gaanim_uniforms.intensity, 0.0, 1.0);
    let frame = floor(time * 12.0);
    let seed = gaanim_uniforms.seed;
    // Horizontal bands jump sideways on some frames.
    let band = floor(uv.y * 24.0);
    let roll = gaanim_preset_hash(vec2<f32>(band + seed * 7.1, frame + seed * 13.7));
    let jumps = step(1.0 - intensity * 0.6, roll);
    let shift = (gaanim_preset_hash(vec2<f32>(frame + seed, band * 3.3)) - 0.5) * 0.12 * intensity;
    let moved = vec2<f32>(fract(uv.x + shift * jumps), uv.y);
    let split = vec2<f32>(0.008 * intensity * (0.5 + roll), 0.0);
    let center = gaanim_scene(moved);
    let red = gaanim_scene(moved + split).r;
    let blue = gaanim_scene(moved - split).b;
    return vec4<f32>(red, center.g, blue, center.a);
}
"#;

const LUT: &str = r#"
fn gaanim_lut_at(r: u32, g: u32, b: u32, size: u32) -> vec3<f32> {
    return gaanim_data[r + g * size + b * size * size].rgb;
}

fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    let size = u32(gaanim_uniforms.size);
    let last = f32(size - 1u);
    let p = clamp(color.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * last;
    let i0 = vec3<u32>(floor(p));
    let i1 = min(i0 + vec3<u32>(1u), vec3<u32>(size - 1u));
    let f = p - floor(p);
    let c00 = mix(gaanim_lut_at(i0.x, i0.y, i0.z, size), gaanim_lut_at(i1.x, i0.y, i0.z, size), f.x);
    let c10 = mix(gaanim_lut_at(i0.x, i1.y, i0.z, size), gaanim_lut_at(i1.x, i1.y, i0.z, size), f.x);
    let c01 = mix(gaanim_lut_at(i0.x, i0.y, i1.z, size), gaanim_lut_at(i1.x, i0.y, i1.z, size), f.x);
    let c11 = mix(gaanim_lut_at(i0.x, i1.y, i1.z, size), gaanim_lut_at(i1.x, i1.y, i1.z, size), f.x);
    let graded = mix(mix(c00, c10, f.y), mix(c01, c11, f.y), f.z);
    return vec4<f32>(mix(color.rgb, graded, clamp(gaanim_uniforms.strength, 0.0, 1.0)), color.a);
}
"#;

impl PostPreset {
    pub const ALL: [Self; 12] = [
        Self::Grain,
        Self::Vignette,
        Self::ChromaticAberration,
        Self::ColorGrade,
        Self::Tint,
        Self::Duotone,
        Self::Halftone,
        Self::Dither,
        Self::Crt,
        Self::Pixelate,
        Self::Glitch,
        Self::Bloom,
    ];

    /// Uniform names, in the order the preset's values are given.
    pub fn uniforms(self) -> &'static [&'static str] {
        match self {
            Self::Grain => &["amount", "size", "animated"],
            Self::Vignette => &["strength", "softness"],
            Self::ChromaticAberration => &["amount"],
            Self::ColorGrade => &["exposure", "contrast", "saturation", "temperature"],
            Self::Tint => &["red", "green", "blue", "amount"],
            Self::Duotone => &[
                "shadow_red",
                "shadow_green",
                "shadow_blue",
                "highlight_red",
                "highlight_green",
                "highlight_blue",
                "amount",
            ],
            Self::Halftone => &["dot"],
            Self::Dither => &["levels"],
            Self::Crt => &["strength"],
            Self::Pixelate => &["size"],
            Self::Glitch => &["intensity", "seed"],
            Self::Bloom => &["threshold", "intensity", "radius"],
        }
    }

    fn body(self) -> &'static str {
        match self {
            Self::Grain => GRAIN,
            Self::Vignette => VIGNETTE,
            Self::ChromaticAberration => CHROMATIC_ABERRATION,
            Self::ColorGrade => COLOR_GRADE,
            Self::Tint => TINT,
            Self::Duotone => DUOTONE,
            Self::Halftone => HALFTONE,
            Self::Dither => DITHER,
            Self::Crt => CRT,
            Self::Pixelate => PIXELATE,
            Self::Glitch => GLITCH,
            Self::Bloom => BLOOM,
        }
    }

    /// The preset's shader, validated once per process.
    pub fn shader(self) -> PostProcessShader {
        static SHADERS: OnceLock<Vec<PostProcessShader>> = OnceLock::new();
        let shaders = SHADERS.get_or_init(|| {
            Self::ALL
                .iter()
                .map(|preset| {
                    let source = format!("{HELPERS}{}", preset.body());
                    let shader = if *preset == Self::Bloom {
                        PostProcessShader::with_bloom(source, preset.uniforms())
                    } else {
                        PostProcessShader::with_uniforms(source, preset.uniforms())
                    }
                    .expect("built-in post-process presets are valid WGSL");
                    // Still grain draws the same noise at every time.
                    if *preset == Self::Grain {
                        shader.time_gated_by("animated")
                    } else {
                        shader
                    }
                })
                .collect()
        });
        let index = Self::ALL
            .iter()
            .position(|preset| *preset == self)
            .expect("every preset is listed in ALL");
        shaders[index].clone()
    }
}

/// Most texels an image overlay keeps on its long side: a larger image is
/// downscaled when it is read, so its data stays a few megabytes.
pub const MAX_OVERLAY_SIDE: u32 = 2048;

/// How an image overlay composites with the frame beneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayBlend {
    /// The overlay over the frame.
    Normal,
    /// Darkens like ink or paper grain: white leaves the frame as it is.
    #[default]
    Multiply,
    /// Lightens: black leaves the frame as it is.
    Screen,
    /// Multiplies the frame's darks and screens its lights.
    Overlay,
}

impl OverlayBlend {
    /// Parses `"normal"`, `"multiply"`, `"screen"` and `"overlay"`.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "normal" => Some(Self::Normal),
            "multiply" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "overlay" => Some(Self::Overlay),
            _ => None,
        }
    }

    /// The `mode` uniform of the overlay shader.
    pub fn mode(self) -> f64 {
        match self {
            Self::Normal => 0.0,
            Self::Multiply => 1.0,
            Self::Screen => 2.0,
            Self::Overlay => 3.0,
        }
    }
}

/// Covers the frame with the overlay image, centered and cropped like CSS
/// `background-size: cover`, sampled bilinearly from four RGBA8 texels per
/// `gaanim_data` entry, and blends it with `mode` at `opacity`.
const OVERLAY: &str = r#"
fn gaanim_overlay_texel(x: i32, y: i32) -> vec4<f32> {
    let width = i32(gaanim_uniforms.width);
    let height = i32(gaanim_uniforms.height);
    let index = clamp(y, 0, height - 1) * width + clamp(x, 0, width - 1);
    let packed = gaanim_data[index / 4];
    return unpack4x8unorm(bitcast<u32>(packed[index % 4]));
}

fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    let size = vec2<f32>(gaanim_uniforms.width, gaanim_uniforms.height);
    let scale = max(resolution.x / size.x, resolution.y / size.y);
    let p = (uv * resolution - 0.5 * resolution) / scale + 0.5 * size - vec2<f32>(0.5);
    let base = floor(p);
    let f = p - base;
    let x = i32(base.x);
    let y = i32(base.y);
    let texel = mix(
        mix(gaanim_overlay_texel(x, y), gaanim_overlay_texel(x + 1, y), f.x),
        mix(gaanim_overlay_texel(x, y + 1), gaanim_overlay_texel(x + 1, y + 1), f.x),
        f.y,
    );
    let mode = gaanim_uniforms.mode;
    var blended = texel.rgb;
    if (mode > 0.5 && mode < 1.5) {
        blended = color.rgb * texel.rgb;
    } else if (mode > 1.5 && mode < 2.5) {
        blended = vec3<f32>(1.0) - (vec3<f32>(1.0) - color.rgb) * (vec3<f32>(1.0) - texel.rgb);
    } else if (mode > 2.5) {
        blended = select(
            vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - color.rgb) * (vec3<f32>(1.0) - texel.rgb),
            2.0 * color.rgb * texel.rgb,
            color.rgb <= vec3<f32>(0.5),
        );
    }
    let amount = clamp(gaanim_uniforms.opacity, 0.0, 1.0) * texel.a;
    return vec4<f32>(mix(color.rgb, blended, amount), color.a);
}
"#;

/// An image laid over the frame by a post-process pass (see [`OVERLAY`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageOverlay {
    pub width: u32,
    pub height: u32,
    /// Four RGBA8 texels per entry, their bits as `f32`s.
    pub data: Arc<[[f32; 4]]>,
}

impl ImageOverlay {
    /// Pack `rgba`, `width` by `height` straight-alpha RGBA8 texels with the
    /// top row first, at most [`MAX_OVERLAY_SIDE`] on either side.
    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8]) -> Result<Self, PostProcessError> {
        let invalid = |message: String| PostProcessError::InvalidUniforms(message);
        if width == 0 || height == 0 || width.max(height) > MAX_OVERLAY_SIDE {
            return Err(invalid(format!(
                "an overlay is 1 to {MAX_OVERLAY_SIDE} texels on each side, got {width}x{height}"
            )));
        }
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(invalid("overlay texels do not match its size".to_string()));
        }
        let data: Vec<[f32; 4]> = rgba
            .chunks(16)
            .map(|block| {
                let mut entry = [0.0_f32; 4];
                for (lane, texel) in block.chunks_exact(4).enumerate() {
                    let bits = u32::from_le_bytes([texel[0], texel[1], texel[2], texel[3]]);
                    entry[lane] = f32::from_bits(bits);
                }
                entry
            })
            .collect();
        Ok(Self {
            width,
            height,
            data: data.into(),
        })
    }

    /// The overlay's pass shader, whose uniforms are `width`, `height`,
    /// `mode` ([`OverlayBlend::mode`]) and `opacity`.
    pub fn shader(&self) -> Result<PostProcessShader, PostProcessError> {
        PostProcessShader::with_data(
            OVERLAY,
            ["width", "height", "mode", "opacity"],
            Arc::clone(&self.data),
        )
    }
}

/// A 3D lookup table parsed from an Adobe/Resolve `.cube` file.
#[derive(Debug, Clone, PartialEq)]
pub struct CubeLut {
    /// Entries per axis.
    pub size: usize,
    /// `size³` colors with red varying fastest, then green, then blue.
    pub colors: Arc<[[f32; 4]]>,
}

/// Largest `.cube` table accepted (per axis).
pub const MAX_LUT_SIZE: usize = 65;

impl CubeLut {
    /// Parse `LUT_3D_SIZE`, optional `DOMAIN_MIN`/`DOMAIN_MAX` and the color
    /// rows of a `.cube` file; comments, `TITLE` and 1D sections are rejected
    /// or ignored as the format prescribes.
    pub fn parse(text: &str) -> Result<Self, PostProcessError> {
        let invalid = PostProcessError::InvalidLut;
        let mut size = None;
        let mut domain_min = [0.0_f32; 3];
        let mut domain_max = [1.0_f32; 3];
        let mut colors = Vec::new();
        let triple = |parts: &[&str], line: usize| -> Result<[f32; 3], PostProcessError> {
            if parts.len() != 3 {
                return Err(invalid(format!("line {line}: expected three numbers")));
            }
            let mut values = [0.0_f32; 3];
            for (value, part) in values.iter_mut().zip(parts) {
                *value = part
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| invalid(format!("line {line}: {part:?} is not a number")))?;
            }
            Ok(values)
        };
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let content = raw.split('#').next().unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }
            let parts: Vec<&str> = content.split_whitespace().collect();
            match parts[0] {
                "TITLE" => {}
                "LUT_1D_SIZE" => {
                    return Err(invalid("1D tables are not supported".to_string()));
                }
                "LUT_3D_SIZE" => {
                    let value = parts
                        .get(1)
                        .and_then(|value| value.parse::<usize>().ok())
                        .filter(|value| (2..=MAX_LUT_SIZE).contains(value))
                        .ok_or_else(|| {
                            invalid(format!("LUT_3D_SIZE must be between 2 and {MAX_LUT_SIZE}"))
                        })?;
                    size = Some(value);
                }
                "DOMAIN_MIN" => domain_min = triple(&parts[1..], line)?,
                "DOMAIN_MAX" => domain_max = triple(&parts[1..], line)?,
                keyword if keyword.chars().next().is_some_and(char::is_alphabetic) => {}
                _ => {
                    let [r, g, b] = triple(&parts, line)?;
                    colors.push([r, g, b]);
                }
            }
        }
        let size = size.ok_or_else(|| invalid("missing LUT_3D_SIZE".to_string()))?;
        if colors.len() != size * size * size {
            return Err(invalid(format!(
                "expected {} colors for LUT_3D_SIZE {size}, found {}",
                size * size * size,
                colors.len()
            )));
        }
        if domain_min
            .iter()
            .zip(&domain_max)
            .any(|(min, max)| max <= min)
        {
            return Err(invalid("DOMAIN_MAX must exceed DOMAIN_MIN".to_string()));
        }
        // Normalize outputs to the [0, 1] range the shader writes.
        let colors = colors
            .into_iter()
            .map(|color| {
                let mut out = [0.0_f32; 4];
                for axis in 0..3 {
                    out[axis] =
                        (color[axis] - domain_min[axis]) / (domain_max[axis] - domain_min[axis]);
                }
                out[3] = 1.0;
                out
            })
            .collect();
        Ok(Self { size, colors })
    }

    /// Read and parse a `.cube` file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, PostProcessError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|error| PostProcessError::ReadLut {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Self::parse(&text)
    }

    /// The LUT pass shader, with uniforms `size` (fixed to this table) and
    /// `strength`.
    pub fn shader(&self) -> Result<PostProcessShader, PostProcessError> {
        PostProcessShader::with_data(
            format!("{HELPERS}{LUT}"),
            ["size", "strength"],
            self.colors.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlays_pack_four_texels_per_entry() {
        let texels: Vec<u8> = (0..5 * 4).map(|value| value as u8 * 13).collect();
        let overlay = ImageOverlay::from_rgba8(5, 1, &texels).unwrap();
        // Five texels take two entries; the last one's lanes are unused.
        assert_eq!(overlay.data.len(), 2);
        let bits = overlay.data[1][0].to_bits();
        assert_eq!(
            bits.to_le_bytes(),
            [texels[16], texels[17], texels[18], texels[19]]
        );
        assert!(overlay.shader().is_ok());
        assert!(ImageOverlay::from_rgba8(0, 1, &[]).is_err());
        assert!(ImageOverlay::from_rgba8(2, 1, &texels).is_err());
        assert!(ImageOverlay::from_rgba8(MAX_OVERLAY_SIDE + 1, 1, &[0; 4]).is_err());
        assert_eq!(OverlayBlend::parse("screen"), Some(OverlayBlend::Screen));
        assert_eq!(OverlayBlend::parse("darken"), None);
    }

    #[test]
    fn an_overlay_shader_equals_itself_when_its_texels_read_as_nan() {
        // Opaque blue packs to 0xffff0000, a NaN as f32. Bundles find a
        // frame's shader in their table by equality.
        let blue = [0, 0, 255, 255];
        let shader = || {
            ImageOverlay::from_rgba8(1, 1, &blue)
                .unwrap()
                .shader()
                .unwrap()
        };
        assert!(shader().data().unwrap()[0][0].is_nan());
        let first = shader();
        assert_eq!(first, first.clone());
        assert_eq!(first, shader());
        let red = ImageOverlay::from_rgba8(1, 1, &[255, 0, 0, 255]).unwrap();
        assert_ne!(first, red.shader().unwrap());
    }

    #[test]
    fn every_preset_is_valid_wgsl_with_its_uniforms() {
        for preset in PostPreset::ALL {
            let shader = preset.shader();
            let names: Vec<&str> = shader.uniforms().iter().map(|name| name.as_ref()).collect();
            assert_eq!(names, preset.uniforms(), "{preset:?}");
        }
    }

    #[test]
    fn cube_files_parse_into_normalized_tables() {
        let identity = "TITLE \"id\"\n# comment\nLUT_3D_SIZE 2\n\
            0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let lut = CubeLut::parse(identity).unwrap();
        assert_eq!(lut.size, 2);
        assert_eq!(lut.colors[1], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(lut.colors[6], [0.0, 1.0, 1.0, 1.0]);
        assert!(lut.shader().is_ok());

        let scaled = "LUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 2 2 2\n\
            0 0 0\n2 0 0\n0 2 0\n2 2 0\n0 0 2\n2 0 2\n0 2 2\n2 2 2\n";
        assert_eq!(
            CubeLut::parse(scaled).unwrap().colors[7],
            [1.0, 1.0, 1.0, 1.0]
        );

        for bad in [
            "0 0 0\n",
            "LUT_3D_SIZE 1\n0 0 0\n",
            "LUT_3D_SIZE 2\n0 0 0\n",
            "LUT_3D_SIZE 2\n0 0\n",
            "LUT_3D_SIZE 2\n0 0 x\n",
            "LUT_1D_SIZE 4\n",
            "LUT_3D_SIZE 99\n",
        ] {
            assert!(CubeLut::parse(bad).is_err(), "{bad:?}");
        }
        assert!(matches!(
            CubeLut::from_file("/nonexistent/grade.cube"),
            Err(error @ PostProcessError::ReadLut { .. })
                if error.to_string().starts_with("could not read .cube LUT")
        ));
    }
}
