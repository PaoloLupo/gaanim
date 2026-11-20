use super::*;
use crate::canvas::{Anchor, DrawableHandle, TextAnchor};
use bevy::ecs::world::CommandQueue;
use gaanim_scene::{MobjectId, ObjectTag};

#[test]
fn reactive_meshes_rebuild_only_when_their_inputs_change() {
    let last = LastInputs::default();
    let id = ObjectId::from_raw(7);
    assert!(last.changed(None, &[(id, 1.0)]));
    assert!(!last.changed(None, &[(id, 1.0)]));
    assert!(last.changed(None, &[(id, 2.0)]));
    // A mesh that follows the time rebuilds as it passes.
    assert!(last.changed(Some(0.5), &[(id, 2.0)]));
    assert!(!last.changed(Some(0.5), &[(id, 2.0)]));
    assert!(last.changed(Some(0.6), &[(id, 2.0)]));
}
use gaanim_animation::rehearsal::Lean;
use gaanim_core::peniko::Brush;
use gaanim_math::SpatialTransform;
use gaanim_scene::{LocalBounds, TextBaseline};
use gaanim_timeline::snapshot::WorldSnapshot;

fn compile_canvas_for_layout(canvas: SceneModel) -> World {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    world
}

fn lottie_test_asset(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "gaanim-compiled-lottie-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &path,
        r#"{
            "v":"5.7.5","fr":30,"ip":0,"op":30,"w":100,"h":100,
            "layers":[{"ty":4,"ind":1,"st":0,"ip":0,"op":30,
                "ks":{"p":{"a":1,"k":[
                    {"t":0,"s":[0,0],"o":{"x":0.33,"y":0.33},"i":{"x":0.67,"y":0.67}},
                    {"t":30,"s":[60,0]}
                ]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0}},
                "shapes":[
                    {"ty":"rc","p":{"a":0,"k":[10,50]},"s":{"a":0,"k":[10,10]},"r":{"a":0,"k":0}},
                    {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100},"r":1}
                ]
            }]
        }"#,
    )
    .unwrap();

    path
}

#[test]
fn compiled_lottie_playback_animates_after_frozen_declaration_and_seeks_back() {
    use gaanim_renderer::lottie::{LottiePlayer, sample_lottie_system};

    let path = lottie_test_asset("playback");

    for (activate, looping) in [(false, false), (true, false), (true, true)] {
        let mut canvas = SceneModel::new(100, 100);
        let clip = canvas
            .lottie_with_options(
                &path,
                crate::canvas::LottieOptions {
                    offset: 0.25,
                    duration: Some(0.5),
                    speed: 2.0,
                    looping,
                    ..Default::default()
                },
            )
            .unwrap();
        canvas.wait(1.0); // Freeze the declaration before scheduling playback.
        canvas.segment("lottie", None).unwrap();
        canvas.wait(2.0);
        if activate {
            canvas.play_items(vec![clip.into()]).unwrap();
        }
        canvas.wait(1.0);
        let mut world = compile_canvas_for_layout(canvas);
        world.insert_resource(gaanim_animation::PlaybackState::default());
        let mut schedule = Schedule::default();
        schedule.add_systems(sample_lottie_system);
        let mut sample = |time| {
            world
                .resource_mut::<gaanim_animation::PlaybackState>()
                .current_time = time;
            schedule.run(&mut world);
            world
                .query::<&LottiePlayer>()
                .single(&world)
                .unwrap()
                .scene()
                .encoding()
                .transforms
                .clone()
        };
        let first = sample(0.0);
        assert_eq!(sample(2.9), first, "hold the source offset before play");
        assert_eq!(
            sample(3.0),
            first,
            "play begins at the absolute scene cursor"
        );
        let middle = sample(3.125);
        if activate {
            assert_ne!(middle, first, "Scene.play must produce visible motion");
            if looping {
                assert_eq!(sample(3.375), middle, "repeat the selected source interval");
            } else {
                assert_ne!(sample(3.5), middle, "advance to the final frame");
                assert_eq!(sample(3.5), sample(4.0), "hold the final frame");
            }
        } else {
            assert_eq!(middle, first, "an unplayed declaration must remain still");
        }
        assert_eq!(sample(0.0), first, "backward seek restores the first frame");
        assert_eq!(sample(3.125), middle, "forward seek is deterministic");
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn dotlottie_commands_survive_compilation_and_exact_seeks() {
    use gaanim_renderer::lottie::{
        LottieInput, LottiePackageOptions, LottiePlayer, sample_lottie_system,
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/assets/dotlottie_demo.lottie");
    let mut canvas = SceneModel::new(200, 160);
    canvas.preload(std::slice::from_ref(&path)).unwrap();
    let clip = canvas
        .lottie_with_package_options(
            &path,
            Default::default(),
            LottiePackageOptions {
                state_machine_id: Some("main".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(clip.animation_ids(), ["idle", "active"]);
    assert!(clip.clone().fire_event("reset").is_err());
    canvas.play(vec![clip.drawable.write(1.0)]);
    let start = canvas.current_time();
    canvas.play_items(vec![clip.clone().into()]).unwrap();
    assert_eq!(
        canvas.current_time(),
        start,
        "machine activation is non-blocking"
    );
    canvas.wait(1.0);
    clip.clone()
        .set_input("active", LottieInput::Boolean(true))
        .unwrap();
    clip.clone().set_theme(Some("gold")).unwrap();
    canvas.wait(1.0);
    clip.clone().fire_event("reset").unwrap();
    canvas.wait(1.0);
    clip.set_theme(None).unwrap();
    canvas.wait(1.0);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    world.insert_resource(gaanim_animation::PlaybackState::default());
    let mut schedule = Schedule::default();
    schedule.add_systems(sample_lottie_system);
    let mut sample = |time| {
        timeline.seek(&mut world, time);
        world
            .resource_mut::<gaanim_animation::PlaybackState>()
            .current_time = time;
        schedule.run(&mut world);
        let player = world.query::<&LottiePlayer>().single(&world).unwrap();
        let encoding = player.scene().encoding();
        (
            encoding.path_data.clone(),
            encoding.draw_data.clone(),
            encoding.transforms.clone(),
        )
    };
    let times = [0.0, 0.35, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0];
    let expected: Vec<_> = times.iter().map(|t| sample(*t)).collect();
    assert_ne!(expected[0], expected[1]);
    assert_ne!(expected[3], expected[4]);
    assert_ne!(
        expected[6], expected[7],
        "theme resets even at an unchanged source frame"
    );
    for index in (0..times.len()).rev() {
        assert_eq!(sample(times[index]), expected[index]);
    }
}

#[test]
fn lottie_write_and_create_reveal_vectors_before_playback() {
    use gaanim_renderer::lottie::{LottiePlayer, sample_lottie_system};
    let path = lottie_test_asset("reveal");
    for write in [false, true] {
        let mut canvas = SceneModel::new(100, 100);
        let clip = canvas.lottie(&path).unwrap();
        let animation = if write {
            clip.drawable.write(1.0)
        } else {
            clip.drawable.create(1.0)
        };
        canvas.play(vec![animation]);
        canvas.play_items(vec![clip.into()]).unwrap();
        let (mut world, mut timeline) = compile_camera_timeline(canvas);
        world.insert_resource(gaanim_animation::PlaybackState::default());
        let mut schedule = Schedule::default();
        schedule.add_systems(sample_lottie_system);
        let mut sample = |time| {
            timeline.seek(&mut world, time);
            world
                .resource_mut::<gaanim_animation::PlaybackState>()
                .current_time = time;
            schedule.run(&mut world);
            let player = world.query::<&LottiePlayer>().single(&world).unwrap();
            let encoded = player.scene().encoding();
            (
                encoded.path_data.clone(),
                encoded.draw_data.clone(),
                encoded.transforms.clone(),
            )
        };
        let empty = sample(0.0);
        let outline = sample(0.35);
        let fill = sample(0.85);
        let first = sample(1.0);
        assert!(
            outline.0.len() > empty.0.len(),
            "draw visible contours during reveal"
        );
        assert_ne!(
            outline, fill,
            "cross-fade the authored fill after the outline"
        );
        assert_ne!(fill, first, "finish the fill phase");
        assert_ne!(sample(1.5).2, first.2, "start source playback after reveal");
        assert_eq!(sample(0.0), empty, "rewind hides the composition");
        assert_eq!(sample(0.35), outline, "rewind restores partial outlines");
        assert_eq!(
            sample(1.0),
            first,
            "reveal returns to the first source frame"
        );
    }
    let _ = std::fs::remove_file(path);
}

fn only_text_root(world: &mut World) -> (gaanim_math::Bounds3D, TextBaseline, SpatialTransform) {
    let roots = world
        .query::<(
            &LocalBounds,
            &TextBaseline,
            &SpatialTransform,
            Option<&bevy::prelude::ChildOf>,
        )>()
        .iter(world)
        .filter_map(|(bounds, baseline, transform, parent)| {
            parent
                .is_none()
                .then_some((bounds.0, *baseline, *transform))
        })
        .collect::<Vec<_>>();
    assert_eq!(roots.len(), 1, "expected exactly one text root");
    roots[0]
}

fn assert_point_close(actual: DVec3, expected: DVec3) {
    assert!(
        actual.distance(expected) < 1e-6,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn at_anchor_point_places_center_on_transformed_reference_anchor() {
    let mut canvas = SceneModel::new(640, 360);
    let reference = canvas
        .rect(100.0, 40.0)
        .move_to(30.0, -10.0)
        .scale_to(1.5)
        .rotate_to(std::f64::consts::FRAC_PI_2);
    let point = reference.anchor_point(Anchor::TopRight, DVec3::new(5.0, -3.0, 0.0));
    canvas.rect(10.0, 6.0).at_anchor_point(point);

    let mut world = compile_canvas_for_layout(canvas);
    let mut roots = world
        .query::<(
            &LocalBounds,
            &SpatialTransform,
            Option<&bevy::prelude::ChildOf>,
        )>()
        .iter(&world)
        .filter_map(|(bounds, transform, parent)| {
            parent.is_none().then_some((bounds.0, *transform))
        })
        .collect::<Vec<_>>();
    roots.sort_by(|(left, _), (right, _)| left.width().total_cmp(&right.width()));
    assert_eq!(roots.len(), 2);

    let (target_bounds, target_transform) = roots[0];
    let (reference_bounds, reference_transform) = roots[1];
    let reference_local =
        Anchor::TopRight.get_point(&reference_bounds) + DVec3::new(5.0, -3.0, 0.0);
    assert_point_close(
        target_transform
            .to_mat4()
            .transform_point3(target_bounds.center()),
        reference_transform
            .to_mat4()
            .transform_point3(reference_local),
    );
}

#[test]
fn default_single_line_text_keeps_its_baseline_center_after_transform_layout() {
    for source in ["HAPPY", "gyp", "$frac(x_1^2, y_2) = 1$"] {
        let mut canvas = SceneModel::new(640, 360);
        canvas
            .text(source)
            .at_text_default(73.0, -41.0)
            .scale_to(1.65)
            .rotate_to(0.23)
            .with_pivot(-120.0, 95.0);

        let mut world = compile_canvas_for_layout(canvas);
        let (bounds, baseline, transform) = only_text_root(&mut world);
        let local_anchor = DVec3::new(bounds.center().x, baseline.0, bounds.center().z);
        assert_point_close(
            transform.to_mat4().transform_point3(local_anchor),
            DVec3::new(73.0, -41.0, 0.0),
        );
    }
}

#[test]
fn unpositioned_single_line_text_places_its_baseline_center_at_the_origin() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas.text("Hola mundo");

    let mut world = compile_canvas_for_layout(canvas);
    let (bounds, baseline, transform) = only_text_root(&mut world);
    let local_anchor = DVec3::new(bounds.center().x, baseline.0, bounds.center().z);
    assert_point_close(
        transform.to_mat4().transform_point3(local_anchor),
        DVec3::ZERO,
    );
}

#[test]
fn equation_transform_hands_off_the_target_baseline_at_the_endpoint() {
    use gaanim_text::prelude::{TextContent, TextFlow, TextRole, TextSpec, TextStyle};

    let equation = |source: &str| {
        TextSpec::new(
            vec![TextContent::Literal(format!("$ {source} $"))],
            Some(TextRole::Title),
            TextStyle::default(),
            TextFlow::default(),
        )
        .expect("valid equation")
    };
    let mut canvas = SceneModel::new(16.0, 9.0);
    let source = canvas
        .text_spec(equation("integral_(-infinity)^infinity = x^2 d x"))
        .scale_by(3.0)
        .at_text_default(0.0, 0.0);
    let target = canvas
        .text_spec(equation("x^2 d x"))
        .scale_by(3.0)
        .at_text_default(0.0, 0.0);
    canvas.play(vec![source.transform(&target)]);
    canvas.wait(0.1);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let target_baseline = world
        .query::<(&LocalBounds, &TextBaseline, Option<&bevy::prelude::ChildOf>)>()
        .iter(&world)
        .filter(|(_, _, parent)| parent.is_none())
        .min_by(|(left, ..), (right, ..)| left.0.width().total_cmp(&right.0.width()))
        .map(|(_, baseline, _)| baseline.0)
        .expect("target text root");

    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 1.0);
    let transformed_baseline = world
        .query::<(
            &gaanim_scene::Path2D,
            &TextBaseline,
            Option<&bevy::prelude::ChildOf>,
        )>()
        .iter(&world)
        .find_map(|(path, baseline, parent)| {
            (parent.is_none() && !path.0.elements().is_empty()).then_some(baseline.0)
        })
        .expect("flattened transformed text root");

    assert!((transformed_baseline - target_baseline).abs() < 1.0e-9);
}

#[test]
fn shifting_unpositioned_text_preserves_its_implicit_baseline_anchor() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas.text("Hola mundo").shift_by(2.0, -1.0);

    let mut world = compile_canvas_for_layout(canvas);
    let (bounds, baseline, transform) = only_text_root(&mut world);
    let local_anchor = DVec3::new(bounds.center().x, baseline.0, bounds.center().z);
    assert_point_close(
        transform.to_mat4().transform_point3(local_anchor),
        DVec3::new(2.0, -1.0, 0.0),
    );
}

#[test]
fn unpositioned_multiline_text_keeps_its_visual_center_at_the_origin() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas.text("Primera línea\nSegunda línea");

    let mut world = compile_canvas_for_layout(canvas);
    let (bounds, _, transform) = only_text_root(&mut world);
    assert_point_close(
        transform.to_mat4().transform_point3(bounds.center()),
        DVec3::ZERO,
    );
}

#[test]
fn explicit_text_baseline_anchors_place_left_center_and_right_edges() {
    for (anchor, x_selector) in [
        (
            TextAnchor::BaselineLeft,
            fn_x_min as fn(&gaanim_math::Bounds3D) -> f64,
        ),
        (TextAnchor::BaselineCenter, fn_x_center),
        (TextAnchor::BaselineRight, fn_x_max),
    ] {
        let mut canvas = SceneModel::new(640, 360);
        canvas.text("gyp").at_text_anchor(17.0, 29.0, anchor);

        let mut world = compile_canvas_for_layout(canvas);
        let (bounds, baseline, transform) = only_text_root(&mut world);
        let local_anchor = DVec3::new(x_selector(&bounds), baseline.0, bounds.center().z);
        assert_point_close(
            transform.to_mat4().transform_point3(local_anchor),
            DVec3::new(17.0, 29.0, 0.0),
        );
    }

    fn fn_x_min(bounds: &gaanim_math::Bounds3D) -> f64 {
        bounds.min.x
    }
    fn fn_x_center(bounds: &gaanim_math::Bounds3D) -> f64 {
        bounds.center().x
    }
    fn fn_x_max(bounds: &gaanim_math::Bounds3D) -> f64 {
        bounds.max.x
    }
}

#[test]
fn multiline_default_is_visual_center_but_explicit_anchor_uses_first_baseline() {
    let mut default_canvas = SceneModel::new(640, 360);
    default_canvas
        .text("First line\nsecond line")
        .at_text_default(-35.0, 61.0);
    let mut default_world = compile_canvas_for_layout(default_canvas);
    let (bounds, _, transform) = only_text_root(&mut default_world);
    assert_point_close(
        transform.to_mat4().transform_point3(bounds.center()),
        DVec3::new(-35.0, 61.0, 0.0),
    );

    let mut explicit_canvas = SceneModel::new(640, 360);
    explicit_canvas
        .text("First line\nsecond line")
        .at_text_anchor(-35.0, 61.0, TextAnchor::BaselineLeft);
    let mut explicit_world = compile_canvas_for_layout(explicit_canvas);
    let (bounds, baseline, transform) = only_text_root(&mut explicit_world);
    assert_point_close(
        transform.to_mat4().transform_point3(DVec3::new(
            bounds.min.x,
            baseline.0,
            bounds.center().z,
        )),
        DVec3::new(-35.0, 61.0, 0.0),
    );
    assert!(
        (baseline.0 - bounds.center().y).abs() > 0.01,
        "first baseline must remain distinct from the multiline visual center"
    );
}

#[test]
fn geometric_top_left_places_the_corner_without_promising_a_baseline() {
    let mut baselines = Vec::new();
    for source in ["HAPPY", "gyp"] {
        let mut canvas = SceneModel::new(640, 360);
        canvas.text(source).at_anchor(12.0, 34.0, Anchor::TopLeft);
        let mut world = compile_canvas_for_layout(canvas);
        let (bounds, baseline, transform) = only_text_root(&mut world);
        assert_point_close(
            transform
                .to_mat4()
                .transform_point3(Anchor::TopLeft.get_point(&bounds)),
            DVec3::new(12.0, 34.0, 0.0),
        );
        baselines.push(
            transform
                .to_mat4()
                .transform_point3(DVec3::new(bounds.min.x, baseline.0, 0.0))
                .y,
        );
    }
    assert!(
        (baselines[0] - baselines[1]).abs() > 0.1,
        "geometric top-left alignment must not imply equal baselines: {baselines:?}"
    );
}

#[test]
fn reactive_reveal_ends_at_exact_data_coordinate() {
    let map = gaanim_visualization::CoordinateMap2D::new(
        gaanim_visualization::Axis::linear(0.0, 3.0 * std::f64::consts::PI).unwrap(),
        gaanim_visualization::Axis::linear(-1.0, 1.0).unwrap(),
        gaanim_visualization::PlotFrame::new(600.0, 240.0).unwrap(),
    );
    let function = ReactiveFunction::new(1, 1, vec![], |values| Ok(vec![values[0].sin()]));
    let reveal = ScalarSource::constant(std::f64::consts::FRAC_PI_2);
    let path = sampled_reactive_path(
        &map,
        &function,
        (0.0, 3.0 * std::f64::consts::PI),
        Some(&reveal),
        gaanim_visualization::Sampling::Fixed { samples: 65 },
        0.0,
        &[],
    );

    assert!((path.bounding_box().x1 + 200.0).abs() < 1e-9);
    assert!(
        sampled_reactive_path(
            &map,
            &function,
            (0.0, 3.0 * std::f64::consts::PI),
            Some(&ScalarSource::constant(0.0)),
            gaanim_visualization::Sampling::Fixed { samples: 65 },
            0.0,
            &[],
        )
        .is_empty()
    );
}

#[test]
fn reactive_geometry_matches_across_direct_rewind_and_repeated_seeks() {
    let mut canvas = SceneModel::new(640, 360);
    let amplitude = canvas.parameter(1.0).unwrap();
    let amplitude_id = amplitude.drawable().id;
    let space = canvas
        .coordinate_axes(
            gaanim_visualization::Axis::linear(-2.0, 2.0).unwrap(),
            gaanim_visualization::Axis::linear(-3.0, 3.0).unwrap(),
            Some(400.0),
            Some(240.0),
            false,
        )
        .unwrap();
    canvas
        .reactive_plot(
            &space,
            ReactiveFunction::new(
                1,
                1,
                vec![
                    gaanim_animation::ReactiveInput::Signal(amplitude_id),
                    gaanim_animation::ReactiveInput::Time,
                ],
                |values| Ok(vec![values[1] * (values[0] + values[2]).sin()]),
            ),
            (-2.0, 2.0),
            gaanim_visualization::Sampling::Fixed { samples: 33 },
        )
        .unwrap();
    canvas.play(vec![amplitude.animate().set(2.0).duration(2.0)]);

    let mut world = World::new();
    world.insert_resource(gaanim_animation::PlaybackState::default());
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    let plot = world
        .query_filtered::<Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
        .single(&world)
        .expect("one reactive plot");

    let mut capture = |time| {
        timeline.seek(&mut world, time);
        gaanim_animation::always_redraw_regen_system(&mut world);
        world
            .get::<gaanim_scene::Path2D>(plot)
            .expect("reactive path")
            .0
            .elements()
            .to_vec()
    };
    let direct = capture(1.5);
    let _earlier = capture(0.25);
    let rewind = capture(1.5);
    let _later = capture(1.9);
    let repeated = capture(1.5);

    assert_eq!(direct, rewind);
    assert_eq!(direct, repeated);
}

trait UnifiedTextFixture {
    fn math_text(&mut self, source: &str) -> DrawableHandle;
    fn test_title(&mut self, source: &str) -> DrawableHandle;
    fn test_subtitle(&mut self, source: &str) -> DrawableHandle;
    fn configured_text(
        &mut self,
        source: &str,
        style: gaanim_text::prelude::TextStyle,
        flow: gaanim_text::prelude::TextFlow,
    ) -> DrawableHandle;
}

impl UnifiedTextFixture for SceneModel {
    fn math_text(&mut self, source: &str) -> DrawableHandle {
        self.text(&format!("${source}$"))
    }

    fn test_title(&mut self, source: &str) -> DrawableHandle {
        let spec = StructuredTextSpec::new(
            vec![source.into()],
            Some(gaanim_text::prelude::TextRole::Title),
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid title fixture");
        self.text_spec(spec)
    }

    fn test_subtitle(&mut self, source: &str) -> DrawableHandle {
        let spec = StructuredTextSpec::new(
            vec![source.into()],
            Some(gaanim_text::prelude::TextRole::Subtitle),
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid subtitle fixture");
        self.text_spec(spec)
    }

    fn configured_text(
        &mut self,
        source: &str,
        style: gaanim_text::prelude::TextStyle,
        flow: gaanim_text::prelude::TextFlow,
    ) -> DrawableHandle {
        let spec = StructuredTextSpec::new(vec![source.into()], None, style, flow)
            .expect("valid configured text fixture");
        self.text_spec(spec)
    }
}

#[test]
fn structured_math_parts_preserve_native_typst_whitespace() {
    let spec = StructuredTextSpec::new(
        vec![
            "$".into(),
            gaanim_text::prelude::TextPart::new(
                "variable",
                vec!["x".into()],
                StructuredTextStyle::default(),
            )
            .into(),
            " dot 5 = ".into(),
            gaanim_text::prelude::TextPart::new(
                "result",
                vec!["25".into()],
                StructuredTextStyle::default(),
            )
            .into(),
            "$".into(),
        ],
        None,
        StructuredTextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    )
    .expect("valid structured equation");

    let source = structured_typst_content(&spec, 32.0);
    assert_eq!(source, "$x dot 5 = 25$");
    assert!(!source.contains("$$"));
    assert!(!source.contains("#h(0pt)"));
}

#[test]
fn adjacent_sibling_parts_use_typst_spacing_only_inside_shared_math() {
    let part = |name: &str, text: &str, style: StructuredTextStyle| {
        gaanim_text::prelude::TextPart::new(name, vec![text.into()], style).into()
    };
    let spec = |content| {
        StructuredTextSpec::new(
            content,
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid structured text")
    };

    let implicit = spec(vec![
        "$".into(),
        part("left", "a", StructuredTextStyle::default()),
        part("right", "b", StructuredTextStyle::default()),
        "$".into(),
    ]);
    assert_eq!(structured_typst_content(&implicit, 32.0), "$a b$");

    let explicit = spec(vec![
        "$".into(),
        part("left", "a ", StructuredTextStyle::default()),
        part("right", " b", StructuredTextStyle::default()),
        "$".into(),
    ]);
    let explicit_source = structured_typst_content(&explicit, 32.0);
    assert_eq!(explicit_source, "$a b$");

    let prose = spec(vec![
        part("left", "a", StructuredTextStyle::default()),
        part("right", "b", StructuredTextStyle::default()),
    ]);
    assert_eq!(structured_typst_content(&prose, 32.0), "#text(\"ab\")");

    let tight_literal = spec(vec![
        "$".into(),
        part("variable", "x", StructuredTextStyle::default()),
        "_1".into(),
        "$".into(),
    ]);
    assert_eq!(structured_typst_content(&tight_literal, 32.0), "$x _1$");

    let colored = StructuredTextStyle {
        color: Some(PenikoColor::from_rgb8(255, 0, 0)),
        ..StructuredTextStyle::default()
    };
    let distributed_delimiters = spec(vec![
        part("left", "$a", StructuredTextStyle::default()),
        part("right", "b$", colored),
    ]);
    let distributed_source = structured_typst_content(&distributed_delimiters, 32.0);
    assert!(!distributed_source.contains("#h("));
    assert!(distributed_source.starts_with("$a text("));
    assert!(distributed_source.contains("ff0000"));
}

#[test]
fn all_math_content_boundaries_receive_typst_whitespace() {
    let part = |name: &str, text: &str, style: StructuredTextStyle| {
        gaanim_text::prelude::TextPart::new(name, vec![text.into()], style).into()
    };
    let spec = |content| {
        StructuredTextSpec::new(
            content,
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid structured equation")
    };

    let equation = spec(vec![
        "$".into(),
        part("sum_force", "sum F_t", StructuredTextStyle::default()),
        "=".into(),
        part("mass", "m", StructuredTextStyle::default()),
        part("acceleration", "a_t", StructuredTextStyle::default()),
        "$".into(),
    ]);
    assert_eq!(
        structured_typst_content(&equation, 32.0),
        "$sum F_t = m a_t$"
    );

    let tight_syntax = spec(vec![
        "$-".into(),
        part("variable", "x", StructuredTextStyle::default()),
        "_1".into(),
        "$".into(),
    ]);
    assert_eq!(structured_typst_content(&tight_syntax, 32.0), "$- x _1$");

    let colored = StructuredTextStyle {
        color: Some(PenikoColor::from_rgb8(255, 0, 0)),
        ..StructuredTextStyle::default()
    };
    let styled_right = spec(vec![
        "$".into(),
        part("left", "x", StructuredTextStyle::default()),
        "=".into(),
        part("right", "y", colored),
        "$".into(),
    ]);
    let styled_source = structured_typst_content(&styled_right, 32.0);
    assert!(styled_source.starts_with("$x = text("));
    assert!(!styled_source.contains("#h("));
    assert!(styled_source.contains("ff0000"));

    let display_wrapped = spec(vec![
        "$ ".into(),
        part("variable", "x", StructuredTextStyle::default()),
        "= 1".into(),
        " $".into(),
    ]);
    assert_eq!(
        structured_typst_content(&display_wrapped, 32.0),
        "$ x = 1 $"
    );

    let inline = spec(vec!["$x".into(), "+".into(), "1$".into()]);
    assert_eq!(structured_typst_content(&inline, 32.0), "$x + 1$");
}

#[test]
fn structured_inline_markup_emits_typst_styles_and_skips_math() {
    let spec = StructuredTextSpec::new(
        vec!["Normal, _emphasis_, *strong*, *_both_* y $x_1 * 5$.".into()],
        None,
        StructuredTextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    )
    .expect("valid inline markup");

    let source = structured_typst_content(&spec, 32.0);
    assert!(source.contains("style: \"italic\""));
    assert!(source.contains("weight: 700"));
    assert!(source.contains("$x_1 * 5$"));
    assert!(!source.contains("_emphasis_"));
    assert!(!source.contains("*strong*"));
}

#[test]
fn disabled_inline_markup_emits_literal_delimiters() {
    let spec = StructuredTextSpec::new_with_markup(
        vec!["tb:agriet_xy, *nota y $x_1$".into()],
        None,
        StructuredTextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
        false,
    )
    .expect("markup-free text accepts lone delimiters");

    let source = structured_typst_content(&spec, 32.0);
    assert!(source.contains("tb:agriet_xy, *nota y "));
    assert!(source.contains("$x_1$"));
    assert!(!source.contains("weight: 700"));
    assert!(!source.contains("italic"));
}

#[test]
fn explicit_math_boundary_spaces_match_implicit_typst_whitespace() {
    let equation = |with_boundary_spaces: bool| {
        StructuredTextSpec::new(
            vec![
                "$".into(),
                gaanim_text::prelude::TextPart::new(
                    "variable",
                    vec!["5".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                if with_boundary_spaces {
                    " 5 = ".into()
                } else {
                    "5 =".into()
                },
                gaanim_text::prelude::TextPart::new(
                    "result",
                    vec!["25".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                "$".into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid structured equation")
    };
    let mut canvas = SceneModel::new(640, 360);
    canvas.text_spec(equation(false));
    canvas.text_spec(equation(true));

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);

    let mut widths = world
        .query::<(&LocalBounds, Option<&bevy::prelude::ChildOf>)>()
        .iter(&world)
        .filter_map(|(bounds, parent)| parent.is_none().then_some(bounds.0.width()))
        .collect::<Vec<_>>();
    widths.sort_by(f64::total_cmp);
    assert_eq!(
        widths.len(),
        2,
        "expected two compiled text roots: {widths:?}"
    );
    assert!(
        (widths[1] - widths[0]).abs() < 0.01,
        "explicit and implicit boundary spaces must compile identically: {widths:?}"
    );
}

#[test]
fn local_math_fill_preserves_unstyled_typst_spacing_and_width() {
    let equation = |colored: bool| {
        let mut gravity_style = StructuredTextStyle::default();
        let mut acceleration_style = StructuredTextStyle::default();
        if colored {
            gravity_style.color = Some(PenikoColor::from_rgb8(255, 215, 0));
            acceleration_style.color = Some(PenikoColor::from_rgb8(255, 215, 0));
        }
        StructuredTextSpec::new(
            vec![
                "$ ".into(),
                "-".into(),
                gaanim_text::prelude::TextPart::new(
                    "mass_left",
                    vec!["m".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                gaanim_text::prelude::TextPart::new(
                    "gravity",
                    vec!["g sin(theta)".into()],
                    gravity_style,
                )
                .into(),
                "=".into(),
                gaanim_text::prelude::TextPart::new(
                    "mass_right",
                    vec!["m".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                gaanim_text::prelude::TextPart::new(
                    "length",
                    vec!["L".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                gaanim_text::prelude::TextPart::new(
                    "acceleration",
                    vec!["theta''".into()],
                    acceleration_style,
                )
                .into(),
                " $".into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid display equation")
    };

    let plain_source = structured_typst_content(&equation(false), 32.0);
    let colored_source = structured_typst_content(&equation(true), 32.0);
    assert_eq!(plain_source, "$ - m g sin(theta) = m L theta'' $");
    assert!(
        !colored_source.contains("#h("),
        "local color must not replace Typst parser spacing: {colored_source}"
    );

    let mut canvas = SceneModel::new(640, 360);
    canvas.text_spec(equation(false));
    canvas.text_spec(equation(true));
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);

    let mut widths = world
        .query::<(&LocalBounds, Option<&bevy::prelude::ChildOf>)>()
        .iter(&world)
        .filter_map(|(bounds, parent)| parent.is_none().then_some(bounds.0.width()))
        .collect::<Vec<_>>();
    widths.sort_by(f64::total_cmp);
    assert_eq!(widths.len(), 2, "expected two text roots: {widths:?}");
    assert!(
        (widths[1] - widths[0]).abs() < 0.01,
        "local fill changed equation width: {widths:?}"
    );
}

#[test]
fn every_local_math_style_stays_inside_one_typst_equation() {
    let style = StructuredTextStyle {
        font: Some("New Computer Modern".to_owned()),
        math_font: Some("New Computer Modern Math".to_owned()),
        fallbacks: vec!["Consolas".to_owned()],
        size: Some(36.0),
        weight: Some(700),
        italic: Some(false),
        color: Some(PenikoColor::from_rgb8(255, 215, 0)),
        stroke_color: Some(PenikoColor::from_rgb8(20, 20, 20)),
        stroke_width: Some(0.25),
        opacity: Some(0.8),
        letter_spacing: Some(0.25),
        word_spacing: Some(0.5),
        decorations: vec!["underline".to_owned()],
        baseline: Some(1.0),
    };
    let equation = StructuredTextSpec::new(
        vec![
            "$ ".into(),
            gaanim_text::prelude::TextPart::new(
                "left",
                vec!["x".into()],
                StructuredTextStyle::default(),
            )
            .into(),
            "=".into(),
            gaanim_text::prelude::TextPart::new("right", vec!["y".into()], style).into(),
            " $".into(),
        ],
        None,
        StructuredTextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    )
    .expect("valid styled display equation");

    let source = structured_typst_content(&equation, 32.0);
    assert!(
        source.starts_with("$ ") && source.ends_with(" $"),
        "{source}"
    );
    assert!(!source.contains("#h("), "{source}");

    let mut canvas = SceneModel::new(640, 360);
    canvas.text_spec(equation);
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);
    assert_eq!(
        world
            .query::<(&LocalBounds, Option<&bevy::prelude::ChildOf>)>()
            .iter(&world)
            .filter(|(_, parent)| parent.is_none())
            .count(),
        1
    );
    assert!(
        world
            .query::<&bevy::prelude::ChildOf>()
            .iter(&world)
            .count()
            > 0,
        "the styled equation must compile vector children"
    );
}

#[test]
fn implicit_sibling_part_boundary_compiles_as_valid_math_tokens() {
    let equation = StructuredTextSpec::new(
        vec![
            "$".into(),
            gaanim_text::prelude::TextPart::new(
                "left",
                vec!["m".into()],
                StructuredTextStyle::default(),
            )
            .into(),
            gaanim_text::prelude::TextPart::new(
                "right",
                vec!["g".into()],
                StructuredTextStyle::default(),
            )
            .into(),
            "$".into(),
        ],
        None,
        StructuredTextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    )
    .expect("valid structured equation");
    assert_eq!(structured_typst_content(&equation, 32.0), "$m g$");
    let mut canvas = SceneModel::new(640, 360);
    canvas.text_spec(equation);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);

    let widths = world
        .query::<(&LocalBounds, Option<&bevy::prelude::ChildOf>)>()
        .iter(&world)
        .filter_map(|(bounds, parent)| parent.is_none().then_some(bounds.0.width()))
        .collect::<Vec<_>>();
    assert_eq!(widths.len(), 1, "expected one text root: {widths:?}");
    assert!(widths[0] > 0.0, "compiled math must have positive width");
}

#[test]
fn camera_and_drawable_play_compile_at_the_same_start_time() {
    let mut canvas = SceneModel::new(640, 360);
    let marker = canvas.circle(20.0);
    let marker_anim = marker.fade_in(2.0);
    let camera_anim = canvas.camera_orbit(0.4, 0.1, 2.0);
    canvas.play(vec![marker_anim, camera_anim]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let parallel_clips: Vec<_> = timeline
        .clips
        .values()
        .filter(|clip| (clip.duration - 2.0).abs() < 1e-9)
        .collect();
    assert_eq!(parallel_clips.len(), 2); // fade + one atomic orbit pose
    assert!(parallel_clips.iter().any(|clip| matches!(
        &clip.payload,
        gaanim_timeline::clip::ClipPayload::Animation(animation)
            if matches!(
                animation.lens,
                gaanim_timeline::clip::PropertyLensSpec::CameraOrbit { .. }
            )
    )));
    assert!(parallel_clips.iter().all(|clip| clip.start.abs() < 1e-9));
    assert!((timeline.cached_duration - 2.0).abs() < 1e-9);
}

fn compile_camera_timeline(canvas: SceneModel) -> (World, Timeline) {
    let mut world = World::new();
    world.insert_resource(gaanim_math::Camera::ortho_2d(1280, 720));
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    (world, timeline)
}

/// pan_to a drawable that follows a reactive point ends on the point,
/// where the drawable is drawn, not where it was declared, and the next
/// camera move starts from there.
#[test]
fn pan_to_a_follower_ends_where_it_is_drawn() {
    let mut canvas = SceneModel::new(960, 540);
    let x = canvas.parameter(0.0).unwrap();
    let rider = canvas.circle(10.0).follow_endpoint(
        CanvasEndpoint::Expression {
            x: x.source(),
            y: ScalarSource::Constant(40.0),
            z: ScalarSource::Constant(0.0),
        },
        DVec3::ZERO,
        gaanim_animation::FollowOffsetSpace::World,
    );
    canvas.play(vec![x.animate().set(120.0).duration(1.0)]);
    let pan = canvas.camera_pan_to_endpoint(CanvasEndpoint::Entity(rider.id), 1.0);
    canvas.play(vec![pan]);
    let zoom = canvas.camera_zoom_to(2.0, 1.0);
    canvas.play(vec![zoom]);

    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    for time in [2.0, 2.5, 3.0] {
        timeline.seek(&mut world, time);
        let position = world.resource::<gaanim_math::Camera>().position;
        assert!(
            (position.x - 120.0).abs() < 1e-6 && (position.y - 40.0).abs() < 1e-6,
            "camera at {position:?} at {time}"
        );
    }
}

/// Orbits of a camera with z up chain: the second turn starts where the
/// first ended, so two half turns with opposite tilts end at the pose
/// they started from.
#[test]
fn chained_orbits_of_a_z_up_camera_return_to_their_start() {
    let mut canvas = SceneModel::new(960, 540);
    let _marker = canvas.circle(1.0);
    let perspective = canvas.camera_perspective(0.8, 0.1, 1000.0, 0.0);
    canvas.play(vec![perspective]);
    let aim = canvas.camera_look_at_endpoints(
        Some(CanvasEndpoint::Static(DVec3::new(12.0, -7.0, 3.0))),
        CanvasEndpoint::Static(DVec3::new(0.0, 0.0, 0.5)),
        DVec3::Z,
        0.5,
    );
    canvas.play(vec![aim]);
    let rise = canvas.camera_orbit(std::f64::consts::PI, 0.35, 1.0);
    canvas.play(vec![rise]);
    let fall = canvas.camera_orbit(std::f64::consts::PI, -0.35, 1.0);
    canvas.play(vec![fall]);

    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    timeline.seek(&mut world, 0.5);
    let start = world.resource::<gaanim_math::Camera>().position;
    assert!(
        (start - DVec3::new(12.0, -7.0, 3.0)).length() < 1e-6,
        "start {start:?}"
    );
    timeline.seek(&mut world, 1.5);
    let middle = world.resource::<gaanim_math::Camera>().position;
    timeline.seek(&mut world, 2.5);
    let end = world.resource::<gaanim_math::Camera>().position;
    assert!(
        (end - start).length() < 1e-6,
        "start {start:?}, middle {middle:?}, end {end:?}"
    );
}

#[test]
fn depth_of_field_clips_set_the_resource_and_seeks_restore_it() {
    let mut canvas = SceneModel::new(960, 540);
    let layer = canvas.parallax_layer(3.0).unwrap();
    let hill = canvas.circle(1.0);
    canvas.add_to_parallax_layer(&layer, &hill).unwrap();
    let set = canvas
        .camera_depth_of_field(Some(0.5), Some(0.12), None, 1.0)
        .unwrap()
        .duration(0.0);
    canvas.play(vec![set]);
    canvas.wait(0.5);
    let rack = canvas
        .camera_depth_of_field(Some(3.0), None, None, 1.0)
        .unwrap();
    canvas.play(vec![rack]);

    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    let dof = |world: &World| *world.resource::<gaanim_scene::DepthOfField>();
    timeline.seek(&mut world, 0.25);
    assert_eq!(dof(&world).focus, 0.5);
    assert_eq!(dof(&world).aperture, 0.12);
    assert!((dof(&world).blur_at(3.0) - 0.1).abs() < 1e-9);
    timeline.seek(&mut world, 2.0);
    assert!((dof(&world).focus - 3.0).abs() < 1e-9);
    timeline.seek(&mut world, 0.25);
    assert_eq!(dof(&world).focus, 0.5);
}

#[test]
fn follow_and_dynamic_frames_keep_the_pose_where_their_targets_end() {
    use gaanim_animation::FollowOffsetSpace;
    let mut canvas = SceneModel::new(960, 540);
    let rover = canvas.circle(0.5).move_to(-4.0, 0.0);
    let follow = canvas.camera_follow_endpoint(
        crate::canvas::CanvasEndpoint::Entity(rover.id),
        DVec3::new(0.0, 1.0, 0.0),
        FollowOffsetSpace::World,
        0.0,
        2.0,
    );
    // The rover's move comes after the camera's in the same play.
    canvas.play(vec![
        follow,
        rover.animate().move_to(4.0, 0.0).duration(2.0),
    ]);
    canvas.wait(1.0);
    let pan = canvas.camera_pan_to(4.0, 5.0, 1.0);
    canvas.play(vec![pan]);
    let card = canvas.rect(2.0, 1.0).move_to(-2.0, -2.0);
    let frame = canvas.camera_frame_many(std::slice::from_ref(&card), [0.5; 4], true, 1.0);
    canvas.play(vec![frame, card.animate().move_to(3.0, -3.0).duration(1.0)]);
    canvas.wait(1.0);

    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    let pose = |world: &World| {
        let camera = world.resource::<gaanim_math::Camera>();
        let gaanim_math::Projection::Orthographic { zoom } = camera.projection else {
            panic!("orthographic camera");
        };
        (camera.position.truncate(), zoom)
    };
    // After the follow the camera stays over the rover's end, offset.
    timeline.seek(&mut world, 2.5);
    assert!((pose(&world).0 - DVec2::new(4.0, 1.0)).length() < 1e-9);
    // The next move starts there instead of cutting back to the origin.
    timeline.seek(&mut world, 3.0);
    assert!((pose(&world).0 - DVec2::new(4.0, 1.0)).length() < 1e-9);
    timeline.seek(&mut world, 4.0);
    assert!((pose(&world).0 - DVec2::new(4.0, 5.0)).length() < 1e-9);
    // The dynamic frame keeps the framing of the card where it ends.
    timeline.seek(&mut world, 5.5);
    let (center, zoom) = pose(&world);
    assert!(
        (center - DVec2::new(3.0, -3.0)).length() < 1e-9,
        "{center:?}"
    );
    // A 3 x 2 box (the card plus its margins) in the 960 x 540 frame.
    assert!(
        (zoom - (540.0 / 2.0f64).min(960.0 / 3.0)).abs() < 1e-9,
        "{zoom}"
    );
}

#[test]
fn a_camera_at_an_angle_keeps_its_framing_where_the_followed_drawable_ends() {
    use gaanim_animation::FollowOffsetSpace;
    let mut canvas = SceneModel::new(960, 540);
    let rover = canvas.circle(0.5);
    let eye = DVec3::new(0.0, 4.0, 10.0);
    let look = canvas.camera_look_at((0.0, 4.0, 10.0), (0.0, 0.0, 0.0), None, 0.5);
    let perspective = canvas.camera_perspective(0.8, 0.1, 100.0, 0.5);
    canvas.play(vec![look, perspective]);
    let follow = canvas.camera_follow_endpoint(
        crate::canvas::CanvasEndpoint::Entity(rover.id),
        DVec3::new(0.0, 0.0, -1.0),
        FollowOffsetSpace::World,
        0.0,
        2.0,
    );
    canvas.play(vec![
        follow,
        rover.animate().move_to(4.0, 1.0).duration(2.0),
    ]);
    canvas.wait(1.0);
    let orbit = canvas.camera_orbit(0.5, 0.0, 1.0);
    canvas.play(vec![orbit]);

    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    let camera = |world: &World| *world.resource::<gaanim_math::Camera>();
    timeline.seek(&mut world, 0.5);
    let before = camera(&world);
    // After the follow the camera looks at the rover's end, offset, as
    // it looked at the origin: moved, not turned.
    let held = DVec3::new(4.0, 1.0, -1.0);
    timeline.seek(&mut world, 3.0);
    let after = camera(&world);
    assert!((after.target - held).length() < 1e-9, "{:?}", after.target);
    assert!(
        (after.position - (eye + held)).length() < 1e-9,
        "{:?}",
        after.position
    );
    assert!((after.rotation.dot(before.rotation).abs() - 1.0).abs() < 1e-12);
    let center = after.world_to_screen(held)
        - DVec2::new(
            f64::from(after.viewport_width) / 2.0,
            f64::from(after.viewport_height) / 2.0,
        );
    assert!(center.length() < 1e-6, "{center:?}");
    // The orbit that follows turns around that point.
    timeline.seek(&mut world, 4.5);
    let orbited = camera(&world);
    assert!((orbited.target - held).length() < 1e-9);
    assert!(((orbited.position - held).length() - eye.length()).abs() < 1e-9);
}

#[test]
fn a_camera_that_looks_at_a_moving_drawable_turns_with_it_and_keeps_where_it_ends() {
    let mut canvas = SceneModel::new(1280, 720);
    let cube = canvas
        .cube(0.5, gaanim_scene::Material3D::default())
        .unwrap();
    let eye = DVec3::new(0.0, 4.0, 10.0);
    let look = canvas.camera_look_at((0.0, 4.0, 10.0), (0.0, 0.0, 0.0), None, 0.0);
    let perspective = canvas.camera_perspective(0.8, 0.1, 100.0, 0.0);
    canvas.play(vec![look, perspective]);
    // From where it stands, the camera turns to the cube as it moves.
    let aim = canvas
        .camera_look_at_endpoints(None, CanvasEndpoint::Entity(cube.id), DVec3::Y, 2.0)
        .rate_func(gaanim_math::RateFunc::Linear);
    canvas.play(vec![
        aim,
        cube.animate()
            .move_to_3d(4.0, 1.0, -2.0)
            .duration(2.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    let orbit = canvas.camera_orbit(0.5, 0.0, 1.0);
    canvas.play(vec![orbit]);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    // What the camera phase shows: the timeline's pose with the rig's.
    let rig_at = |world: &mut World, timeline: &mut Timeline, time: f64| {
        timeline.seek(world, time);
        world.insert_resource(std::mem::replace(timeline, Timeline::new()));
        gaanim_timeline::camera_binding_system(world);
        gaanim_timeline::camera_rig_system(world);
        *timeline = world.remove_resource::<Timeline>().unwrap();
        world.resource::<gaanim_math::CameraRigCamera>().0
    };
    let end = DVec3::new(4.0, 1.0, -2.0);
    // Halfway the camera has turned halfway toward where the cube is
    // then, without moving.
    let middle = rig_at(&mut world, &mut timeline, 1.0);
    assert!(
        (middle.position - eye).length() < 1e-9,
        "{:?}",
        middle.position
    );
    assert!(
        (middle.target - end * 0.25).length() < 1e-9,
        "{:?}",
        middle.target
    );
    let center = middle.world_to_screen(end * 0.25) - DVec2::new(640.0, 360.0);
    assert!(center.length() < 1e-6, "{center:?}");
    // Once it ends it keeps looking where the cube ended, and the orbit
    // after it turns around that point.
    let ended = rig_at(&mut world, &mut timeline, 2.0);
    assert!((ended.target - end).length() < 1e-9, "{:?}", ended.target);
    assert!((ended.position - eye).length() < 1e-9);
    let orbited = rig_at(&mut world, &mut timeline, 3.0);
    assert!(
        (orbited.target - end).length() < 1e-9,
        "{:?}",
        orbited.target
    );
    assert!(((orbited.position - end).length() - (eye - end).length()).abs() < 1e-9);
}

#[test]
fn a_camera_that_follows_turning_keeps_its_eye_and_the_target_centered() {
    use gaanim_animation::FollowOffsetSpace;
    let mut canvas = SceneModel::new(1280, 720);
    let cube = canvas
        .cube(0.5, gaanim_scene::Material3D::default())
        .unwrap();
    let eye = DVec3::new(0.0, 4.0, 10.0);
    let look = canvas.camera_look_at((0.0, 4.0, 10.0), (0.0, 0.0, 0.0), None, 0.0);
    let perspective = canvas.camera_perspective(0.8, 0.1, 100.0, 0.0);
    canvas.play(vec![look, perspective]);
    let follow = canvas.camera_follow_endpoint_with(
        CanvasEndpoint::Entity(cube.id),
        DVec3::ZERO,
        FollowOffsetSpace::World,
        0.0,
        true,
        2.0,
    );
    canvas.play(vec![
        follow,
        cube.animate()
            .move_to_3d(4.0, 1.0, -2.0)
            .duration(2.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    let orbit = canvas.camera_orbit(0.5, 0.0, 1.0);
    canvas.play(vec![orbit]);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    let rig_at = |world: &mut World, timeline: &mut Timeline, time: f64| {
        timeline.seek(world, time);
        world.insert_resource(std::mem::replace(timeline, Timeline::new()));
        gaanim_timeline::camera_binding_system(world);
        gaanim_timeline::camera_rig_system(world);
        *timeline = world.remove_resource::<Timeline>().unwrap();
        world.resource::<gaanim_math::CameraRigCamera>().0
    };
    let end = DVec3::new(4.0, 1.0, -2.0);
    // Halfway the eye has not moved and the cube is at the center.
    let middle = rig_at(&mut world, &mut timeline, 1.0);
    assert!(
        (middle.position - eye).length() < 1e-9,
        "{:?}",
        middle.position
    );
    let center = middle.world_to_screen(end * 0.5) - DVec2::new(640.0, 360.0);
    assert!(center.length() < 1e-6, "{center:?}");
    // Once it ends the camera keeps looking where the cube ended, and
    // the orbit after it turns around that point.
    let ended = rig_at(&mut world, &mut timeline, 2.0);
    assert!(
        (ended.position - eye).length() < 1e-9,
        "{:?}",
        ended.position
    );
    assert!((ended.target - end).length() < 1e-9, "{:?}", ended.target);
    let center = ended.world_to_screen(end) - DVec2::new(640.0, 360.0);
    assert!(center.length() < 1e-6, "{center:?}");
    let orbited = rig_at(&mut world, &mut timeline, 3.0);
    assert!(
        (orbited.target - end).length() < 1e-9,
        "{:?}",
        orbited.target
    );
    assert!(((orbited.position - end).length() - (eye - end).length()).abs() < 1e-9);
}

#[test]
fn a_camera_that_follows_a_part_of_a_model_keeps_where_its_animation_takes_it() {
    use bevy::prelude::App;
    use gaanim_animation::{FollowOffsetSpace, TrackingEndpoint};
    let mut canvas = SceneModel::new(1280, 720);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/fixtures/assets/Wolf.glb");
    let wolf = canvas.gltf(&path, &Default::default()).unwrap();
    let head = wolf.part("Head").unwrap();
    let look = canvas.camera_look_at((0.0, 4.0, 10.0), (0.0, 0.0, 0.0), None, 0.0);
    let perspective = canvas.camera_perspective(0.8, 0.1, 100.0, 0.0);
    canvas.play(vec![look, perspective]);
    // The follow ends early in the headbutt, with the head away from
    // where it rests.
    let headbutt = wolf
        .animation("Headbutt", Some(2.0), 1.0, false, false, 0.0, 0.0)
        .unwrap();
    let follow = canvas.camera_follow_endpoint(
        CanvasEndpoint::Entity(head.id),
        DVec3::ZERO,
        FollowOffsetSpace::World,
        0.0,
        0.2,
    );
    canvas.play(vec![follow, headbutt]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let head = entity_of(app.world_mut(), &head);
    let seek = |app: &mut App, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
    };
    let head_at = |app: &mut App, time: f64| {
        seek(app, time);
        gaanim_animation::resolve_tracking_endpoint(&TrackingEndpoint::Entity(head), app.world())
            .unwrap()
    };
    let rest = head_at(&mut app, 0.0);
    let ended = head_at(&mut app, 0.2);
    assert!((ended - rest).length() > 0.01, "{rest:?} {ended:?}");
    // After the follow the camera looks where the head was as it ended,
    // not where it rests.
    seek(&mut app, 0.3);
    let held = app.world().resource::<gaanim_math::Camera>().target;
    assert!((held - ended).length() < 1e-6, "{held:?} {ended:?}");
}

#[test]
fn a_3d_line_lies_between_its_endpoints_and_grows_from_its_start() {
    use bevy::prelude::App;
    let mut canvas = SceneModel::new(1280, 720);
    let material = gaanim_scene::Material3D::default();
    let start = canvas
        .sphere(0.2, 16, 8, material)
        .unwrap()
        .move_to_3d(-1.0, 0.0, 0.0);
    let end = canvas
        .sphere(0.2, 16, 8, material)
        .unwrap()
        .move_to_3d(1.0, 0.0, 0.0);
    let line = canvas
        .line_3d(
            CanvasEndpoint::Entity(start.id),
            CanvasEndpoint::Entity(end.id),
            0.1,
            12,
            true,
            material,
        )
        .unwrap();
    canvas.play(vec![line.animate().create().duration(1.0)]);
    canvas.play(vec![end.animate().move_to_3d(1.0, 2.0, 2.0).duration(1.0)]);
    canvas.play(vec![line.animate().uncreate().duration(1.0)]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    // As a host does after a replay: the t=0 keyframe holds the scene, so
    // seeking back restores it.
    app.world_mut().resource_mut::<Timeline>().keyframes.clear();
    gaanim_timeline::capture_reload_keyframe(app.world_mut());
    let line = entity_of(app.world_mut(), &line);
    let at = |app: &mut App, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        *app.world().get::<SpatialTransform>(line).unwrap()
    };
    let near = |a: DVec3, b: DVec3| a.distance(b) < 1e-6;
    // Before `create` it is not there, caps and all.
    assert_eq!(at(&mut app, 0.0).scale, DVec3::ZERO);
    // Halfway through, it reaches halfway from its start, as thick as ever.
    let half = at(&mut app, 0.5);
    assert!(
        near(half.translation, DVec3::new(-0.5, 0.0, 0.0)),
        "{half:?}"
    );
    assert!(near(half.scale, DVec3::new(1.0, 1.0, 1.0)), "{half:?}");
    // It follows the end where it moves.
    let (from, to) = (DVec3::new(-1.0, 0.0, 0.0), DVec3::new(1.0, 2.0, 2.0));
    let moved = at(&mut app, 2.0);
    assert!(near(moved.translation, (from + to) / 2.0), "{moved:?}");
    assert!(
        (moved.scale.y - from.distance(to)).abs() < 1e-6,
        "{moved:?}"
    );
    assert!(
        near(moved.rotation * DVec3::Y, (to - from).normalize()),
        "{moved:?}"
    );
    // `uncreate` takes it back to its start.
    let leaving = at(&mut app, 2.5);
    assert!(
        near(leaving.translation, from + (to - from) / 4.0),
        "{leaving:?}"
    );
    assert_eq!(at(&mut app, 3.0).scale, DVec3::ZERO);
    // A seek back grows it again.
    let again = at(&mut app, 0.5);
    assert!(
        near(again.translation, DVec3::new(-0.5, 0.0, 0.0)),
        "{again:?}"
    );
}

#[test]
fn a_sphere_morphs_into_a_cube_and_a_torus_where_they_stand_and_seeks_back() {
    use bevy::prelude::App;
    use gaanim_scene::{LocalBounds, Material3D, Opacity, TriangleMeshData};
    let mut canvas = SceneModel::new(1280, 720);
    let red = Material3D::matte(PenikoColor::from_rgb8(255, 0, 0));
    let blue = Material3D::matte(PenikoColor::from_rgb8(0, 0, 255));
    let sphere = canvas.sphere(1.0, 32, 16, red).unwrap();
    let cube = canvas.cube(2.0, blue).unwrap().move_to_3d(3.0, 0.0, 0.0);
    let torus = canvas
        .torus(1.0, 0.4, 32, 16, red)
        .unwrap()
        .move_to_3d(0.0, 3.0, 0.0);
    canvas.play(vec![
        sphere.animate().transform_to(&cube).unwrap().duration(1.0),
    ]);
    canvas.play(vec![
        sphere.animate().transform_to(&torus).unwrap().duration(1.0),
    ]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    app.world_mut().resource_mut::<Timeline>().keyframes.clear();
    gaanim_timeline::capture_reload_keyframe(app.world_mut());
    let (sphere, cube) = (
        entity_of(app.world_mut(), &sphere),
        entity_of(app.world_mut(), &cube),
    );
    let at = |app: &mut App, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        let world = app.world();
        (
            world
                .get::<TriangleMeshData>(sphere)
                .unwrap()
                .vertices
                .len(),
            world.get::<SpatialTransform>(sphere).unwrap().translation,
            world.get::<Material3D>(sphere).unwrap().color,
            world.get::<LocalBounds>(sphere).unwrap().0,
        )
    };
    let grid = (gaanim_objects::morph3d::COLUMNS + 1) * (gaanim_objects::morph3d::ROWS + 1);
    let near = |a: DVec3, b: DVec3| a.distance(b) < 1e-6;

    // The sphere shows its own mesh until the morph starts.
    let (vertices, place, _, _) = at(&mut app, 0.0);
    assert_eq!((vertices, place), (33 * 17, DVec3::ZERO));
    // Halfway it is the grid, halfway to the cube and between colors,
    // in the bounds the sphere and the cube share.
    let (vertices, place, color, bounds) = at(&mut app, 0.5);
    assert_eq!(vertices, grid);
    assert!(near(place, DVec3::new(1.5, 0.0, 0.0)), "{place}");
    let rgba = color.to_rgba8();
    assert!(rgba.r > 64 && rgba.b > 64, "{rgba:?}");
    assert!(
        near(bounds.max, DVec3::ONE) && near(bounds.min, -DVec3::ONE),
        "{bounds:?}"
    );
    // The template stays hidden.
    assert_eq!(app.world().get::<Opacity>(cube).unwrap().0, 0.0);
    // Then it is the cube where the cube stands, until the next morph
    // takes it on to the torus.
    let (vertices, place, color, _) = at(&mut app, 1.0);
    assert_eq!(vertices, 24);
    assert!(near(place, DVec3::new(3.0, 0.0, 0.0)), "{place}");
    assert_eq!(color.to_rgba8().b, 255);
    assert_eq!(at(&mut app, 1.5).0, grid);
    let (vertices, place, _, _) = at(&mut app, 2.0);
    assert_eq!(vertices, 33 * 17);
    assert!(near(place, DVec3::new(0.0, 3.0, 0.0)), "{place}");
    // Seeking back blends again, and before it shows the sphere.
    assert_eq!(at(&mut app, 0.25).0, grid);
    assert_eq!(at(&mut app, 0.0), at(&mut app, 0.0));
    assert_eq!(at(&mut app, 0.0).0, 33 * 17);
}

#[test]
fn a_seek_past_a_morph_in_an_earlier_segment_shows_the_mesh_it_ended_on() {
    use bevy::prelude::App;
    use gaanim_scene::{Material3D, TriangleMeshData};
    let mut canvas = SceneModel::new(1280, 720);
    canvas.segment("morph", None).unwrap();
    let material = Material3D::default();
    let sphere = canvas.sphere(1.0, 32, 16, material).unwrap();
    let cube = canvas.cube(2.0, material).unwrap();
    canvas.play(vec![
        sphere.animate().transform_to(&cube).unwrap().duration(1.0),
    ]);
    canvas.segment("after", None).unwrap();
    canvas.wait(2.0);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let sphere = entity_of(app.world_mut(), &sphere);
    let at = |app: &mut App, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        app.world()
            .get::<TriangleMeshData>(sphere)
            .unwrap()
            .vertices
            .len()
    };
    // Straight into the next segment, then back before the morph and
    // forward again.
    assert_eq!(at(&mut app, 2.0), 24);
    assert_eq!(at(&mut app, 0.0), 33 * 17);
    assert_eq!(at(&mut app, 2.5), 24);
}

/// A field's isosurface is a lit mesh with normals, and its volume a
/// drawable bounded by the field's box that holds its values, mapped
/// over their range, and the colors of its values.
#[test]
fn isosurfaces_and_volumes_spawn_from_a_sampled_field() {
    use bevy::prelude::App;
    use gaanim_objects::isosurface::SampledField;
    use gaanim_scene::{LocalBounds, TriangleMeshData, VolumeData};
    let n = 12;
    let values = (0..n * n * n)
        .map(|index| {
            let at = |step: usize| -1.0 + 2.0 * (step as f32) / (n - 1) as f32;
            let (x, y, z) = (at(index % n), at(index / n % n), at(index / (n * n)));
            x * x + y * y + z * z
        })
        .collect();
    let field = SampledField {
        values,
        size: [n; 3],
        min: DVec3::splat(-1.0),
        max: DVec3::new(1.0, 1.0, 2.0),
    };
    let mut canvas = SceneModel::new(1280, 720);
    let shell = canvas
        .isosurface(&field, 0.5, gaanim_scene::Material3D::default())
        .unwrap();
    let mut transfer = [[0_u8; 4]; 256];
    transfer[255] = [255, 128, 0, 255];
    let cloud = canvas.volume(&field, &transfer, None, 2.0);
    assert!(canvas.isosurface(&field, 9.0, Default::default()).is_err());
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let shell = entity_of(app.world_mut(), &shell);
    let cloud = entity_of(app.world_mut(), &cloud);
    let world = app.world();
    let mesh = world.get::<TriangleMeshData>(shell).unwrap();
    assert!(mesh.material.is_some());
    assert_eq!(
        mesh.normals.as_ref().map(Vec::len),
        Some(mesh.vertices.len())
    );
    let volume = world.get::<VolumeData>(cloud).unwrap();
    assert_eq!(volume.size, [n as u32; 3]);
    assert_eq!((volume.min, volume.max), ([-1.0; 3], [1.0, 1.0, 2.0]));
    assert_eq!(volume.density, 2.0);
    // The lowest values, about the middle, map to 0; the highest, at the
    // far corners, to 255.
    let texels = volume.texels().unwrap();
    assert_eq!(texels.iter().min(), Some(&0));
    assert_eq!(texels[texels.len() - 1], 255);
    assert_eq!(&volume.transfer.data.data()[255 * 4..], &[255, 128, 0, 255]);
    let bounds = world.get::<LocalBounds>(cloud).unwrap().0;
    assert_eq!(
        (bounds.min, bounds.max),
        (DVec3::splat(-1.0), DVec3::new(1.0, 1.0, 2.0))
    );
}

#[test]
fn copies_grow_in_and_move_to_another_set_through_seeks() {
    use bevy::prelude::App;
    use gaanim_scene::{LocalBounds, Material3D, MeshInstances, TriangleMeshData};
    let mut canvas = SceneModel::new(1280, 720);
    let material = Material3D::default();
    let row = canvas
        .points_3d(
            &[[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            0.1,
            (8, 4),
            None,
            material,
        )
        .unwrap();
    let column = canvas
        .points_3d(
            &[[0.0; 3], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0]],
            0.1,
            (8, 4),
            None,
            material,
        )
        .unwrap();
    canvas.play(vec![row.animate().create().duration(1.0)]);
    canvas.play(vec![
        row.animate().transform_to(&column).unwrap().duration(1.0),
    ]);
    canvas.play(vec![row.animate().uncreate().duration(1.0)]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let row = entity_of(app.world_mut(), &row);
    let at = |app: &mut App, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        let world = app.world();
        let instances = world.get::<MeshInstances>(row).unwrap();
        (
            instances.reveal,
            instances.copies.positions.clone(),
            world.get::<LocalBounds>(row).unwrap().0,
            world.get::<TriangleMeshData>(row).unwrap().vertices.len(),
        )
    };
    let near = |a: DVec3, b: DVec3| a.distance(b) < 1e-6;

    // Hidden before `create()`, then grown in, around the row.
    let (reveal, positions, bounds, vertices) = at(&mut app, 0.0);
    assert_eq!((reveal, vertices), (0.0, 9 * 5));
    assert_eq!(positions[2], [2.0, 0.0, 0.0]);
    assert!(
        near(bounds.min, DVec3::splat(-0.1)) && near(bounds.max, DVec3::new(2.1, 0.1, 0.1)),
        "{bounds:?}"
    );
    assert!((at(&mut app, 0.5).0 - 0.5).abs() < 1e-6);
    assert_eq!(at(&mut app, 1.0).0, 1.0);
    // Halfway through the transform each copy is halfway to its
    // place in the column, and the mesh they copy stays.
    let (reveal, positions, bounds, vertices) = at(&mut app, 1.5);
    assert_eq!((reveal, vertices), (1.0, 9 * 5));
    let halfway = DVec3::from(positions[2].map(f64::from));
    assert!(near(halfway, DVec3::new(1.0, 1.0, 0.0)), "{halfway}");
    assert!(near(bounds.max, DVec3::new(1.1, 1.1, 0.1)), "{bounds:?}");
    // Then the column, around the column.
    let (_, positions, bounds, _) = at(&mut app, 2.0);
    assert_eq!(positions[2], [0.0, 2.0, 0.0]);
    assert!(near(bounds.max, DVec3::new(0.1, 2.1, 0.1)), "{bounds:?}");
    // `uncreate()` shrinks the column's copies away in the same order.
    assert!((at(&mut app, 2.5).0 - 0.5).abs() < 1e-6);
    assert_eq!(at(&mut app, 3.0).0, 0.0);
    // Seeking back shows the row again, partly grown.
    let (reveal, positions, bounds, _) = at(&mut app, 0.5);
    assert!((reveal - 0.5).abs() < 1e-6);
    assert_eq!(positions[2], [2.0, 0.0, 0.0]);
    assert!(near(bounds.max, DVec3::new(2.1, 0.1, 0.1)), "{bounds:?}");
}

#[test]
fn copies_of_a_mesh_check_what_they_copy() {
    use crate::mesh_copies::InstancesError;
    use gaanim_scene::{Material3D, MeshCopies};
    let mut canvas = SceneModel::new(320, 180);
    let material = Material3D::default();
    let cube = canvas.cube(1.0, material).unwrap();
    let at = |positions: Vec<[f32; 3]>| MeshCopies {
        positions,
        ..MeshCopies::default()
    };
    let error =
        |canvas: &mut SceneModel, copies: MeshCopies| canvas.instances(&cube, copies, true).err();
    assert_eq!(
        error(&mut canvas, at(Vec::new())),
        Some(InstancesError::NoPositions)
    );
    let uneven = MeshCopies {
        scales: Some(vec![[1.0; 3]]),
        ..at(vec![[0.0; 3]; 2])
    };
    assert_eq!(
        error(&mut canvas, uneven),
        Some(InstancesError::LengthMismatch)
    );
    assert_eq!(
        error(&mut canvas, at(vec![[f32::NAN, 0.0, 0.0]])),
        Some(InstancesError::NotFinite)
    );
    let unturned = MeshCopies {
        rotations: Some(vec![[0.0; 4]]),
        ..at(vec![[0.0; 3]])
    };
    assert_eq!(
        error(&mut canvas, unturned),
        Some(InstancesError::NotFinite)
    );
    // Copies are not copied again, nor are meshes of another scene or
    // drawables that are not primitives.
    let copies = canvas
        .instances(&cube, at(vec![[0.0; 3]; 2]), true)
        .unwrap();
    assert_eq!(
        canvas.instances(&copies, at(vec![[0.0; 3]]), true).err(),
        Some(InstancesError::NotAPrimitive)
    );
    let circle = canvas.circle(1.0);
    assert_eq!(
        canvas.instances(&circle, at(vec![[0.0; 3]]), true).err(),
        Some(InstancesError::NotAPrimitive)
    );
    let rod = canvas
        .line_3d(
            CanvasEndpoint::Static(DVec3::ZERO),
            CanvasEndpoint::Static(DVec3::X),
            0.1,
            8,
            true,
            material,
        )
        .unwrap();
    assert_eq!(
        canvas.instances(&rod, at(vec![[0.0; 3]]), true).err(),
        Some(InstancesError::NotAPrimitive)
    );
    let mut other = SceneModel::new(320, 180);
    assert_eq!(
        other.instances(&cube, at(vec![[0.0; 3]]), true).err(),
        Some(InstancesError::ForeignPrototype)
    );
    // Copies transform only into as many copies.
    let three = canvas
        .instances(&cube, at(vec![[0.0; 3]; 3]), true)
        .unwrap();
    assert!(copies.animate().transform_to(&three).is_err());
    assert!(copies.animate().transform_to(&cube).is_err());
    assert!(cube.animate().transform_to(&copies).is_err());
    let two = canvas
        .instances(&cube, at(vec![[1.0; 3]; 2]), false)
        .unwrap();
    assert!(copies.animate().transform_to(&two).is_ok());
}

#[test]
fn a_3d_mesh_transforms_only_into_a_mesh_lit_as_it_is() {
    let mut canvas = SceneModel::new(320, 180);
    let material = gaanim_scene::Material3D::default();
    let sphere = canvas.sphere(1.0, 16, 8, material).unwrap();
    let circle = canvas.circle(1.0);
    let surface = canvas.surface_mesh(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![0, 1, 2],
        None,
    );
    assert!(sphere.animate().transform_to(&circle).is_err());
    assert!(circle.animate().transform_to(&sphere).is_err());
    assert!(sphere.animate().replacement_transform_to(&surface).is_err());
    let cube = canvas.cube(1.0, material).unwrap();
    assert!(sphere.animate().transform_to(&cube).is_ok());
    let square = canvas.square(1.0);
    assert!(circle.animate().transform_to(&square).is_ok());
}

#[test]
fn a_camera_that_follows_a_drawable_an_updater_moves_keeps_where_it_ends() {
    use bevy::prelude::App;
    use gaanim_animation::{FollowOffsetSpace, TrackingEndpoint};
    let mut canvas = SceneModel::new(1280, 720);
    let comet = canvas.circle(0.3);
    comet.add_updater(crate::canvas::UpdaterPreset::Orbit {
        cx: 0.0,
        cy: 0.0,
        radius: 3.0,
        speed: 1.0,
    });
    let follow = canvas.camera_follow_endpoint(
        CanvasEndpoint::Entity(comet.id),
        DVec3::ZERO,
        FollowOffsetSpace::World,
        0.0,
        1.0,
    );
    canvas.play(vec![follow]);
    // A pan after it starts where the follow ended.
    let pan = canvas.camera_pan_to(0.0, 0.0, 1.0);
    canvas.play(vec![pan]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let comet = entity_of(app.world_mut(), &comet);
    let seek = |app: &mut App, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
    };
    seek(&mut app, 1.0);
    let ended =
        gaanim_animation::resolve_tracking_endpoint(&TrackingEndpoint::Entity(comet), app.world())
            .unwrap();
    // Where the comet was as the follow ended, not where it was authored.
    assert!(ended.truncate().distance(gaanim_core::glam::DVec2::ZERO) > 1.0);
    seek(&mut app, 1.0 + 1e-6);
    let held = app.world().resource::<gaanim_math::Camera>().position;
    assert!(
        (held.truncate() - ended.truncate()).length() < 1e-3,
        "{held:?} {ended:?}"
    );
}

#[test]
fn a_camera_on_a_saved_pose_keeps_a_drawable_moved_in_3d_centered_after_following_it() {
    use gaanim_animation::FollowOffsetSpace;
    let mut canvas = SceneModel::new(960, 540);
    let home = canvas
        .camera_state_3d(
            DVec3::new(7.0, 5.0, 10.0),
            DVec3::new(0.0, -1.0, 0.0),
            DVec3::Y,
            0.75,
            0.1,
            100.0,
        )
        .unwrap();
    let cut = canvas.camera_to(&home, 0.0).unwrap();
    canvas.play(vec![cut]);
    let ball = canvas
        .sphere(1.1, 16, 8, gaanim_scene::Material3D::default())
        .unwrap()
        .move_to_3d(-0.2, 0.1, 1.2);
    canvas.wait(0.5);
    let follow = canvas.camera_follow_endpoint(
        crate::canvas::CanvasEndpoint::Entity(ball.id),
        DVec3::ZERO,
        FollowOffsetSpace::World,
        0.3,
        3.2,
    );
    canvas.play(vec![
        follow,
        ball.animate().move_to_3d(3.0, 1.5, -2.0).duration(3.2),
    ]);
    canvas.wait(0.5);

    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    timeline.seek(&mut world, 4.0);
    let camera = *world.resource::<gaanim_math::Camera>();
    let center = camera.world_to_screen(DVec3::new(3.0, 1.5, -2.0))
        - DVec2::new(
            f64::from(camera.viewport_width) / 2.0,
            f64::from(camera.viewport_height) / 2.0,
        );
    // The lag leaves the camera a hundredth of a pixel short.
    assert!(center.length() < 0.05, "{center:?} {camera:?}");
}

#[test]
fn named_camera_state_restores_complete_authored_pose() {
    let mut canvas = SceneModel::new(960, 540);
    let _marker = canvas.circle(1.0);
    let pan_to_detail = canvas.camera_pan_to(120.0, -30.0, 1.0);
    canvas.play(vec![pan_to_detail]);
    let saved = canvas.camera_save("detail").unwrap();
    let pan_home = canvas.camera_pan_to(0.0, 0.0, 1.0);
    canvas.play(vec![pan_home]);
    let restored = canvas.camera_restore("detail", 1.0).unwrap();
    canvas.play(vec![restored.clone()]);
    let _ = saved;
    assert!(matches!(
        restored.inner.anim_type,
        AnimationType::CameraState { .. }
    ));

    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    timeline.seek(&mut world, 3.0);
    let camera = world.resource::<gaanim_math::Camera>();
    assert!((camera.position - DVec3::new(120.0, -30.0, 0.0)).length() < 1e-9);

    timeline.seek(&mut world, 0.25);
    timeline.seek(&mut world, 3.0);
    let camera = world.resource::<gaanim_math::Camera>();
    assert!((camera.position - DVec3::new(120.0, -30.0, 0.0)).length() < 1e-9);
}

#[test]
fn camera_state_handles_validate_names_ownership_and_projection() {
    let first = SceneModel::new(960, 540);
    let second = SceneModel::new(960, 540);
    let state = first
        .camera_state_3d(
            DVec3::new(7.0, 5.0, 6.0),
            DVec3::ZERO,
            DVec3::Y,
            0.8,
            0.1,
            1000.0,
        )
        .unwrap();
    assert_eq!(
        second.camera_to(&state, 1.0).unwrap_err(),
        crate::canvas::CameraStateError::ForeignScene
    );
    assert_eq!(
        first.camera_save("").unwrap_err(),
        crate::canvas::CameraStateError::EmptyName
    );
    assert_eq!(
        first.camera_restore("missing", 1.0).unwrap_err(),
        crate::canvas::CameraStateError::UnknownName("missing".into())
    );
    assert!(first.camera_state_2d(DVec2::ZERO, 0.0, 0.0).is_err());
}

#[test]
fn camera_capture_freezes_reactive_value_at_its_cursor() {
    let mut canvas = SceneModel::new(960, 540);
    let parameter = canvas.parameter(0.0).unwrap();
    let parameter_id = parameter.drawable().id;
    let point = canvas.point_ref(
        ScalarSource::function(ReactiveFunction::new(
            0,
            1,
            vec![gaanim_animation::ReactiveInput::Signal(parameter_id)],
            |values| Ok(vec![values[0] * 260.0 - 130.0]),
        ))
        .unwrap(),
        ScalarSource::constant(25.0),
    );
    let parameter_anim = parameter.animate().set(1.0).duration(1.0);
    let camera_anim = canvas.camera_pan_to_endpoint(point.0, 1.0);
    canvas.play(vec![parameter_anim, camera_anim]);
    let captured = canvas.camera_capture();
    let pan_home = canvas.camera_pan_to(0.0, 0.0, 1.0);
    canvas.play(vec![pan_home]);
    let restore_capture = canvas.camera_to(&captured, 1.0).unwrap();
    canvas.play(vec![restore_capture]);

    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    timeline.seek(&mut world, 3.0);
    let camera = world.resource::<gaanim_math::Camera>();
    assert!((camera.position - DVec3::new(130.0, 25.0, 0.0)).length() < 1e-9);
}

#[test]
fn camera_state_transitions_between_2d_and_3d() {
    let mut canvas = SceneModel::new(960, 540);
    let _marker = canvas.circle(1.0);
    let perspective = canvas
        .camera_state_3d(
            DVec3::new(7.0, 5.0, 6.0),
            DVec3::ZERO,
            DVec3::Y,
            0.8,
            0.1,
            1000.0,
        )
        .unwrap();
    let to_perspective = canvas.camera_to(&perspective, 1.0).unwrap();
    canvas.play(vec![to_perspective]);
    let orthographic = canvas
        .camera_state_2d(DVec2::new(40.0, -20.0), 1.5, 0.2)
        .unwrap();
    let to_orthographic = canvas.camera_to(&orthographic, 0.0).unwrap();
    canvas.play(vec![to_orthographic]);

    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    timeline.seek(&mut world, 0.0);
    timeline.seek(&mut world, 0.5);
    assert!(matches!(
        world.resource::<gaanim_math::Camera>().projection,
        gaanim_math::Projection::Perspective { .. }
    ));
    timeline.seek(&mut world, 1.0);
    let camera = world.resource::<gaanim_math::Camera>();
    assert_eq!(camera.position, DVec3::new(40.0, -20.0, 0.0));
    assert!(matches!(
        camera.projection,
        gaanim_math::Projection::Orthographic { zoom } if (zoom - 1.5).abs() < 1e-9
    ));
}

#[test]
fn animated_look_at_starts_from_a_valid_perspective_pose() {
    let mut canvas = SceneModel::new(1280, 720);
    let _marker = canvas.circle(1.0);
    let perspective = canvas.camera_perspective(0.8, 0.1, 1000.0, 0.0);
    canvas.play(vec![perspective]);
    let look_at = canvas.camera_look_at((8.0, 6.0, 8.0), (0.0, 0.0, 0.0), None, 1.0);
    canvas.play(vec![look_at]);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);

    timeline.seek(&mut world, 0.0);
    let start = *world.resource::<gaanim_math::Camera>();
    assert!(start.validate().is_ok());
    assert!(matches!(
        start.projection,
        gaanim_math::Projection::Perspective { .. }
    ));
    assert_eq!(start.position, DVec3::new(8.0, 6.0, 8.0));
    assert!((start.position - start.target).length() > 1.0);
    timeline.seek(&mut world, 0.5);
    let middle = *world.resource::<gaanim_math::Camera>();
    assert!(middle.validate().is_ok());
    assert!((middle.position - middle.target).length() > 1.0);
}

/// A look-at with another `up` rolls the view over the move: its first
/// frame is the pose before it, and it ends on the new `up`.
#[test]
fn animated_look_at_turns_to_a_new_up_over_the_move() {
    let mut canvas = SceneModel::new(1280, 720);
    let _marker = canvas.circle(1.0);
    let perspective = canvas.camera_perspective(0.6, 0.1, 1000.0, 0.0);
    canvas.play(vec![perspective]);
    let start = canvas.camera_look_at_endpoints(
        Some(CanvasEndpoint::Static(DVec3::new(-5.0, 4.0, 8.0))),
        CanvasEndpoint::Static(DVec3::new(0.0, 0.5, 0.0)),
        DVec3::Y,
        0.0,
    );
    canvas.play(vec![start]);
    canvas.wait(1.0);
    let overhead = canvas.camera_look_at_endpoints(
        Some(CanvasEndpoint::Static(DVec3::new(0.0, 20.0, 0.01))),
        CanvasEndpoint::Static(DVec3::ZERO),
        DVec3::NEG_X,
        3.0,
    );
    canvas.play(vec![overhead]);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);

    let rotation_at = |world: &mut World, timeline: &mut Timeline, time: f64| {
        timeline.seek(world, time);
        world.resource::<gaanim_math::Camera>().rotation
    };
    let before = rotation_at(&mut world, &mut timeline, 0.9);
    let first = rotation_at(&mut world, &mut timeline, 1.0);
    assert!(
        before.angle_between(first) < 1e-9,
        "{before:?} -> {first:?}"
    );
    let early = rotation_at(&mut world, &mut timeline, 1.05);
    assert!(
        before.angle_between(early) < 0.05,
        "{before:?} -> {early:?}"
    );
    timeline.seek(&mut world, 4.5);
    let end = *world.resource::<gaanim_math::Camera>();
    assert_eq!(end.up, DVec3::NEG_X);
    let screen_up = end.rotation * DVec3::Y;
    assert!((screen_up - DVec3::NEG_X).length() < 1e-3, "{screen_up:?}");
}

/// Turning a perspective camera orthographic flattens it over the
/// animation, with the target centered, and ends on the same pose.
#[test]
fn orthographic_animations_flatten_a_perspective_camera() {
    let eye = DVec3::new(7.0, 5.0, 10.0);
    let target = DVec3::new(0.0, 0.5, 0.0);
    let mut canvas = SceneModel::new(1280, 720);
    let _marker = canvas.circle(1.0);
    let perspective = canvas.camera_perspective(0.7, 0.1, 1000.0, 0.0);
    canvas.play(vec![perspective]);
    let look_at = canvas.camera_look_at_endpoints(
        Some(CanvasEndpoint::Static(eye)),
        CanvasEndpoint::Static(target),
        DVec3::Y,
        0.0,
    );
    canvas.play(vec![look_at]);
    let orthographic = canvas.camera_orthographic(1.5, 1.0);
    canvas.play(vec![orthographic]);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);

    timeline.seek(&mut world, 0.5);
    let middle = *world.resource::<gaanim_math::Camera>();
    assert!(middle.validate().is_ok());
    let gaanim_math::Projection::Perspective { fov_y, .. } = middle.projection else {
        panic!("orthographic only at the end: {:?}", middle.projection);
    };
    assert!(fov_y < 0.7);
    assert!((middle.target - target).length() < 1e-9);
    assert!((middle.position - target).length() > (eye - target).length());
    timeline.seek(&mut world, 1.0);
    let end = *world.resource::<gaanim_math::Camera>();
    assert!((end.position - eye).length() < 1e-9);
    assert!(matches!(
        end.projection,
        gaanim_math::Projection::Orthographic { zoom } if (zoom - 1.5).abs() < 1e-9
    ));
}

#[test]
fn camera_orbit_seek_preserves_authored_radius() {
    let eye = DVec3::new(10.0, 7.0, 13.0);
    let target = DVec3::new(0.0, -0.5, 0.0);
    let radius = (eye - target).length();
    let mut canvas = SceneModel::new(1280, 720);
    let _marker = canvas.circle(1.0);
    let perspective = canvas.camera_perspective(0.8, 0.1, 1000.0, 0.0);
    canvas.play(vec![perspective]);
    let look_at = canvas.camera_look_at_endpoints(
        Some(CanvasEndpoint::Static(eye)),
        CanvasEndpoint::Static(target),
        DVec3::Y,
        0.5,
    );
    canvas.play(vec![look_at]);
    let orbit = canvas.camera_orbit(0.8, 0.2, 1.4);
    canvas.play(vec![orbit]);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);

    for time in [0.5, 1.2, 1.9] {
        timeline.seek(&mut world, time);
        let camera = *world.resource::<gaanim_math::Camera>();
        assert!(camera.validate().is_ok(), "invalid camera at {time}");
        assert_eq!(camera.target, target);
        assert!(
            ((camera.position - target).length() - radius).abs() < 1e-9,
            "orbit radius changed at {time}: {:?}",
            camera.position
        );
        let forward = camera.rotation * -DVec3::Z;
        let expected = (target - camera.position).normalize();
        assert!(forward.dot(expected) > 1.0 - 1e-9);
    }
}

/// A group about a chosen origin pivots on it and `move_to` places it,
/// so a rigid body modelled from its tip turns about the tip.
#[test]
fn a_group_about_a_point_turns_about_it_and_is_placed_by_it() {
    let mut canvas = SceneModel::new(1280, 720);
    let stick = canvas.circle(0.1).move_to_3d(0.0, 1.0, 0.0);
    let knob = canvas.circle(0.2).move_to_3d(0.0, 2.0, 0.0);
    canvas
        .group_about(&[&stick, &knob], DVec3::ZERO)
        .rotate_to_3d(0.0, 0.0, 1.0)
        .move_to_3d(-6.0, -1.5, 0.0);
    let (mut world, _) = compile_camera_timeline(canvas);
    let mut groups =
        world.query_filtered::<&gaanim_math::SpatialTransform, With<gaanim_scene::GroupMarker>>();
    let transforms: Vec<_> = groups.iter(&world).copied().collect();
    assert_eq!(transforms.len(), 1);
    let matrix = transforms[0].to_mat4();
    let tip = matrix.transform_point3(DVec3::ZERO);
    assert!(
        (tip - DVec3::new(-6.0, -1.5, 0.0)).length() < 1e-9,
        "{tip:?}"
    );
    let top = matrix.transform_point3(DVec3::new(0.0, 2.0, 0.0));
    let expected = DVec3::new(-6.0 - 2.0 * 1f64.sin(), -1.5 + 2.0 * 1f64.cos(), 0.0);
    assert!((top - expected).length() < 1e-9, "{top:?}");
}

/// A stop shows what was made before it: a camera reset and a cut made
/// after it, as the next segment starts at its instant, come a moment
/// later, and the next frame shows them.
#[test]
fn a_stop_shows_the_frame_before_the_cuts_made_after_it() {
    let mut canvas = SceneModel::new(1280, 720);
    canvas.segment("first", None).unwrap();
    let mark = canvas.circle(0.3).move_to(-6.0, 3.0);
    let pan = canvas.camera_pan_to(2.0, 0.0, 1.0);
    canvas.play(vec![pan]);
    canvas.stop(None).unwrap();
    canvas.segment("second", None).unwrap();
    let reset = canvas.camera_reset(0.0);
    canvas.play(vec![reset]);
    let mark = mark.move_to(6.0, -3.0);
    canvas.play(vec![mark.animate().move_to(5.0, -3.0).duration(1.0)]);
    canvas.stop(None).unwrap();
    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    let stops: Vec<f64> = timeline
        .segments
        .iter()
        .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
        .collect();
    assert_eq!(stops[0], 1.0);
    assert!((stops[1] - 2.0).abs() < 1e-5, "{stops:?}");
    let entity = world
        .query_filtered::<Entity, (
            With<gaanim_scene::Opacity>,
            Without<gaanim_animation::FloatSignal>,
        )>()
        .single(&world)
        .unwrap();
    for (time, camera_x, mark_x) in [
        (1.0, 2.0, -6.0),
        (1.0 + 1.0 / 60.0, 0.0, 6.0),
        (1.0, 2.0, -6.0),
    ] {
        timeline.seek(&mut world, time);
        let camera = world.resource::<gaanim_math::Camera>().position.x;
        let mark = world.get::<gaanim_math::SpatialTransform>(entity).unwrap();
        let mark = mark.to_mat4().transform_point3(DVec3::ZERO).x;
        assert!(
            (camera - camera_x).abs() < 1e-6,
            "camera at {camera} at {time}"
        );
        assert!((mark - mark_x).abs() < 0.01, "mark at {mark} at {time}");
    }
}

/// A camera constraint switched after a stop at its instant switches as
/// a cut does: the stop shows the camera as it was, the next frame the
/// camera with the switch.
#[test]
fn a_stop_shows_the_camera_before_a_constraint_switched_after_it() {
    let mut canvas = SceneModel::new(1280, 720);
    canvas.segment("first", None).unwrap();
    let constraint = canvas
        .camera_bind_2d(
            Some(CanvasEndpoint::Static(DVec3::new(120.0, -35.0, 0.0))),
            None,
            None,
            ScalarSource::constant(1.0),
            true,
        )
        .unwrap();
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    canvas.segment("second", None).unwrap();
    constraint.disable();
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    canvas.segment("third", None).unwrap();
    constraint.enable();
    canvas.wait(1.0);
    let (mut world, timeline) = compile_camera_timeline(canvas);
    let stops: Vec<f64> = timeline
        .segments
        .iter()
        .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
        .collect();
    let mut camera_x = |time: f64| {
        world.insert_resource(gaanim_math::Camera::ortho_2d(1280, 720));
        gaanim_animation::apply_camera_bindings(&mut world, time);
        world.resource::<gaanim_math::Camera>().position.x
    };
    let frame = 1.0 / 60.0;
    assert_eq!(camera_x(stops[0]), 120.0, "{stops:?}");
    assert_eq!(camera_x(stops[0] + frame), 0.0);
    assert_eq!(camera_x(stops[1]), 0.0);
    assert_eq!(camera_x(stops[1] + frame), 120.0);
}

/// A constraint made for its segment holds to the segment's last stop
/// and lets go as the next segment starts; one made for the scene holds
/// on until a segment that starts by releasing the camera, which shows
/// the 2D view from its first frame.
#[test]
fn segment_cameras_let_go_as_the_next_segment_starts() {
    let mut canvas = SceneModel::new(1280, 720);
    let eye = DVec3::new(9.0, 5.0, 8.0);
    let bind = |canvas: &mut SceneModel| {
        canvas
            .camera_bind_3d(
                Some(CanvasEndpoint::Static(eye)),
                Some(CanvasEndpoint::Static(DVec3::ZERO)),
                None,
                CanvasEndpoint::Static(DVec3::Y),
                None,
                ScalarSource::constant(1.0),
                true,
            )
            .unwrap()
    };
    canvas.segment("model", None).unwrap();
    bind(&mut canvas).until_segment_end();
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    canvas.segment("plain", None).unwrap();
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    canvas.segment("again", None).unwrap();
    let _kept = bind(&mut canvas);
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    canvas.segment("flat", None).unwrap();
    canvas.release_camera().unwrap();
    canvas.wait(1.0);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    let stops: Vec<f64> = timeline
        .segments
        .iter()
        .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
        .collect();
    let mut rig_at = |time: f64| {
        timeline.seek(&mut world, time);
        world.insert_resource(std::mem::replace(&mut timeline, Timeline::new()));
        gaanim_timeline::camera_binding_system(&mut world);
        timeline = world.remove_resource::<Timeline>().unwrap();
        world.resource::<gaanim_math::CameraRigCamera>().0
    };
    let bound = |camera: gaanim_math::Camera| (camera.position - eye).length() < 1e-9;
    let frame = 1.0 / 60.0;
    assert!(bound(rig_at(0.5)));
    assert!(bound(rig_at(stops[0])), "{stops:?}");
    assert!(!bound(rig_at(stops[0] + frame)));
    assert!(bound(rig_at(stops[1] + 0.5)));
    assert!(bound(rig_at(stops[2])));
    let flat = rig_at(stops[2] + frame);
    assert!(!bound(flat));
    assert!(matches!(
        flat.projection,
        gaanim_math::Projection::Orthographic { zoom } if (zoom - 1.0).abs() < 1e-9
    ));
    assert!(flat.position.truncate().length() < 1e-9);
}

/// While a transition shows it, the outgoing segment keeps the camera it
/// ended with, its constraint included, though the incoming segment
/// releases the constraint and resets the camera as it starts; the
/// incoming segment has its own camera from the transition's first frame.
#[test]
fn the_outgoing_segment_keeps_its_camera_while_it_leaves() {
    let mut canvas = SceneModel::new(1280, 720);
    canvas.segment("model", None).unwrap();
    let _cube = canvas
        .cube(1.0, gaanim_scene::Material3D::default())
        .unwrap();
    let eye = DVec3::new(9.0, 5.0, 8.0);
    let constraint = canvas
        .camera_bind_3d(
            Some(CanvasEndpoint::Static(eye)),
            Some(CanvasEndpoint::Static(DVec3::new(3.0, 1.0, -2.0))),
            Some(ScalarSource::constant(0.6)),
            CanvasEndpoint::Static(DVec3::Y),
            None,
            ScalarSource::constant(1.0),
            true,
        )
        .unwrap();
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    canvas
        .segment(
            "next",
            Some(gaanim_timeline::transition::TransitionType::cross_fade(0.6)),
        )
        .unwrap();
    constraint.disable();
    let reset = canvas.camera_reset(0.0);
    canvas.play(vec![reset]);
    canvas.wait(1.0);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    world.init_resource::<gaanim_math::ResolvedCamera>();
    world.init_resource::<gaanim_math::CameraViewOverride>();
    world.init_resource::<gaanim_math::CameraViewport>();
    let mut camera_phase = bevy::prelude::Schedule::default();
    camera_phase.add_systems(gaanim_scene::systems::resolve_camera_system);
    let mut at = |time: f64| {
        timeline.seek(&mut world, time);
        world.insert_resource(std::mem::replace(&mut timeline, Timeline::new()));
        gaanim_timeline::camera_binding_system(&mut world);
        gaanim_timeline::camera_rig_system(&mut world);
        camera_phase.run(&mut world);
        timeline = world.remove_resource::<Timeline>().unwrap();
        let shown = world.resource::<gaanim_math::ResolvedCamera>().camera;
        let outgoing = world
            .get_resource::<gaanim_scene::SceneTransitionFrame>()
            .and_then(|frame| frame.outgoing_camera);
        (shown, outgoing)
    };
    let is_bound = |camera: &gaanim_math::Camera| (camera.position - eye).length() < 1e-9;
    // Before the transition, one camera: the constraint's.
    let (shown, outgoing) = at(0.95);
    assert!(is_bound(&shown) && outgoing.is_none());
    // While it fades, the model keeps that camera and the next segment
    // shows the reset one.
    for time in [1.02, 1.3, 1.55] {
        let (shown, outgoing) = at(time);
        assert!(!is_bound(&shown), "at {time}: {shown:?}");
        let outgoing = outgoing.unwrap_or_else(|| panic!("no outgoing camera at {time}"));
        assert!(is_bound(&outgoing), "at {time}: {outgoing:?}");
    }
    // Once it ends, one camera again.
    let (shown, outgoing) = at(1.8);
    assert!(!is_bound(&shown) && outgoing.is_none());
}

/// What only says which segment holds a drawable, as `reuse` made just
/// after a stop, stays at the stop's instant, with what follows it.
#[test]
fn a_reuse_after_a_stop_keeps_its_instant() {
    let mut canvas = SceneModel::new(1280, 720);
    canvas.segment("first", None).unwrap();
    let mark = canvas.circle(0.3);
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    canvas.segment("second", None).unwrap();
    canvas.reuse(&mark).unwrap();
    canvas.play(vec![mark.animate().move_to(1.0, 0.0).duration(1.0)]);
    let (_, timeline) = compile_camera_timeline(canvas);
    let starts: Vec<f64> = timeline
        .clips
        .values()
        .filter(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(animation)
                    if matches!(
                        animation.lens,
                        gaanim_timeline::clip::PropertyLensSpec::Translation { .. }
                    )
            )
        })
        .map(|clip| clip.start)
        .collect();
    assert_eq!(starts, [1.0]);
}

/// An object that enters with `create` or `grow_from_center` shows from
/// its entry on, though a fade-out and a fade-in come later; one whose
/// entry is its first fade-in stays transparent until then.
#[test]
fn a_later_fade_in_does_not_hide_an_entry_before_it() {
    let mut canvas = SceneModel::new(1280, 720);
    let ring = canvas.circle(1.0);
    let square = canvas.rect(2.0, 2.0);
    let faded = canvas.circle(0.5);
    canvas.play(vec![
        ring.animate().create().duration(0.9),
        square.animate().grow_from_center().duration(0.9),
    ]);
    canvas.wait(1.0);
    canvas.play(vec![
        ring.animate().fade_out().duration(0.4),
        square.animate().fade_out().duration(0.4),
        faded.animate().fade_in().duration(0.4),
    ]);
    canvas.play(vec![
        ring.animate().fade_in().duration(0.4),
        square.animate().fade_in().duration(0.4),
    ]);
    canvas.wait(0.5);
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    // The ring, the square and the faded circle, in the order spawned.
    let mut entities: Vec<_> = world
        .query_filtered::<(Entity, &MobjectId), (
            With<gaanim_scene::Opacity>,
            Without<gaanim_animation::FloatSignal>,
        )>()
        .iter(&world)
        .map(|(entity, id)| (id.0, entity))
        .collect();
    entities.sort();
    let entities: Vec<_> = entities.into_iter().map(|(_, entity)| entity).collect();
    assert_eq!(entities.len(), 3);
    for (time, expected) in [
        (0.6, [1.0, 1.0, 0.0]),
        (1.5, [1.0, 1.0, 0.0]),
        (3.0, [1.0, 1.0, 1.0]),
        (0.6, [1.0, 1.0, 0.0]),
    ] {
        timeline.seek(&mut world, time);
        for (entity, expected) in entities.iter().zip(expected) {
            let opacity = world.get::<gaanim_scene::Opacity>(*entity).unwrap().0;
            assert!(
                (opacity - expected).abs() < 1e-6,
                "{opacity} at {time}, expected {expected}"
            );
        }
    }
}

/// `move_to` places the origin of a group about one, declared and
/// animated, as `move_to_3d` does.
#[test]
fn move_to_places_the_origin_of_a_group_about_one() {
    let mut canvas = SceneModel::new(1280, 720);
    let ball = canvas.circle(0.5).move_to(2.0, 1.0);
    let origin = DVec3::new(2.0, 0.5, 0.0);
    let group = canvas
        .group_about(&[&ball], origin)
        .move_to_default(-1.0, 1.0);
    canvas.wait(0.5);
    canvas.play(vec![group.animate().move_to(3.0, -2.0).duration(0.5)]);
    canvas.wait(0.5);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);
    for (time, expected) in [
        (0.25, DVec3::new(-1.0, 1.0, 0.0)),
        (1.25, DVec3::new(3.0, -2.0, 0.0)),
    ] {
        timeline.seek(&mut world, time);
        let mut groups = world
            .query_filtered::<&gaanim_math::SpatialTransform, With<gaanim_scene::GroupMarker>>();
        let placed = groups
            .single(&world)
            .unwrap()
            .to_mat4()
            .transform_point3(origin);
        assert!((placed - expected).length() < 1e-6, "at {time}: {placed:?}");
    }
}

/// `animate().move_to_3d`, a cut after declaration and a reactive
/// position put a group's origin, or the center of its box, where a
/// declared `move_to_3d` does.
#[test]
fn a_group_moves_by_its_pivot_however_it_is_moved() {
    for origin in [Some(DVec3::new(2.0, 0.5, 0.0)), None] {
        let mut canvas = SceneModel::new(1280, 720);
        let p = canvas.parameter(3.0).unwrap();
        let ball = canvas.circle(0.5).move_to_3d(2.0, 1.0, 0.0);
        let group = match origin {
            Some(origin) => canvas.group_about(&[&ball], origin),
            None => canvas.group(&[&ball]),
        }
        .move_to_3d(0.0, 0.0, 0.0);
        canvas.wait(0.5);
        // It is there already: it stays.
        canvas.play(vec![
            group.animate().move_to_3d(0.0, 0.0, 0.0).duration(0.5),
        ]);
        canvas.play(vec![
            group.animate().move_to_3d(1.0, -1.0, 0.5).duration(0.5),
        ]);
        canvas.wait(0.5);
        let group = group.move_to_3d(-2.0, 0.0, 0.0);
        canvas.wait(0.5);
        group.move_to_3d(&p, 0.0, 1.0);
        canvas.wait(0.5);
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        let pivot = origin.unwrap_or(DVec3::new(2.0, 1.0, 0.0));
        for (time, expected) in [
            (0.25, DVec3::ZERO),
            (0.75, DVec3::ZERO),
            (2.25, DVec3::new(-2.0, 0.0, 0.0)),
            (1.75, DVec3::new(1.0, -1.0, 0.5)),
            (2.75, DVec3::new(3.0, 0.0, 1.0)),
            (0.75, DVec3::ZERO),
        ] {
            timeline.seek(&mut world, time);
            let mut groups = world
                .query_filtered::<&gaanim_math::SpatialTransform, With<gaanim_scene::GroupMarker>>(
                );
            let transform = *groups.single(&world).unwrap();
            let placed = transform.to_mat4().transform_point3(pivot);
            assert!(
                (placed - expected).length() < 1e-6,
                "origin {origin:?} at {time}: {placed:?}, expected {expected:?}"
            );
        }
    }
}

/// `screen_offset` shifts the lens over its animation, an orbit keeps
/// the shift, so the target stays where it shows, and a reset brings it
/// back to the center.
#[test]
fn a_screen_offset_holds_through_an_orbit_until_a_reset() {
    let target = DVec3::new(0.0, 0.5, 0.0);
    let mut canvas = SceneModel::new(1280, 720);
    let _marker = canvas.circle(1.0);
    let perspective = canvas.camera_perspective(0.8, 0.1, 1000.0, 0.0);
    canvas.play(vec![perspective]);
    let look_at = canvas.camera_look_at_endpoints(
        Some(CanvasEndpoint::Static(DVec3::new(0.0, 2.0, 12.0))),
        CanvasEndpoint::Static(target),
        DVec3::Y,
        0.0,
    );
    canvas.play(vec![look_at]);
    let shift = DVec2::new(-300.0, 40.0);
    let offset = canvas.camera_screen_offset(shift, 1.0).unwrap();
    canvas.play(vec![offset.rate_func(gaanim_math::RateFunc::Linear)]);
    let orbit = canvas.camera_orbit(0.8, 0.2, 1.0);
    canvas.play(vec![orbit]);
    let reset = canvas.camera_reset(1.0);
    canvas.play(vec![reset.rate_func(gaanim_math::RateFunc::Linear)]);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);

    for (time, expected) in [
        (0.5, shift * 0.5),
        (1.5, shift),
        (1.0, shift),
        (2.5, shift * 0.5),
        (3.0, DVec2::ZERO),
    ] {
        timeline.seek(&mut world, time);
        let camera = *world.resource::<gaanim_math::Camera>();
        assert!(
            (camera.lens_shift - expected).length() < 1e-9,
            "shift {:?} at {time}, expected {expected:?}",
            camera.lens_shift
        );
        if (1.0..=2.0).contains(&time) {
            assert!((camera.hud_point(target) - shift).length() < 1e-6);
        }
    }
}

#[test]
fn camera_orbit_dollies_along_the_orbit() {
    let eye = DVec3::new(0.0, 2.0, 12.0);
    let target = DVec3::new(0.0, 0.5, 0.0);
    let radius = (eye - target).length();
    let mut canvas = SceneModel::new(1280, 720);
    let _marker = canvas.circle(1.0);
    let perspective = canvas.camera_perspective(0.8, 0.1, 1000.0, 0.0);
    canvas.play(vec![perspective]);
    let look_at = canvas.camera_look_at_endpoints(
        Some(CanvasEndpoint::Static(eye)),
        CanvasEndpoint::Static(target),
        DVec3::Y,
        0.0,
    );
    canvas.play(vec![look_at]);
    let orbit = canvas
        .camera_orbit_dolly(0.6, 0.5, 0.25, 2.0)
        .rate_func(gaanim_math::RateFunc::Linear);
    canvas.play(vec![orbit]);
    // The next move starts from where the orbit left the eye.
    let after = canvas.camera_orbit(0.1, 0.0, 1.0);
    canvas.play(vec![after]);
    let (mut world, mut timeline) = compile_camera_timeline(canvas);

    // The distance shrinks geometrically: half way, by the square root of 0.25.
    for (time, factor) in [(1.0, 0.5), (2.0, 0.25), (3.0, 0.25)] {
        timeline.seek(&mut world, time);
        let camera = *world.resource::<gaanim_math::Camera>();
        assert!(camera.validate().is_ok(), "invalid camera at {time}");
        assert_eq!(camera.target, target);
        let distance = (camera.position - target).length();
        assert!(
            (distance - radius * factor).abs() < 1e-9,
            "distance {distance} at {time}, expected {}",
            radius * factor
        );
        let forward = camera.rotation * -DVec3::Z;
        assert!(forward.dot((target - camera.position).normalize()) > 1.0 - 1e-9);
    }
}

#[test]
fn reactive_readout_resolves_body_family_like_scene_text() {
    use gaanim_core::kurbo::Shape as _;

    let mut canvas = SceneModel::new(640, 360);
    canvas.reactive_readout(
        gaanim_animation::ScalarSource::Constant(54.0),
        ".0f",
        "",
        "%",
        "invalid",
        Some(0.5),
    );
    let mut world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let mut config = gaanim_text::prelude::TextConfig::default();
    // Typst embeds Libertinus; the legacy registry does not know it by name.
    config
        .roles
        .get_mut(&gaanim_text::prelude::TextRole::Body)
        .unwrap()
        .font_family = "Libertinus Serif".to_owned();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
    queue.apply(&mut world);

    let (path, baseline) = world
        .query::<(
            &gaanim_animation::ReactiveReadout,
            &gaanim_scene::PathSource,
            &gaanim_scene::TextBaseline,
        )>()
        .iter(&world)
        .map(|(_, path, baseline)| (path.0.clone(), baseline.0))
        .next()
        .expect("readout spawned");
    let expected = gaanim_text::typst_compiler::shape_typst_text_run(
        &fonts,
        "54%",
        "Libertinus Serif",
        None,
        0.5,
    )
    .unwrap()
    .path;
    let ink = expected.bounding_box();
    let (expected, _) = gaanim_animation::right_align_readout_path(expected, Bounds3D::default());
    let (actual, expected) = (path.bounding_box(), expected.bounding_box());
    for delta in [
        actual.x0 - expected.x0,
        actual.y0 - expected.y0,
        actual.x1 - expected.x1,
        actual.y1 - expected.y1,
        baseline + (ink.y0 + ink.y1) * 0.5,
    ] {
        assert!(
            delta.abs() < 1e-9,
            "{actual:?} vs {expected:?}, baseline {baseline}"
        );
    }
}

/// A member the author hid is not shown by the entry of the group that
/// holds it, as the marks of a coordinate space are not by its create:
/// its own animation shows it.
#[test]
fn author_hidden_members_wait_for_their_own_entry() {
    let mut canvas = SceneModel::new(640, 360);
    let shown = canvas.circle(10.0);
    let kept = canvas.circle(10.0).hidden().unwrap();
    let group = canvas.group(&[&shown, &kept]);
    canvas.play(vec![group.animate().create().duration(1.0)]);
    canvas.play(vec![kept.fade_in(1.0)]);

    let mut world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

    let opacity = |world: &mut World, handle: &DrawableHandle| {
        let id = ObjectId::from_raw(handle.id.as_raw() - 1);
        world
            .query::<(&MobjectId, &Opacity)>()
            .iter(world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .0
    };
    for (time, expected) in [(0.5, 0.0), (1.0, 0.0), (2.0, 1.0), (0.5, 0.0)] {
        timeline.seek(&mut world, time);
        assert!((opacity(&mut world, &shown) - 1.0).abs() < 1e-5 || time < 1.0);
        let kept_opacity = opacity(&mut world, &kept);
        assert!(
            (kept_opacity - expected).abs() < 1e-5,
            "kept {kept_opacity} at {time}"
        );
    }
}

#[test]
fn late_declared_group_members_stay_hidden_until_fade_in() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.wait(0.8);
    let outline = canvas.circle(20.0);
    let number = canvas.reactive_readout(
        gaanim_animation::ScalarSource::Constant(54.0),
        ".0f",
        "",
        "%",
        "invalid",
        Some(24.0),
    );
    let readout = canvas.reactive_readout_group(None, None, &number, None, 10.0);
    let _group = canvas.group(&[&outline, &readout]);
    let caption = canvas.circle(10.0);
    canvas.play(vec![outline.fade_in(1.0)]);
    canvas.play(vec![readout.fade_in(1.0), caption.fade_in(1.0)]);

    let mut world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

    for time in [0.0, 1.3, 2.3, 3.0, 1.3, 0.0] {
        timeline.seek(&mut world, time);
        for handle in [&readout, &caption] {
            let id = ObjectId::from_raw(handle.id.as_raw() - 1);
            let opacity = world
                .query::<(&MobjectId, &Opacity)>()
                .iter(&world)
                .find(|(object, _)| object.0 == id)
                .unwrap()
                .1
                .0;
            let expected = if time < 1.8 {
                0.0
            } else if time > 2.8 {
                1.0
            } else {
                0.5
            };
            assert!(
                (opacity - expected).abs() < 1e-5,
                "opacity {opacity} at {time}"
            );
        }
    }
}

#[test]
fn object_declared_after_wait_stays_hidden_until_its_declaration_time() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.wait(1.0);
    canvas.circle(20.0);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

    timeline.seek(&mut world, 0.5);
    assert!(
        world
            .query::<&Opacity>()
            .iter(&world)
            .all(|opacity| opacity.0 == 0.0),
        "late-declared object leaked before its declaration time"
    );

    timeline.seek(&mut world, 1.0);
    assert!(
        world
            .query::<&Opacity>()
            .iter(&world)
            .any(|opacity| opacity.0 == 1.0),
        "late-declared object did not become visible at its declaration time"
    );
}

#[test]
fn late_group_keeps_already_visible_members_visible() {
    let mut canvas = SceneModel::new(640, 360);
    let first = canvas.circle(10.0);
    let second = canvas.circle(10.0);
    canvas.wait(1.0);
    let group = canvas.group(&[&first, &second]);
    canvas.play(vec![group.animate().opacity(0.3).duration(0.5)]);

    let mut world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

    let opacity_of = |world: &mut World, handle: &DrawableHandle| {
        let id = ObjectId::from_raw(handle.id.as_raw() - 1);
        world
            .query::<(&MobjectId, &Opacity)>()
            .iter(world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .0
    };
    for time in [0.5, 2.0, 0.0] {
        timeline.seek(&mut world, time);
        for member in [&first, &second] {
            assert_eq!(opacity_of(&mut world, member), 1.0, "member at {time}");
        }
        let expected = if time < 1.0 { 1.0 } else { 0.3 };
        let group_opacity = opacity_of(&mut world, &group);
        assert!(
            (group_opacity - expected).abs() < 1e-5,
            "group opacity {group_opacity} at {time}"
        );
    }
}

#[test]
fn visual_effects_are_attached_to_compiled_drawables() {
    let mut canvas = SceneModel::new(640, 360);
    canvas
        .circle(60.0)
        .glow(PenikoColor::WHITE, 18.0, 1.2)
        .blur(5.0)
        .shadow(
            PenikoColor::BLACK,
            gaanim_core::glam::DVec2::new(8.0, -8.0),
            7.0,
        );

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    assert_eq!(
        world
            .query::<&gaanim_renderer::effects::Glow>()
            .iter(&world)
            .count(),
        1
    );
    assert_eq!(
        world
            .query::<&gaanim_renderer::effects::GaussianBlur>()
            .iter(&world)
            .count(),
        1
    );
    assert_eq!(
        world
            .query::<&gaanim_renderer::effects::DropShadow>()
            .iter(&world)
            .count(),
        1
    );
}

#[test]
fn tips_stay_hidden_with_their_path_until_its_entry() {
    use bevy::prelude::App;
    use gaanim_animation::{StrokeTips, TipKind};
    // A trim before a later grow keeps the path hidden until the grow.
    let mut canvas = SceneModel::new(640, 360);
    let route = canvas
        .polyline(&[(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)])
        .tip(Some(TipKind::Dot), Some(TipKind::Arrow), None, None)
        .unwrap();
    canvas.play(vec![route.animate().trim(Some(0.2), None, None)]);
    canvas.wait(0.5);
    canvas.play(vec![route.animate().grow_arrow()]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let entity = entity_of(app.world_mut(), &route);
    for (time, shown) in [(0.5, false), (1.2, false), (2.0, true), (0.3, false)] {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        let world = app.world();
        let tips = world.get::<StrokeTips>(entity).unwrap();
        for tip in [tips.start_entity, tips.end_entity] {
            let shape = world.get::<gaanim_scene::Path2D>(tip.unwrap()).unwrap();
            assert_eq!(!shape.0.elements().is_empty(), shown, "tip at {time}");
        }
    }
}

#[test]
fn the_rehearsal_plays_the_audience_and_its_answers_along_the_timeline() {
    use bevy::prelude::App;
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_poll_session(crate::canvas::PollSession {
        relay: Some("https://relay.example.dev".into()),
        code: "ABC234".into(),
    });
    canvas
        .rehearsal(gaanim_animation::rehearsal::RehearsalSpec {
            names: gaanim_animation::rehearsal::RehearsalSpec::names(10),
            ..Default::default()
        })
        .unwrap();
    canvas.segment("Sala", None).unwrap();
    let audience = canvas.audience().unwrap();
    let count = audience.count().unwrap();
    canvas.wait(10.0);
    canvas.stop(None).unwrap();
    canvas.segment("Pregunta", None).unwrap();
    let quiz = canvas
        .quiz(
            "¿2 + 2?",
            ["3", "4"],
            vec![1],
            20,
            1000,
            Lean::Right(1.0),
            crate::canvas::PollStyle::default(),
        )
        .unwrap();
    let right = quiz.votes(1).unwrap();
    canvas.wait(4.0);
    canvas.stop(None).unwrap();
    canvas.wait(1.0);
    let board = canvas.leaderboard();
    let leader = board.points(0).unwrap();
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let mut value_at = |parameter: &crate::canvas::visualization::Parameter, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        let entity = entity_of(app.world_mut(), parameter.drawable());
        app.world()
            .get::<gaanim_animation::FloatSignal>(entity)
            .unwrap()
            .value
    };
    // Players join over the room's ten seconds.
    assert_eq!(value_at(&count, 0.0), 0.0);
    let half = value_at(&count, 5.0);
    assert!(half > 2.0 && half < 8.0, "{half}");
    assert_eq!(value_at(&count, 10.0), 10.0);
    // Everyone answers right by the stop, and scores.
    assert_eq!(value_at(&right, 10.0), 0.0);
    assert_eq!(value_at(&right, 14.0), 10.0);
    assert!(value_at(&leader, 15.0) > 500.0);
    // Back in the room, fewer players again.
    assert!(value_at(&count, 3.0) < 5.0);
}

/// `quiz.revealed()` is 0 until the playhead passes the reveal, even
/// while it rests on the stop the reveal shares, then 1 (#297).
#[test]
fn revealed_turns_on_past_the_reveal() {
    use bevy::prelude::App;
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_poll_session(crate::canvas::PollSession {
        relay: Some("https://relay.example.dev".into()),
        code: "ABC234".into(),
    });
    let quiz = canvas
        .quiz(
            "¿2 + 2?",
            ["3", "4"],
            vec![1],
            20,
            1000,
            Lean::Auto,
            crate::canvas::PollStyle::default(),
        )
        .unwrap();
    let revealed = quiz.revealed().unwrap();
    canvas.wait(2.0);
    canvas.stop(None).unwrap();
    quiz.reveal().unwrap();
    assert!(quiz.revealed().is_err(), "asked after the reveal");
    canvas.wait(1.0);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    // As a host does after a replay: the t=0 keyframe holds the scene, so
    // seeking back restores the signal the reveal has not driven yet.
    app.world_mut().resource_mut::<Timeline>().keyframes.clear();
    gaanim_timeline::capture_reload_keyframe(app.world_mut());
    let mut value_at = |time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        let entity = entity_of(app.world_mut(), revealed.drawable());
        app.world()
            .get::<gaanim_animation::FloatSignal>(entity)
            .unwrap()
            .value
    };
    assert_eq!(value_at(0.0), 0.0);
    assert_eq!(value_at(1.0), 0.0);
    assert_eq!(value_at(2.0), 0.0, "resting on the stop");
    assert_eq!(value_at(2.01), 1.0);
    assert_eq!(value_at(3.0), 1.0);
    assert_eq!(value_at(1.0), 0.0, "seeking back");
}

#[test]
fn a_rehearsal_deals_its_players_to_teams_that_score_their_points() {
    use bevy::prelude::App;
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_poll_session(crate::canvas::PollSession {
        relay: Some("https://relay.example.dev".into()),
        code: "ABC234".into(),
    });
    canvas
        .rehearsal(gaanim_animation::rehearsal::RehearsalSpec {
            names: gaanim_animation::rehearsal::RehearsalSpec::names(8),
            team_skill: vec![0.95, 0.05],
            ..Default::default()
        })
        .unwrap();
    canvas.segment("Pregunta", None).unwrap();
    let teams = canvas
        .teams(vec!["Rojo".into(), "Azul".into()], None, false)
        .unwrap();
    let quiz = canvas
        .quiz(
            "¿2 + 2?",
            ["3", "4", "5"],
            vec![1],
            20,
            1000,
            Lean::Auto,
            crate::canvas::PollStyle::default(),
        )
        .unwrap();
    let (red, blue, players, leader) = (
        teams.score(0).unwrap(),
        teams.score(1).unwrap(),
        teams.players(1).unwrap(),
        teams.leader().unwrap(),
    );
    canvas.wait(4.0);
    canvas.stop(None).unwrap();
    quiz.reveal().unwrap();
    canvas.wait(1.0);
    let timeline = compiled_timeline(&canvas);
    let session = timeline.poll_session.as_ref().unwrap();
    assert_eq!(session.teams.as_ref().unwrap().names, ["Rojo", "Azul"]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let mut value_at = |parameter: &crate::canvas::visualization::Parameter, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        let entity = entity_of(app.world_mut(), parameter.drawable());
        app.world()
            .get::<gaanim_animation::FloatSignal>(entity)
            .unwrap()
            .value
    };
    assert_eq!(value_at(&players, 4.5), 4.0);
    assert_eq!(value_at(&red, 0.0), 0.0);
    // The skilled team answers right and leads.
    assert!(value_at(&red, 4.5) > value_at(&blue, 4.5));
    assert_eq!(value_at(&leader, 4.5), 0.0);
}

#[test]
fn tipped_paths_spawn_their_tips_as_children() {
    use gaanim_animation::{StrokeTip, StrokeTips, TipKind};
    let mut canvas = SceneModel::new(640, 360);
    let route = canvas
        .polyline(&[(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)])
        .tip(None, Some(TipKind::Arrow), Some(0.3), None)
        .unwrap();
    let plain = canvas.circle(1.0);
    assert!(
        plain
            .clone()
            .tip(None, Some(TipKind::Dot), Some(-1.0), None)
            .is_err()
    );

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);

    let entity = entity_of(&mut world, &route);
    let tips = world.get::<StrokeTips>(entity).unwrap();
    assert_eq!((tips.start, tips.end), (None, Some(TipKind::Arrow)));
    assert!(tips.start_entity.is_none());
    let tip = tips.end_entity.unwrap();
    assert!(world.get::<StrokeTip>(tip).is_some());
    assert_eq!(world.get::<ChildOf>(tip).unwrap().parent(), entity);
    let plain = entity_of(&mut world, &plain);
    assert!(world.get::<StrokeTips>(plain).is_none());
}

#[test]
fn group_blend_modes_reach_members_that_can_override_them() {
    use gaanim_core::peniko::{BlendMode, Mix};
    let mut canvas = SceneModel::new(640, 360);
    let plain = canvas.circle(1.0);
    let own = canvas.square(1.0);
    let opted = canvas.circle(0.3);
    let group = canvas
        .group(&[&plain, &own, &opted])
        .blend(Some(Mix::Screen.into()));
    // Like fill, a member restyled after grouping keeps its own mode,
    // including an explicit normal one.
    let own = own.blend(Some(Mix::Multiply.into()));
    let opted = opted.blend(Some(BlendMode::default()));
    let lone = canvas.circle(0.5);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);

    let mut blend_of = |handle: &DrawableHandle| {
        let entity = entity_of(&mut world, handle);
        world
            .get::<gaanim_renderer::effects::ElementBlend>(entity)
            .map(|blend| blend.0)
    };
    assert_eq!(blend_of(&plain), Some(BlendMode::from(Mix::Screen)));
    assert_eq!(blend_of(&own), Some(BlendMode::from(Mix::Multiply)));
    assert_eq!(blend_of(&opted), Some(BlendMode::default()));
    assert_eq!(blend_of(&group), Some(BlendMode::from(Mix::Screen)));
    assert_eq!(blend_of(&lone), None);
}

fn entity_of(world: &mut World, handle: &DrawableHandle) -> Entity {
    let id = ObjectId::from_raw(handle.id.as_raw() - 1);
    world
        .query::<(Entity, &MobjectId)>()
        .iter(world)
        .find(|(_, object)| object.0 == id)
        .unwrap()
        .0
}

#[test]
fn camera_views_bind_their_screen_to_the_framing_drawable() {
    use crate::canvas::{CameraViewError, CameraViewFit, CameraViewOptions};
    use gaanim_renderer::effects::{CameraView, ViewLayer};

    let mut canvas = SceneModel::new(640, 360);
    let frame = canvas.rect(2.0, 1.0).move_to(3.0, 2.0);
    let marker = canvas.dot(0.1).move_to(2.5, 2.0);
    let bone = canvas
        .circle(0.2)
        .move_to(3.2, 1.8)
        .view_layer(Some("xray"))
        .unwrap();
    let view = canvas
        .rounded_rect(4.0, 2.0, 0.2)
        .move_to(-4.0, -1.0)
        .camera_view_with(
            Some(&frame),
            CameraViewOptions {
                fit: CameraViewFit::Cover,
                exclude: vec![marker.clone()],
                layers: vec![" xray ".into()],
                ..Default::default()
            },
        )
        .unwrap();
    let screen = view.screen().clone();
    assert_eq!(view.frame().id, frame.id);
    assert!(
        view.zoom_source().is_none(),
        "the frame's size sets the zoom"
    );

    let (mut world, _) = compiled_world(&canvas);
    let screen_entity = entity_of(&mut world, &screen);
    let camera = world.get::<CameraView>(screen_entity).unwrap().clone();
    assert_eq!(camera.source, entity_of(&mut world, &frame));
    assert_eq!(camera.exclude, vec![entity_of(&mut world, &marker)]);
    assert_eq!(camera.fit, CameraViewFit::Cover);
    assert_eq!(camera.layers, vec![Arc::<str>::from("xray")]);
    assert!(camera.zoom.is_none());
    let bone_entity = entity_of(&mut world, &bone);
    assert_eq!(
        world.get::<ViewLayer>(bone_entity),
        Some(&ViewLayer(Arc::from("xray")))
    );

    let label = canvas.text("zoom");
    assert_eq!(
        label.camera_view(&frame).unwrap_err(),
        CameraViewError::UnsupportedScreen
    );
    assert_eq!(
        screen.clone().camera_view(&screen).unwrap_err(),
        CameraViewError::OwnSource
    );
    let foreign = SceneModel::new(640, 360).rect(1.0, 1.0);
    assert_eq!(
        screen.clone().camera_view(&foreign).unwrap_err(),
        CameraViewError::ForeignScene
    );
    assert_eq!(
        bone.clone().view_layer(Some("  ")).unwrap_err(),
        CameraViewError::InvalidLayer
    );

    screen.no_camera_view();
    bone.view_layer(None).unwrap();
    let (mut world, _) = compiled_world(&canvas);
    assert_eq!(world.query::<&CameraView>().iter(&world).count(), 0);
    assert_eq!(world.query::<&ViewLayer>().iter(&world).count(), 0);
}

#[test]
fn explicit_camera_view_zoom_animates_at_constant_perceived_speed() {
    use crate::canvas::{CameraViewError, CameraViewOptions, CameraViewZoom};
    use gaanim_renderer::effects::CameraView;

    let mut canvas = SceneModel::new(640, 360);
    let view = canvas
        .rect(4.0, 2.0)
        .camera_view_with(
            None,
            CameraViewOptions {
                zoom: Some(CameraViewZoom::Value(3.0)),
                ..Default::default()
            },
        )
        .unwrap();
    canvas.play(vec![view.animate_zoom_to(12.0).unwrap().duration(1.0)]);
    assert_eq!(view.zoom_to(0.0).unwrap_err(), CameraViewError::InvalidZoom);

    let (mut world, mut timeline) = compiled_world(&canvas);
    let screen = entity_of(&mut world, view.screen());
    let zoom_at = |world: &mut World, timeline: &mut Timeline, time: f64| {
        timeline.seek(world, time);
        let camera = world.get::<CameraView>(screen).unwrap().clone();
        camera.zoom.unwrap().evaluate(world).unwrap()
    };
    assert!((zoom_at(&mut world, &mut timeline, 0.0) - 3.0).abs() < 1e-9);
    // Halfway through the eased time the zoom is the geometric mean.
    assert!((zoom_at(&mut world, &mut timeline, 0.5) - 6.0).abs() < 1e-9);
    assert!((zoom_at(&mut world, &mut timeline, 1.0) - 12.0).abs() < 1e-9);
    let camera = world.get::<CameraView>(screen).unwrap().clone();
    assert_eq!(camera.source, entity_of(&mut world, view.frame()));

    let computed = canvas
        .rect(4.0, 2.0)
        .camera_view_with(
            None,
            CameraViewOptions {
                zoom: Some(CameraViewZoom::Source(2.0.into())),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        computed.zoom_to(3.0).unwrap_err(),
        CameraViewError::ComputedZoom
    );
}

#[test]
fn frame_sized_camera_views_zoom_by_scaling_their_frame() {
    let mut canvas = SceneModel::new(640, 360);
    let frame = canvas.rect(2.0, 1.0).move_to(3.0, 2.0);
    let view = canvas
        .rect(4.0, 2.0)
        .move_to(-4.0, -1.0)
        .camera_view(&frame)
        .unwrap();
    view.zoom_to(4.0).unwrap();
    canvas.play(vec![view.animate_zoom_to(8.0).unwrap().duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    for (time, scale) in [(0.0, 0.5), (1.0, 0.25)] {
        timeline.seek(&mut world, time);
        let transform = transform_of(&mut world, &frame);
        assert!(
            (transform.scale.x - scale).abs() < 1e-9,
            "{time}: {transform:?}"
        );
        assert!(
            (transform.scale.y - scale).abs() < 1e-9,
            "{time}: {transform:?}"
        );
    }
}

#[test]
fn camera_view_follows_and_removals_act_from_the_cursor() {
    use gaanim_renderer::effects::CameraView;

    let mut canvas = SceneModel::new(640, 360);
    let frame = canvas.rect(2.0, 1.0).move_to(3.0, 2.0);
    let screen = canvas.rect(4.0, 2.0).move_to(-4.0, -1.0);
    let view = screen.clone().camera_view(&frame).unwrap();
    let dot = canvas.dot(0.1).move_to(1.0, 1.0);
    canvas.play(vec![dot.animate().move_to(2.0, 1.0).duration(1.0)]);
    view.follow(crate::canvas::CanvasEndpoint::Entity(dot.id), DVec3::ZERO);
    canvas.play(vec![dot.animate().move_to(5.0, 2.0).duration(1.0)]);
    screen.no_camera_view();
    canvas.wait(1.0);

    let (mut world, _) = compiled_world(&canvas);
    let shown = world.query::<&CameraView>().single(&world).unwrap().clone();
    assert_eq!((shown.since, shown.until), (0.0, Some(2.0)));
    assert!(shown.shown_at(0.0) && shown.shown_at(1.99) && !shown.shown_at(2.0));
    let follow = world
        .query::<&gaanim_animation::EndpointFollow>()
        .single(&world)
        .unwrap();
    assert_eq!(follow.since, 1.0);
    assert!(!follow.holds_at(0.5) && follow.holds_at(1.0));
}

#[test]
fn camera_snapshots_freeze_what_their_view_showed() {
    use bevy::prelude::App;
    use gaanim_core::kurbo::Point;
    use gaanim_renderer::pipeline::{FrozenViews, capture_frame};

    let mut canvas = SceneModel::new(640, 360);
    let frame = canvas.rect(2.0, 2.0).move_to(3.0, 0.0);
    let screen = canvas.rect(2.0, 2.0).move_to(-3.0, 0.0);
    let view = screen.clone().camera_view(&frame).unwrap();
    let dot = canvas.dot(0.2).move_to(2.5, 0.0);
    canvas.play(vec![dot.animate().move_to(3.5, 0.0).duration(1.0)]);
    let photo = view.snapshot();
    canvas.play(vec![
        photo.animate().move_to(0.0, -2.0).duration(1.0),
        dot.animate().move_to(2.5, 0.5).duration(1.0),
    ]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    assert_eq!(app.world().resource::<FrozenViews>().len(), 1);
    let photo_entity = entity_of(app.world_mut(), &photo);
    let dot_entity = entity_of(app.world_mut(), &dot);
    let captured = |app: &mut App, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        capture_frame(app.world_mut(), None)
    };
    // Hidden until its move starts, as it was created.
    let before = captured(&mut app, 0.5);
    assert!(
        before
            .elements
            .iter()
            .all(|element| element.entity != photo_entity)
    );

    let after = captured(&mut app, 2.0);
    let shown = after
        .elements
        .iter()
        .find(|element| element.entity == photo_entity)
        .and_then(|element| element.screen.as_ref())
        .expect("the snapshot draws its view");
    assert!(shown.only_layers && shown.layers.len() == 1);
    let frozen: Vec<_> = after
        .elements
        .iter()
        .filter(|element| element.layer.as_ref() == Some(&shown.layers[0]))
        .collect();
    // The dot where the view saw it at 1 s, not where it went since.
    assert_eq!(frozen.len(), 1, "the view showed only the dot");
    assert_ne!(frozen[0].entity, dot_entity);
    let seen = frozen[0].transform.translation();
    assert!(
        (seen.x - 3.5).abs() < 1e-9 && seen.y.abs() < 1e-9,
        "{seen:?}"
    );
    // What the camera framed lands on the snapshot where it moved.
    let center = shown.content.unwrap() * Point::new(3.0, 0.0);
    assert!(
        center.x.abs() < 1e-9 && (center.y + 2.0).abs() < 1e-9,
        "{center:?}"
    );
    // The live view goes on showing the dot that moved.
    let live = after
        .elements
        .iter()
        .find(|element| element.entity == dot_entity)
        .unwrap();
    assert!(live.layer.is_none());
    let moved = live.transform.translation();
    assert!((moved.x - 2.5).abs() < 1e-9 && (moved.y - 0.5).abs() < 1e-9);
}

#[test]
fn camera_snapshots_freeze_the_3d_content_their_view_showed() {
    use bevy::prelude::App;
    use gaanim_renderer::pipeline::capture_frame;

    let mut canvas = SceneModel::new(640, 360);
    let frame = canvas.rect(2.0, 2.0).move_to(3.0, 0.0);
    let screen = canvas.rect(2.0, 2.0).move_to(-3.0, 0.0);
    let view = screen.clone().camera_view(&frame).unwrap();
    let cube = canvas
        .cube(0.4, gaanim_scene::Material3D::default())
        .unwrap()
        .move_to(2.5, 0.0);
    // Out of the frame, but kept: it may shadow what the frame shows.
    let sphere = canvas
        .sphere(0.3, 8, 6, gaanim_scene::Material3D::default())
        .unwrap()
        .move_to(-5.0, 2.5);
    canvas.play(vec![cube.animate().move_to(3.5, 0.0).duration(1.0)]);
    let photo = view.snapshot();
    canvas.play(vec![
        photo.animate().move_to(0.0, -2.0).duration(1.0),
        cube.animate().move_to(2.5, 0.5).duration(1.0),
    ]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let photo_entity = entity_of(app.world_mut(), &photo);
    let cube_entity = entity_of(app.world_mut(), &cube);
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(2.0);
    app.update();
    let after = capture_frame(app.world_mut(), None);
    let frozen = after
        .elements
        .iter()
        .find(|element| element.entity == photo_entity)
        .and_then(|element| element.screen.as_ref())
        .and_then(|view| view.frozen.clone())
        .expect("the snapshot keeps the 3D content its view showed");
    // The cube where the view saw it at 1 s and the sphere, under keys
    // of their own.
    let sources = &frozen.three_d.sources;
    let sphere_entity = entity_of(app.world_mut(), &sphere);
    assert_eq!(sources.len(), 2);
    assert!(
        sources
            .iter()
            .all(|source| source.entity != cube_entity && source.entity != sphere_entity)
    );
    assert!(
        sources.iter().any(|source| {
            let at = source.transform.w_axis;
            (at.x - 3.5).abs() < 1e-9 && at.y.abs() < 1e-9
        }),
        "the cube where it stood"
    );
    // Its image covers what the frame framed, as the screen showed it.
    let region = frozen.region;
    assert!(
        (region.x0 - 2.0).abs() < 1e-9
            && (region.x1 - 4.0).abs() < 1e-9
            && (region.y0 + 1.0).abs() < 1e-9
            && (region.y1 - 1.0).abs() < 1e-9,
        "{region:?}"
    );
    assert!(frozen.resolution > 0.0);
}

#[test]
fn camera_snapshots_keep_the_effects_mattes_and_glass_their_view_showed() {
    use bevy::prelude::App;
    use gaanim_renderer::pipeline::capture_frame;

    let mut canvas = SceneModel::new(640, 360);
    let frame = canvas.rect(4.0, 4.0).move_to(3.0, 0.0);
    let screen = canvas.rect(4.0, 4.0).move_to(-3.0, 0.0);
    let view = screen.clone().camera_view(&frame).unwrap();
    let grade = gaanim_renderer::post_presets::PostPreset::ColorGrade.shader();
    let graded = canvas.rect(0.5, 0.5).move_to(2.0, 1.0).shader_effect(
        vec![
            gaanim_renderer::post_process::PostProcessPass::constant(grade, &[0.0, 1.0, 0.0, 0.0])
                .unwrap(),
        ],
        0.3,
    );
    let hole = canvas.circle(0.4).move_to(4.0, 1.0);
    let matted = canvas
        .rect(1.0, 1.0)
        .move_to(4.0, 1.0)
        .matte(
            Some(&hole),
            gaanim_renderer::object_effects::MatteMode::Alpha,
        )
        .unwrap();
    let lens = canvas
        .rect(1.0, 0.6)
        .move_to(3.0, -1.0)
        .glass(Some(gaanim_renderer::object_effects::Glass::default()));
    canvas.wait(1.0);
    let photo = view.snapshot();
    canvas.play(vec![photo.animate().move_to(0.0, -2.0).duration(1.0)]);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let live: Vec<Entity> = [&graded, &matted, &hole, &lens]
        .into_iter()
        .map(|handle| entity_of(app.world_mut(), handle))
        .collect();
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(1.5);
    app.update();
    let after = capture_frame(app.world_mut(), None);
    let photo_entity = entity_of(app.world_mut(), &photo);
    let layer = after
        .elements
        .iter()
        .find(|element| element.entity == photo_entity)
        .and_then(|element| element.screen.as_ref())
        .map(|view| view.layers[0].clone())
        .expect("the snapshot draws its view");
    let frozen: Vec<_> = after
        .elements
        .iter()
        .filter(|element| element.layer.as_ref() == Some(&layer))
        .collect();
    // The frozen drawables keep their effect, matte and glass, under
    // frozen keys the frame's lists draw them through.
    let effect = frozen
        .iter()
        .find_map(|element| element.effect_root)
        .expect("a frozen drawable keeps its shader effect");
    let effect = after
        .effects
        .iter()
        .find(|captured| captured.root == effect)
        .expect("the frame draws the frozen effect");
    assert_eq!(effect.time, Some(1.0), "frozen at the snapshot's instant");
    let matte = frozen
        .iter()
        .find_map(|element| element.matte_root)
        .expect("a frozen drawable keeps its matte");
    let matte = after
        .mattes
        .iter()
        .find(|captured| captured.root == matte)
        .expect("the frame draws the frozen matte");
    assert!(
        frozen
            .iter()
            .any(|element| element.matte_of == Some(matte.source)),
        "the matte's source is frozen with it"
    );
    let glass = frozen
        .iter()
        .find_map(|element| element.glass_root)
        .expect("a frozen drawable keeps its glass");
    assert!(after.glasses.iter().any(|captured| captured.root == glass));
    for root in [effect.root, matte.root, matte.source, glass] {
        assert!(!live.contains(&root), "frozen keys are not live drawables");
    }
}

#[test]
fn a_camera_view_with_a_3d_camera_of_its_own_draws_from_where_it_stands() {
    use bevy::prelude::App;
    use gaanim_renderer::pipeline::capture_frame;

    let mut canvas = SceneModel::new(1280, 720);
    let ball = canvas
        .sphere(0.5, 12, 8, gaanim_scene::Material3D::default())
        .unwrap();
    let label = canvas.rect(0.4, 0.4).move_to(-3.0, 0.0);
    let screen = canvas.rect(4.0, 2.0).move_to(4.0, 2.0);
    let eye = DVec3::new(0.0, 10.0, 0.1);
    let view = screen
        .clone()
        .camera_view_3d(
            CanvasEndpoint::Static(eye),
            CanvasEndpoint::Entity(ball.id),
            DVec3::Y,
            0.6,
            Default::default(),
        )
        .unwrap();
    canvas.play(vec![ball.animate().move_to_3d(2.0, 0.0, 1.0).duration(1.0)]);
    let _ = (view, label);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    let screen = entity_of(app.world_mut(), &screen);
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(1.0);
    app.update();
    let camera = *app.world().resource::<gaanim_math::Camera>();
    let frame = capture_frame(app.world_mut(), Some(&camera));
    let shown = frame
        .elements
        .iter()
        .find(|element| element.entity == screen)
        .and_then(|element| element.screen.as_ref())
        .expect("the screen shows its view");
    // It shows only what its own camera draws, on a layer of its own.
    assert!(shown.only_layers && shown.layers.len() == 1);
    let own = shown
        .frozen
        .as_ref()
        .expect("its camera draws the 3D content");
    assert!((own.camera.position - eye).length() < 1e-9);
    // It looks at the ball where the ball is now, on a frame the
    // screen's shape as tall as the main camera's.
    let ball_at = DVec3::new(2.0, 0.0, 1.0);
    let center = own.camera.world_to_screen(ball_at)
        - gaanim_core::glam::DVec2::new(
            f64::from(own.camera.viewport_width) / 2.0,
            f64::from(own.camera.viewport_height) / 2.0,
        );
    assert!(center.length() < 1e-6, "{center:?}");
    assert!((own.region.width() / own.region.height() - 2.0).abs() < 1e-9);
    assert_eq!(own.three_d.sources.len(), 1, "the ball, and no 2D drawable");
}

#[test]
fn pop_out_and_pop_in_move_the_screen_through_the_region_its_camera_sees() {
    use crate::canvas::{CameraViewOptions, CameraViewZoom};

    let mut canvas = SceneModel::new(640, 360);
    let frame = canvas.rect(2.0, 1.0).move_to(3.0, 2.0);
    let view = canvas
        .rect(4.0, 2.0)
        .move_to(-4.0, -1.0)
        .camera_view(&frame)
        .unwrap();
    canvas.play(vec![view.pop_out().duration(1.0)]);
    canvas.play(vec![view.pop_in().duration(1.0)]);
    canvas.play(vec![view.pop_out().duration(1.0)]);
    // An explicit zoom of 4 shrinks the screen to a quarter over the frame.
    let lens = canvas
        .circle(1.0)
        .move_to(4.0, -3.0)
        .camera_view_with(
            Some(&frame),
            CameraViewOptions {
                zoom: Some(CameraViewZoom::Value(4.0)),
                ..Default::default()
            },
        )
        .unwrap();
    canvas.play(vec![lens.pop_out().duration(1.0)]);
    // A following camera pops back to where its target is now.
    let dot = canvas.dot(0.1).move_to(1.0, 1.0);
    let chase = canvas
        .rect(4.0, 2.0)
        .move_to(0.0, -3.0)
        .camera_view_with(
            None,
            CameraViewOptions {
                zoom: Some(CameraViewZoom::Value(2.0)),
                ..Default::default()
            },
        )
        .unwrap();
    chase.follow(crate::canvas::CanvasEndpoint::Entity(dot.id), DVec3::ZERO);
    canvas.play(vec![dot.animate().move_to(5.0, 2.0).duration(1.0)]);
    canvas.play(vec![chase.pop_in().duration(1.0)]);

    let (mut world, mut timeline) = compiled_world(&canvas);
    let placed = |world: &mut World, handle: &DrawableHandle| {
        let transform = transform_of(world, handle);
        (transform.translation.truncate(), transform.scale.x)
    };
    let region = DVec2::new(3.0, 2.0);
    let rest = DVec2::new(-4.0, -1.0);
    for (time, center, scale) in [
        (0.0, region, 0.5),
        (1.0, rest, 1.0),
        (2.0, region, 0.5),
        (3.0, rest, 1.0),
    ] {
        timeline.seek(&mut world, time);
        let (at, size) = placed(&mut world, view.screen());
        assert!(at.distance(center) < 1e-9, "{time}: {at:?}");
        assert!((size - scale).abs() < 1e-9, "{time}: {size}");
    }
    timeline.seek(&mut world, 3.0);
    let (at, size) = placed(&mut world, lens.screen());
    assert!(at.distance(region) < 1e-9 && (size - 0.25).abs() < 1e-9);
    timeline.seek(&mut world, 4.0);
    let (at, size) = placed(&mut world, lens.screen());
    assert!(at.distance(DVec2::new(4.0, -3.0)) < 1e-9 && (size - 1.0).abs() < 1e-9);
    timeline.seek(&mut world, 6.0);
    let (at, size) = placed(&mut world, chase.screen());
    assert!(at.distance(DVec2::new(5.0, 2.0)) < 1e-9, "{at:?}");
    assert!((size - 0.5).abs() < 1e-9, "{size}");
}

#[test]
fn camera_insets_join_their_frame_and_screen_and_hide_the_joins_from_the_view() {
    use crate::canvas::{CameraInsetOptions, CameraInsetShape, CameraViewError, CanvasEndpoint};
    use gaanim_renderer::effects::CameraView;

    let mut canvas = SceneModel::new(640, 360);
    let target = canvas.dot(0.1).move_to(-3.0, -1.0);
    let inset = canvas
        .camera_inset(
            CanvasEndpoint::Entity(target.id),
            CameraInsetOptions::default(),
        )
        .unwrap();
    assert_eq!(inset.connectors().len(), 2);
    let lens = canvas
        .camera_inset(
            CanvasEndpoint::Static(DVec3::new(2.0, 1.0, 0.0)),
            CameraInsetOptions {
                shape: CameraInsetShape::Circle,
                connectors: false,
                fixed: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(lens.connectors().is_empty());
    let reactive = CanvasEndpoint::Expression {
        x: 1.0.into(),
        y: 2.0.into(),
        z: 0.0.into(),
    };
    assert_eq!(
        canvas
            .camera_inset(reactive.clone(), CameraInsetOptions::default())
            .unwrap_err(),
        CameraViewError::UnsupportedTarget
    );
    canvas
        .camera_inset(
            reactive,
            CameraInsetOptions {
                follow: true,
                ..Default::default()
            },
        )
        .unwrap();

    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.0);
    let screen = entity_of(&mut world, inset.screen());
    let camera = world.get::<CameraView>(screen).unwrap().clone();
    for connector in inset.connectors() {
        let connector = entity_of(&mut world, connector);
        assert!(camera.exclude.contains(&connector));
    }
    assert!((camera.zoom.unwrap().evaluate(&world).unwrap() - 2.0).abs() < 1e-9);
    // Later ids shift past the helpers the first inset compiles, so find
    // the fixed screen by its components.
    let fixed_screens = world
        .query_filtered::<(), (With<CameraView>, With<gaanim_scene::HudOverlay>)>()
        .iter(&world)
        .count();
    let huds = world
        .query_filtered::<Entity, With<gaanim_scene::HudOverlay>>()
        .iter(&world)
        .count();
    let views = world.query::<&CameraView>().iter(&world).count();
    assert_eq!(
        fixed_screens, 1,
        "{huds} HUD entities, {views} camera views"
    );
    // The frame sits on the target, sized like the screen before its zoom.
    let frame = transform_of(&mut world, inset.frame());
    assert!(
        frame
            .translation
            .truncate()
            .distance(DVec2::new(-3.0, -1.0))
            < 1e-9
    );
}

#[test]
fn clip_mask_uses_another_drawables_world_geometry() {
    let mut canvas = SceneModel::new(640, 360);
    let target = canvas.rect(300.0, 160.0).move_to(80.0, 0.0);
    let mask = canvas.circle(55.0).move_to(80.0, 0.0);
    target.clip(&mask, gaanim_core::peniko::Fill::NonZero);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let masks = world
        .query::<&gaanim_renderer::effects::ClipMask>()
        .iter(&world)
        .collect::<Vec<_>>();
    assert_eq!(masks.len(), 1);
    assert!(!masks[0].path.is_empty());
    let bounds = masks[0].path.bounding_box();
    assert!((bounds.center().x).abs() < 1e-6);
    assert!((bounds.width() - 110.0).abs() < 1e-3);
}

/// A 3D polyline with a parameter in a coordinate is redrawn from it.
#[test]
fn reactive_3d_polylines_follow_their_coordinates() {
    let mut canvas = SceneModel::new(640, 360);
    let lift = canvas.parameter(0.0).unwrap();
    let color = PenikoColor::from_rgb8(192, 80, 46);
    canvas
        .reactive_polyline_3d(
            vec![
                [0.0.into(), lift.source(), 0.0.into()],
                [1.0.into(), 0.0.into(), 2.0.into()],
            ],
            None,
        )
        .stroke(color, 0.05);
    canvas.wait(0.5);
    lift.set(2.0).unwrap();
    canvas.wait(0.5);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let entity = world
        .query_filtered::<Entity, With<gaanim_animation::ReactiveLineRegen>>()
        .single(&world)
        .unwrap();
    let mut first_y = |world: &mut World, time: f64| {
        timeline.seek(world, time);
        gaanim_animation::reactive_3d_regen_system(world);
        let line = world.get::<LineListData>(entity).unwrap();
        assert_eq!(line.color, color);
        assert_eq!(line.points[1], [1.0, 0.0, 2.0]);
        line.points[0][1]
    };
    assert_eq!(first_y(&mut world, 0.2), 0.0);
    assert_eq!(first_y(&mut world, 0.8), 2.0);
}

#[test]
fn traced_3d_paths_draw_in_their_stroke_color() {
    let mut canvas = SceneModel::new(640, 360);
    let pen = canvas.dot(0.1);
    let color = PenikoColor::from_rgb8(192, 80, 46);
    let trace = canvas
        .traced_path_3d(&pen, None, None, 0.01)
        .stroke(color, 0.06);
    let (mut world, _) = compiled_world(&canvas);
    let entity = entity_of(&mut world, &trace);
    assert_eq!(world.get::<LineListData>(entity).unwrap().color, color);
}

#[test]
fn wipe_clips_its_target_to_a_hidden_mask_that_follows_its_bounds() {
    let mut canvas = SceneModel::new(640, 360);
    let target = canvas.rect(300.0, 160.0).move_to(80.0, 0.0);
    assert_eq!(
        target
            .wipe(
                ScalarSource::constant(0.5),
                gaanim_layout::Direction::Custom(DVec3::ZERO)
            )
            .err(),
        Some(crate::canvas::InvalidWipe)
    );
    let mask = target
        .wipe(ScalarSource::constant(0.5), gaanim_layout::Direction::Right)
        .unwrap();
    let ops = canvas.state.lock().unwrap().active().ops.clone();
    assert!(ops.iter().any(|op| matches!(
        op,
        Op::SetClip { target: clipped, mask: Some(by), .. }
            if *clipped == target.id && *by == mask.id
    )));
    assert!(ops.iter().any(|op| matches!(
        op,
        Op::AttachEndpointFollow {
            target: follower,
            endpoint: CanvasEndpoint::Between { .. },
            ..
        } if *follower == mask.id
    )));

    let animated = canvas.rect(100.0, 100.0);
    let anim = animated
        .animate()
        .wipe(gaanim_layout::Direction::Up)
        .unwrap();
    assert!(matches!(
        anim.inner.anim_type,
        AnimationType::SignalFloat { to } if to == 1.0
    ));
}

#[test]
fn segments_show_only_the_active_segment_on_seek() {
    let red = PenikoColor::from_rgb8(255, 0, 0);
    let blue = PenikoColor::from_rgb8(0, 0, 255);
    let mut canvas = SceneModel::new(640, 360);
    canvas.segment("first", None).unwrap();
    canvas.text("First segment").fill(red);
    canvas.wait(1.0);
    canvas.segment("second", None).unwrap();
    canvas.text("Second segment").fill(blue);
    canvas.wait(1.0);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

    let visible_for = |world: &mut World, color| {
        world
            .query::<(&FillBrush, Option<&gaanim_scene::Visible>)>()
            .iter(world)
            .find_map(|(fill, visible)| {
                matches!(&fill.0, Some(Brush::Solid(found)) if *found == color)
                    .then_some(visible.is_some())
            })
            .expect("colored segment object should exist")
    };

    timeline.seek(&mut world, 0.0);
    assert!(visible_for(&mut world, red));
    assert!(!visible_for(&mut world, blue));

    timeline.seek(&mut world, 1.0);
    assert!(!visible_for(&mut world, red));
    assert!(visible_for(&mut world, blue));
}

#[test]
fn terminal_stop_holds_the_completed_segment_at_a_shared_boundary() {
    let red = PenikoColor::from_rgb8(255, 0, 0);
    let blue = PenikoColor::from_rgb8(0, 0, 255);
    let mut canvas = SceneModel::new(640, 360);
    canvas.segment("first", None).unwrap();
    canvas.text("First segment").fill(red);
    canvas.wait(1.0);
    canvas.stop(None).unwrap();
    canvas.segment("second", None).unwrap();
    canvas.text("Second segment").fill(blue);
    canvas.wait(1.0);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

    let visible_for = |world: &mut World, color| {
        world
            .query::<(&FillBrush, Option<&gaanim_scene::Visible>)>()
            .iter(world)
            .find_map(|(fill, visible)| {
                matches!(&fill.0, Some(Brush::Solid(found)) if *found == color)
                    .then_some(visible.is_some())
            })
            .expect("colored segment object should exist")
    };

    timeline.seek(&mut world, 1.0);
    assert!(visible_for(&mut world, red));
    assert!(!visible_for(&mut world, blue));
    assert_eq!(
        timeline.segment_label().as_deref(),
        Some("1 / 2 · first · stop 1")
    );

    timeline.seek(&mut world, 1.000_001);
    assert!(!visible_for(&mut world, red));
    assert!(visible_for(&mut world, blue));
}

#[test]
fn segment_metadata_uses_the_compiled_clock_exactly() {
    // Per-segment cursors and the builder's running clock sum these waits
    // in different orders: the manifest ends at 9.4, the clips at
    // 9.399999999999999.
    let slides: [&[f64]; 8] = [
        &[0.15],
        &[0.55, 1.05, 0.35],
        &[0.45, 0.55, 0.35],
        &[0.35, 0.25],
        &[1.05, 1.05],
        &[0.95, 0.55],
        &[0.45, 0.35],
        &[0.95],
    ];
    let mut canvas = SceneModel::new(640, 360);
    for (index, waits) in slides.iter().enumerate() {
        canvas.segment(format!("slide {index}"), None).unwrap();
        for &wait in *waits {
            canvas.wait(wait);
            canvas.stop(None).unwrap();
        }
    }
    let manifest_end = canvas
        .segment_manifest()
        .segments
        .last()
        .map(|segment| segment.end_time)
        .unwrap();

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    assert!(
        manifest_end > timeline.cached_duration,
        "fixture must reproduce the float drift"
    );
    let stop_clips = timeline
        .clips
        .values()
        .filter(|clip| matches!(clip.payload, gaanim_timeline::clip::ClipPayload::Stop))
        .map(|clip| clip.start.to_bits())
        .collect::<HashSet<_>>();
    for segment in &timeline.segments {
        assert!(segment.end_time <= timeline.cached_duration);
        for stop in &segment.stops {
            assert!(
                stop_clips.contains(&stop.time.to_bits()),
                "{} has a stop at {} without a matching clip",
                segment.name,
                stop.time
            );
        }
        // A terminal stop keeps its own scene on screen, not the next one.
        let terminal = segment.stops.last().unwrap();
        let scene = timeline.scene_at(terminal.time).unwrap();
        assert_eq!(timeline.scenes[scene].name, segment.name);
    }
}

#[test]
fn justified_paragraph_compiles_to_vector_glyphs() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.configured_text(
        "Este párrafo debe ocupar varias líneas y conservar glifos vectoriales.",
        gaanim_text::prelude::TextStyle {
            size: Some(28.0),
            ..Default::default()
        },
        gaanim_text::prelude::TextFlow {
            wrap: gaanim_text::prelude::TextWrap::Width(180.0),
            align: gaanim_text::prelude::TextAlign::Justify,
            line_spacing: 1.25,
            ..Default::default()
        },
    );

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let mut query = world.query::<&LocalBounds>();
    let visible_bounds = query
        .iter(&world)
        .filter(|bounds| bounds.0.width() > 0.0 && bounds.0.height() > 0.0)
        .count();
    assert!(visible_bounds > 5, "paragraph should produce vector glyphs");
}

/// Height of the tallest compiled drawable in `canvas`.
fn tallest_compiled_height(canvas: &SceneModel) -> f64 {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);
    let mut query = world.query::<&LocalBounds>();
    query
        .iter(&world)
        .map(|bounds| bounds.0.height())
        .fold(0.0, f64::max)
}

#[test]
fn typst_documents_default_to_body_text_size() {
    let mut text = SceneModel::new(16.0, 9.0);
    text.text("Result table");
    let mut document = SceneModel::new(16.0, 9.0);
    document.typst("Result table");
    let text_height = tallest_compiled_height(&text);
    let document_height = tallest_compiled_height(&document);
    assert!(text_height > 0.0 && text_height < 1.0, "{text_height}");
    let ratio = document_height / text_height;
    assert!(
        (0.7..1.4).contains(&ratio),
        "a Typst document line ({document_height}) should match body text ({text_height})"
    );

    // Typst's own lengths keep their proportion to the text.
    let mut table = SceneModel::new(16.0, 9.0);
    table.typst("#table(columns: 2, [A], [B], [C], [D])");
    let table_height = tallest_compiled_height(&table);
    assert!(
        table_height > 2.0 * document_height && table_height < 8.0 * document_height,
        "two table rows with 5pt insets: {table_height} vs line {document_height}"
    );

    // Built-in components size generated markup in scene units.
    let mut units = SceneModel::new(16.0, 9.0);
    units.typst_in_scene_units("#set text(size: 0.5pt)\nResult table");
    let units_height = tallest_compiled_height(&units);
    assert!((0.3..0.8).contains(&units_height), "{units_height}");
}

/// Width of the widest compiled drawable in `canvas`.
fn widest_compiled_width(canvas: &SceneModel) -> f64 {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);
    let mut query = world.query::<&LocalBounds>();
    query
        .iter(&world)
        .map(|bounds| bounds.0.width())
        .fold(0.0, f64::max)
}

#[test]
fn ellipse_arcs_keep_their_two_radii() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    canvas.ellipse_arc(0.0, 0.0, 2.0, 0.5, 0.0, std::f64::consts::PI);
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);
    let bounds = world
        .query::<&LocalBounds>()
        .iter(&world)
        .find(|bounds| bounds.0.width() > 0.0)
        .expect("arc bounds")
        .0;
    // The upper half: 4 wide, half a radius tall.
    assert!((bounds.width() - 4.0).abs() < 1e-6, "{bounds:?}");
    assert!((bounds.height() - 0.5).abs() < 1e-6, "{bounds:?}");
    assert!(bounds.min.y.abs() < 1e-6, "{bounds:?}");
}

#[test]
fn typst_document_page_fits_centered_content() {
    let table = "#table(columns: 3, [a], [b], [c])";
    let mut plain = SceneModel::new(16.0, 9.0);
    plain.typst(table);
    let mut centered = SceneModel::new(16.0, 9.0);
    centered.typst(&format!("#align(center)[Title]\n{table}"));
    let plain_width = widest_compiled_width(&plain);
    let centered_width = widest_compiled_width(&centered);
    assert!(plain_width > 0.0, "{plain_width}");
    assert!(
        centered_width < plain_width * 1.2,
        "#align(center) must center within the content ({centered_width}), \
             not widen it to a page ({plain_width})"
    );
}

#[test]
fn max_lines_cuts_the_text_and_an_ellipsis_ends_it() {
    use gaanim_text::prelude::TextOverflow;
    let fonts = gaanim_text::font::FontRegistry::new();
    let white = gaanim_core::peniko::Color::WHITE;
    let spec = |overflow| {
        StructuredTextSpec::new(
            vec!["A bounded paragraph that runs over several lines of text.".into()],
            None,
            gaanim_text::prelude::TextStyle {
                size: Some(30.0),
                ..Default::default()
            },
            gaanim_text::prelude::TextFlow {
                wrap: gaanim_text::prelude::TextWrap::Width(240.0),
                line_spacing: 1.2,
                max_lines: Some(2),
                overflow,
                ..Default::default()
            },
        )
        .unwrap()
    };
    let lines = |source: &str| {
        gaanim_text::prelude::measure_typst_lines(
            &fonts,
            source,
            false,
            Some("New Computer Modern"),
            Some("New Computer Modern Math"),
            Some(30.0),
            None,
            Some(Brush::Solid(white)),
            StrokeBrush::transparent(),
        )
        .unwrap()
        .1
        .line_count
    };
    let fitted = |overflow| {
        fitted_text_typst_source(
            &fonts,
            &spec(overflow),
            Some(240.0),
            30.0,
            "New Computer Modern",
            "New Computer Modern Math",
            white,
        )
    };
    let full = structured_text_typst_source(
        &spec(TextOverflow::Clip),
        Some(240.0),
        30.0,
        "New Computer Modern",
        white,
    );
    assert!(lines(&full) > 2);

    let clipped = fitted(TextOverflow::Clip);
    assert_eq!(lines(&clipped), 2);
    assert!(!clipped.contains('…'));
    let ellipsis = fitted(TextOverflow::Ellipsis);
    assert_eq!(lines(&ellipsis), 2);
    assert!(ellipsis.contains('…'));
    // Visible overflow draws every line.
    assert_eq!(fitted(TextOverflow::Visible), full);
}

#[test]
fn text_cuts_end_between_characters_or_after_whole_formulas() {
    let spec = StructuredTextSpec::new(
        vec!["ab $x^2$ c\\$d".into()],
        None,
        StructuredTextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    )
    .unwrap();
    let leaves = structured_text_leaves(&spec);
    let points = leaf_cut_points(&leaves);
    // a, b, a space, the formula, a space, c, the escaped dollar, d.
    assert_eq!(points.len(), 8);
    assert_eq!(word_cuts(&points), [2, 4, 8]);
    let cut = |keep, ellipsis| {
        cut_leaves(&leaves, &points, keep, ellipsis)
            .into_iter()
            .map(|leaf| leaf.text)
            .collect::<String>()
    };
    assert_eq!(cut(4, false), "ab $x^2$");
    assert_eq!(cut(3, false), "ab");
    assert_eq!(cut(7, false), "ab $x^2$ c\\$");
    assert_eq!(cut(2, true), "ab…");
    assert_eq!(cut(0, true), "…");
}

#[test]
fn the_root_style_outlines_decorates_and_spaces_the_whole_text() {
    let spec = StructuredTextSpec::new(
        vec!["Notice".into()],
        None,
        StructuredTextStyle {
            decorations: vec!["underline".to_owned()],
            stroke_color: Some(PenikoColor::from_rgb8(255, 0, 0)),
            stroke_width: Some(0.05),
            word_spacing: Some(0.2),
            baseline: Some(0.1),
            ..Default::default()
        },
        gaanim_text::prelude::TextFlow::default(),
    )
    .unwrap();
    let source =
        structured_text_typst_source(&spec, None, 0.3, "New Computer Modern", PenikoColor::WHITE);
    assert!(source.contains("#underline["), "{source}");
    assert!(
        source.contains("stroke: 0.05pt + rgb(\"ff0000\")"),
        "{source}"
    );
    assert!(source.contains("spacing: 100% + 0.2pt"), "{source}");
    // A shifted baseline stays in its line instead of moving a block.
    assert!(source.contains("baseline: -0.1pt"), "{source}");
    assert!(!source.contains("#move("), "{source}");
}

/// Characters of the glyphs drawn at the timeline's current state, with
/// their fills: those whose own and parent opacity are both visible.
fn visible_glyphs(world: &mut World) -> Vec<(char, Option<Brush>)> {
    let parents: HashMap<Entity, f32> = world
        .query::<(Entity, &Opacity)>()
        .iter(world)
        .map(|(entity, opacity)| (entity, opacity.0))
        .collect();
    let mut glyphs: Vec<(u64, char, Option<Brush>)> = world
        .query::<(
            &gaanim_scene::components::TextSpan,
            &bevy::prelude::ChildOf,
            &Opacity,
            &FillBrush,
            &MobjectId,
        )>()
        .iter(world)
        .filter(|(_, parent, opacity, ..)| {
            opacity.0 > 0.5
                && parents
                    .get(&parent.parent())
                    .is_some_and(|value| *value > 0.5)
        })
        .map(|(span, _, _, fill, id)| (id.0.as_raw(), span.character, fill.0.clone()))
        .collect();
    glyphs.sort_by_key(|(id, ..)| *id);
    glyphs
        .into_iter()
        .map(|(_, character, fill)| (character, fill))
        .collect()
}

fn glyph_text(glyphs: &[(char, Option<Brush>)]) -> String {
    glyphs.iter().map(|(character, _)| character).collect()
}

#[test]
fn a_fade_transform_shows_the_glyphs_of_its_text_target() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.text("Old");
    let target = canvas.text("New");
    canvas.play(vec![source.animate().write().duration(1.0)]);
    canvas.play(vec![
        source
            .animate()
            .fade_transform_to(&target)
            .unwrap()
            .duration(1.0),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.9);
    assert_eq!(glyph_text(&visible_glyphs(&mut world)), "Old");
    timeline.seek(&mut world, 2.0);
    assert_eq!(glyph_text(&visible_glyphs(&mut world)), "New");
}

#[test]
fn become_after_the_text_was_shown_changes_it_at_the_cursor() {
    let mut canvas = SceneModel::new(640, 360);
    let text = canvas.text("Before");
    canvas.play(vec![text.animate().write().duration(1.0)]);
    text.r#become(
        StructuredTextSpec::new(
            vec!["Later".into()],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .unwrap(),
        None,
    );
    canvas.wait(1.0);
    // The handle keeps its identity: it moves the new content.
    canvas.play(vec![text.animate().fade_out().duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.99);
    assert_eq!(glyph_text(&visible_glyphs(&mut world)), "Before");
    timeline.seek(&mut world, 1.5);
    assert_eq!(glyph_text(&visible_glyphs(&mut world)), "Later");
    timeline.seek(&mut world, 3.0);
    assert_eq!(glyph_text(&visible_glyphs(&mut world)), "");
}

/// A text declared after earlier animations, which enters with a write
/// and later fades out and in, shows from its write on: the later
/// fade-in is not its entry.
#[test]
fn a_text_written_mid_segment_shows_before_its_later_fade_in() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.wait(0.5);
    let text = canvas.text("Late");
    canvas.play(vec![text.animate().write().duration(0.5)]);
    canvas.wait(1.0);
    canvas.play(vec![text.animate().fade_out().duration(0.5)]);
    canvas.play(vec![text.animate().fade_in().duration(0.5)]);
    canvas.wait(0.5);
    let (mut world, mut timeline) = compiled_world(&canvas);
    for (time, expected) in [
        (0.25, ""),
        (1.5, "Late"),
        (2.5, ""),
        (3.25, "Late"),
        (1.5, "Late"),
    ] {
        timeline.seek(&mut world, time);
        assert_eq!(
            glyph_text(&visible_glyphs(&mut world)),
            expected,
            "at {time}"
        );
    }
}

/// A hidden text that a move shows draws its glyphs, whether the move
/// plays alone or with a fade-in.
#[test]
fn a_hidden_text_a_move_shows_draws_its_glyphs() {
    let mut canvas = SceneModel::new(640, 360);
    let moved = canvas.text("Moved").hidden().unwrap();
    let faded = canvas.text("Faded").hidden().unwrap();
    canvas.wait(0.5);
    canvas.play(vec![
        moved.animate().move_to_3d(2.0, 1.0, 0.0).duration(0.5),
        faded.animate().move_to_3d(-2.0, 1.0, 0.0).duration(0.5),
        faded.animate().fade_in().duration(0.5),
    ]);
    canvas.wait(0.5);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.25);
    assert_eq!(glyph_text(&visible_glyphs(&mut world)), "");
    timeline.seek(&mut world, 1.25);
    assert_eq!(glyph_text(&visible_glyphs(&mut world)), "MovedFaded");
    // The fade-in owns the faded text's opacity: halfway, it is half on.
    timeline.seek(&mut world, 0.75);
    let faded = world
        .query::<(&gaanim_scene::components::TextSpan, &bevy::prelude::ChildOf)>()
        .iter(&world)
        .find(|(span, _)| span.character == 'F')
        .map(|(_, parent)| parent.parent())
        .unwrap();
    let opacity = world.get::<Opacity>(faded).unwrap().0;
    assert!((opacity - 0.5).abs() < 0.05, "{opacity}");
}

#[test]
fn a_part_recolored_after_the_text_was_shown_changes_at_the_cursor() {
    let accent = PenikoColor::from_rgb8(255, 160, 0);
    let mut canvas = SceneModel::new(640, 360);
    let text = canvas.text_spec(
        StructuredTextSpec::new(
            vec![
                "is ".into(),
                gaanim_text::prelude::TextPart::new(
                    "mass",
                    vec!["mass".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .unwrap(),
    );
    canvas.play(vec![text.animate().write().duration(1.0)]);
    assert!(!text.fill_text_part(&["mass".to_owned()], accent));
    text.select("mass").fill(accent);
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let accented = |world: &mut World| {
        visible_glyphs(world)
            .into_iter()
            .filter(|(_, fill)| *fill == Some(Brush::Solid(accent)))
            .map(|(character, _)| character)
            .collect::<String>()
    };
    timeline.seek(&mut world, 0.99);
    assert_eq!(accented(&mut world), "");
    timeline.seek(&mut world, 1.5);
    assert_eq!(accented(&mut world), "mass");
}

#[test]
fn equation_fragment_fill_overrides_matching_vector_glyphs() {
    let highlight = gaanim_core::peniko::Color::from_rgb8(255, 180, 0);
    let mut canvas = SceneModel::new(640, 360);
    canvas.math_text("E = m c^2").color_by("m", highlight);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let mut query = world.query::<&gaanim_scene::FillBrush>();
    assert!(query.iter(&world).any(|fill| {
        matches!(
            &fill.0,
            Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
        )
    }));
}

#[test]
fn text_fragment_fill_overrides_matching_vector_glyphs() {
    let highlight = gaanim_core::peniko::Color::from_rgb8(64, 180, 255);
    let mut canvas = SceneModel::new(640, 360);
    canvas
        .text("Energy depends on mass")
        .color_by("mass", highlight);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let mut query = world.query::<&gaanim_scene::FillBrush>();
    assert!(query.iter(&world).any(|fill| {
        matches!(
            &fill.0,
            Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
        )
    }));
}

#[test]
fn compound_paint_animation_reaches_text_glyphs() {
    let fill_target = PenikoColor::from_rgb8(32, 96, 224);
    let stroke_target = PenikoColor::from_rgb8(255, 180, 0);
    let fragment_start = PenikoColor::from_rgb8(220, 32, 64);
    let mut canvas = SceneModel::new(640, 360);
    let text = canvas
        .text("Color")
        .stroke(PenikoColor::WHITE, 2.0)
        .color_by("C", fragment_start);
    canvas.play(vec![
        text.animate()
            .color(fill_target)
            .stroke(stroke_target, 7.0)
            .duration(1.0),
    ]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.5);

    let mut halfway_fills = world
        .query::<(&FillBrush, &LocalBounds)>()
        .iter(&world)
        .filter_map(|(fill, bounds)| {
            (bounds.0.width() > 0.0 && bounds.0.height() > 0.0)
                .then(|| match fill.0 {
                    Some(Brush::Solid(color)) => Some(color.to_rgba8()),
                    _ => None,
                })
                .flatten()
        })
        .collect::<Vec<_>>();
    halfway_fills.sort_by_key(|color| (color.r, color.g, color.b, color.a));
    halfway_fills.dedup();
    assert!(
        halfway_fills.len() > 1,
        "each glyph should interpolate from its own fill, including fragment overrides"
    );

    timeline.seek(&mut world, 1.0);

    let painted_glyphs = world
        .query::<(&FillBrush, &gaanim_scene::StrokeBrush, &LocalBounds)>()
        .iter(&world)
        .filter(|(fill, stroke, bounds)| {
            bounds.0.width() > 0.0
                && bounds.0.height() > 0.0
                && matches!(&fill.0, Some(Brush::Solid(color)) if *color == fill_target)
                && matches!(&stroke.brush, Some(Brush::Solid(color)) if *color == stroke_target)
                && (stroke.style.width - 7.0).abs() < 1e-9
        })
        .count();

    assert!(
        painted_glyphs > 1,
        "compound paint animation should update the visible text glyphs, got {painted_glyphs}"
    );
}

#[test]
fn paper_theme_applies_role_fills_to_text_glyphs() {
    let mut canvas = SceneModel::new(640, 360);
    canvas
        .set_theme("paper")
        .expect("paper is a built-in theme");
    canvas.test_title("Heading");
    canvas.test_subtitle("Subheading");
    canvas.text("Body copy");
    canvas.math_text("x = y");

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = canvas.themed_text_config();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let fills: Vec<_> = world
        .query::<&gaanim_scene::FillBrush>()
        .iter(&world)
        .filter_map(|fill| match fill.0 {
            Some(gaanim_core::peniko::Brush::Solid(color)) => Some(color),
            _ => None,
        })
        .collect();

    assert!(fills.contains(&PenikoColor::BLACK));
    assert!(!fills.contains(&PenikoColor::WHITE));
}

#[test]
fn group_z_index_keeps_its_creation_order() {
    // Regression for #25: post_apply used to reset creation_order to 0,
    // dropping the tie-breaker for layered groups and texts.
    let mut canvas = SceneModel::new(16.0, 9.0);
    let first = canvas.rect(1.0, 1.0);
    let second = canvas.circle(0.5);
    canvas.group(&[&first, &second]).z_index(5);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let mut query = world.query_filtered::<&RenderOrder, With<gaanim_scene::GroupMarker>>();
    let layered = query
        .iter(&world)
        .find(|order| order.z_index == 5)
        .expect("layered group");
    assert_ne!(layered.creation_order, 0);
}

fn compiled_solid_fills(canvas: &SceneModel) -> (World, Vec<PenikoColor>) {
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let colors = world
        .query::<&gaanim_scene::FillBrush>()
        .iter(&world)
        .filter_map(|fill| match fill.0.as_ref() {
            Some(gaanim_core::peniko::Brush::Solid(color)) => Some(*color),
            _ => None,
        })
        .collect();
    (world, colors)
}

#[test]
fn new_scenes_compile_with_the_default_technical_theme() {
    let technical = crate::canvas::CanvasTheme::builtin("technical").unwrap();
    let mut canvas = SceneModel::new(640, 360);
    assert_eq!(canvas.theme.as_deref(), Some("technical"));
    assert_eq!(canvas.background, Some(technical.palette.background));
    canvas.circle(40.0);
    let explicit = PenikoColor::from_rgb8(0xFF, 0x00, 0x00);
    canvas.circle(20.0).fill(explicit).move_to(100.0, 0.0);
    canvas.text("Body copy");

    let (world, fills) = compiled_solid_fills(&canvas);
    assert!(
        fills.contains(&technical.palette.accent),
        "themed shape fill"
    );
    assert!(fills.contains(&explicit), "explicit fills still win");
    assert!(fills.contains(&technical.palette.foreground), "themed text");
    let rgba = technical.palette.background.to_rgba8();
    assert_eq!(
        world.resource::<ClearColor>().0,
        Color::srgba_u8(rgba.r, rgba.g, rgba.b, rgba.a)
    );
}

#[test]
fn explicit_backgrounds_survive_theme_changes_and_removal() {
    let navy = PenikoColor::from_rgb8(0x0F, 0x17, 0x2A);
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_background(Some(navy));
    canvas.set_theme("paper").unwrap();
    assert_eq!(canvas.background, Some(navy));
    canvas.clear_theme();
    assert_eq!(
        (canvas.theme.as_deref(), canvas.background),
        (None, Some(navy))
    );

    let mut plain = SceneModel::new(640, 360);
    plain.clear_theme();
    assert_eq!(
        (plain.background, plain.background_paint.is_none()),
        (None, true)
    );
    assert!(plain.theme_color("foreground").is_err());
    plain.set_theme("paper").unwrap();
    assert_eq!(plain.background, Some(PenikoColor::WHITE));
}

#[test]
fn unthemed_compile_uses_the_host_text_config_like_runtime_replay() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.clear_theme();
    canvas.text("Body copy");
    let host = gaanim_text::prelude::TextConfig::default();
    let resolved = canvas.scene_text_config(&host);
    for (role, style) in &host.roles {
        assert_eq!(
            resolved.roles[role].fill_color, style.fill_color,
            "{role:?}"
        );
    }
    // `compile` no longer picks black text for the white default
    // background: both paths keep the host (white) foreground, which
    // `unthemed_contrast_warning` reports.
    let (_, fills) = compiled_solid_fills(&canvas);
    assert!(fills.contains(&PenikoColor::WHITE));
    assert!(!fills.contains(&PenikoColor::BLACK));
    assert!(canvas.unthemed_contrast_warning().is_some());
}

#[test]
fn theme_selected_after_authoring_is_materialized_during_compile() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.circle(40.0);
    canvas
        .circle(20.0)
        .fill(PenikoColor::BLACK)
        .move_to(100.0, 0.0);
    let mut theme = crate::canvas::CanvasTheme::builtin("paper").unwrap();
    let brand = PenikoColor::from_rgb8(0x25, 0x63, 0xEB);
    theme
        .set_colors(&HashMap::from([("accent".to_string(), brand)]))
        .unwrap();
    canvas.apply_theme(theme);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let mut query = world.query::<&gaanim_scene::FillBrush>();
    let colors = query
        .iter(&world)
        .filter_map(|fill| match fill.0.as_ref() {
            Some(gaanim_core::peniko::Brush::Solid(color)) => Some(*color),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(colors.contains(&brand));
    assert!(colors.contains(&PenikoColor::BLACK));
}

#[test]
fn camera_zoom_and_frame_default_to_exponential_with_zoom_hand_off() {
    let mut canvas = SceneModel::new(960, 540);
    let card = canvas.rect(120.0, 80.0).move_to(200.0, -60.0);
    let zoom_in = canvas.camera_zoom_to_source(ScalarSource::constant(8.0), 1.0);
    canvas.play(vec![zoom_in]);
    let frame = canvas.camera_frame_many(std::slice::from_ref(&card), [20.0; 4], false, 1.0);
    canvas.play(vec![frame]);
    let linear = canvas.camera_frame_many_with_interpolation(
        &[card],
        [20.0; 4],
        false,
        gaanim_math::ZoomInterpolation::Linear,
        1.0,
    );
    canvas.play(vec![linear]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    use gaanim_timeline::clip::PropertyLensSpec;
    let mut lenses: Vec<_> = world
        .resource::<Timeline>()
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(animation) => {
                Some((clip.start, animation.lens.clone()))
            }
            _ => None,
        })
        .collect();
    lenses.sort_by(|left, right| left.0.total_cmp(&right.0));
    assert!(lenses.iter().any(|(_, lens)| matches!(
        lens,
        PropertyLensSpec::CameraZoomSource {
            interpolation: gaanim_math::ZoomInterpolation::Exponential,
            ..
        }
    )));
    let pan_zoom = lenses
        .iter()
        .find_map(|(_, lens)| match lens {
            PropertyLensSpec::CameraPanZoom {
                from_zoom,
                interpolation,
                ..
            } => Some((*from_zoom, *interpolation)),
            _ => None,
        })
        .expect("exponential frame_to emits one coupled pan+zoom clip");
    // The constant zoom_to target is the hand-off pose for the next frame.
    assert_eq!(pan_zoom, (8.0, gaanim_math::ZoomInterpolation::Exponential));
    assert!(lenses.iter().any(|(_, lens)| matches!(
        lens,
        PropertyLensSpec::CameraZoom {
            interpolation: gaanim_math::ZoomInterpolation::Linear,
            ..
        }
    )));
}

#[test]
fn dynamic_camera_frame_keeps_all_compiled_targets_and_bounds() {
    let mut canvas = SceneModel::new(960, 540);
    let left = canvas.circle(85.0).move_to(-260.0, -10.0);
    let right = canvas.rect(180.0, 110.0).move_to(250.0, -10.0);
    let frame = canvas.camera_frame_many(
        &[left.clone(), right.clone()],
        [48.0, 72.0, 48.0, 72.0],
        true,
        1.0,
    );
    canvas.play(vec![frame]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let targets = world
        .resource::<Timeline>()
        .clips
        .values()
        .find_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(animation) => match &animation.lens {
                gaanim_timeline::clip::PropertyLensSpec::CameraFrameDynamic { targets, .. } => {
                    Some(targets.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("dynamic camera frame clip");
    assert_eq!(targets.len(), 2);
    let bounds = targets
        .iter()
        .map(|target| gaanim_animation::resolve_entity_bounds(*target, &world))
        .collect::<Vec<_>>();
    assert!(bounds.iter().all(Option::is_some), "bounds: {bounds:?}");
    let union = bounds
        .into_iter()
        .flatten()
        .reduce(|left, right| left.union(&right))
        .unwrap();
    assert!(union.min.x < -300.0, "union: {union:?}");
    assert!(union.max.x > 300.0, "union: {union:?}");

    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.5);
    world.insert_resource(timeline);
    let restored_bounds = targets
        .iter()
        .map(|target| gaanim_animation::resolve_entity_bounds(*target, &world))
        .collect::<Vec<_>>();
    assert!(
        restored_bounds.iter().all(Option::is_some),
        "restored bounds: {restored_bounds:?}"
    );
    let restored_union = restored_bounds
        .into_iter()
        .flatten()
        .reduce(|left, right| left.union(&right))
        .unwrap();
    assert!(
        restored_union.min.x < -300.0,
        "restored: {restored_union:?}"
    );
    assert!(restored_union.max.x > 300.0, "restored: {restored_union:?}");
}

#[test]
fn compiled_camera_binding_is_active_during_its_authored_window() {
    let mut canvas = SceneModel::new(960, 540);
    let constraint = canvas
        .camera_bind_2d(
            Some(CanvasEndpoint::Static(DVec3::new(120.0, -35.0, 0.0))),
            Some(ScalarSource::constant(1.4)),
            None,
            ScalarSource::constant(1.0),
            true,
        )
        .unwrap();
    canvas.wait(2.0);
    constraint.disable();

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    world.insert_resource(gaanim_math::Camera::ortho_2d(960, 540));

    assert_eq!(
        world
            .query::<&gaanim_animation::CameraBinding>()
            .iter(&world)
            .count(),
        1
    );
    gaanim_animation::apply_camera_bindings(&mut world, 1.0);
    let camera = world.resource::<gaanim_math::Camera>();
    assert_eq!(camera.position.x, 120.0);
    assert_eq!(camera.position.y, -35.0);
    assert!(matches!(
        camera.projection,
        gaanim_math::Projection::Orthographic { zoom: 1.4 }
    ));
}

#[test]
fn compiled_camera_binding_resolves_point_ref_parameters() {
    let mut canvas = SceneModel::new(960, 540);
    let parameter = canvas.parameter(0.0).unwrap();
    let parameter_id = parameter.drawable().id;
    let x = ScalarSource::function(ReactiveFunction::new(
        0,
        1,
        vec![gaanim_animation::ReactiveInput::Signal(parameter_id)],
        |values| Ok(vec![values[0] * 260.0 - 130.0]),
    ))
    .unwrap();
    let point = canvas.point_ref(x, ScalarSource::constant(25.0));
    let _constraint = canvas
        .camera_bind_2d(Some(point.0), None, None, ScalarSource::constant(1.0), true)
        .unwrap();

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    world.insert_resource(gaanim_math::Camera::ortho_2d(960, 540));

    gaanim_animation::apply_camera_bindings(&mut world, 0.0);
    let camera = world.resource::<gaanim_math::Camera>();
    assert_eq!(camera.position.x, -130.0);
    assert_eq!(camera.position.y, 25.0);
}

/// `bind_3d` reads an eye and an up made of reactive coordinates every
/// frame, with no point to carry them.
#[test]
fn compiled_3d_camera_binding_reads_reactive_coordinates() {
    let mut canvas = SceneModel::new(960, 540);
    let parameter = canvas.parameter(0.0).unwrap();
    let parameter_id = parameter.drawable().id;
    let x = ScalarSource::function(ReactiveFunction::new(
        0,
        1,
        vec![gaanim_animation::ReactiveInput::Signal(parameter_id)],
        |values| Ok(vec![values[0] * 10.0]),
    ))
    .unwrap();
    let eye = CanvasEndpoint::Expression {
        x: x.clone(),
        y: 4.0.into(),
        z: 12.0.into(),
    };
    let up = CanvasEndpoint::Expression {
        x,
        y: 1.0.into(),
        z: 0.0.into(),
    };
    let _constraint = canvas
        .camera_bind_3d(
            Some(eye),
            Some(CanvasEndpoint::Static(DVec3::ZERO)),
            None,
            up,
            None,
            ScalarSource::constant(1.0),
            true,
        )
        .unwrap();
    canvas.play(vec![parameter.animate().set(0.5).duration(1.0)]);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    world.insert_resource(gaanim_math::Camera::ortho_2d(960, 540));
    timeline.seek(&mut world, 1.0);
    gaanim_animation::apply_camera_bindings(&mut world, 1.0);
    let camera = world.resource::<gaanim_math::Camera>();
    assert!(
        (camera.position - DVec3::new(5.0, 4.0, 12.0)).length() < 1e-9,
        "{:?}",
        camera.position
    );
    assert!((camera.up - DVec3::new(5.0, 1.0, 0.0).normalize()).length() < 1e-9);
}

#[test]
fn fragment_transform_moves_selected_vector_glyphs() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.math_text("E = m c^2");
    let target = canvas.math_text("p = m v");
    let morph = source
        .select("m")
        .morph_to(&target.select("m"), 0.8)
        .expect("selections share a SceneModel");
    canvas.play(vec![morph]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    assert!(timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
                lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
                ..
            })
        )
    }));
}

#[test]
fn named_fragment_tag_resolves_to_a_vector_selection() {
    let highlight = gaanim_core::peniko::Color::from_rgb8(64, 180, 255);
    let mut canvas = SceneModel::new(640, 360);
    let formula = canvas.math_text("E = m c^2").define_tag("mass", "m", None);
    formula
        .tag("mass")
        .expect("registered tag should resolve")
        .fill(highlight);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let mut query = world.query::<&gaanim_scene::FillBrush>();
    assert!(query.iter(&world).any(|fill| {
        matches!(
            &fill.0,
            Some(gaanim_core::peniko::Brush::Solid(color)) if *color == highlight
        )
    }));
}

#[test]
fn cancel_term_places_a_diagonal_strike_over_the_selected_glyphs() {
    let strike_color = PenikoColor::WHITE;
    let mut canvas = SceneModel::new(640, 360);
    let formula = canvas
        .math_text("x + 3 = 7")
        .define_tag("constant", "3", None);
    let cancel = formula
        .tag("constant")
        .expect("registered tag should resolve")
        .cancel(0.6);
    canvas.play(vec![cancel]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let selected_bounds = {
        let mut query = world.query::<(&gaanim_scene::components::TextSpan, &LocalBounds)>();
        query
            .iter(&world)
            .find_map(|(span, bounds)| (span.character == '3').then_some(bounds.0))
            .expect("the selected digit should retain its glyph bounds")
    };
    let strike_bounds = {
        let mut query = world.query::<(&gaanim_scene::StrokeBrush, &LocalBounds)>();
        query
                .iter(&world)
                .find_map(|(stroke, bounds)| {
                    matches!(&stroke.brush, Some(gaanim_core::peniko::Brush::Solid(color)) if *color == strike_color)
                        .then_some(bounds.0)
                })
                .expect("cancel should spawn a white strikethrough")
    };
    let center_delta = strike_bounds.center() - selected_bounds.center();
    assert!(
        center_delta.length() < 3.0,
        "cancel strike center {:?} should overlap selected glyph center {:?}",
        strike_bounds.center(),
        selected_bounds.center()
    );
    assert!(strike_bounds.width() > 0.0 && strike_bounds.height() > 0.0);
}

#[test]
fn cancel_mark_fades_when_text_step_replaces_its_source() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.text_spec(
        StructuredTextSpec::new(
            vec![
                "$".into(),
                gaanim_text::prelude::TextPart::new(
                    "variable",
                    vec!["x".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                " + ".into(),
                gaanim_text::prelude::TextPart::new(
                    "constant",
                    vec!["3".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                " = ".into(),
                gaanim_text::prelude::TextPart::new(
                    "result",
                    vec!["7".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                "$".into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid source equation"),
    );
    let target = canvas.text_spec(
        StructuredTextSpec::new(
            vec![
                "$".into(),
                gaanim_text::prelude::TextPart::new(
                    "variable",
                    vec!["x".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                " = ".into(),
                gaanim_text::prelude::TextPart::new(
                    "result",
                    vec!["4".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                "$".into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid target equation"),
    );
    canvas.play(vec![
        source.tag("constant").expect("constant tag").cancel(0.6),
    ]);
    canvas.play(vec![source.step_to(&target, None, 0.8).unwrap()]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let strike = world
        .query::<(
            bevy::prelude::Entity,
            &gaanim_scene::StrokeBrush,
            Option<&bevy::prelude::ChildOf>,
        )>()
        .iter(&world)
        .find_map(|(entity, stroke, parent)| {
            (parent.is_none() && stroke.brush.is_some()).then_some(entity)
        })
        .expect("cancel should spawn a root strike");

    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 1.4);
    assert!(
        world
            .get::<Opacity>(strike)
            .is_some_and(|opacity| opacity.0 < 0.01),
        "the cancellation mark must leave with the replaced source text"
    );
}

#[test]
fn tagged_equation_transform_moves_shared_tags() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas
        .math_text("E = m c^2")
        .define_tag("mass", "m", None)
        .move_to(0.0, 70.0);
    let target = canvas
        .math_text("p = m v")
        .define_tag("mass", "m", None)
        .move_to(0.0, -90.0);
    let copy = source
        .tag("mass")
        .unwrap()
        .copy_to(&target.tag("mass").unwrap(), 0.8)
        .expect("selections share a SceneModel");
    canvas.play(vec![copy]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    assert!(timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                    ..
                }
            ) if from.y > to.y
        )
    }));
}

#[test]
fn reveal_groups_align_ligatures_and_attach_punctuation() {
    let ids: Vec<ObjectId> = (1..=6).map(ObjectId::from_raw).collect();
    // "¡fire, ok": the "fi" ligature draws one glyph for two characters.
    let glyphs: Vec<(ObjectId, char)> = ids.iter().copied().zip("¡fre,o".chars()).collect();
    let visible = [
        ('¡', None),
        ('f', Some(0)),
        ('i', Some(0)),
        ('r', Some(0)),
        ('e', Some(0)),
        (',', None),
        ('o', Some(1)),
    ];
    assert_eq!(
        reveal_groups(&glyphs, &visible),
        Some(vec![ids[..5].to_vec(), vec![ids[5]]])
    );
    assert_eq!(reveal_groups(&glyphs, &[('x', None)]), None);
}

#[test]
fn clips_carry_the_play_they_came_from_while_lines_are_tracked() {
    crate::canvas::track_script_lines(true);
    let mut canvas = SceneModel::new(640, 360);
    let circle = canvas.circle(1.0);
    let square = canvas.square(1.0);
    canvas.play(vec![circle.fade_in(1.0)]);
    canvas.play(vec![square.fade_in(0.5), circle.fade_out(0.5)]);
    let timeline = compiled_timeline(&canvas);
    let mut origins: Vec<_> = timeline
        .clips
        .iter()
        .filter(|(_, clip)| {
            matches!(
                clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(_)
            )
        })
        .map(|(id, clip)| (clip.start, timeline.clip_origins.get(&id).copied()))
        .collect();
    origins.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(origins.iter().all(|(_, origin)| origin.is_some()));
    let first = origins.first().unwrap().1;
    let last = origins.last().unwrap().1;
    assert_ne!(first, last, "each play has its own origin");
    // The key names the play's segment and its place there.
    let state = canvas.state.lock().unwrap();
    let segment = state.active();
    let plays: Vec<_> = segment
        .ops
        .iter()
        .enumerate()
        .filter(|(_, op)| matches!(op, Op::Play(_)))
        .map(|(index, _)| crate::canvas::ops::op_origin(segment.id, index))
        .collect();
    assert_eq!(plays, vec![first.unwrap(), last.unwrap()]);
}

fn compiled_timeline(canvas: &SceneModel) -> Timeline {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    timeline
}

fn translation_clips(timeline: &Timeline) -> Vec<(f64, f64, DVec3, DVec3, RateFunc)> {
    let mut clips: Vec<_> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                    rate_func,
                    ..
                },
            ) => Some((clip.start, clip.duration, *from, *to, rate_func.clone())),
            _ => None,
        })
        .collect();
    clips.sort_by(|a, b| a.0.total_cmp(&b.0));
    clips
}

#[test]
fn even_yoyo_returns_to_start_and_offset_loops_accumulate() {
    use gaanim_math::RepeatMode;
    let mut canvas = SceneModel::new(640, 360);
    let dot = canvas.circle(0.2);
    canvas.play(vec![dot.animate().shift_by(1.0, 0.0).duration(0.5).repeat(
        2,
        RepeatMode::PingPong,
        0.25,
    )]);
    canvas.play(vec![dot.animate().shift_by(0.0, 1.0).duration(1.0)]);
    let clips = translation_clips(&compiled_timeline(&canvas));
    assert_eq!(clips.len(), 2);
    let (start, duration, from, to, rate) = &clips[0];
    assert_eq!((*start, *duration), (0.0, 1.25));
    assert_eq!(*to, DVec3::new(1.0, 0.0, 0.0));
    assert!(rate.evaluate(1.0).abs() < 1e-12);
    // The next animation starts from where the yoyo came back to.
    assert_eq!(clips[1].0, 1.25);
    assert_eq!(clips[1].2, *from);
    assert_eq!(clips[1].3, *from + DVec3::Y);

    let mut canvas = SceneModel::new(640, 360);
    let dot = canvas.circle(0.2);
    canvas.play(vec![
        dot.animate()
            .shift_by(1.0, 0.0)
            .duration(0.5)
            .loop_for(1.6, RepeatMode::Offset, 0.0),
    ]);
    let clips = translation_clips(&compiled_timeline(&canvas));
    let starts: Vec<f64> = clips.iter().map(|clip| clip.0).collect();
    assert_eq!(starts, [0.0, 0.5, 1.0]);
    assert_eq!(clips[2].3, clips[0].2 + DVec3::new(3.0, 0.0, 0.0));
}

#[test]
fn oriented_paths_and_arcs_compile_to_path_follow() {
    let mut canvas = SceneModel::new(640, 360);
    let route = canvas.polyline(&[(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)]);
    let plane = canvas.circle(0.2);
    canvas.play(vec![
        plane
            .animate()
            .move_along_with(
                &route,
                crate::anim::PathFollowOptions {
                    orient: Some(0.5),
                    start: 0.0,
                    end: 0.5,
                },
            )
            .unwrap(),
    ]);
    let ball = canvas.circle(0.2);
    canvas.play(vec![ball.animate().shift_by(2.0, 0.0).path_arc(1.0)]);
    let timeline = compiled_timeline(&canvas);
    let follows: Vec<_> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens:
                        gaanim_timeline::clip::PropertyLensSpec::PathFollow {
                            path,
                            orient,
                            reset_anchor,
                            ..
                        },
                    ..
                },
            ) => Some((clip.start, path.clone(), *orient, *reset_anchor)),
            _ => None,
        })
        .collect();
    assert_eq!(follows.len(), 2);
    let (_, route_part, orient, reset_anchor) =
        follows.iter().find(|entry| entry.0 == 0.0).unwrap();
    assert_eq!(*orient, Some(0.5));
    assert!(
        *reset_anchor,
        "move_along places the local origin on the route"
    );
    // Half of the 7-unit route ends 3.5 units along it.
    let end = gaanim_math::get_point_at_alpha(route_part, 1.0);
    assert!((end.x - 3.5).abs() < 1e-3 && end.y.abs() < 1e-3, "{end:?}");
    let (_, arc, orient, reset_anchor) = follows.iter().find(|entry| entry.0 > 0.0).unwrap();
    assert_eq!(*orient, None);
    assert!(!*reset_anchor, "path_arc keeps the authored pivot");
    let middle = gaanim_math::get_point_at_alpha(arc, 0.5);
    assert!(
        middle.y < -0.2,
        "arc should bow below the chord: {middle:?}"
    );
}

#[test]
fn move_along_travels_backwards_when_start_exceeds_end() {
    let mut canvas = SceneModel::new(640, 360);
    let route = canvas.polyline(&[(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)]);
    let dot = canvas.circle(0.2);
    canvas.play(vec![
        dot.animate()
            .move_along_with(
                &route,
                crate::anim::PathFollowOptions {
                    orient: None,
                    start: 5.5 / 7.0,
                    end: 2.0 / 7.0,
                },
            )
            .unwrap(),
    ]);
    let timeline = compiled_timeline(&canvas);
    let path = timeline
        .clips
        .values()
        .find_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::PathFollow { path, .. },
                    ..
                },
            ) => Some(path.clone()),
            _ => None,
        })
        .unwrap();
    // 5.5 units along the route is (4, 1.5); 2 units is (2, 0).
    let first = gaanim_math::get_point_at_alpha(&path, 0.0);
    let last = gaanim_math::get_point_at_alpha(&path, 1.0);
    assert!(
        (first.x - 4.0).abs() < 1e-3 && (first.y - 1.5).abs() < 1e-3,
        "{first:?}"
    );
    assert!(
        (last.x - 2.0).abs() < 1e-3 && last.y.abs() < 1e-3,
        "{last:?}"
    );
}

#[test]
fn move_along_keeps_the_declared_state_until_it_starts() {
    let mut canvas = SceneModel::new(640, 360);
    let route = canvas.polyline(&[(-6.0, -2.0), (0.0, 2.0), (6.0, -2.0)]);
    let ball = canvas.circle(0.2).move_to(-6.0, -2.0);
    canvas.wait(1.0);
    canvas.play(vec![ball.animate().grow_from_center().duration(0.3)]);
    canvas.play(vec![
        ball.animate().move_along(&route).unwrap().duration(1.0),
    ]);

    let mut world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

    let id = ObjectId::from_raw(ball.id.as_raw() - 1);
    for (time, translation, scale) in [
        (0.5, DVec3::new(-6.0, -2.0, 0.0), 0.0),
        (3.0, DVec3::new(6.0, -2.0, 0.0), 1.0),
        (0.5, DVec3::new(-6.0, -2.0, 0.0), 0.0),
    ] {
        timeline.seek(&mut world, time);
        let transform = *world
            .query::<(&MobjectId, &SpatialTransform)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1;
        assert!(
            transform.translation.distance(translation) < 1e-6,
            "{:?} at {time}",
            transform.translation
        );
        assert!(
            (transform.scale.x - scale).abs() < 1e-9,
            "{:?} at {time}",
            transform.scale
        );
    }
}

/// Compile `canvas` and return its timeline with a t = 0 keyframe.
fn compiled_world(canvas: &SceneModel) -> (World, Timeline) {
    let mut world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    (world, timeline)
}

/// A text animated to a point lands where `Text.move_to` would put it:
/// from `move_to(-4, 2)` to `(4, 2)` it slides along its baseline.
#[test]
fn text_move_animations_keep_the_baseline_placement() {
    let mut canvas = SceneModel::new(640, 360);
    let word = canvas
        .configured_text(
            "Blend",
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .at_text_default(-4.0, 2.0);
    canvas.wait(0.5);
    canvas.play(vec![word.animate().move_to_text(4.0, 2.0).duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.2);
    let before = transform_of(&mut world, &word).translation;
    timeline.seek(&mut world, 1.5);
    let after = transform_of(&mut world, &word).translation;
    assert!((after.y - before.y).abs() < 1e-9, "{before:?} -> {after:?}");
    assert!(
        (after.x - before.x - 8.0).abs() < 1e-9,
        "{before:?} -> {after:?}"
    );
}

/// A group's fill and stroke animations recolor every member, also in a
/// nested group, as its immediate `fill` and `stroke` do.
#[test]
fn group_paint_animations_reach_every_member() {
    let red = PenikoColor::from_rgb8(255, 0, 0);
    let mut canvas = SceneModel::new(640, 360);
    let square = canvas.rect(1.0, 1.0);
    let circle = canvas.circle(0.5);
    let dot = canvas.circle(0.2);
    let inner = canvas.group(&[&circle]);
    let outer = canvas.group(&[&square, &inner, &dot]);
    canvas.play(vec![outer.animate().fill(red).duration(1.0)]);
    canvas.play(vec![outer.animate().stroke(red, 0.1).duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 2.0);
    for member in [&square, &circle, &dot] {
        let entity = entity_of(&mut world, member);
        assert_eq!(
            world.get::<FillBrush>(entity).unwrap().0,
            Some(Brush::Solid(red))
        );
        assert_eq!(
            world.get::<StrokeBrush>(entity).unwrap().brush,
            Some(Brush::Solid(red))
        );
    }
}

/// Effects set after the first play are cuts at the cursor, as paints
/// are: before it the declared effects show, after it the new ones.
#[test]
fn effects_set_after_the_first_play_cut_at_the_cursor() {
    use gaanim_renderer::effects::{DropShadow, GaussianBlur, Glow};
    let mut canvas = SceneModel::new(640, 360);
    let glowing = canvas
        .circle(0.5)
        .glow(PenikoColor::WHITE, 0.3, 2.0)
        .blur(0.2);
    let plain = canvas.circle(0.5).move_to_default(2.0, 0.0);
    canvas.play(vec![plain.animate().shift_by(0.5, 0.0).duration(1.0)]);
    glowing.clone().no_effects();
    plain
        .clone()
        .shadow(PenikoColor::BLACK, DVec2::new(0.1, -0.1), 0.05);
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let (glowing, plain) = (
        entity_of(&mut world, &glowing),
        entity_of(&mut world, &plain),
    );
    timeline.seek(&mut world, 0.5);
    assert!(world.get::<Glow>(glowing).is_some());
    assert!(world.get::<GaussianBlur>(glowing).is_some());
    assert!(world.get::<DropShadow>(plain).is_none());
    timeline.seek(&mut world, 1.5);
    assert!(world.get::<Glow>(glowing).is_none());
    assert!(world.get::<GaussianBlur>(glowing).is_none());
    assert!(world.get::<DropShadow>(plain).is_some());
}

fn transform_of(world: &mut World, handle: &DrawableHandle) -> SpatialTransform {
    let id = ObjectId::from_raw(handle.id.as_raw() - 1);
    *world
        .query::<(&MobjectId, &SpatialTransform)>()
        .iter(world)
        .find(|(object, _)| object.0 == id)
        .unwrap()
        .1
}

/// `orient_3d` points the local axes where it is told, set or animated.
#[test]
fn orient_3d_points_the_local_axes_along_the_given_directions() {
    let mut canvas = SceneModel::new(640, 360);
    let laid = canvas
        .rect(2.0, 1.0)
        .orient_3d(DVec3::NEG_Z, DVec3::NEG_X)
        .unwrap();
    let turned = canvas.rect(2.0, 1.0);
    assert!(turned.clone().orient_3d(DVec3::X, DVec3::X).is_err());
    canvas.play(vec![
        turned
            .animate()
            .orient_3d(DVec3::Y, DVec3::NEG_X)
            .unwrap()
            .duration(1.0),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.0);
    let rotation = transform_of(&mut world, &laid).rotation;
    assert!((rotation * DVec3::X - DVec3::NEG_Z).length() < 1e-9);
    assert!((rotation * DVec3::Y - DVec3::NEG_X).length() < 1e-9);
    timeline.seek(&mut world, 1.5);
    let rotation = transform_of(&mut world, &turned).rotation;
    assert!((rotation * DVec3::X - DVec3::Y).length() < 1e-9);
    assert!((rotation * DVec3::Y - DVec3::NEG_X).length() < 1e-9);
}

#[test]
fn a_boolean_keeps_the_look_its_first_operand_had_when_it_was_made() {
    use gaanim_core::peniko::{Brush, Color};
    let mut canvas = SceneModel::new(640, 360);
    let red = Color::from_rgb8(255, 0, 0);
    let a = canvas.circle(1.0).fill(red);
    let b = canvas.square(1.5).move_to_default(0.5, 0.0);
    let union = canvas
        .boolean(
            &[&a, &b],
            crate::canvas::BooleanOperation::Union,
            false,
            0.0025,
            crate::canvas::BooleanRule::NonZero,
        )
        .unwrap();
    let _ = a
        .fill(Color::from_rgb8(0, 0, 255))
        .stroke(Color::WHITE, 0.1);
    canvas.wait(1.0);
    let (mut world, _) = compiled_world(&canvas);
    let id = ObjectId::from_raw(union.id.as_raw() - 1);
    let (fill, stroke) = world
        .query::<(&MobjectId, &FillBrush, &StrokeBrush)>()
        .iter(&world)
        .find(|(object, ..)| object.0 == id)
        .map(|(_, fill, stroke)| (fill.0.clone(), stroke.brush.clone()))
        .unwrap();
    assert_eq!(fill, Some(Brush::Solid(red)));
    assert_eq!(stroke, None, "{stroke:?}");
}

#[test]
fn squash_stretch_deforms_along_velocity_only_while_moving() {
    let mut canvas = SceneModel::new(640, 360);
    let ball = canvas
        .circle(0.5)
        .move_to(-3.0, 0.0)
        .squash_stretch(0.1, 1.5)
        .unwrap();
    assert!(canvas.circle(0.5).squash_stretch(-1.0, 1.5).is_err());
    assert!(canvas.circle(0.5).squash_stretch(0.1, 0.5).is_err());
    canvas.play(vec![
        ball.animate()
            .move_to(3.0, 0.0)
            .duration(1.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let id = ObjectId::from_raw(ball.id.as_raw() - 1);
    let mut deform_at = |time: f64| {
        timeline.seek(&mut world, time);
        world
            .query::<(&MobjectId, Option<&gaanim_scene::ShapeDeform>)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .map_or(kurbo::Affine::IDENTITY, |deform| deform.0)
    };
    // 6 units per second, so the stretch is capped at 1.5 along x.
    let moving = deform_at(0.5).as_coeffs();
    assert!((moving[0] - 1.5).abs() < 1e-9, "{moving:?}");
    assert!((moving[3] - 1.0 / 1.5).abs() < 1e-9, "{moving:?}");
    assert_eq!(deform_at(1.8), kurbo::Affine::IDENTITY);
    assert!((deform_at(0.5).as_coeffs()[0] - 1.5).abs() < 1e-9);
}

#[test]
fn settle_bounces_past_the_target_and_extends_the_clip() {
    let mut canvas = SceneModel::new(640, 360);
    let card = canvas.square(1.0).move_to(-1.0, 0.0);
    assert!(card.animate().settle(-0.1, 3.0, 6.0).is_err());
    assert!(card.animate().settle(0.1, 0.0, 6.0).is_err());
    assert!(card.animate().settle(0.1, 3.0, f64::NAN).is_err());
    canvas.play(vec![
        card.animate()
            .move_to(1.0, 0.0)
            .duration(0.5)
            .rate_func(gaanim_math::RateFunc::Linear)
            .settle(0.12, 3.0, 6.0)
            .unwrap(),
    ]);
    let after = canvas.circle(0.2);
    canvas.play(vec![after.fade_in(0.1)]);
    // 2 distances per second: amplitude 0.24 of the 2-unit move (0.48
    // units), settled below 0.1% of the distance; the
    // next play starts after the bounce.
    let (mut world, mut timeline) = compiled_world(&canvas);
    let tail = (0.24f64 / gaanim_math::RateFunc::SETTLE_EPSILON).ln() / 6.0;
    assert!(
        timeline.cached_duration >= 0.5 + tail + 0.1 - 1e-6,
        "{}",
        timeline.cached_duration
    );
    let mut x_at = |time: f64| {
        timeline.seek(&mut world, time);
        transform_of(&mut world, &card).translation.x
    };
    assert!((x_at(0.25) - 0.0).abs() < 1e-9);
    // A quarter period after arriving it has overshot the target.
    let peak = 0.48 * (-0.5f64).exp();
    assert!((x_at(0.5 + 1.0 / 12.0) - 1.0 - peak).abs() < 1e-3);
    assert!(x_at(0.75) < 1.0);
    assert!((x_at(0.5 + tail) - 1.0).abs() < 1e-6);
    // Seeking back reproduces the bounce.
    assert!((x_at(0.5 + 1.0 / 12.0) - 1.0 - peak).abs() < 1e-3);
}

#[test]
fn delayed_follow_reads_the_leader_in_the_past_at_any_seek() {
    let mut canvas = SceneModel::new(640, 360);
    let leader = canvas.circle(0.3).move_to(-2.0, 0.0);
    let dots: Vec<_> = (1..=3)
        .map(|index| {
            let dot = canvas.circle(0.1);
            dot.follow_endpoint_delayed(
                CanvasEndpoint::Entity(leader.id),
                DVec3::new(0.0, -0.5, 0.0),
                gaanim_animation::FollowOffsetSpace::World,
                0.25 * index as f64,
            )
            .unwrap()
        })
        .collect();
    assert!(
        dots[0]
            .follow_endpoint_delayed(
                CanvasEndpoint::Static(DVec3::ZERO),
                DVec3::ZERO,
                gaanim_animation::FollowOffsetSpace::World,
                0.1,
            )
            .is_err()
    );
    assert!(
        dots[0]
            .follow_endpoint_delayed(
                CanvasEndpoint::Entity(leader.id),
                DVec3::ZERO,
                gaanim_animation::FollowOffsetSpace::World,
                -0.1,
            )
            .is_err()
    );
    canvas.play(dots.iter().map(|dot| dot.fade_in(0.01)).collect());
    canvas.play(vec![
        leader
            .animate()
            .move_to(2.0, 0.0)
            .duration(1.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let start = 0.01;
    let leader_x = |time: f64| -2.0 + 4.0 * (time - start).clamp(0.0, 1.0);
    // Out of order, so no state can carry over between seeks.
    for time in [1.3, 0.2, 0.9, 0.5, 2.0, 0.0, 0.6, 1.3] {
        timeline.seek(&mut world, time);
        for (index, dot) in dots.iter().enumerate() {
            let delay = 0.25 * (index + 1) as f64;
            let position = transform_of(&mut world, dot).translation;
            let expected = leader_x((time - delay).max(0.0));
            assert!(
                (position.x - expected).abs() < 1e-9 && (position.y + 0.5).abs() < 1e-9,
                "dot {index} at {time}: {position:?}, expected x {expected}"
            );
        }
    }
}

#[test]
fn particles_compile_to_layers_bursts_and_anchor_trails() {
    use crate::canvas::{Emitter, EmitterShape, ParticleColors, ParticleOptions};
    use gaanim_animation::{PARTICLE_FADE_LEVELS, ParticleEmitter};
    let mut canvas = SceneModel::new(640, 360);
    let logo = canvas.circle(0.2);
    let emitter = Emitter::new(EmitterShape::Point)
        .unwrap()
        .at_drawable(&logo, 0.0, 0.5)
        .unwrap();
    let options = ParticleOptions {
        rate: 10.0,
        colors: ParticleColors::Palette(vec![PenikoColor::WHITE, PenikoColor::BLACK]),
        seed: 9,
        ..ParticleOptions::default()
    };
    let sparks = canvas.particles(&emitter, options).unwrap();
    assert!(canvas.circle(0.1).burst(5).is_err());
    assert!(canvas.circle(0.1).animate().burst(5).is_err());
    assert!(sparks.clone().burst(0).is_err());
    canvas.play(vec![
        logo.animate()
            .move_to(4.0, 0.0)
            .duration(2.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    // A burst lasts the longest life (1.2 s) unless given a duration.
    canvas.play(vec![sparks.animate().burst(20).unwrap()]);
    sparks.clone().burst(3).unwrap();
    canvas.wait(0.5);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let emitter_of = |world: &mut World| {
        world
            .query::<&ParticleEmitter>()
            .single(world)
            .unwrap()
            .clone()
    };
    let compiled = emitter_of(&mut world);
    assert_eq!(compiled.layers.len(), 2 * PARTICLE_FADE_LEVELS);
    assert_eq!(compiled.system.colors, 2);
    assert_eq!(compiled.system.start, 0.0);
    let bursts = &compiled.system.bursts;
    assert_eq!(bursts.len(), 2);
    assert!((bursts[0].0 - 2.0).abs() < 1e-9 && bursts[0].1 == 20);
    assert!((bursts[1].0 - 3.2).abs() < 1e-9 && bursts[1].1 == 3);
    assert!(compiled.anchor.is_some());

    // The trail follows the anchor, so particles leave from where it was.
    timeline.seek(&mut world, 1.0);
    let at_one = emitter_of(&mut world);
    assert!(
        (at_one.trail.at(0.5).x - 1.0).abs() < 1e-6,
        "{:?}",
        at_one.trail
    );
    assert!(
        (at_one.trail.at(1.0).x - 2.0).abs() < 1e-6,
        "{:?}",
        at_one.trail
    );
    assert!((at_one.origin_at(1.0).y - 0.5).abs() < 1e-6);
    let outlines = at_one.outlines(1.0);
    assert!(outlines.iter().any(|path| !path.elements().is_empty()));
    // A seek elsewhere and back reproduces the frame exactly.
    timeline.seek(&mut world, 3.0);
    timeline.seek(&mut world, 1.0);
    assert_eq!(emitter_of(&mut world).outlines(1.0), outlines);
}

#[test]
fn repeated_bursts_fire_once_per_cycle() {
    use crate::canvas::{Emitter, EmitterShape, ParticleOptions};
    let mut canvas = SceneModel::new(640, 360);
    let emitter = Emitter::new(EmitterShape::Point).unwrap();
    let options = ParticleOptions {
        rate: 0.0,
        ..ParticleOptions::default()
    };
    let sparks = canvas.particles(&emitter, options.clone()).unwrap();
    let yoyo = canvas.particles(&emitter, options).unwrap();
    canvas.play(vec![
        sparks.animate().burst(40).unwrap().duration(0.5).repeat(
            3,
            gaanim_math::RepeatMode::Cycle,
            0.0,
        ),
        yoyo.animate().burst(10).unwrap().duration(0.4).repeat(
            3,
            gaanim_math::RepeatMode::PingPong,
            0.5,
        ),
    ]);
    canvas.wait(0.5);
    let (mut world, _) = compiled_world(&canvas);
    let mut bursts: Vec<Vec<(f64, u32)>> = world
        .query::<&gaanim_animation::ParticleEmitter>()
        .iter(&world)
        .map(|emitter| emitter.system.bursts.clone())
        .collect();
    bursts.sort_by_key(|bursts| bursts[0].1);
    assert_eq!(bursts.len(), 2);
    let close = |bursts: &[(f64, u32)], expected: &[(f64, u32)]| {
        bursts.len() == expected.len()
            && bursts
                .iter()
                .zip(expected)
                .all(|(a, b)| (a.0 - b.0).abs() < 1e-9 && a.1 == b.1)
    };
    // Yoyo: 0.4 s cycles separated by 0.5 s gaps.
    assert!(
        close(&bursts[0], &[(0.0, 10), (0.9, 10), (1.8, 10)]),
        "{bursts:?}"
    );
    assert!(
        close(&bursts[1], &[(0.0, 40), (0.5, 40), (1.0, 40)]),
        "{bursts:?}"
    );
}

#[test]
fn delayed_follow_chains_through_followers_at_any_seek() {
    use gaanim_animation::FollowOffsetSpace;
    let mut canvas = SceneModel::new(640, 360);
    let leader = canvas.circle(0.3).move_to(-2.0, 0.0);
    let follow = |follower: &DrawableHandle, source: &DrawableHandle, offset, delay| {
        follower
            .follow_endpoint_delayed(
                CanvasEndpoint::Entity(source.id),
                offset,
                FollowOffsetSpace::World,
                delay,
            )
            .unwrap()
    };
    // A chain of delayed followers of delayed followers.
    let mut links: Vec<DrawableHandle> = Vec::new();
    for index in 0..3 {
        let link = canvas.circle(0.1);
        let (source, offset) = match links.last() {
            Some(previous) => (previous.clone(), DVec3::ZERO),
            None => (leader.clone(), DVec3::new(0.0, -0.5, 0.0)),
        };
        links.push(follow(&link, &source, offset, 0.2));
        assert_eq!(links.len(), index + 1);
    }
    // Delayed followers of an undelayed `follow` and of a `follow_to`.
    let plain = canvas.circle(0.1);
    follow(&plain, &leader, DVec3::new(0.0, 0.5, 0.0), 0.0);
    let after_plain = canvas.circle(0.1);
    follow(&after_plain, &plain, DVec3::ZERO, 0.3);
    let bound = canvas.circle(0.1);
    bound.follow_to(&leader, 0.0, 1.0);
    let after_bound = canvas.circle(0.1);
    follow(&after_bound, &bound, DVec3::ZERO, 0.3);
    // A cycle stays finite and deterministic.
    let (a, b) = (canvas.circle(0.1), canvas.circle(0.1));
    follow(&a, &b, DVec3::ZERO, 0.1);
    follow(&b, &a, DVec3::ZERO, 0.1);

    let entered: Vec<DrawableHandle> = links
        .iter()
        .cloned()
        .chain([plain, after_plain.clone(), bound, after_bound.clone(), a, b])
        .collect();
    canvas.play(
        entered
            .iter()
            .map(|drawable| drawable.fade_in(0.01))
            .collect(),
    );
    canvas.play(vec![
        leader
            .animate()
            .move_to(2.0, 0.0)
            .duration(1.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let start = 0.01;
    let leader_x = |time: f64| -2.0 + 4.0 * (time - start).clamp(0.0, 1.0);
    let expected =
        |time: f64, delay: f64, y: f64| DVec3::new(leader_x((time - delay).max(0.0)), y, 0.0);
    for time in [1.5, 0.3, 0.95, 0.1, 2.0, 0.0, 0.7, 1.5] {
        timeline.seek(&mut world, time);
        let mut checks: Vec<(&DrawableHandle, DVec3)> = links
            .iter()
            .enumerate()
            .map(|(index, link)| (link, expected(time, 0.2 * (index + 1) as f64, -0.5)))
            .collect();
        checks.push((&after_plain, expected(time, 0.3, 0.5)));
        checks.push((&after_bound, expected(time, 0.3, 1.0)));
        for (drawable, expected) in checks {
            let position = transform_of(&mut world, drawable).translation;
            assert!(
                (position - expected).length() < 1e-9,
                "at {time}: {position:?}, expected {expected:?}"
            );
        }
    }
}

/// Leader x in the echo-of-follower tests: -2 to 2 over 1 s, linearly,
/// starting at 0.01 s (after the subject's entry).
fn echo_leader_x(time: f64) -> f64 {
    -2.0 + 4.0 * (time - 0.01).clamp(0.0, 1.0)
}

/// Builds a moving leader and the drawable `make` returns, gives that
/// drawable a 2-copy echo 0.1 s apart, and checks at out-of-order seeks
/// that the copy `lag` seconds back sits at `position_at(t - lag)`, the
/// drawable's own position then.
fn assert_echo_copies_trail(
    make: impl FnOnce(&mut SceneModel, &DrawableHandle) -> DrawableHandle,
    position_at: impl Fn(f64) -> DVec3,
) {
    let mut canvas = SceneModel::new(640, 360);
    let leader = canvas.circle(0.3).move_to(-2.0, 0.0);
    let subject = make(&mut canvas, &leader).echo(Some(
        super::super::types::EchoSpec::new(2, 0.1, 0.5).unwrap(),
    ));
    canvas.play(vec![subject.fade_in(0.01)]);
    canvas.play(vec![
        leader
            .animate()
            .move_to(2.0, 0.0)
            .duration(1.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let source = ObjectId::from_raw(subject.id.as_raw() - 1);
    let mut copies: Vec<(Entity, f64)> = world
        .query::<(Entity, &gaanim_animation::EchoGhost)>()
        .iter(&world)
        .filter(|(_, echo)| echo.source == source)
        .map(|(entity, echo)| (entity, echo.lag))
        .collect();
    copies.sort_by(|left, right| left.1.total_cmp(&right.1));
    assert_eq!(copies.len(), 2, "expected two echo copies of the subject");
    // Every copy time t - lag is inside the timeline and after the entry.
    for time in [1.5, 0.35, 0.95, 0.25, 2.0, 0.6, 1.5] {
        timeline.seek(&mut world, time);
        for &(copy, lag) in &copies {
            let expected = position_at(time - lag);
            // A copy that caught up with the resting drawable is hidden.
            if (expected - position_at(time)).length() < 1e-9 {
                assert!(
                    world.get::<gaanim_scene::Visible>(copy).is_none(),
                    "copy {lag} shown at rest at {time}"
                );
                continue;
            }
            assert!(
                world.get::<gaanim_scene::Visible>(copy).is_some(),
                "copy {lag} hidden at {time}"
            );
            let actual = world.get::<SpatialTransform>(copy).unwrap().translation;
            assert!(
                (actual - expected).length() < 1e-9,
                "copy {lag} at {time}: {actual:?}, expected {expected:?}"
            );
        }
    }
}

#[test]
fn echo_of_follower_control_echo_of_the_animated_leader() {
    assert_echo_copies_trail(
        |_: &mut SceneModel, leader: &DrawableHandle| leader.clone(),
        |time| DVec3::new(echo_leader_x(time), 0.0, 0.0),
    );
}

#[test]
fn echo_of_follower_plain_follow_with_offset() {
    assert_echo_copies_trail(
        |canvas: &mut SceneModel, leader: &DrawableHandle| {
            canvas.circle(0.1).follow_endpoint(
                CanvasEndpoint::Entity(leader.id),
                DVec3::new(0.0, 0.5, 0.0),
                gaanim_animation::FollowOffsetSpace::World,
            )
        },
        |time| DVec3::new(echo_leader_x(time), 0.5, 0.0),
    );
}

#[test]
fn echo_of_follower_delayed_follow() {
    assert_echo_copies_trail(
        |canvas: &mut SceneModel, leader: &DrawableHandle| {
            canvas
                .circle(0.1)
                .follow_endpoint_delayed(
                    CanvasEndpoint::Entity(leader.id),
                    DVec3::new(0.0, -0.5, 0.0),
                    gaanim_animation::FollowOffsetSpace::World,
                    0.3,
                )
                .unwrap()
        },
        // Holds the leader's start until 0.3 s have passed.
        |time| DVec3::new(echo_leader_x((time - 0.3).max(0.0)), -0.5, 0.0),
    );
}

#[test]
fn echo_of_follower_follow_to() {
    assert_echo_copies_trail(
        |canvas: &mut SceneModel, leader: &DrawableHandle| {
            let follower = canvas.circle(0.1);
            follower.follow_to(leader, 0.0, 1.0);
            follower
        },
        |time| DVec3::new(echo_leader_x(time), 1.0, 0.0),
    );
}

#[test]
fn echo_of_follower_attach_to() {
    assert_echo_copies_trail(
        |canvas: &mut SceneModel, leader: &DrawableHandle| {
            let follower = canvas.circle(0.1);
            follower.attach_to(leader);
            follower
        },
        |time| DVec3::new(echo_leader_x(time), 0.0, 0.0),
    );
}

#[test]
fn squash_stretch_of_a_follower_deforms_along_the_followed_motion() {
    // (make the follower, its delay behind the leader)
    type MakeFollower = fn(&mut SceneModel, &DrawableHandle) -> DrawableHandle;
    let followers: [(MakeFollower, f64); 2] = [
        (
            |canvas: &mut SceneModel, leader: &DrawableHandle| {
                let follower = canvas.circle(0.5).squash_stretch(0.1, 1.5).unwrap();
                follower.follow_to(leader, 0.0, 1.0);
                follower
            },
            0.0,
        ),
        (
            |canvas: &mut SceneModel, leader: &DrawableHandle| {
                canvas
                    .circle(0.5)
                    .squash_stretch(0.1, 1.5)
                    .unwrap()
                    .follow_endpoint_delayed(
                        CanvasEndpoint::Entity(leader.id),
                        DVec3::ZERO,
                        gaanim_animation::FollowOffsetSpace::World,
                        0.3,
                    )
                    .unwrap()
            },
            0.3,
        ),
    ];
    for (make, delay) in followers {
        let mut canvas = SceneModel::new(640, 360);
        let leader = canvas.circle(0.3).move_to(-3.0, 0.0);
        let follower = make(&mut canvas, &leader);
        canvas.play(vec![
            follower.fade_in(0.01),
            leader
                .animate()
                .move_to(3.0, 0.0)
                .duration(1.0)
                .rate_func(gaanim_math::RateFunc::Linear),
        ]);
        canvas.wait(1.0);
        let (mut world, mut timeline) = compiled_world(&canvas);
        let id = ObjectId::from_raw(follower.id.as_raw() - 1);
        let mut deform_at = |time: f64| {
            timeline.seek(&mut world, time);
            world
                .query::<(&MobjectId, Option<&gaanim_scene::ShapeDeform>)>()
                .iter(&world)
                .find(|(object, _)| object.0 == id)
                .unwrap()
                .1
                .map_or(kurbo::Affine::IDENTITY, |deform| deform.0)
        };
        // 6 units per second along x, so the stretch is capped at 1.5.
        for time in [0.5 + delay, 1.8, 0.4 + delay] {
            let coeffs = deform_at(time).as_coeffs();
            if time > 1.5 {
                assert_eq!(coeffs, kurbo::Affine::IDENTITY.as_coeffs(), "delay {delay}");
            } else {
                assert!(
                    (coeffs[0] - 1.5).abs() < 1e-9 && (coeffs[3] - 1.0 / 1.5).abs() < 1e-9,
                    "delay {delay} at {time}: {coeffs:?}"
                );
            }
        }
    }
}

#[test]
fn particles_anchored_on_a_follower_trail_the_followed_motion() {
    use crate::canvas::{Emitter, EmitterShape, ParticleOptions};
    use gaanim_animation::ParticleEmitter;
    let mut canvas = SceneModel::new(640, 360);
    let leader = canvas.circle(0.3);
    let logo = canvas.circle(0.2);
    logo.follow_to(&leader, 0.0, 0.0);
    let emitter = Emitter::new(EmitterShape::Point)
        .unwrap()
        .at_drawable(&logo, 0.0, 0.5)
        .unwrap();
    let options = ParticleOptions {
        rate: 10.0,
        seed: 9,
        ..ParticleOptions::default()
    };
    let _sparks = canvas.particles(&emitter, options).unwrap();
    canvas.play(vec![
        logo.fade_in(0.01),
        leader
            .animate()
            .move_to(4.0, 0.0)
            .duration(2.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    canvas.wait(0.5);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let emitter_of = |world: &mut World| {
        world
            .query::<&ParticleEmitter>()
            .single(world)
            .unwrap()
            .clone()
    };
    // Out of order: the trail is rebuilt from the leader at every seek.
    for time in [1.0, 2.3, 0.4, 1.0] {
        timeline.seek(&mut world, time);
        let trail = emitter_of(&mut world).trail;
        for sample in [time * 0.5, time] {
            let expected = 2.0 * sample.min(2.0);
            assert!(
                (trail.at(sample).x - expected).abs() < 1e-6,
                "at {time}, sample {sample}: {:?}",
                trail
            );
        }
    }
}

#[test]
fn repeater_count_shows_copies_in_order_and_seeks_back() {
    let mut canvas = SceneModel::new(640, 360);
    let dot = canvas.circle(0.1);
    let step = crate::canvas::RepeatStep {
        offset: DVec2::new(0.5, 0.0),
        ..crate::canvas::RepeatStep::default()
    };
    let row = canvas.repeat(&dot, 4, step).unwrap().count(1.0).unwrap();
    assert!(row.clone().count(4.5).is_err());
    assert!(canvas.circle(0.1).count(1.0).is_err());
    assert!(canvas.circle(0.1).animate().count(1.0).is_err());
    canvas.play(vec![
        row.animate()
            .count(4.0)
            .unwrap()
            .duration(1.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    canvas.wait(0.5);
    row.clone().count(2.0).unwrap();
    canvas.wait(0.5);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let mut presences = |time: f64| {
        timeline.seek(&mut world, time);
        // Copies are declared in order, so their ids are increasing.
        let mut copies: Vec<(u64, f32)> = world
            .query::<(&gaanim_scene::Presence, &MobjectId)>()
            .iter(&world)
            .map(|(presence, object)| (object.0.as_raw(), presence.0))
            .collect();
        copies.sort_by_key(|copy| copy.0);
        copies
            .into_iter()
            .map(|(_, presence)| presence)
            .collect::<Vec<_>>()
    };
    assert_eq!(presences(0.5), [1.0, 1.0, 0.5, 0.0]);
    assert_eq!(presences(1.2), [1.0; 4]);
    assert_eq!(presences(1.8), [1.0, 1.0, 0.0, 0.0]);
    assert_eq!(presences(0.0), [1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn connect_validates_its_points() {
    let mut canvas = SceneModel::new(640, 360);
    let a = canvas.circle(0.1);
    let b = canvas.circle(0.1).move_to(1.0, 0.0);
    let mode = gaanim_renderer::effects::ConnectMode::Range;
    assert!(canvas.connect(&[&a], 1.0, mode, 1, true).is_err());
    assert!(canvas.connect(&[&a, &b], 0.0, mode, 1, true).is_err());
    assert!(canvas.connect(&[&a, &b], 1.0, mode, 0, true).is_err());
    let group = canvas.group(&[&a, &b]);
    let links = canvas.connect(&[&group], 2.0, mode, 1, true).unwrap();
    assert!(matches!(
        &links.spec.lock().unwrap().kind,
        SpawnKind::Connect { sources, fade: true, .. } if sources.len() == 2
    ));
    let (world, _) = compiled_world(&canvas);
    let mut world = world;
    let layers = world
        .query::<&gaanim_renderer::effects::ConnectBinding>()
        .iter(&world)
        .count();
    assert_eq!(layers, CONNECT_FADE_LAYERS);
}

#[test]
fn points_move_each_vertex_straight_and_seek_exactly() {
    let mut canvas = SceneModel::new(640, 360);
    let square = canvas.polygon(vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
    let target = vec![(1.0, -1.0), (3.0, 1.0), (1.0, 3.0), (-1.0, 1.0)];
    assert!(square.animate().points(Vec::new()).is_err());
    assert!(square.animate().points(vec![(f64::NAN, 0.0); 4]).is_err());
    assert!(
        square
            .animate()
            .points_along(
                target.clone(),
                crate::canvas::VertexPath::Rigid {
                    about: Some(Point::new(f64::INFINITY, 0.0))
                }
            )
            .is_err()
    );
    assert!(canvas.circle(1.0).animate().points(target.clone()).is_err());
    canvas.play(vec![
        square
            .animate()
            .points(target)
            .unwrap()
            .duration(1.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    canvas.wait(0.5);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let id = ObjectId::from_raw(square.id.as_raw() - 1);
    let mut vertices_at = |time: f64| {
        timeline.seek(&mut world, time);
        let path = world
            .query::<(&MobjectId, &gaanim_scene::Path2D)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .0
            .clone();
        path.elements()
            .iter()
            .filter_map(|element| match element {
                gaanim_core::kurbo::PathEl::MoveTo(point)
                | gaanim_core::kurbo::PathEl::LineTo(point) => Some((point.x, point.y)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        vertices_at(1.2),
        [(1.0, -1.0), (3.0, 1.0), (1.0, 3.0), (-1.0, 1.0)]
    );
    assert_eq!(
        vertices_at(0.5),
        [(0.5, -0.5), (2.5, 0.5), (1.5, 2.5), (-0.5, 1.5)]
    );
    assert_eq!(
        vertices_at(0.0),
        [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]
    );
}

#[test]
fn points_change_the_vertex_count_and_turn_rigidly() {
    use crate::canvas::VertexPath;
    let mut canvas = SceneModel::new(640, 360);
    let corners = [(-1.0, -0.6), (1.0, -0.6), (0.3, 1.0)];
    let triangle = canvas.polygon(corners.to_vec());
    let turned: Vec<(f64, f64)> = corners.iter().map(|&(x, y)| (-x, -y)).collect();
    canvas.play(vec![
        triangle
            .animate()
            .points_along(turned.clone(), VertexPath::Rigid { about: None })
            .unwrap()
            .duration(1.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    let square = vec![(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    canvas.play(vec![
        triangle
            .animate()
            .points(square.clone())
            .unwrap()
            .duration(1.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let id = ObjectId::from_raw(triangle.id.as_raw() - 1);
    let mut vertices_at = |time: f64| {
        timeline.seek(&mut world, time);
        let path = world
            .query::<(&MobjectId, &gaanim_scene::Path2D)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .0
            .clone();
        path.elements()
            .iter()
            .filter_map(|element| match element {
                gaanim_core::kurbo::PathEl::MoveTo(point)
                | gaanim_core::kurbo::PathEl::LineTo(point) => Some(*point),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // Halfway through the half turn the triangle is turned a quarter,
    // not collapsed: its sides keep their length.
    let half = vertices_at(0.5);
    assert_eq!(half.len(), 3);
    assert!(((half[1] - half[0]).hypot() - 2.0).abs() < 1e-9);
    let end = vertices_at(2.0);
    let expected: Vec<Point> = square.iter().map(|&point| point.into()).collect();
    assert_eq!(end, expected);
    // The first corner splits in two on the way to the square.
    let opening = vertices_at(1.5);
    assert_eq!(opening.len(), 4);
    assert!((opening[0] - Point::new(0.0, -0.2)).hypot() < 1e-9);
    assert!((opening[1] - Point::new(1.0, -0.2)).hypot() < 1e-9);
    let start = vertices_at(0.0);
    let declared: Vec<Point> = corners.iter().map(|&point| point.into()).collect();
    assert_eq!(start, declared);
}

#[test]
fn a_closed_polyline_takes_new_points_and_stays_closed() {
    let mut canvas = SceneModel::new(640, 360);
    let outline = canvas.closed_polyline(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
    let outline = outline
        .points(vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0)])
        .unwrap();
    canvas.play(vec![
        outline
            .animate()
            .points(vec![(0.0, 0.0), (3.0, 0.0), (3.0, 3.0)])
            .unwrap()
            .duration(1.0),
    ]);
    let reactive = canvas.reactive_polyline(vec![(0.0.into(), 0.0.into()); 3], true);
    assert!(reactive.points(vec![(0.0, 0.0); 3]).is_err());
    let (mut world, mut timeline) = compiled_world(&canvas);
    let id = ObjectId::from_raw(outline.id.as_raw() - 1);
    let mut outline_at = |time: f64| {
        timeline.seek(&mut world, time);
        world
            .query::<(&MobjectId, &gaanim_scene::Path2D)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .0
            .elements()
            .to_vec()
    };
    use gaanim_core::kurbo::PathEl::{ClosePath, LineTo, MoveTo};
    assert_eq!(
        outline_at(0.0),
        [
            MoveTo(Point::new(0.0, 0.0)),
            LineTo(Point::new(2.0, 0.0)),
            LineTo(Point::new(2.0, 2.0)),
            ClosePath
        ]
    );
    assert_eq!(
        outline_at(1.0),
        [
            MoveTo(Point::new(0.0, 0.0)),
            LineTo(Point::new(3.0, 0.0)),
            LineTo(Point::new(3.0, 3.0)),
            ClosePath
        ]
    );
}

#[test]
fn launched_animations_run_under_later_plays() {
    let mut canvas = SceneModel::new(640, 360);
    let wheel = canvas.circle(1.0);
    let label = canvas.circle(0.5).move_to(3.0, 0.0);
    canvas
        .launch_composition_configured(
            super::super::canvas_impl::Composition::leaf(
                wheel
                    .animate()
                    .rotate_by(4.0)
                    .duration(4.0)
                    .rate_func(gaanim_math::RateFunc::Linear),
            ),
            None,
            None,
        )
        .unwrap();
    canvas.play(vec![label.animate().move_to(3.0, 2.0).duration(1.0)]);
    canvas.wait(3.0);
    assert!((canvas.current_time() - 4.0).abs() < 1e-9);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let angle = |world: &mut World, timeline: &mut Timeline, time: f64| {
        timeline.seek(world, time);
        transform_of(world, &wheel)
            .rotation
            .to_euler(gaanim_core::glam::EulerRot::XYZ)
            .2
    };
    assert!((angle(&mut world, &mut timeline, 2.0) - 2.0).abs() < 1e-6);
    assert!((angle(&mut world, &mut timeline, 0.5) - 0.5).abs() < 1e-6);
    let label_y = |world: &mut World, timeline: &mut Timeline, time: f64| {
        timeline.seek(world, time);
        transform_of(world, &label).translation.y
    };
    // The play after the launch starts at the same instant.
    assert!((label_y(&mut world, &mut timeline, 1.0) - 2.0).abs() < 1e-6);
}

#[test]
fn points_setter_declares_the_shape_then_cuts_at_the_cursor() {
    let mut canvas = SceneModel::new(640, 360);
    let shape = canvas.polygon(vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
    assert!(shape.clone().points(Vec::new()).is_err());
    assert!(canvas.circle(1.0).points(vec![(0.0, 0.0)]).is_err());
    let declared = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    let shape = shape.points(declared.clone()).unwrap();
    canvas.play(vec![shape.animate().shift_by(0.0, 0.0).duration(1.0)]);
    let cut = vec![(0.0, 0.0), (3.0, 0.0), (3.0, 3.0), (0.0, 3.0)];
    let _ = shape.clone().points(cut.clone()).unwrap();
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let id = ObjectId::from_raw(shape.id.as_raw() - 1);
    let mut vertices_at = |time: f64| {
        timeline.seek(&mut world, time);
        let path = world
            .query::<(&MobjectId, &gaanim_scene::Path2D)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .0
            .clone();
        path.elements()
            .iter()
            .filter_map(|element| match element {
                gaanim_core::kurbo::PathEl::MoveTo(point)
                | gaanim_core::kurbo::PathEl::LineTo(point) => Some((point.x, point.y)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(vertices_at(0.5), declared);
    assert_eq!(vertices_at(1.5), cut);
    assert_eq!(vertices_at(0.2), declared);
}

#[test]
fn held_echo_copies_freeze_when_the_drawable_stops() {
    let mut canvas = SceneModel::new(640, 360);
    let ball = canvas.circle(0.5).move_to(-3.0, 0.0).echo(Some(
        super::super::types::EchoSpec::new(2, 0.25, 0.5)
            .unwrap()
            .with_hold(true),
    ));
    canvas.play(vec![
        ball.animate()
            .move_to(3.0, 0.0)
            .duration(1.0)
            .rate_func(gaanim_math::RateFunc::Linear),
    ]);
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let mut copies: Vec<(Entity, u32)> = world
        .query::<(Entity, &gaanim_animation::EchoGhost)>()
        .iter(&world)
        .map(|(entity, echo)| (entity, echo.rank))
        .collect();
    copies.sort_by_key(|(_, rank)| *rank);
    let copy_x = |world: &mut World, timeline: &mut Timeline, time: f64| {
        timeline.seek(world, time);
        copies
            .iter()
            .map(|(entity, _)| {
                world
                    .get::<SpatialTransform>(*entity)
                    .unwrap()
                    .translation
                    .x
            })
            .collect::<Vec<_>>()
    };
    // Moving: like a plain echo, 0.25 s and 0.5 s behind (6 units per s).
    let moving = copy_x(&mut world, &mut timeline, 0.75);
    assert!(
        (moving[0] - 0.0).abs() < 1e-9 && (moving[1] + 1.5).abs() < 1e-9,
        "{moving:?}"
    );
    // Stopped at 1.0 s: the copies stay where they were instead of
    // catching up with the ball at x = 3.
    for time in [1.5, 1.99] {
        let held = copy_x(&mut world, &mut timeline, time);
        assert!(
            (held[0] - 1.5).abs() < 1e-9 && (held[1] - 0.0).abs() < 1e-9,
            "{held:?}"
        );
    }
}

#[test]
fn echo_with_a_window_records_only_inside_it() {
    let mut canvas = SceneModel::new(640, 360);
    let ball = canvas.circle(0.5).move_to(-3.0, 0.0).echo(Some(
        super::super::types::EchoSpec::new(2, 0.1, 0.5)
            .unwrap()
            .with_window(Some(1.0), Some(1.5))
            .unwrap(),
    ));
    canvas.play(vec![ball.animate().move_to(0.0, 0.0).duration(1.0)]);
    canvas.play(vec![ball.animate().move_to(3.0, 0.0).duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let mut copies: Vec<(Entity, f64)> = world
        .query::<(Entity, &gaanim_animation::EchoGhost)>()
        .iter(&world)
        .map(|(entity, echo)| (entity, echo.lag))
        .collect();
    copies.sort_by(|left, right| left.1.total_cmp(&right.1));
    let shown = |world: &World, copy: Entity| world.get::<gaanim_scene::Visible>(copy).is_some();
    // Moving before the window: no trail.
    timeline.seek(&mut world, 0.5);
    assert!(copies.iter().all(|&(copy, _)| !shown(&world, copy)));
    // Inside it, the copies whose delayed time is inside it show.
    timeline.seek(&mut world, 1.15);
    assert!(shown(&world, copies[0].0), "1.05 is recorded");
    assert!(!shown(&world, copies[1].0), "0.95 is before the window");
    timeline.seek(&mut world, 1.4);
    assert!(copies.iter().all(|&(copy, _)| shown(&world, copy)));
    // After it the trail drains: 1.55 - 0.1 is still inside.
    timeline.seek(&mut world, 1.55);
    assert!(shown(&world, copies[0].0) && shown(&world, copies[1].0));
    timeline.seek(&mut world, 1.8);
    assert!(copies.iter().all(|&(copy, _)| !shown(&world, copy)));
    assert!(
        super::super::types::EchoSpec::new(2, 0.1, 0.5)
            .unwrap()
            .with_window(Some(2.0), Some(1.0))
            .is_err()
    );
}

#[test]
fn echo_copies_trail_their_drawable_through_seeks() {
    let mut canvas = SceneModel::new(640, 360);
    let ball = canvas.circle(0.5).move_to(-3.0, 0.0).echo(Some(
        super::super::types::EchoSpec::new(3, 0.1, 0.5).unwrap(),
    ));
    canvas.play(vec![ball.animate().move_to(3.0, 0.0).duration(1.0)]);
    canvas.wait(0.5);
    let (mut world, mut timeline) = compiled_world(&canvas);

    let mut copies: Vec<(Entity, gaanim_animation::EchoGhost)> = world
        .query::<(Entity, &gaanim_animation::EchoGhost)>()
        .iter(&world)
        .map(|(entity, echo)| (entity, echo.clone()))
        .collect();
    copies.sort_by_key(|(_, echo)| echo.rank);
    let ranks: Vec<_> = copies.iter().map(|(_, echo)| echo.rank).collect();
    assert_eq!(ranks, [1, 2, 3]);
    for ((_, echo), (lag, opacity)) in copies.iter().zip([(0.1, 0.5), (0.2, 0.25), (0.3, 0.125)]) {
        assert!((echo.lag - lag).abs() < 1e-12);
        assert!((echo.opacity - opacity).abs() < 1e-6);
    }

    for time in [0.6, 0.25, 1.4, 0.05] {
        let expected: Vec<Option<f64>> = copies
            .iter()
            .map(|(_, echo)| {
                let earlier = time - echo.lag;
                (earlier >= 0.0).then(|| {
                    timeline.seek(&mut world, earlier);
                    transform_of(&mut world, &ball).translation.x
                })
            })
            .collect();
        timeline.seek(&mut world, time);
        let now = transform_of(&mut world, &ball).translation.x;
        for ((copy, echo), expected) in copies.iter().zip(expected) {
            let visible = world.get::<gaanim_scene::Visible>(*copy).is_some();
            match expected {
                None => assert!(!visible, "copy {} at {time}", echo.rank),
                // A copy that caught up with the resting ball is hidden.
                Some(x) if (x - now).abs() < 1e-9 => {
                    assert!(!visible, "copy {} at rest at {time}", echo.rank);
                }
                Some(x) => {
                    assert!(visible, "copy {} at {time}", echo.rank);
                    let actual = world.get::<SpatialTransform>(*copy).unwrap().translation.x;
                    assert!(
                        (actual - x).abs() < 1e-9,
                        "copy {} at {time}: {actual} vs {x}",
                        echo.rank
                    );
                }
            }
        }
    }
}

#[test]
fn skew_shears_about_the_pivot_and_seeks_exactly() {
    let mut canvas = SceneModel::new(640, 360);
    let wall = canvas
        .rect(2.0, 2.0)
        .move_to(0.0, 0.0)
        .with_pivot(0.0, -1.0);
    canvas.play(vec![wall.animate().skew_to(0.5, 0.0).duration(1.0)]);
    canvas.wait(1.0);
    let wall = wall.skew_to(0.0, 0.0);
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);

    // The base stays on the pivot; the top leans by skew × height (2).
    for (time, top_x) in [(0.5, 0.5), (1.5, 1.0), (2.5, 0.0), (0.5, 0.5)] {
        timeline.seek(&mut world, time);
        let affine = transform_of(&mut world, &wall).to_affine_2d();
        let base = affine * Point::new(0.0, -1.0);
        let top = affine * Point::new(0.0, 1.0);
        assert!(
            base.distance(Point::new(0.0, -1.0)) < 1e-6,
            "{base:?} at {time}"
        );
        assert!(
            top.distance(Point::new(top_x, 1.0)) < 1e-6,
            "{top:?} at {time}"
        );
    }
}

#[test]
fn anchored_placement_lands_on_the_skewed_shape() {
    let mut canvas = SceneModel::new(640, 360);
    canvas
        .rect(3.0, 2.0)
        .skew_to(0.5, 0.0)
        .at_anchor(-4.0, 2.0, Anchor::TopLeft);
    canvas
        .text("gyp")
        .with_pivot(0.0, 0.0)
        .skew_to(0.3, 0.1)
        .at_text_anchor(5.0, -1.0, TextAnchor::BaselineLeft);

    let mut world = compile_canvas_for_layout(canvas);
    let (text_bounds, baseline, text_transform) = only_text_root(&mut world);
    let rect = world
        .query_filtered::<(&LocalBounds, &SpatialTransform), Without<TextBaseline>>()
        .iter(&world)
        .find(|(bounds, _)| (bounds.0.width() - 3.0).abs() < 1e-9)
        .map(|(bounds, transform)| (bounds.0, *transform))
        .unwrap();
    assert_point_close(
        rect.1
            .to_mat4()
            .transform_point3(Anchor::TopLeft.get_point(&rect.0)),
        DVec3::new(-4.0, 2.0, 0.0),
    );
    let text_anchor = DVec3::new(text_bounds.min.x, baseline.0, text_bounds.center().z);
    assert_point_close(
        text_transform.to_mat4().transform_point3(text_anchor),
        DVec3::new(5.0, -1.0, 0.0),
    );
}

#[test]
fn pivot_rotation_keeps_the_declared_pose_until_it_starts() {
    let mut canvas = SceneModel::new(640, 360);
    let bar = canvas.rect(3.0, 0.3).move_to(1.5, 0.0).with_pivot(0.0, 0.0);
    canvas.wait(1.0);
    canvas.play(vec![
        bar.animate()
            .rotate_by(std::f64::consts::FRAC_PI_2)
            .duration(1.0),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    for (time, origin, tip) in [
        (0.5, Point::new(1.5, 0.0), Point::new(2.5, 0.0)),
        (3.0, Point::new(0.0, 1.5), Point::new(0.0, 2.5)),
        (0.5, Point::new(1.5, 0.0), Point::new(2.5, 0.0)),
    ] {
        timeline.seek(&mut world, time);
        let affine = transform_of(&mut world, &bar).to_affine_2d();
        let center = affine * Point::ORIGIN;
        let along = affine * Point::new(1.0, 0.0);
        assert!(center.distance(origin) < 1e-6, "{center:?} at {time}");
        assert!(along.distance(tip) < 1e-6, "{along:?} at {time}");
    }
}

#[test]
fn about_point_turns_an_animate_rotation_around_that_point() {
    let mut canvas = SceneModel::new(640, 360);
    let bar = canvas.rect(3.0, 0.3).move_to(1.5, 0.0);
    let other = canvas.rect(3.0, 0.3).move_to(1.5, 0.0);
    canvas.play(vec![
        bar.animate()
            .rotate_by(std::f64::consts::FRAC_PI_2)
            .about_point(0.0, 0.0)
            .duration(1.0),
        other
            .animate()
            .rotate_by(std::f64::consts::FRAC_PI_2)
            .pivot(0.0, 0.0)
            .duration(1.0),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    timeline.seek(&mut world, 1.0);
    for handle in [&bar, &other] {
        let affine = transform_of(&mut world, handle).to_affine_2d();
        let center = affine * Point::ORIGIN;
        let along = affine * Point::new(1.0, 0.0);
        assert!(center.distance(Point::new(0.0, 1.5)) < 1e-6, "{center:?}");
        assert!(along.distance(Point::new(0.0, 2.5)) < 1e-6, "{along:?}");
    }
}

#[test]
fn a_pivot_set_after_showing_cuts_at_the_cursor_without_moving() {
    let mut canvas = SceneModel::new(640, 360);
    let square = canvas.rect(0.4, 0.4).move_to(2.0, 2.0).hidden().unwrap();
    canvas.play(vec![square.animate().fade_in().duration(1.0)]);
    let square = square.with_pivot(0.0, 2.0);
    canvas.play(vec![square.animate().scale_to(2.0).duration(1.0)]);
    // Already scaled about its center: the new pivot moves nothing.
    let grown = canvas.rect(1.0, 1.0).move_to(1.0, 0.0);
    canvas.play(vec![grown.animate().scale_to(2.0).duration(1.0)]);
    let grown = grown.with_pivot(0.0, 0.0);
    canvas.wait(1.0);
    canvas.play(vec![grown.animate().scale_to(1.0).duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    let mut drawn = |time: f64, handle: &DrawableHandle| {
        timeline.seek(&mut world, time);
        let affine = transform_of(&mut world, handle).to_affine_2d();
        (affine * Point::ORIGIN, affine * Point::new(0.5, 0.0))
    };
    for (time, center) in [
        (1.0, Point::new(2.0, 2.0)),
        (2.0, Point::new(4.0, 2.0)),
        (0.5, Point::new(2.0, 2.0)),
        (2.0, Point::new(4.0, 2.0)),
    ] {
        let (drawn, _) = drawn(time, &square);
        assert!(drawn.distance(center) < 1e-9, "{drawn:?} at {time}");
    }
    for (time, center, edge) in [
        (3.5, Point::new(1.0, 0.0), Point::new(2.0, 0.0)),
        (5.0, Point::new(0.5, 0.0), Point::new(1.0, 0.0)),
        (2.5, Point::new(1.0, 0.0), Point::new(1.75, 0.0)),
        (3.5, Point::new(1.0, 0.0), Point::new(2.0, 0.0)),
    ] {
        let (drawn, side) = drawn(time, &grown);
        assert!(drawn.distance(center) < 1e-9, "{drawn:?} at {time}");
        assert!(side.distance(edge) < 1e-9, "{side:?} at {time}");
    }
}

#[test]
fn move_along_an_arrow_travels_its_axis_from_tail_to_tip() {
    let mut canvas = SceneModel::new(640, 360);
    let straight = canvas.arrow(-2.0, 1.0, 2.0, 1.0);
    let curved = canvas.curved_arrow_arc(0.0, -1.0, 1.5, 0.0, std::f64::consts::PI);
    let dot = canvas.dot(0.05).move_to(-2.0, 1.0);
    let other = canvas.dot(0.05).move_to(1.5, -1.0);
    canvas.play(vec![
        dot.animate()
            .move_along(&straight)
            .unwrap()
            .duration(1.0)
            .rate_func(RateFunc::Linear),
        other
            .animate()
            .move_along(&curved)
            .unwrap()
            .duration(1.0)
            .rate_func(RateFunc::Linear),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    for (time, on_line, on_arc) in [
        (0.5, Point::new(0.0, 1.0), Point::new(0.0, 0.5)),
        (1.0, Point::new(2.0, 1.0), Point::new(-1.5, -1.0)),
    ] {
        timeline.seek(&mut world, time);
        let at = |world: &mut World, handle: &DrawableHandle| {
            transform_of(world, handle).to_affine_2d() * Point::ORIGIN
        };
        let line = at(&mut world, &dot);
        let arc = at(&mut world, &other);
        assert!(line.distance(on_line) < 1e-3, "{line:?} at {time}");
        assert!(arc.distance(on_arc) < 1e-3, "{arc:?} at {time}");
    }
}

#[test]
fn full_turns_about_an_off_center_pivot_keep_the_pivot_fixed() {
    // The pivot is off the drawable's origin, so the turn also swings its
    // translation; both must follow the same eased angle.
    let (cx, cy) = (4.3, -2.2);
    let mut canvas = SceneModel::new(640, 360);
    let arm = canvas
        .rect(0.6, 0.1)
        .move_to(cx + 0.5, cy)
        .with_pivot(cx, cy);
    canvas.play(vec![
        arm.animate()
            .rotate_by(std::f64::consts::TAU)
            .duration(1.0)
            .rate_func(RateFunc::Smooth),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    timeline.seek(&mut world, 0.0);
    let local_pivot = transform_of(&mut world, &arm).to_affine_2d().inverse() * Point::new(cx, cy);
    for time in [0.1, 0.25, 0.4, 0.5, 0.75, 0.9, 1.0] {
        timeline.seek(&mut world, time);
        let affine = transform_of(&mut world, &arm).to_affine_2d();
        let pivot = affine * local_pivot;
        assert!(
            pivot.distance(Point::new(cx, cy)) < 1e-3,
            "pivot drifted to {pivot:?} at {time}"
        );
        let direction = affine * Point::new(1.0, 0.0) - affine * Point::ORIGIN;
        let expected = std::f64::consts::TAU * RateFunc::Smooth.evaluate(time);
        let turned = direction
            .y
            .atan2(direction.x)
            .rem_euclid(std::f64::consts::TAU);
        let error = (turned - expected.rem_euclid(std::f64::consts::TAU)).abs();
        assert!(
            error.min(std::f64::consts::TAU - error) < 1e-6,
            "turned {turned} instead of {expected} at {time}"
        );
    }
}

#[test]
fn scale_entries_keep_the_declared_position_until_they_start() {
    let mut canvas = SceneModel::new(640, 360);
    let rows = [2.5, 0.8, -0.8, -2.5];
    let squares: Vec<_> = rows
        .iter()
        .map(|y| canvas.square(0.6).move_to(-5.0, *y))
        .collect();
    canvas.play(
        squares
            .iter()
            .map(|square| square.animate().scale_by(1.5).duration(1.0))
            .collect(),
    );
    canvas.play(
        squares
            .iter()
            .zip(rows)
            .map(|(square, y)| square.animate().move_to(4.0, y).duration(1.0))
            .collect(),
    );
    canvas.play(vec![
        squares[0].animate().grow_from_center().duration(1.0),
        squares[1].animate().spin_in_from_nothing().duration(1.0),
        squares[2].animate().grow_from_point(0.0, 0.0).duration(1.0),
        squares[3]
            .animate()
            .grow_from_edge(gaanim_layout::Direction::Left)
            .duration(1.0),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    // Mid-way through the first scale, before any move or entry.
    for (time, x, scale) in [(0.5, -5.0, None), (3.5, 4.0, Some(1.5)), (0.5, -5.0, None)] {
        timeline.seek(&mut world, time);
        for (square, y) in squares.iter().zip(rows) {
            let transform = transform_of(&mut world, square);
            assert!(
                transform.translation.distance(DVec3::new(x, y, 0.0)) < 1e-6,
                "{:?} at {time}",
                transform.translation
            );
            if let Some(scale) = scale {
                assert!(
                    (transform.scale.x - scale).abs() < 1e-6,
                    "{:?}",
                    transform.scale
                );
            } else {
                assert!(transform.scale.x > 1.0, "visible while scaling at {time}");
            }
        }
    }
}

#[test]
fn entries_and_emphasis_wait_for_their_delay() {
    let mut canvas = SceneModel::new(640, 360);
    let squares: Vec<_> = (0..5)
        .map(|index| canvas.square(0.6).move_to(index as f64 * 1.5 - 3.0, 0.0))
        .collect();
    canvas.play(
        vec![
            squares[0].animate().grow_from_point(0.0, 0.0),
            squares[1]
                .animate()
                .grow_from_edge(gaanim_layout::Direction::Left),
            squares[2].animate().spin_in_from_nothing(),
            squares[3].animate().indicate(None, 1.2),
            squares[4].animate().wiggle(0.1, 3),
        ]
        .into_iter()
        .map(|anim| anim.duration(1.0).delay(1.0))
        .collect(),
    );
    let (mut world, mut timeline) = compiled_world(&canvas);

    timeline.seek(&mut world, 0.5);
    for square in &squares[..3] {
        assert_eq!(transform_of(&mut world, square).scale.x, 0.0, "hidden");
    }
    for (index, square) in squares[3..].iter().enumerate() {
        let transform = transform_of(&mut world, square);
        assert_eq!(transform.scale.x, 1.0);
        assert_eq!(transform.translation.x, (index + 3) as f64 * 1.5 - 3.0);
    }
    timeline.seek(&mut world, 1.5);
    for square in &squares[..3] {
        let scale = transform_of(&mut world, square).scale.x;
        assert!(scale > 0.0 && scale < 1.0, "growing: {scale}");
    }
    let indicated = transform_of(&mut world, &squares[3]).scale.x;
    assert!(indicated > 1.0, "indicating: {indicated}");
    let wiggled = transform_of(&mut world, &squares[4]).translation.x;
    assert!((wiggled - 3.0).abs() > 1e-6, "wiggling: {wiggled}");
}

#[test]
fn flashes_shoot_rays_and_circumscribe_draws_a_frame_around_the_target() {
    let mut canvas = SceneModel::new(640, 360);
    let star = canvas.circle(0.5).move_to_default(1.0, 1.0);
    let flash = star.animate().flash(None, None, 8, 0.3, 0.1).unwrap();
    let circumscribe = star
        .animate()
        .circumscribe(None, None, [0.1; 4], 0.0)
        .unwrap();
    for anim in [&flash, &circumscribe] {
        assert!(matches!(
            anim.inner.anim_type,
            AnimationType::ShowPassingFlash { time_width } if time_width == 1.0
        ));
    }
    assert!(star.animate().flash(None, None, 0, 0.3, 0.1).is_err());
    assert!(star.animate().flash(None, None, 8, 0.0, 0.1).is_err());
    assert!(star.animate().flash(None, None, 8, 0.3, -0.1).is_err());
    canvas.play(vec![flash, circumscribe]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.5);
    gaanim_animation::surrounding_rect_system(&mut world);
    let rays = world
        .query::<(
            &gaanim_animation::SurroundingRect,
            &gaanim_scene::PathSource,
        )>()
        .iter(&world)
        .find_map(|(frame, path)| frame.rays.map(|rays| (rays, path.0.clone())))
        .expect("a flash draws rays");
    assert_eq!(rays.0.count, 8);
    let starts: Vec<_> = rays
        .1
        .elements()
        .iter()
        .filter_map(|element| match element {
            gaanim_core::kurbo::PathEl::MoveTo(point) => Some(*point),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 8);
    for point in starts {
        // Half the circle's side plus the gap from its center.
        let distance = point.distance(gaanim_core::kurbo::Point::new(1.0, 1.0));
        assert!((distance - 0.6).abs() < 1e-6, "{distance}");
    }
}

#[test]
fn indications_tint_with_the_accent_unless_given_a_color() {
    let mut canvas = SceneModel::new(640, 360);
    let red = gaanim_core::peniko::Color::from_rgb8(255, 0, 0);
    let accent = canvas
        .theme_color("accent")
        .unwrap_or(crate::builder::DEFAULT_EMPHASIS_COLOR);
    let foreground = canvas
        .theme_color("foreground")
        .unwrap_or(crate::builder::DEFAULT_EMPHASIS_CONTRAST);
    let plain = canvas.circle(0.5).fill(gaanim_core::peniko::Color::BLACK);
    let colored = canvas.circle(0.5).fill(gaanim_core::peniko::Color::BLACK);
    let accented = canvas.circle(0.5).fill(accent);
    canvas.play(vec![
        plain.animate().indicate(None, 1.2),
        colored.animate().indicate(Some(red), 1.5),
        accented.animate().indicate(None, 1.2),
    ]);
    let (_, timeline) = compiled_world(&canvas);
    let tints: Vec<_> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::FillColor { to, .. },
                    ..
                },
            ) => Some(*to),
            _ => None,
        })
        .collect();
    assert!(tints.contains(&accent), "{tints:?}");
    assert!(tints.contains(&red));
    assert!(
        tints.contains(&foreground),
        "an accent circle turns to the foreground: {tints:?}"
    );
}

#[test]
fn text_emphases_differ_pulse_beats_twice_wave_lifts_in_turn_highlight_tints() {
    let mut canvas = SceneModel::new(640, 360);
    let text = canvas.text("ab cd");
    canvas.play(vec![text.select("cd").wave(1.0)]);
    canvas.play(vec![text.select("cd").pulse(1.0)]);
    canvas.play(vec![text.select("cd").highlight(1.0)]);
    let (_, timeline) = compiled_world(&canvas);
    let starts =
        |within: std::ops::Range<f64>,
         pick: &dyn Fn(&gaanim_timeline::clip::PropertyLensSpec) -> bool| {
            let mut starts: Vec<f64> = timeline
                .clips
                .values()
                .filter(|clip| within.contains(&clip.start))
                .filter(|clip| match &clip.payload {
                    gaanim_timeline::clip::ClipPayload::Animation(animation) => {
                        pick(&animation.lens)
                    }
                    _ => false,
                })
                .map(|clip| (clip.start * 1000.0).round() / 1000.0)
                .collect();
            starts.sort_by(f64::total_cmp);
            starts.dedup();
            starts
        };
    use gaanim_timeline::clip::PropertyLensSpec as Lens;
    let lifts = starts(0.0..1.0, &|lens| matches!(lens, Lens::Translation { .. }));
    assert_eq!(
        lifts,
        vec![0.0, 0.25, 0.5, 0.75],
        "one glyph after the other"
    );
    let beats = starts(
        1.0..2.0,
        &|lens| matches!(lens, Lens::Scale { from, to } if from != to),
    );
    assert_eq!(beats, vec![1.0, 1.25, 1.5, 1.75], "two beats");
    let accent = canvas
        .theme_color("accent")
        .unwrap_or(crate::builder::DEFAULT_EMPHASIS_COLOR);
    let tints = starts(
        2.0..3.0,
        &|lens| matches!(lens, Lens::FillColor { to, .. } if *to == accent),
    );
    assert_eq!(tints, vec![2.0]);
    let grows = starts(
        2.0..3.0,
        &|lens| matches!(lens, Lens::Scale { from, to } if from != to),
    );
    assert!(grows.is_empty(), "a highlight only tints");
}

#[test]
fn clearing_a_look_at_keeps_the_turn_it_gave() {
    let mut canvas = SceneModel::new(640, 360);
    let pointer = canvas.rect(1.0, 0.2);
    pointer.drive_falloff(gaanim_animation::FalloffEffect::LookAt {
        target: gaanim_animation::FalloffTarget::Point(DVec2::new(0.0, 5.0)),
        offset: 0.0,
    });
    canvas.wait(1.0);
    pointer.clear_falloff();
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    for time in [0.5, 1.5] {
        timeline.seek(&mut world, time);
        world.insert_resource(gaanim_animation::PlaybackState {
            current_time: time,
            ..Default::default()
        });
        gaanim_animation::falloff::apply_falloff_system(&mut world);
        let (_, _, turn) = transform_of(&mut world, &pointer)
            .rotation
            .to_euler(gaanim_core::glam::EulerRot::XYZ);
        assert!(
            (turn - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
            "{turn} at {time}"
        );
        let mut restore = Schedule::default();
        restore.add_systems(gaanim_animation::falloff::restore_falloff_system);
        restore.run(&mut world);
    }
}

#[test]
fn camera_moves_and_magic_moves_take_the_play_default_duration() {
    use crate::canvas::PlayItem;
    let mut canvas = SceneModel::new(640, 360);
    let before = magic_move_board(&mut canvas, &[("dot", 1.0)]);
    let after = magic_move_board(&mut canvas, &[("dot", 2.0)]);
    let pan = canvas.camera_pan_to(3.0, 0.0, 1.0);
    let keyed = before
        .magic_move_to(
            &after,
            crate::canvas::MagicMoveKey::Name,
            crate::canvas::MagicMoveUnmatched::Fade,
            None,
        )
        .unwrap();
    canvas
        .play_items_configured(
            vec![PlayItem::Animation(pan), PlayItem::Animation(keyed)],
            Some(2.5),
            None,
        )
        .unwrap();
    let fixed = canvas.camera_pan_to(0.0, 0.0, 1.0).duration(0.5);
    canvas
        .play_items_configured(vec![PlayItem::Animation(fixed)], Some(2.5), None)
        .unwrap();
    let (_, timeline) = compiled_world(&canvas);
    let mut pans: Vec<_> = timeline
        .clips
        .values()
        .filter(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::CameraPosition { .. },
                        ..
                    }
                )
            )
        })
        .map(|clip| (clip.start, clip.duration))
        .collect();
    pans.sort_by(|left, right| left.0.total_cmp(&right.0));
    assert_eq!(pans, vec![(0.0, 2.5), (2.5, 0.5)]);
}

#[test]
fn text_declared_transparent_fades_back_in_with_its_root_opacity() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let text = canvas.text("(4.6) texto").opacity(0.0);
    canvas.play(vec![text.animate().opacity(1.0).duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let root = ObjectId::from_raw(text.id.as_raw() - 1);
    let root_entity = world
        .query::<(Entity, &MobjectId)>()
        .iter(&world)
        .find(|(_, object)| object.0 == root)
        .unwrap()
        .0;
    for (time, expected) in [(0.0, 0.0), (1.0, 1.0)] {
        timeline.seek(&mut world, time);
        assert!((opacity_of(&mut world, &text) - expected).abs() < 1e-5);
        let spans: Vec<f32> = world
            .query::<(&Opacity, &ChildOf)>()
            .iter(&world)
            .filter(|(_, parent)| parent.parent() == root_entity)
            .map(|(opacity, _)| opacity.0)
            .collect();
        assert!(!spans.is_empty());
        assert!(
            spans.iter().all(|&opacity| opacity == 1.0),
            "{spans:?} at {time}"
        );
    }
}

#[test]
fn z_index_set_after_a_reactive_binding_orders_the_drawable() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let level = canvas.parameter(0.2).expect("parameter");
    let mask = canvas.rect(2.0, 2.0).opacity(0.0);
    let water = canvas
        .fill_level(
            &mask,
            Brush::Solid(PenikoColor::WHITE),
            0.2,
            crate::canvas::FillLevelDirection::Up,
            false,
        )
        .expect("fill level")
        .set_fill_level(&level)
        .expect("reactive level")
        .z_index(4);
    canvas.wait(0.5);
    let (mut world, _) = compiled_world(&canvas);
    let id = ObjectId::from_raw(water.id.as_raw() - 1);
    let order = world
        .query::<(&MobjectId, &RenderOrder)>()
        .iter(&world)
        .find(|(object, _)| object.0 == id)
        .map(|(_, order)| order.z_index);
    assert_eq!(order, Some(4));
}

#[test]
fn z_layers_rank_drawables_for_the_whole_scene() {
    let mut canvas = SceneModel::new(640, 360);
    let wall = canvas.rect(1.0, 1.0).z_index(400);
    assert!(wall.clone().z_layer("model").is_err());
    assert!(canvas.z_layers(&[], None).is_err());
    assert!(canvas.z_layers(&["model", "model"], None).is_err());
    assert!(canvas.z_layers(&["model", " "], None).is_err());
    assert!(canvas.z_layers(&["model"], Some("overlay")).is_err());
    canvas
        .z_layers(&["back", "model", "overlay"], Some("model"))
        .unwrap();
    canvas
        .z_layers(&["back", "model", "overlay"], Some("model"))
        .unwrap();
    assert!(canvas.z_layers(&["model", "overlay"], None).is_err());
    assert!(wall.clone().z_layer("front").is_err());
    let lens = canvas.circle(0.5).z_layer("overlay").unwrap();
    let floor = canvas.rect(2.0, 2.0).z_layer("back").unwrap();
    canvas.play(vec![lens.animate().move_to(1.0, 0.0).duration(1.0)]);
    // Draw order holds for the whole scene, so a later call applies too.
    let wall = wall.z_layer("model").unwrap();
    let unlayered = canvas.circle(0.2);
    canvas.wait(1.0);
    let (mut world, _) = compiled_world(&canvas);
    let mut layer = |handle: &DrawableHandle| {
        let id = ObjectId::from_raw(handle.id.as_raw() - 1);
        world
            .query::<(&MobjectId, Option<&gaanim_scene::ZLayer>)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .map(|layer| layer.0)
    };
    assert_eq!(
        [&lens, &floor, &wall, &unlayered].map(&mut layer),
        [Some(1), Some(-1), Some(0), None]
    );
}

fn opacity_of(world: &mut World, handle: &DrawableHandle) -> f32 {
    let id = ObjectId::from_raw(handle.id.as_raw() - 1);
    world
        .query::<(&MobjectId, &Opacity)>()
        .iter(world)
        .find(|(object, _)| object.0 == id)
        .unwrap()
        .1
        .0
}

#[test]
fn entrances_show_the_opacity_an_object_was_declared_or_left_with() {
    let mut canvas = SceneModel::new(640, 360);
    let faint = canvas.rect(1.0, 1.0).opacity(0.3).hidden().unwrap();
    let unset = canvas.rect(1.0, 1.0).opacity(0.0).hidden().unwrap();
    canvas.play(vec![
        faint.animate().fade_in().duration(1.0),
        unset.animate().fade_in().duration(1.0),
    ]);
    canvas.play(vec![faint.animate().fade_out().duration(1.0)]);
    canvas.play(vec![faint.animate().fade_in().duration(1.0)]);
    canvas.play(vec![faint.animate().opacity(0.0).duration(1.0)]);
    canvas.wait(1.0);
    canvas.play(vec![faint.animate().create().duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    for (time, expected) in [(1.0, 0.3), (2.0, 0.0), (3.0, 0.3), (4.5, 0.0), (6.0, 0.3)] {
        timeline.seek(&mut world, time);
        let opacity = opacity_of(&mut world, &faint);
        assert!((opacity - expected).abs() < 1e-6, "{opacity} at {time}");
    }
    timeline.seek(&mut world, 1.0);
    assert_eq!(opacity_of(&mut world, &unset), 1.0);
}

#[test]
fn absolute_geometry_turns_scales_and_skews_about_its_box_center() {
    use std::f64::consts::PI;

    let mut canvas = SceneModel::new(640, 360);
    // Box center (4, 2); the local origin stays at the scene origin.
    let triangle = canvas.polygon(vec![(3.0, 1.0), (5.0, 1.0), (4.0, 3.0)]);
    let arc = canvas.curved_arrow_arc(0.0, -2.5, 0.38, 1.9, 5.2);
    let line = canvas.line(1.0, 1.0, 3.0, 1.0).rotate_to(PI / 2.0);
    canvas.play(vec![
        triangle.animate().rotate_by(2.0 * PI).duration(1.0),
        arc.animate().rotate_by(PI).duration(1.0),
    ]);
    canvas.play(vec![
        triangle.animate().scale_to(2.0).duration(1.0),
        arc.animate().skew_to(0.4, 0.0).duration(1.0),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    let world_point = |world: &mut World, handle: &DrawableHandle, local: DVec3| {
        transform_of(world, handle)
            .to_mat4()
            .transform_point3(local)
    };
    timeline.seek(&mut world, 0.0);
    let arc_center =
        transform_of(&mut world, &arc).translation + transform_of(&mut world, &arc).anchor;
    for time in [0.0, 0.3, 0.5, 1.0, 1.5, 2.0] {
        timeline.seek(&mut world, time);
        let center = world_point(&mut world, &triangle, DVec3::new(4.0, 2.0, 0.0));
        assert!(
            center.distance(DVec3::new(4.0, 2.0, 0.0)) < 1e-6,
            "{time}: {center:?}"
        );
        let pivot =
            transform_of(&mut world, &arc).translation + transform_of(&mut world, &arc).anchor;
        assert!(pivot.distance(arc_center) < 1e-6, "{time}: {pivot:?}");
    }
    assert!(arc_center.distance(DVec3::ZERO) > 1.0, "{arc_center:?}");
    // The declared rotation also turns the line about its midpoint.
    let start = world_point(&mut world, &line, DVec3::new(1.0, 1.0, 0.0));
    let end = world_point(&mut world, &line, DVec3::new(3.0, 1.0, 0.0));
    assert!(
        start.distance(DVec3::new(2.0, 0.0, 0.0)) < 1e-9,
        "{start:?}"
    );
    assert!(end.distance(DVec3::new(2.0, 2.0, 0.0)) < 1e-9, "{end:?}");
}

#[test]
fn pivot_turns_of_a_rotated_object_start_from_its_pose() {
    let mut canvas = SceneModel::new(640, 360);
    let bar = canvas
        .rect(2.0, 0.4)
        .move_to(1.0, 0.0)
        .with_pivot(0.0, 0.0)
        .rotate_to(0.5);
    canvas.wait(1.0);
    canvas.play(vec![bar.animate().rotate_by(1.0).duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let tip = |world: &mut World| {
        transform_of(world, &bar)
            .to_mat4()
            .transform_point3(DVec3::new(2.0, 0.0, 0.0))
    };
    let radius = |point: DVec3| point.truncate().length();
    timeline.seek(&mut world, 0.5);
    let declared = tip(&mut world);
    timeline.seek(&mut world, 1.0);
    let started = tip(&mut world);
    assert!(
        started.distance(declared) < 1e-6,
        "{started:?} vs {declared:?}"
    );
    // The swing follows a polyline with one-degree steps.
    for time in [1.3, 1.7, 2.0] {
        timeline.seek(&mut world, time);
        let point = tip(&mut world);
        assert!(
            (radius(point) - radius(declared)).abs() < 1e-4,
            "{time}: {point:?}"
        );
    }
    let angle = |point: DVec3| point.y.atan2(point.x);
    assert!((angle(tip(&mut world)) - angle(declared) - 1.0).abs() < 1e-6);
}

#[test]
fn pop_in_hides_the_screen_until_the_next_pop_out() {
    let mut canvas = SceneModel::new(640, 360);
    let frame = canvas.rect(2.0, 1.0).move_to(3.0, 2.0);
    let view = canvas
        .rect(4.0, 2.0)
        .move_to(-4.0, -1.0)
        .camera_view(&frame)
        .unwrap();
    canvas.play(vec![view.pop_out().duration(1.0)]);
    canvas.play(vec![view.pop_in().duration(1.0)]);
    canvas.wait(1.0);
    canvas.play(vec![view.pop_out().duration(1.0)]);
    canvas.play(vec![view.screen().animate().fade_out().duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    for (time, expected) in [
        (0.5, 1.0),
        (1.5, 1.0),
        (2.0, 0.0),
        (2.5, 0.0),
        (3.0, 1.0),
        (3.5, 1.0),
        (4.0, 1.0),
        (5.0, 0.0),
    ] {
        timeline.seek(&mut world, time);
        let opacity = opacity_of(&mut world, view.screen());
        assert!((opacity - expected).abs() < 1e-6, "{time}: {opacity}");
    }
    timeline.seek(&mut world, 1.5);
    assert!((opacity_of(&mut world, view.screen()) - 1.0).abs() < 1e-6);
}

#[test]
fn polls_take_votes_from_their_cursor_to_their_close_or_segment_end() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_poll_session(crate::canvas::PollSession {
        relay: Some("https://relay.example.dev".into()),
        code: "ABC234".into(),
    });
    canvas.segment("Intro", None).unwrap();
    canvas.dot(0.1);
    canvas.wait(1.0);
    canvas.segment("Vote", None).unwrap();
    canvas.wait(0.5);
    let whole = canvas
        .poll(
            "Whole?",
            ["A", "B"],
            Lean::Weights(vec![1.0, 0.0]),
            crate::canvas::PollStyle::default(),
        )
        .unwrap();
    let bar = whole
        .bar(
            0,
            crate::canvas::PollBarOptions {
                length: 4.0,
                thickness: 0.5,
                radius: 0.0,
                direction: crate::canvas::BarDirection::Right,
                scale: crate::canvas::BarScale::Total,
            },
        )
        .unwrap();
    let share = whole.share(0).unwrap();
    canvas.wait(2.0);
    let early = canvas
        .poll(
            "Early?",
            ["X", "Y", "Z"],
            Lean::Auto,
            crate::canvas::PollStyle::default(),
        )
        .unwrap();
    canvas.wait(1.0);
    early.close().unwrap();
    canvas.wait(0.5);

    let timeline = compiled_timeline(&canvas);
    let windows: Vec<(&str, f64, f64)> = timeline
        .polls
        .iter()
        .map(|poll| (poll.question.as_str(), poll.open, poll.close))
        .collect();
    assert_eq!(windows, [("Whole?", 1.5, 5.0), ("Early?", 3.5, 4.5)]);
    // The rehearsal's twelve players all voted, as the poll leans.
    assert_eq!(timeline.polls[0].preview, [12, 0]);
    assert_eq!(timeline.polls[1].preview.iter().sum::<u32>(), 12);
    let session = timeline.poll_session.as_ref().unwrap();
    assert_eq!(session.code, "ABC234");
    // Only a scene that shows its audience opens a lobby.
    assert!(!session.lobby);
    assert!(bar.id != share.drawable().id);
}

#[test]
fn a_character_schedules_its_expressions_at_the_cursor() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.segment("Intro", None).unwrap();
    let hero = canvas.character(None, "Ana", 2.0).unwrap();
    assert_eq!(
        hero.character(),
        gaanim_objects::character::catalog()
            .character_from_seed(gaanim_objects::character::character_seed("Ana"))
    );
    canvas.wait(1.0);
    hero.express(Some("happy"), false).unwrap();
    canvas.wait(1.0);
    hero.express(Some("winner"), true).unwrap();
    canvas.wait(1.0);
    assert!(hero.express(Some("dancing"), false).is_err());
    assert!(canvas.character(Some([9, 0, 0, 0, 0]), "", 2.0).is_err());
    assert!(canvas.character(None, "", 0.0).is_err());

    let (mut world, _) = compiled_world(&canvas);
    let rig = world
        .query::<&gaanim_animation::characters::CharacterRig>()
        .single(&world)
        .unwrap()
        .clone();
    let times: Vec<f64> = rig.schedule.iter().map(|(time, _)| *time).collect();
    assert_eq!(times, [1.0, 2.0]);
    assert_eq!(
        rig.layers.len(),
        gaanim_objects::character::catalog().max_layers()
    );
    assert_eq!(rig.expression_at(2.5).unwrap().name, "winner");
}

#[test]
fn a_gated_stop_compiles_at_its_absolute_time() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_poll_session(crate::canvas::PollSession {
        relay: Some("https://relay.example.dev".into()),
        code: "ABC234".into(),
    });
    canvas.segment("Intro", None).unwrap();
    canvas.wait(1.0);
    canvas.segment("Quiz", None).unwrap();
    let quiz = canvas
        .quiz(
            "¿2 + 2?",
            ["3", "4"],
            vec![1],
            20,
            1000,
            Lean::Auto,
            crate::canvas::PollStyle::default(),
        )
        .unwrap();
    canvas.wait(0.5);
    let before = canvas.stop_count();
    canvas.stop(None).unwrap();
    let until = quiz.answered(None, Some(0.8)).unwrap();
    canvas.gate_stop(before, until.clone());
    assert!(quiz.answered(Some(3), Some(0.5)).is_err());
    assert!(quiz.answered(None, Some(1.5)).is_err());
    canvas.wait(1.0);

    let timeline = compiled_timeline(&canvas);
    assert_eq!(timeline.stop_gates.len(), 1);
    assert!((timeline.stop_gates[0].time - 1.5).abs() < 1e-9);
    assert_eq!(timeline.stop_gates[0].until, until);
    assert!(matches!(
        until,
        crate::canvas::GateCondition::AnswerShare { players: true, .. }
    ));
}

#[test]
fn showing_the_audience_opens_a_lobby_for_phones() {
    let mut canvas = SceneModel::new(640, 360);
    assert_eq!(
        canvas.audience().unwrap_err(),
        crate::canvas::PollError::NoSession
    );
    canvas.set_poll_session(crate::canvas::PollSession {
        relay: Some("https://relay.example.dev".into()),
        code: "ABC234".into(),
    });
    canvas.segment("Intro", None).unwrap();
    canvas.wait(2.0);
    canvas.segment("Lobby", None).unwrap();
    let audience = canvas.audience().unwrap();
    assert_eq!(audience.url(), "https://relay.example.dev/s/ABC234");
    let options = crate::canvas::LiveTextOptions {
        size: Some(0.4),
        weight: None,
        font: None,
        align: crate::canvas::TextAlign::Center,
    };
    audience.name(0, options.clone()).unwrap();
    audience.name(5, options).unwrap();
    audience.count().unwrap();
    audience.joined(1).unwrap();
    audience.age(1).unwrap();
    audience.qr(2.0).unwrap();
    canvas.wait(1.0);

    let timeline = compiled_timeline(&canvas);
    assert!(timeline.polls.is_empty());
    assert!(timeline.poll_session.as_ref().unwrap().lobby);
    // A new game goes back to the lobby, after the intro.
    assert_eq!(timeline.game_start(), Some(2.0));
}

#[test]
fn stops_record_their_ambient_loop_length() {
    let mut canvas = SceneModel::new(640, 360);
    let dot = canvas.dot(0.1);
    canvas.wait(1.0);
    canvas
        .stop_with_loop(
            Some("placas".into()),
            crate::canvas::Composition::leaf(dot.animate().shift_by(1.0, 0.0).duration(1.5)),
        )
        .unwrap();
    canvas.wait(0.5);
    canvas.stop(None).unwrap();
    let stops = &canvas.segment_manifest().segments[0].stops;
    assert_eq!(stops.len(), 2);
    assert_eq!((stops[0].time, stops[0].ambient), (1.0, Some(1.5)));
    assert_eq!((stops[1].time, stops[1].ambient), (3.0, None));
    canvas.wait(1.0);
    assert!(matches!(
        canvas.stop_with_loop(
            None,
            crate::canvas::Composition::leaf(dot.animate().shift_by(1.0, 0.0).duration(0.0)),
        ),
        Err(crate::canvas::StopLoopError::Empty)
    ));
}

#[test]
fn bounds_measure_any_drawable_at_the_cursor() {
    let mut canvas = SceneModel::new(640, 360);
    let rect = canvas.rect(2.0, 1.0).move_to(3.0, -1.0);
    let group = {
        let a = canvas.square(1.0).move_to(-4.0, 0.0);
        let b = canvas.circle(0.5).move_to(-1.0, 2.0);
        canvas.group(&[&a, &b])
    };
    canvas.play(vec![rect.animate().move_to(0.0, 0.0).duration(1.0)]);
    let canvas = canvas.into_shared();
    let (rect_box, group_box) = {
        let _guard = canvas.lock().unwrap();
        (rect.clone(), group.clone())
    };
    let rect_box = rect_box.bounds().unwrap();
    assert!((rect_box.width() - 2.0).abs() < 1e-9 && (rect_box.height() - 1.0).abs() < 1e-9);
    // Measured at the cursor, after the move ended.
    assert!(rect_box.center().truncate().length() < 1e-9, "{rect_box:?}");
    let group_box = group_box.bounds().unwrap();
    assert!((group_box.min.x + 4.5).abs() < 1e-9 && (group_box.max.y - 2.5).abs() < 1e-9);
}

#[test]
fn pixels_map_onto_the_displayed_image() {
    use gaanim_objects::primitives::ImageView;
    let mut canvas = SceneModel::new(640, 360);
    let image = gaanim_core::peniko::ImageData {
        data: gaanim_core::peniko::Blob::new(std::sync::Arc::new(vec![255u8; 4 * 200 * 100])),
        format: gaanim_core::peniko::ImageFormat::Rgba8,
        alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
        width: 200,
        height: 100,
    };
    // 200x100 px shown 4 units wide: 50 px per unit.
    let view = ImageView {
        source_x: 0.0,
        source_y: 0.0,
        source_width: 200.0,
        source_height: 100.0,
        display_width: 4.0,
        display_height: 2.0,
        scale_x: 0.02,
        scale_y: 0.02,
        quality: gaanim_core::peniko::ImageQuality::Medium,
    };
    let picture = canvas
        .spawn(SpawnKind::Image { image, view })
        .move_to(1.0, 1.0);
    let corner = picture.pixel(0.0, 0.0).unwrap();
    let inner = picture.pixel(150.0, 25.0).unwrap();
    assert!(canvas.rect(1.0, 1.0).pixel(0.0, 0.0).is_err());
    let marker = canvas.dot(0.05).at_anchor_point(inner);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.0);
    let at = transform_of(&mut world, &marker).translation;
    // Pixel (150, 25): x = -2 + 3 = 1, y = 1 - 0.5 = 0.5, then moved by (1, 1).
    assert!(at.distance(DVec3::new(2.0, 1.5, 0.0)) < 1e-9, "{at:?}");
    assert_eq!(corner.offset, DVec3::new(-2.0, 1.0, 0.0));
}

/// `billboard()` turns any drawable toward the camera: an image as
/// well as a shape.
#[test]
fn images_and_shapes_become_billboards() {
    use bevy::prelude::App;
    use gaanim_objects::primitives::ImageView;
    let mut canvas = SceneModel::new(640, 360);
    let image = gaanim_core::peniko::ImageData {
        data: gaanim_core::peniko::Blob::new(std::sync::Arc::new(vec![255u8; 4 * 4])),
        format: gaanim_core::peniko::ImageFormat::Rgba8,
        alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
        width: 2,
        height: 2,
    };
    let view = ImageView {
        source_x: 0.0,
        source_y: 0.0,
        source_width: 2.0,
        source_height: 2.0,
        display_width: 1.0,
        display_height: 1.0,
        scale_x: 0.5,
        scale_y: 0.5,
        quality: gaanim_core::peniko::ImageQuality::Medium,
    };
    let picture = canvas
        .spawn(SpawnKind::Image { image, view })
        .billboard()
        .move_to_3d(1.0, 0.0, -1.0);
    let square = canvas.rect(1.0, 1.0).billboard();
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    for handle in [&picture, &square] {
        let entity = entity_of(app.world_mut(), handle);
        assert!(app.world().get::<gaanim_scene::Billboard>(entity).is_some());
    }
}

/// A billboard group whose member lies off the group's origin puts the
/// point of its box that `anchor` names where `move_to_3d` put it, seen
/// in perspective from the side, as a lone drawable does.
#[test]
fn a_billboard_group_puts_its_anchor_where_move_to_3d_puts_it() {
    let mut canvas = SceneModel::new(1280, 720);
    let look = canvas.camera_look_at((7.0, 4.0, 9.0), (0.0, 0.0, 0.0), None, 0.0);
    let perspective = canvas.camera_perspective(0.6, 0.1, 100.0, 0.0);
    canvas.play(vec![look, perspective]);
    // Its right edge on the scene's origin.
    let bar = canvas.rect(5.0, 1.0).move_to(-2.5, 0.0);
    let group = canvas
        .group(&[&bar])
        .move_to_3d(0.0, 0.0, 0.0)
        .billboard_shifted(
            gaanim_core::glam::DVec2::ZERO,
            Some(crate::canvas::Anchor::Right),
        );
    canvas.wait(0.5);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(0.25);
    app.update();
    let group = entity_of(app.world_mut(), &group);
    let shift = *app
        .world()
        .get::<gaanim_scene::BillboardShift>(group)
        .unwrap();
    assert_eq!(shift.point, DVec3::new(-2.5, 0.0, 0.0));
    let camera = app.world().resource::<gaanim_math::ResolvedCamera>().camera;
    assert!(matches!(
        camera.projection,
        gaanim_math::Projection::Perspective { .. }
    ));
    let bar = entity_of(app.world_mut(), &bar);
    let global = app
        .world()
        .get::<gaanim_math::GlobalSpatialTransform>(bar)
        .unwrap();
    let right = global.affine_2d * gaanim_core::kurbo::Point::new(2.5, 0.0);
    // The camera looks at the origin: the canvas's middle.
    assert!(right.to_vec2().hypot() < 1e-6, "right edge at {right:?}");
}

/// A drawing laid on a model plane puts each of its points p at
/// origin + x * u + y * v, a moved shape and a group's members alike.
#[test]
fn a_drawing_on_a_model_plane_lands_at_origin_plus_its_axes() {
    use bevy::prelude::App;
    let mut canvas = SceneModel::new(640, 360);
    let card = canvas
        .rect(2.0, 1.0)
        .move_to(3.0, 1.0)
        .in_3d_on_plane(None, DVec3::X, DVec3::Z, DVec3::new(0.0, 5.0, 0.0))
        .unwrap();
    let near = canvas.rect(0.5, 0.5).move_to(1.0, 0.0);
    let far = canvas.rect(0.5, 0.5).move_to(3.0, 0.0);
    canvas
        .group(&[&near, &far])
        .in_3d_on_plane(None, DVec3::Y, DVec3::Z, DVec3::new(7.0, 0.0, 0.0))
        .unwrap();
    assert!(
        canvas
            .rect(1.0, 1.0)
            .in_3d_on_plane(None, DVec3::X, DVec3::X, DVec3::ZERO)
            .is_err()
    );
    // One already shown is placed at the cursor the same way.
    let late = canvas.rect(2.0, 1.0).move_to(3.0, 1.0);
    canvas.wait(0.5);
    let late = late
        .in_3d_on_plane(None, DVec3::X, DVec3::Z, DVec3::new(0.0, 5.0, 0.0))
        .unwrap();
    canvas.wait(0.5);
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(0.75);
    app.update();
    let mut world_point = |handle: &DrawableHandle, local: DVec3| {
        let entity = entity_of(app.world_mut(), handle);
        app.world()
            .get::<gaanim_math::GlobalSpatialTransform>(entity)
            .unwrap()
            .mat4
            .transform_point3(local)
    };
    let close = |a: DVec3, b: DVec3| a.distance(b) < 1e-9;
    for (local, expected) in [
        (DVec3::ZERO, DVec3::new(3.0, 5.0, 1.0)),
        (DVec3::X, DVec3::new(4.0, 5.0, 1.0)),
        (DVec3::Y, DVec3::new(3.0, 5.0, 2.0)),
    ] {
        let point = world_point(&card, local);
        assert!(close(point, expected), "{local:?}: {point:?}");
        let point = world_point(&late, local);
        assert!(close(point, expected), "late {local:?}: {point:?}");
    }
    let point = world_point(&near, DVec3::ZERO);
    assert!(close(point, DVec3::new(7.0, 1.0, 0.0)), "{point:?}");
    let point = world_point(&far, DVec3::ZERO);
    assert!(close(point, DVec3::new(7.0, 3.0, 0.0)), "{point:?}");
}

#[test]
fn media_points_and_regions_stay_on_their_pixels_through_an_animated_crop() {
    use gaanim_animation::{EndpointFollow, MediaRegion, resolve_tracking_endpoint};
    let mut canvas = SceneModel::new(640, 360);
    let image = gaanim_core::peniko::ImageData {
        data: gaanim_core::peniko::Blob::new(std::sync::Arc::new(vec![255u8; 4 * 200 * 100])),
        format: gaanim_core::peniko::ImageFormat::Rgba8,
        alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
        width: 200,
        height: 100,
    };
    // 200x100 px contained in a 4x2 frame: 50 px per unit.
    let options = crate::canvas::types::ImageOptions {
        width: Some(4.0),
        height: Some(2.0),
        ..Default::default()
    };
    let view = options.resolve(200, 100).unwrap();
    let picture = canvas
        .spawn(SpawnKind::Image { image, view })
        .move_to(1.0, 1.0)
        .rotate_to(std::f64::consts::FRAC_PI_2);
    picture.spec.lock().unwrap().media_frame = Some(options.media_frame(view));
    let point = picture.media_point(0.75, 0.25, true).unwrap();
    // The top-right quarter of the source.
    let region = picture
        .media_region(100.0, 0.0, 100.0, 50.0, false)
        .unwrap();
    assert!(picture.media_region(0.0, 0.0, 0.0, 10.0, false).is_err());
    assert!(picture.media_point(f64::NAN, 0.0, false).is_err());
    assert!(canvas.rect(1.0, 1.0).media_point(0.0, 0.0, false).is_err());
    let marker = canvas.dot(0.05).follow_endpoint(
        point.0.clone(),
        DVec3::ZERO,
        gaanim_animation::FollowOffsetSpace::World,
    );
    // Then show the source's right half, contained: the same scale,
    // with the frame centered on pixel x 150.
    let zoom = picture
        .animate()
        .crop(100.0, 0.0, 100.0, 100.0, false)
        .unwrap()
        .duration(1.0);
    canvas.play(vec![zoom]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let marker_entity = entity_of(&mut world, &marker);
    let follow = world.get::<EndpointFollow>(marker_entity).unwrap().clone();
    let region_bounds = |world: &mut World| {
        world
            .query::<(&MediaRegion, &gaanim_scene::LocalBounds)>()
            .single(world)
            .unwrap()
            .1
            .0
    };
    let close = |a: DVec3, b: DVec3| a.distance(b) < 1e-9;
    // Turned a quarter: local (x, y) shows at (1 - y, 1 + x).
    timeline.seek(&mut world, 0.0);
    let at = resolve_tracking_endpoint(&follow.endpoint, &world).unwrap();
    // Pixel (150, 25) is local (1, 0.5).
    assert!(close(at, DVec3::new(0.5, 2.0, 0.0)), "{at:?}");
    // The compiled region already lies on its pixels: local x 0..2, y 0..1.
    let bounds = region_bounds(&mut world);
    assert!(close(bounds.min, DVec3::new(0.0, 1.0, 0.0)), "{bounds:?}");
    assert!(close(bounds.max, DVec3::new(1.0, 3.0, 0.0)), "{bounds:?}");
    timeline.seek(&mut world, 1.0);
    gaanim_animation::media_region_system(&mut world);
    let at = resolve_tracking_endpoint(&follow.endpoint, &world).unwrap();
    // Pixel (150, 25) is now local (0, 0.5).
    assert!(close(at, DVec3::new(0.5, 1.0, 0.0)), "{at:?}");
    let bounds = region_bounds(&mut world);
    assert!(close(bounds.min, DVec3::new(0.0, 0.0, 0.0)), "{bounds:?}");
    assert!(close(bounds.max, DVec3::new(1.0, 2.0, 0.0)), "{bounds:?}");
    let _ = region;
}

#[test]
fn matrix_to_applies_a_linear_map_about_the_pivot() {
    use crate::canvas::{LinearMap2D, LinearMapError};
    assert_eq!(
        LinearMap2D::new([[1.0, 2.0], [2.0, 4.0]]),
        Err(LinearMapError::Singular)
    );
    let rows = [[0.8, 0.3], [-0.5, 1.2]];
    let map = LinearMap2D::new(rows).unwrap();
    let mut canvas = SceneModel::new(640, 360);
    let square = canvas.square(2.0).move_to(1.0, 1.0).matrix_to(map);
    let mirrored = canvas
        .square(2.0)
        .matrix_to(LinearMap2D::new([[-1.0, 0.0], [0.0, 1.0]]).unwrap());
    let animated = canvas.square(2.0);
    canvas.play(vec![animated.animate().matrix_to(map).duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 1.0);
    let apply = |local: DVec3| {
        DVec3::new(
            rows[0][0] * local.x + rows[0][1] * local.y,
            rows[1][0] * local.x + rows[1][1] * local.y,
            0.0,
        )
    };
    for local in [DVec3::new(1.0, 1.0, 0.0), DVec3::new(-1.0, 0.5, 0.0)] {
        let point = transform_of(&mut world, &square)
            .to_mat4()
            .transform_point3(local);
        assert!(
            point.distance(DVec3::new(1.0, 1.0, 0.0) + apply(local)) < 1e-9,
            "{point:?}"
        );
        let point = transform_of(&mut world, &animated)
            .to_mat4()
            .transform_point3(local);
        assert!(point.distance(apply(local)) < 1e-6, "{point:?}");
    }
    let point = transform_of(&mut world, &mirrored)
        .to_mat4()
        .transform_point3(DVec3::new(1.0, 0.5, 0.0));
    assert!(
        point.distance(DVec3::new(-1.0, 0.5, 0.0)) < 1e-9,
        "{point:?}"
    );
}

#[test]
fn rectangular_insets_take_their_own_aspect() {
    use crate::canvas::{CameraInsetOptions, CameraInsetShape, CameraViewError, CanvasEndpoint};
    let mut canvas = SceneModel::new(640, 360);
    let target = CanvasEndpoint::Static(DVec3::new(-3.0, 0.0, 0.0));
    let tall = canvas
        .camera_inset(
            target.clone(),
            CameraInsetOptions {
                size: Some(2.0),
                aspect: Some(0.5),
                shape: CameraInsetShape::Rect,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(matches!(
        canvas.camera_inset(
            target,
            CameraInsetOptions {
                aspect: Some(2.0),
                shape: CameraInsetShape::Circle,
                ..Default::default()
            },
        ),
        Err(CameraViewError::InvalidAspect)
    ));
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.0);
    for part in [tall.screen(), tall.frame()] {
        let id = ObjectId::from_raw(part.id.as_raw() - 1);
        let bounds = world
            .query::<(&MobjectId, &gaanim_scene::LocalBounds)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .0;
        assert!(
            (bounds.width() / bounds.height() - 0.5).abs() < 1e-9,
            "{bounds:?}"
        );
    }
}

#[test]
fn inset_frames_and_connectors_enter_and_leave_with_their_screen() {
    use crate::canvas::{CameraInsetOptions, CanvasEndpoint};

    let mut canvas = SceneModel::new(640, 360);
    let target = CanvasEndpoint::Static(DVec3::new(-3.0, -1.0, 0.0));
    let inset = canvas
        .camera_inset(target.clone(), CameraInsetOptions::default())
        .unwrap();
    canvas.wait(0.5);
    // Declared mid-scene, it still waits for its entry.
    let late = canvas
        .camera_inset(target, CameraInsetOptions::default())
        .unwrap();
    canvas.wait(0.5);
    canvas.play(vec![
        inset.pop_out().duration(1.0),
        late.pop_out().duration(1.0),
    ]);
    canvas.play(vec![inset.pop_in().duration(1.0)]);
    canvas.wait(1.0);
    canvas.play(vec![inset.pop_out().duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    let parts = |view: &crate::canvas::CameraViewHandle| {
        let mut parts = vec![view.screen().clone(), view.frame().clone()];
        parts.extend(view.connectors().iter().cloned());
        parts
    };
    for (time, shown) in [
        (0.25, false),
        (1.5, true),
        (2.5, true),
        (3.5, false),
        (4.5, true),
    ] {
        timeline.seek(&mut world, time);
        for part in parts(&inset) {
            let opacity = opacity_of(&mut world, &part);
            assert_eq!(opacity > 0.5, shown, "{time}: {opacity}");
        }
    }
    for (time, shown) in [(0.75, false), (1.5, true)] {
        timeline.seek(&mut world, time);
        for part in parts(&late) {
            let opacity = opacity_of(&mut world, &part);
            assert_eq!(opacity > 0.5, shown, "{time}: {opacity}");
        }
    }
}

#[test]
fn dimensions_are_visible_from_their_declaration_without_an_entry() {
    use crate::canvas::{CanvasEndpoint, DimensionOptions};

    let mut canvas = SceneModel::new(640, 360);
    let endpoint = |x: f64, y: f64| CanvasEndpoint::Static(DVec3::new(x, y, 0.0));
    // Declared before the labelled one: `opacity_of` maps handle ids to
    // runtime ids, which the label's glyphs shift.
    let entering = canvas
        .dimension_between_with_options(
            endpoint(1.0, 0.0),
            endpoint(4.0, 0.0),
            0.3,
            DimensionOptions::default(),
        )
        .unwrap();
    let still = canvas
        .dimension_between_with_options(
            endpoint(-5.5, 0.1),
            endpoint(-2.5, 0.1),
            0.3,
            DimensionOptions {
                label: Some("control".into()),
                ..Default::default()
            },
        )
        .unwrap();
    canvas.wait(1.0);
    canvas.play(vec![entering.drawable.animate().fade_in().duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    // Opacity is inherited, so the entry fades the group itself.
    timeline.seek(&mut world, 0.5);
    for part in [&still.line, &still.extensions] {
        assert!(opacity_of(&mut world, part) > 0.99);
    }
    assert!(opacity_of(&mut world, &entering.drawable) < 1e-6);
    timeline.seek(&mut world, 2.0);
    for part in [&entering.drawable, &entering.extensions] {
        assert!(opacity_of(&mut world, part) > 0.99);
    }
}

#[test]
fn scale_entries_pin_their_box_point_with_absolute_geometry_and_pivots() {
    let mut canvas = SceneModel::new(640, 360);
    // Declared in absolute coordinates: the pivot stays at the origin.
    let triangle = canvas.polygon(vec![(3.0, 1.0), (5.0, 1.0), (4.0, 3.0)]);
    let square = canvas.square(1.0).with_pivot(0.3, 0.2).move_to(-2.0, -2.0);
    canvas.play(vec![
        triangle.animate().grow_from_center().duration(1.0),
        square
            .animate()
            .grow_from_edge(gaanim_layout::Direction::Down)
            .duration(1.0),
    ]);
    canvas.play(vec![triangle.animate().scale_by(2.0).duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);

    let world_point = |world: &mut World, handle: &DrawableHandle, local: DVec3| {
        transform_of(world, handle)
            .to_mat4()
            .transform_point3(local)
    };
    for (handle, local) in [
        (&triangle, DVec3::new(4.0, 2.0, 0.0)),
        (&square, DVec3::new(0.0, -0.5, 0.0)),
    ] {
        timeline.seek(&mut world, 1.0);
        let pinned = world_point(&mut world, handle, local);
        for time in [0.1, 0.5, 0.9] {
            timeline.seek(&mut world, time);
            let point = world_point(&mut world, handle, local);
            assert!(
                point.distance(pinned) < 1e-6,
                "{point:?} drifted from {pinned:?} at {time}"
            );
        }
    }
    timeline.seek(&mut world, 2.0);
    assert!((transform_of(&mut world, &triangle).scale.x - 2.0).abs() < 1e-6);
}

#[test]
fn trims_chain_from_the_current_window() {
    let mut canvas = SceneModel::new(640, 360);
    let ring = canvas.circle(1.0).trim(Some(0.5), Some(0.5), None, None);
    canvas.play(vec![ring.animate().trim(Some(0.0), Some(1.0), None)]);
    canvas.play(vec![ring.animate().trim(None, None, Some(0.25))]);
    let timeline = compiled_timeline(&canvas);
    let mut trims: Vec<_> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::PathTrim { from, to, .. },
                    ..
                },
            ) => Some((clip.start, clip.duration, *from, *to)),
            _ => None,
        })
        .collect();
    trims.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    assert_eq!(
        trims
            .iter()
            .map(|trim| (trim.2, trim.3))
            .collect::<Vec<_>>(),
        [
            ([0.0, 1.0, 0.0], [0.5, 0.5, 0.0]),
            ([0.5, 0.5, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 1.0, 0.25]),
        ]
    );
    assert_eq!(trims[0].1, 0.0);
}

#[test]
fn dash_offset_animations_continue_from_the_current_offset() {
    let mut canvas = SceneModel::new(640, 360);
    let style = gaanim_core::kurbo::Stroke::new(0.05).with_dashes(0.5, [0.2, 0.1]);
    let ring = canvas
        .circle(1.0)
        .stroke_with_style(Brush::Solid(PenikoColor::WHITE), style);
    canvas.play(vec![ring.animate().dash_offset(2.0)]);
    canvas.play(vec![ring.animate().dash_offset(3.0)]);
    let timeline = compiled_timeline(&canvas);
    let mut lenses: Vec<_> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Dynamic(lens),
                    ..
                },
            ) => Some((clip.start, format!("{lens:?}"))),
            _ => None,
        })
        .collect();
    lenses.sort_by(|a, b| a.0.total_cmp(&b.0));
    let lenses: Vec<_> = lenses.into_iter().map(|(_, lens)| lens).collect();
    assert_eq!(
        lenses,
        [
            "DashOffsetLens { from: 0.5, to: 2.0 }",
            "DashOffsetLens { from: 2.0, to: 3.0 }",
        ]
    );
}

#[test]
fn parameter_driven_spin_integrates_the_parameter_tweens() {
    let mut canvas = SceneModel::new(640, 360);
    let wheel = canvas.circle(1.0);
    let speed = canvas.parameter(1.0).unwrap();
    canvas.wait(1.0);
    wheel.add_updater(crate::canvas::UpdaterPreset::Procedural(
        gaanim_animation::ProceduralLayer::Spin {
            speed: gaanim_animation::LayerParam::signal(speed.drawable().id),
        },
    ));
    canvas.wait(1.0);
    canvas.play(vec![speed.animate().set(3.0).duration(2.0)]);
    canvas.wait(1.0);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);

    let entity = entity_of(&mut world, &wheel);
    let motion = world
        .get::<gaanim_animation::ProceduralMotion>(entity)
        .unwrap();
    // 1 rad/s over [1, 2], a symmetric ease from 1 to 3 rad/s over
    // [2, 4] (2 rad/s on average), then 3 rad/s.
    assert!((motion.offset_at(2.0).rotation - 1.0).abs() < 1e-9);
    assert!((motion.offset_at(4.0).rotation - 5.0).abs() < 1e-6);
    assert!((motion.offset_at(5.0).rotation - 8.0).abs() < 1e-6);
}

#[test]
fn periodic_and_spinning_presets_stack_as_layers_of_timeline_time() {
    use crate::canvas::UpdaterPreset;
    let mut canvas = SceneModel::new(640, 360);
    let square = canvas.square(1.0);
    canvas.wait(1.0);
    square.add_updater(UpdaterPreset::Bob {
        amplitude: 0.5,
        frequency: 0.25,
    });
    square.add_updater(UpdaterPreset::Pulse {
        min_scale: 1.0,
        max_scale: 1.5,
        frequency: 0.5,
    });
    square.add_updater(UpdaterPreset::Rotate { speed: 1.5 });
    canvas.wait(4.0);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);

    let entity = entity_of(&mut world, &square);
    let motion = world
        .get::<gaanim_animation::ProceduralMotion>(entity)
        .unwrap();
    // The updaters' functions of the time since they were attached,
    // all three at once.
    for time in [1.0, 1.7, 2.0, 3.3, 4.9] {
        let local = time - 1.0;
        let offset = motion.offset_at(time);
        let bob = 0.5 * (std::f64::consts::TAU * 0.25 * local).sin();
        let pulse = 1.25 + 0.25 * (std::f64::consts::TAU * 0.5 * local).sin();
        assert!((offset.translation.y - bob).abs() < 1e-9, "bob at {time}");
        assert!((offset.scale - pulse).abs() < 1e-9, "pulse at {time}");
        assert!(
            (offset.rotation - 1.5 * local).abs() < 1e-9,
            "spin at {time}"
        );
    }
    assert!(world.get::<gaanim_animation::Updater>(entity).is_none());
}

#[test]
fn dash_flow_updaters_reach_every_drawn_member() {
    let mut canvas = SceneModel::new(640, 360);
    let first = canvas.circle(1.0);
    let second = canvas.square(1.0);
    let pipes = canvas.group(&[&first, &second]);
    canvas.wait(1.0);
    pipes.add_updater(crate::canvas::UpdaterPreset::DashFlow { speed: 0.5 });
    canvas.wait(2.0);
    pipes.remove_updater();

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);

    for member in [&first, &second] {
        let entity = entity_of(&mut world, member);
        let flow = world.get::<gaanim_animation::DashFlow>(entity).unwrap();
        assert_eq!(flow.runs, [(0.5, 1.0, Some(3.0))]);
    }
}

#[test]
fn effect_animations_continue_from_the_current_effects() {
    use crate::canvas::{DropShadow, Glow};
    let mut canvas = SceneModel::new(640, 360);
    let card = canvas.rect(2.0, 1.0).shadow(
        PenikoColor::BLACK,
        gaanim_core::glam::DVec2::new(0.0, -0.05),
        0.05,
    );
    let glow = Glow {
        radius: 0.5,
        intensity: 2.0,
        color: PenikoColor::WHITE,
    };
    let lifted = DropShadow {
        color: PenikoColor::BLACK,
        offset: gaanim_core::glam::DVec2::new(0.0, -0.25),
        blur_radius: 0.4,
    };
    canvas.play(vec![
        card.animate()
            .shadow(Some(lifted.clone()))
            .glow(Some(glow.clone()))
            .scale_to(1.04),
    ]);
    canvas.play(vec![card.animate().glow(None)]);
    let timeline = compiled_timeline(&canvas);
    let mut lenses: Vec<_> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Dynamic(lens),
                    ..
                },
            ) => Some((clip.start, format!("{lens:?}"))),
            _ => None,
        })
        .collect();
    lenses.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(lenses.len(), 2);
    assert!(lenses[0].1.contains("blur_radius: 0.05"), "{}", lenses[0].1);
    assert!(lenses[0].1.contains("blur_radius: 0.4"));
    // The second clip starts from the glow and keeps the lifted shadow.
    assert!(lenses[1].1.contains("intensity: 2.0"));
    assert!(lenses[1].1.contains("to: EffectState { glow: None"));
    assert!(lenses[1].1.matches("blur_radius: 0.4").count() == 2);
}

#[test]
fn spatial_stagger_delays_follow_distance_from_the_origin() {
    use crate::canvas::{Composition, StaggerLayout, StaggerOrigin};
    let mut canvas = SceneModel::new(640, 360);
    let dots: Vec<_> = (0..5)
        .map(|index| canvas.circle(0.1).move_to(index as f64 - 2.0, 0.0))
        .collect();
    let starts = |origin, total| {
        let children = dots
            .iter()
            .map(|dot| Composition::leaf(dot.animate().opacity(0.5).duration(1.0)))
            .collect();
        let layout = StaggerLayout {
            origin,
            grid: None,
            total,
            easing: None,
        };
        let schedule = Composition::stagger_layout(children, 0.1, layout)
            .unwrap()
            .schedule(None)
            .unwrap();
        schedule
            .entries
            .iter()
            .map(|entry| (entry.start * 1000.0).round() / 1000.0)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        starts(StaggerOrigin::Start, None),
        [0.0, 0.1, 0.2, 0.3, 0.4]
    );
    assert_eq!(
        starts(StaggerOrigin::Center, None),
        [0.2, 0.1, 0.0, 0.1, 0.2]
    );
    assert_eq!(
        starts(StaggerOrigin::Edges, None),
        [0.0, 0.1, 0.2, 0.1, 0.0]
    );
    assert_eq!(
        starts(StaggerOrigin::Point(-2.0, 0.0), Some(2.0)),
        [0.0, 0.5, 1.0, 1.5, 2.0]
    );
    let mut random = starts(StaggerOrigin::Random(7), None);
    assert_eq!(random, starts(StaggerOrigin::Random(7), None));
    random.sort_by(f64::total_cmp);
    assert_eq!(random, [0.0, 0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn repeated_compositions_report_their_expanded_schedule() {
    use crate::canvas::Composition;
    let mut canvas = SceneModel::new(640, 360);
    let dot = canvas.circle(0.2);
    let spin = Composition::leaf(dot.animate().rotate_by(1.0).duration(1.0).repeat(
        3,
        gaanim_math::RepeatMode::Cycle,
        0.5,
    ));
    assert_eq!(spin.schedule(None).unwrap().span, 4.0);
    let pair = Composition::parallel(vec![
        Composition::leaf(dot.animate().shift_by(1.0, 0.0).duration(1.0)),
        Composition::leaf(canvas.circle(0.1).animate().rotate_by(1.0).duration(0.5)),
    ])
    .unwrap()
    .repeat(2, 0.5)
    .unwrap();
    let schedule = pair.schedule(None).unwrap();
    assert_eq!(schedule.span, 2.5);
    assert_eq!(schedule.entries.len(), 4);
    canvas
        .play_composition_configured(pair, None, None)
        .unwrap();
    let clips = translation_clips(&compiled_timeline(&canvas));
    assert_eq!(clips.len(), 2);
    assert_eq!(clips[1].0, 1.5);
    assert_eq!(clips[1].2, clips[0].3);
}

fn write_start_times(anim: impl FnOnce(&DrawableHandle) -> crate::canvas::Anim) -> Vec<f64> {
    let mut canvas = SceneModel::new(640, 360);
    let text = canvas.text("uno bb dos");
    canvas.play(vec![anim(&text).duration(1.0)]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut starts: Vec<f64> = timeline
        .clips
        .values()
        .filter(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::PathCompletion {
                            from,
                            to,
                        } | gaanim_timeline::clip::PropertyLensSpec::PathPartial {
                            from,
                            to,
                        },
                        ..
                    }
                ) if *from == 0.0 && *to == 1.0
            )
        })
        .map(|clip| clip.start)
        .collect();
    starts.sort_by(f64::total_cmp);
    starts
}

fn distinct_counts(starts: &[f64]) -> Vec<usize> {
    let mut counts: Vec<usize> = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        if index > 0 && (start - starts[index - 1]).abs() < 1e-9 {
            *counts.last_mut().unwrap() += 1;
        } else {
            counts.push(1);
        }
    }
    counts
}

#[test]
fn write_lasts_and_draws_each_piece_as_manim_does() {
    use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};
    let mut canvas = SceneModel::new(640, 360);
    let short = canvas.text("I");
    let long = canvas.text("Sphinx of black quartz");
    // 1 s for fewer than 15 pieces, 2 s from 15, unless a duration is set.
    assert_eq!(short.animate().write().into_builder().duration, 1.0);
    assert_eq!(long.animate().write().into_builder().duration, 2.0);
    assert_eq!(long.animate().unwrite().into_builder().duration, 2.0);
    assert_eq!(
        long.animate().write().duration(0.5).into_builder().duration,
        0.5
    );
    canvas.play(vec![short.animate().write()]);

    let timeline = compiled_timeline(&canvas);
    let clips: Vec<(f64, f64, PropertyLensSpec, RateFunc)> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(AnimationSpec {
                lens, rate_func, ..
            }) if clip.duration > 0.5 => {
                Some((clip.start, clip.duration, lens.clone(), rate_func.clone()))
            }
            _ => None,
        })
        .collect();
    let half = |rate: &RateFunc, from: f64, to: f64| matches!(rate, RateFunc::Squish { start, end, .. } if *start == from && *end == to);
    // The one piece spans the second: its outline over the first half...
    assert!(clips.iter().any(|(start, duration, lens, rate)| {
            *start == 0.0
                && *duration == 1.0
                && matches!(lens, PropertyLensSpec::PathPartial { from, to } if *from == 0.0 && *to == 1.0)
                && half(rate, 0.0, 0.5)
        }));
    // ...then its fill, while the outline thins out to its own (none).
    assert!(clips.iter().any(|(_, _, lens, rate)| {
            matches!(lens, PropertyLensSpec::FillDrawProgress { from, to } if *from == 0.0 && *to == 1.0)
                && half(rate, 0.5, 1.0)
        }));
    assert!(clips.iter().any(|(_, _, lens, rate)| {
        matches!(lens, PropertyLensSpec::StrokeWidth { from, to } if *from > 0.0 && *to == 0.0)
            && half(rate, 0.5, 1.0)
    }));
}

#[test]
fn write_by_word_starts_each_word_together() {
    let by_glyph = write_start_times(|text| text.animate().write());
    assert_eq!(distinct_counts(&by_glyph), [1; 8]);

    let by_word =
        write_start_times(|text| text.animate().write().reveal_unit(TextRevealUnit::Word));
    assert_eq!(distinct_counts(&by_word), [3, 2, 3]);

    // Center order starts the middle word "bb" first, then both sides.
    let centered = write_start_times(|text| {
        text.animate()
            .write()
            .reveal_unit(TextRevealUnit::Word)
            .draw_order(crate::anim::DrawOrder::Center)
    });
    assert_eq!(distinct_counts(&centered), [2, 6]);
}

#[test]
fn equation_expansion_morphs_semantic_terms_without_cross_fading_pairs() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.math_text("E = m c^2").define_tag("mass", "m", None);
    let target = canvas
        .math_text("E = (m_1 + m_2) c^2")
        .define_tag("mass", "(m_1 + m_2)", None);
    let expansion = source.expand_to(&target, "mass", 0.8).unwrap();
    canvas.play(vec![expansion]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let semantic_morphs = timeline
        .clips
        .values()
        .filter(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::PathMorph { .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationSemanticMorph"
            )
        })
        .count();
    assert!(semantic_morphs >= 3, "shared and tagged terms should morph");
    assert!(timeline.clips.values().all(|clip| {
        !matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                    label: Some(label),
                    ..
                }
            ) if clip.duration > 0.0 && label == "EquationSemanticMorph"
        )
    }));
    assert!(timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Scale { from, to },
                    label: Some(label),
                    ..
                }
            ) if label == "EquationEmerge"
                && (clip.start - 0.16).abs() < 1e-9
                && *from == gaanim_core::glam::DVec3::ZERO
                && *to != gaanim_core::glam::DVec3::ZERO
        )
    }));
    assert!(timeline.clips.values().all(|clip| {
        !matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                    label: Some(label),
                    ..
                }
            ) if label == "EquationEmerge" && clip.duration > 0.0
        )
    }));
}

#[test]
fn equation_step_prioritizes_semantic_tags_then_matches_common_glyphs() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas
        .math_text("x + 3 = 7")
        .define_tag("result", "7", None);
    let target = canvas.math_text("x = 4").define_tag("result", "4", None);
    let step = source
        .step_to(
            &target,
            Some(vec![("result".to_string(), "result".to_string())]),
            0.8,
        )
        .unwrap();
    canvas.play(vec![step]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let semantic_morphs = timeline
        .clips
        .values()
        .filter(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::PathMorph { .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationSemanticMorph"
            )
        })
        .count();
    assert!(
        semantic_morphs >= 3,
        "x and = should auto-match while the result tag forces 7 -> 4"
    );
    let handoffs = timeline
        .clips
        .values()
        .filter(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                        label: Some(label),
                        ..
                    }
                ) if label == "EquationHandoff" && clip.duration == 0.0
            )
        })
        .count();
    assert!(handoffs >= semantic_morphs * 2);
    assert!(timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Scale { to, .. },
                    label: Some(label),
                    ..
                }
            ) if label == "EquationCollapse"
                && *to == gaanim_core::glam::DVec3::ZERO
        )
    }));
    assert!(timeline.clips.values().all(|clip| {
        !matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                    label: Some(label),
                    ..
                }
            ) if label == "EquationCollapse" && clip.duration > 0.0
        )
    }));
}

#[test]
fn equation_step_handoff_is_exact_and_target_remains_animatable() {
    let mut canvas = SceneModel::new(640, 360);
    let equation = |middle: &str, result: &str| {
        StructuredTextSpec::new(
            vec![
                "$".into(),
                gaanim_text::prelude::TextPart::new(
                    "variable",
                    vec!["x".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                middle.into(),
                gaanim_text::prelude::TextPart::new(
                    "result",
                    vec![result.into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                "$".into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid structured equation")
    };
    let source = canvas.text_spec(equation(" dot 5 = ", "25")).scale_to(2.0);
    let target = canvas.text_spec(equation(" = ", "5")).scale_to(2.0);
    let step = source.step_to(&target, None, 0.8).unwrap();
    canvas.play(vec![step]);
    canvas.play(vec![
        target.tag("result").expect("result tag").indicate(0.4),
    ]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.8);

    let child_opacities: Vec<Vec<f32>> = world
        .query::<(
            Option<&bevy::prelude::ChildOf>,
            Option<&bevy::prelude::Children>,
        )>()
        .iter(&world)
        .filter_map(|(parent, children)| {
            (parent.is_none()).then_some(children?).map(|children| {
                children
                    .iter()
                    .filter_map(|child| world.get::<Opacity>(child).map(|opacity| opacity.0))
                    .collect()
            })
        })
        .collect();
    assert_eq!(child_opacities.len(), 2);
    assert!(
        child_opacities
            .iter()
            .any(|values| values.iter().all(|value| *value == 0.0))
    );
    assert!(
        child_opacities
            .iter()
            .any(|values| values.iter().all(|value| *value > 0.99))
    );

    timeline.seek(&mut world, 1.0);
    assert!(
        world
            .query::<&Opacity>()
            .iter(&world)
            .filter(|opacity| opacity.0 > 0.99)
            .count()
            >= 3
    );

    timeline.seek(&mut world, 1.2);
    let child_opacities: Vec<Vec<f32>> = world
        .query::<(
            Option<&bevy::prelude::ChildOf>,
            Option<&bevy::prelude::Children>,
        )>()
        .iter(&world)
        .filter_map(|(parent, children)| {
            (parent.is_none()).then_some(children?).map(|children| {
                children
                    .iter()
                    .filter_map(|child| world.get::<Opacity>(child).map(|opacity| opacity.0))
                    .collect()
            })
        })
        .collect();
    assert!(
        child_opacities
            .iter()
            .any(|values| !values.is_empty() && values.iter().all(|value| *value > 0.99)),
        "all target glyphs must remain visible after animating one selection: {child_opacities:?}"
    );
}

#[test]
fn equation_part_colors_survive_morph_handoff_and_seek() {
    let red = PenikoColor::from_rgb8(0xef, 0x44, 0x44);
    let equation = |term: &str| {
        StructuredTextSpec::new(
            vec![
                "$".into(),
                gaanim_text::prelude::TextPart::new(
                    "term",
                    vec![term.into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                " + 1 = 2$".into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid structured equation")
    };
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.text_spec(equation("x"));
    let target = canvas.text_spec(equation("theta''"));
    assert!(target.fill_text_part(&["term".to_owned()], red));
    let target_spec = target.text_spec().expect("target text spec");
    let target_source = structured_text_typst_source(
        &target_spec,
        Some(640.0),
        48.0,
        "New Computer Modern",
        PenikoColor::BLACK,
    );
    assert!(target_source.contains("ef4444"), "{target_source}");
    canvas.play(vec![source.morph_to(&target, 0.8).unwrap()]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    assert!(
            world
                .query::<&gaanim_scene::FillBrush>()
                .iter(&world)
                .any(|fill| matches!(fill.0.as_ref(), Some(gaanim_core::peniko::Brush::Solid(color)) if *color == red)),
            "the semantic target must compile with its authored part fill"
        );
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.8);

    let red_states = world
            .query::<(
                bevy::prelude::Entity,
                &Opacity,
                &gaanim_scene::FillBrush,
                Option<&bevy::prelude::ChildOf>,
                Option<&gaanim_scene::ObjectTag>,
            )>()
            .iter(&world)
            .filter_map(|(entity, opacity, fill, parent, tag)| {
                matches!(fill.0.as_ref(), Some(gaanim_core::peniko::Brush::Solid(color)) if *color == red)
                    .then_some((entity, opacity.0, parent.map(|parent| parent.parent()), tag.map(|tag| tag.0.clone())))
            })
            .collect::<Vec<_>>();

    assert!(
        world
            .query::<(&Opacity, &gaanim_scene::FillBrush)>()
            .iter(&world)
            .any(|(opacity, fill)| {
                opacity.0 > 0.99
                    && matches!(
                        fill.0.as_ref(),
                        Some(gaanim_core::peniko::Brush::Solid(color)) if *color == red
                    )
            }),
        "the visible target term must retain its selected fill after the handoff: {red_states:?}"
    );
}

#[test]
fn text_step_does_not_mutate_unrelated_written_text() {
    let mut canvas = SceneModel::new(640, 360);
    let title = canvas
        .text_spec(
            StructuredTextSpec::new(
                vec!["Resolver paso a paso".into()],
                Some(gaanim_text::prelude::TextRole::Title),
                StructuredTextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid title"),
        )
        .move_to(0.0, 120.0);
    let equation = |middle: &str, result: &str| {
        StructuredTextSpec::new(
            vec![
                "$".into(),
                gaanim_text::prelude::TextPart::new(
                    "variable",
                    vec!["x".into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                middle.into(),
                gaanim_text::prelude::TextPart::new(
                    "result",
                    vec![result.into()],
                    StructuredTextStyle::default(),
                )
                .into(),
                "$".into(),
            ],
            None,
            StructuredTextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("valid structured equation")
    };
    let source = canvas.text_spec(equation(" dot 5 = ", "25")).scale_to(2.0);
    let target = canvas.text_spec(equation(" = ", "5")).scale_to(2.0);
    canvas.play(vec![title.write(1.0), source.write(1.0)]);
    canvas.wait(0.4);
    canvas.play(vec![source.step_to(&target, None, 0.8).unwrap()]);
    canvas.play(vec![
        target.tag("result").expect("result tag").indicate(0.45),
    ]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    let title_children = world
        .query::<(
            &MobjectId,
            &ObjectTag,
            Option<&bevy::prelude::ChildOf>,
            Option<&bevy::prelude::Children>,
        )>()
        .iter(&world)
        .find_map(|(_, tag, parent, children)| {
            (parent.is_none() && tag.0.contains("Resolver paso a paso"))
                .then_some(children?.iter().collect::<Vec<_>>())
        })
        .expect("compiled title root");
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 1.4);
    let title_paths = title_children
        .iter()
        .map(|child| {
            world
                .get::<gaanim_scene::Path2D>(*child)
                .expect("title glyph path")
                .clone()
        })
        .collect::<Vec<_>>();
    for time in [1.8, 2.2] {
        timeline.seek(&mut world, time);
        assert!(
            title_children.iter().enumerate().all(|(index, child)| {
                world
                    .get::<Opacity>(*child)
                    .is_some_and(|opacity| opacity.0 > 0.99)
                    && world
                        .get::<gaanim_animation::writing::FillDrawProgress>(*child)
                        .is_none_or(|progress| progress.0 > 0.99)
                    && world.get::<gaanim_scene::Path2D>(*child) == title_paths.get(index)
            }),
            "title glyphs must remain fully drawn during the equation transition at {time}s"
        );
    }
}

#[test]
fn tagged_equation_copy_keeps_opacity_changes_instantaneous() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas
        .math_text("E = m c^2")
        .define_tag("mass", "m", None)
        .move_to(0.0, 70.0);
    let target = canvas
        .math_text("p = m v")
        .define_tag("mass", "m", None)
        .move_to(0.0, -90.0);
    let copy = source
        .tag("mass")
        .unwrap()
        .copy_to(&target.tag("mass").unwrap(), 0.8)
        .expect("selections share a SceneModel");
    canvas.play(vec![copy]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    assert!(timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::PathMorph { .. },
                    label: Some(label),
                    ..
                }
            ) if label == "EquationSemanticMorph"
        )
    }));
    assert!(timeline.clips.values().all(|clip| {
        !matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { .. },
                    ..
                }
            ) if clip.duration > 0.0
        )
    }));
}

#[test]
fn tagged_equation_copy_preserves_both_visible_equations_after_seek() {
    let mut canvas = SceneModel::new(640, 360);
    let title = canvas.text("One variable");
    let source = canvas
        .math_text("E = m c^2")
        .define_tag("mass", "m", None)
        .move_to(0.0, 70.0);
    let target = canvas
        .math_text("p = m v")
        .define_tag("mass", "m", None)
        .move_to(0.0, -90.0);
    canvas.play(vec![
        title.write(1.0),
        source.write(1.0),
        target.fade_in(1.0),
    ]);
    canvas.wait(0.5);
    let copy = source
        .tag("mass")
        .unwrap()
        .copy_to(&target.tag("mass").unwrap(), 0.9)
        .expect("selections share a SceneModel");
    canvas.play(vec![copy]);
    canvas.wait(0.25);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 2.55);

    let textual_roots: Vec<_> = world
        .query::<(
            bevy::prelude::Entity,
            Option<&bevy::prelude::ChildOf>,
            Option<&bevy::prelude::Children>,
        )>()
        .iter(&world)
        .filter_map(|(entity, parent, children)| {
            (parent.is_none() && children.is_some()).then_some(entity)
        })
        .collect();
    assert_eq!(textual_roots.len(), 3);
    for root in textual_roots {
        let children = world.get::<bevy::prelude::Children>(root).unwrap();
        assert!(children.iter().all(|child| {
            world
                .get::<Opacity>(child)
                .is_some_and(|opacity| opacity.0 > 0.99)
        }));
    }
}

#[test]
fn fade_in_from_down_schedules_opacity_and_upward_translation() {
    let mut canvas = SceneModel::new(640, 360);
    let label = canvas.text("Aparece desde abajo");
    let entrance = label
        .animate()
        .fade_in_from(crate::canvas::Direction::Down, 72.0)
        .duration(0.8);
    canvas.play(vec![entrance]);

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    assert!(timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { from, to },
                    ..
                }
            ) if *from == 0.0 && *to == 1.0
        )
    }));
    assert!(timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    lens: gaanim_timeline::clip::PropertyLensSpec::Translation { from, to },
                    ..
                }
            ) if from.y < to.y
        )
    }));
}

#[test]
fn text_with_inline_math_compiles_to_vector_glyphs() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.text("Energia $E = m c^2$ es famosa");
    canvas.text("Valor $1/2$ y $sqrt(2)$");
    canvas.text("Angulo $alpha + beta$");
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);
    let fills: Vec<_> = world
        .query::<&gaanim_scene::FillBrush>()
        .iter(&world)
        .filter_map(|f| f.0.clone())
        .collect();
    assert!(
        fills.len() > 10,
        "text with inline math should produce vector glyphs, got {}",
        fills.len()
    );
    assert!(
        fills.iter().all(|b| match b {
            gaanim_core::peniko::Brush::Solid(c) => *c != gaanim_core::peniko::Color::BLACK,
            _ => true,
        }),
        "inline math glyphs should not be black on black background"
    );
    assert!(
        world
            .query::<&LocalBounds>()
            .iter(&world)
            .any(|b| b.0.width() > 0.5)
    );
}

#[test]
fn paragraph_with_inline_math_compiles_to_vector_glyphs() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.configured_text(
        "La energia $E = m c^2$ relaciona masa y energia en $x^2 + y^2 = z^2$.",
        gaanim_text::prelude::TextStyle {
            size: Some(28.0),
            ..Default::default()
        },
        gaanim_text::prelude::TextFlow {
            wrap: gaanim_text::prelude::TextWrap::Width(500.0),
            ..Default::default()
        },
    );
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    let mut world = world;
    queue.apply(&mut world);
    let fills: Vec<_> = world
        .query::<&gaanim_scene::FillBrush>()
        .iter(&world)
        .filter_map(|f| f.0.clone())
        .collect();
    assert!(
        fills.len() > 15,
        "paragraph with inline math should produce vector glyphs, got {}",
        fills.len()
    );
    assert!(
        fills.iter().all(|b| match b {
            gaanim_core::peniko::Brush::Solid(c) => *c != gaanim_core::peniko::Color::BLACK,
            _ => true,
        }),
        "inline math paragraph glyphs should not be black on black background"
    );
}

#[test]
fn compiled_text_measure_rewraps_at_the_offered_width() {
    let id = gaanim_layout::LayoutId(7);
    let fonts = gaanim_text::font::FontRegistry::new();
    let measurer = CompiledLayoutMeasure {
            fixed: BTreeMap::new(),
            texts: BTreeMap::from([(
                id,
                CompiledTextMeasure {
                    spec: StructuredTextSpec::new(
                        vec!["Responsive text measures its true wrapped height before any ECS entity is materialized.".into()],
                        None,
                        gaanim_text::prelude::TextStyle::default(),
                        gaanim_text::prelude::TextFlow::default(),
                    ).unwrap(),
                    font_size: 28.0,
                    font_family: "New Computer Modern".into(),
                    math_font: "New Computer Modern Math".into(),
                    color: gaanim_core::peniko::Color::WHITE,
                },
            )]),
            text_compositions: RefCell::default(),
            text_candidates: RefCell::default(),
            line_extents: RefCell::default(),
            cap_heights: RefCell::default(),
            natural_text_sizes: RefCell::default(),
            font_registry: &fonts,
        };
    let narrow = gaanim_layout::IntrinsicMeasure::measure(
        &measurer,
        id,
        gaanim_layout::BoxConstraints {
            min: DVec2::ZERO,
            max: DVec2::new(180.0, 1000.0),
        },
    )
    .unwrap();
    let wide = gaanim_layout::IntrinsicMeasure::measure(
        &measurer,
        id,
        gaanim_layout::BoxConstraints {
            min: DVec2::ZERO,
            max: DVec2::new(520.0, 1000.0),
        },
    )
    .unwrap();

    assert!(narrow.y > wide.y, "narrow={narrow:?}, wide={wide:?}");
    assert!(gaanim_layout::IntrinsicMeasure::is_width_sensitive(
        &measurer, id
    ));
}

#[test]
fn auto_text_measure_preserves_the_width_used_for_composition() {
    let fonts = gaanim_text::font::FontRegistry::new();
    let cases = [
        ("Titulo de presentacion", 64.0),
        ("Hola mundo", 48.0),
        ("Texto Normal", 32.0),
        ("$integral alpha d t + 2 = 0$", 32.0),
    ];

    for (index, (content, font_size)) in cases.into_iter().enumerate() {
        let id = gaanim_layout::LayoutId(index as u64 + 1);
        let measurer = CompiledLayoutMeasure {
            fixed: BTreeMap::new(),
            texts: BTreeMap::from([(
                id,
                CompiledTextMeasure {
                    spec: StructuredTextSpec::new(
                        vec![content.into()],
                        None,
                        gaanim_text::prelude::TextStyle::default(),
                        gaanim_text::prelude::TextFlow::default(),
                    )
                    .unwrap(),
                    font_size,
                    font_family: "New Computer Modern".into(),
                    math_font: "New Computer Modern Math".into(),
                    color: gaanim_core::peniko::Color::WHITE,
                },
            )]),
            text_compositions: RefCell::default(),
            text_candidates: RefCell::default(),
            line_extents: RefCell::default(),
            cap_heights: RefCell::default(),
            natural_text_sizes: RefCell::default(),
            font_registry: &fonts,
        };
        let wide = gaanim_layout::IntrinsicMeasure::measure(
            &measurer,
            id,
            gaanim_layout::BoxConstraints {
                min: DVec2::ZERO,
                max: DVec2::new(1760.0, 1000.0),
            },
        )
        .unwrap();
        assert!(wide.x < 1760.0, "fixture should have tight visual bounds");
        assert_eq!(
            measurer.text_compositions.borrow().get(&id),
            Some(&TextComposition::Natural),
            "{content:?} fits unwrapped and must be materialized unwrapped"
        );
        // A hugging parent offers exactly the measured ink width next; the
        // text must keep its single line instead of breaking again.
        let hugged = gaanim_layout::IntrinsicMeasure::measure(
            &measurer,
            id,
            gaanim_layout::BoxConstraints {
                min: DVec2::ZERO,
                max: DVec2::new(wide.x, 1000.0),
            },
        )
        .unwrap();
        assert_eq!(hugged, wide, "{content:?} rewrapped at its own ink width");
        assert_eq!(
            measurer.text_compositions.borrow().get(&id),
            Some(&TextComposition::Natural)
        );
    }
}

#[test]
fn wrapped_text_keeps_its_lines_when_offered_its_ink_width() {
    let id = gaanim_layout::LayoutId(3);
    let fonts = gaanim_text::font::FontRegistry::new();
    let measurer = CompiledLayoutMeasure {
            fixed: BTreeMap::new(),
            texts: BTreeMap::from([(
                id,
                CompiledTextMeasure {
                    spec: StructuredTextSpec::new(
                        vec!["A wrapped paragraph inside a hugging card keeps the lines it was measured with.".into()],
                        None,
                        gaanim_text::prelude::TextStyle::default(),
                        gaanim_text::prelude::TextFlow::default(),
                    )
                    .unwrap(),
                    font_size: 28.0,
                    font_family: "New Computer Modern".into(),
                    math_font: "New Computer Modern Math".into(),
                    color: gaanim_core::peniko::Color::WHITE,
                },
            )]),
            text_compositions: RefCell::default(),
            text_candidates: RefCell::default(),
            line_extents: RefCell::default(),
            cap_heights: RefCell::default(),
            natural_text_sizes: RefCell::default(),
            font_registry: &fonts,
        };
    let measure = |width: f64| {
        gaanim_layout::IntrinsicMeasure::measure(
            &measurer,
            id,
            gaanim_layout::BoxConstraints {
                min: DVec2::ZERO,
                max: DVec2::new(width, 1000.0),
            },
        )
        .unwrap()
    };
    let first = measure(400.0);
    assert!(
        first.x < 400.0,
        "fixture must wrap with ragged ink: {first:?}"
    );
    let hugged = measure(first.x);
    assert_eq!(hugged, first, "offering the ink width must not add lines");
    assert_eq!(
        measurer.text_compositions.borrow().get(&id).copied(),
        Some(TextComposition::Wrapped {
            width: 400.0,
            ink_width: first.x,
        })
    );
    // Less room than the ink really rewraps.
    let narrow = measure(first.x * 0.5);
    assert!(narrow.y > first.y, "narrow={narrow:?}, first={first:?}");
}

#[test]
fn a_placed_box_moves_for_a_later_reflow_only_from_its_cursor() {
    let mut canvas = SceneModel::new(1280, 720);
    let first = canvas.rect(1.0, 1.0);
    let row = canvas.group(&[&first]);
    first.claim_layout(&row).unwrap();
    let spec = crate::canvas::LayoutSpec {
        kind: gaanim_layout::LayoutNodeKind::Row { wrap: false },
        style: gaanim_layout::LayoutStyle::default(),
        within: LayoutWithin::Intrinsic,
    };
    let member = |handle: &DrawableHandle| crate::canvas::LayoutMemberSpec {
        id: handle.id,
        style: gaanim_layout::LayoutItemStyle::default(),
    };
    canvas.reflow_layout(
        &row,
        vec![member(&first)],
        spec.clone(),
        1,
        None,
        None,
        None,
    );
    // Its left edge at x = -4, as `place(zone, anchor="left")` puts it.
    let row = row.at_anchor(-4.0, 0.0, crate::canvas::Anchor::Left);
    canvas.wait(1.0);
    let second = canvas.rect(2.0, 1.0);
    second.claim_layout(&row).unwrap();
    canvas.set_group_members(&row, &[&first, &second]);
    canvas.reflow_layout(
        &row,
        vec![member(&first), member(&second)],
        spec,
        2,
        Some(1.0),
        Some(&second),
        None,
    );
    canvas.wait(1.0);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let x = |world: &mut World, timeline: &mut Timeline, time: f64| {
        timeline.seek(world, time);
        transform_of(world, &row).translation.x
    };
    // One unit wide, centered at -3.5 until the reflow; three wide,
    // centered at -2.5 once it ends; on its way in between.
    assert!((x(&mut world, &mut timeline, 0.5) + 3.5).abs() < 1e-9);
    let midway = x(&mut world, &mut timeline, 1.5);
    assert!(midway > -3.5 && midway < -2.5, "{midway}");
    assert!((x(&mut world, &mut timeline, 2.5) + 2.5).abs() < 1e-9);
}

#[test]
fn nested_layout_materializes_paragraph_at_the_outer_assigned_width() {
    let mut canvas = SceneModel::new(1280, 720);
    let paragraph = canvas.configured_text(
        "Nested responsive paragraphs must use the card width instead of the safe frame width.",
        gaanim_text::prelude::TextStyle {
            size: Some(40.0),
            ..Default::default()
        },
        gaanim_text::prelude::TextFlow::default(),
    );
    let inner = canvas.group(&[&paragraph]);
    paragraph.claim_layout(&inner).unwrap();
    canvas.reflow_layout(
        &inner,
        vec![crate::canvas::LayoutMemberSpec {
            id: paragraph.id,
            style: gaanim_layout::LayoutItemStyle::default(),
        }],
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
            style: gaanim_layout::LayoutStyle {
                width: gaanim_layout::SizeRule::Fill(1.0),
                height: gaanim_layout::SizeRule::Fill(1.0),
                padding: gaanim_layout::Insets::all(28.0),
                align: gaanim_layout::Align::Stretch,
                ..Default::default()
            },
            within: LayoutWithin::Intrinsic,
        },
        1,
        None,
        None,
        None,
    );

    let outer = canvas.group(&[&inner]);
    inner.claim_layout(&outer).unwrap();
    canvas.reflow_layout(
        &outer,
        vec![crate::canvas::LayoutMemberSpec {
            id: inner.id,
            style: gaanim_layout::LayoutItemStyle::default(),
        }],
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Stack,
            style: gaanim_layout::LayoutStyle {
                width: gaanim_layout::SizeRule::Fixed(340.0),
                height: gaanim_layout::SizeRule::Fixed(220.0),
                align: gaanim_layout::Align::Stretch,
                ..Default::default()
            },
            within: LayoutWithin::Safe,
        },
        1,
        None,
        None,
        None,
    );

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    let mut world = world;
    queue.apply(&mut world);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.0);
    let visible_bounds: Vec<_> = world
        .query::<(&LocalBounds, &Opacity)>()
        .iter(&world)
        .filter(|(_, opacity)| opacity.0 > 0.99)
        .map(|(bounds, _)| bounds.0)
        .collect();
    assert!(
        visible_bounds.iter().any(|bounds| bounds.width() > 200.0
            && bounds.width() < 300.0
            && bounds.height() > 100.0),
        "expected a wrapped paragraph inside the 284-unit content box, got {visible_bounds:?}"
    );
    assert!(
        visible_bounds
            .iter()
            .all(|bounds| bounds.width() <= 340.0 + 1.0e-6),
        "safe-frame paragraph leaked into the nested layout: {visible_bounds:?}"
    );
}

#[test]
fn root_layout_preserves_authored_at_translation() {
    let mut canvas = SceneModel::new(1280, 720);
    let first = canvas.rect(160.0, 60.0);
    let second = canvas.rect(120.0, 40.0);
    let root = canvas.group(&[&first, &second]);
    first.claim_layout(&root).unwrap();
    second.claim_layout(&root).unwrap();
    canvas.reflow_layout(
        &root,
        vec![
            crate::canvas::LayoutMemberSpec {
                id: first.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            },
            crate::canvas::LayoutMemberSpec {
                id: second.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            },
        ],
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
            style: gaanim_layout::LayoutStyle {
                gap: DVec2::splat(24.0),
                ..Default::default()
            },
            within: LayoutWithin::Intrinsic,
        },
        1,
        None,
        None,
        None,
    );
    root.move_to(400.0, 200.0);

    let mut world = compile_canvas_for_layout(canvas);
    let transform = world
        .query_filtered::<&SpatialTransform, (
            bevy::prelude::With<gaanim_scene::GroupMarker>,
            bevy::prelude::Without<bevy::prelude::ChildOf>,
        )>()
        .single(&world)
        .expect("one root layout group");
    assert_point_close(transform.translation, DVec3::new(400.0, 200.0, 0.0));
}

/// A scaled box scales its children with it, as a CSS transform does:
/// they keep their own scale under it instead of undoing the box's.
#[test]
fn a_scaled_box_keeps_its_children_at_their_own_scale() {
    let mut canvas = SceneModel::new(1280, 720);
    let first = canvas.rect(160.0, 60.0);
    let second = canvas.rect(120.0, 40.0);
    let root = canvas.group(&[&first, &second]);
    first.claim_layout(&root).unwrap();
    second.claim_layout(&root).unwrap();
    let root = root.scale_to(2.0);
    canvas.reflow_layout(
        &root,
        [&first, &second]
            .iter()
            .map(|member| crate::canvas::LayoutMemberSpec {
                id: member.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            })
            .collect(),
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
            style: gaanim_layout::LayoutStyle::default(),
            within: LayoutWithin::Intrinsic,
        },
        1,
        None,
        None,
        None,
    );

    let mut world = compile_canvas_for_layout(canvas);
    let scales: Vec<DVec3> = world
        .query_filtered::<&SpatialTransform, bevy::prelude::With<bevy::prelude::ChildOf>>()
        .iter(&world)
        .map(|transform| transform.scale)
        .collect();
    assert_eq!(scales.len(), 2);
    for scale in scales {
        assert_point_close(scale, DVec3::ONE);
    }
}

/// A stack moved after its solve crops a `fit="cover"` layer to the
/// stack where it is, not where the solve centred it.
#[test]
fn a_cover_layer_is_cropped_where_its_moved_stack_is() {
    let mut canvas = SceneModel::new(1280, 720);
    let layer = canvas.rect(80.0, 80.0);
    let root = canvas.group(&[&layer]);
    layer.claim_layout(&root).unwrap();
    let root = root.move_to(300.0, 0.0);
    canvas.reflow_layout(
        &root,
        vec![crate::canvas::LayoutMemberSpec {
            id: layer.id,
            style: gaanim_layout::LayoutItemStyle {
                fit: gaanim_layout::FitMode::Cover,
                ..Default::default()
            },
        }],
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Stack,
            style: gaanim_layout::LayoutStyle {
                width: gaanim_layout::SizeRule::Fixed(100.0),
                height: gaanim_layout::SizeRule::Fixed(50.0),
                ..Default::default()
            },
            within: LayoutWithin::Intrinsic,
        },
        1,
        None,
        None,
        None,
    );

    let mut world = compile_canvas_for_layout(canvas);
    let clips: Vec<_> = world
        .query::<&gaanim_renderer::effects::ClipMask>()
        .iter(&world)
        .map(|clip| clip.path.bounding_box())
        .collect();
    // The layer is scaled by 1.25 to cover the stack, so the stack's
    // 100 x 50 box is 80 x 40 in the layer's own space, around its center.
    assert_eq!(clips.len(), 1);
    let clip = clips[0];
    assert!((clip.center().x).abs() < 1.0e-6, "{clip:?}");
    assert!((clip.width() - 80.0).abs() < 1.0e-6, "{clip:?}");
    assert!((clip.height() - 40.0).abs() < 1.0e-6, "{clip:?}");
}

/// A child detached from a moved box leaves the hierarchy from then on,
/// where it stands, and then moves into another box: before each change
/// it still follows the box it was in. Seeking back restores each one.
#[test]
fn a_child_changes_box_on_the_timeline() {
    let mut canvas = SceneModel::new(1280, 720);
    let first = canvas.rect(80.0, 30.0);
    let second = canvas.rect(80.0, 30.0);
    let other = canvas.rect(80.0, 30.0);
    let row = canvas.group(&[&first, &second]);
    first.claim_layout(&row).unwrap();
    second.claim_layout(&row).unwrap();
    let row = row.move_to(300.0, 200.0);
    let column = canvas.group(&[&other]);
    other.claim_layout(&column).unwrap();
    let column = column.move_to(-300.0, -200.0);
    let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
        id: handle.id,
        style: gaanim_layout::LayoutItemStyle::default(),
    };
    let spec = |kind| crate::canvas::LayoutSpec {
        kind,
        style: gaanim_layout::LayoutStyle {
            gap: DVec2::splat(20.0),
            ..Default::default()
        },
        within: LayoutWithin::Intrinsic,
    };
    let row_spec = spec(gaanim_layout::LayoutNodeKind::Row { wrap: false });
    let column_spec = spec(gaanim_layout::LayoutNodeKind::Column { wrap: false });
    canvas.reflow_layout(
        &row,
        vec![member(&first), member(&second)],
        row_spec.clone(),
        1,
        None,
        None,
        None,
    );
    canvas.reflow_layout(
        &column,
        vec![member(&other)],
        column_spec.clone(),
        1,
        None,
        None,
        None,
    );
    canvas.wait(1.0);
    second.release_layout(&row);
    canvas.set_group_members(&row, &[&first]);
    canvas.reflow_layout(&row, vec![member(&first)], row_spec, 2, None, None, None);
    canvas.wait(1.0);
    second.claim_layout(&column).unwrap();
    canvas.set_group_members(&column, &[&other, &second]);
    canvas.reflow_layout(
        &column,
        vec![member(&other), member(&second)],
        column_spec,
        2,
        Some(0.5),
        Some(&second),
        None,
    );
    canvas.wait(1.0);

    let (mut world, mut timeline) = compiled_world(&canvas);
    let state = |world: &mut World, time: f64, timeline: &mut Timeline| {
        timeline.seek(world, time);
        let entity = entity_of(world, &second);
        let parent = world
            .get::<bevy::prelude::ChildOf>(entity)
            .map(|of| of.parent());
        let opacity = opacity_of(world, &second);
        (parent, transform_of(world, &second).translation, opacity)
    };
    let row_entity = entity_of(&mut world, &row);
    let column_entity = entity_of(&mut world, &column);
    // In the row, 50 right of its middle; detached at (350, 200).
    for time in [0.5, 3.5, 1.5, 0.5] {
        let (parent, translation, opacity) = state(&mut world, time, &mut timeline);
        assert!((opacity - 1.0).abs() < 1e-6, "hidden at {time}");
        match time {
            0.5 => {
                assert_eq!(parent, Some(row_entity), "at {time}");
                assert_point_close(translation, DVec3::new(50.0, 0.0, 0.0));
            }
            1.5 => {
                assert_eq!(parent, None, "at {time}");
                assert_point_close(translation, DVec3::new(350.0, 200.0, 0.0));
            }
            _ => {
                // Under the other rect, 25 below the column's middle.
                assert_eq!(parent, Some(column_entity), "at {time}");
                assert_point_close(translation, DVec3::new(0.0, -25.0, 0.0));
            }
        }
    }
}

/// `box.reflow()` measures a child scaled since it was placed at its new
/// size, a nested box as well as a leaf, and its siblings make room; any
/// other reflow keeps the size the child was placed with, as a CSS
/// transform does.
#[test]
fn remeasuring_makes_room_for_a_scaled_child() {
    use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};
    let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
        id: handle.id,
        style: gaanim_layout::LayoutItemStyle::default(),
    };
    let third_rest = |remeasure: bool, nested: bool| {
        let mut canvas = SceneModel::new(1280, 720);
        let cells: Vec<_> = (0..3)
            .map(|_| {
                let rect = canvas.rect(80.0, 30.0);
                if !nested {
                    return rect;
                }
                let inner = canvas.group(&[&rect]);
                rect.claim_layout(&inner).unwrap();
                canvas.reflow_layout(
                    &inner,
                    vec![member(&rect)],
                    crate::canvas::LayoutSpec {
                        kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
                        style: gaanim_layout::LayoutStyle::default(),
                        within: LayoutWithin::Intrinsic,
                    },
                    1,
                    None,
                    None,
                    None,
                );
                inner
            })
            .collect();
        let refs: Vec<_> = cells.iter().collect();
        let container = canvas.group(&refs);
        for cell in &cells {
            cell.claim_layout(&container).unwrap();
        }
        let spec = crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Row { wrap: false },
            style: gaanim_layout::LayoutStyle {
                gap: DVec2::splat(20.0),
                ..Default::default()
            },
            within: LayoutWithin::Intrinsic,
        };
        let members = || cells.iter().map(member).collect::<Vec<_>>();
        canvas.reflow_layout(&container, members(), spec.clone(), 1, None, None, None);
        canvas.play(vec![cells[1].animate().scale_to(2.0).duration(0.5)]);
        if remeasure {
            canvas.remeasure_layout(&container, members(), spec, 2, Some(0.5), &container);
        } else {
            canvas.reflow_layout(&container, members(), spec, 2, Some(0.5), None, None);
        }
        let (_, timeline) = compiled_world(&canvas);
        timeline
            .clips
            .values()
            .filter(|clip| clip.start >= 0.5 - 1.0e-9)
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(AnimationSpec {
                    lens: PropertyLensSpec::Translation { to, .. },
                    ..
                }) => Some(to.x),
                _ => None,
            })
            .fold(f64::NEG_INFINITY, f64::max)
    };
    // 80 + 20 + 160 + 20 + 80 wide: the last cell rests 140 from the
    // middle once the scaled one counts at 160, 100 while it counts at 80.
    for nested in [false, true] {
        let (remeasured, kept) = (third_rest(true, nested), third_rest(false, nested));
        assert!(
            (remeasured - 140.0).abs() < 1.0e-6,
            "nested {nested}: {remeasured}"
        );
        assert!((kept - 100.0).abs() < 1.0e-6, "nested {nested}: {kept}");
    }
}

#[test]
fn root_layout_replays_anchor_scale_and_rotation_against_resolved_bounds() {
    let mut canvas = SceneModel::new(1280, 720);
    let first = canvas.rect(160.0, 60.0);
    let second = canvas.rect(120.0, 40.0);
    let root = canvas.group(&[&first, &second]);
    first.claim_layout(&root).unwrap();
    second.claim_layout(&root).unwrap();
    canvas.reflow_layout(
        &root,
        vec![
            crate::canvas::LayoutMemberSpec {
                id: first.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            },
            crate::canvas::LayoutMemberSpec {
                id: second.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            },
        ],
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
            style: gaanim_layout::LayoutStyle {
                gap: DVec2::splat(24.0),
                ..Default::default()
            },
            within: LayoutWithin::Intrinsic,
        },
        1,
        None,
        None,
        None,
    );
    root.scale_to(1.5)
        .rotate_to(0.25)
        .at_anchor(300.0, 140.0, Anchor::TopRight);

    let mut world = compile_canvas_for_layout(canvas);
    let (bounds, transform) = world
        .query_filtered::<(&LocalBounds, &SpatialTransform), (
            bevy::prelude::With<gaanim_scene::GroupMarker>,
            bevy::prelude::Without<bevy::prelude::ChildOf>,
        )>()
        .single(&world)
        .map(|(bounds, transform)| (bounds.0, *transform))
        .expect("one root layout group");
    assert_point_close(transform.scale, DVec3::splat(1.5));
    assert_point_close(
        transform
            .to_mat4()
            .transform_point3(Anchor::TopRight.get_point(&bounds)),
        DVec3::new(300.0, 140.0, 0.0),
    );
}

/// `line_spacing` is the distance between baselines in ems, as CSS
/// `line-height` is (#298): 1.2 puts them 1.2 em apart.
#[test]
fn line_spacing_sets_the_distance_between_baselines() {
    let fonts = gaanim_text::font::FontRegistry::new();
    let height = |content: &str, spacing: f64| {
        let spec = StructuredTextSpec::new(
            vec![content.into()],
            None,
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow {
                wrap: gaanim_text::prelude::TextWrap::Width(10.0),
                line_spacing: spacing,
                ..Default::default()
            },
        )
        .unwrap();
        let source = structured_text_typst_source(
            &spec,
            Some(10.0),
            100.0,
            "New Computer Modern",
            gaanim_core::peniko::Color::WHITE,
        );
        let (bounds, metrics) = gaanim_text::prelude::measure_typst_lines(
            &fonts,
            &source,
            false,
            Some("New Computer Modern"),
            None,
            Some(100.0),
            None,
            Some(gaanim_core::peniko::Brush::Solid(
                gaanim_core::peniko::Color::WHITE,
            )),
            StrokeBrush::transparent(),
        )
        .unwrap();
        (bounds.height(), metrics.line_count)
    };
    for spacing in [1.0, 1.2, 1.5] {
        let (one, lines) = height("H", spacing);
        assert_eq!(lines, 1);
        // Each word breaks onto its own line.
        let (three, lines) = height("H H H", spacing);
        assert_eq!(lines, 3);
        let gap = (three - one) / 2.0;
        assert!(
            (gap - 100.0 * spacing).abs() < 0.5,
            "{spacing}: baselines {gap} apart"
        );
    }
}

#[test]
fn inline_math_helpers_handle_escapes_and_doubles() {
    assert_eq!(
        typst_inline_content("a $x$ b"),
        "#text(\"a \")$x$#text(\" b\")"
    );
    assert_eq!(typst_inline_content("sin math"), "#text(\"sin math\")");
    assert_eq!(typst_inline_content("$E=mc^2$"), "$E=mc^2$");
    assert_eq!(
        typst_inline_content("Escapado \\$ literal $x$"),
        "#text(\"Escapado $ literal \")$x$"
    );
    assert_eq!(split_text_math("a $x$ b $y$ c").len(), 5);
    let spec = StructuredTextSpec::new(
        vec!["Hola $x^2$ mundo".into()],
        None,
        gaanim_text::prelude::TextStyle::default(),
        gaanim_text::prelude::TextFlow {
            wrap: gaanim_text::prelude::TextWrap::Width(400.0),
            ..Default::default()
        },
    )
    .unwrap();
    let para = structured_text_typst_source(
        &spec,
        Some(400.0),
        32.0,
        "New Computer Modern",
        gaanim_core::peniko::Color::WHITE,
    );
    assert!(
        para.contains("$x^2$"),
        "paragraph source should embed math, got {para}"
    );
    assert!(para.contains("#text(\"Hola \")"));
    assert!(!para.contains("lang:"), "no lang unless requested: {para}");
    let spanish = StructuredTextSpec::new(
        vec!["Hola".into()],
        None,
        gaanim_text::prelude::TextStyle::default(),
        gaanim_text::prelude::TextFlow {
            hyphenate: true,
            lang: Some("es".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let spanish = structured_text_typst_source(
        &spanish,
        Some(400.0),
        32.0,
        "New Computer Modern",
        gaanim_core::peniko::Color::WHITE,
    );
    assert!(
        spanish.contains("hyphenate: true, lang: \"es\""),
        "{spanish}"
    );
    for invalid in ["", "e", "espa", "ES", "e\"s"] {
        assert!(
            StructuredTextSpec::new(
                vec!["Hola".into()],
                None,
                gaanim_text::prelude::TextStyle::default(),
                gaanim_text::prelude::TextFlow {
                    lang: Some(invalid.into()),
                    ..Default::default()
                },
            )
            .is_err(),
            "{invalid:?} must be rejected"
        );
    }
    let txt = text_inline_typst_source("prueba $x^2$ fin", gaanim_core::peniko::Color::WHITE);
    assert!(txt.contains("$x^2$"));
    let txt2 = text_inline_typst_source("$alpha + beta = 1$", gaanim_core::peniko::Color::WHITE);
    assert!(txt2.contains("$alpha + beta = 1$"));
}

#[test]
fn layout_reflow_animates_displaced_members_and_fades_the_insertion() {
    let mut canvas = SceneModel::new(640, 360);
    let first = canvas.rect(80.0, 30.0);
    let second = canvas.rect(80.0, 30.0);
    let container = canvas.group(&[&first]);
    let spec = crate::canvas::LayoutSpec {
        kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
        style: gaanim_layout::LayoutStyle {
            gap: DVec2::splat(20.0),
            align: gaanim_layout::Align::Center,
            ..Default::default()
        },
        within: LayoutWithin::Intrinsic,
    };
    canvas.reflow_layout(
        &container,
        vec![crate::canvas::LayoutMemberSpec {
            id: first.id,
            style: gaanim_layout::LayoutItemStyle::default(),
        }],
        spec.clone(),
        1,
        None,
        None,
        None,
    );
    canvas.segment("layout-update", None).unwrap();
    canvas.set_group_members(&container, &[&first, &second]);
    canvas.reflow_layout(
        &container,
        vec![
            crate::canvas::LayoutMemberSpec {
                id: first.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            },
            crate::canvas::LayoutMemberSpec {
                id: second.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            },
        ],
        spec,
        2,
        Some(0.5),
        Some(&second),
        None,
    );

    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);

    assert!(timeline.clips.values().any(|clip| matches!(
        &clip.payload,
        gaanim_timeline::clip::ClipPayload::Animation(gaanim_timeline::clip::AnimationSpec {
            lens: gaanim_timeline::clip::PropertyLensSpec::Translation { .. },
            ..
        })
    )));
    assert!(timeline.clips.values().any(|clip| matches!(
        &clip.payload,
        gaanim_timeline::clip::ClipPayload::Animation(
            gaanim_timeline::clip::AnimationSpec {
                lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { from, to },
                ..
            }
        ) if *from == 0.0 && *to == 1.0
    )));
}

/// A deformed group moves its points as wholes, the 3D line between
/// them follows them, and its mesh's vertices move one by one, all by
/// the field at their place, eased in with the animation and gone again
/// before it.
#[test]
fn a_deformed_group_moves_points_lines_and_mesh_vertices_by_the_field() {
    use bevy::prelude::App;
    let mut canvas = SceneModel::new(640, 360);
    let material = gaanim_scene::Material3D::default();
    let top = canvas.circle(0.1).move_to_3d(0.0, 2.0, 0.0);
    let bottom = canvas.circle(0.1).move_to_3d(0.0, 0.0, 0.0);
    let line = canvas
        .line_3d(
            CanvasEndpoint::Entity(top.id),
            CanvasEndpoint::Entity(bottom.id),
            0.05,
            8,
            false,
            material,
        )
        .unwrap();
    let cube = canvas
        .cube(1.0, material)
        .unwrap()
        .move_to_3d(3.0, 2.0, 0.0);
    let model = canvas.group(&[&top, &bottom, &line, &cube]);
    canvas.wait(0.5);
    // A shear: x moves by half of y. A lattice reads it exactly.
    let field = |p: DVec3| DVec3::new(0.5 * p.y, 0.0, 0.0);
    let grid = model
        .deform_grid(
            Some(crate::canvas::DeformSamples::Counts([3, 3, 3])),
            Some(Bounds3D::new(DVec3::splat(-5.0), DVec3::splat(5.0))),
        )
        .unwrap();
    let values = grid.points().into_iter().map(field).collect();
    let anim = model.animate().deform(Some(values)).unwrap().duration(1.0);
    canvas.play(vec![anim]);
    canvas.wait(0.5);

    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    app.world_mut().resource_mut::<Timeline>().keyframes.clear();
    gaanim_timeline::capture_reload_keyframe(app.world_mut());
    let (top, line, cube) = (
        entity_of(app.world_mut(), &top),
        entity_of(app.world_mut(), &line),
        entity_of(app.world_mut(), &cube),
    );
    let at = |app: &mut App, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        let world = app.world();
        let origin = |entity| {
            world
                .get::<gaanim_math::GlobalSpatialTransform>(entity)
                .unwrap()
                .mat4
                .transform_point3(DVec3::ZERO)
        };
        let corners: Vec<DVec3> = {
            let global = world
                .get::<gaanim_math::GlobalSpatialTransform>(cube)
                .unwrap()
                .mat4;
            let mesh = world.get::<gaanim_scene::TriangleMeshData>(cube).unwrap();
            mesh.vertices
                .iter()
                .map(|vertex| global.transform_point3(DVec3::from_array(vertex.map(f64::from))))
                .collect()
        };
        (origin(top), origin(line), corners)
    };
    let close = |a: DVec3, b: DVec3| a.distance(b) < 1e-5;
    // Each corner of the cube stands 0.5 * y * weight along x from its
    // undeformed x, 2.5 or 3.5.
    let sheared = |corners: &[DVec3], weight: f64| {
        corners.iter().all(|corner| {
            let rest = corner.x - 0.5 * corner.y * weight;
            (rest - 2.5).abs() < 1e-5 || (rest - 3.5).abs() < 1e-5
        })
    };
    let (point, middle, corners) = at(&mut app, 1.5);
    assert!(close(point, DVec3::new(1.0, 2.0, 0.0)), "{point:?}");
    // The line runs from the moved top to the bottom, which stayed.
    assert!(close(middle, DVec3::new(0.5, 1.0, 0.0)), "{middle:?}");
    assert!(sheared(&corners, 1.0), "{corners:?}");
    let (point, _, corners) = at(&mut app, 1.0);
    assert!(close(point, DVec3::new(0.5, 2.0, 0.0)), "halfway {point:?}");
    assert!(sheared(&corners, 0.5), "halfway {corners:?}");
    let (point, middle, corners) = at(&mut app, 0.25);
    assert!(close(point, DVec3::new(0.0, 2.0, 0.0)), "before {point:?}");
    assert!(
        close(middle, DVec3::new(0.0, 1.0, 0.0)),
        "before {middle:?}"
    );
    assert!(sheared(&corners, 0.0), "before {corners:?}");
}

/// Each copy of a mesh drawn at many points moves by the field at its
/// place in the world, whatever its scale and turn, and the mesh they
/// share keeps its shape.
#[test]
fn a_deformed_set_of_copies_moves_each_copy_by_the_field_at_its_place() {
    use bevy::prelude::App;
    use gaanim_scene::{MeshCopies, MeshInstances, TriangleMeshData};
    let mut canvas = SceneModel::new(640, 360);
    let material = gaanim_scene::Material3D::default();
    let cube = canvas.cube(1.0, material).unwrap();
    let quarter_turn = gaanim_core::glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let copies = canvas
        .instances(
            &cube,
            MeshCopies {
                positions: vec![[-3.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
                scales: Some(vec![[0.25; 3], [0.25, 0.25, 1.0], [0.5; 3]]),
                rotations: Some(vec![
                    [0.0, 0.0, 0.0, 1.0],
                    [0.0, 0.0, 0.0, 1.0],
                    quarter_turn.to_array(),
                ]),
                colors: None,
            },
            false,
        )
        .unwrap()
        .move_to_3d(0.0, 2.0, 0.0);
    canvas.wait(0.5);
    // Up by 1, and by half of x more: a lattice reads it exactly.
    let field = |p: DVec3| DVec3::new(0.0, 0.0, 1.0 + 0.5 * p.x);
    let grid = copies
        .deform_grid(
            Some(crate::canvas::DeformSamples::Counts([3, 3, 3])),
            Some(Bounds3D::new(DVec3::splat(-5.0), DVec3::splat(5.0))),
        )
        .unwrap();
    let values = grid.points().into_iter().map(field).collect();
    let anim = copies.animate().deform(Some(values)).unwrap().duration(1.0);
    canvas.play(vec![anim]);
    canvas.wait(0.5);

    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    app.world_mut().resource_mut::<Timeline>().keyframes.clear();
    gaanim_timeline::capture_reload_keyframe(app.world_mut());
    let copies = entity_of(app.world_mut(), &copies);
    let authored = app
        .world()
        .get::<TriangleMeshData>(copies)
        .unwrap()
        .vertices
        .clone();
    let at = |app: &mut App, time: f64| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        let world = app.world();
        let global = world
            .get::<gaanim_math::GlobalSpatialTransform>(copies)
            .unwrap()
            .mat4;
        let places: Vec<DVec3> = world
            .get::<MeshInstances>(copies)
            .unwrap()
            .copies
            .positions
            .iter()
            .map(|place| global.transform_point3(DVec3::from_array(place.map(f64::from))))
            .collect();
        let mesh = world
            .get::<TriangleMeshData>(copies)
            .unwrap()
            .vertices
            .clone();
        (places, mesh)
    };
    let close = |places: &[DVec3], lifts: [f64; 3]| {
        places
            .iter()
            .zip([-3.0, -1.0, 1.0])
            .zip(lifts)
            .all(|((place, x), lift)| place.distance(DVec3::new(x, 2.0, lift)) < 1e-5)
    };
    let (places, mesh) = at(&mut app, 1.5);
    assert!(close(&places, [-0.5, 0.5, 1.5]), "{places:?}");
    assert_eq!(mesh, authored);
    let (places, _) = at(&mut app, 1.0);
    assert!(close(&places, [-0.25, 0.25, 0.75]), "halfway {places:?}");
    let (places, mesh) = at(&mut app, 0.25);
    assert!(close(&places, [0.0; 3]), "before {places:?}");
    assert_eq!(mesh, authored);
}

/// A centered column 300 high holding a, d, b and c (80 wide, 30, 50, 30
/// and 30 high) where, both at 0.5 s and over 1 s, d leaves and e (40
/// high) enters after c, d first when `leave_first`, folding and
/// unfolding when `fold` and fading otherwise, with another box laid
/// out in between and, with `stop`, a stop just before: the app and the
/// entities of a, b, c, d and e.
fn simultaneous_column(
    leave_first: bool,
    fold: bool,
    stop: bool,
) -> (bevy::prelude::App, [Entity; 5]) {
    use bevy::prelude::App;
    let mut canvas = SceneModel::new(640, 360);
    let [a, d, b, c, e] = [30.0, 50.0, 30.0, 30.0, 40.0].map(|height| canvas.rect(80.0, height));
    let container = canvas.group(&[&a, &d, &b, &c]);
    let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
        id: handle.id,
        style: gaanim_layout::LayoutItemStyle::default(),
    };
    let spec = crate::canvas::LayoutSpec {
        kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
        style: gaanim_layout::LayoutStyle {
            gap: DVec2::splat(20.0),
            height: gaanim_layout::SizeRule::Fixed(300.0),
            justify: gaanim_layout::Justify::Center,
            ..Default::default()
        },
        within: LayoutWithin::Intrinsic,
    };
    let mut members = vec![&a, &d, &b, &c];
    canvas.reflow_layout(
        &container,
        members.iter().map(|handle| member(handle)).collect(),
        spec.clone(),
        1,
        None,
        None,
        None,
    );
    canvas.wait(0.5);
    if stop {
        // The box made between the changes then comes a moment after
        // the stop, and the second change with it.
        canvas.stop(None).unwrap();
    }
    let changes = if leave_first {
        [false, true]
    } else {
        [true, false]
    };
    for (version, entering) in (2..).zip(changes) {
        if version == 3 {
            // A box made between the two lays itself out at once, as
            // `scene.layout.box(...)` does.
            let note = canvas.rect(40.0, 20.0);
            let other = canvas.group(&[&note]);
            canvas.reflow_layout(
                &other,
                vec![member(&note)],
                spec.clone(),
                1,
                None,
                None,
                None,
            );
        }
        if entering {
            members.push(&e);
        } else {
            members.retain(|handle| handle.id != d.id);
        }
        canvas.set_group_members(&container, &members);
        let specs = members.iter().map(|handle| member(handle)).collect();
        let (enters, leaves) = if entering {
            (Some(&e), None)
        } else {
            (None, Some(&d))
        };
        if fold {
            canvas.reflow_layout_folding(
                &container,
                specs,
                spec.clone(),
                version,
                Some(1.0),
                enters,
                leaves,
            );
            let child = if entering { &e } else { &d };
            canvas
                .fold_layout_child(child, crate::canvas::Direction::Down, entering, 1.0)
                .unwrap();
        } else {
            canvas.reflow_layout(
                &container,
                specs,
                spec.clone(),
                version,
                Some(1.0),
                enters,
                leaves,
            );
        }
    }
    canvas.wait(1.5);

    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    app.world_mut().resource_mut::<Timeline>().keyframes.clear();
    gaanim_timeline::capture_reload_keyframe(app.world_mut());
    let entities = [&a, &b, &c, &d, &e].map(|handle| entity_of(app.world_mut(), handle));
    (app, entities)
}

/// Where `entities` stand in the world at `time`.
fn world_places_at(app: &mut bevy::prelude::App, time: f64, entities: [Entity; 5]) -> [DVec3; 5] {
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
    app.update();
    let world = app.world();
    entities.map(|entity| {
        world
            .get::<gaanim_math::GlobalSpatialTransform>(entity)
            .unwrap()
            .mat4
            .transform_point3(DVec3::ZERO)
    })
}

/// Two changes of one box that start at the same instant run together,
/// whatever their order and even at a stop: the others slide from where
/// they stood to the layout both leave, the leaving child fades where it
/// stood or folds with the child before it, and the entering one unfolds
/// with the child before it.
#[test]
fn simultaneous_layout_changes_run_together_in_either_order() {
    let times = [0.5, 0.6, 0.75, 1.0, 1.25, 1.5, 2.0];
    for (fold, stop) in [(false, false), (true, false), (false, true), (true, true)] {
        let (mut leave_first, entities) = simultaneous_column(true, fold, stop);
        let (mut enter_first, others) = simultaneous_column(false, fold, stop);
        let [a0, b0, _, d0, _] = world_places_at(&mut leave_first, 0.5, entities);
        let [_, b1, c1, _, e1] = world_places_at(&mut leave_first, 1.5, entities);
        assert!((b1 - b0).length() > 10.0, "{b0:?} {b1:?}");
        // After a stop the second change starts a moment later: at the
        // stop's own frame the child it brings in, still hidden, may
        // stand elsewhere, and the slide lags by that moment.
        let (times, slack) = if stop {
            (&times[1..], 1e-3)
        } else {
            (&times[..], 1e-6)
        };
        for &time in times {
            let places = world_places_at(&mut leave_first, time, entities);
            let reversed = world_places_at(&mut enter_first, time, others);
            for (place, other) in places.iter().zip(&reversed) {
                assert!(
                    place.distance(*other) < 1e-6,
                    "fold {fold}, stop {stop}, t={time}: {places:?} {reversed:?}"
                );
            }
            let [a, b, c, d, e] = places;
            let progress = RateFunc::Smooth.evaluate(((time - 0.5) / 1.0).clamp(0.0, 1.0));
            assert!(
                b.distance(b0.lerp(b1, progress)) < slack,
                "fold {fold}, stop {stop}, t={time}: {b:?} from {b0:?} to {b1:?}"
            );
            if time > 1.5 {
                continue;
            }
            if fold {
                assert!((d - a).distance(d0 - a0) < 1e-6, "t={time}: {d:?} {a:?}");
                assert!((e - c).distance(e1 - c1) < 1e-6, "t={time}: {e:?} {c:?}");
            } else {
                assert!(d.distance(d0) < 1e-6, "t={time}: {d:?} {d0:?}");
                assert!(e.distance(e1) < 1e-6, "t={time}: {e:?} {e1:?}");
            }
        }
    }
}

/// A column 300 high whose two 80 × 30 children are centered, where an
/// 80 × 50 child unfolds at `index` at 0.5 s and folds out at 1.5 s, each
/// over 1 s: the app and the entities of the first of the two others and
/// of the folding child.
fn folding_column(index: usize) -> (bevy::prelude::App, Entity, Entity) {
    use bevy::prelude::App;
    let mut canvas = SceneModel::new(640, 360);
    let first = canvas.rect(80.0, 30.0);
    let folding = canvas.rect(80.0, 50.0);
    let last = canvas.rect(80.0, 30.0);
    let container = canvas.group(&[&first, &last]);
    let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
        id: handle.id,
        style: gaanim_layout::LayoutItemStyle::default(),
    };
    let spec = crate::canvas::LayoutSpec {
        kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
        style: gaanim_layout::LayoutStyle {
            gap: DVec2::splat(20.0),
            height: gaanim_layout::SizeRule::Fixed(300.0),
            justify: gaanim_layout::Justify::Center,
            ..Default::default()
        },
        within: LayoutWithin::Intrinsic,
    };
    canvas.reflow_layout(
        &container,
        vec![member(&first), member(&last)],
        spec.clone(),
        1,
        None,
        None,
        None,
    );
    canvas.wait(0.5);
    let mut unfolded = vec![&first, &last];
    unfolded.insert(index, &folding);
    canvas.set_group_members(&container, &unfolded);
    canvas
        .fold_layout_child(&folding, crate::canvas::Direction::Down, true, 1.0)
        .unwrap();
    canvas.reflow_layout_folding(
        &container,
        unfolded.iter().map(|handle| member(handle)).collect(),
        spec.clone(),
        2,
        Some(1.0),
        Some(&folding),
        None,
    );
    canvas.wait(1.0);
    canvas.set_group_members(&container, &[&first, &last]);
    canvas
        .fold_layout_child(&folding, crate::canvas::Direction::Down, false, 1.0)
        .unwrap();
    canvas.reflow_layout_folding(
        &container,
        vec![member(&first), member(&last)],
        spec,
        3,
        Some(1.0),
        None,
        Some(&folding),
    );
    canvas.wait(1.0);

    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    app.world_mut().resource_mut::<Timeline>().keyframes.clear();
    gaanim_timeline::capture_reload_keyframe(app.world_mut());
    let first = entity_of(app.world_mut(), &first);
    let folding = entity_of(app.world_mut(), &folding);
    (app, first, folding)
}

/// The local translations of `entities` and the opacity of the last one
/// at `time`.
fn folding_at(app: &mut bevy::prelude::App, time: f64, entities: [Entity; 2]) -> ([DVec3; 2], f32) {
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
    app.update();
    let world = app.world();
    let translation = |entity| world.get::<SpatialTransform>(entity).unwrap().translation;
    let opacity = world
        .get::<gaanim_scene::Opacity>(entities[1])
        .map_or(1.0, |opacity| opacity.0);
    (
        [translation(entities[0]), translation(entities[1])],
        opacity,
    )
}

/// A child that unfolds between two others moves with the one before
/// it, shown at once behind its wipe; folding out, it goes with it and
/// hides when the fold ends.
#[test]
fn an_unfolding_child_moves_with_the_sibling_before_it() {
    let (mut app, first, folding) = folding_column(1);
    let ([start, _], _) = folding_at(&mut app, 0.5, [first, folding]);
    let ([end, _], _) = folding_at(&mut app, 1.5, [first, folding]);
    // The column is centered, so the child before it moves.
    assert!((end - start).length() > 10.0, "{start:?} {end:?}");
    let ([before, child], _) = folding_at(&mut app, 1.5, [first, folding]);
    let rest = child - before;
    for time in [0.5, 0.8, 1.2, 1.6, 2.0, 2.4] {
        let ([before, child], opacity) = folding_at(&mut app, time, [first, folding]);
        let offset = child - before;
        assert!(offset.distance(rest) < 1e-6, "{time}: {offset:?} {rest:?}");
        assert_eq!(opacity, 1.0, "{time}");
    }
    let (_, hidden) = folding_at(&mut app, 2.6, [first, folding]);
    assert_eq!(hidden, 0.0);
}

/// A first child unfolds from where the box's content began, the top of
/// the child that was first, and folds out to where it begins again.
#[test]
fn a_first_child_unfolds_from_the_start_of_the_box() {
    let (mut app, next, folding) = folding_column(0);
    // Tops, y up: the child is 50 high and the next one 30.
    let tops = |app: &mut bevy::prelude::App, time: f64| {
        let ([next_center, child_center], _) = folding_at(app, time, [next, folding]);
        (next_center.y + 15.0, child_center.y + 25.0)
    };
    let (next_top, child_top) = tops(&mut app, 0.5);
    assert!(
        (child_top - next_top).abs() < 1e-6,
        "{child_top} {next_top}"
    );
    let (next_top, child_top) = tops(&mut app, 1.5);
    assert!(child_top - next_top > 60.0, "{child_top} {next_top}");
    let (next_top, child_top) = tops(&mut app, 2.5);
    assert!(
        (child_top - next_top).abs() < 1e-6,
        "{child_top} {next_top}"
    );
}

/// A box, a box that draws only its background and a loose text,
/// each clipped before a column adopts it, still draw clipped: the
/// background, and the copy the column rewraps a text into.
#[test]
fn a_drawable_clipped_before_a_box_adopts_it_keeps_its_clip() {
    use bevy::prelude::App;
    let clipped = PenikoColor::from_rgb8(120, 60, 230);
    let mut canvas = SceneModel::new(640, 360);
    let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
        id: handle.id,
        style: gaanim_layout::LayoutItemStyle::default(),
    };
    let column = |width, height| crate::canvas::LayoutSpec {
        kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
        style: gaanim_layout::LayoutStyle {
            gap: DVec2::splat(20.0),
            width,
            height,
            ..Default::default()
        },
        within: LayoutWithin::Intrinsic,
    };
    let content = || column(Default::default(), Default::default());
    // Each column exists before what it adopts.
    for kind in 0..3 {
        let first = canvas.rect(80.0, 30.0);
        let outer = canvas.group(&[&first]);
        canvas.reflow_layout(&outer, vec![member(&first)], content(), 1, None, None, None);
        let item = match kind {
            0 => {
                let empty = canvas.group(&[]);
                canvas
                    .decorate_layout(&empty, Some(Brush::Solid(clipped)), None, 0.0)
                    .unwrap();
                let size = column(
                    gaanim_layout::SizeRule::Fixed(80.0),
                    gaanim_layout::SizeRule::Fixed(40.0),
                );
                canvas.reflow_layout(&empty, Vec::new(), size, 1, None, None, None);
                empty
            }
            1 => {
                let label = canvas.text("boxed").fill(clipped);
                let boxed = canvas.group(&[&label]);
                canvas.reflow_layout(&boxed, vec![member(&label)], content(), 1, None, None, None);
                boxed
            }
            _ => canvas.text("loose").fill(clipped),
        };
        // As `box.add` does: the column owns it by the end of the script.
        item.claim_layout(&outer).unwrap();
        let mask = canvas.rect(400.0, 100.0);
        let item = item.clip(&mask, gaanim_core::peniko::Fill::NonZero);
        canvas.set_group_members(&outer, &[&first, &item]);
        canvas.reflow_layout(
            &outer,
            vec![member(&first), member(&item)],
            content(),
            2,
            None,
            Some(&item),
            None,
        );
    }
    canvas.wait(0.5);

    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(0.25);
    app.update();
    let world = app.world_mut();
    let shown = |world: &World, entity| {
        let mut current = Some(entity);
        let mut opacity = 1.0;
        while let Some(entity) = current {
            opacity *= world
                .get::<gaanim_scene::Opacity>(entity)
                .map_or(1.0, |opacity| opacity.0);
            current = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        opacity > 0.0
    };
    let mut leaves = world.query_filtered::<(
        Entity,
        &FillBrush,
        Option<&gaanim_renderer::effects::ClipMask>,
    ), (
        With<gaanim_scene::Path2D>,
        Without<gaanim_scene::GroupMarker>,
    )>();
    let leaves: Vec<(Entity, bool)> = leaves
        .iter(world)
        .filter(|(_, fill, _)| matches!(&fill.0, Some(Brush::Solid(color)) if *color == clipped))
        .map(|(entity, _, clip)| (entity, clip.is_some()))
        .collect();
    let drawn: Vec<(Entity, bool)> = leaves
        .into_iter()
        .filter(|(entity, _)| shown(world, *entity))
        .collect();
    // The background and the glyphs of both texts.
    assert!(drawn.len() >= 3, "{drawn:?}");
    let unclipped: Vec<Entity> = drawn
        .iter()
        .filter(|(_, clip)| !clip)
        .map(|(entity, _)| *entity)
        .collect();
    assert!(
        unclipped.is_empty(),
        "unclipped: {unclipped:?} of {drawn:?}"
    );
}

/// What follows a loose text, given before a column adopts it at 0.5 s,
/// follows the text until then, and from then on the copy the column
/// rewraps it into, by its origin and by an anchor of its bounds.
#[test]
fn a_follow_given_before_a_box_adopts_a_text_follows_its_copy() {
    use bevy::prelude::App;
    use gaanim_animation::FollowOffsetSpace;
    let mut canvas = SceneModel::new(640, 360);
    let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
        id: handle.id,
        style: gaanim_layout::LayoutItemStyle::default(),
    };
    let column = crate::canvas::LayoutSpec {
        kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
        style: gaanim_layout::LayoutStyle {
            gap: DVec2::splat(20.0),
            ..Default::default()
        },
        within: LayoutWithin::Intrinsic,
    };
    let first = canvas.rect(80.0, 30.0);
    let outer = canvas.group(&[&first]);
    canvas.reflow_layout(
        &outer,
        vec![member(&first)],
        column.clone(),
        1,
        None,
        None,
        None,
    );
    let label = canvas.text("target").move_to(-250.0, 120.0);
    canvas.circle(5.0).follow_endpoint(
        CanvasEndpoint::Entity(label.id),
        DVec3::ZERO,
        FollowOffsetSpace::World,
    );
    canvas.circle(5.0).follow_endpoint(
        CanvasEndpoint::Anchor(label.anchor_point(Anchor::TopRight, DVec3::ZERO)),
        DVec3::ZERO,
        FollowOffsetSpace::World,
    );
    canvas.wait(0.5);
    label.claim_layout(&outer).unwrap();
    canvas.set_group_members(&outer, &[&first, &label]);
    canvas.reflow_layout(
        &outer,
        vec![member(&first), member(&label)],
        column,
        2,
        None,
        Some(&label),
        None,
    );
    canvas.wait(0.5);

    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.finish();
    app.cleanup();
    app.update();
    crate::runtime::replay_canvas_into(app.world_mut(), canvas);
    app.update();
    app.world_mut().resource_mut::<Timeline>().keyframes.clear();
    gaanim_timeline::capture_reload_keyframe(app.world_mut());
    let label = entity_of(app.world_mut(), &label);
    // The followers, by what they follow: the text's origin or a corner.
    let follower = |app: &mut App, anchored: bool| {
        app.world_mut()
            .query::<(Entity, &gaanim_animation::EndpointFollow)>()
            .iter(app.world())
            .find(|(_, follow)| {
                matches!(
                    follow.endpoint,
                    gaanim_animation::TrackingEndpoint::EntityAnchor { .. }
                ) == anchored
            })
            .unwrap()
            .0
    };
    let (origin, corner) = (follower(&mut app, false), follower(&mut app, true));
    let copy = app
        .world()
        .get::<gaanim_animation::ReplacedBy>(label)
        .expect("the column rewraps the text into a copy")
        .copy;
    // Where the followers stand, and where the origin and the top right
    // corner of `text` are, at `time`.
    let at = |app: &mut App, time: f64, text: Entity| {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        let world = app.world();
        let place = |entity| {
            world
                .get::<gaanim_math::GlobalSpatialTransform>(entity)
                .unwrap()
                .mat4
                .transform_point3(DVec3::ZERO)
        };
        let bounds = world.get::<LocalBounds>(text).unwrap().0;
        let top_right = world
            .get::<gaanim_math::GlobalSpatialTransform>(text)
            .unwrap()
            .mat4
            .transform_point3(DVec3::new(bounds.max.x, bounds.max.y, 0.0));
        ([place(origin), place(corner)], [place(text), top_right])
    };
    let close = |a: DVec3, b: DVec3| a.truncate().distance(b.truncate()) < 1e-6;
    let ([followed, cornered], [text, top_right]) = at(&mut app, 0.25, label);
    assert!(close(followed, text), "before: {followed:?} {text:?}");
    assert!(
        close(cornered, top_right),
        "before: {cornered:?} {top_right:?}"
    );
    let ([followed, cornered], [text, top_right]) = at(&mut app, 0.75, copy);
    assert!(close(followed, text), "after: {followed:?} {text:?}");
    assert!(
        close(cornered, top_right),
        "after: {cornered:?} {top_right:?}"
    );
    assert!(
        text.distance(DVec3::new(-250.0, 120.0, 0.0)) > 50.0,
        "the column moved the text: {text:?}"
    );
}

#[test]
fn layout_records_what_the_editor_inspector_draws() {
    let mut canvas = SceneModel::new(640, 360);
    let first = canvas.rect(80.0, 30.0);
    let second = canvas.rect(80.0, 30.0);
    let container = canvas.group(&[&first, &second]);
    let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
        id: handle.id,
        style: gaanim_layout::LayoutItemStyle::default(),
    };
    canvas.reflow_layout(
        &container,
        vec![member(&first), member(&second)],
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Row { wrap: false },
            style: gaanim_layout::LayoutStyle {
                gap: DVec2::splat(20.0),
                padding: gaanim_layout::Insets::all(10.0),
                ..Default::default()
            },
            within: LayoutWithin::Intrinsic,
        },
        1,
        None,
        None,
        None,
    );
    canvas.record_layout_zones(vec![(
        "top".to_owned(),
        Bounds3D::new_2d(-1.0, 0.0, 1.0, 1.0),
    )]);
    canvas.wait(1.0);

    let (mut world, _) = compiled_world(&canvas);
    let mut inspections = world.query::<&gaanim_scene::LayoutInspection>();
    let inspection = inspections.iter(&world).next().expect("box inspection");
    let frame = inspection.at(0.0).expect("arrangement at the start");
    assert_eq!(frame.kind, gaanim_scene::LayoutInspectionKind::Row);
    assert_eq!(frame.padding, [10.0; 4]);
    assert_eq!(frame.cells.len(), 2);
    // The cells sit side by side, 20 apart, around the box's center.
    let gap = frame.cells[1].bounds.min.x - frame.cells[0].bounds.max.x;
    assert!((gap - 20.0).abs() < 1.0e-6, "{gap}");
    let zones = world.resource::<gaanim_scene::LayoutZones>();
    assert_eq!(zones.0.len(), 1);
    assert_eq!(zones.0[0].name, "top");
    assert_eq!((zones.0[0].segment, zones.0[0].start), (0, 0.0));
    assert!((zones.0[0].end - 1.0).abs() < 1.0e-9);
}

#[test]
fn layout_reflow_keeps_a_member_offset_from_its_rest() {
    use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};
    let mut canvas = SceneModel::new(640, 360);
    let first = canvas.rect(80.0, 30.0);
    let second = canvas.rect(80.0, 30.0);
    let third = canvas.rect(80.0, 30.0);
    let container = canvas.group(&[&first, &second]);
    let spec = crate::canvas::LayoutSpec {
        kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
        style: gaanim_layout::LayoutStyle {
            gap: DVec2::splat(20.0),
            align: gaanim_layout::Align::Center,
            ..Default::default()
        },
        within: LayoutWithin::Intrinsic,
    };
    let member = |handle: &crate::canvas::DrawableHandle| crate::canvas::LayoutMemberSpec {
        id: handle.id,
        style: gaanim_layout::LayoutItemStyle::default(),
    };
    canvas.reflow_layout(
        &container,
        vec![member(&first), member(&second)],
        spec.clone(),
        1,
        None,
        None,
        None,
    );
    // The first member moves on its own, then the box gains a member.
    canvas.play(vec![first.animate().shift_by(30.0, 0.0).duration(0.5)]);
    canvas.set_group_members(&container, &[&first, &second, &third]);
    canvas.reflow_layout(
        &container,
        vec![member(&first), member(&second), member(&third)],
        spec,
        2,
        Some(0.5),
        Some(&third),
        None,
    );

    let (_, timeline) = compiled_world(&canvas);
    // Every member rests at x = 0; only the moved one stays 30 to the right
    // while the reflow slides it to its new row.
    let reflow_moves: Vec<_> = timeline
        .clips
        .values()
        .filter(|clip| clip.start >= 0.5 - 1.0e-9)
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(AnimationSpec {
                lens: PropertyLensSpec::Translation { from, to },
                ..
            }) => Some((*from, *to)),
            _ => None,
        })
        .collect();
    assert!(
        reflow_moves
            .iter()
            .any(|(from, to)| (from.x - 30.0).abs() < 1.0e-6 && (to.x - 30.0).abs() < 1.0e-6),
        "{reflow_moves:?}"
    );
    assert!(
        reflow_moves
            .iter()
            .all(|(_, to)| to.x.abs() < 1.0e-6 || (to.x - 30.0).abs() < 1.0e-6),
        "{reflow_moves:?}"
    );
}

fn temp_svg(name: &str, body: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("gaanim_{name}_{}.svg", std::process::id()));
    std::fs::write(&path, body).unwrap();
    path
}

fn morph_lenses(timeline: &Timeline) -> Vec<(f64, gaanim_timeline::clip::PropertyLensSpec)> {
    use gaanim_timeline::clip::{AnimationSpec, ClipPayload};
    timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(AnimationSpec { lens, .. }) => {
                Some((clip.duration, lens.clone()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn transform_to_keeps_a_gradient_fill() {
    use gaanim_timeline::clip::PropertyLensSpec;
    // Issue #418: the morph used to flatten the gradient to its first stop.
    let svg = |d: &str| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="400">
                  <defs><radialGradient id="r" gradientUnits="userSpaceOnUse" cx="200" cy="200" r="120">
                    <stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/>
                  </radialGradient></defs>
                  <path id="a" d="{d}" fill="url(#r)"/>
                </svg>"##
        )
    };
    let first = temp_svg(
        "gradient_morph_a",
        &svg("M 100 100 L 300 100 L 300 300 L 100 300 Z"),
    );
    let second = temp_svg(
        "gradient_morph_b",
        &svg("M 120 80 L 320 120 L 280 320 L 80 280 Z"),
    );
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.svg(&first).unwrap();
    let target = canvas.svg(&second).unwrap().opacity(0.0);
    std::fs::remove_file(first).unwrap();
    std::fs::remove_file(second).unwrap();
    canvas.play(vec![
        source
            .part("a")
            .unwrap()
            .animate()
            .transform_to(&target.part("a").unwrap())
            .unwrap()
            .duration(1.0),
    ]);

    let (_, timeline) = compiled_world(&canvas);
    let lenses = morph_lenses(&timeline);
    assert!(
        lenses.iter().any(|(duration, lens)| *duration == 1.0
            && matches!(
                lens,
                PropertyLensSpec::FillPaint {
                    from: Brush::Gradient(_),
                    to: Brush::Gradient(_)
                }
            )),
        "the morph must interpolate the gradient"
    );
    assert!(
        !lenses
            .iter()
            .any(|(_, lens)| matches!(lens, PropertyLensSpec::FillColor { .. })),
        "a solid fill clip flattens the gradient"
    );
}

#[test]
fn transform_to_morphs_svg_hierarchies_part_by_part() {
    use gaanim_core::kurbo::{BezPath, PathEl};
    use gaanim_timeline::clip::PropertyLensSpec;
    // Issue #419: the roots draw nothing, so morphing them as one shape
    // left nothing to draw.
    let rect = |id: &str, x: f64| {
        format!(r##"<rect id="{id}" x="{x}" y="10" width="40" height="40" fill="#ff0000"/>"##)
    };
    let circle = |id: &str, x: f64| {
        format!(r##"<circle id="{id}" cx="{x}" cy="30" r="20" fill="#00ff00"/>"##)
    };
    let svg = |parts: String| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="60"><g id="todo">{parts}</g></svg>"##
        )
    };
    // Part "a" turns from a square into a circle and "b" the other way,
    // which shape matching alone would not pair; "c" is new.
    let first = temp_svg(
        "hierarchy_morph_a",
        &svg(rect("a", 10.0) + &circle("b", 120.0)),
    );
    let second = temp_svg(
        "hierarchy_morph_b",
        &svg(circle("a", 30.0) + &rect("b", 100.0) + &circle("c", 175.0)),
    );
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.svg(&first).unwrap();
    let target = canvas.svg(&second).unwrap().opacity(0.0);
    std::fs::remove_file(first).unwrap();
    std::fs::remove_file(second).unwrap();
    canvas.play(vec![
        source
            .animate()
            .transform_to(&target)
            .unwrap()
            .duration(1.0),
    ]);

    let (_, timeline) = compiled_world(&canvas);
    let curved = |path: &BezPath| {
        path.elements()
            .iter()
            .any(|element| matches!(element, PathEl::CurveTo(..)))
    };
    let lenses = morph_lenses(&timeline);
    let mut morphs: Vec<(bool, bool)> = lenses
        .iter()
        .filter_map(|(duration, lens)| match lens {
            PropertyLensSpec::PathMorph { from, to, .. } if *duration == 1.0 => {
                Some((curved(from), curved(to)))
            }
            _ => None,
        })
        .collect();
    morphs.sort();
    assert_eq!(
        morphs,
        vec![(false, true), (true, false)],
        "parts pair by id"
    );
    assert!(
            lenses.iter().any(|(duration, lens)| *duration == 1.0
                && matches!(lens, PropertyLensSpec::Opacity { from, to } if *from == 0.0 && *to == 1.0)),
            "the new part fades in"
        );
}

#[test]
fn text_transform_to_continues_the_source_handle_on_its_target() {
    use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.math_text("x + 3 = 7");
    let target = canvas.math_text("x = 4");
    canvas.play(vec![
        source
            .animate()
            .transform_to(&target)
            .unwrap()
            .duration(1.0),
    ]);
    canvas.play(vec![source.animate().fade_out().duration(0.5)]);

    let (_, timeline) = compiled_world(&canvas);
    let opacity_clips: Vec<_> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::Opacity { to, .. },
                label,
                ..
            }) => Some((clip.start, clip.duration, *target, *to, label.clone())),
            _ => None,
        })
        .collect();
    // The target root is shown when the transition starts...
    let target_root = opacity_clips
        .iter()
        .find(|(start, _, _, to, label)| {
            *start == 0.0 && *to > 0.0 && label.as_deref() == Some("EquationHandoff")
        })
        .map(|(_, _, id, _, _)| *id)
        .expect("the transition reveals its target root");
    // ...and a later animation of the source handle drives that target.
    assert!(
        opacity_clips.iter().any(|(start, duration, id, to, _)| {
            (*start - 1.0).abs() < 1.0e-9 && *duration == 0.5 && *id == target_root && *to == 0.0
        }),
        "fade_out on the source must continue on the target: {opacity_clips:?}"
    );
}

#[test]
fn create_after_fade_out_shows_the_object_again() {
    use gaanim_timeline::clip::{AnimationSpec, ClipPayload, PropertyLensSpec};
    let mut canvas = SceneModel::new(640, 360);
    let line = canvas.line(-1.0, 0.0, 1.0, 0.0);
    canvas.play(vec![line.animate().create().duration(1.0)]);
    canvas.play(vec![line.animate().fade_out().duration(0.5)]);
    canvas.play(vec![line.animate().create().duration(1.0)]);

    let (_, timeline) = compiled_world(&canvas);
    let restored = timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            ClipPayload::Animation(AnimationSpec {
                lens: PropertyLensSpec::Opacity { from, to },
                ..
            }) if *from == 0.0 && *to == 1.0
        ) && (clip.start - 1.5).abs() < 1.0e-9
    });
    assert!(restored, "the second create() must restore the opacity");
}

#[test]
fn responsive_layout_text_stays_hidden_until_its_fade_in() {
    let mut canvas = SceneModel::new(640, 360);
    let text = canvas.configured_text(
        "Hidden until it fades in",
        gaanim_text::prelude::TextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    );
    let container = canvas.group(&[&text]);
    text.claim_layout(&container).unwrap();
    canvas.reflow_layout(
        &container,
        vec![crate::canvas::LayoutMemberSpec {
            id: text.id,
            style: gaanim_layout::LayoutItemStyle::default(),
        }],
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
            style: gaanim_layout::LayoutStyle::default(),
            within: LayoutWithin::Safe,
        },
        1,
        None,
        None,
        None,
    );
    canvas.wait(0.5);
    canvas.play(vec![text.animate().fade_in().duration(0.5)]);

    let (_, timeline) = compiled_world(&canvas);
    // The layout recomposes the text at its offered width. That swap must
    // not reveal the replacement before the authored fade-in at 0.5 s.
    let reveal_starts: Vec<_> = timeline
        .clips
        .values()
        .filter(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { to, .. },
                        ..
                    }
                ) if *to > 0.0
            )
        })
        .map(|clip| clip.start)
        .collect();
    assert!(
        reveal_starts
            .iter()
            .any(|start| (start - 0.5).abs() < 1.0e-9),
        "the authored fade-in must remain: {reveal_starts:?}"
    );
    assert!(
        reveal_starts.iter().all(|start| *start >= 0.5 - 1.0e-9),
        "revealed before the fade-in: {reveal_starts:?}"
    );
}

/// A text its box composes anew at another width keeps the fill it was
/// recolored with after its declaration.
#[test]
fn responsive_layout_text_keeps_the_fill_it_was_recolored_with() {
    use gaanim_layout::{LayoutItemStyle, LayoutNodeKind, LayoutStyle, SizeRule};
    let red = PenikoColor::from_rgb8(255, 0, 0);
    let mut canvas = SceneModel::new(640, 360);
    let text = canvas.configured_text(
        "Wraps at the width its box offers",
        gaanim_text::prelude::TextStyle::default(),
        gaanim_text::prelude::TextFlow::default(),
    );
    let container = canvas.group(&[&text]);
    text.claim_layout(&container).unwrap();
    let reflow = |canvas: &mut SceneModel, width: f64, version: u64| {
        canvas.reflow_layout(
            &container,
            vec![crate::canvas::LayoutMemberSpec {
                id: text.id,
                style: LayoutItemStyle::default(),
            }],
            crate::canvas::LayoutSpec {
                kind: LayoutNodeKind::Column { wrap: false },
                style: LayoutStyle {
                    width: SizeRule::Fixed(width),
                    ..Default::default()
                },
                within: LayoutWithin::Safe,
            },
            version,
            None,
            None,
            None,
        );
    };
    reflow(&mut canvas, 2.0, 1);
    canvas.play(vec![text.animate().fill(red).duration(0.5)]);
    reflow(&mut canvas, 6.0, 2);
    canvas.wait(0.5);

    let (mut world, _) = compiled_world(&canvas);
    let recolored = world
        .query::<&FillBrush>()
        .iter(&world)
        .filter(|fill| fill.0 == Some(Brush::Solid(red)))
        .count();
    assert!(recolored > 0, "the copy at the new width lost its red fill");
}

/// The copy a layout composes a text into shows the text as it was: one
/// hidden with `opacity(0)`, or faded out before it is placed, stays
/// hidden until it fades in.
#[test]
fn responsive_layout_text_keeps_the_opacity_it_was_hidden_with() {
    let mut canvas = SceneModel::new(640, 360);
    let text = |canvas: &mut SceneModel, content: &str| {
        canvas.configured_text(
            content,
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
    };
    let hidden = text(&mut canvas, "Hidden with opacity").opacity(0.0);
    let faded = text(&mut canvas, "Faded out before it is placed");
    canvas.play(vec![faded.animate().fade_out().duration(0.5)]);
    let container = canvas.group(&[&hidden, &faded]);
    hidden.claim_layout(&container).unwrap();
    faded.claim_layout(&container).unwrap();
    canvas.reflow_layout(
        &container,
        [&hidden, &faded]
            .into_iter()
            .map(|member| crate::canvas::LayoutMemberSpec {
                id: member.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            })
            .collect(),
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Stack,
            style: gaanim_layout::LayoutStyle::default(),
            within: LayoutWithin::Safe,
        },
        1,
        None,
        None,
        None,
    );
    canvas.wait(0.5);
    canvas.play(vec![faded.animate().fade_in().duration(0.5)]);

    let (_, timeline) = compiled_world(&canvas);
    let reveal_starts: Vec<_> = timeline
        .clips
        .values()
        .filter(|clip| {
            matches!(
                &clip.payload,
                gaanim_timeline::clip::ClipPayload::Animation(
                    gaanim_timeline::clip::AnimationSpec {
                        lens: gaanim_timeline::clip::PropertyLensSpec::Opacity { to, .. },
                        ..
                    }
                ) if *to > 0.0
            )
        })
        .map(|clip| clip.start)
        .collect();
    assert!(
        reveal_starts
            .iter()
            .any(|start| (start - 1.0).abs() < 1.0e-9),
        "the authored fade-in must remain: {reveal_starts:?}"
    );
    assert!(
        reveal_starts.iter().all(|start| *start >= 1.0 - 1.0e-9),
        "a hidden text showed when it was placed: {reveal_starts:?}"
    );
}

#[test]
fn a_falloff_scales_group_members_by_their_distance_to_a_moving_target() {
    use gaanim_animation::{
        FalloffChannel, FalloffEffect, FalloffExpr, FalloffShape, FalloffTarget,
    };
    let mut canvas = SceneModel::new(640, 360);
    let members: Vec<_> = [-2.0, 0.0, 2.0]
        .into_iter()
        .map(|x| canvas.rect(1.0, 1.0).move_to_default(x, 0.0))
        .collect();
    let group = canvas.group(&members.iter().collect::<Vec<_>>());
    let cursor = canvas.rect(0.2, 0.2).move_to_default(-2.0, 0.0);
    group.drive_falloff(FalloffEffect::Scalar {
        channel: FalloffChannel::Scale,
        expr: FalloffExpr::Remap {
            input: Box::new(FalloffExpr::Distance {
                target: FalloffTarget::Object(cursor.id),
                radius: 2.0,
                shape: FalloffShape::Linear,
            }),
            low: 1.0,
            high: 3.0,
        },
    });
    canvas.play(vec![cursor.animate().move_to(2.0, 0.0).duration(2.0)]);

    let (mut world, mut timeline) = compiled_world(&canvas);
    let mut restore = Schedule::default();
    restore.add_systems(gaanim_animation::falloff::restore_falloff_system);
    let mut scales_at = |time: f64| {
        timeline.seek(&mut world, time);
        world.insert_resource(gaanim_animation::PlaybackState {
            current_time: time,
            ..Default::default()
        });
        gaanim_animation::falloff::apply_falloff_system(&mut world);
        let scales: Vec<f64> = members
            .iter()
            .map(|member| transform_of(&mut world, member).scale.x)
            .collect();
        restore.run(&mut world);
        scales
    };
    let start = scales_at(0.0);
    assert!((start[0] - 3.0).abs() < 1e-6, "under the cursor: {start:?}");
    assert!((start[1] - 1.0).abs() < 1e-6 && (start[2] - 1.0).abs() < 1e-6);
    let end = scales_at(2.0);
    assert!((end[2] - 3.0).abs() < 1e-6, "the cursor moved: {end:?}");
    assert!((end[0] - 1.0).abs() < 1e-6);
    let middle = scales_at(1.0);
    assert!(
        (middle[1] - 3.0).abs() < 0.2,
        "the cursor is over the middle member: {middle:?}"
    );
    // Seeking back reproduces the first frame exactly.
    assert_eq!(scales_at(0.0), start);
}

/// Opacity and scale of the object `id` (a handle id is one past its entity id).
fn object_state(world: &mut World, id: ObjectId) -> (f32, DVec3) {
    let id = ObjectId::from_raw(id.as_raw() - 1);
    world
        .query::<(&MobjectId, &Opacity, &SpatialTransform)>()
        .iter(world)
        .find(|(object, ..)| object.0 == id)
        .map(|(_, opacity, transform)| (opacity.0, transform.scale))
        .unwrap()
}

#[test]
fn blink_switches_the_target_off_and_on_that_many_times() {
    let mut canvas = SceneModel::new(640, 360);
    let rect = canvas.rect(1.0, 1.0);
    canvas.play(vec![rect.animate().blink(3)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let mut opacity_at = |time: f64| {
        timeline.seek(&mut world, time);
        opacity_of(&mut world, &rect)
    };
    // Three blinks of half a second each.
    assert_eq!(opacity_at(0.0), 1.0);
    assert!(opacity_at(0.15) < 0.01, "first blink is off");
    assert!((opacity_at(0.4) - 1.0).abs() < 0.01, "and back on");
    assert!(opacity_at(0.65) < 0.01, "second blink");
    assert!(opacity_at(1.15) < 0.01, "third blink");
    assert_eq!(opacity_at(1.5), 1.0, "visible again at the end");
}

#[test]
fn broadcast_ripples_grow_and_fade_one_after_another_from_where_the_source_is() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.circle(0.5).move_to_default(2.0, 1.0);
    let anim = source.animate().broadcast(3, 3.0, 0.5).unwrap();
    let AnimationType::Broadcast { ghosts, .. } = anim.inner.anim_type.clone() else {
        panic!("broadcast builds a Broadcast animation");
    };
    assert_eq!(ghosts.len(), 3);
    canvas.play(vec![anim.duration(3.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let mut ripples_at = |time: f64| {
        timeline.seek(&mut world, time);
        ghosts
            .iter()
            .map(|ghost| object_state(&mut world, *ghost))
            .collect::<Vec<_>>()
    };
    // Three ripples of 1.5 s, a ripple apart by half of one: 0.75 s.
    let before = ripples_at(0.0);
    for (opacity, _) in &before {
        assert_eq!(
            *opacity, 0.0,
            "nothing shows before the ripples start: {before:?}"
        );
    }
    let middle = ripples_at(0.75);
    assert!(
        middle[0].0 > 0.1 && middle[0].1.x > 1.5,
        "the first is growing: {middle:?}"
    );
    assert!(
        middle[1].0 < 0.01,
        "the second is only starting: {middle:?}"
    );
    assert!(middle[2].0 < 0.01);
    for (opacity, scale) in ripples_at(3.0) {
        assert!(opacity < 0.01, "every ripple has faded out");
        assert!((scale.x - 3.0).abs() < 1e-6, "at the full size: {scale:?}");
    }
    assert_eq!(
        ripples_at(0.75),
        middle,
        "seeking back reproduces the frame"
    );
}

#[test]
fn broadcast_rejects_values_that_cannot_ripple() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.circle(0.5);
    assert!(source.animate().broadcast(0, 3.0, 0.2).is_err());
    assert!(source.animate().broadcast(65, 3.0, 0.2).is_err());
    assert!(source.animate().broadcast(3, 0.5, 0.2).is_err());
    assert!(source.animate().broadcast(3, f64::NAN, 0.2).is_err());
    assert!(source.animate().broadcast(3, 3.0, -1.0).is_err());
    assert!(source.animate().fade_in().broadcast(3, 3.0, 0.2).is_err());
}

#[test]
fn a_spotlight_dims_and_returns_and_its_helpers_stay_invisible() {
    let mut canvas = SceneModel::new(640, 360);
    let target = canvas.rect(1.0, 1.0);
    let anim = canvas
        .spotlight(
            vec![crate::canvas::BoundsTarget::Drawable(target.id)],
            0.7,
            [0.1; 4],
            0.05,
        )
        .unwrap();
    let overlay = anim.inner.target;
    canvas.wait(1.0);
    canvas.play(vec![anim.duration(2.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let mut dim_at = |time: f64| {
        timeline.seek(&mut world, time);
        object_state(&mut world, overlay).0
    };
    assert!(dim_at(0.0) < 0.01, "nothing is dimmed before it starts");
    assert!(dim_at(0.99) < 0.01);
    assert!((dim_at(2.0) - 0.7).abs() < 0.01, "fully dimmed halfway");
    assert!(dim_at(3.0) < 0.01, "and back to normal");
    // The cover and the hole only shape the overlay.
    let overlay_entity = ObjectId::from_raw(overlay.as_raw() - 1);
    let target_entity = ObjectId::from_raw(target.id.as_raw() - 1);
    let helpers: Vec<f32> = world
        .query::<(&MobjectId, &Opacity)>()
        .iter(&world)
        .filter(|(object, _)| object.0 != overlay_entity && object.0 != target_entity)
        .map(|(_, opacity)| opacity.0)
        .collect();
    assert_eq!(helpers.len(), 2);
    assert!(helpers.iter().all(|opacity| *opacity == 0.0), "{helpers:?}");
    assert!(
        canvas
            .spotlight(
                vec![crate::canvas::BoundsTarget::Drawable(target.id)],
                0.0,
                [0.1; 4],
                0.0
            )
            .is_err()
    );
    assert!(
        canvas
            .spotlight(
                vec![crate::canvas::BoundsTarget::Drawable(target.id)],
                1.5,
                [0.1; 4],
                0.0
            )
            .is_err()
    );
}

/// World matrix of the entity whose local box is `width` wide.
fn world_matrix_of_width(world: &mut World, width: f64) -> gaanim_core::glam::DMat4 {
    let entity = world
        .query::<(Entity, &LocalBounds)>()
        .iter(world)
        .find(|(_, bounds)| (bounds.0.width() - width).abs() < 1e-6)
        .unwrap()
        .0;
    let mut matrix = gaanim_core::glam::DMat4::IDENTITY;
    let mut current = Some(entity);
    while let Some(node) = current {
        let local = world.get::<SpatialTransform>(node).unwrap().to_mat4();
        matrix = local * matrix;
        current = world.get::<ChildOf>(node).map(ChildOf::parent);
    }
    matrix
}

#[test]
fn an_svg_part_turns_about_a_scene_pivot() {
    let svg_file =
        std::env::temp_dir().join(format!("gaanim_svg_pivot_test_{}.svg", std::process::id()));
    std::fs::write(
        &svg_file,
        r##"<svg width="300" height="200" xmlns="http://www.w3.org/2000/svg">
                <rect id="early" x="20" y="40" width="100" height="20"/>
                <rect id="late" x="160" y="120" width="60" height="20"/>
              </svg>"##,
    )
    .unwrap();
    let mut canvas = SceneModel::new(640, 360);
    let svg = canvas.svg(&svg_file).unwrap();
    std::fs::remove_file(svg_file).unwrap();
    let svg = svg.scale_to(1.5).move_to(3.0, 1.0);
    let early = svg.part("early").unwrap();
    let late = svg.part("late").unwrap();
    // One pivot is declared with the SVG, the other at the cursor.
    let early_pivot = DVec3::new(-1.0, 2.0, 0.0);
    let late_pivot = DVec3::new(5.0, -2.0, 0.0);
    let early = early.with_pivot(early_pivot.x, early_pivot.y);
    canvas.wait(0.5);
    let late = late.with_pivot(late_pivot.x, late_pivot.y);
    canvas.play(vec![
        early.animate().rotate_by(1.0).duration(1.0),
        late.animate().rotate_by(-1.0).duration(1.0),
    ]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    for (width, pivot) in [(1.0, early_pivot), (0.6, late_pivot)] {
        timeline.seek(&mut world, 0.5);
        let local = world_matrix_of_width(&mut world, width)
            .inverse()
            .transform_point3(pivot);
        timeline.seek(&mut world, 1.5);
        let turned = world_matrix_of_width(&mut world, width).transform_point3(local);
        assert!(turned.distance(pivot) < 1e-6, "{width}: {turned:?}");
    }
}

#[test]
fn a_joint_turns_about_its_pivot_after_its_group_turned() {
    let mut canvas = SceneModel::new(640, 360);
    let upper = canvas.rect(2.0, 0.2).move_to(1.0, 0.0);
    let lower = canvas.rect(1.5, 0.2).move_to(2.75, 0.0);
    let joint = canvas.circle(0.1).move_to(2.0, 0.0);
    // The forearm's box is 1.6 wide, from the joint to its end.
    let forearm = canvas.group(&[&lower, &joint]).with_pivot(2.0, 0.0);
    let arm = canvas.group(&[&upper, &forearm]).with_pivot(0.0, 0.0);
    canvas.play(vec![arm.animate().rotate_by(0.5).duration(1.0)]);
    canvas.play(vec![forearm.animate().rotate_by(0.7).duration(1.0)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    let elbow = gaanim_core::glam::DQuat::from_rotation_z(0.5) * DVec3::new(2.0, 0.0, 0.0);
    timeline.seek(&mut world, 1.0);
    let local = world_matrix_of_width(&mut world, 1.6)
        .inverse()
        .transform_point3(elbow);
    timeline.seek(&mut world, 2.0);
    let turned = world_matrix_of_width(&mut world, 1.6).transform_point3(local);
    assert!(turned.distance(elbow) < 1e-6, "{turned:?}");
}

#[test]
fn a_drawable_left_out_of_the_post_process_marks_its_members() {
    let mut canvas = SceneModel::new(640, 360);
    let a = canvas.rect(1.0, 1.0);
    let b = canvas.circle(0.5);
    let _label = canvas.group(&[&a, &b]).post_process(false);
    let _kept = canvas.rect(2.0, 1.0);
    canvas.wait(0.5);
    let (mut world, _) = compiled_world(&canvas);
    let exempt = world
        .query_filtered::<(), With<gaanim_renderer::effects::PostProcessExempt>>()
        .iter(&world)
        .count();
    // The group and its two members, not the other rectangle.
    assert_eq!(exempt, 3);
}

#[test]
fn a_stepped_segment_steps_from_its_start_and_again_from_each_stop() {
    use gaanim_timeline::timeline::StepWindow;
    let mut canvas = SceneModel::new(640, 360);
    canvas.segment_with("smooth", None, None, None).unwrap();
    canvas.wait(1.0);
    let collage = canvas.segment_with("collage", None, None, None).unwrap();
    canvas.wait(1.1);
    canvas.stop(None).unwrap();
    canvas.wait(0.5);
    canvas.set_segment_step_rate(&collage, Some(12.0)).unwrap();
    assert!(canvas.set_segment_step_rate(&collage, Some(0.0)).is_err());
    let (_, timeline) = compiled_world(&canvas);
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let windows = &timeline.step_windows;
    assert_eq!(windows.len(), 2, "{windows:?}");
    let expected = [(1.0, 2.1), (2.1, 2.6)];
    for (window, (start, end)) in windows.iter().zip(expected) {
        assert!(
            close(window.start, start) && close(window.end, end),
            "{window:?}"
        );
        assert_eq!(window.rate, 12.0);
    }
    assert!(close(timeline.step_time(0.55), 0.55));
    assert!(close(timeline.step_time(1.1), 1.0 + 1.0 / 12.0));
    // The stop shows its exact frame, then the steps start from it.
    assert!(close(timeline.step_time(2.1), 2.1));
    assert!(close(timeline.step_time(2.15), 2.1));
    let _: &[StepWindow] = windows;
}

#[test]
fn a_step_rate_marks_a_group_and_its_members() {
    let mut canvas = SceneModel::new(640, 360);
    let a = canvas.rect(1.0, 1.0);
    let b = canvas.circle(0.5);
    let _cutout = canvas.group(&[&a, &b]).step_rate(Some(12.0)).unwrap();
    b.step_rate(Some(6.0)).unwrap();
    let _smooth = canvas.rect(2.0, 1.0);
    assert!(canvas.rect(1.0, 1.0).step_rate(Some(f64::NAN)).is_err());
    canvas.wait(0.5);
    let (mut world, timeline) = compiled_world(&canvas);
    assert!(timeline.stepped_objects);
    let mut rates: Vec<f64> = world
        .query::<&gaanim_scene::StepRate>()
        .iter(&world)
        .map(|rate| rate.0)
        .collect();
    rates.sort_by(f64::total_cmp);
    // The circle keeps its own rate; the group and the rectangle take 12.
    assert_eq!(rates, [6.0, 12.0, 12.0]);
}

#[test]
fn segments_blur_their_motion_their_own_way() {
    use gaanim_renderer::effects::{MotionBlur, MotionBlurWindows};
    let blur = |angle: f64| MotionBlur::new(angle, 8, None).unwrap();
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_motion_blur(Some(blur(180.0)));
    let mut segments = Vec::new();
    for name in ["scene", "wide", "sharp"] {
        segments.push(canvas.segment_with(name, None, None, None).unwrap());
        canvas.wait(1.0);
    }
    canvas
        .set_segment_motion_blur(&segments[1], Some(blur(360.0)))
        .unwrap();
    canvas.set_segment_motion_blur(&segments[2], None).unwrap();
    let (world, _) = compiled_world(&canvas);
    assert!(world.get_resource::<MotionBlur>().is_none());
    let windows = world.resource::<MotionBlurWindows>();
    assert_eq!(windows.at(0.5), Some(blur(180.0)));
    assert_eq!(windows.at(1.5), Some(blur(360.0)));
    assert_eq!(windows.at(2.5), None);
    // A frame on a cut belongs to the segment it enters.
    assert_eq!(windows.at(1.0), Some(blur(360.0)));
    assert_eq!(windows.at(2.0), None);
}

#[test]
fn an_outline_follows_the_shader_effect_passes() {
    use gaanim_renderer::object_effects::{ShaderEffect, is_outline_shader};
    let mut canvas = SceneModel::new(640, 360);
    let outline = crate::canvas::OutlineSpec {
        color: PenikoColor::WHITE,
        width: 0.1,
    };
    let plain = canvas.rect(1.0, 1.0).outline(Some(outline));
    let grade = gaanim_renderer::post_presets::PostPreset::ColorGrade.shader();
    let graded = canvas
        .rect(1.0, 1.0)
        .move_to(3.0, 0.0)
        .outline(Some(outline))
        .shader_effect(
            vec![
                gaanim_renderer::post_process::PostProcessPass::constant(
                    grade,
                    &[0.0, 1.0, 0.0, 0.0],
                )
                .unwrap(),
            ],
            0.3,
        );
    let removed = canvas
        .rect(1.0, 1.0)
        .move_to(-3.0, 0.0)
        .outline(Some(outline))
        .outline(None);
    let _ = (plain, graded, removed);
    let (mut world, _) = compiled_world(&canvas);
    let mut effects: Vec<(Vec<bool>, f64)> = world
        .query::<&ShaderEffect>()
        .iter(&world)
        .map(|effect| {
            let outlined = effect
                .post
                .passes
                .iter()
                .map(|pass| is_outline_shader(&pass.shader))
                .collect();
            (outlined, effect.margin)
        })
        .collect();
    effects.sort_by_key(|(passes, _)| passes.len());
    // The removed outline leaves no effect; the plain one is two passes.
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0].0, vec![true, true]);
    assert!((effects[0].1 - (0.1 + OUTLINE_ANTIALIAS_MARGIN)).abs() < 1e-12);
    // The outline traces what the effect's passes leave, beyond their reach.
    assert_eq!(effects[1].0, vec![false, true, true]);
    assert!((effects[1].1 - (0.3 + 0.1 + OUTLINE_ANTIALIAS_MARGIN)).abs() < 1e-12);
}

#[test]
fn a_spotlight_cuts_a_hole_around_each_target() {
    let mut canvas = SceneModel::new(640, 360);
    let left = canvas.rect(1.0, 1.0).move_to(-4.0, 1.0);
    let right = canvas.rect(1.0, 1.0).move_to(4.0, -1.0);
    let anim = canvas
        .spotlight(
            vec![
                crate::canvas::BoundsTarget::Drawable(left.id),
                crate::canvas::BoundsTarget::Drawable(right.id),
            ],
            0.7,
            [0.1; 4],
            0.05,
        )
        .unwrap();
    canvas.play(vec![anim]);
    let (mut world, _) = compiled_world(&canvas);
    let operands: Vec<usize> = world
        .query::<&gaanim_renderer::effects::BooleanBinding>()
        .iter(&world)
        .map(|binding| binding.sources.len())
        .collect();
    // The cover, then one hole per target.
    assert_eq!(operands, vec![3]);
    assert!(canvas.spotlight(Vec::new(), 0.7, [0.1; 4], 0.05).is_err());
}

#[test]
fn a_spotlight_on_a_text_selection_cuts_one_hole_per_line() {
    let mut canvas = SceneModel::new(640, 360);
    let card = canvas.rect(1.0, 1.0).move_to(-4.0, 0.0);
    let text = canvas.text("uno dos tres");
    let anim = canvas
        .spotlight(
            vec![
                crate::canvas::BoundsTarget::Drawable(card.id),
                crate::canvas::BoundsTarget::TextSelection {
                    target: text.id,
                    fragment: "dos".into(),
                    occurrence: None,
                },
            ],
            0.7,
            [0.1; 4],
            0.05,
        )
        .unwrap();
    canvas.play(vec![anim]);
    let (mut world, _) = compiled_world(&canvas);
    let mut shapes: Vec<(gaanim_animation::SurroundingShape, usize)> = world
        .query::<&gaanim_animation::SurroundingRect>()
        .iter(&world)
        .map(|frame| (frame.shape, frame.to.len()))
        .collect();
    shapes.sort_by_key(|(shape, _)| *shape == gaanim_animation::SurroundingShape::Lines);
    // The card keeps its frame; the selection's three glyphs get lines.
    assert_eq!(
        shapes,
        vec![
            (gaanim_animation::SurroundingShape::Frame, 1),
            (gaanim_animation::SurroundingShape::Lines, 3),
        ]
    );
}

#[test]
fn flashes_sweep_a_frame_or_an_underline_that_follow_the_target() {
    let mut canvas = SceneModel::new(640, 360);
    let target = canvas.rect(2.0, 1.0).move_to_default(1.0, 1.0);
    let around = target
        .animate()
        .flash_around(None, None, [0.1; 4], 0.05, 0.4)
        .unwrap();
    let under = target
        .animate()
        .flash_under(None, None, 0.08, 0.1, 0.5)
        .unwrap();
    assert!(matches!(
        around.inner.anim_type,
        AnimationType::ShowPassingFlash { time_width } if time_width == 0.4
    ));
    canvas.play(vec![around, under]);
    let flash = |width: Option<f64>, padding: [f64; 4], time_width: f64| {
        target
            .animate()
            .flash_around(None, width, padding, 0.0, time_width)
    };
    assert!(
        flash(Some(0.1), [0.1; 4], 0.4).is_err(),
        "a width needs a color"
    );
    assert!(flash(None, [-1.0; 4], 0.4).is_err());
    assert!(flash(None, [0.1; 4], 0.0).is_err());
    assert!(
        target
            .animate()
            .flash_under(None, None, 0.1, 0.0, 1.5)
            .is_err()
    );
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.5);
    let mut shapes = world
        .query::<&gaanim_animation::SurroundingRect>()
        .iter(&world)
        .map(|frame| frame.shape)
        .collect::<Vec<_>>();
    shapes.sort_by_key(|shape| *shape as u8);
    assert_eq!(
        shapes,
        vec![
            gaanim_animation::SurroundingShape::Frame,
            gaanim_animation::SurroundingShape::Underline
        ]
    );
}

#[test]
fn an_animated_boundary_cycles_its_stroke_through_the_colors_from_the_cursor() {
    let mut canvas = SceneModel::new(640, 360);
    let card = canvas.rect(2.0, 1.0);
    canvas.wait(1.0);
    let red = gaanim_core::peniko::Color::from_rgb8(255, 0, 0);
    let blue = gaanim_core::peniko::Color::from_rgb8(0, 0, 255);
    card.animated_boundary(vec![red, blue], 0.5, 0.04, [0.1; 4], 0.05)
        .unwrap();
    assert!(
        card.animated_boundary(vec![red], 0.5, 0.04, [0.1; 4], 0.05)
            .is_err()
    );
    assert!(
        card.animated_boundary(vec![red, blue], f64::NAN, 0.04, [0.1; 4], 0.05)
            .is_err()
    );
    assert!(
        card.animated_boundary(vec![red, blue], 0.5, 0.0, [0.1; 4], 0.05)
            .is_err()
    );
    canvas.wait(3.0);
    let (mut world, _) = compiled_world(&canvas);
    let cycle = world
        .query::<&gaanim_animation::StrokeCycle>()
        .single(&world)
        .unwrap()
        .clone();
    assert_eq!(cycle.start, 1.0, "it starts where it was declared");
    assert_eq!(cycle.rate, 0.5);
    assert_eq!(cycle.color_at(1.0), red);
    // A turn takes two seconds: the color list has two entries, so the
    // second one is reached after one of them.
    assert_eq!(cycle.color_at(2.0), blue);
}

#[test]
fn connectors_with_fixed_points_are_placed_by_the_box_that_holds_them() {
    use crate::canvas::ops::CanvasEndpoint;
    let mut canvas = SceneModel::new(640, 360);
    let connector = |canvas: &mut SceneModel| {
        canvas
            .connector(
                CanvasEndpoint::Static(DVec3::ZERO),
                CanvasEndpoint::Static(DVec3::new(1.5, 0.0, 0.0)),
                Vec::new(),
                0.18,
                0.15,
                0.036,
                None,
            )
            .unwrap()
    };
    let (first, second) = (connector(&mut canvas), connector(&mut canvas));
    let container = canvas.group(&[&first, &second]);
    first.claim_layout(&container).unwrap();
    second.claim_layout(&container).unwrap();
    canvas.reflow_layout(
        &container,
        [&first, &second]
            .into_iter()
            .map(|member| crate::canvas::LayoutMemberSpec {
                id: member.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            })
            .collect(),
        crate::canvas::LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
            style: gaanim_layout::LayoutStyle::default(),
            within: LayoutWithin::Safe,
        },
        1,
        None,
        None,
        None,
    );
    canvas.wait(0.5);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.25);
    let mut fixed = world.query::<&gaanim_animation::updaters::TrackingConnector>();
    assert!(
        fixed.iter(&world).all(|connector| connector.local),
        "a connector a box holds draws its own points"
    );
    let (a, b) = (
        transform_of(&mut world, &first).translation,
        transform_of(&mut world, &second).translation,
    );
    assert!(
        (a.y - b.y).abs() > 1e-6,
        "each connector sits in its own cell of the column: {a:?} and {b:?}"
    );
}

#[test]
fn check_reports_content_that_leaves_its_box_and_only_that() {
    let report = |height: f64| {
        let mut canvas = SceneModel::new(640, 360);
        let members = [canvas.rect(2.0, 1.0), canvas.rect(2.0, 1.0)];
        let container = canvas.group(&members.iter().collect::<Vec<_>>());
        for member in &members {
            member.claim_layout(&container).unwrap();
        }
        canvas.reflow_layout(
            &container,
            members
                .iter()
                .map(|member| crate::canvas::LayoutMemberSpec {
                    id: member.id,
                    style: gaanim_layout::LayoutItemStyle::default(),
                })
                .collect(),
            crate::canvas::LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
                style: gaanim_layout::LayoutStyle {
                    height: gaanim_layout::SizeRule::Fixed(height),
                    ..gaanim_layout::LayoutStyle::default()
                },
                within: LayoutWithin::Safe,
            },
            1,
            None,
            None,
            None,
        );
        canvas
            .compiled_layout_diagnostics()
            .into_iter()
            .map(|(_, message)| message)
            .collect::<Vec<_>>()
    };
    let tight = report(1.5);
    assert!(
        tight
            .iter()
            .any(|message| message.contains("content leaves its box by")
                && message.contains("in segment \"")),
        "two boxes of height 1 do not fit in 1.5: {tight:?}"
    );
    let roomy = report(2.5);
    assert!(
        roomy
            .iter()
            .all(|message| !message.contains("leaves its box") && !message.contains("overlaps")),
        "they fit in 2.5: {roomy:?}"
    );
}

/// A row too narrow for its texts on one line breaks them, and check
/// says so; a row with room, or a text that chose its wrap width, not.
#[test]
fn check_reports_rows_that_break_their_texts() {
    let report = |width: f64, wrap: gaanim_text::prelude::TextWrap| {
        let mut canvas = SceneModel::new(16.0, 9.0);
        let members: Vec<_> = ["Un sismo", "fuerte"]
            .into_iter()
            .map(|word| {
                let spec = gaanim_text::prelude::TextSpec::new(
                    vec![word.into()],
                    None,
                    gaanim_text::prelude::TextStyle::default(),
                    gaanim_text::prelude::TextFlow {
                        wrap,
                        ..Default::default()
                    },
                )
                .unwrap();
                canvas.text_spec(spec)
            })
            .collect();
        let container = canvas.group(&members.iter().collect::<Vec<_>>());
        for member in &members {
            member.claim_layout(&container).unwrap();
        }
        canvas.reflow_layout(
            &container,
            members
                .iter()
                .map(|member| crate::canvas::LayoutMemberSpec {
                    id: member.id,
                    style: gaanim_layout::LayoutItemStyle::default(),
                })
                .collect(),
            crate::canvas::LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Row { wrap: false },
                style: gaanim_layout::LayoutStyle {
                    width: gaanim_layout::SizeRule::Fixed(width),
                    ..gaanim_layout::LayoutStyle::default()
                },
                within: LayoutWithin::Safe,
            },
            1,
            None,
            None,
            None,
        );
        canvas
            .compiled_layout_diagnostics()
            .into_iter()
            .map(|(_, message)| message)
            .filter(|message| message.contains("leaves less than a line"))
            .collect::<Vec<_>>()
    };
    let auto = gaanim_text::prelude::TextWrap::Auto;
    let tight = report(1.2, auto);
    assert_eq!(tight.len(), 1, "{tight:?}");
    assert!(
        tight[0].contains("to text \"Un sismo\", so it breaks"),
        "{tight:?}"
    );
    assert!(report(12.0, auto).is_empty());
    let chosen = report(1.2, gaanim_text::prelude::TextWrap::Width(0.5));
    assert!(chosen.is_empty(), "{chosen:?}");
}

/// A row of two rectangles, laid out twice with a play in between.
fn reflowed_row(
    animate: impl FnOnce(&mut SceneModel, &DrawableHandle, &DrawableHandle, &DrawableHandle),
) -> (
    World,
    Timeline,
    DrawableHandle,
    DrawableHandle,
    DrawableHandle,
) {
    let mut canvas = SceneModel::new(640, 360);
    let first = canvas.rect(2.0, 1.0);
    let second = canvas.rect(2.0, 1.0);
    let container = canvas.group(&[&first, &second]);
    first.claim_layout(&container).unwrap();
    second.claim_layout(&container).unwrap();
    let reflow = |canvas: &mut SceneModel, version| {
        let members = [&first, &second]
            .into_iter()
            .map(|member| crate::canvas::LayoutMemberSpec {
                id: member.id,
                style: gaanim_layout::LayoutItemStyle::default(),
            })
            .collect();
        canvas.reflow_layout(
            &container,
            members,
            crate::canvas::LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Row { wrap: false },
                style: gaanim_layout::LayoutStyle::default(),
                within: LayoutWithin::Safe,
            },
            version,
            None,
            None,
            None,
        );
    };
    reflow(&mut canvas, 1);
    canvas.wait(0.5);
    animate(&mut canvas, &container, &first, &second);
    reflow(&mut canvas, 2);
    canvas.wait(0.5);
    let (world, timeline) = compiled_world(&canvas);
    (world, timeline, container, first, second)
}

#[test]
fn a_member_grow_entry_stays_hidden_when_its_box_reflows_afterwards() {
    let (mut world, mut timeline, _, first, second) = reflowed_row(|canvas, _, first, _| {
        canvas.play(vec![first.animate().grow_from_center().duration(0.5)]);
    });
    timeline.seek(&mut world, 0.25);
    assert_eq!(transform_of(&mut world, &first).scale, DVec3::ZERO);
    assert_ne!(transform_of(&mut world, &second).scale, DVec3::ZERO);
    timeline.seek(&mut world, 1.5);
    assert!(transform_of(&mut world, &first).scale.x > 0.99);
}

#[test]
fn a_box_grow_entry_stays_hidden_when_it_reflows_afterwards() {
    let (mut world, mut timeline, container, ..) = reflowed_row(|canvas, container, _, _| {
        canvas.play(vec![container.animate().grow_from_center().duration(0.5)]);
    });
    timeline.seek(&mut world, 0.25);
    assert_eq!(transform_of(&mut world, &container).scale, DVec3::ZERO);
    timeline.seek(&mut world, 1.5);
    assert!(transform_of(&mut world, &container).scale.x > 0.99);
}

#[test]
fn an_entry_stays_hidden_when_the_object_is_indicated_later() {
    // A quiz tile: it enters after a while, then the right answer pulses.
    let mut canvas = SceneModel::new(640, 360);
    let tile = canvas.rect(2.0, 1.0);
    canvas.wait(1.0);
    canvas.play(vec![tile.animate().grow_from_center().duration(0.5)]);
    canvas.play(vec![tile.animate().indicate(None, 1.2).duration(0.6)]);
    let (mut world, mut timeline) = compiled_world(&canvas);
    timeline.seek(&mut world, 0.5);
    assert_eq!(transform_of(&mut world, &tile).scale, DVec3::ZERO);
    timeline.seek(&mut world, 2.5);
    assert!((transform_of(&mut world, &tile).scale.x - 1.0).abs() < 1e-9);
}

#[test]
fn points_compile_to_one_path_with_a_circle_per_position() {
    let mut canvas = SceneModel::new(640, 360);
    assert!(canvas.points(Vec::new(), 0.1).is_err());
    assert!(canvas.points(vec![(0.0, f64::NAN)], 0.1).is_err());
    assert!(canvas.points(vec![(0.0, 0.0)], 0.0).is_err());
    canvas
        .points(vec![(-1.0, 0.0), (2.0, 1.0), (0.5, -3.0)], 0.25)
        .expect("valid point cloud");

    let mut world = compile_canvas_for_layout(canvas);
    let mut query = world.query::<(
        &gaanim_scene::ObjectTag,
        &gaanim_scene::Path2D,
        &gaanim_scene::LocalBounds,
    )>();
    let (_, path, bounds) = query
        .iter(&world)
        .find(|(tag, _, _)| tag.0 == "Points")
        .expect("one Points object");
    let subpaths = path
        .0
        .elements()
        .iter()
        .filter(|element| matches!(element, gaanim_core::kurbo::PathEl::MoveTo(_)))
        .count();
    assert_eq!(subpaths, 3);
    assert_eq!(
        bounds.0.min.truncate(),
        gaanim_core::glam::DVec2::new(-1.25, -3.25)
    );
    assert_eq!(
        bounds.0.max.truncate(),
        gaanim_core::glam::DVec2::new(2.25, 1.25)
    );
}

/// A box inside another, both with a background and starting with the
/// same content: the backgrounds share a render order, and only their
/// depth draws the outer one first.
#[test]
fn nested_box_backgrounds_draw_outermost_first() {
    use crate::canvas::{LayoutMemberSpec, LayoutSpec, LayoutWithin};
    use gaanim_layout::LayoutNodeKind;

    fn decorated(scene: &mut SceneModel, member: &DrawableHandle) -> DrawableHandle {
        let root = scene.group(&[member]);
        member.claim_layout(&root).unwrap();
        scene
            .decorate_layout(&root, Some(PenikoColor::WHITE.into()), None, 0.0)
            .unwrap();
        let spec = LayoutSpec {
            kind: LayoutNodeKind::Row { wrap: false },
            style: Default::default(),
            within: LayoutWithin::Intrinsic,
        };
        let members = vec![LayoutMemberSpec {
            id: member.id,
            style: member.layout_item(),
        }];
        scene.reflow_layout(&root, members, spec, 1, None, None, None);
        root
    }

    let mut canvas = SceneModel::new(640, 360);
    let number = canvas.reactive_readout(
        gaanim_animation::ScalarSource::constant(44.0),
        ".0f",
        "",
        "%",
        "—",
        None,
    );
    let inner = decorated(&mut canvas, &number);
    decorated(&mut canvas, &inner);
    let mut world = compile_canvas_for_layout(canvas);
    let mut query = world.query::<(&gaanim_scene::LayoutBackdrop, &RenderOrder)>();
    let mut backdrops = query
        .iter(&world)
        .map(|(backdrop, order)| (backdrop.depth, *order))
        .collect::<Vec<_>>();
    backdrops.sort_by_key(|(depth, _)| *depth);
    assert_eq!(
        backdrops
            .iter()
            .map(|(depth, _)| *depth)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(backdrops[0].1, backdrops[1].1);
}

/// The render order holds for the whole timeline: a child created before
/// the rest of its decorated box and replaced later stays above the
/// box's background until it leaves.
#[test]
fn box_background_stays_beneath_a_child_replaced_later() {
    use crate::canvas::{LayoutMemberSpec, LayoutSpec, LayoutWithin};
    use gaanim_layout::LayoutNodeKind;

    fn snapshot(members: &[&DrawableHandle]) -> Vec<LayoutMemberSpec> {
        members
            .iter()
            .map(|member| LayoutMemberSpec {
                id: member.id,
                style: member.layout_item(),
            })
            .collect()
    }

    let mut canvas = SceneModel::new(640, 360);
    let old = canvas.rect(1.0, 0.5);
    let first = canvas.rect(2.0, 0.5);
    let root = canvas.group(&[&first, &old]);
    first.claim_layout(&root).unwrap();
    old.claim_layout(&root).unwrap();
    // A Python box puts its background on its content's lowest layer.
    let background = canvas
        .decorate_layout(&root, Some(PenikoColor::WHITE.into()), None, 0.0)
        .unwrap()
        .z_index(0);
    let spec = LayoutSpec {
        kind: LayoutNodeKind::Column { wrap: false },
        style: Default::default(),
        within: LayoutWithin::Intrinsic,
    };
    canvas.reflow_layout(
        &root,
        snapshot(&[&first, &old]),
        spec.clone(),
        1,
        None,
        None,
        None,
    );
    canvas.wait(1.0);
    let new = canvas.rect(1.5, 0.5);
    new.claim_layout(&root).unwrap();
    old.release_layout(&root);
    canvas.set_group_members(&root, &[&first, &new]);
    canvas.reflow_layout(
        &root,
        snapshot(&[&first, &new]),
        spec,
        2,
        Some(0.5),
        Some(&new),
        Some(&old),
    );
    canvas.wait(0.5);

    let mut world = compile_canvas_for_layout(canvas);
    let mut order = |handle: &DrawableHandle| {
        let entity = entity_of(&mut world, handle);
        *world.get::<RenderOrder>(entity).unwrap()
    };
    let (background, old) = (order(&background), order(&old));
    assert_eq!(background.z_index, old.z_index);
    assert!(
        background.creation_order <= old.creation_order,
        "the background draws over the replaced child: {background:?} vs {old:?}"
    );
}

/// A readout inside a layout keeps room for the widest value its
/// parameter takes, so its cell holds it while it counts; one outside a
/// layout keeps none.
#[test]
fn readouts_in_layouts_reserve_their_widest_value() {
    use crate::canvas::{LayoutMemberSpec, LayoutSpec, LayoutWithin};
    use gaanim_layout::LayoutNodeKind;

    let mut canvas = SceneModel::new(640, 360);
    let parameter = canvas.parameter(0.0).unwrap();
    let readout = |canvas: &mut SceneModel| {
        let number = canvas.reactive_readout(parameter.source(), ".0f", "", "%", "—", None);
        canvas.reactive_readout_group(None, None, &number, None, 0.1)
    };
    let placed = readout(&mut canvas);
    let root = canvas.group(&[&placed]);
    placed.claim_layout(&root).unwrap();
    let spec = LayoutSpec {
        kind: LayoutNodeKind::Row { wrap: false },
        style: Default::default(),
        within: LayoutWithin::Intrinsic,
    };
    let members = vec![LayoutMemberSpec {
        id: placed.id,
        style: placed.layout_item(),
    }];
    canvas.reflow_layout(&root, members, spec, 1, None, None, None);
    let _free = readout(&mut canvas);
    canvas.wait(0.5);
    parameter.set(4444.0).unwrap();
    canvas.wait(0.5);

    let mut world = compile_canvas_for_layout(canvas);
    let mut query = world.query::<(&gaanim_animation::ReactiveReadout, &LocalBounds)>();
    let mut readouts = query
        .iter(&world)
        .map(|(readout, bounds)| (readout.reserve, bounds.0.width(), readout.last_text.clone()))
        .collect::<Vec<_>>();
    readouts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let [(free, _, _), (reserve, width, text)] = readouts.as_slice() else {
        panic!("two readouts: {readouts:?}");
    };
    assert_eq!(*free, 0.0);
    assert_eq!(text, "0%");
    let registry = gaanim_text::font::FontRegistry::new();
    let registry = &registry;
    let body = gaanim_text::prelude::TextConfig::default();
    let body = &body.roles[&gaanim_text::prelude::TextRole::Body];
    let widest = |number: &str| {
        gaanim_animation::shape_readout_text_with_weight(
            registry,
            "",
            number,
            "%",
            &body.font_family,
            None,
            body.size,
        )
        .unwrap()
        .1
        .width()
    };
    assert!((reserve - widest("4444")).abs() < 1e-9, "{reserve}");
    assert!(widest("0") < *reserve);
    assert!(
        (width - reserve).abs() < 1e-9,
        "the cell holds the widest text"
    );
}

#[test]
fn readout_decimal_separator_reaches_the_compiled_text() {
    let mut canvas = SceneModel::new(640, 360);
    let number = canvas.reactive_readout(
        gaanim_animation::ScalarSource::constant(1.23456),
        ".2f",
        "",
        "",
        "—",
        None,
    );
    assert!(number.readout_decimal_separator('7').is_err());
    number.readout_decimal_separator(',').unwrap();
    let mut world = compile_canvas_for_layout(canvas);
    let mut query = world.query::<&gaanim_animation::ReactiveReadout>();
    let readout = query.single(&world).expect("exactly one compiled readout");
    assert_eq!(readout.last_text, "1,23");
}

/// `T = 1.62 s` is set as type is: `spacing` apart from each term's
/// advance, which holds its ink, rather than from the ink.
#[test]
fn readout_rows_set_their_terms_by_their_advance() {
    let mut canvas = SceneModel::new(640, 360);
    let number = canvas.reactive_readout(
        gaanim_animation::ScalarSource::constant(1.62),
        ".2f",
        "",
        "",
        "—",
        None,
    );
    let text = |canvas: &mut SceneModel, value: &str| {
        let spec = gaanim_text::prelude::TextSpec::new(
            vec![value.into()],
            None,
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .unwrap();
        canvas.text_spec(spec)
    };
    let label = text(&mut canvas, "$T$");
    let equals = text(&mut canvas, "=");
    let unit = text(&mut canvas, "s");
    canvas.reactive_readout_group(Some(&label), Some(&equals), &number, Some(&unit), 0.1);
    let mut world = compile_canvas_for_layout(canvas);
    let layout = world
        .query::<&gaanim_animation::ReactiveReadoutLayout>()
        .single(&world)
        .unwrap()
        .clone();
    let parts = [
        layout.label,
        layout.equals,
        Some(layout.number),
        layout.unit,
    ];
    let mut edges = Vec::new();
    for id in parts.into_iter().flatten() {
        let (advance, x, ink) = world
            .query::<(
                &MobjectId,
                &gaanim_scene::TextAdvance,
                &SpatialTransform,
                &LocalBounds,
            )>()
            .iter(&world)
            .find(|(object, ..)| object.0 == id)
            .map(|(_, advance, transform, ink)| (*advance, transform.translation.x, ink.0))
            .expect("each term keeps its advance");
        assert!(advance.left <= ink.min.x && ink.max.x <= advance.right);
        edges.push((x + advance.left, x + advance.right));
    }
    assert_eq!(edges.len(), 4);
    for pair in edges.windows(2) {
        assert!((pair[1].0 - pair[0].1 - 0.1).abs() < 1e-9, "{edges:?}");
    }
}

#[test]
fn sampled_series_op_compiles_to_driver_component() {
    use gaanim_animation::{SampledInterpolation, SampledProperty};

    let mut canvas = SceneModel::new(640, 360);
    let dot = canvas.dot(8.0);
    canvas.wait(1.5);
    dot.drive_from_samples(
        vec![0.0, 1.0],
        vec![1.0, 2.0],
        SampledProperty::TranslateX,
        SampledInterpolation::Linear,
        10.0,
        0.0,
    )
    .expect("valid sampled series");

    dot.drive_from_samples(
        vec![0.0, 1.0],
        vec![3.0, 4.0],
        SampledProperty::TranslateY,
        SampledInterpolation::Step,
        1.0,
        0.0,
    )
    .expect("valid sampled series");

    let mut world = compile_canvas_for_layout(canvas);
    let mut query = world.query::<&gaanim_animation::SampledSeriesDrivers>();
    let drivers = query.single(&world).expect("exactly one driven entity");
    assert_eq!(drivers.0.len(), 2, "x and y channels are independent");
    let y = drivers.get(SampledProperty::TranslateY).expect("y channel");
    assert_eq!(y.values.to_vec(), vec![3.0, 4.0]);
    let driver = drivers.get(SampledProperty::TranslateX).expect("x channel");
    assert_eq!(driver.property, SampledProperty::TranslateX);
    assert_eq!(driver.interpolation, SampledInterpolation::Linear);
    assert_eq!(driver.times.to_vec(), vec![0.0, 1.0]);
    assert_eq!(driver.values.to_vec(), vec![1.0, 2.0]);
    assert_eq!(driver.scale, 10.0);
    assert_eq!(driver.start_at, 1.5, "driver starts at the authored cursor");
    assert!(driver.base.is_none());
}

/// One series drives several drawables, each at its own scale, and is
/// kept once.
#[test]
fn one_series_drives_each_target_at_its_scale() {
    use gaanim_animation::{SampledInterpolation, SampledProperty};

    let mut canvas = SceneModel::new(640, 360);
    let low = canvas.rect(1.0, 1.0);
    let high = canvas.rect(1.0, 1.0);
    assert!(
        DrawableHandle::drive_each_from_samples(
            &[(&low, f64::NAN)],
            vec![0.0, 1.0],
            vec![0.0, 1.0],
            SampledProperty::TranslateX,
            SampledInterpolation::Linear,
            0.0,
            false,
        )
        .is_err()
    );
    DrawableHandle::drive_each_from_samples(
        &[(&low, 0.5), (&high, 2.0)],
        vec![0.0, 1.0],
        vec![0.0, 1.0],
        SampledProperty::TranslateX,
        SampledInterpolation::Linear,
        0.0,
        false,
    )
    .unwrap();
    let mut world = compile_canvas_for_layout(canvas);
    let drivers: Vec<_> = world
        .query::<&gaanim_animation::SampledSeriesDrivers>()
        .iter(&world)
        .map(|drivers| drivers.get(SampledProperty::TranslateX).unwrap().clone())
        .collect();
    assert_eq!(drivers.len(), 2);
    let mut scales: Vec<f64> = drivers.iter().map(|driver| driver.scale).collect();
    scales.sort_by(f64::total_cmp);
    assert_eq!(scales, vec![0.5, 2.0]);
    assert!(Arc::ptr_eq(&drivers[0].values, &drivers[1].values));
    assert!(Arc::ptr_eq(&drivers[0].times, &drivers[1].times));
}

/// Bars of a leaderboard, named after their player, grouped in rank order.
fn magic_move_board(canvas: &mut SceneModel, rows: &[(&str, f64)]) -> DrawableHandle {
    let bars: Vec<DrawableHandle> = rows
        .iter()
        .enumerate()
        .map(|(rank, (name, score))| {
            canvas
                .rect(*score, 0.6)
                .move_to(score / 2.0 - 3.0, 1.5 - rank as f64)
                .named(*name)
        })
        .collect();
    canvas.group(&bars.iter().collect::<Vec<_>>())
}

fn labelled_clips(timeline: &Timeline, label: &str) -> Vec<(f64, f64)> {
    let mut clips: Vec<(f64, f64)> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(
                gaanim_timeline::clip::AnimationSpec {
                    label: Some(found), ..
                },
            ) if found == label => Some((clip.start, clip.duration)),
            _ => None,
        })
        .collect();
    clips.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    clips
}

/// Opacity of `entity` as drawn: its own times its ancestors'.
fn drawn_opacity(world: &World, entity: Entity) -> f32 {
    let mut opacity = 1.0;
    let mut current = Some(entity);
    while let Some(node) = current {
        opacity *= world.get::<Opacity>(node).map_or(1.0, |value| value.0);
        current = world.get::<ChildOf>(node).map(ChildOf::parent);
    }
    opacity
}

/// The drawn opacities of the glyphs of each root with children, from
/// left to right.
fn glyph_opacities(world: &mut World) -> Vec<Vec<f32>> {
    let mut roots: Vec<(Entity, f64)> = world
        .query_filtered::<(Entity, &SpatialTransform), (With<Children>, Without<ChildOf>)>()
        .iter(world)
        .map(|(entity, transform)| (entity, transform.translation.x))
        .collect();
    roots.sort_by(|a, b| a.1.total_cmp(&b.1));
    roots
        .into_iter()
        .map(|(root, _)| {
            let glyphs: Vec<Entity> = world
                .query::<(Entity, &ChildOf)>()
                .iter(world)
                .filter(|(_, child_of)| child_of.parent() == root)
                .map(|(entity, _)| entity)
                .collect();
            glyphs
                .into_iter()
                .map(|glyph| drawn_opacity(world, glyph))
                .collect()
        })
        .collect()
}

#[test]
fn matching_ends_on_the_target_in_every_mode() {
    for (mode, after) in [
        ("tex", "abd"),
        ("text", "abd"),
        ("shapes", "abd"),
        ("shapes", "abcd"),
    ] {
        let mut canvas = SceneModel::new(640, 360);
        let source = canvas.text("abc").move_to(-2.0, 0.0);
        let target = canvas.text(after).move_to(2.0, 0.0);
        canvas.wait(0.5);
        canvas
            .transform_matching(&source, &target, mode, 1.0)
            .unwrap();
        canvas.wait(0.5);
        let (mut world, mut timeline) = compiled_world(&canvas);
        timeline.seek(&mut world, 1.9);
        let [before, after_glyphs] = glyph_opacities(&mut world).try_into().unwrap();
        assert!(
            before.iter().all(|&opacity| opacity == 0.0),
            "{mode}: {before:?}"
        );
        assert_eq!(after_glyphs.len(), after.len(), "{mode}");
        assert!(
            after_glyphs.iter().all(|&opacity| opacity == 1.0),
            "{mode} {after}: {after_glyphs:?}"
        );
    }
}

#[test]
fn magic_move_morphs_keyed_members_and_fades_the_rest() {
    let mut canvas = SceneModel::new(640, 360);
    let before = magic_move_board(&mut canvas, &[("ana", 4.0), ("bo", 3.0), ("cy", 2.0)]);
    let after = magic_move_board(&mut canvas, &[("bo", 5.0), ("ana", 3.5), ("dee", 1.0)]);
    let anim = before
        .magic_move_to(
            &after,
            crate::canvas::MagicMoveKey::Name,
            crate::canvas::MagicMoveUnmatched::Fade,
            Some(0.8),
        )
        .unwrap();
    canvas.play(vec![anim]);
    let timeline = compiled_timeline(&canvas);

    // "ana" and "bo" morph; "cy" fades out and "dee" fades in.
    let morphs = labelled_clips(&timeline, "TransformMatching");
    assert_eq!(morphs, vec![(0.0, 0.8), (0.0, 0.8)]);
    let fades: Vec<_> = labelled_clips(&timeline, "MagicMove")
        .into_iter()
        .filter(|(_, duration)| *duration > 0.0)
        .collect();
    assert_eq!(fades, vec![(0.0, 0.8), (0.0, 0.8)]);
    assert!((timeline.cached_duration - 0.8).abs() < 1e-9);
}

#[test]
fn magic_move_cut_swaps_unmatched_members_without_fading() {
    let mut canvas = SceneModel::new(640, 360);
    let before = magic_move_board(&mut canvas, &[("ana", 4.0), ("cy", 2.0)]);
    let after = magic_move_board(&mut canvas, &[("ana", 3.0), ("dee", 1.0)]);
    let anim = before
        .magic_move_to(
            &after,
            crate::canvas::MagicMoveKey::Name,
            crate::canvas::MagicMoveUnmatched::Cut,
            Some(0.5),
        )
        .unwrap();
    canvas.play(vec![anim]);
    let timeline = compiled_timeline(&canvas);
    assert_eq!(labelled_clips(&timeline, "TransformMatching").len(), 1);
    assert!(
        labelled_clips(&timeline, "MagicMove")
            .iter()
            .all(|(_, duration)| *duration == 0.0)
    );
}

#[test]
fn magic_move_rejects_invalid_arguments_and_reads_callback_keys() {
    let mut canvas = SceneModel::new(640, 360);
    let before = magic_move_board(&mut canvas, &[("ana", 4.0)]);
    let after = magic_move_board(&mut canvas, &[("ana", 3.0)]);
    let key = crate::canvas::MagicMoveKey::Name;
    let fade = crate::canvas::MagicMoveUnmatched::Fade;
    assert_eq!(
        before
            .magic_move_to(&before, key, fade, Some(1.0))
            .unwrap_err(),
        crate::canvas::MagicMoveError::SameDrawable
    );
    assert_eq!(
        before
            .magic_move_to(&after, key, fade, Some(0.0))
            .unwrap_err(),
        crate::canvas::MagicMoveError::InvalidDuration
    );
    let mut other = SceneModel::new(640, 360);
    let foreign = other.circle(1.0);
    assert_eq!(
        before
            .magic_move_to(&foreign, key, fade, Some(1.0))
            .unwrap_err(),
        crate::canvas::MagicMoveError::ForeignScene
    );
    let members = before
        .magic_move_members(|member| Ok::<_, ()>(member.name().map(|name| name.to_uppercase())))
        .unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].1, "ANA");
    let failed = before.magic_move_to_by(&after, |_| Err("no key"), fade, None);
    assert!(matches!(
        failed,
        Err(crate::canvas::MagicMoveFailure::Key("no key"))
    ));
}

#[test]
fn magic_move_transition_pairs_named_drawables_across_segments() {
    use gaanim_timeline::transition::{MagicMoveKey, TransitionType};
    let mut canvas = SceneModel::new(640, 360);
    canvas.segment("before", None).unwrap();
    let source = magic_move_board(&mut canvas, &[("ana", 4.0), ("bo", 3.0), ("cy", 2.0)]);
    canvas.wait(1.0);
    canvas
        .segment(
            "after",
            Some(TransitionType::magic_move(0.6, MagicMoveKey::Name)),
        )
        .unwrap();
    let target = magic_move_board(&mut canvas, &[("bo", 5.0), ("ana", 3.5)]);
    canvas.wait(1.0);
    let timeline = compiled_timeline(&canvas);

    let connection = timeline
        .scene_connections
        .first()
        .expect("segments are linked");
    let TransitionType::Morph { duration, mappings } = &connection.transition else {
        panic!("a magic move compiles into a morph");
    };
    assert_eq!(*duration, 0.6);
    assert_eq!(mappings.len(), 2, "ana and bo pair; cy cross-fades");
    let _ = (source, target);
}
