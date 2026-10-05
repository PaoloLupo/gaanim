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

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use bevy::prelude::{Component, Entity};
use gaanim_core::kurbo;
use gaanim_core::peniko::{Blob, Fill, ImageAlphaType, ImageData, ImageFormat};
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
/// as `edge` (0 to 1). `twist` also slides what the rim shows along the
/// outline, clockwise for positive values (to the right along the top, to
/// the left along the bottom), as a share of the bend.
/// `transparency` (0 to 1) is how clear the glass is:
/// 1 shows what is behind it, 0 turns it into milky white. The drawable
/// itself is drawn above, so a translucent fill tints the glass.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    pub blur: f64,
    pub saturation: f64,
    pub refraction: f64,
    pub edge: f64,
    pub dispersion: f64,
    pub bevel: f64,
    pub transparency: f64,
    pub twist: f64,
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
            transparency: 1.0,
            twist: 0.0,
        }
    }
}

impl Glass {
    /// Clear glass that bends and splits what is behind it along a wide
    /// rounded rim, like Apple's Liquid Glass.
    pub const LIQUID: Self = Self {
        blur: 0.04,
        saturation: 1.4,
        refraction: 1.0,
        edge: 0.8,
        dispersion: 0.12,
        bevel: 0.6,
        transparency: 0.92,
        twist: 0.6,
    };

    /// How far beyond its outline the glass reads what is behind it.
    pub fn reach(&self) -> f64 {
        let bend = self.refraction.max(0.0) * (1.0 + self.twist.abs().min(2.0));
        3.0 * self.blur.max(0.0) + bend * (1.0 + self.dispersion.max(0.0))
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
    if (top) {
        return sum / total;
    }
    // The outline keeps its sharp copy in red and its rim in alpha.
    return vec4<f32>(glass_sample(p, resolution, false).rgb, sum.a / total);
}
"#;

const GLASS_FINISH: &str = r#"
fn glass_outline(p: vec2<f32>, resolution: vec2<f32>) -> vec4<f32> {
    return glass_sample(p + vec2<f32>(0.0, 0.5 * resolution.y), resolution, false);
}

fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let p = uv * resolution;
    if (p.y >= 0.5 * resolution.y) {
        return gaanim_scene(uv);
    }
    // The blurred outline is the height of the lens: 1/2 on the outline,
    // rising to 1 inside across the rim; its slope points inward.
    let bevel = max(gaanim_uniforms.bevel, 1.0);
    let h = max(1.0, 0.2 * bevel);
    let slope = vec2<f32>(
        glass_outline(p + vec2<f32>(h, 0.0), resolution).a - glass_outline(p - vec2<f32>(h, 0.0), resolution).a,
        glass_outline(p + vec2<f32>(0.0, h), resolution).a - glass_outline(p - vec2<f32>(0.0, h), resolution).a,
    );
    let inward = select(vec2<f32>(0.0), normalize(slope), length(slope) > 1e-6);
    // 0 on the outline, 1 where the flat middle of the lens starts.
    let depth = clamp(2.0 * glass_outline(p, resolution).a - 1.0, 0.0, 1.0);
    // Like the steep side of a dome, the rim bends most at the outline and
    // leaves the middle untouched.
    let bend = gaanim_uniforms.refraction * (1.0 - depth);
    let split = gaanim_uniforms.dispersion;
    // The rim shows what lies outside the glass, pulled in like a lens.
    // Along the outline too: clockwise for a positive twist.
    let along = vec2<f32>(-inward.y, inward.x);
    let shift = (-inward + along * gaanim_uniforms.twist) * bend;
    let red = glass_sample(p + shift * (1.0 + split), resolution, true);
    let green = glass_sample(p + shift, resolution, true);
    let blue = glass_sample(p + shift * (1.0 - split), resolution, true);
    var color = vec3<f32>(red.r, green.g, blue.b);
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(luma), color, gaanim_uniforms.saturation);
    // Less transparent glass turns milky: white with a hint of what is behind.
    let milk = mix(vec3<f32>(0.96), color, 0.12);
    color = mix(milk, color, gaanim_uniforms.transparency);
    // A thin bright line along the outline, strongest where it faces the
    // light from the top left or the opposite corner, and a faint glow
    // inside the rim.
    let e = 1.5;
    let sharp = vec2<f32>(
        glass_outline(p + vec2<f32>(e, 0.0), resolution).r - glass_outline(p - vec2<f32>(e, 0.0), resolution).r,
        glass_outline(p + vec2<f32>(0.0, e), resolution).r - glass_outline(p - vec2<f32>(0.0, e), resolution).r,
    );
    let line = clamp(length(sharp) * 1.5, 0.0, 1.0);
    let facing = abs(dot(inward, normalize(vec2<f32>(-1.0, -1.0))));
    let shine = gaanim_uniforms.edge * (line * (0.45 + 0.55 * facing) + 0.05 * pow(1.0 - depth, 4.0));
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
            [
                "bevel",
                "refraction",
                "dispersion",
                "saturation",
                "edge",
                "transparency",
                "twist",
            ],
        )
        .ok()?;
        Some((blur, finish))
    }) else {
        return Vec::new();
    };
    let pixels = |value: f64| (value.max(0.0) * density) as f32;
    let (sigma, bevel) = (pixels(glass.blur), pixels(glass.bevel).max(1.0));
    // The outline blurs so its height reaches the flat middle `bevel` in
    // from the outline.
    let rim = bevel / 1.5;
    vec![
        (blur.clone(), vec![sigma, rim, 1.0]),
        (blur.clone(), vec![sigma, rim, 0.0]),
        (
            finish.clone(),
            vec![
                bevel,
                pixels(glass.refraction),
                glass.dispersion.clamp(0.0, 1.0) as f32,
                glass.saturation.max(0.0) as f32,
                glass.edge.clamp(0.0, 1.0) as f32,
                glass.transparency.clamp(0.0, 1.0) as f32,
                glass.twist.clamp(-2.0, 2.0) as f32,
            ],
        ),
    ]
}

/// One drawable to draw through its shader effect this frame.
#[derive(Clone)]
pub struct EffectLayer {
    /// What the drawable and its descendants draw, in world coordinates.
    /// Shared, so publishing the layer to the render world copies a pointer.
    pub scene: Arc<Scene>,
    /// Maps world coordinates onto the texture's pixels (Y down).
    pub to_pixels: kurbo::Affine,
    /// The image the frame draws in the drawable's place; its size is the
    /// texture's.
    pub image: ImageData,
    /// The passes, with the frame set to the whole texture.
    pub request: PostProcessRequest,
    /// The image always holds the same pixels (a soft effect of one built
    /// fragment), so its texture is drawn once and kept while it is drawn.
    pub fixed: bool,
}

impl EffectLayer {
    /// Whether this layer draws the same image as `other`.
    pub fn same_output(&self, other: &Self) -> bool {
        self.to_pixels == other.to_pixels
            && self.image == other.image
            && self.fixed == other.fixed
            && crate::canvas::draws_same(&self.scene, &other.scene)
            && self.request.same_output(&other.request)
    }
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

/// Compositions on a thread an image may go undrawn before
/// [`effect_image`] forgets it. Several scenes composed in turn on one
/// thread keep their images; despawned drawables release theirs.
const IMAGE_RETENTION: u32 = 16;

/// An image of [`effect_image`] and the compositions since it was last drawn.
struct CachedImage {
    image: ImageData,
    idle: u32,
}

thread_local! {
    static IMAGES: RefCell<HashMap<u64, CachedImage>> = RefCell::new(HashMap::new());
}

/// The image drawn in place of the drawable `key` at this size. The same
/// key and size return the same image, so Vello refreshes one atlas slot.
/// Images are kept per thread: two scenes composed at once, such as an
/// export and a recording, share entity keys but never images.
pub fn effect_image(key: u64, width: u32, height: u32) -> ImageData {
    IMAGES.with_borrow_mut(|images| effect_image_in(images, key, width, height))
}

/// Call once per composition, before its effects take their images: forgets
/// the images this thread has not drawn for [`IMAGE_RETENTION`]
/// compositions, so drawables that are gone do not keep their pixels.
pub fn age_effect_images() {
    IMAGES.with_borrow_mut(age_images);
}

fn age_images(images: &mut HashMap<u64, CachedImage>) {
    images.retain(|_, cached| {
        cached.idle += 1;
        cached.idle <= IMAGE_RETENTION
    });
}

fn effect_image_in(
    images: &mut HashMap<u64, CachedImage>,
    key: u64,
    width: u32,
    height: u32,
) -> ImageData {
    if let Some(cached) = images.get_mut(&key)
        && cached.image.width == width
        && cached.image.height == height
    {
        cached.idle = 0;
        return cached.image.clone();
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
    images.insert(
        key,
        CachedImage {
            image: image.clone(),
            idle: 0,
        },
    );
    image
}

struct Slot {
    image: ImageData,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    post: GpuPostProcess,
    /// The texture holds the pixels of a fixed layer.
    drawn: bool,
}

/// Runs the shader effects of a frame on the GPU, before the frame itself
/// is rendered with the same Vello renderer.
#[derive(Default)]
pub struct ObjectEffects {
    device: Option<wgpu::Device>,
    /// Textures by image id.
    slots: HashMap<u64, Slot>,
    /// Textures of images no longer drawn, kept for a new image of their
    /// size: an image's identity can change while its drawable does not.
    spare: Vec<Slot>,
    /// Where batches of several layers are drawn; see [`Self::render_batch`].
    atlas: Option<Atlas>,
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
            self.spare.clear();
            self.atlas = None;
            self.device = Some(device.clone());
        }
        let gone: Vec<u64> = self
            .slots
            .keys()
            .copied()
            .filter(|id| !layers.iter().any(|layer| layer.image.data.id() == *id))
            .collect();
        for id in gone {
            if let Some(slot) = self.slots.remove(&id) {
                renderer.override_image(&slot.image, None);
                self.spare.push(slot);
            }
        }
        for layer in layers {
            let spare = &mut self.spare;
            self.slots.entry(layer.image.data.id()).or_insert_with(|| {
                let (width, height) = (layer.image.width, layer.image.height);
                let mut slot = match spare
                    .iter()
                    .position(|slot| (slot.image.width, slot.image.height) == (width, height))
                {
                    Some(index) => spare.swap_remove(index),
                    None => {
                        let texture = effect_texture(device, width, height);
                        Slot {
                            image: layer.image.clone(),
                            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                            texture,
                            // A drawable whose image changed size takes the
                            // passes of the image it replaces, whose
                            // pipelines took about 0.5 ms each to build.
                            post: spare.pop().map(|slot| slot.post).unwrap_or_default(),
                            drawn: false,
                        }
                    }
                };
                slot.image = layer.image.clone();
                slot.drawn = false;
                renderer.override_image(
                    &slot.image,
                    Some(wgpu::TexelCopyTextureInfoBase {
                        texture: slot.texture.clone(),
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    }),
                );
                slot
            });
        }
        // Spare textures last one frame: a drawable that changed its image
        // has taken one back by now.
        self.spare.clear();

        // Fixed layers drawn by an earlier frame keep their pixels, in their
        // textures and in Vello's image atlas, which recopies an image only
        // when it is marked dirty.
        let pending: Vec<EffectLayer> = layers
            .iter()
            .filter(|layer| {
                !(layer.fixed
                    && self
                        .slots
                        .get(&layer.image.data.id())
                        .is_some_and(|slot| slot.drawn))
            })
            .cloned()
            .collect();
        let atlas_side = device.limits().max_texture_dimension_2d.min(MAX_ATLAS_SIDE);
        let mut start = 0;
        while start < pending.len() {
            let batch = next_batch(&pending, start, atlas_side);
            self.render_batch(
                device,
                queue,
                renderer,
                &pending[start..batch.end],
                &batch,
                antialiasing,
            )?;
            // A later batch may draw these images (glass shows what is
            // behind it), so its render copies them.
            for layer in &pending[start..batch.end] {
                if let Some(slot) = self.slots.get_mut(&layer.image.data.id()) {
                    renderer.mark_override_image_dirty(&slot.image);
                    slot.drawn = layer.fixed;
                }
            }
            start = batch.end;
        }
        // Each render consumes the pending copies of overridden images, so
        // the images are marked only once every texture is drawn: the frame
        // then copies all of them.
        for layer in &pending {
            if let Some(slot) = self.slots.get(&layer.image.data.id()) {
                renderer.mark_override_image_dirty(&slot.image);
            }
        }
        Ok(())
    }

    /// Draw the layers of one batch into their textures and apply their
    /// passes. A single layer is drawn into its texture directly; several
    /// share one Vello render into the atlas, since Vello's cost is mostly
    /// per render rather than per pixel, and are copied out of it.
    fn render_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        layers: &[EffectLayer],
        batch: &Batch,
        antialiasing: AaConfig,
    ) -> Result<(), vello::Error> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gaanim-object-effect"),
        });
        if let [layer] = layers {
            let slot = &self.slots[&layer.image.data.id()];
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
        } else {
            let atlas = self.atlas.take().filter(|atlas| atlas.fits(batch.extent));
            let atlas = atlas.unwrap_or_else(|| Atlas::new(device, batch.extent));
            let mut placed = Scene::new();
            for (layer, &(x, y)) in layers.iter().zip(&batch.origins) {
                let (x, y) = (f64::from(x), f64::from(y));
                let area = kurbo::Rect::new(
                    x,
                    y,
                    x + f64::from(layer.image.width),
                    y + f64::from(layer.image.height),
                );
                // What a layer draws past its texture stays out of its
                // neighbours, as the edge of its own texture would cut it.
                placed.push_clip_layer(Fill::NonZero, kurbo::Affine::IDENTITY, &area);
                placed.append(
                    &layer.scene,
                    Some(kurbo::Affine::translate((x, y)) * layer.to_pixels),
                );
                placed.pop_layer();
            }
            renderer.render_to_texture(
                device,
                queue,
                &placed,
                &atlas.view,
                &RenderParams {
                    base_color: vello::peniko::Color::TRANSPARENT,
                    width: atlas.size.0,
                    height: atlas.size.1,
                    antialiasing_method: antialiasing,
                },
            )?;
            for (layer, &(x, y)) in layers.iter().zip(&batch.origins) {
                let slot = &self.slots[&layer.image.data.id()];
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &atlas.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x, y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    slot.texture.as_image_copy(),
                    wgpu::Extent3d {
                        width: layer.image.width,
                        height: layer.image.height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            self.atlas = Some(atlas);
        }
        for layer in layers {
            let Some(slot) = self.slots.get_mut(&layer.image.data.id()) else {
                continue;
            };
            if slot
                .post
                .prepare(device, queue, &slot.texture, Some(&layer.request), None)
            {
                slot.post.encode(&mut encoder);
            }
        }
        queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Whether the last frame drew any effect.
    pub fn is_active(&self) -> bool {
        !self.slots.is_empty()
    }
}

/// Largest side of the atlas that batches of layers are drawn into.
const MAX_ATLAS_SIDE: u32 = 8192;
/// The atlas grows in steps of this many pixels, so that batches of
/// slightly different sizes reuse it.
const ATLAS_STEP: u32 = 512;

/// Consecutive layers drawn with one Vello render, and where each one sits
/// in the atlas.
#[derive(Debug, PartialEq)]
struct Batch {
    /// One past the last layer of the batch.
    end: usize,
    /// The top-left corner of each layer's texture in the atlas.
    origins: Vec<(u32, u32)>,
    /// The size of the atlas area the batch covers.
    extent: (u32, u32),
}

/// The layers from `start` on that can share one render, placed in rows of
/// an atlas no wider or taller than `side`. The batch ends before a layer
/// that does not fit, or that draws the image of a layer already in it,
/// which must be finished first.
fn next_batch(layers: &[EffectLayer], start: usize, side: u32) -> Batch {
    let mut origins = Vec::new();
    let mut drawn = Vec::new();
    let (mut x, mut y, mut row_height, mut width) = (0_u32, 0_u32, 0_u32, 0_u32);
    for layer in &layers[start..] {
        let (w, h) = (layer.image.width, layer.image.height);
        if !origins.is_empty() && draws_any_image(&layer.scene, &drawn) {
            break;
        }
        let (mut at_x, mut at_y) = (x, y);
        if at_x + w > side {
            (at_x, at_y) = (0, y + row_height);
        }
        if !origins.is_empty() && (at_x + w > side || at_y + h > side) {
            break;
        }
        if at_y != y {
            row_height = 0;
        }
        origins.push((at_x, at_y));
        drawn.push(layer.image.data.id());
        (x, y) = (at_x + w, at_y);
        row_height = row_height.max(h);
        width = width.max(x);
    }
    Batch {
        end: start + origins.len(),
        origins,
        extent: (width, y + row_height),
    }
}

/// Whether `scene` draws any of the images `ids`.
fn draws_any_image(scene: &Scene, ids: &[u64]) -> bool {
    scene.encoding().resources.patches.iter().any(|patch| {
        matches!(patch, vello_encoding::Patch::Image { image, .. } if ids.contains(&image.data.id()))
    })
}

/// The texture several layers are drawn into with one render.
struct Atlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
}

impl Atlas {
    fn new(device: &wgpu::Device, (width, height): (u32, u32)) -> Self {
        let step = |length: u32| length.max(1).div_ceil(ATLAS_STEP) * ATLAS_STEP;
        let size = (step(width), step(height));
        let texture = effect_texture(device, size.0, size.1);
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture,
            size,
        }
    }

    /// Whether `extent` fits without leaving most of the atlas unused.
    fn fits(&self, (width, height): (u32, u32)) -> bool {
        let (atlas_width, atlas_height) = self.size;
        let used = u64::from(width.max(ATLAS_STEP)) * u64::from(height.max(ATLAS_STEP));
        width <= atlas_width
            && height <= atlas_height
            && used * 4 >= u64::from(atlas_width) * u64::from(atlas_height)
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
        assert_eq!(passes[0].1, vec![20.0, 20.0, 1.0]);
        assert_eq!(passes[1].1[2], 0.0);
        assert_eq!(passes[2].1[1], 10.0);
        assert!(Glass::LIQUID.reach() > Glass::default().refraction);
    }

    fn layer(key: u64, width: u32, height: u32, draws: &[&EffectLayer]) -> EffectLayer {
        let mut scene = Scene::new();
        for drawn in draws {
            scene.draw_image(&drawn.image, kurbo::Affine::IDENTITY);
        }
        EffectLayer {
            scene: Arc::new(scene),
            to_pixels: kurbo::Affine::IDENTITY,
            image: effect_image(key, width, height),
            request: PostProcessRequest {
                passes: Vec::new(),
                frame: kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
                time: 0.0,
                transition: None,
            },
            fixed: false,
        }
    }

    #[test]
    fn independent_layers_share_rows_of_the_atlas() {
        let layers: Vec<_> = (0..3).map(|key| layer(100 + key, 300, 200, &[])).collect();
        let batch = next_batch(&layers, 0, 700);
        assert_eq!(batch.end, 3);
        assert_eq!(batch.origins, vec![(0, 0), (300, 0), (0, 200)]);
        assert_eq!(batch.extent, (600, 400));
    }

    #[test]
    fn a_layer_that_draws_another_of_its_batch_starts_the_next() {
        let card = layer(110, 100, 100, &[]);
        let other = layer(111, 100, 100, &[]);
        let glass = layer(112, 200, 100, &[&card]);
        let layers = vec![card, other, glass];
        let first = next_batch(&layers, 0, 4096);
        assert_eq!(first.end, 2, "the glass waits for the card it shows");
        let second = next_batch(&layers, first.end, 4096);
        assert_eq!((second.end, second.origins), (3, vec![(0, 0)]));
    }

    #[test]
    fn a_batch_ends_before_a_layer_that_does_not_fit() {
        let layers = vec![layer(120, 600, 600, &[]), layer(121, 600, 600, &[])];
        assert_eq!(next_batch(&layers, 0, 1000).end, 1);
        assert_eq!(next_batch(&layers, 0, 1200).end, 2, "side by side");
        let alone = next_batch(&layers, 1, 1000);
        assert_eq!((alone.end, alone.extent), (2, (600, 600)));
    }

    #[test]
    fn the_same_drawable_and_size_reuse_the_image() {
        let first = effect_image(7, 10, 20);
        assert_eq!(first.data.id(), effect_image(7, 10, 20).data.id());
        assert_ne!(first.data.id(), effect_image(7, 11, 20).data.id());
    }

    #[test]
    fn images_not_drawn_for_a_while_are_forgotten() {
        let mut images = HashMap::new();
        let kept = effect_image_in(&mut images, 1, 4, 4);
        effect_image_in(&mut images, 2, 4, 4);
        for _ in 0..IMAGE_RETENTION * 2 {
            age_images(&mut images);
            effect_image_in(&mut images, 1, 4, 4);
        }
        assert!(!images.contains_key(&2), "an undrawn image is released");
        let again = effect_image_in(&mut images, 1, 4, 4);
        assert_eq!(kept.data.id(), again.data.id(), "a drawn image is kept");
    }
}
