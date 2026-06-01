use std::f64::consts::PI;
use bevy::prelude::*;
use gaanim_math::Camera;
use gaanim_scene::GaanimScenePlugin;
use gaanim_animation::{GaanimAnimationPlugin, DeltaTime};
use gaanim_timeline::{GaanimTimelinePlugin, timeline::Timeline};
use gaanim_renderer::prelude::*;
use gaanim_api::prelude::*;

fn main() {
    App::new()
        // Add Bevy's rendering and windowing defaults
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Gaanim v2 — Fluent SceneBuilder & Relative Layout Demo".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        // Add our programmatic animation engine plugins
        .add_plugins(GaanimScenePlugin)
        .add_plugins(GaanimAnimationPlugin)
        .add_plugins(GaanimTimelinePlugin)
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
fn setup_scene(mut commands: Commands, mut timeline: ResMut<Timeline>) {
    // 1. Spawn a default Orthographic camera resource and entity
    commands.insert_resource(Camera::ortho_2d(1280, 720));
    commands.spawn((Camera2d, VelloView));

    // 2. Initialize the premium SceneBuilder!
    let mut scene = SceneBuilder::new(&mut commands, &mut timeline);

    // 3. Spawn Circle (with auto-incremented ObjectId: 1)
    let circle = scene.circle(50.0)
        .translate(-150.0, 0.0) // Positioned on the left
        .fill(gaanim_core::peniko::Color::from_rgba8(0, 102, 204, 255)) // Slick blue
        .stroke(gaanim_core::peniko::Color::WHITE, 3.0)
        .spawn();

    // 4. Spawn Square next to the Circle (with auto-incremented ObjectId: 2)
    // No more explicit coordinate typing or raw ObjectId::from_parts!
    let square = scene.square(80.0)
        .next_to(circle, LayoutDirection::Right, 50.0) // Automatically computed relative layout!
        .fill(gaanim_core::peniko::Color::from_rgba8(235, 64, 120, 255)) // Premium pink
        .stroke(gaanim_core::peniko::Color::WHITE, 2.0)
        .spawn();

    // 5. Sequence high-level, elegant, and stable animation tween tracks!
    // No manual from/to state parameters are required; the builder handles them seamlessly.

    // Clip 1: Spring translation of Circle to the right (starts at t = 0.5s)
    scene.wait(0.5);
    scene.play(
        circle.translate_to_2d(150.0, 50.0)
            .duration(2.0)
            .spring()
    );

    // Clip 2: Smooth rotation of Square concurrently with Circle fading out!
    scene.wait(0.2);
    scene.play_parallel(vec![
        square.rotate_by(PI)
            .duration(1.5)
            .spring(),
        circle.fade_to(0.3)
            .duration(1.0)
            .smooth(),
    ]);

    // Clip 3: Shift the Square back left and color it white
    scene.wait(0.5);
    scene.play(
        square.shift_2d(-100.0, -100.0)
            .duration(1.5)
            .spring()
    );

    // Clip 4: Scale the Circle back up, shift it, and fade both out
    scene.wait(0.5);
    scene.play_parallel(vec![
        circle.scale_uniform(1.8)
            .duration(1.2)
            .spring(),
        circle.fade_in()
            .duration(1.0)
            .smooth(),
        square.fade_out()
            .duration(1.2)
            .smooth(),
    ]);

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
