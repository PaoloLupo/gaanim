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

use bevy::prelude::Component;
use gaanim_core::kurbo;
use gaanim_core::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use vello::wgpu;
use vello::{AaConfig, RenderParams, Scene};

use crate::post_process::{CanvasPostProcess, GpuPostProcess, PostProcessRequest};

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
    fn the_same_drawable_and_size_reuse_the_image() {
        let first = effect_image(7, 10, 20);
        assert_eq!(first.data.id(), effect_image(7, 10, 20).data.id());
        assert_ne!(first.data.id(), effect_image(7, 11, 20).data.id());
    }
}
