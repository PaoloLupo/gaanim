//! Built-in shader transitions, in the spirit of gl-transitions.
//!
//! Each preset is a [`gaanim_scene::TransitionShader`] like one written by
//! hand: WGSL defining `transition(uv)` over `gaanim_from`, `gaanim_to` and
//! `progress`, with named `f32` uniforms. Every one is a pure function of its
//! inputs, so a seek renders the same frame as playback.

use gaanim_scene::TransitionShader;

/// A built-in shader transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionPreset {
    /// The outgoing segment rushes toward the viewer in a radial blur that
    /// resolves into the incoming one. `strength` (0..1) is the blur reach.
    CrossZoom,
    /// The incoming segment sweeps in along a direction while both warp
    /// toward the edge. `dx`, `dy` give the direction (scene axes, y up) and
    /// `smoothness` (0..1) the width of the edge.
    DirectionalWarp,
    /// Concentric waves ripple out from the center and dissolve into the
    /// incoming segment. `amplitude` (waves across the frame), `speed`.
    Ripple,
    /// Horizontal bands slip sideways with split color channels, cutting to
    /// the incoming segment at the middle. `strength` (0..1), `bands`, `seed`.
    GlitchDisplace,
    /// The incoming segment appears where a grayscale map is darkest first
    /// (see [`luma_map`]). `softness` (0..1) blurs the edge.
    Luma,
}

const HELPERS: &str = r#"
const GAANIM_PI: f32 = 3.14159265;

fn gaanim_transition_hash(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(0.1031, 0.1030));
    q = q + dot(q, q.yx + vec2<f32>(33.33));
    return fract((q.x + q.y) * q.x);
}
"#;

const CROSS_ZOOM: &str = r#"
fn transition(uv: vec2<f32>) -> vec4<f32> {
    let center = vec2<f32>(0.5);
    // The blur peaks at the middle, where the segments trade places.
    let reach = gaanim_uniforms.strength * sin(progress * GAANIM_PI);
    let blend = smoothstep(0.3, 0.7, progress);
    var color = vec4<f32>(0.0);
    for (var i = 0; i < 24; i = i + 1) {
        let scale = 1.0 - reach * f32(i) / 23.0;
        let q = center + (uv - center) * scale;
        color = color + mix(gaanim_from(q), gaanim_to(q), blend);
    }
    return color / 24.0;
}
"#;

const DIRECTIONAL_WARP: &str = r#"
fn transition(uv: vec2<f32>) -> vec4<f32> {
    // Scene directions point up; uv grows downward.
    var direction = vec2<f32>(gaanim_uniforms.dx, -gaanim_uniforms.dy);
    if (length(direction) < 1e-6) {
        direction = vec2<f32>(1.0, 0.0);
    }
    var v = normalize(direction);
    v = v / (abs(v.x) + abs(v.y));
    let smoothness = max(gaanim_uniforms.smoothness, 1e-3);
    let d = v.x * 0.5 + v.y * 0.5;
    let m = 1.0 - smoothstep(
        -smoothness,
        0.0,
        v.x * uv.x + v.y * uv.y - (d - 0.5 + progress * (1.0 + smoothness)),
    );
    return mix(
        gaanim_from((uv - 0.5) * (1.0 - m) + 0.5),
        gaanim_to((uv - 0.5) * m + 0.5),
        m,
    );
}
"#;

const RIPPLE: &str = r#"
fn transition(uv: vec2<f32>) -> vec4<f32> {
    let resolution = gaanim_resolution();
    let aspect = resolution.x / max(resolution.y, 1.0);
    let direction = (uv - 0.5) * vec2<f32>(aspect, 1.0);
    let distance = length(direction);
    let wave = sin(progress * distance * gaanim_uniforms.amplitude - progress * gaanim_uniforms.speed);
    // The waves die out as the incoming segment settles.
    let strength = sin(progress * GAANIM_PI) / 30.0;
    let offset = direction * (wave + 0.5) * strength / vec2<f32>(aspect, 1.0);
    return mix(gaanim_from(uv + offset), gaanim_to(uv + offset * 0.5), smoothstep(0.2, 1.0, progress));
}
"#;

const GLITCH_DISPLACE: &str = r#"
fn transition(uv: vec2<f32>) -> vec4<f32> {
    let amount = gaanim_uniforms.strength * sin(progress * GAANIM_PI);
    let bands = max(gaanim_uniforms.bands, 1.0);
    // Bands and their shifts change a dozen times over the transition.
    let step_index = floor(progress * 12.0);
    let band = floor(uv.y * bands);
    let noise = gaanim_transition_hash(vec2<f32>(band, step_index + gaanim_uniforms.seed * 17.0));
    let shift = (noise - 0.5) * amount * 0.4;
    let q = vec2<f32>(uv.x + shift, uv.y);
    let split = vec2<f32>(amount * 0.03, 0.0);
    let incoming = step(0.5, progress);
    let red = mix(gaanim_from(q + split), gaanim_to(q + split), incoming);
    let rest = mix(gaanim_from(q), gaanim_to(q), incoming);
    let blue = mix(gaanim_from(q - split), gaanim_to(q - split), incoming);
    return vec4<f32>(red.r, rest.g, blue.b, rest.a);
}
"#;

const LUMA: &str = r#"
fn gaanim_luma_at(x: i32, y: i32) -> f32 {
    let width = i32(gaanim_uniforms.map_width);
    let height = i32(gaanim_uniforms.map_height);
    let cx = clamp(x, 0, width - 1);
    let cy = clamp(y, 0, height - 1);
    return gaanim_data[cy * width + cx].x;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    // Bilinear sample of the reveal map stretched over the frame.
    let size = vec2<f32>(gaanim_uniforms.map_width, gaanim_uniforms.map_height);
    let p = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)) * size - 0.5;
    let base = floor(p);
    let f = p - base;
    let x = i32(base.x);
    let y = i32(base.y);
    let top = mix(gaanim_luma_at(x, y), gaanim_luma_at(x + 1, y), f.x);
    let bottom = mix(gaanim_luma_at(x, y + 1), gaanim_luma_at(x + 1, y + 1), f.x);
    let luma = mix(top, bottom, f.y);
    let softness = clamp(gaanim_uniforms.softness, 0.0, 1.0);
    let t = progress * (1.0 + softness);
    let m = smoothstep(luma, luma + softness + 1e-4, t);
    return mix(gaanim_from(uv), gaanim_to(uv), m);
}
"#;

/// Longest side of a luma reveal map, in samples.
pub const LUMA_MAP_MAX: u32 = 256;

impl TransitionPreset {
    pub const ALL: [Self; 5] = [
        Self::CrossZoom,
        Self::DirectionalWarp,
        Self::Ripple,
        Self::GlitchDisplace,
        Self::Luma,
    ];

    /// The name used by the Python API.
    pub fn name(self) -> &'static str {
        match self {
            Self::CrossZoom => "cross_zoom",
            Self::DirectionalWarp => "directional_warp",
            Self::Ripple => "ripple",
            Self::GlitchDisplace => "glitch_displace",
            Self::Luma => "luma",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.name() == name)
    }

    /// Uniform names and their default values, in declaration order.
    pub fn uniforms(self) -> &'static [(&'static str, f32)] {
        match self {
            Self::CrossZoom => &[("strength", 0.4)],
            Self::DirectionalWarp => &[("dx", 1.0), ("dy", 0.0), ("smoothness", 0.5)],
            Self::Ripple => &[("amplitude", 100.0), ("speed", 50.0)],
            Self::GlitchDisplace => &[("strength", 0.5), ("bands", 24.0), ("seed", 0.0)],
            Self::Luma => &[("softness", 0.1), ("map_width", 1.0), ("map_height", 1.0)],
        }
    }

    fn body(self) -> &'static str {
        match self {
            Self::CrossZoom => CROSS_ZOOM,
            Self::DirectionalWarp => DIRECTIONAL_WARP,
            Self::Ripple => RIPPLE,
            Self::GlitchDisplace => GLITCH_DISPLACE,
            Self::Luma => LUMA,
        }
    }

    /// WGSL source of the preset.
    pub fn source(self) -> String {
        format!("{HELPERS}\n{}", self.body())
    }

    /// The preset with `values` replacing defaults by name. `luma` needs its
    /// reveal map as `map` (see [`luma_map`]); other presets ignore it.
    /// Unknown names, and a luma preset without a map, are errors.
    pub fn shader(
        self,
        values: &[(&str, f32)],
        map: Option<LumaMap>,
    ) -> Result<TransitionShader, String> {
        let uniforms = self.uniforms();
        for (name, _) in values {
            let settable = uniforms.iter().any(|(uniform, _)| uniform == name)
                && !matches!(*name, "map_width" | "map_height");
            if !settable {
                let known: Vec<&str> = uniforms
                    .iter()
                    .map(|(uniform, _)| *uniform)
                    .filter(|uniform| !matches!(*uniform, "map_width" | "map_height"))
                    .collect();
                return Err(format!(
                    "{} has no setting {name:?}; it takes {}",
                    self.name(),
                    known.join(", ")
                ));
            }
        }
        let mut resolved: Vec<f32> = uniforms
            .iter()
            .map(|(name, default)| {
                values
                    .iter()
                    .rev()
                    .find_map(|(given, value)| (given == name).then_some(*value))
                    .unwrap_or(*default)
            })
            .collect();
        let data = match (self, map) {
            (Self::Luma, Some(map)) => {
                resolved[1] = map.width as f32;
                resolved[2] = map.height as f32;
                Some(map.samples)
            }
            (Self::Luma, None) => {
                return Err("the luma transition needs a grayscale image".to_string());
            }
            _ => None,
        };
        Ok(TransitionShader {
            source: self.source(),
            uniforms: uniforms.iter().map(|(name, _)| name.to_string()).collect(),
            values: resolved,
            data,
        })
    }
}

/// A grayscale reveal map for [`TransitionPreset::Luma`], one sample per
/// `vec4` (in `x`), row by row from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct LumaMap {
    pub width: u32,
    pub height: u32,
    pub samples: Vec<[f32; 4]>,
}

/// The luma of `image`, scaled to at most [`LUMA_MAP_MAX`] samples on its
/// longest side; `invert` reveals the brightest areas first.
pub fn luma_map(image: &image::DynamicImage, invert: bool) -> LumaMap {
    let image = if image.width().max(image.height()) > LUMA_MAP_MAX {
        image.resize(
            LUMA_MAP_MAX,
            LUMA_MAP_MAX,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image.clone()
    };
    let gray = image.to_luma8();
    let samples = gray
        .pixels()
        .map(|pixel| {
            let value = f32::from(pixel.0[0]) / 255.0;
            let value = if invert { 1.0 - value } else { value };
            [value, 0.0, 0.0, 0.0]
        })
        .collect();
    LumaMap {
        width: gray.width(),
        height: gray.height(),
        samples,
    }
}

/// [`luma_map`] of the image file at `path`.
pub fn luma_map_from_file(
    path: impl AsRef<std::path::Path>,
    invert: bool,
) -> Result<LumaMap, String> {
    let path = path.as_ref();
    let image = image::open(path)
        .map_err(|error| format!("could not read the luma image {}: {error}", path.display()))?;
    Ok(luma_map(&image, invert))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post_process::PostProcessShader;

    fn gradient_map() -> LumaMap {
        let image = image::DynamicImage::ImageLuma8(image::GrayImage::from_fn(4, 2, |x, _| {
            image::Luma([(x * 80) as u8])
        }));
        luma_map(&image, false)
    }

    #[test]
    fn every_preset_is_a_valid_transition() {
        for preset in TransitionPreset::ALL {
            let map = (preset == TransitionPreset::Luma).then(gradient_map);
            let description = preset.shader(&[], map).unwrap();
            let shader = PostProcessShader::transition(
                description.source.as_str(),
                description.uniforms.iter(),
                description
                    .data
                    .map(|data| std::sync::Arc::<[[f32; 4]]>::from(data.as_slice())),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", preset.name()));
            assert!(shader.is_transition());
            assert_eq!(TransitionPreset::from_name(preset.name()), Some(preset));
        }
    }

    #[test]
    fn settings_override_defaults_and_unknown_ones_fail() {
        let warp = TransitionPreset::DirectionalWarp
            .shader(&[("dy", 1.0), ("dx", 0.0)], None)
            .unwrap();
        assert_eq!(warp.values, [0.0, 1.0, 0.5]);
        let error = TransitionPreset::Ripple
            .shader(&[("strength", 1.0)], None)
            .unwrap_err();
        assert!(error.contains("amplitude, speed"), "{error}");
        assert!(TransitionPreset::Luma.shader(&[], None).is_err());
        assert!(
            TransitionPreset::Luma
                .shader(&[("map_width", 3.0)], Some(gradient_map()))
                .is_err()
        );
    }

    #[test]
    fn luma_maps_are_small_and_record_their_size() {
        let map = gradient_map();
        assert_eq!((map.width, map.height), (4, 2));
        assert_eq!(map.samples.len(), 8);
        assert_eq!(map.samples[1][0], 80.0 / 255.0);
        let large = image::DynamicImage::ImageLuma8(image::GrayImage::new(1024, 512));
        let scaled = luma_map(&large, true);
        assert_eq!((scaled.width, scaled.height), (256, 128));
        assert_eq!(scaled.samples[0][0], 1.0);
        let shader = TransitionPreset::Luma.shader(&[], Some(map)).unwrap();
        assert_eq!(shader.values, [0.1, 4.0, 2.0]);
    }
}
