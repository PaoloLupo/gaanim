use super::*;
use crate::clip::{AnimationSpec, ClipPayload};
use crate::snapshot::WorldSnapshot;
use gaanim_core::ObjectId;
use gaanim_core::kurbo::BezPath;
use gaanim_math::{RateFunc, SpatialTransform};
use gaanim_scene::{FillLevel, Material3D, MobjectId, PathSource};
use std::sync::Arc;

#[test]
fn gate_conditions_read_live_results_only() {
    use gaanim_animation::polls::PollResults;
    let answers = GateCondition::Answers {
        poll: "q0".into(),
        count: 3,
    };
    let mut results = PollResults::default();
    results.counts.insert("q0".into(), vec![2, 1]);
    // Previews and exports never advance a stop.
    assert!(!answers.holds(&results));
    results.live = true;
    assert!(answers.holds(&results));
    assert_eq!(answers.progress(&results), "3/3 answered");

    let share = GateCondition::AnswerShare {
        poll: "q0".into(),
        share: 0.8,
        players: true,
    };
    // Nobody playing: not met, even with answers.
    assert!(!share.holds(&results));
    results.players = 4;
    // 80% of 4 players is 3.2: four answers are needed.
    assert!(!share.holds(&results));
    results.counts.insert("q0".into(), vec![3, 1]);
    assert!(share.holds(&results));

    let time_up = GateCondition::TimeUp { poll: "q0".into() };
    assert!(!time_up.holds(&results));
    results.remaining.insert("q0".into(), 0.0);
    assert!(time_up.holds(&results));

    let crowd = GateCondition::Players { count: 10 };
    assert!(!crowd.holds(&results));
    assert!(GateCondition::Any(vec![crowd.clone(), time_up.clone()]).holds(&results));
    assert!(!GateCondition::All(vec![crowd, time_up]).holds(&results));
}

#[test]
fn the_resting_stop_is_the_paused_one_or_the_looping_one() {
    let mut timeline = Timeline::new();
    timeline.cached_duration = 4.0;
    timeline.set_segments(vec![SegmentMetadata {
        id: 1,
        name: "gates".into(),
        notes: None,
        target: None,
        start_time: 0.0,
        end_time: 4.0,
        stops: vec![
            SegmentStop {
                name: None,
                time: 1.0,
                ambient: None,
            },
            SegmentStop {
                name: None,
                time: 2.0,
                ambient: Some(1.0),
            },
        ],
    }]);
    timeline.set_stop_gates(vec![StopGate {
        time: 1.0,
        until: GateCondition::Players { count: 1 },
    }]);
    timeline.current_time = 1.0;
    timeline.is_playing = false;
    assert_eq!(timeline.resting_stop(), Some(1.0));
    assert!(timeline.stop_gate_at(1.0).is_some());
    assert!(timeline.stop_gate_at(2.0).is_none());
    timeline.is_playing = true;
    assert_eq!(timeline.resting_stop(), None);
    // Inside an ambient loop the presentation rests on its stop.
    timeline.current_time = 2.5;
    assert_eq!(timeline.resting_stop(), Some(2.0));
    timeline.is_playing = false;
    timeline.current_time = 0.5;
    assert_eq!(timeline.resting_stop(), None);
}

#[test]
fn morph_transition_shares_one_box_between_pairs() {
    use crate::transition::{MorphMapping, MorphProperty};
    use gaanim_core::glam::DVec3;
    use gaanim_math::Bounds3D;
    use gaanim_scene::LocalBounds;

    let mut timeline = Timeline::default();
    let (from, to) = (timeline.add_scene("a"), timeline.add_scene("b"));
    let mut world = World::new();
    let mut spawn = |raw: u64, scene: SceneId, x: f64, half: f64| {
        let transform = SpatialTransform {
            translation: DVec3::new(x, 0.0, 0.0),
            ..SpatialTransform::default()
        };
        let entity = world
            .spawn((
                MobjectId(ObjectId::from_raw(raw)),
                transform,
                LocalBounds(Bounds3D::new_2d(-half, -half, half, half)),
                Opacity(1.0),
                SceneMember(scene),
            ))
            .id();
        (entity, scene)
    };
    let source = spawn(1, from, -4.0, 1.0);
    let target = spawn(2, to, 4.0, 3.0);
    let other = spawn(3, from, 0.0, 1.0);
    let mappings = [MorphMapping {
        source: ObjectId::from_raw(1),
        target: ObjectId::from_raw(2),
        property: MorphProperty::All,
    }];

    apply_morph_transition(
        &mut world,
        &[source, target, other],
        &mappings,
        0.5,
        0.5,
        from,
        to,
    );

    for (entity, _) in [source, target] {
        let placed = morph_world_box(&world, entity).unwrap();
        assert!((placed.center().x - 0.0).abs() < 1e-9);
        assert!((placed.width() - 4.0).abs() < 1e-9);
        assert_eq!(world.get::<Opacity>(entity).unwrap().0, 1.0);
    }
    assert_eq!(world.get::<Opacity>(other.0).unwrap().0, 0.5);
}

fn absolute_seek_fixture() -> (World, Timeline, Entity) {
    let object_id = ObjectId::from_raw(404);
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((100.0, 0.0));
    let mut world = World::new();
    let entity = world
        .spawn((
            MobjectId(object_id),
            SpatialTransform::default(),
            Opacity(1.0),
            Path2D(Arc::new(path.clone())),
            PathSource(Arc::new(path)),
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline::default();
    let track = timeline.add_track("absolute", 0);
    timeline.add_keyframe(0.0, snapshot);
    timeline.add_clip(
        track,
        0.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: object_id,
            lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        }),
    );
    timeline.add_clip(
        track,
        0.5,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: object_id,
            lens: PropertyLensSpec::Translation {
                from: gaanim_core::glam::DVec3::ZERO,
                to: gaanim_core::glam::DVec3::new(40.0, -20.0, 0.0),
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        }),
    );
    timeline.add_clip(
        track,
        1.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: object_id,
            lens: PropertyLensSpec::Opacity {
                from: 1.0,
                to: 0.25,
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        }),
    );
    (world, timeline, entity)
}

fn translation_x(world: &World, entity: Entity) -> f64 {
    world.get::<SpatialTransform>(entity).unwrap().translation.x
}

#[test]
fn a_step_window_shows_each_step_and_keeps_the_playhead() {
    // The fixture moves x from 0 to 40 over [0.5, 1.5].
    let (mut world, mut timeline, entity) = absolute_seek_fixture();
    world.insert_resource(gaanim_animation::PlaybackState::default());
    timeline.set_step_windows(vec![StepWindow {
        start: 0.5,
        end: 1.25,
        rate: 4.0,
    }]);
    timeline.seek(&mut world, 0.7);
    assert_eq!(translation_x(&world, entity), 0.0);
    assert_eq!(timeline.current_time, 0.7);
    assert_eq!(timeline.evaluation_time(), 0.5);
    assert_eq!(
        world
            .resource::<gaanim_animation::PlaybackState>()
            .current_time,
        0.5
    );
    timeline.seek(&mut world, 0.99);
    assert!((translation_x(&world, entity) - 10.0).abs() < 1e-9);
    // The window's end shows its exact frame, and time is smooth after.
    timeline.seek(&mut world, 1.25);
    assert!((translation_x(&world, entity) - 30.0).abs() < 1e-9);
    timeline.seek(&mut world, 1.3);
    assert!((translation_x(&world, entity) - 32.0).abs() < 1e-9);
    // Seeking back into the window steps again.
    timeline.seek(&mut world, 1.2);
    assert!((translation_x(&world, entity) - 20.0).abs() < 1e-9);
}

#[test]
fn a_drawable_with_a_step_rate_steps_its_clips_and_ends_on_their_value() {
    let (mut world, mut timeline, entity) = absolute_seek_fixture();
    world.insert_resource(gaanim_animation::PlaybackState::default());
    world.entity_mut(entity).insert(gaanim_scene::StepRate(3.0));
    timeline.stepped_objects = true;
    // Steps of a third of a second: 0.7 shows 2/3, 1.45 shows 4/3.
    timeline.seek(&mut world, 0.7);
    assert!((translation_x(&world, entity) - 40.0 * (2.0 / 3.0 - 0.5)).abs() < 1e-9);
    timeline.seek(&mut world, 1.45);
    assert!((translation_x(&world, entity) - 40.0 * (4.0 / 3.0 - 0.5)).abs() < 1e-9);
    // The playhead and the world's time stay smooth for the others.
    assert_eq!(timeline.evaluation_time(), 1.45);
    timeline.seek(&mut world, 1.5);
    assert_eq!(translation_x(&world, entity), 40.0);
}

#[test]
fn stepped_playback_moves_updaters_a_whole_step_at_a_time() {
    let (mut world, mut timeline, entity) = absolute_seek_fixture();
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = seen.clone();
    world
        .entity_mut(entity)
        .insert(gaanim_animation::Updater::new(move |_, elapsed, _, _| {
            record.lock().unwrap().push(elapsed);
            true
        }));
    world.insert_resource(gaanim_animation::PlaybackState {
        is_playing: true,
        scaled_dt: 0.1,
        current_time: 0.0,
        updaters_current: false,
    });
    timeline.set_step_windows(vec![StepWindow {
        start: 0.0,
        end: 2.0,
        rate: 4.0,
    }]);
    timeline.seek(&mut world, 0.0);
    seen.lock().unwrap().clear();
    for frame in 1..=6 {
        timeline.seek(&mut world, frame as f64 * 0.1);
        assert!(
            world
                .resource::<gaanim_animation::PlaybackState>()
                .updaters_current
        );
        // Playback leaves the updaters where the seek put them.
        gaanim_animation::updater_system(&mut world);
    }
    assert_eq!(*seen.lock().unwrap(), [0.0, 0.0, 0.25, 0.25, 0.5, 0.5]);
}

/// The absolute seek fixture with a visible source, as a scene shows it.
fn visible_echo_fixture() -> (World, Timeline, Entity) {
    let (mut world, _, entity) = absolute_seek_fixture();
    world.entity_mut(entity).insert(gaanim_scene::Visible);
    let (_, mut timeline, _) = absolute_seek_fixture();
    timeline.keyframes.clear();
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    (world, timeline, entity)
}

#[test]
fn echo_ghosts_show_their_source_at_delayed_times() {
    let source_id = ObjectId::from_raw(404);
    let (mut world, mut timeline, source) = visible_echo_fixture();
    let ghost = |world: &mut World, raw: u64, lag: f64, opacity: f32, rank: u32| {
        world
            .spawn((
                MobjectId(ObjectId::from_raw(raw)),
                gaanim_animation::EchoGhost {
                    source: source_id,
                    lag,
                    opacity,
                    parent: None,
                    rank,
                    hold: false,
                    motion_sources: Arc::default(),
                    window: None,
                },
            ))
            .id()
    };
    let near = ghost(&mut world, 900, 0.25, 0.5, 1);
    let far = ghost(&mut world, 901, 1.0, 0.25, 2);

    for target in [1.2, 0.3, 1.9, 0.0, 1.2, 1.75] {
        timeline.seek(&mut world, target);
        for (copy, lag, factor) in [(near, 0.25, 0.5), (far, 1.0, 0.25)] {
            let visible = world.get::<gaanim_scene::Visible>(copy).is_some();
            if target - lag < 0.0 {
                assert!(!visible, "{target} - {lag} is before the timeline");
                continue;
            }
            let (mut reference_world, mut reference, reference_entity) = visible_echo_fixture();
            reference.seek(&mut reference_world, target - lag);
            assert!(visible, "copy {lag} at {target}");
            assert_eq!(
                world.get::<SpatialTransform>(copy),
                reference_world.get::<SpatialTransform>(reference_entity),
                "transform of copy {lag} at {target}"
            );
            assert_eq!(
                world.get::<Path2D>(copy).map(|path| path.0.clone()),
                reference_world
                    .get::<Path2D>(reference_entity)
                    .map(|path| path.0.clone()),
                "path of copy {lag} at {target}"
            );
            let expected = reference_world.get::<Opacity>(reference_entity).unwrap().0 * factor;
            assert!((world.get::<Opacity>(copy).unwrap().0 - expected).abs() < 1e-6);
        }
    }

    // At rest, a copy that caught up with its source would only stack
    // on it: it is hidden, and shown again once the source moves.
    timeline.cached_duration = 4.0;
    timeline.seek(&mut world, 3.5);
    assert!(world.get::<gaanim_scene::Visible>(near).is_none());
    assert!(world.get::<gaanim_scene::Visible>(far).is_none());
    timeline.seek(&mut world, 1.2);
    assert!(world.get::<gaanim_scene::Visible>(near).is_some());

    // Copies are not keyframed and disappear with their source.
    let snapshot = WorldSnapshot::capture(&mut world);
    assert!(!snapshot.entities.contains_key(&ObjectId::from_raw(900)));
    world.entity_mut(source).remove::<gaanim_scene::Visible>();
    let mut hidden = timeline.keyframes[&OrderedFloat(0.0)].clone();
    hidden.entities.get_mut(&source_id).unwrap().visible = false;
    timeline.add_keyframe(0.0, hidden);
    timeline.seek(&mut world, 1.5);
    assert!(world.get::<gaanim_scene::Visible>(near).is_none());
    assert!(world.get::<gaanim_scene::Visible>(far).is_none());
}

#[test]
fn absolute_replay_matches_forced_restore_for_random_access() {
    let (mut optimized_world, mut optimized, optimized_entity) = absolute_seek_fixture();
    let (mut reference_world, mut reference, reference_entity) = absolute_seek_fixture();

    for target in [2.0, 0.25, 1.75, 0.0, 1.2] {
        optimized.seek(&mut optimized_world, target);
        reference.last_restore_kf_time = None;
        reference.seek(&mut reference_world, target);

        assert_eq!(
            optimized_world.get::<SpatialTransform>(optimized_entity),
            reference_world.get::<SpatialTransform>(reference_entity)
        );
        assert_eq!(
            optimized_world.get::<Opacity>(optimized_entity),
            reference_world.get::<Opacity>(reference_entity)
        );
        assert_eq!(
            optimized_world.get::<Path2D>(optimized_entity),
            reference_world.get::<Path2D>(reference_entity)
        );
    }
}

#[test]
fn dynamic_camera_clip_disables_restore_skipping() {
    let (mut world, mut timeline, _) = absolute_seek_fixture();
    timeline.seek(&mut world, 0.25);
    let track = timeline.tracks.keys().next().unwrap();
    timeline.add_clip(
        track,
        0.5,
        0.5,
        ClipPayload::Animation(AnimationSpec {
            target: ObjectId::from_raw(404),
            lens: PropertyLensSpec::CameraPosition {
                from: gaanim_core::glam::DVec3::ZERO,
                to: gaanim_core::glam::DVec3::X,
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        }),
    );

    assert!(!timeline.can_replay_without_restore(&mut world, OrderedFloat(0.0), 0.75));
}

#[test]
fn captured_camera_state_restores_deterministically_across_seeks() {
    let mut world = World::new();
    let camera_id = ObjectId::from_parts(0, 1);
    world.spawn((MobjectId(camera_id), SpatialTransform::default()));
    world.insert_resource(gaanim_math::Camera::ortho_2d(960, 540));
    world.insert_resource(crate::snapshot::CapturedCameraStates::default());

    let mut timeline = Timeline::new();
    let track = timeline.add_track("Camera", 0);
    timeline.add_clip(
        track,
        0.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: camera_id,
            lens: PropertyLensSpec::CameraPosition {
                from: gaanim_core::glam::DVec3::ZERO,
                to: gaanim_core::glam::DVec3::new(120.0, -30.0, 0.0),
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: Some("Camera".into()),
        }),
    );
    timeline.add_clip(track, 1.0, 0.0, ClipPayload::CameraCapture { id: 10 });
    timeline.add_clip(track, 1.0, 0.0, ClipPayload::CameraCapture { id: 11 });
    timeline.add_clip(
        track,
        1.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: camera_id,
            lens: PropertyLensSpec::CameraPosition {
                from: gaanim_core::glam::DVec3::new(120.0, -30.0, 0.0),
                to: gaanim_core::glam::DVec3::ZERO,
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: Some("Camera".into()),
        }),
    );
    timeline.add_clip(track, 2.0, 0.0, ClipPayload::CameraCapture { id: 12 });
    timeline.add_clip(
        track,
        2.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: camera_id,
            lens: PropertyLensSpec::CameraState {
                from: gaanim_animation::CameraStateSource::Captured(12),
                to: gaanim_animation::CameraStateSource::Captured(10),
                orbit: false,
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: Some("Camera".into()),
        }),
    );
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));

    timeline.seek(&mut world, 3.0);
    let expected = world.resource::<gaanim_math::Camera>().pose();
    assert_eq!(
        expected.position,
        gaanim_core::glam::DVec3::new(120.0, -30.0, 0.0)
    );

    timeline.seek(&mut world, 0.25);
    timeline.seek(&mut world, 3.0);
    assert_eq!(world.resource::<gaanim_math::Camera>().pose(), expected);

    let snapshot = WorldSnapshot::capture(&mut world);
    world
        .resource_mut::<crate::snapshot::CapturedCameraStates>()
        .0
        .clear();
    snapshot.restore(&mut world);
    assert!(
        world
            .resource::<crate::snapshot::CapturedCameraStates>()
            .0
            .contains_key(&10)
    );
}

#[test]
fn zoom_through_shows_one_segment_per_half_and_zooms_about_its_center() {
    use crate::scene::SceneMember;
    use gaanim_core::glam::{DVec2, DVec3};
    use gaanim_scene::Opacity;

    let mut world = World::new();
    world.insert_resource(gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900));
    let mut timeline = Timeline::default();
    let track = timeline.add_track("main", 0);
    let first = timeline.add_scene("first");
    let second = timeline.add_scene("second");
    timeline.index_scene(first, 0.0);
    timeline.index_scene(second, 1.0);
    timeline.add_clip(track, 0.0, 0.0, ClipPayload::SceneStart(first));
    timeline.add_clip(track, 1.0, 0.0, ClipPayload::SceneEnd(first));
    timeline.add_clip(track, 1.0, 0.0, ClipPayload::SceneStart(second));
    timeline.add_clip(track, 3.0, 0.0, ClipPayload::SceneEnd(second));
    let open = timeline.add_scene("open");
    timeline.add_clip(track, 3.0, 0.0, ClipPayload::SceneStart(open));
    let bounds = timeline.scene_bounds_all();
    assert_eq!(bounds.len(), 2);
    for scene in [first, second, open] {
        assert_eq!(bounds.get(&scene).copied(), timeline.scene_bounds(scene));
    }
    timeline.connect(
        first,
        second,
        crate::transition::TransitionType::ZoomThrough {
            duration: 1.0,
            center: DVec2::new(2.0, 1.0),
            max_zoom: 4.0,
        },
    );
    let mut spawn = |raw, scene| {
        world
            .spawn((
                MobjectId(ObjectId::from_raw(raw)),
                SpatialTransform::default(),
                Opacity(1.0),
                SceneMember(scene),
            ))
            .id()
    };
    let outgoing = spawn(1, first);
    let incoming = spawn(2, second);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    let opacities = |world: &World| {
        (
            world.get::<Opacity>(outgoing).unwrap().0,
            world.get::<Opacity>(incoming).unwrap().0,
        )
    };
    let camera = |world: &World| {
        let camera = world.resource::<gaanim_math::Camera>();
        let gaanim_math::Projection::Orthographic { zoom } = camera.projection else {
            panic!("orthographic camera")
        };
        (camera.position, zoom)
    };

    timeline.seek(&mut world, 1.25);
    assert_eq!(opacities(&world), (1.0, 0.0));
    // Halfway into the zoom: 2.5×, with (2, 1) fixed on screen.
    let (position, zoom) = camera(&world);
    assert!((zoom - 2.5).abs() < 1e-9);
    assert!((position - DVec3::new(1.2, 0.6, 0.0)).length() < 1e-9);

    timeline.seek(&mut world, 1.75);
    assert_eq!(opacities(&world), (0.0, 1.0));
    assert!((camera(&world).1 - 2.5).abs() < 1e-9);

    // Repeated seeks are exact, and the camera is back once it ends.
    timeline.seek(&mut world, 1.25);
    assert!((camera(&world).1 - 2.5).abs() < 1e-9);
    timeline.seek(&mut world, 2.5);
    assert_eq!(camera(&world), (DVec3::ZERO, 1.0));
    assert_eq!(opacities(&world).1, 1.0);
}

#[test]
fn surrounding_rect_retarget_seek_is_exact_and_reversible() {
    let mut world = World::new();
    let frame_id = ObjectId::from_parts(1, 1);
    let from = ObjectId::from_parts(2, 1);
    let to = ObjectId::from_parts(3, 1);
    let frame_entity = world
        .spawn((
            MobjectId(frame_id),
            SpatialTransform::default(),
            gaanim_animation::SurroundingRect::new(vec![from], [12.0; 4], 8.0),
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline::default();
    let track = timeline.add_track("SurroundingRect", 0);
    timeline.add_keyframe(0.0, snapshot);
    timeline.add_clip(
        track,
        1.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: frame_id,
            lens: PropertyLensSpec::SurroundingRectTargets {
                from: vec![from],
                to: vec![to],
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        }),
    );

    timeline.seek(&mut world, 1.5);
    let active = world
        .get::<gaanim_animation::SurroundingRect>(frame_entity)
        .unwrap();
    assert_eq!(active.from, vec![from]);
    assert_eq!(active.to, vec![to]);
    assert_eq!(active.progress, 0.5);

    timeline.seek(&mut world, 0.5);
    let restored = world
        .get::<gaanim_animation::SurroundingRect>(frame_entity)
        .unwrap();
    assert_eq!(restored.from, vec![from]);
    assert_eq!(restored.to, vec![from]);
    assert_eq!(restored.progress, 1.0);
}

#[test]
fn media_frame_seek_restores_crop_and_immediate_cuts() {
    use gaanim_scene::{ImageFit, MediaFrame};
    let mut world = World::new();
    let object_id = ObjectId::from_raw(0);
    let from = MediaFrame {
        crop: gaanim_core::kurbo::Rect::new(0.0, 0.0, 400.0, 200.0),
        width: 8.0,
        height: 4.5,
        fit: ImageFit::Cover,
        quality: gaanim_core::peniko::ImageQuality::Medium,
        alignment: 0.5,
    };
    let to = MediaFrame {
        crop: gaanim_core::kurbo::Rect::new(100.0, 50.0, 300.0, 150.0),
        ..from
    };
    let cut = MediaFrame {
        width: 4.0,
        quality: gaanim_core::peniko::ImageQuality::High,
        ..to
    };
    let entity = world
        .spawn((MobjectId(object_id), SpatialTransform::default(), from))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline::default();
    let track = timeline.add_track("MediaFrame", 0);
    timeline.add_keyframe(0.0, snapshot);
    for (start, duration, a, b) in [(1.0, 2.0, from, to), (4.0, 0.0, to, cut)] {
        timeline.add_clip(
            track,
            start,
            duration,
            ClipPayload::Animation(AnimationSpec {
                target: object_id,
                lens: PropertyLensSpec::MediaFrame { from: a, to: b },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
    }
    for (time, expected) in [
        (5.0, cut),
        (0.0, from),
        (2.0, from.interpolate(to, 0.5)),
        (3.5, to),
        (4.0, cut),
        (0.5, from),
    ] {
        timeline.seek(&mut world, time);
        assert_eq!(
            *world.get::<MediaFrame>(entity).unwrap(),
            expected,
            "time {time}"
        );
    }
}

/// Playback seeks forward from the previous frame and restores what
/// other systems wrote since; an image's clip path, which follows its
/// media frame, keeps the crop the replay leaves after the crop ends,
/// as a seek to the same instant does.
#[test]
fn forward_replay_keeps_an_image_path_on_its_finished_crop() {
    use gaanim_scene::{ImageFit, MediaFrame, RasterImage};
    let from = MediaFrame {
        crop: gaanim_core::kurbo::Rect::new(0.0, 0.0, 1654.0, 2339.0),
        width: 7.8,
        height: 3.8,
        fit: ImageFit::Contain,
        quality: gaanim_core::peniko::ImageQuality::Medium,
        alignment: 0.5,
    };
    let to = MediaFrame {
        crop: gaanim_core::kurbo::Rect::new(132.0, 164.0, 1522.0, 866.0),
        ..from
    };
    let fixture = || {
        let mut world = World::new();
        let empty = Arc::new(BezPath::new());
        let image = world
            .spawn((
                MobjectId(ObjectId::from_raw(1)),
                SpatialTransform::default(),
                Opacity(1.0),
                from,
                RasterImage::none(),
                Path2D(Arc::clone(&empty)),
                PathSource(empty),
                gaanim_scene::LocalBounds::default(),
            ))
            .id();
        // A reactive signal disables the whole-world replay shortcut, so
        // every seek goes through the keyframe restore.
        world.spawn((
            MobjectId(ObjectId::from_raw(2)),
            SpatialTransform::default(),
            Opacity(1.0),
            gaanim_animation::FloatSignal::new(0.0),
        ));
        let mut frames = bevy::ecs::schedule::Schedule::default();
        frames.add_systems(gaanim_scene::media_frame::update_media_frames);
        frames.run(&mut world);
        let mut timeline = Timeline::default();
        let track = timeline.add_track("crop", 0);
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.add_clip(
            track,
            0.0,
            0.6,
            ClipPayload::Animation(AnimationSpec {
                target: ObjectId::from_raw(1),
                lens: PropertyLensSpec::MediaFrame { from, to },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
        (world, timeline, frames, image)
    };
    let (mut world, mut timeline, mut frames, image) = fixture();
    let (mut reference_world, mut reference, mut reference_frames, reference_image) = fixture();
    for step in 0..=36 {
        let time = f64::from(step) / 30.0;
        timeline.seek(&mut world, time);
        frames.run(&mut world);
        reference.replay_baseline = None;
        reference.seek(&mut reference_world, time);
        reference_frames.run(&mut reference_world);
        let (clip, transform) = world.get::<MediaFrame>(image).unwrap().geometry();
        assert_eq!(
            world.get::<Path2D>(image).unwrap().0.elements(),
            gaanim_core::kurbo::Shape::to_path(&clip, 0.1).elements(),
            "path at t={time}"
        );
        assert_eq!(
            world.get::<RasterImage>(image).unwrap().local_transform,
            transform,
            "image at t={time}"
        );
        assert_eq!(
            world.get::<Path2D>(image),
            reference_world.get::<Path2D>(reference_image),
            "path against a seek at t={time}"
        );
    }
}

#[test]
fn material_3d_seek_is_exact_and_reversible() {
    let mut world = World::new();
    let object_id = ObjectId::from_raw(0);
    let from = Material3D::new(gaanim_core::peniko::Color::BLACK, 0.8, 0.1, None, 0.0).unwrap();
    let to = Material3D::new(
        gaanim_core::peniko::Color::WHITE,
        0.2,
        0.9,
        Some(gaanim_core::peniko::Color::WHITE),
        4.0,
    )
    .unwrap();
    let entity = world
        .spawn((MobjectId(object_id), SpatialTransform::default(), from))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline::default();
    let track = timeline.add_track("Material", 0);
    timeline.add_keyframe(0.0, snapshot);
    timeline.add_clip(
        track,
        1.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: object_id,
            lens: PropertyLensSpec::Material3D { from, to },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: Some("Material".into()),
        }),
    );

    timeline.seek(&mut world, 2.0);
    assert_eq!(*world.get::<Material3D>(entity).unwrap(), to);
    timeline.seek(&mut world, 0.5);
    assert_eq!(*world.get::<Material3D>(entity).unwrap(), from);
    timeline.seek(&mut world, 1.5);
    let middle = *world.get::<Material3D>(entity).unwrap();
    assert!((middle.roughness - 0.5).abs() < 1e-6);
    assert!((middle.metallic - 0.5).abs() < 1e-6);
    assert!((middle.emissive_strength - 2.0).abs() < 1e-6);
}

#[test]
fn paint_seek_is_exact_and_reversible_for_fill_and_stroke() {
    use gaanim_core::peniko::{Brush, Color, Gradient};
    let mut world = World::new();
    let id = ObjectId::from_raw(707);
    let from = Brush::Solid(Color::BLACK);
    let to = Brush::Gradient(
        Gradient::new_linear((0., 0.), (20., 0.)).with_stops([Color::WHITE, Color::BLACK]),
    );
    let stroke = StrokeBrush {
        brush: Some(from.clone()),
        ..StrokeBrush::default()
    };
    let entity = world
        .spawn((
            MobjectId(id),
            SpatialTransform::default(),
            FillBrush(Some(from.clone())),
            stroke,
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline::default();
    let track = timeline.add_track("Paint", 0);
    timeline.add_keyframe(0., snapshot);
    for lens in [
        PropertyLensSpec::FillPaint {
            from: from.clone(),
            to: to.clone(),
        },
        PropertyLensSpec::StrokePaint {
            from: from.clone(),
            to: to.clone(),
        },
    ] {
        timeline.add_clip(
            track,
            1.,
            1.,
            ClipPayload::Animation(AnimationSpec {
                target: id,
                lens,
                rate_func: RateFunc::Linear,
                delay: 0.,
                label: None,
            }),
        );
    }
    for time in [2., 0.5, 1.5, 2., 1.5] {
        timeline.seek(&mut world, time);
        let expected = gaanim_animation::paint::interpolate_paint(&from, &to, time - 1.);
        assert_eq!(
            world.get::<FillBrush>(entity).unwrap().0,
            Some(expected.clone())
        );
        assert_eq!(
            world.get::<StrokeBrush>(entity).unwrap().brush,
            Some(expected)
        );
    }
}

#[test]
fn paint_cut_can_replace_gradient_kind_and_rewind() {
    use gaanim_core::peniko::{Brush, Color, Gradient};
    let mut world = World::new();
    let id = ObjectId::from_raw(708);
    let from = Brush::Gradient(
        Gradient::new_linear((0., 0.), (20., 0.)).with_stops([Color::WHITE, Color::BLACK]),
    );
    let to = Brush::Gradient(
        Gradient::new_radial((0., 0.), 20.).with_stops([Color::WHITE, Color::BLACK]),
    );
    let entity = world
        .spawn((
            MobjectId(id),
            SpatialTransform::default(),
            FillBrush(Some(from.clone())),
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline::default();
    let track = timeline.add_track("Paint cut", 0);
    timeline.add_keyframe(0., snapshot);
    timeline.add_clip(
        track,
        1.,
        0.,
        ClipPayload::Animation(AnimationSpec {
            target: id,
            lens: PropertyLensSpec::FillPaint {
                from: from.clone(),
                to: to.clone(),
            },
            rate_func: RateFunc::Linear,
            delay: 0.,
            label: None,
        }),
    );
    timeline.seek(&mut world, 1.);
    assert_eq!(world.get::<FillBrush>(entity).unwrap().0, Some(to));
    timeline.seek(&mut world, 0.5);
    assert_eq!(world.get::<FillBrush>(entity).unwrap().0, Some(from));
}

#[test]
fn future_fill_level_clip_resets_to_its_initial_value_without_a_snapshot() {
    let mut world = World::new();
    let object_id = ObjectId::from_raw(91);
    let entity = world
        .spawn((
            MobjectId(object_id),
            SpatialTransform::default(),
            FillLevel(0.75),
        ))
        .id();
    let mut timeline = Timeline::default();
    let track = timeline.add_track("FillLevel", 0);
    timeline.add_clip(
        track,
        1.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: object_id,
            lens: PropertyLensSpec::FillLevel {
                from: 0.0,
                to: 0.75,
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: Some("FillLevel".into()),
        }),
    );

    timeline.seek(&mut world, 0.5);

    assert_eq!(world.get::<FillLevel>(entity).unwrap().0, 0.0);
}

#[test]
fn zero_path_completion_restores_an_empty_path() {
    let mut source = BezPath::new();
    source.move_to((0.0, 0.0));
    source.line_to((100.0, 0.0));
    let object_id = ObjectId::from_raw(77);
    let mut world = World::new();
    let entity = world
        .spawn((
            MobjectId(object_id),
            Path2D(Arc::new(source.clone())),
            PathSource(Arc::new(source)),
            SpatialTransform::default(),
        ))
        .id();
    let line_object_id = ObjectId::from_raw(78);
    let line_entity = world
        .spawn((
            MobjectId(line_object_id),
            LineListData {
                points: vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]],
                indices: None,
                strip: true,
                color: gaanim_core::peniko::Color::WHITE,
                colors: None,
                revealed: None,
            },
            SpatialTransform::default(),
            gaanim_scene::Visibility::Inherited,
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline::default();
    let track = timeline.add_track("Path", 0);
    timeline.add_keyframe(0.0, snapshot);
    timeline.add_clip(
        track,
        0.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: object_id,
            lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: Some("PathCompletion".into()),
        }),
    );
    timeline.add_clip(
        track,
        1.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: line_object_id,
            lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: Some("LineListCompletion".into()),
        }),
    );

    timeline.seek(&mut world, 0.0);
    assert!(world.get::<Path2D>(entity).unwrap().0.elements().is_empty());
    assert!(
        world
            .get::<LineListData>(line_entity)
            .unwrap()
            .points
            .is_empty()
    );
    assert_eq!(
        *world.get::<gaanim_scene::Visibility>(line_entity).unwrap(),
        gaanim_scene::Visibility::Hidden
    );

    timeline.seek(&mut world, 1.5);
    assert_eq!(
        world.get::<LineListData>(line_entity).unwrap().points.len(),
        2
    );
    assert_eq!(
        *world.get::<gaanim_scene::Visibility>(line_entity).unwrap(),
        gaanim_scene::Visibility::Inherited
    );
}

#[test]
fn fixed_step_simulation_moves_during_explicit_export_seeks_and_rewinds() {
    let mut world = World::new();
    world.insert_resource(gaanim_animation::PlaybackState {
        is_playing: false,
        scaled_dt: 0.0,
        current_time: 0.0,
        updaters_current: false,
    });
    let object_id = ObjectId::from_raw(0);
    let updater = gaanim_animation::Updater::new_simulation(
        |dt, _elapsed, entity, world| {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                transform.translation.x += dt;
            }
            true
        },
        |_entity, _world| true,
        1.0 / 240.0,
    )
    .unwrap();
    let mut initial_transform = SpatialTransform::default();
    initial_transform.translation.x = 3.0;
    let entity = world
        .spawn((MobjectId(object_id), initial_transform, updater))
        .id();

    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline {
        cached_duration: 1.0,
        ..Timeline::default()
    };
    timeline.add_keyframe(0.0, snapshot);

    timeline.seek(&mut world, 0.0);
    timeline.seek(&mut world, 0.1);
    let first = world.get::<SpatialTransform>(entity).unwrap().translation.x;
    assert!((first - 3.1).abs() < 1e-12);

    timeline.seek(&mut world, 0.2);
    let second = world.get::<SpatialTransform>(entity).unwrap().translation.x;
    assert!((second - 3.2).abs() < 1e-12);

    timeline.seek(&mut world, 0.1);
    let rewound = world.get::<SpatialTransform>(entity).unwrap().translation.x;
    assert!((rewound - first).abs() < 1e-12);
}

#[test]
fn traced_path_seek_samples_followed_sources_like_playback() {
    let mut world = World::new();
    world.insert_resource(gaanim_animation::PlaybackState::default());

    // The leader moves along x through a sampled series; the mass only
    // reaches it through `follow`, so its authored pose (0, 5) is stale.
    let series = gaanim_animation::SampledSeriesDriver::new(
        vec![0.0, 4.0],
        vec![0.0, 4.0],
        gaanim_animation::SampledProperty::TranslateX,
        gaanim_animation::SampledInterpolation::Linear,
        1.0,
        0.0,
    )
    .unwrap();
    let leader = world
        .spawn((
            MobjectId(ObjectId::from_raw(0)),
            SpatialTransform::default(),
            gaanim_animation::SampledSeriesDrivers::from(series),
        ))
        .id();
    let mass = world
        .spawn((
            MobjectId(ObjectId::from_raw(1)),
            SpatialTransform::new_2d(0.0, 5.0),
            gaanim_animation::EndpointFollow {
                endpoint: gaanim_animation::TrackingEndpoint::Entity(leader),
                offset: gaanim_core::glam::DVec3::ZERO,
                offset_space: gaanim_animation::FollowOffsetSpace::World,
                since: 0.0,
            },
        ))
        .id();
    let trace = world
        .spawn((
            MobjectId(ObjectId::from_raw(2)),
            SpatialTransform::default(),
            Path2D(Arc::new(BezPath::new())),
            gaanim_animation::TracedPath::new(mass, 0.01, None),
        ))
        .id();

    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline {
        cached_duration: 4.0,
        ..Timeline::default()
    };
    timeline.add_keyframe(0.0, snapshot);

    timeline.seek(&mut world, 2.0);
    let points = world
        .get::<gaanim_animation::TracedPath>(trace)
        .unwrap()
        .points
        .clone();
    assert!(points.len() > 2);
    assert!(
        points.iter().all(|point| point.y.abs() < 1e-9),
        "no sample may use the unresolved authored pose: {points:?}"
    );
    let last = points.last().unwrap();
    assert!(
        (last.x - 2.0).abs() < 1e-9,
        "trail ends at the seek target: {last:?}"
    );
}

#[test]
fn traced_path_seek_samples_sources_moved_by_clips() {
    let mut world = World::new();
    world.insert_resource(gaanim_animation::PlaybackState::default());
    let source_id = ObjectId::from_raw(0);
    let source = world
        .spawn((MobjectId(source_id), SpatialTransform::default()))
        .id();
    let trace = world
        .spawn((
            MobjectId(ObjectId::from_raw(1)),
            SpatialTransform::default(),
            Path2D(Arc::new(BezPath::new())),
            gaanim_animation::TracedPath::new(source, 0.01, None),
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline::default();
    let track = timeline.add_track("move", 0);
    timeline.add_keyframe(0.0, snapshot);
    timeline.add_clip(
        track,
        0.0,
        2.0,
        ClipPayload::Animation(AnimationSpec {
            target: source_id,
            lens: PropertyLensSpec::Translation {
                from: gaanim_core::glam::DVec3::ZERO,
                to: gaanim_core::glam::DVec3::new(4.0, 2.0, 0.0),
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        }),
    );

    // Seeking straight to 1.5, like `gaanim export --from 1.5`, traces
    // the clip's motion up to there instead of one point.
    timeline.seek(&mut world, 1.5);
    let points = world
        .get::<gaanim_animation::TracedPath>(trace)
        .unwrap()
        .points
        .clone();
    assert!(points.len() > 10, "{} points", points.len());
    assert!(points[0].distance(gaanim_core::glam::DVec3::ZERO) < 1e-9);
    let last = *points.last().unwrap();
    assert!(last.distance(gaanim_core::glam::DVec3::new(3.0, 1.5, 0.0)) < 1e-6);
    let source_at = world.get::<SpatialTransform>(source).unwrap().translation;
    assert!(source_at.distance(gaanim_core::glam::DVec3::new(3.0, 1.5, 0.0)) < 1e-6);

    // A short seek back, as motion blur samples before a frame, keeps
    // the trace and drops only its newer points.
    timeline.seek(&mut world, 1.45);
    let trace_state = world.get::<gaanim_animation::TracedPath>(trace).unwrap();
    assert!(trace_state.points.len() > 10);
    assert!(
        trace_state
            .sample_times
            .iter()
            .all(|time| *time <= 1.45 + 1e-9)
    );
}

/// Minimum distance, maximum points, start and dissipating time.
type TraceParams = (f64, Option<usize>, f64, Option<f64>);

/// What property bindings a [`settling_trace_world`] has.
#[derive(Clone, Copy, PartialEq)]
enum TraceBindings {
    None,
    /// Bindings with windows on another drawable: one follows time, one a
    /// signal animated after the source stops.
    Elsewhere,
    /// A binding that moves the trace's source.
    OnSource,
}

/// Where a [`settling_trace_world`] has an updater.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Reactive {
    None,
    /// On another entity, which it alone moves.
    Elsewhere,
    /// On the source, so rebuilds replay every sample.
    OnSource,
}

/// A trace of a source that clips move for its first two seconds, and
/// its group from 6 to 7 s, with `checkpoints` at segment starts every
/// 2 s up to 40 s, and a `reactive` updater. The drawable the bindings
/// target, if any, is the last entity returned.
fn settling_trace_world(
    reactive: Reactive,
    checkpoints: bool,
    (min_distance, max_points, start_at, dissipating_time): TraceParams,
    bindings: TraceBindings,
) -> (World, Timeline, Entity, Entity) {
    let mut world = World::new();
    world.insert_resource(gaanim_animation::PlaybackState::default());
    let source_id = ObjectId::from_raw(0);
    let group_id = ObjectId::from_raw(5);
    let group = world
        .spawn((MobjectId(group_id), SpatialTransform::default()))
        .id();
    let source = world
        .spawn((
            MobjectId(source_id),
            SpatialTransform::default(),
            ChildOf(group),
        ))
        .id();
    let trace = world
        .spawn((
            MobjectId(ObjectId::from_raw(1)),
            SpatialTransform::default(),
            Path2D(Arc::new(BezPath::new())),
            gaanim_animation::TracedPath::new(source, min_distance, max_points)
                .starting_at(start_at)
                .with_dissipating_time(dissipating_time),
        ))
        .id();
    match reactive {
        Reactive::None => {}
        Reactive::Elsewhere => {
            world.spawn((
                MobjectId(ObjectId::from_raw(2)),
                SpatialTransform::default(),
                gaanim_animation::Updater::new(|_, _, _, _| true),
            ));
        }
        Reactive::OnSource => {
            world
                .entity_mut(source)
                .insert(gaanim_animation::Updater::new(|_, _, _, _| true));
        }
    }
    let bound = match bindings {
        TraceBindings::OnSource => source,
        _ => world
            .spawn((
                MobjectId(ObjectId::from_raw(3)),
                SpatialTransform::default(),
            ))
            .id(),
    };
    let signal_id = ObjectId::from_raw(4);
    if bindings != TraceBindings::None {
        use gaanim_animation::{
            PropertyBinding, PropertyParameter, PropertySources, PropertyValue,
            ResolvedPropertySources, ScalarSource,
        };
        let signal = world
            .spawn((
                MobjectId(signal_id),
                SpatialTransform::default(),
                gaanim_animation::FloatSignal::new(0.0),
            ))
            .id();
        let binding = |sources, fallback, start, end| PropertyBinding {
            target: bound,
            source: ResolvedPropertySources {
                sources,
                parameters: vec![PropertyParameter {
                    logical: signal_id,
                    native: signal_id,
                    entity: signal,
                    initial: 0.0,
                }],
                anchor_offset: gaanim_core::glam::DVec3::ZERO,
                local_anchor: None,
            },
            start,
            end,
            fallback,
        };
        let constant = ScalarSource::Constant;
        world.spawn(binding(
            PropertySources::Translation {
                values: [ScalarSource::Time, constant(1.0), constant(0.0)],
                anchor: None,
            },
            PropertyValue::Translation(gaanim_core::glam::DVec3::ZERO),
            5.0,
            Some(15.0),
        ));
        // Its own channel, so its last write survives the other binding.
        world.spawn(binding(
            PropertySources::Scale([
                ScalarSource::Signal(signal_id),
                constant(1.0),
                constant(1.0),
            ]),
            PropertyValue::Scale(gaanim_core::glam::DVec3::ONE),
            10.0,
            Some(12.5),
        ));
    }
    let mut timeline = Timeline {
        segment_checkpoints: checkpoints,
        ..Timeline::default()
    };
    let track = timeline.add_track("move", 0);
    for (start, to) in [(0.0, (4.0, 2.0)), (1.2, (-1.0, 3.0))] {
        timeline.add_clip(
            track,
            start,
            0.8,
            ClipPayload::Animation(AnimationSpec {
                target: source_id,
                lens: PropertyLensSpec::Translation {
                    from: gaanim_core::glam::DVec3::ZERO,
                    to: gaanim_core::glam::DVec3::new(to.0, to.1, 0.0),
                },
                rate_func: RateFunc::Smooth,
                delay: 0.0,
                label: None,
            }),
        );
    }
    timeline.add_clip(
        track,
        6.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: group_id,
            lens: PropertyLensSpec::Translation {
                from: gaanim_core::glam::DVec3::ZERO,
                to: gaanim_core::glam::DVec3::new(0.0, -2.0, 0.0),
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        }),
    );
    if bindings != TraceBindings::None {
        timeline.add_clip(
            track,
            8.0,
            12.0,
            ClipPayload::Animation(AnimationSpec {
                target: signal_id,
                lens: PropertyLensSpec::SignalFloat { from: 0.0, to: 6.0 },
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
    }
    // A camera clip whose target id is the source's moves no drawable.
    timeline.add_clip(
        track,
        20.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: source_id,
            lens: PropertyLensSpec::CameraZoom {
                from: 1.0,
                to: 2.0,
                interpolation: gaanim_math::ZoomInterpolation::default(),
            },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        }),
    );
    timeline.add_clip(track, 40.0, 0.0, ClipPayload::Wait);
    let segments = (0..20)
        .map(|index| SegmentMetadata {
            id: index + 1,
            name: format!("segment {index}"),
            notes: None,
            target: None,
            start_time: f64::from(index) * 2.0,
            end_time: f64::from(index + 1) * 2.0,
            stops: Vec::new(),
        })
        .collect();
    timeline.set_segments(segments);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    (world, timeline, trace, bound)
}

#[test]
fn settled_traces_rebuild_like_sampling_every_frame() {
    let traces: [TraceParams; 4] = [
        (0.01, None, 0.0, Some(0.3)),
        (0.0, None, 0.0, Some(1.0)),
        (0.05, Some(40), 0.0, None),
        (0.01, None, 1.0, None),
    ];
    let cases = traces
        .into_iter()
        .map(|trace| (trace, TraceBindings::None, Reactive::None))
        .chain([
            (traces[0], TraceBindings::Elsewhere, Reactive::None),
            (traces[0], TraceBindings::OnSource, Reactive::None),
            // An updater that moves something else leaves the trace as
            // clips move it.
            (traces[0], TraceBindings::None, Reactive::Elsewhere),
            (traces[0], TraceBindings::Elsewhere, Reactive::Elsewhere),
        ]);
    for (trace, bindings, reactive) in cases {
        let (mut world, mut timeline, entity, bound) =
            settling_trace_world(reactive, false, trace, bindings);
        let (mut reference_world, mut reference, reference_entity, reference_bound) =
            settling_trace_world(Reactive::OnSource, false, trace, bindings);
        // Far apart, so every seek rebuilds the trace.
        for time in [
            30.0, 2.0, 1.5, 0.5, 12.345, 39.99, 2.6, 11.0, 14.99, 0.3, 15.01,
        ] {
            HELD_TRACE_SAMPLES.with(|held| held.set(0));
            timeline.seek(&mut world, time);
            let held = HELD_TRACE_SAMPLES.with(std::cell::Cell::get);
            reference.seek(&mut reference_world, time);
            assert_eq!(
                HELD_TRACE_SAMPLES.with(std::cell::Cell::get),
                held,
                "the reference replays every sample"
            );
            assert_eq!(
                held > 0,
                time > 7.1 && bindings != TraceBindings::OnSource,
                "t={time}: {held} held samples"
            );
            assert_eq!(
                world.get::<SpatialTransform>(bound),
                reference_world.get::<SpatialTransform>(reference_bound),
                "bound drawable at t={time}"
            );
            let trail = |world: &World, entity| {
                let trace = world.get::<gaanim_animation::TracedPath>(entity).unwrap();
                (trace.points.clone(), trace.sample_times.clone())
            };
            assert_eq!(
                trail(&world, entity),
                trail(&reference_world, reference_entity),
                "t={time}"
            );
            assert_eq!(
                world.get::<Path2D>(entity),
                reference_world.get::<Path2D>(reference_entity),
                "t={time}"
            );
        }
    }
}

#[test]
fn traces_keep_segment_checkpoints() {
    let trace = (0.01, None, 0.0, Some(0.3));
    let (mut world, mut timeline, entity, _) =
        settling_trace_world(Reactive::None, true, trace, TraceBindings::Elsewhere);
    let (mut reference_world, mut reference, reference_entity, _) =
        settling_trace_world(Reactive::None, false, trace, TraceBindings::Elsewhere);
    let times = (0..=40)
        .map(|step| f64::from(step) * 0.75)
        .chain([31.0, 13.0, 35.5, 1.5, 0.9, 23.9, 23.95, 24.0, 6.1]);
    for time in times {
        timeline.seek(&mut world, time);
        reference.seek(&mut reference_world, time);
        for (component, same) in [
            (
                "trail",
                world
                    .get::<gaanim_animation::TracedPath>(entity)
                    .unwrap()
                    .points
                    == reference_world
                        .get::<gaanim_animation::TracedPath>(reference_entity)
                        .unwrap()
                        .points,
            ),
            (
                "path",
                world.get::<Path2D>(entity) == reference_world.get::<Path2D>(reference_entity),
            ),
        ] {
            assert!(same, "{component} at t={time}");
        }
    }
    assert!(
        timeline.checkpoints.snapshots.len() > 2,
        "{} checkpoints",
        timeline.checkpoints.snapshots.len()
    );
    assert!(reference.checkpoints.snapshots.is_empty());
}

#[test]
fn traced_path_seek_respects_start_and_dissipating_window() {
    let mut world = World::new();
    world.insert_resource(gaanim_animation::PlaybackState::default());

    let source_id = ObjectId::from_raw(0);
    let trace_id = ObjectId::from_raw(1);
    let updater = gaanim_animation::Updater::new_simulation(
        |dt, _elapsed, entity, world| {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                transform.translation.x += dt;
            }
            true
        },
        |_entity, _world| true,
        1.0 / 240.0,
    )
    .unwrap()
    .starting_at(1.0);
    let source = world
        .spawn((MobjectId(source_id), SpatialTransform::default(), updater))
        .id();
    let traced_path = gaanim_animation::TracedPath::new(source, 0.01, None)
        .starting_at(1.0)
        .with_dissipating_time(Some(1.0));
    let trace = world
        .spawn((
            MobjectId(trace_id),
            SpatialTransform::default(),
            Path2D(Arc::new(BezPath::new())),
            traced_path,
        ))
        .id();

    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline {
        cached_duration: 3.0,
        ..Timeline::default()
    };
    timeline.add_keyframe(0.0, snapshot);

    timeline.seek(&mut world, 0.5);
    assert!(
        world
            .get::<gaanim_animation::TracedPath>(trace)
            .unwrap()
            .points
            .is_empty()
    );

    timeline.seek(&mut world, 2.5);
    let at_two_and_a_half = world
        .get::<gaanim_animation::TracedPath>(trace)
        .unwrap()
        .sample_times
        .clone();
    assert!(!at_two_and_a_half.is_empty());
    assert!(at_two_and_a_half.iter().all(|time| *time >= 1.5 - 1e-9));
    assert!(at_two_and_a_half.iter().all(|time| *time <= 2.5 + 1e-9));

    timeline.seek(&mut world, 1.5);
    timeline.seek(&mut world, 2.5);
    assert_eq!(
        world
            .get::<gaanim_animation::TracedPath>(trace)
            .unwrap()
            .sample_times,
        at_two_and_a_half
    );
}

/// A simulated source with clips of its own, such as a fade in, is
/// traced where it steps to: replaying its clips leaves its translation
/// to the simulation, also when a sample falls between two steps.
#[test]
fn traced_path_seek_keeps_simulated_sources_where_they_step() {
    let mut world = World::new();
    world.insert_resource(gaanim_animation::PlaybackState::default());
    let source_id = ObjectId::from_raw(0);
    let updater = gaanim_animation::Updater::new_simulation(
        |dt, _elapsed, entity, world| {
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                transform.translation.x += dt;
            }
            true
        },
        |_entity, _world| true,
        0.1,
    )
    .unwrap();
    let source = world
        .spawn((
            MobjectId(source_id),
            SpatialTransform::default(),
            gaanim_scene::Opacity(0.0),
            updater,
        ))
        .id();
    let trace = world
        .spawn((
            MobjectId(ObjectId::from_raw(1)),
            SpatialTransform::default(),
            Path2D(Arc::new(BezPath::new())),
            gaanim_animation::TracedPath::new(source, 0.0, None),
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let mut timeline = Timeline {
        cached_duration: 2.0,
        ..Timeline::default()
    };
    let track = timeline.add_track("fade", 0);
    timeline.add_keyframe(0.0, snapshot);
    timeline.add_clip(
        track,
        0.0,
        0.3,
        ClipPayload::Animation(AnimationSpec {
            target: source_id,
            lens: PropertyLensSpec::Opacity { from: 0.0, to: 1.0 },
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        }),
    );

    timeline.seek(&mut world, 1.05);
    let source_at = world.get::<SpatialTransform>(source).unwrap().translation;
    assert!((source_at.x - 1.0).abs() < 1e-9, "{source_at:?}");
    let points = &world
        .get::<gaanim_animation::TracedPath>(trace)
        .unwrap()
        .points;
    assert!(
        points.windows(2).all(|pair| pair[1].x >= pair[0].x - 1e-9),
        "the trail never jumps back to the start: {points:?}"
    );
    assert!((points.last().unwrap().x - 1.0).abs() < 1e-9);
}

#[test]
fn remove_updater_clip_removes_component_after_timestamp() {
    let mut world = World::new();
    let object_id = ObjectId::from_raw(0);
    let entity = world
        .spawn((
            MobjectId(object_id),
            SpatialTransform::default(),
            gaanim_animation::orbit_updater(gaanim_core::glam::DVec3::ZERO, 10.0, 1.0),
        ))
        .id();

    let snapshot = WorldSnapshot::capture(&mut world);

    let mut timeline = Timeline::default();
    let track = timeline.tracks.insert_with_key(|id| crate::clip::Track {
        id,
        name: "Reactive".into(),
        order: 0,
        object_id: Some(object_id),
        scene: None,
    });
    timeline.add_keyframe(0.0, snapshot);
    timeline.add_clip(
        track,
        1.0,
        0.0,
        ClipPayload::RemoveUpdater { target: object_id },
    );

    timeline.seek(&mut world, 0.5);
    assert!(world.get::<gaanim_animation::Updater>(entity).is_some());
    assert_eq!(
        world
            .get::<gaanim_animation::Updater>(entity)
            .and_then(|u| u.stop_at),
        None
    );

    timeline.seek(&mut world, 1.5);
    let updater = world
        .get::<gaanim_animation::Updater>(entity)
        .expect("updater should remain present but frozen");
    assert_eq!(updater.stop_at, Some(1.0));
    assert!((updater.elapsed - 1.0).abs() < 1e-9);
}

#[test]
fn seeking_within_a_scene_does_not_toggle_inactive_scene_visibility() {
    use bevy::ecs::system::SystemState;
    use bevy::prelude::RemovedComponents;
    use gaanim_scene::Visible;

    let mut world = World::new();
    let mut timeline = Timeline::default();
    let first = timeline.add_scene("first");
    let second = timeline.add_scene("second");
    timeline.index_scene(first, 0.0);
    timeline.index_scene(second, 1.0);
    let track = timeline.add_track("slides", 0);
    timeline.add_clip(track, 0.0, 2.0, ClipPayload::Wait);
    timeline.add_clip(
        track,
        0.75,
        0.5,
        ClipPayload::Transition {
            from: first,
            to: second,
            transition_type: TransitionType::CrossFade { duration: 0.5 },
        },
    );
    let first_entity = world
        .spawn((
            MobjectId(ObjectId::from_raw(700)),
            SceneMember(first),
            Visible,
        ))
        .id();
    let second_entity = world
        .spawn((
            MobjectId(ObjectId::from_raw(701)),
            SceneMember(second),
            Visible,
        ))
        .id();
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.25);
    assert!(world.get::<Visible>(first_entity).is_some());
    assert!(world.get::<Visible>(second_entity).is_none());
    let mut removals = SystemState::<RemovedComponents<Visible>>::new(&mut world);
    assert_eq!(removals.get_mut(&mut world).unwrap().read().count(), 1);
    world.clear_trackers();

    timeline.seek(&mut world, 0.5);
    assert_eq!(
        removals.get_mut(&mut world).unwrap().read().count(),
        0,
        "inactive slide entities must not be shown and hidden again every frame"
    );

    timeline.seek(&mut world, 0.875);
    assert!(world.get::<Visible>(first_entity).is_some());
    assert!(world.get::<Visible>(second_entity).is_some());
    assert_eq!(world.get::<Opacity>(first_entity).unwrap().0, 0.75);
    assert_eq!(world.get::<Opacity>(second_entity).unwrap().0, 0.25);
    timeline.seek(&mut world, 1.5);
    assert!(world.get::<Visible>(first_entity).is_none());
    assert!(world.get::<Visible>(second_entity).is_some());
    timeline.seek(&mut world, 0.25);
    assert!(world.get::<Visible>(first_entity).is_some());
    assert!(world.get::<Visible>(second_entity).is_none());
    timeline.keyframes[&OrderedFloat(0.0)].restore(&mut world);
    assert!(world.get::<Visible>(first_entity).is_some());
    assert!(world.get::<Visible>(second_entity).is_some());
}

#[test]
fn scene_membership_event_is_applied_at_its_timestamp_and_reversible() {
    let mut world = World::new();
    let object_id = ObjectId::from_raw(0);
    let mut timeline = Timeline::default();
    let first_scene = timeline.add_scene("first");
    let second_scene = timeline.add_scene("second");
    let entity = world
        .spawn((
            MobjectId(object_id),
            SpatialTransform::default(),
            SceneMember(first_scene),
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);
    let track = timeline.add_track("Scene membership", 0);
    timeline.add_keyframe(0.0, snapshot);
    timeline.add_clip(
        track,
        1.0,
        0.0,
        ClipPayload::SetSceneMember {
            target: object_id,
            scene: Some(second_scene),
        },
    );
    timeline.add_clip(
        track,
        2.0,
        0.0,
        ClipPayload::SetSceneMember {
            target: object_id,
            scene: None,
        },
    );

    timeline.seek(&mut world, 0.5);
    assert_eq!(
        world.get::<SceneMember>(entity).map(|s| s.0),
        Some(first_scene)
    );

    timeline.seek(&mut world, 1.0);
    assert_eq!(
        world.get::<SceneMember>(entity).map(|s| s.0),
        Some(second_scene)
    );

    timeline.seek(&mut world, 2.0);
    assert_eq!(world.get::<SceneMember>(entity), None);
    assert!(world.get::<gaanim_scene::Visible>(entity).is_some());

    timeline.seek(&mut world, 0.5);
    assert_eq!(
        world.get::<SceneMember>(entity).map(|s| s.0),
        Some(first_scene)
    );
}

#[test]
fn completed_spring_morph_commits_exact_target_path() {
    let mut from = BezPath::new();
    from.move_to((0.0, 0.0));
    from.line_to((10.0, 0.0));
    from.line_to((5.0, 10.0));
    from.close_path();

    let mut to = BezPath::new();
    to.move_to((0.0, 0.0));
    to.curve_to((0.0, 8.0), (10.0, 8.0), (10.0, 0.0));
    to.close_path();

    let mut world = World::new();
    let object_id = ObjectId::from_raw(0);
    let entity = world
        .spawn((
            MobjectId(object_id),
            SpatialTransform::default(),
            Path2D(Arc::new(from.clone())),
            PathSource(Arc::new(from.clone())),
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);

    let mut timeline = Timeline::default();
    let track = timeline.add_track("Morph", 0);
    timeline.add_keyframe(0.0, snapshot);
    timeline.add_clip(
        track,
        0.0,
        1.0,
        ClipPayload::Animation(AnimationSpec {
            target: object_id,
            lens: PropertyLensSpec::PathMorph {
                from,
                to: to.clone(),
                table: Default::default(),
            },
            rate_func: RateFunc::Spring {
                stiffness: 90.0,
                damping: 12.0,
            },
            delay: 0.0,
            label: Some("Transform".into()),
        }),
    );

    timeline.seek(&mut world, 0.5);
    let active_path = world.get::<Path2D>(entity).unwrap().0.clone();
    assert_eq!(world.get::<PathSource>(entity).unwrap().0, active_path);
    assert_ne!(active_path.as_ref(), &to);

    timeline.seek(&mut world, 1.0);

    assert_eq!(world.get::<Path2D>(entity).unwrap().0.as_ref(), &to);
    assert_eq!(world.get::<PathSource>(entity).unwrap().0.as_ref(), &to);
}

#[test]
fn semantic_segment_positions_and_stops_are_explicit() {
    let mut timeline = Timeline::default();
    timeline.set_segments(vec![
        SegmentMetadata {
            id: 10,
            name: "intro".to_string(),
            notes: Some("Opening".to_string()),
            target: None,
            start_time: 0.0,
            end_time: 3.0,
            stops: vec![SegmentStop {
                name: Some("señal".to_string()),
                time: 1.0,
                ambient: None,
            }],
        },
        SegmentMetadata {
            id: 20,
            name: "Área".to_string(),
            notes: None,
            target: None,
            start_time: 3.0,
            end_time: 5.0,
            stops: vec![
                SegmentStop {
                    name: None,
                    time: 4.0,
                    ambient: None,
                },
                SegmentStop {
                    name: Some("nearby".to_string()),
                    time: 4.1,
                    ambient: None,
                },
            ],
        },
    ]);

    assert_eq!(
        timeline.segment_position_at(1.5),
        Some(SegmentPosition {
            segment_id: 10,
            stop_index: Some(0),
        })
    );
    assert_eq!(timeline.next_stop(1.0), Some(4.0));
    assert_eq!(timeline.previous_stop(3.0), Some(1.0));
    assert_eq!(timeline.previous_stop(4.1), Some(4.0));
    assert_eq!(
        timeline.segment_time(SegmentPosition {
            segment_id: 20,
            stop_index: Some(0),
        }),
        Some(4.0)
    );
    assert_eq!(timeline.segment_time_named("ÁREA"), Some(3.0));
    assert_eq!(
        timeline.segment_stop_time_named("INTRO", "SEÑAL"),
        Some(1.0)
    );
    assert_eq!(timeline.segment_time_indexed("área", Some(0)), Some(4.0));
    assert_eq!(timeline.segment_time_indexed("área", Some(2)), None);

    timeline.current_time = 4.0;
    timeline.update_segment_position();
    assert_eq!(
        timeline.segment_label().as_deref(),
        Some("2 / 2 · Área · stop 1")
    );
}

#[test]
fn playback_seeks_only_rebuild_fragments_of_animating_objects() {
    use bevy::prelude::{App, Update};
    use gaanim_math::GlobalSpatialTransform;
    use gaanim_renderer::pipeline::{GaanimRenderCache, gaanim_render_system};
    use gaanim_scene::{GlobalOpacity, RenderLayer, RenderOrder, Visible};

    // `SetTextBaseline` is not an absolute 2D clip, so it forces every
    // playback seek through the full keyframe restore and clip replay.
    for force_restore in [false, true] {
        let mut app = App::new();
        app.init_resource::<GaanimRenderCache>()
            .add_systems(Update, gaanim_render_system);
        let mut timeline = Timeline::default();
        let track = timeline.add_track("segments", 0);
        let ids: Vec<_> = (1..=3).map(ObjectId::from_raw).collect();
        for (index, &id) in ids.iter().enumerate() {
            let mut path = BezPath::new();
            path.move_to((0.0, 0.0));
            path.curve_to((10.0, 20.0), (20.0, 20.0), (30.0, 0.0));
            app.world_mut().spawn((
                MobjectId(id),
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                Opacity(1.0),
                GlobalOpacity(1.0),
                RenderOrder::default(),
                RenderLayer::Vello2D,
                Visible,
                FillBrush::color(gaanim_core::peniko::Color::WHITE),
                Path2D(Arc::new(path.clone())),
                PathSource(Arc::new(path)),
            ));
            timeline.add_clip(
                track,
                index as f64,
                1.0,
                ClipPayload::Animation(AnimationSpec {
                    target: id,
                    lens: PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
                    rate_func: RateFunc::Linear,
                    delay: 0.0,
                    label: None,
                }),
            );
            if force_restore {
                timeline.add_clip(
                    track,
                    index as f64,
                    0.0,
                    ClipPayload::SetTextBaseline {
                        target: id,
                        baseline: 1.0,
                    },
                );
            }
        }
        timeline.add_keyframe(0.0, WorldSnapshot::capture(app.world_mut()));
        app.world_mut()
            .insert_resource(gaanim_animation::PlaybackState::default());

        let fragments = |app: &App| {
            let cache = &app.world().resource::<GaanimRenderCache>().fragment_cache;
            ids.iter().map(|id| cache[id].clone()).collect::<Vec<_>>()
        };
        timeline.seek(app.world_mut(), 2.25);
        app.update();
        let before = fragments(&app);
        app.world_mut()
            .resource_mut::<gaanim_animation::PlaybackState>()
            .scaled_dt = 1.0 / 60.0;
        timeline.seek(app.world_mut(), 2.25 + 1.0 / 60.0);
        app.update();
        let after = fragments(&app);

        let rebuilt: Vec<_> = before
            .iter()
            .zip(&after)
            .map(|(before, after)| !Arc::ptr_eq(before, after))
            .collect();
        assert_eq!(
            rebuilt,
            [false, false, true],
            "restore={force_restore}: completed objects must keep their retained fragments"
        );
    }
}

fn incremental_replay_fixture(with_scenes: bool) -> (World, Timeline, [Entity; 3]) {
    let mut world = World::new();
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.curve_to((10.0, 20.0), (20.0, 20.0), (30.0, 0.0));
    let [written, faded, external] = [1, 2, 3].map(|raw| {
        world
            .spawn((
                MobjectId(ObjectId::from_raw(raw)),
                SpatialTransform::default(),
                Opacity(1.0),
                Path2D(Arc::new(path.clone())),
                PathSource(Arc::new(path.clone())),
                gaanim_scene::LocalBounds::default(),
            ))
            .id()
    });
    // A reactive signal disables the whole-world replay shortcut, so every
    // seek goes through the keyframe restore.
    world.spawn((
        MobjectId(ObjectId::from_raw(4)),
        SpatialTransform::default(),
        Opacity(1.0),
        gaanim_animation::FloatSignal::new(0.0),
    ));
    let mut timeline = Timeline::default();
    let track = timeline.add_track("replay", 0);
    let animate = |target, lens| {
        ClipPayload::Animation(AnimationSpec {
            target: ObjectId::from_raw(target),
            lens,
            rate_func: RateFunc::Linear,
            delay: 0.0,
            label: None,
        })
    };
    timeline.add_clip(
        track,
        0.0,
        1.0,
        animate(1, PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 }),
    );
    timeline.add_clip(
        track,
        1.0,
        1.0,
        animate(
            1,
            PropertyLensSpec::Translation {
                from: gaanim_core::glam::DVec3::ZERO,
                to: gaanim_core::glam::DVec3::new(3.0, 1.0, 0.0),
            },
        ),
    );
    timeline.add_clip(
        track,
        0.5,
        1.0,
        animate(2, PropertyLensSpec::Opacity { from: 0.0, to: 1.0 }),
    );
    timeline.add_clip(
        track,
        2.0,
        0.5,
        animate(2, PropertyLensSpec::PathCompletion { from: 1.0, to: 0.4 }),
    );
    // Hidden until its clip starts; the external write restores its full path.
    timeline.add_clip(
        track,
        2.2,
        0.4,
        animate(3, PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 }),
    );
    if with_scenes {
        // Segments compile to scenes: membership, visibility and a
        // transform-writing transition all change between slides.
        let first = timeline.add_scene("first");
        let second = timeline.add_scene("second");
        timeline.index_scene(first, 0.0);
        timeline.index_scene(second, 1.5);
        timeline.add_clip(track, 0.0, 0.0, ClipPayload::SceneStart(first));
        timeline.add_clip(track, 1.5, 0.0, ClipPayload::SceneEnd(first));
        timeline.add_clip(track, 1.5, 0.0, ClipPayload::SceneStart(second));
        timeline.add_clip(
            track,
            1.25,
            0.5,
            ClipPayload::Transition {
                from: first,
                to: second,
                transition_type: TransitionType::Slide {
                    duration: 0.5,
                    direction: crate::transition::SlideDirection::Left,
                },
            },
        );
        timeline.add_clip(
            track,
            2.0,
            0.0,
            ClipPayload::SetSceneMember {
                target: ObjectId::from_raw(1),
                scene: Some(second),
            },
        );
        for (entity, scene) in [(written, first), (faded, first), (external, second)] {
            world
                .entity_mut(entity)
                .insert((SceneMember(scene), gaanim_scene::Visible));
        }
    }
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    (world, timeline, [written, faded, external])
}

#[test]
fn forward_replay_restores_only_changed_entities_and_matches_full_restore() {
    for with_scenes in [false, true] {
        assert_incremental_replay_matches_full_restore(with_scenes);
    }
}

fn assert_incremental_replay_matches_full_restore(with_scenes: bool) {
    let (mut world, mut timeline, entities) = incremental_replay_fixture(with_scenes);
    let (mut reference_world, mut reference, reference_entities) =
        incremental_replay_fixture(with_scenes);
    let times = (0..=48)
        .map(|step| f64::from(step) / 16.0)
        .chain([0.8, 0.9, 1.7, 2.2, 2.9]);
    for (step, time) in times.enumerate() {
        if step == 20 {
            // A non-timeline write (an updater or layout pass) since the last seek.
            world
                .get_mut::<SpatialTransform>(entities[2])
                .unwrap()
                .translation
                .x = 5.0;
            reference_world
                .get_mut::<SpatialTransform>(reference_entities[2])
                .unwrap()
                .translation
                .x = 5.0;
        }
        timeline.seek(&mut world, time);
        reference.replay_baseline = None;
        reference.seek(&mut reference_world, time);
        for (&entity, &reference_entity) in entities.iter().zip(&reference_entities) {
            assert_eq!(
                world.get::<SpatialTransform>(entity),
                reference_world.get::<SpatialTransform>(reference_entity),
                "transform at t={time}"
            );
            assert_eq!(
                world.get::<Opacity>(entity),
                reference_world.get::<Opacity>(reference_entity),
                "opacity at t={time}"
            );
            assert_eq!(
                world.get::<Path2D>(entity),
                reference_world.get::<Path2D>(reference_entity),
                "path at t={time}"
            );
            assert_eq!(
                world.get::<SceneMember>(entity),
                reference_world.get::<SceneMember>(reference_entity),
                "scene at t={time}"
            );
            assert_eq!(
                world.get::<gaanim_scene::Visible>(entity).is_some(),
                reference_world
                    .get::<gaanim_scene::Visible>(reference_entity)
                    .is_some(),
                "visibility at t={time}, scenes={with_scenes}"
            );
        }
    }

    // Once written, neither the finished path nor the static entity is
    // rewritten by later playback frames.
    let settled = if with_scenes { 2.6 } else { 1.2 };
    timeline.seek(&mut world, settled);
    timeline.seek(&mut world, settled + 0.05);
    world.clear_trackers();
    timeline.seek(&mut world, settled + 0.1);
    let changed_paths = world
        .query_filtered::<Entity, bevy::prelude::Changed<Path2D>>()
        .iter(&world)
        .collect::<Vec<_>>();
    assert!(changed_paths.is_empty(), "{changed_paths:?}");
    assert!(
        !world
            .query_filtered::<Entity, bevy::prelude::Changed<SpatialTransform>>()
            .iter(&world)
            .any(|entity| entity == entities[2])
    );
}

/// What layout, connectors and bindings do after every seek: derive the
/// bounds of each entity from its pose, and redraw one path from its
/// opacity.
fn derive_layout(world: &mut World, entities: &[Entity; 3]) {
    for &entity in entities {
        let x = world.get::<SpatialTransform>(entity).unwrap().translation.x;
        world
            .get_mut::<gaanim_scene::LocalBounds>(entity)
            .unwrap()
            .0 = gaanim_math::Bounds3D::new_2d(x, 0.0, x + 10.0, 1.0);
    }
    let opacity = f64::from(world.get::<Opacity>(entities[1]).unwrap().0);
    let rect = gaanim_core::kurbo::Rect::new(0.0, 0.0, 1.0 + 10.0 * opacity, 2.0);
    world.get_mut::<Path2D>(entities[1]).unwrap().0 =
        Arc::new(gaanim_core::kurbo::Shape::to_path(&rect, 0.1));
}

#[test]
fn derived_writes_restore_only_their_components_and_match_full_restore() {
    for with_scenes in [false, true] {
        let (mut world, mut timeline, entities) = incremental_replay_fixture(with_scenes);
        let (mut reference_world, mut reference, reference_entities) =
            incremental_replay_fixture(with_scenes);
        let same = |world: &World, reference_world: &World, time: f64, stage: &str| {
            for (&entity, &reference_entity) in entities.iter().zip(&reference_entities) {
                let context = format!("{stage} at t={time}, scenes={with_scenes}");
                assert_eq!(
                    world.get::<SpatialTransform>(entity),
                    reference_world.get::<SpatialTransform>(reference_entity),
                    "transform {context}"
                );
                assert_eq!(
                    world.get::<Opacity>(entity),
                    reference_world.get::<Opacity>(reference_entity),
                    "opacity {context}"
                );
                assert_eq!(
                    world.get::<Path2D>(entity),
                    reference_world.get::<Path2D>(reference_entity),
                    "path {context}"
                );
                assert_eq!(
                    world.get::<gaanim_scene::LocalBounds>(entity),
                    reference_world.get::<gaanim_scene::LocalBounds>(reference_entity),
                    "bounds {context}"
                );
                assert_eq!(
                    world.get::<gaanim_scene::Visible>(entity).is_some(),
                    reference_world
                        .get::<gaanim_scene::Visible>(reference_entity)
                        .is_some(),
                    "visibility {context}"
                );
            }
        };
        let times = (0..=48)
            .map(|step| f64::from(step) / 16.0)
            .chain([0.8, 0.9, 1.7, 2.2, 2.9]);
        for time in times {
            timeline.seek(&mut world, time);
            reference.replay_baseline = None;
            reference.seek(&mut reference_world, time);
            same(&world, &reference_world, time, "after the seek");
            derive_layout(&mut world, &entities);
            derive_layout(&mut reference_world, &reference_entities);
            same(&world, &reference_world, time, "after layout");
        }

        // Once settled, a seek after layout restores the bounds layout
        // wrote and leaves the rest of the entity alone.
        let settled = if with_scenes { 2.6 } else { 1.2 };
        for time in [settled, settled + 0.05] {
            timeline.seek(&mut world, time);
            derive_layout(&mut world, &entities);
        }
        world.clear_trackers();
        derive_layout(&mut world, &entities);
        timeline.seek(&mut world, settled + 0.1);
        let changed = |world: &mut World, entity| {
            let mut query = world.query_filtered::<Entity, Or<(
                Changed<SpatialTransform>,
                Changed<Opacity>,
                Changed<PathSource>,
            )>>();
            query.iter(world).any(|changed| changed == entity)
        };
        assert!(
            !changed(&mut world, entities[2]),
            "scenes={with_scenes}: only the derived bounds are restored"
        );
    }
}

/// Four segments starting at 0, 1, 2 and 3; a clip runs across 3. With
/// `scenes`, each segment is a scene, as slides compile.
fn checkpoint_fixture(
    checkpoints: bool,
    signal: bool,
    scenes: bool,
) -> (World, Timeline, [Entity; 3]) {
    use gaanim_core::glam::DVec3;

    let mut world = World::new();
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.curve_to((10.0, 20.0), (20.0, 20.0), (30.0, 0.0));
    let entities = [1, 2, 3].map(|raw| {
        world
            .spawn((
                MobjectId(ObjectId::from_raw(raw)),
                SpatialTransform::default(),
                Opacity(1.0),
                Path2D(Arc::new(path.clone())),
                PathSource(Arc::new(path.clone())),
            ))
            .id()
    });
    if signal {
        // Forces the restoring path on the reference too.
        world.spawn((
            MobjectId(ObjectId::from_raw(4)),
            SpatialTransform::default(),
            Opacity(1.0),
            gaanim_animation::FloatSignal::new(0.0),
        ));
    }
    let mut timeline = Timeline {
        segment_checkpoints: checkpoints,
        ..Default::default()
    };
    let track = timeline.add_track("replay", 0);
    let mut clip = |target, start, duration, lens| {
        timeline.add_clip(
            track,
            start,
            duration,
            ClipPayload::Animation(AnimationSpec {
                target: ObjectId::from_raw(target),
                lens,
                rate_func: RateFunc::Linear,
                delay: 0.0,
                label: None,
            }),
        );
    };
    let at = |x| DVec3::new(x, 0.0, 0.0);
    // Written in the first segment, then undone later from another
    // start: before that clip starts, the earlier write still shows.
    clip(
        1,
        0.0,
        0.8,
        PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
    );
    clip(
        1,
        2.2,
        0.4,
        PropertyLensSpec::PathCompletion { from: 0.5, to: 0.3 },
    );
    clip(
        1,
        0.2,
        0.5,
        PropertyLensSpec::Translation {
            from: at(0.0),
            to: at(2.0),
        },
    );
    clip(
        1,
        1.0,
        0.0,
        PropertyLensSpec::Translation {
            from: at(2.0),
            to: at(-1.0),
        },
    );
    clip(
        1,
        2.0,
        0.5,
        PropertyLensSpec::Translation {
            from: at(-1.0),
            to: at(4.0),
        },
    );
    // Hidden until its clip, a segment later.
    clip(
        2,
        1.2,
        0.4,
        PropertyLensSpec::PathCompletion { from: 0.0, to: 1.0 },
    );
    clip(
        2,
        0.5,
        0.5,
        PropertyLensSpec::Opacity { from: 1.0, to: 0.2 },
    );
    clip(
        2,
        1.0,
        0.5,
        PropertyLensSpec::Opacity { from: 0.2, to: 0.9 },
    );
    // Runs across the last segment start.
    clip(
        3,
        2.9,
        0.4,
        PropertyLensSpec::Opacity { from: 1.0, to: 0.0 },
    );
    clip(
        3,
        3.4,
        0.3,
        PropertyLensSpec::PathCompletion { from: 1.0, to: 0.2 },
    );
    if scenes {
        let ids = ["first", "second", "third", "fourth"].map(|name| timeline.add_scene(name));
        for (index, &scene) in ids.iter().enumerate() {
            let start = index as f64;
            timeline.index_scene(scene, start);
            timeline.add_clip(track, start, 0.0, ClipPayload::SceneStart(scene));
            timeline.add_clip(track, start + 1.0, 0.0, ClipPayload::SceneEnd(scene));
        }
        // Ends exactly where the second segment starts.
        timeline.add_clip(
            track,
            0.8,
            0.2,
            ClipPayload::Transition {
                from: ids[0],
                to: ids[1],
                transition_type: TransitionType::Slide {
                    duration: 0.2,
                    direction: crate::transition::SlideDirection::Left,
                },
            },
        );
        // Moves to the third scene as it starts.
        timeline.add_clip(
            track,
            2.0,
            0.0,
            ClipPayload::SetSceneMember {
                target: ObjectId::from_raw(1),
                scene: Some(ids[2]),
            },
        );
        for (entity, scene) in entities.iter().zip([ids[0], ids[1], ids[3]]) {
            world
                .entity_mut(*entity)
                .insert((SceneMember(scene), gaanim_scene::Visible));
        }
    }
    let segments = [0.0, 1.0, 2.0, 3.0, 3.7]
        .windows(2)
        .enumerate()
        .map(|(index, span)| SegmentMetadata {
            id: index as u32 + 1,
            name: format!("segment {index}"),
            notes: None,
            target: None,
            start_time: span[0],
            end_time: span[1],
            stops: Vec::new(),
        })
        .collect();
    timeline.set_segments(segments);
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    (world, timeline, entities)
}

#[test]
fn checkpoint_seeks_match_seeks_from_the_first_keyframe() {
    // Forward playback, which captures each checkpoint from the one
    // before, then jumps back and forth across segments; and the same
    // times from the end, which captures the last one from t=0.
    let forward: Vec<f64> = (0..=60)
        .map(|step| f64::from(step) / 16.0)
        .chain([1.0, 0.3, 2.3, 1.1, 3.5, 2.0, 0.0, 3.05, 2.95, 1.9, 3.7])
        .collect();
    let backward: Vec<f64> = forward.iter().rev().copied().collect();
    for (signal, scenes, times) in [(false, false), (true, false), (false, true)]
        .into_iter()
        .flat_map(|(signal, scenes)| [(signal, scenes, &forward), (signal, scenes, &backward)])
    {
        let (mut world, mut timeline, entities) = checkpoint_fixture(true, signal, scenes);
        let (mut reference_world, mut reference, reference_entities) =
            checkpoint_fixture(false, signal, scenes);
        for (step, &time) in times.iter().enumerate() {
            if step % 17 == 16 && (signal || scenes) {
                // A write outside the timeline (a layout pass) since the
                // last seek, right after some checkpoint captures. A
                // restoring seek undoes it; without scenes or signals
                // the reference replays over the world and keeps it.
                for (world, entity) in [
                    (&mut world, entities[2]),
                    (&mut reference_world, reference_entities[2]),
                ] {
                    world
                        .get_mut::<SpatialTransform>(entity)
                        .unwrap()
                        .translation
                        .y = 5.0;
                }
            }
            timeline.seek(&mut world, time);
            reference.seek(&mut reference_world, time);
            for (&entity, &reference_entity) in entities.iter().zip(&reference_entities) {
                assert_eq!(
                    world.get::<SpatialTransform>(entity),
                    reference_world.get::<SpatialTransform>(reference_entity),
                    "transform at t={time}"
                );
                assert_eq!(
                    world.get::<Opacity>(entity),
                    reference_world.get::<Opacity>(reference_entity),
                    "opacity at t={time}"
                );
                assert_eq!(
                    world.get::<Path2D>(entity),
                    reference_world.get::<Path2D>(reference_entity),
                    "path at t={time}, signal={signal}"
                );
                assert_eq!(
                    world.get::<SceneMember>(entity),
                    reference_world.get::<SceneMember>(reference_entity),
                    "scene at t={time}"
                );
                assert_eq!(
                    world.get::<gaanim_scene::Visible>(entity).is_some(),
                    reference_world
                        .get::<gaanim_scene::Visible>(reference_entity)
                        .is_some(),
                    "visibility at t={time}, scenes={scenes}"
                );
            }
        }
        // 3.0 has a clip running across it.
        assert_eq!(
            timeline.checkpoints.times,
            [OrderedFloat(1.0), OrderedFloat(2.0)]
        );
        assert_eq!(timeline.checkpoints.snapshots.len(), 2);
        assert!(reference.checkpoints.snapshots.is_empty());
    }
}

#[test]
fn many_clips_checkpoint_the_next_play_without_segments() {
    let fixture = |checkpoints| {
        let (world, mut timeline, entities) = checkpoint_fixture(checkpoints, false, false);
        timeline.set_segments(Vec::new());
        let track = timeline.add_track("fades", 2);
        for index in 0..CHECKPOINT_CLIPS {
            timeline.add_clip(track, index as f64 / 1000.0, 0.5, ClipPayload::Wait);
        }
        (world, timeline, entities)
    };
    let (mut world, mut timeline, entities) = fixture(true);
    let (mut reference_world, mut reference, reference_entities) = fixture(false);
    for time in (0..=60)
        .map(|step| f64::from(step) / 16.0)
        .chain([1.1, 0.3, 3.5, 1.0, 0.0, 2.95])
    {
        timeline.seek(&mut world, time);
        reference.seek(&mut reference_world, time);
        for (&entity, &reference_entity) in entities.iter().zip(&reference_entities) {
            assert_eq!(
                world.get::<SpatialTransform>(entity),
                reference_world.get::<SpatialTransform>(reference_entity),
                "transform at t={time}"
            );
            assert_eq!(
                world.get::<Opacity>(entity),
                reference_world.get::<Opacity>(reference_entity),
                "opacity at t={time}"
            );
            assert_eq!(
                world.get::<Path2D>(entity),
                reference_world.get::<Path2D>(reference_entity),
                "path at t={time}"
            );
        }
    }
    // The first start after the fades with every earlier clip finished;
    // the later ones follow too few clips.
    assert_eq!(timeline.checkpoints.times, [OrderedFloat(1.0)]);
    assert_eq!(timeline.checkpoints.snapshots.len(), 1);
}

#[test]
fn checkpoints_are_dropped_when_the_timeline_changes() {
    let (mut world, mut timeline, _) = checkpoint_fixture(true, false, false);
    // A single seek into a segment does not capture it.
    timeline.seek(&mut world, 2.5);
    assert!(timeline.checkpoints.snapshots.is_empty());
    timeline.seek(&mut world, 2.6);
    assert_eq!(timeline.checkpoints.snapshots.len(), 1);
    let track = timeline.add_track("late", 1);
    // A clip across 2.0 leaves 1.0 as the only checkpoint.
    timeline.add_clip(track, 1.8, 0.5, ClipPayload::Wait);
    timeline.seek(&mut world, 2.5);
    timeline.seek(&mut world, 2.6);
    assert_eq!(timeline.checkpoints.times, [OrderedFloat(1.0)]);
    assert_eq!(
        timeline
            .checkpoints
            .snapshots
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [OrderedFloat(1.0)]
    );
}

#[test]
fn a_poll_takes_votes_inside_its_window() {
    let poll = |id: &str, segment: u32, open: f64, close: f64| TimelinePoll {
        id: id.to_string(),
        question: format!("{id}?"),
        options: vec!["A".to_string(), "B".to_string()],
        preview: vec![0, 0],
        segment,
        open,
        close,
        quiz: None,
        multiple: false,
        image: None,
    };
    let segment = |id: u32, start_time: f64, end_time: f64, stops: &[f64]| SegmentMetadata {
        id,
        name: format!("segment {id}"),
        notes: None,
        target: None,
        start_time,
        end_time,
        stops: stops
            .iter()
            .map(|time| SegmentStop {
                name: None,
                time: *time,
                ambient: None,
            })
            .collect(),
    };
    let mut timeline = Timeline::new();
    timeline.set_segments(vec![segment(1, 0.0, 4.0, &[]), segment(2, 4.0, 6.0, &[])]);
    timeline.set_polls(
        vec![poll("second", 2, 4.0, 6.0), poll("first", 1, 1.0, 4.0)],
        None,
    );

    assert_eq!(timeline.polls[0].id, "first");
    let open_at =
        |timeline: &Timeline, time| timeline.poll_open_at(time).map(|poll| poll.id.clone());
    assert_eq!(open_at(&timeline, 0.5), None);
    assert_eq!(open_at(&timeline, 1.0).as_deref(), Some("first"));
    // Playing through the boundary shows the next segment and its poll.
    assert_eq!(open_at(&timeline, 4.0).as_deref(), Some("second"));
    assert_eq!(open_at(&timeline, 6.0).as_deref(), Some("second"));
    assert_eq!(open_at(&timeline, 6.5), None);

    // A stop at the end of the first segment keeps its poll open there.
    timeline.set_segments(vec![
        segment(1, 0.0, 4.0, &[4.0]),
        segment(2, 4.0, 6.0, &[]),
    ]);
    assert_eq!(open_at(&timeline, 4.0).as_deref(), Some("first"));

    // A lobby without polls ending in a stop: resting there must not
    // open the next segment's question on the phones.
    timeline.set_polls(vec![poll("second", 2, 4.0, 6.0)], None);
    assert_eq!(open_at(&timeline, 4.0), None);
    assert_eq!(open_at(&timeline, 4.01).as_deref(), Some("second"));

    // The podium comes once the last question is behind.
    timeline.set_segments(vec![
        segment(1, 0.0, 4.0, &[4.0]),
        segment(2, 4.0, 6.0, &[6.0]),
        segment(3, 6.0, 9.0, &[9.0]),
    ]);
    assert_eq!(timeline.game_stage(4.0), GameStage::Play);
    assert_eq!(timeline.game_stage(6.0), GameStage::Play);
    assert_eq!(timeline.game_stage(6.5), GameStage::Podium);
    assert_eq!(timeline.game_stage(9.0), GameStage::Podium);
}
