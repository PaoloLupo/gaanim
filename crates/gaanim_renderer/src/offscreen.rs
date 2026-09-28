//! Vello scenes rendered into images on the preview's own render device.
//!
//! Spawn [`VelloImageRender`] with an image made by [`vello_target_image`]:
//! each frame the entity exists, the render world rasterizes its scene into
//! the image with the renderer the canvas uses. Pair it with Bevy's
//! `Readback` to get the pixels back, as the web player's Presenter View does
//! for its cue previews: a browser has no threads for a second renderer.

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue, render_system};
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp, RenderSystems};
use vello::{AaConfig, RenderParams};

use crate::canvas::VelloRenderer;

/// Rasterize `scene`, in the image's pixels, over `base_color` into `image`.
#[derive(Component, Clone, ExtractComponent)]
pub struct VelloImageRender {
    pub scene: Arc<vello::Scene>,
    pub image: Handle<Image>,
    pub base_color: vello::peniko::Color,
}

/// An image Vello can render into and Bevy can read back.
pub fn vello_target_image(width: u32, height: u32) -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    image
}

/// Tightly packed RGBA rows from a texture readback, whose rows are padded
/// to the copy alignment.
pub fn unpad_rgba_rows(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row = width as usize * 4;
    let stride = row.div_ceil(256) * 256;
    if stride == row || height <= 1 {
        return data[..(row * height as usize).min(data.len())].to_vec();
    }
    data.chunks(stride)
        .take(height as usize)
        .flat_map(|chunk| &chunk[..row.min(chunk.len())])
        .copied()
        .collect()
}

fn render_images(
    renders: Query<&VelloImageRender>,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    renderer: Res<VelloRenderer>,
) {
    let Ok(mut renderer) = renderer.lock() else {
        return;
    };
    for render in &renders {
        let Some(target) = images.get(&render.image) else {
            continue;
        };
        let size = target.texture_descriptor.size;
        if let Err(error) = renderer.render_to_texture(
            device.wgpu_device(),
            &queue,
            &render.scene,
            &target.texture_view,
            &RenderParams {
                base_color: render.base_color,
                width: size.width,
                height: size.height,
                antialiasing_method: AaConfig::Area,
            },
        ) {
            error!("Vello failed to render an image: {error}");
        }
    }
}

pub(crate) struct VelloImagePlugin;

impl Plugin for VelloImagePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<VelloImageRender>::default());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        // Before the frame's commands, which carry the readback copies.
        render_app.add_systems(
            Render,
            render_images
                .in_set(RenderSystems::Render)
                .before(render_system)
                .run_if(resource_exists::<VelloRenderer>),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_rows_lose_their_padding() {
        // 3 pixels: 12 bytes per row, padded to 256.
        let mut data = vec![0u8; 256 * 2];
        data[..12].copy_from_slice(&[1; 12]);
        data[256..268].copy_from_slice(&[2; 12]);
        let rgba = unpad_rgba_rows(&data, 3, 2);
        assert_eq!(rgba.len(), 24);
        assert_eq!(&rgba[..12], &[1; 12]);
        assert_eq!(&rgba[12..], &[2; 12]);
        // 64 pixels fill the alignment exactly.
        let exact = vec![7u8; 256 * 2];
        assert_eq!(unpad_rgba_rows(&exact, 64, 2).len(), 512);
    }
}
