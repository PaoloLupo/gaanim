//! Runtime replay helpers for canonical `gaanim_api` descriptions.

use bevy::prelude::*;
use gaanim_math::Camera;
use gaanim_renderer::prelude::VelloView;
use gaanim_timeline::timeline::Timeline;

use crate::canvas::Canvas;

/// Full-target color clear performed before the fitted PBR viewport.
///
/// A camera only clears inside its viewport. Keeping this pass separate avoids
/// stale 3D pixels when editor chrome changes the fitted canvas viewport.
#[derive(Component)]
struct GaanimFullWindowClearCamera;

/// The primary PBR camera owned by the Gaanim scene runtime.
#[derive(Component)]
struct GaanimPbrCamera;

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
    let mut font_registry = match world.remove_resource::<gaanim_text::font::FontRegistry>() {
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
    canvas.register_theme_fonts(&mut font_registry);

    {
        let has_camera_2d = world
            .query_filtered::<Entity, With<Camera2d>>()
            .iter(world)
            .next()
            .is_some();
        let has_clear_camera = world
            .query_filtered::<Entity, With<GaanimFullWindowClearCamera>>()
            .iter(world)
            .next()
            .is_some();
        let has_camera_3d = world
            .query_filtered::<Entity, With<GaanimPbrCamera>>()
            .iter(world)
            .next()
            .is_some();

        // A Camera2d retained by the project hub must use the same overlay
        // policy as a camera spawned directly for a script.
        let mut vello_cameras =
            world.query_filtered::<&mut bevy::prelude::Camera, (With<Camera2d>, With<VelloView>)>();
        for mut camera in vello_cameras.iter_mut(world) {
            camera.order = 1;
            camera.clear_color = ClearColorConfig::None;
        }

        let mut commands = world.commands();
        commands.insert_resource(Camera::ortho_2d(width, height));
        // Spawn the 2D camera first. bevy_egui assigns the primary context to
        // the first camera created, so this must be the camera that renders
        // last and therefore owns the egui pass. The 3D camera still renders
        // first through its lower render order.
        if !has_camera_2d {
            commands.spawn((
                Camera2d,
                VelloView,
                bevy::prelude::Camera {
                    order: 1,
                    clear_color: ClearColorConfig::None,
                    ..default()
                },
                bevy::core_pipeline::tonemapping::Tonemapping::None,
            ));
        }
        if !has_clear_camera {
            // Clear the complete render target before the fitted PBR viewport.
            // RenderLayers::none keeps this camera color-only.
            commands.spawn((
                Camera2d,
                GaanimFullWindowClearCamera,
                bevy::prelude::Camera {
                    order: -1,
                    clear_color: ClearColorConfig::Default,
                    ..default()
                },
                bevy::camera::visibility::RenderLayers::none(),
            ));
        }
        if !has_camera_3d {
            // Perspective camera for 3D meshes (PBR). Render BEFORE 2D so
            // Vello vector content and egui remain on top.
            // Use Tonemapping::None to avoid requiring tonemapping_luts
            // (which needs zstd).
            commands.spawn((
                Camera3d::default(),
                GaanimPbrCamera,
                bevy::prelude::Camera {
                    order: 0,
                    clear_color: ClearColorConfig::None,
                    ..default()
                },
                bevy::core_pipeline::tonemapping::Tonemapping::None,
            ));
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
    use crate::canvas::{LayoutMemberSpec, LayoutSpec, LayoutWithin};
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

    #[test]
    fn invalid_typst_math_in_layout_does_not_remove_runtime_resources() {
        let mut canvas = Canvas::new(640, 360);
        let equation = canvas.text("$integral alpha dt + 2 = 0$");
        let column = canvas.group(&[&equation]);
        equation.claim_layout(&column).unwrap();
        canvas.reflow_layout(
            &column,
            vec![LayoutMemberSpec {
                id: equation.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            }],
            LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
                style: gaanim_layout::LayoutStyle {
                    width: gaanim_layout::SizeRule::Fill(1.0),
                    align: gaanim_layout::Align::Start,
                    ..Default::default()
                },
                within: LayoutWithin::Safe,
            },
            1,
            None,
            None,
            None,
        );
        let diagnostics = canvas.clone();
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());

        replay_canvas_into(&mut world, canvas);

        assert!(world.contains_resource::<Timeline>());
        assert!(world.contains_resource::<gaanim_text::font::FontRegistry>());
        assert!(world.contains_resource::<gaanim_text::prelude::TextConfig>());
        assert!(
            diagnostics
                .check_layout()
                .iter()
                .any(|message| message.contains("unknown variable: dt")),
            "expected the Typst failure to remain available as a layout diagnostic"
        );
    }

    #[test]
    fn hybrid_camera_stack_clears_full_target_before_pbr_and_vello() {
        let canvas = Canvas::new(640, 360);
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        world.spawn((
            Camera2d,
            VelloView,
            bevy::prelude::Camera {
                clear_color: ClearColorConfig::Default,
                ..default()
            },
        ));

        replay_canvas_into(&mut world, canvas);
        world.flush();

        let mut clear_query =
            world.query_filtered::<&bevy::prelude::Camera, With<GaanimFullWindowClearCamera>>();
        let clear = clear_query
            .single(&world)
            .expect("full-window clear camera");
        assert_eq!(clear.order, -1);
        assert!(matches!(clear.clear_color, ClearColorConfig::Default));
        assert!(clear.viewport.is_none());

        let mut pbr_query = world.query_filtered::<&bevy::prelude::Camera, With<GaanimPbrCamera>>();
        let pbr = pbr_query.single(&world).expect("PBR camera");
        assert_eq!(pbr.order, 0);
        assert!(matches!(pbr.clear_color, ClearColorConfig::None));

        let mut vello_query =
            world.query_filtered::<&bevy::prelude::Camera, (With<Camera2d>, With<VelloView>)>();
        let vello = vello_query.single(&world).expect("Vello camera");
        assert_eq!(vello.order, 1);
        assert!(matches!(vello.clear_color, ClearColorConfig::None));
    }
}
