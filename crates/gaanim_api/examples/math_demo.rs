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
        // Standard window settings for the demo
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Gaanim v2 — GPU Math Equations & Text Demo".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(GaanimScenePlugin)
        .add_plugins(GaanimAnimationPlugin)
        .add_plugins(GaanimTimelinePlugin)
        .add_plugins(GaanimTextPlugin)
        .add_plugins(GaanimApiPlugin)
        .add_plugins(GaanimRendererPlugin)
        .add_systems(Startup, setup_scene)
        .add_systems(Update, drive_timeline_clock)
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut timeline: ResMut<Timeline>,
    font_registry: Res<FontRegistry>,
    text_config: Res<gaanim_text::prelude::TextConfig>,
) {
    // 1. Spawn viewport camera and Vello view
    commands.insert_resource(Camera::ortho_2d(1280, 720));
    commands.spawn((Camera2d, VelloView));

    // 2. Initialize the fluent SceneBuilder
    let mut scene = SceneBuilder::new(
        &mut commands,
        &mut *timeline,
        &*font_registry,
        &*text_config,
    );

    // 3. Spawn a plain Title text using HarfBuzz shaper (Arial, 64px, White by default)
    let title_text = scene.title("Gaanim Vector Engine");

    // 4. Spawn a premium mathematical formula using default Math role (NewCMMath, 48pt by default)
    // Formula: E = m c^2
    let math_formula = scene.equation("E = m c^2");

    // 5. Spawn another equation showing fraction and sum limits
    // Formula: sum_(i=1)^n i = (n(n+1))/2
    let sum_formula = scene.equation("sum_(i=1)^n i = frac(n(n+1), 2)");

    // 6. Spawn a beautiful decorative circle in the background
    let bg_circle = scene
        .circle(80.0)
        .fill(gaanim_core::peniko::Color::from_rgb8(25, 50, 100))
        .z_index(-10)
        .spawn();

    // Setup initial animation timeline
    // Slide plain text, formulas and scale the circle in parallel
    scene.play_parallel(vec![
        bg_circle.scale_uniform(1.2).spring().duration(1.5),
        title_text
            .translate_to_2d(-230.0, 240.0)
            .spring()
            .duration(1.8),
        math_formula
            .translate_to_2d(-100.0, 60.0)
            .smooth()
            .duration(1.0),
        sum_formula
            .translate_to_2d(-200.0, -150.0)
            .spring()
            .duration(2.0),
    ]);

    // Let the scene hold for a brief moment
    scene.wait(1.0);

    // INNOVATION DEMO: Select specific characters semantically and color/animate them!
    // 1. Select "m c^2" in the first equation and color it bright gold!
    let mut mc2_selection = scene.select(math_formula, "m c^2");
    mc2_selection.set_fill(gaanim_core::peniko::Color::from_rgb8(255, 215, 0)); // Gold

    // 2. Select "n(n+1)" in the sum equation and color it coral red, then animate it!
    let mut numerator_selection = scene.select(sum_formula, "n(n+1)");
    numerator_selection.set_fill(gaanim_core::peniko::Color::from_rgb8(255, 100, 100)); // Premium Coral Red

    // Coordinated animation: shift numerator "n(n+1)" up slightly in parallel
    numerator_selection
        .animate()
        .spring()
        .duration(1.5)
        .shift_2d(0.0, 30.0);

    // Hold at the end of the animation sequence
    scene.wait(1.5);

    // Add a breakpoint marker
    scene.slide();

    // Loop duration configuration
    timeline.loop_range = Some((0.0, timeline.cached_duration + 0.5));
}

fn drive_timeline_clock(
    mut timeline: ResMut<Timeline>,
    time: Res<Time>,
    mut dt: ResMut<DeltaTime>,
) {
    dt.dt = time.delta_secs_f64();
    timeline.is_playing = true;
}
