use bevy::prelude::*;
use gaanim_animation::{DeltaTime, GaanimAnimationPlugin};
use gaanim_api::prelude::*;
use gaanim_math::Camera;
use gaanim_renderer::prelude::*;
use gaanim_scene::GaanimScenePlugin;
use gaanim_text::GaanimTextPlugin;
use gaanim_text::font::FontRegistry;
use gaanim_timeline::{GaanimTimelinePlugin, timeline::Timeline};

fn main() {
    App::new()
        // Add Bevy's rendering and windowing defaults
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Gaanim v2 — Fluent SceneBuilder & Typst Demo".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        // Add our programmatic animation engine plugins
        .add_plugins(GaanimScenePlugin)
        .add_plugins(GaanimAnimationPlugin)
        .add_plugins(GaanimTimelinePlugin)
        .add_plugins(GaanimTextPlugin)
        .add_plugins(GaanimApiPlugin)
        // Add the high-performance Vello GPU renderer plugin
        .add_plugins(GaanimRendererPlugin)
        // Setup initial camera and scene graph using our premium builder API
        .add_systems(Startup, setup_scene)
        // Simple timeline clock driver
        .add_systems(Update, drive_timeline_clock)
        .run();
}

/// Startup system: Demonstrates how the fluent SceneBuilder handles layout and animations automatically!
fn setup_scene(
    mut commands: Commands,
    mut timeline: ResMut<Timeline>,
    font_registry: Res<FontRegistry>,
    text_config: Res<gaanim_text::prelude::TextConfig>,
) {
    // 1. Spawn a default Orthographic camera resource and entity
    commands.insert_resource(Camera::ortho_2d(1280, 720));
    commands.spawn((Camera2d, VelloView));

    // 2. Initialize the premium SceneBuilder!
    let mut scene = SceneBuilder::new(&mut commands, &mut timeline, &font_registry, &text_config);

    let _text_doc = scene.typst("Paolo", false, None, None, Some(64.0), None);

    // Loop duration marker setup
    timeline.loop_range = Some((0.0, timeline.cached_duration + 0.5));
}

/// Simple clock system that advances the timeline frame-by-frame
fn drive_timeline_clock(
    mut timeline: ResMut<Timeline>,
    time: Res<Time>,
    mut dt: ResMut<DeltaTime>,
) {
    dt.dt = time.delta_secs_f64();
    timeline.is_playing = true;
}
