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
) {
    // 1. Spawn viewport camera and Vello view
    commands.insert_resource(Camera::ortho_2d(1280, 720));
    commands.spawn((Camera2d, VelloView));

    // 2. Initialize the fluent SceneBuilder
    let mut scene = SceneBuilder::new(&mut commands, &mut timeline, &font_registry);

    // 3. Spawn a plain, non-Typst vector text title using HarfBuzz shaper
    // Shapes standard strings using "Arial" system font (or sans-serif fallback)
    let title_text = scene.text(
        "Gaanim Vector Engine",
        "Arial", // Font family
        48.0,    // Font size in pixels
    );

    // 4. Spawn a premium mathematical formula (LaTeX / Typst math mode)
    // Formula: E = m c^2
    let math_formula = scene.typst(
        "E = m c^2",
        true,          // is_math
        None,          // text_font
        None,          // math_font
        None,          // text_size
        Some(64.0),    // math_size in pt
    );

    // 5. Spawn another formula showing fraction and sum
    // Formula: sum_(i=1)^n i = (n(n+1))/2
    let sum_formula = scene.typst(
        "sum_(i=1)^n i = frac(n(n+1), 2)",
        true,
        None,
        None,
        None,
        Some(48.0),
    );

    // 6. Spawn a beautiful decorative circle in the background
    let bg_circle = scene
        .circle(80.0)
        .fill(gaanim_core::peniko::Color::from_rgb8(25, 50, 100))
        .spawn();

    // Setup initial animation timeline
    // Slide plain text, formulas and scale the circle in parallel
    scene.play_parallel(vec![
        bg_circle.scale_uniform(1.2).spring().duration(1.5),
        title_text.translate_to_2d(-230.0, 240.0).spring().duration(1.8),
        math_formula.translate_to_2d(-100.0, 60.0).smooth().duration(1.0),
        sum_formula.translate_to_2d(-200.0, -150.0).spring().duration(2.0),
    ]);

    // Let the scene hold for a brief moment
    scene.wait(1.0);

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
