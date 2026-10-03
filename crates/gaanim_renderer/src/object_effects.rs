//! Shader effects on one drawable: the drawable and its descendants are
//! drawn into a texture of their own, post-process passes run over it, and
//! the result is drawn back into the frame as an image, in the drawable's
//! place in draw order.
//!
//! Composition replaces the drawable's elements by an image fill
//! ([`EffectLayer::image`]) and keeps what they drew as
//! [`EffectLayer::scene`]. Before the frame is rendered, [`ObjectEffects`]
//! renders each layer's scene, applies its passes with
//! [`GpuPostProcess`] and registers the texture with Vello as the image's
//! pixels, as shader backgrounds do. A renderer that does not run the
//! effects composes the drawables plainly instead.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use bevy::prelude::{Component, Entity};
use gaanim_core::kurbo;
use gaanim_core::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use vello::wgpu;
use vello::{AaConfig, RenderParams, Scene};

use crate::post_process::{
    CanvasPostProcess, GpuPostProcess, PostProcessRequest, PostProcessShader,
};

/// Largest side, in pixels, of an effect's texture.
pub const MAX_EFFECT_TEXTURE: u32 = 4096;

/// Component: draw this drawable and its descendants through `post`'s
/// passes. `margin` (scene units) widens the texture around them for
/// effects that reach beyond their outline, such as a glow or a ripple.
#[derive(Component, Clone, Debug)]
pub struct ShaderEffect {
    pub post: CanvasPostProcess,
    pub margin: f64,
}

/// How a track matte shows the drawable it is set on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatteMode {
    /// Where the matte is opaque.
    Alpha,
    /// Where the matte is transparent.
    AlphaInverted,
    /// Where the matte is bright.
    Luma,
    /// Where the matte is dark or transparent.
    LumaInverted,
}

/// Component: show this drawable and its descendants only through `source`
/// (see [`MatteMode`]).
#[derive(Component, Clone, Copy, Debug)]
pub struct Matte {
    pub source: Entity,
    pub mode: MatteMode,
}

/// Component: this drawable is a matte; it is drawn only as the matte of
/// the drawables whose [`Matte`] names it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct MatteSource;

/// Component: glass. What is drawn behind this drawable shows through its
/// outline blurred by `blur` scene units (a Gaussian's sigma), with its
/// colors saturated by `saturation` (1 keeps them). The outline is a lens
/// with a rounded rim `bevel` scene units wide: across the rim, what is
/// behind bends by up to `refraction` scene units, splits into its colors
/// by `dispersion` (0 to 1) and catches a light from the top left as bright
/// as `edge` (0 to 1). The drawable itself is drawn above, so a translucent
/// fill tints the glass.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    pub blur: f64,
    pub saturation: f64,
    pub refraction: f64,
    pub edge: f64,
    pub dispersion: f64,
    pub bevel: f64,
}

impl Default for Glass {
    /// Frosted glass.
    fn default() -> Self {
        Self {
            blur: 0.25,
            saturation: 1.4,
            refraction: 0.08,
            edge: 0.3,
            dispersion: 0.0,
            bevel: 0.12,
        }
    }
}

impl Glass {
    /// Clear glass that bends and splits what is behind it along a wide
    /// rounded rim, like Apple's Liquid Glass.
    pub const LIQUID: Self = Self {
        blur: 0.03,
        saturation: 1.25,
        refraction: 0.3,
        edge: 0.7,
        dispersion: 0.3,
        bevel: 0.35,
    };

    /// How far beyond its outline the glass reads what is behind it.
    pub fn reach(&self) -> f64 {
        3.0 * self.blur.max(0.0) + self.refraction.max(0.0) * (1.0 + self.dispersion.max(0.0))
    }
}

/// Reads the texture of a glass layer: what is behind the glass in the top
/// half and its outline in the bottom half, each clamped to its own half.
const GLASS_COMMON: &str = r#"
fn glass_sample(p: vec2<f32>, resolution: vec2<f32>, top: bool) -> vec4<f32> {
    let half = 0.5 * resolution.y;
    let low = select(half + 0.5, 0.5, top);
    let high = select(resolution.y - 0.5, half - 0.5, top);
    let q = vec2<f32>(clamp(p.x, 0.5, resolution.x - 0.5), clamp(p.y, low, high));
    return gaanim_scene(q / resolution);
}
"#;

const GLASS_BLUR: &str = r#"
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let p = uv * resolution;
    let top = p.y < 0.5 * resolution.y;
    // What is behind blurs by `sigma`; the outline by `bevel` into a rim.
    let sigma = max(select(gaanim_uniforms.bevel, gaanim_uniforms.sigma, top), 0.001);
    if (sigma < 0.5) {
        return glass_sample(p, resolution, top);
    }
    let radius = ceil(3.0 * sigma);
    let step = max(1.0, radius / 32.0);
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -32; i <= 32; i++) {
        let x = f32(i) * step;
        if (abs(x) > radius) {
            continue;
        }
        let weight = exp(-0.5 * x * x / (sigma * sigma));
        let offset = select(vec2<f32>(0.0, x), vec2<f32>(x, 0.0), gaanim_uniforms.horizontal > 0.5);
        sum += glass_sample(p + offset, resolution, top) * weight;
        total += weight;
    }
    return sum / total;
}
"#;

const GLASS_FINISH: &str = r#"
fn glass_height(p: vec2<f32>, resolution: vec2<f32>) -> f32 {
    return glass_sample(p + vec2<f32>(0.0, 0.5 * resolution.y), resolution, false).a;
}

fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let p = uv * resolution;
    if (p.y >= 0.5 * resolution.y) {
        return gaanim_scene(uv);
    }
    // The blurred outline is the height of the lens: 1 inside, 0 outside,
    // sloping across the rim; its slope points inward.
    let bevel = max(gaanim_uniforms.bevel, 1.0);
    let h = max(1.0, 0.5 * bevel);
    let slope = vec2<f32>(
        glass_height(p + vec2<f32>(h, 0.0), resolution) - glass_height(p - vec2<f32>(h, 0.0), resolution),
        glass_height(p + vec2<f32>(0.0, h), resolution) - glass_height(p - vec2<f32>(0.0, h), resolution),
    ) / (2.0 * h);
    // About 1 on the outline, fading to 0 inside and outside.
    let rim = clamp(length(slope) * bevel * 2.5, 0.0, 1.0);
    let inward = select(vec2<f32>(0.0), normalize(slope), length(slope) > 1e-6);
    let bend = gaanim_uniforms.refraction * rim;
    let split = gaanim_uniforms.dispersion;
    // The rim shows what lies outside the glass, pulled in like a lens.
    let red = glass_sample(p - inward * bend * (1.0 + split), resolution, true);
    let green = glass_sample(p - inward * bend, resolution, true);
    let blue = glass_sample(p - inward * bend * (1.0 - split), resolution, true);
    var color = vec3<f32>(red.r, green.g, blue.b);
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(luma), color, gaanim_uniforms.saturation);
    // A light from the top left on the rim, and its glint opposite.
    let light = normalize(vec2<f32>(-1.0, -1.0));
    let facing = dot(-inward, light);
    let shine = gaanim_uniforms.edge * rim * (0.25 + 0.75 * pow(max(facing, 0.0), 2.0) + 0.4 * pow(max(-facing, 0.0), 4.0));
    color = clamp(color + vec3<f32>(shine), vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(color, green.a);
}
"#;

/// The passes that turn a glass layer's texture into its glass, at
/// `density` pixels per scene unit: the texture holds what is behind the
/// glass in its top half and the glass's outline, filled opaque, in the
/// bottom half.
pub fn glass_passes(glass: &Glass, density: f64) -> Vec<(PostProcessShader, Vec<f32>)> {
    static SHADERS: OnceLock<Option<(PostProcessShader, PostProcessShader)>> = OnceLock::new();
    let Some((blur, finish)) = SHADERS.get_or_init(|| {
        let blur = PostProcessShader::with_uniforms(
            format!("{GLASS_COMMON}{GLASS_BLUR}"),
            ["sigma", "bevel", "horizontal"],
        )
        .ok()?;
        let finish = PostProcessShader::with_uniforms(
            format!("{GLASS_COMMON}{GLASS_FINISH}"),
            ["bevel", "refraction", "dispersion", "saturation", "edge"],
        )
        .ok()?;
        Some((blur, finish))
    }) else {
        return Vec::new();
    };
    let pixels = |value: f64| (value.max(0.0) * density) as f32;
    let (sigma, bevel) = (pixels(glass.blur), pixels(glass.bevel).max(1.0));
    vec![
        (blur.clone(), vec![sigma, bevel, 1.0]),
        (blur.clone(), vec![sigma, bevel, 0.0]),
        (
            finish.clone(),
            vec![
                bevel,
                pixels(glass.refraction),
                glass.dispersion.clamp(0.0, 1.0) as f32,
                glass.saturation.max(0.0) as f32,
                glass.edge.clamp(0.0, 1.0) as f32,
            ],
        ),
    ]
}

/// One drawable to draw through its shader effect this frame.
#[derive(Clone)]
pub struct EffectLayer {
    /// What the drawable and its descendants draw, in world coordinates.
    pub scene: Scene,
    /// Maps world coordinates onto the texture's pixels (Y down).
    pub to_pixels: kurbo::Affine,
    /// The image the frame draws in the drawable's place; its size is the
    /// texture's.
    pub image: ImageData,
    /// The passes, with the frame set to the whole texture.
    pub request: PostProcessRequest,
}

/// Texture size for `bounds` (world units) at `pixels_per_unit`, and the
/// density actually used: large drawables are drawn at a lower density so
/// their texture stays within [`MAX_EFFECT_TEXTURE`].
pub fn effect_texture_size(bounds: kurbo::Rect, pixels_per_unit: f64) -> Option<(u32, u32, f64)> {
    if !(bounds.width() > 0.0 && bounds.height() > 0.0 && pixels_per_unit.is_finite())
        || pixels_per_unit <= 0.0
    {
        return None;
    }
    let largest = bounds.width().max(bounds.height()) * pixels_per_unit;
    let density = if largest > f64::from(MAX_EFFECT_TEXTURE) {
        pixels_per_unit * f64::from(MAX_EFFECT_TEXTURE) / largest
    } else {
        pixels_per_unit
    };
    let side = |length: f64| {
        (length * density)
            .ceil()
            .clamp(1.0, f64::from(MAX_EFFECT_TEXTURE)) as u32
    };
    Some((side(bounds.width()), side(bounds.height()), density))
}

/// The image drawn in place of the drawable `key` at this size. The same
/// key and size return the same image, so Vello refreshes one atlas slot.
pub fn effect_image(key: u64, width: u32, height: u32) -> ImageData {
    static IMAGES: OnceLock<Mutex<HashMap<u64, ImageData>>> = OnceLock::new();
    let mut images = IMAGES
        .get_or_init(Default::default)
        .lock()
        .expect("effect images poisoned");
    if let Some(image) = images.get(&key)
        && image.width == width
        && image.height == height
    {
        return image.clone();
    }
    // Vello copies the registered texture instead of reading these bytes; a
    // missing texture draws a transparent image.
    let image = ImageData {
        data: Blob::from(vec![0_u8; width as usize * height as usize * 4]),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    };
    images.insert(key, image.clone());
    image
}

struct Slot {
    image: ImageData,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    post: GpuPostProcess,
}

/// Runs the shader effects of a frame on the GPU, before the frame itself
/// is rendered with the same Vello renderer.
#[derive(Default)]
pub struct ObjectEffects {
    device: Option<wgpu::Device>,
    /// Textures by image id.
    slots: HashMap<u64, Slot>,
}

impl ObjectEffects {
    /// Draw every layer into its texture, apply its passes and register
    /// the texture as its image's pixels; release the textures of images
    /// the frame no longer draws. Call before rendering the frame.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        layers: &[EffectLayer],
        antialiasing: AaConfig,
    ) -> Result<(), vello::Error> {
        if self.device.as_ref() != Some(device) {
            for slot in self.slots.values() {
                renderer.override_image(&slot.image, None);
            }
            self.slots.clear();
            self.device = Some(device.clone());
        }
        self.slots.retain(|id, slot| {
            let keep = layers.iter().any(|layer| layer.image.data.id() == *id);
            if !keep {
                renderer.override_image(&slot.image, None);
            }
            keep
        });
        for layer in layers {
            let slot = self.slots.entry(layer.image.data.id()).or_insert_with(|| {
                let texture = effect_texture(device, layer.image.width, layer.image.height);
                renderer.override_image(
                    &layer.image,
                    Some(wgpu::TexelCopyTextureInfoBase {
                        texture: texture.clone(),
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    }),
                );
                Slot {
                    image: layer.image.clone(),
                    view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    texture,
                    post: GpuPostProcess::default(),
                }
            });
            let mut placed = Scene::new();
            placed.append(&layer.scene, Some(layer.to_pixels));
            renderer.render_to_texture(
                device,
                queue,
                &placed,
                &slot.view,
                &RenderParams {
                    base_color: vello::peniko::Color::TRANSPARENT,
                    width: layer.image.width,
                    height: layer.image.height,
                    antialiasing_method: antialiasing,
                },
            )?;
            if slot
                .post
                .prepare(device, queue, &slot.texture, Some(&layer.request), None)
            {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gaanim-object-effect"),
                });
                slot.post.encode(&mut encoder);
                queue.submit(Some(encoder.finish()));
            }
            // A later layer may draw this image (glass shows what is behind
            // it), so it is copied for the next render too.
            renderer.mark_override_image_dirty(&slot.image);
        }
        // Each render consumes the pending copies of overridden images, so
        // the images are marked only once every texture is drawn: the frame
        // then copies all of them.
        for layer in layers {
            if let Some(slot) = self.slots.get(&layer.image.data.id()) {
                renderer.mark_override_image_dirty(&slot.image);
            }
        }
        Ok(())
    }

    /// Whether the last frame drew any effect.
    pub fn is_active(&self) -> bool {
        !self.slots.is_empty()
    }
}

fn effect_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gaanim-object-effect"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        // Vello renders into it, the passes read and rewrite it, and Vello
        // copies it into its image atlas.
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_drawables_lower_their_density_to_fit_the_texture() {
        let (width, height, density) =
            effect_texture_size(kurbo::Rect::new(0.0, 0.0, 4.0, 2.0), 100.0).unwrap();
        assert_eq!((width, height), (400, 200));
        assert_eq!(density, 100.0);
        let (width, height, density) =
            effect_texture_size(kurbo::Rect::new(0.0, 0.0, 100.0, 10.0), 100.0).unwrap();
        assert_eq!(width, MAX_EFFECT_TEXTURE);
        assert!(height <= 410 && density < 100.0);
        assert!(effect_texture_size(kurbo::Rect::ZERO, 100.0).is_none());
    }

    #[test]
    fn glass_shaders_build_with_their_uniforms_in_pixels() {
        let glass = Glass {
            blur: 0.2,
            bevel: 0.3,
            refraction: 0.1,
            ..Glass::LIQUID
        };
        let passes = glass_passes(&glass, 100.0);
        assert_eq!(passes.len(), 3, "two blur passes and the finish");
        assert_eq!(passes[0].1, vec![20.0, 30.0, 1.0]);
        assert_eq!(passes[1].1[2], 0.0);
        assert_eq!(passes[2].1[1], 10.0);
        assert!(Glass::LIQUID.reach() > Glass::default().refraction);
    }

    #[test]
    fn the_same_drawable_and_size_reuse_the_image() {
        let first = effect_image(7, 10, 20);
        assert_eq!(first.data.id(), effect_image(7, 10, 20).data.id());
        assert_ne!(first.data.id(), effect_image(7, 11, 20).data.id());
    }
}
