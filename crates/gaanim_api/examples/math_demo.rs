//! Canvas multi-segment demo with transitions between segments.
//!
//! Run: `cargo run --example math_demo -p gaanim_api`

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
use gaanim_timeline::{GaanimTimelinePlugin, timeline::Timeline, transition::TransitionType};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Gaanim Multi-Segment Demo".into(),
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

    let mut canvas = Canvas::new(1280, 720);

    // Segment 1: Intro
    let _s1 = canvas.segment("intro", None).expect("intro segment");
    let title_spec = gaanim_text::prelude::TextSpec::new(
        vec!["Multi-Segment Demo".into()],
        Some(gaanim_text::prelude::TextRole::Title),
        gaanim_text::prelude::TextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    )
    .expect("valid unified title text");
    let title = canvas.text_spec(title_spec).fill(Color::WHITE).at(0.0, 0.0);
    title.fade_in(1.0);
    canvas.wait(1.5);
    title.fade_out(0.8); // manual exit (Patron B)
    canvas.wait(0.3);

    // Segment 2: Content with auto cross-fade
    let _s2 = canvas
        .segment("content", Some(TransitionType::CrossFade { duration: 0.5 }))
        .expect("content segment");
    let circle = canvas
        .circle(60.0)
        .fill(Color::from_rgb8(0xE5, 0x4B, 0x4B))
        .at(-200.0, 0.0);
    circle.create(1.5);
    canvas.wait(0.5);
    circle.grow_from_center(1.0);
    canvas.wait(1.0);
    canvas.fade_out_all(0.8);
    canvas.wait(0.3);

    // Segment 3: Conclusion with slide transition
    let _s3 = canvas
        .segment(
            "conclusion",
            Some(TransitionType::Slide {
                duration: 0.6,
                direction: gaanim_timeline::transition::SlideDirection::Left,
            }),
        )
        .expect("conclusion segment");
    let thanks_spec = gaanim_text::prelude::TextSpec::new(
        vec!["Thank You!".into()],
        Some(gaanim_text::prelude::TextRole::Title),
        gaanim_text::prelude::TextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    )
    .expect("valid unified title text");
    let thanks = canvas
        .text_spec(thanks_spec)
        .fill(Color::WHITE)
        .at(0.0, 0.0);
    thanks.fade_in(1.0);
    canvas.wait(2.0);

    info!(
        "Canvas: {} segments, {:.1}s total",
        canvas.segment_count(),
        canvas.current_time()
    );
    canvas.compile_into(&mut commands, &mut timeline, &font_registry, &text_config);
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
