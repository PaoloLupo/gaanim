//! Vello render targets of any size.
//!
//! Vello sorts a target's paths into square bins of [`BIN`] pixels and keeps
//! one entry per bin in arrays of [`MAX_BINS`]. Its shaders run without
//! bounds checks, so a target with more bins writes past those arrays and
//! loses the GPU device. 4096 by 4096 pixels is the largest square that
//! fits; an 8K frame does not. [`render_to_texture`] draws a larger target in
//! bands that fit, each through a texture of its own copied into place.

use std::cell::RefCell;

use gaanim_core::kurbo;
use vello::{RenderParams, Renderer, Scene, wgpu};

/// Side, in pixels, of the square bins Vello sorts a target's paths into.
const BIN: u32 = 256;
/// Most bins one Vello render may cover.
pub(crate) const MAX_BINS: u32 = 256;

/// The bins Vello divides a `width` by `height` target into.
pub(crate) const fn bins(width: u32, height: u32) -> u32 {
    width.div_ceil(BIN) * height.div_ceil(BIN)
}

/// The rectangles (x, y, width, height) that cover a `width` by `height`
/// target in rows from the top, each within [`MAX_BINS`].
fn bands(width: u32, height: u32) -> Vec<(u32, u32, u32, u32)> {
    let across = width.div_ceil(BIN).clamp(1, MAX_BINS);
    let (band_width, band_height) = (across * BIN, MAX_BINS / across * BIN);
    let mut bands = Vec::new();
    for y in (0..height).step_by(band_height as usize) {
        for x in (0..width).step_by(band_width as usize) {
            bands.push((x, y, band_width.min(width - x), band_height.min(height - y)));
        }
    }
    bands
}

/// The texture bands are drawn into before they are copied, kept for the
/// next frame of the same size.
struct BandTexture {
    device: wgpu::Device,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

thread_local! {
    static BAND: RefCell<Option<BandTexture>> = const { RefCell::new(None) };
}

/// Render `scene` into the `params.width` by `params.height` top-left area of
/// `target` (whose view is `view`), as [`Renderer::render_to_texture`] does,
/// in bands when the area has more bins than one render may cover. A banded
/// target needs [`wgpu::TextureUsages::COPY_SRC`] and
/// [`wgpu::TextureUsages::COPY_DST`].
///
/// Vello writes every pixel of a render or, when a stage ran out of buffer,
/// none. A band starts from the target's own pixels, so a band Vello skips
/// leaves its part of the target as it was, as a skipped whole render would:
/// sentinel pixels written before the render still mark the skip.
pub fn render_to_texture(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    target: &wgpu::Texture,
    view: &wgpu::TextureView,
    params: &RenderParams,
) -> Result<(), vello::Error> {
    if bins(params.width, params.height) <= MAX_BINS {
        return renderer.render_to_texture(device, queue, scene, view, params);
    }
    debug_assert!(
        target
            .usage()
            .contains(wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST)
    );
    let bands = bands(params.width, params.height);
    let size = bands
        .iter()
        .fold((1, 1), |(w, h), band| (w.max(band.2), h.max(band.3)));
    BAND.with_borrow_mut(|cached| {
        let band = match cached.take() {
            Some(band)
                if band.device == *device
                    && (band.texture.width(), band.texture.height()) == size
                    && band.texture.format() == target.format() =>
            {
                band
            }
            _ => {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("gaanim-vello-band"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: target.format(),
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                BandTexture {
                    device: device.clone(),
                    view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    texture,
                }
            }
        };
        for &(x, y, width, height) in &bands {
            let region = |texture| wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            };
            let extent = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            // Submissions to one queue run in order: the band holds the
            // target's pixels when Vello draws it, and is copied back before
            // the next band draws into the same texture.
            let copy = |from: wgpu::TexelCopyTextureInfo, to: wgpu::TexelCopyTextureInfo| {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gaanim-vello-band"),
                });
                encoder.copy_texture_to_texture(from, to, extent);
                queue.submit(Some(encoder.finish()));
            };
            copy(region(target), band.texture.as_image_copy());
            let mut placed = Scene::new();
            placed.append(
                scene,
                Some(kurbo::Affine::translate((-f64::from(x), -f64::from(y)))),
            );
            renderer.render_to_texture(
                device,
                queue,
                &placed,
                &band.view,
                &RenderParams {
                    base_color: params.base_color,
                    width,
                    height,
                    antialiasing_method: params.antialiasing_method,
                },
            )?;
            copy(band.texture.as_image_copy(), region(target));
        }
        *cached = Some(band);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_within_the_bins_render_whole() {
        assert_eq!(bins(4096, 4096), MAX_BINS);
        assert_eq!(bins(3840, 2160), 135);
        assert_eq!(bins(8192, 2048), MAX_BINS);
        assert_eq!(bins(7680, 3072), 360);
    }

    #[test]
    fn larger_targets_split_into_bands_that_cover_them() {
        for (width, height) in [
            (7680, 4320),
            (7680, 3600),
            (15360, 8640),
            (70000, 300),
            (300, 70000),
        ] {
            let bands = bands(width, height);
            let mut area = 0;
            for &(x, y, w, h) in &bands {
                assert!(bins(w, h) <= MAX_BINS, "{w}x{h} in {width}x{height}");
                assert!(x + w <= width && y + h <= height);
                area += u64::from(w) * u64::from(h);
            }
            assert_eq!(area, u64::from(width) * u64::from(height));
        }
        // An 8K frame: 30 bins across leave rows of 8 bins.
        let eight_k = bands(7680, 4320);
        assert_eq!(eight_k.len(), 3);
        assert_eq!(eight_k[1], (0, 2048, 7680, 2048));
        assert_eq!(eight_k[2], (0, 4096, 7680, 224));
    }
}
