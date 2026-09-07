//! Authored raster framing, independent of decoded video frames.
use crate::{LocalBounds, Path2D, PathSource, RasterImage};
use bevy::prelude::*;
use gaanim_core::kurbo::{Affine, Rect, Shape};
use gaanim_core::peniko::ImageQuality;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
    Stretch,
}

/// Source crop in pixels and a fixed destination frame in scene units.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MediaFrame {
    pub crop: Rect,
    pub width: f64,
    pub height: f64,
    pub fit: ImageFit,
    pub quality: ImageQuality,
    /// 0 preserves legacy top-left cover; 0.5 centers new framing operations.
    pub alignment: f64,
}
impl MediaFrame {
    pub fn interpolate(self, to: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: f64, b: f64| a + (b - a) * t;
        Self {
            crop: Rect::new(
                lerp(self.crop.x0, to.crop.x0),
                lerp(self.crop.y0, to.crop.y0),
                lerp(self.crop.x1, to.crop.x1),
                lerp(self.crop.y1, to.crop.y1),
            ),
            width: lerp(self.width, to.width),
            height: lerp(self.height, to.height),
            alignment: lerp(self.alignment, to.alignment),
            ..to
        }
    }
    /// Return the clipped visible rectangle and mapping of original source pixels.
    pub fn geometry(self) -> (Rect, Affine) {
        let sx = self.width / self.crop.width();
        let sy = self.height / self.crop.height();
        let (sx, sy) = match self.fit {
            ImageFit::Contain => (sx.min(sy), sx.min(sy)),
            ImageFit::Cover => (sx.max(sy), sx.max(sy)),
            ImageFit::Stretch => (sx, sy),
        };
        let w = self.width.min(self.crop.width() * sx);
        let h = self.height.min(self.crop.height() * sy);
        let center = gaanim_core::kurbo::Point::new(
            self.crop.x0 + w / sx / 2.0 + (self.crop.width() - w / sx) * self.alignment,
            self.crop.y0 + h / sy / 2.0 + (self.crop.height() - h / sy) * self.alignment,
        );
        (
            Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0),
            Affine::new([sx, 0.0, 0.0, -sy, -center.x * sx, center.y * sy]),
        )
    }
}

/// Runs before layout. Changing decoded pixels never changes the authored frame.
pub fn update_media_frames(
    mut query: Query<
        (
            &MediaFrame,
            &mut RasterImage,
            &mut Path2D,
            &mut PathSource,
            &mut LocalBounds,
        ),
        Changed<MediaFrame>,
    >,
) {
    for (frame, mut raster, mut path, mut source, mut bounds) in &mut query {
        let (rect, transform) = frame.geometry();
        if raster.local_transform != transform {
            raster.local_transform = transform;
        }
        if let Some(image) = raster.image.as_ref() {
            if image.sampler.quality != frame.quality {
                raster.image = Some(image.clone().with_quality(frame.quality));
            }
        }
        let geometry = rect.to_path(0.1);
        if *path.0 != geometry {
            path.0 = std::sync::Arc::new(geometry);
            source.0 = path.0.clone();
        }
        let value = gaanim_math::Bounds3D::new_2d(
            -frame.width / 2.0,
            -frame.height / 2.0,
            frame.width / 2.0,
            frame.height / 2.0,
        );
        if bounds.0 != value {
            bounds.0 = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_centers_content_and_keeps_transparent_contain_bands() {
        let frame = MediaFrame {
            crop: Rect::new(100.0, 50.0, 300.0, 150.0),
            width: 8.0,
            height: 8.0,
            fit: ImageFit::Contain,
            quality: ImageQuality::High,
            alignment: 0.5,
        };
        let (rect, transform) = frame.geometry();
        assert_eq!(rect, Rect::new(-4.0, -2.0, 4.0, 2.0));
        assert_eq!(
            transform * frame.crop.center(),
            gaanim_core::kurbo::Point::ZERO
        );
        let cover = MediaFrame {
            fit: ImageFit::Cover,
            ..frame
        };
        assert_eq!(cover.geometry().0, Rect::new(-4.0, -4.0, 4.0, 4.0));
        let stretch = MediaFrame {
            fit: ImageFit::Stretch,
            ..frame
        };
        assert_eq!(stretch.geometry().0, cover.geometry().0);
        assert_ne!(stretch.geometry().1, cover.geometry().1);
    }
}
