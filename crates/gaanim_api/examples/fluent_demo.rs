//! Canvas demo: intro with fade_in, move, fade_out.
//! Run: `cargo run --example fluent_demo -p gaanim_api`

use bevy::prelude::*;
use gaanim_animation::{DeltaTime, GaanimAnimationPlugin};
use gaanim_api::GaanimApiPlugin;
use gaanim_api::canvas::Canvas;
use gaanim_core::peniko::Color;
use gaanim_math::Camera;
use gaanim_renderer::prelude::*;
use gaanim_scene::GaanimScenePlugin;
use gaanim_text::GaanimTextPlugin;
use gaanim_text::font::FontRegistry;
use gaanim_timeline::{GaanimTimelinePlugin, timeline::Timeline};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Gaanim Canvas Demo".into(),
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
    commands.insert_resource(Camera::ortho_2d(1280, 720));
    commands.spawn((Camera2d, VelloView));

    let mut canvas = Canvas::new(1280, 720).background(Color::from_rgb8(0x0f, 0x0f, 0x1a));

    let logo = canvas
        .circle(80.0)
        .fill(Color::from_rgb8(0x19, 0x32, 0x64))
        .at(0.0, 0.0);
    let title_spec = gaanim_text::prelude::TextSpec::new(
        vec!["Gaanim".into()],
        Some(gaanim_text::prelude::TextRole::Title),
        gaanim_text::prelude::TextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    )
    .expect("valid unified title text");
    let title = canvas
        .text_spec(title_spec)
        .fill(Color::WHITE)
        .at(0.0, 180.0);

    logo.fade_in(1.0);
    canvas.wait(0.5);
    title.fade_in(1.0);
    canvas.wait(1.0);
    logo.r#move(-300.0, 0.0).duration(1.5);
    canvas.wait(0.5);
    canvas.fade_out_all(1.0);
    canvas.wait(0.5);

    info!("Canvas: {:.1}s total", canvas.current_time());
    canvas.compile_into(&mut commands, &mut timeline, &font_registry, &text_config);
    timeline.loop_range = Some((0.0, timeline.cached_duration + 0.5));
}

fn drive_timeline_clock(mut tl: ResMut<Timeline>, time: Res<Time>, mut dt: ResMut<DeltaTime>) {
    dt.dt = time.delta_secs_f64();
    tl.is_playing = true;
}
