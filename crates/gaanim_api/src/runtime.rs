//! Runtime replay helpers for canonical `gaanim_api` descriptions.

use bevy::prelude::*;
use gaanim_math::Camera;
use gaanim_renderer::prelude::VelloView;
use gaanim_timeline::timeline::Timeline;

use crate::canvas::Canvas;

/// Replay a [`Canvas`] into a Bevy world.
///
/// This is the canonical runtime bridge used by the editor/hot-reload host and
/// by future scripting language bindings. Bindings should not duplicate replay
/// logic; they should construct `Canvas` and call into this module indirectly.
pub fn replay_canvas_into(world: &mut World, canvas: Canvas) {
    let width = canvas.width;
    let height = canvas.height;

    let mut timeline = match world.remove_resource::<Timeline>() {
        Some(res) => res,
        None => {
            bevy::prelude::error!("Timeline resource missing");
            return;
        }
    };
    let font_registry = match world.remove_resource::<gaanim_text::font::FontRegistry>() {
        Some(res) => res,
        None => {
            bevy::prelude::error!("FontRegistry resource missing");
            world.insert_resource(timeline);
            return;
        }
    };
    let mut text_config = match world.remove_resource::<gaanim_text::prelude::TextConfig>() {
        Some(res) => res,
        None => {
            bevy::prelude::error!("TextConfig resource missing");
            world.insert_resource(timeline);
            world.insert_resource(font_registry);
            return;
        }
    };
    if canvas.theme.is_some() {
        text_config = canvas.themed_text_config();
    }

    {
        let has_camera = world
            .query_filtered::<Entity, With<Camera2d>>()
            .iter(world)
            .next()
            .is_some();

        let mut commands = world.commands();
        commands.insert_resource(Camera::ortho_2d(width, height));
        if !has_camera {
            commands.spawn((Camera2d, VelloView));
        }

        canvas.compile_into(&mut commands, &mut timeline, &font_registry, &text_config);
    }

    let cached_duration = timeline.cached_duration;
    world.insert_resource(timeline);
    world.insert_resource(font_registry);
    world.insert_resource(text_config);

    if let Some(mut tl) = world.get_resource_mut::<Timeline>() {
        tl.loop_range = Some((0.0, cached_duration));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_text::prelude::TextRole;

    #[test]
    fn replay_applies_the_paper_text_theme() {
        let mut canvas = Canvas::new(640, 360);
        canvas
            .set_theme("paper")
            .expect("paper is a built-in theme");

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());

        replay_canvas_into(&mut world, canvas);

        assert_eq!(
            world.resource::<gaanim_text::prelude::TextConfig>().roles[&TextRole::Body].fill_color,
            gaanim_core::peniko::Color::BLACK
        );
    }
}
