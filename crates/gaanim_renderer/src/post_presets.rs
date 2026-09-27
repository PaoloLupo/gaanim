//! Built-in finishing passes for the post-process chain: film grain,
//! vignette, chromatic aberration, color grading, lookup tables, halftone,
//! ordered dithering, CRT, pixelation, glitch and bloom.
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
    pub const ALL: [Self; 10] = [
        Self::Grain,
        Self::Vignette,
        Self::ChromaticAberration,
        Self::ColorGrade,
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
                    if *preset == Self::Bloom {
                        PostProcessShader::with_bloom(source, preset.uniforms())
                    } else {
                        PostProcessShader::with_uniforms(source, preset.uniforms())
                    }
                    .expect("built-in post-process presets are valid WGSL")
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
        let text = std::fs::read_to_string(path).map_err(|error| PostProcessError::ReadSource {
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
            Err(PostProcessError::ReadSource { .. })
        ));
    }
}
