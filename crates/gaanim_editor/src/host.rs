//! The application both `gaanim-core` and `gaanim-play` run: window, engine
//! plugins, the editor UI, and the camera the preview is drawn through.

use bevy::prelude::*;

/// How the window opens.
#[derive(Debug, Clone, Default)]
pub struct HostOptions {
    /// Borderless full screen presentation.
    pub present: bool,
    /// Zero-based monitor index used only with `present`.
    pub monitor: Option<usize>,
    /// Segments to rehearse with `--sections` / `--from`.
    pub selection: gaanim_timeline::selection::SegmentSelection,
}

/// Selector of the canvas the web player renders into.
#[cfg(target_arch = "wasm32")]
pub const WEB_CANVAS: &str = "#gaanim-canvas";

/// The editor application, without a scene: callers load a script session,
/// a playback bundle, or show the project hub.
pub fn host_app(options: &HostOptions) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: if options.present {
                        "Gaanim — Presentation".to_string()
                    } else {
                        "Gaanim".to_string()
                    },
                    resolution: (1280, 720).into(),
                    present_mode: bevy::window::PresentMode::AutoVsync,
                    mode: if options.present {
                        bevy::window::WindowMode::BorderlessFullscreen(
                            options
                                .monitor
                                .map(bevy::window::MonitorSelection::Index)
                                .unwrap_or(bevy::window::MonitorSelection::Primary),
                        )
                    } else {
                        bevy::window::WindowMode::Windowed
                    },
                    // The web player draws into the page's canvas and follows
                    // its size; the page decides the layout.
                    #[cfg(target_arch = "wasm32")]
                    canvas: Some(WEB_CANVAS.to_string()),
                    #[cfg(target_arch = "wasm32")]
                    fit_canvas_to_parent: true,
                    ..default()
                }),
                ..default()
            })
            .set(gaanim_scene::gaanim_asset_plugin())
            .set(gaanim_scene::logging::log_plugin()),
    )
    .add_plugins(gaanim_scene::GaanimScenePlugin)
    .add_plugins(gaanim_animation::GaanimAnimationPlugin)
    .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
    .add_plugins(gaanim_media::GaanimMediaPlugin)
    .add_plugins(gaanim_text::GaanimTextPlugin)
    .add_plugins(gaanim_api::GaanimApiPlugin)
    .add_plugins(gaanim_renderer::GaanimRendererPlugin)
    .add_plugins(crate::GaanimEditorPlugin)
    .insert_resource(gaanim_media::VideoSamplingMode::Realtime)
    .insert_resource(gaanim_media::PreviewAudioEnabled(true))
    // Only the interactive preview may lower its resolution while playing.
    .insert_resource(gaanim_renderer::prelude::PreviewResolution::from_setting(
        std::env::var(gaanim_renderer::prelude::PREVIEW_RESOLUTION_ENV)
            .ok()
            .as_deref(),
    ))
    .insert_resource(crate::PresentationMode {
        active: options.present,
    })
    .insert_resource(options.selection.clone())
    // The overlay toggles the user left on come back in every session.
    .insert_resource(crate::overlays::EditorOverlays::with_preferences(
        crate::overlays::OverlayPreferences::load(),
    ))
    .add_systems(Update, crate::overlays::save_overlay_preferences_system);
    if crate::frame_profile::enabled() {
        app.add_plugins(crate::frame_profile::FrameProfilePlugin);
    }
    // bevy_egui creates its primary context when the application starts. Keep
    // this camera alive for both the project hub and scene launches, so a
    // payload can reuse it instead of creating the egui camera after the
    // first frame.
    spawn_host_camera(app.world_mut());
    app
}

fn spawn_host_camera(world: &mut World) {
    world.spawn((
        Camera2d,
        gaanim_renderer::prelude::VelloView,
        bevy::prelude::Camera {
            order: 1,
            clear_color: bevy::camera::ClearColorConfig::None,
            ..default()
        },
        bevy::core_pipeline::tonemapping::Tonemapping::None,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_installs_a_primary_2d_camera_for_egui_before_scene_replay() {
        let mut world = World::new();
        spawn_host_camera(&mut world);
        assert!(
            world
                .query_filtered::<Entity, With<Camera2d>>()
                .iter(&world)
                .next()
                .is_some()
        );
    }
}
