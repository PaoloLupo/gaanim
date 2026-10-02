//! What the audience sees, live, in Presenter View's "Now on screen".
//!
//! On the desktop the audience window's canvas is already rasterized every
//! frame; a [`CanvasMirror`] draws it again into a small image the size of
//! the preview, one pass over those few pixels. The web player's Presenter
//! View page has no audience window: its own scene camera, otherwise off,
//! draws into that image instead, so Vello rasterizes only the preview's
//! pixels, and skips a frame identical to the last one. Either way the
//! slide shows the room's votes, which the thumbnails, rendered once from
//! the recorded scene, cannot.

use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiTextureHandle, EguiUserTextures, egui};
use gaanim_renderer::prelude::{CanvasMirror, VelloCanvas, VelloView};

use super::{PresenterCamera, PresenterWindow};
use crate::{PresentationMode, ViewportFrame};

/// Pixels the mirror grows or shrinks by, so resizing the window does not
/// make a new image every frame.
const STEP: f32 = 16.0;

/// The image "Now on screen" shows while presenting.
#[derive(Resource, Default)]
pub(crate) struct PresenterMirror {
    image: Handle<Image>,
    size: UVec2,
    texture: Option<egui::TextureId>,
    /// Width over height of what the audience sees.
    aspect: f32,
    /// Physical pixels "Now on screen" had for the slide last frame.
    pub(crate) wanted: Option<egui::Vec2>,
    /// The image the web page's scene camera draws into.
    camera_target: Option<AssetId<Image>>,
}

impl PresenterMirror {
    /// The texture and its width over height, while it shows the audience.
    pub(crate) fn live(&self) -> Option<(egui::TextureId, f32)> {
        self.texture
            .filter(|_| self.size.min_element() > 0 && self.aspect > 0.0)
            .map(|texture| (texture, self.aspect))
    }
}

/// The image's size: `aspect` fitted in `wanted`, its width rounded up to
/// [`STEP`] and its height following, so the scene fills it exactly.
fn mirror_size(wanted: egui::Vec2, aspect: f32) -> UVec2 {
    let width = ((wanted.x.min(wanted.y * aspect) / STEP).ceil() * STEP).max(STEP);
    UVec2::new(width as u32, (width / aspect).round().max(1.0) as u32)
}

/// Keep the mirror sized to the preview and drawn: by the audience canvas on
/// the desktop, by the page's own scene camera on the web.
#[allow(clippy::too_many_arguments)]
pub(crate) fn presenter_mirror_system(
    presentation: Res<PresentationMode>,
    presenter_cameras: Query<(), With<PresenterCamera>>,
    page: Query<(), (With<PrimaryWindow>, With<PresenterWindow>)>,
    canvas: Res<VelloCanvas>,
    frame: Res<ViewportFrame>,
    clear: Res<ClearColor>,
    (mut mirror, mut canvas_mirror): (ResMut<PresenterMirror>, ResMut<CanvasMirror>),
    (mut images, mut textures): (ResMut<Assets<Image>>, ResMut<EguiUserTextures>),
    mut scene_cameras: Query<(Entity, &mut Camera), With<VelloView>>,
    mut commands: Commands,
) {
    let web_page = !page.is_empty();
    let showing = presentation.active && !presenter_cameras.is_empty();
    let aspect = if web_page {
        (frame.output_size.x / frame.output_size.y.max(1.0)) as f32
    } else {
        let size = canvas.size().as_vec2();
        size.x / size.y.max(1.0)
    };
    let wanted = mirror
        .wanted
        .filter(|_| showing && aspect.is_finite() && aspect > 0.0);
    let Some(wanted) = wanted else {
        if canvas_mirror.image != Handle::default() {
            canvas_mirror.image = Handle::default();
        }
        return;
    };
    mirror.aspect = aspect;
    let size = mirror_size(wanted, aspect);
    if size != mirror.size {
        if mirror.texture.is_some() {
            textures.remove_image(&mirror.image);
        }
        mirror.image = images.add(CanvasMirror::target(size));
        mirror.texture = Some(textures.add_image(EguiTextureHandle::Strong(mirror.image.clone())));
        mirror.size = size;
    }
    if web_page {
        let target = mirror.image.id();
        for (entity, mut camera) in &mut scene_cameras {
            if mirror.camera_target != Some(target) {
                commands
                    .entity(entity)
                    .insert(RenderTarget::Image(mirror.image.clone().into()));
            }
            camera.is_active = true;
            if !matches!(camera.clear_color, ClearColorConfig::Custom(color) if color == clear.0) {
                camera.clear_color = ClearColorConfig::Custom(clear.0);
            }
        }
        mirror.camera_target = Some(target);
    } else if canvas_mirror.image != mirror.image || canvas_mirror.clear != clear.0 {
        canvas_mirror.image = mirror.image.clone();
        canvas_mirror.clear = clear.0;
    }
}

/// The web page's scene camera frames the scene for its window, as a
/// presentation does; drawing into the mirror, it fits the scene there.
pub(crate) fn fit_mirror_camera_system(
    page: Query<(), (With<PrimaryWindow>, With<PresenterWindow>)>,
    mirror: Res<PresenterMirror>,
    frame: Res<ViewportFrame>,
    mut cameras: Query<&mut Projection, With<VelloView>>,
) {
    if page.is_empty() || mirror.camera_target.is_none() || mirror.size.x == 0 {
        return;
    }
    let factor = (frame.size.x / f64::from(mirror.size.x)) as f32;
    if !factor.is_finite() || factor <= 0.0 {
        return;
    }
    for mut projection in &mut cameras {
        if let Projection::Orthographic(ortho) = projection.as_mut() {
            ortho.scale *= factor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mirror_fits_the_audience_in_the_preview_in_steps() {
        // A 16:9 audience in a wide preview: as tall as the preview.
        assert_eq!(
            mirror_size(egui::vec2(1000.0, 400.0), 16.0 / 9.0),
            UVec2::new(720, 405)
        );
        // In a tall one: as wide as the preview, rounded up to the step.
        assert_eq!(
            mirror_size(egui::vec2(600.0, 900.0), 16.0 / 9.0),
            UVec2::new(608, 342)
        );
        assert_eq!(mirror_size(egui::vec2(1.0, 1.0), 1.0), UVec2::new(16, 16));
    }
}
