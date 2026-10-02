//! The application `gaanim` and the web player run: window, engine plugins,
//! the editor UI, and the camera the preview is drawn through.

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
    /// Show Presenter View alone in the window: the web player's second
    /// page, which follows the audience page through
    /// [`crate::presenter_link`].
    pub presenter_page: bool,
}

/// What the web page does for the player.
#[derive(Resource, Clone, Copy)]
pub struct WebPage {
    /// Open the Presenter View page next to this one.
    pub open_presenter: fn(),
    /// Copy a link to the open file, ending in this fragment (see
    /// [`crate::share_link`]); empty for the start.
    pub copy_link: fn(&str),
    /// Tell the viewer something, briefly.
    pub notify: fn(&str),
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
                    title: if options.presenter_page {
                        "Gaanim — Presenter View".to_string()
                    } else if options.present {
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
        active: options.present || options.presenter_page,
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
    // The canvas camera owns the primary egui context from the start, for
    // both the project hub and scene launches, so a payload reuses it. It is
    // named rather than left to bevy_egui, which takes the first new camera
    // its query yields: with the clear camera spawned alongside, that was
    // sometimes the clear camera, and no UI was drawn at all.
    app.world_mut()
        .resource_mut::<bevy_egui::EguiGlobalSettings>()
        .auto_create_primary_context = false;
    if options.presenter_page {
        show_presenter_view_alone(app.world_mut());
    } else {
        spawn_host_camera(app.world_mut());
    }
    app
}

/// The window shows Presenter View and nothing else: the primary window is
/// the presenter's (its camera and egui context follow when it is created)
/// and the canvas camera, which draws the slides and the editor's UI, stays
/// off.
fn show_presenter_view_alone(world: &mut World) {
    let primary = world
        .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>()
        .iter(world)
        .next();
    if let Some(primary) = primary {
        world
            .entity_mut(primary)
            .insert(crate::presenter::PresenterWindow);
    }
    world.spawn((
        Camera2d,
        gaanim_renderer::prelude::VelloView,
        bevy::prelude::Camera {
            order: 1,
            is_active: false,
            clear_color: bevy::camera::ClearColorConfig::None,
            ..default()
        },
    ));
    spawn_clear_camera(world);
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
        bevy_egui::PrimaryEguiContext,
    ));
    spawn_clear_camera(world);
}

fn spawn_clear_camera(world: &mut World) {
    // The canvas composites over the window without clearing it. A script
    // replay spawns this clear itself, but a bundle does not: without it the
    // area around the frame kept every earlier frame while the interactive
    // view zoomed out.
    world.spawn((
        Camera2d,
        gaanim_renderer::pipeline::GaanimFullWindowClearCamera,
        bevy::prelude::Camera {
            order: -1,
            clear_color: bevy::camera::ClearColorConfig::Default,
            ..default()
        },
        bevy::camera::visibility::RenderLayers::none(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_installs_a_primary_2d_camera_for_egui_before_scene_replay() {
        let mut world = World::new();
        spawn_host_camera(&mut world);
        // Only the canvas camera draws egui; the clear camera must not.
        let primary = world
            .query_filtered::<Entity, With<bevy_egui::PrimaryEguiContext>>()
            .iter(&world)
            .collect::<Vec<_>>();
        assert_eq!(primary.len(), 1);
        assert!(
            world
                .get::<gaanim_renderer::prelude::VelloView>(primary[0])
                .is_some()
        );
    }

    #[test]
    fn host_clears_the_whole_window_before_the_canvas() {
        let mut world = World::new();
        spawn_host_camera(&mut world);
        let clears = world
            .query_filtered::<&bevy::prelude::Camera, With<
                gaanim_renderer::pipeline::GaanimFullWindowClearCamera,
            >>()
            .iter(&world)
            .map(|camera| (camera.order, camera.clear_color))
            .collect::<Vec<_>>();
        assert!(
            matches!(
                clears.as_slice(),
                [(-1, bevy::camera::ClearColorConfig::Default)]
            ),
            "{clears:?}"
        );
    }
}
