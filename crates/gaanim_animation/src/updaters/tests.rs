use super::*;

#[test]
fn a_3d_line_lays_its_mesh_along_any_direction_and_hides_when_too_short() {
    let near = |a: DVec3, b: DVec3| a.distance(b) < 1e-9;
    let from = DVec3::new(1.0, 2.0, 3.0);
    // Straight down, against the mesh's own axis.
    let down = line_3d_transform(DMat4::IDENTITY, from, from - DVec3::Y * 4.0, 1.0);
    assert!(near(down.translation, from - DVec3::Y * 2.0), "{down:?}");
    assert!(near(down.rotation * DVec3::Y, -DVec3::Y), "{down:?}");
    assert!(near(down.scale, DVec3::new(1.0, 4.0, 1.0)), "{down:?}");
    // Nothing of it shows before it grows, nor between ends that meet.
    assert_eq!(
        line_3d_transform(DMat4::IDENTITY, from, DVec3::ZERO, 0.0).scale,
        DVec3::ZERO
    );
    assert_eq!(
        line_3d_transform(DMat4::IDENTITY, from, from, 1.0).scale,
        DVec3::ZERO
    );
    // Under a parent, the ends stay where they are in the scene.
    let parent = DMat4::from_scale_rotation_translation(
        DVec3::splat(2.0),
        DQuat::from_rotation_z(0.7),
        DVec3::new(-1.0, 0.5, 0.0),
    );
    let to = DVec3::new(-2.0, 1.0, 0.5);
    let placed = line_3d_transform(parent, from, to, 1.0).to_mat4();
    let world = parent * placed;
    assert!(
        near(world.transform_point3(DVec3::Y * -0.5), from),
        "{world:?}"
    );
    assert!(
        near(world.transform_point3(DVec3::Y * 0.5), to),
        "{world:?}"
    );
}
use bevy::prelude::BuildChildrenTransformExt;
use gaanim_core::kurbo::Shape;

#[test]
fn a_stepped_drawable_runs_its_updaters_on_its_steps() {
    let mut world = World::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    world.spawn((
        StepRate(4.0),
        Updater::new(move |dt, elapsed, _, _| {
            record.lock().unwrap().push((dt, elapsed));
            true
        }),
    ));
    for _ in 0..3 {
        advance_updaters_by(&mut world, 0.1);
    }
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let seen_now = seen.lock().unwrap().clone();
    let expected = [(0.0, 0.0), (0.0, 0.0), (0.25, 0.25)];
    assert!(
        seen_now
            .iter()
            .zip(expected)
            .all(|(&(dt, at), (want_dt, want_at))| close(dt, want_dt) && close(at, want_at)),
        "{seen_now:?}"
    );
    seek_updaters(&mut world, 0.6);
    assert_eq!(seen.lock().unwrap().last(), Some(&(0.0, 0.5)));
}

#[test]
fn orientation_samples_turn_the_shorter_way() {
    let quarter = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
    // The second sample is the same turn written as -q: halfway is an
    // eighth of a turn, not a turn the long way round.
    let driver = SampledSeriesDriver::orientations(
        vec![0.0, 1.0],
        vec![DQuat::IDENTITY, -quarter * 2.0],
        SampledInterpolation::Linear,
    )
    .unwrap();
    let half = driver.sample_orientation(0.5).unwrap();
    let expected = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_4);
    assert!(half.dot(expected).abs() > 1.0 - 1e-12, "{half:?}");
    assert!(driver.sample_orientation(2.0).unwrap().dot(quarter).abs() > 1.0 - 1e-12);
    assert!(
        SampledSeriesDriver::orientations(
            vec![0.0, 1.0],
            vec![DQuat::IDENTITY, DQuat::from_xyzw(0.0, 0.0, 0.0, 0.0)],
            SampledInterpolation::Linear,
        )
        .is_err()
    );

    let mut world = World::new();
    world.insert_resource(PlaybackState {
        current_time: 0.25,
        ..Default::default()
    });
    let entity = world
        .spawn((
            SpatialTransform::default(),
            SampledSeriesDrivers::from(driver),
        ))
        .id();
    sampled_series_system(&mut world);
    let rotation = world.get::<SpatialTransform>(entity).unwrap().rotation;
    let expected = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_8);
    assert!(rotation.dot(expected).abs() > 1.0 - 1e-12, "{rotation:?}");
}

#[test]
fn anchors_on_hud_overlays_follow_their_pinned_place() {
    let mut world = World::new();
    let mut camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
    camera.position = DVec3::new(2.0, 1.0, 0.0);
    camera.projection = gaanim_math::Projection::Orthographic { zoom: 2.0 };
    world.insert_resource(camera);
    let local = gaanim_math::Bounds3D::new_2d(-1.0, -0.5, 1.0, 0.5);
    let screen = world
        .spawn((
            SpatialTransform::new_2d(4.0, 3.0),
            LocalBounds(local),
            gaanim_scene::HudOverlay,
        ))
        .id();
    let corner = TrackingEndpoint::EntityAnchor {
        entity: screen,
        normalized: DVec3::new(1.0, 1.0, 0.0),
        offset: DVec3::ZERO,
    };
    let world_point = resolve_tracking_endpoint(&corner, &world).unwrap();
    // The moved camera shows it where the unmoved one shows (5, 3.5).
    let unmoved = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
    let pixel =
        camera.to_vello_transform() * gaanim_core::kurbo::Point::new(world_point.x, world_point.y);
    let expected = unmoved.to_vello_transform() * gaanim_core::kurbo::Point::new(5.0, 3.5);
    assert!(
        (pixel - expected).hypot() < 1e-9,
        "{pixel:?} != {expected:?}"
    );
}

#[test]
fn connector_handles_collapsed_points_and_short_final_segment() {
    assert!(connector_path(&[DVec3::ZERO, DVec3::ZERO], 0.18, 0.15, 0.036, None, 1.0).is_empty());
    assert!(connector_path(&[DVec3::ZERO, DVec3::NAN], 0.18, 0.15, 0.036, None, 1.0).is_empty());
    let points = [DVec3::ZERO, DVec3::ZERO, DVec3::new(0.1, 0.0, 0.0)];
    let path = connector_path(&points, 0.18, 0.15, 0.036, Some(0.5), 1.0);
    let bounds = path.bounding_box();
    assert!((bounds.x1 - 0.1).abs() < 1e-9);
    assert!(bounds.x0.abs() < 1e-9);
    assert!((bounds.height() - 0.15 * 0.05 / 0.18).abs() < 1e-9);
}

#[test]
fn partial_connector_grows_from_the_tail_with_a_full_size_head() {
    let points = [
        DVec3::ZERO,
        DVec3::new(2.0, 0.0, 0.0),
        DVec3::new(2.0, 2.0, 0.0),
    ];
    let (head_length, head_width, body) = (0.4, 0.3, 0.05);
    assert!(connector_path(&points, head_length, head_width, body, None, 0.0).is_empty());

    // Halfway (2 of 4 units) the tip sits on the corner, with the head
    // pointing along the first segment at full size.
    let half = connector_path(&points, head_length, head_width, body, None, 0.5);
    let bounds = half.bounding_box();
    assert!((bounds.x1 - 2.0).abs() < 1e-9, "{bounds:?}");
    assert!(bounds.x0.abs() < 1e-9);
    assert!((bounds.height() - head_width).abs() < 1e-9);

    // Before the visible length reaches the head size the head shrinks.
    let early = connector_path(&points, head_length, head_width, body, None, 0.05);
    let bounds = early.bounding_box();
    assert!((bounds.x1 - 0.2).abs() < 1e-9);
    assert!((bounds.height() - head_width * 0.5).abs() < 1e-9);

    // Completion matches the static connector exactly.
    let complete = connector_path(&points, head_length, head_width, body, None, 1.0);
    let nearly = connector_path(&points, head_length, head_width, body, None, 1.0 - 1e-12);
    let (a, b) = (complete.bounding_box(), nearly.bounding_box());
    assert!((a.x0 - b.x0).abs() < 1e-6 && (a.x1 - b.x1).abs() < 1e-6);
    assert!((a.y0 - b.y0).abs() < 1e-6 && (a.y1 - b.y1).abs() < 1e-6);
    assert!((a.y1 - 2.0).abs() < 1e-9);
}

#[test]
fn surrounding_rect_unions_live_bounds_and_interpolates_edges() {
    let mut world = World::new();
    let left_id = ObjectId::from_parts(1, 1);
    let right_id = ObjectId::from_parts(2, 1);
    world.spawn((
        gaanim_scene::MobjectId(left_id),
        SpatialTransform::new_2d(-20.0, 5.0),
        LocalBounds(gaanim_math::Bounds3D::new_2d(-10.0, -5.0, 10.0, 5.0)),
    ));
    world.spawn((
        gaanim_scene::MobjectId(right_id),
        SpatialTransform::new_2d(80.0, 25.0),
        LocalBounds(gaanim_math::Bounds3D::new_2d(-20.0, -10.0, 20.0, 10.0)),
    ));
    let empty = Arc::new(BezPath::new());
    let frame = world
        .spawn((
            gaanim_scene::MobjectId(ObjectId::from_parts(3, 1)),
            SpatialTransform::default(),
            Path2D(empty.clone()),
            PathSource(empty),
            LocalBounds(gaanim_math::Bounds3D::default()),
            SurroundingRect {
                from: vec![left_id],
                to: vec![right_id],
                progress: 0.5,
                padding: [2.0, 4.0, 6.0, 8.0],
                corner_radius: 8.0,
                last_bounds: None,
                shape: SurroundingShape::Frame,
                notation: None,
                rays: None,
            },
        ))
        .id();

    surrounding_rect_system(&mut world);

    let bounds = world.get::<LocalBounds>(frame).unwrap().0;
    assert_eq!(bounds, gaanim_math::Bounds3D::new_2d(7.0, 1.5, 49.0, 24.5));
    assert!(world.get::<PathSource>(frame).unwrap().0.elements().len() > 4);

    let right_entity = world
        .query::<(Entity, &gaanim_scene::MobjectId)>()
        .iter(&world)
        .find_map(|(entity, id)| (id.0 == right_id).then_some(entity))
        .unwrap();
    world
        .get_mut::<SpatialTransform>(right_entity)
        .unwrap()
        .translation
        .x += 20.0;
    surrounding_rect_system(&mut world);
    assert_eq!(world.get::<LocalBounds>(frame).unwrap().0.max.x, 59.0);
}

#[test]
fn line_frames_cover_each_line_of_a_turned_text() {
    let mut world = World::new();
    let placed =
        gaanim_core::kurbo::Affine::translate((3.0, 1.0)) * gaanim_core::kurbo::Affine::rotate(0.5);
    let text = world
        .spawn(SpatialTransform::new_2d(3.0, 1.0).with_rotation_2d(0.5))
        .id();
    // Two glyphs on the first line and one on the second, in the text's
    // own coordinates.
    let glyphs = [
        (0.0, 0.0, 0.2, 0.4),
        (0.3, 0.0, 0.5, 0.4),
        (0.0, -0.8, 0.2, -0.4),
    ];
    let ids: Vec<ObjectId> = glyphs
        .iter()
        .enumerate()
        .map(|(index, &(x0, y0, x1, y1))| {
            let id = ObjectId::from_parts(10 + index as u32, 1);
            world.spawn((
                gaanim_scene::MobjectId(id),
                SpatialTransform::default(),
                LocalBounds(gaanim_math::Bounds3D::new_2d(x0, y0, x1, y1)),
                ChildOf(text),
            ));
            id
        })
        .collect();
    let empty = Arc::new(BezPath::new());
    let holes = world
        .spawn((
            gaanim_scene::MobjectId(ObjectId::from_parts(2, 1)),
            SpatialTransform::default(),
            Path2D(empty.clone()),
            PathSource(empty),
            LocalBounds(gaanim_math::Bounds3D::default()),
            SurroundingRect::lines(ids, [0.1; 4], 0.0),
        ))
        .id();
    surrounding_rect_system(&mut world);

    // Back in the text's coordinates, each line has its own frame.
    let path = placed.inverse() * world.get::<PathSource>(holes).unwrap().0.as_ref().clone();
    let mut frames = Vec::new();
    for element in path.elements() {
        match element {
            gaanim_core::kurbo::PathEl::MoveTo(point) => {
                frames.push(gaanim_core::kurbo::Rect::from_points(*point, *point));
            }
            gaanim_core::kurbo::PathEl::LineTo(point) => {
                let frame = frames.last_mut().unwrap();
                *frame = frame.union_pt(*point);
            }
            _ => {}
        }
    }
    let expected = [
        gaanim_core::kurbo::Rect::new(-0.1, -0.1, 0.6, 0.5),
        gaanim_core::kurbo::Rect::new(-0.1, -0.9, 0.3, -0.3),
    ];
    assert_eq!(frames.len(), expected.len());
    for (frame, expected) in frames.iter().zip(expected) {
        for (value, expected) in [
            (frame.x0, expected.x0),
            (frame.y0, expected.y0),
            (frame.x1, expected.x1),
            (frame.y1, expected.y1),
        ] {
            assert!(
                (value - expected).abs() < 1e-9,
                "{frame:?} is not {expected:?}"
            );
        }
    }
    let drawn = world.get::<PathSource>(holes).unwrap().0.bounding_box();
    let bounds = world.get::<LocalBounds>(holes).unwrap().0;
    assert_eq!((bounds.min.x, bounds.max.y), (drawn.x0, drawn.y1));
}

#[test]
fn an_underline_is_one_line_under_the_padded_bounds() {
    let mut world = World::new();
    let id = ObjectId::from_parts(1, 1);
    world.spawn((
        gaanim_scene::MobjectId(id),
        SpatialTransform::new_2d(10.0, 20.0),
        LocalBounds(gaanim_math::Bounds3D::new_2d(-5.0, -2.0, 5.0, 2.0)),
    ));
    let empty = Arc::new(BezPath::new());
    let line = world
        .spawn((
            gaanim_scene::MobjectId(ObjectId::from_parts(2, 1)),
            SpatialTransform::default(),
            Path2D(empty.clone()),
            PathSource(empty),
            LocalBounds(gaanim_math::Bounds3D::default()),
            SurroundingRect::underline(vec![id], 1.0, 0.5),
        ))
        .id();
    surrounding_rect_system(&mut world);
    // The target spans x 5..15 and y 18..22: the line sits 1 below and
    // overhangs 0.5 on each side.
    let bounds = world.get::<LocalBounds>(line).unwrap().0;
    assert_eq!(bounds, gaanim_math::Bounds3D::new_2d(4.5, 17.0, 15.5, 17.0));
    let path = world.get::<PathSource>(line).unwrap().0.clone();
    assert_eq!(path.elements().len(), 2, "one segment: a move and a line");
    let drawn = path.bounding_box();
    assert_eq!(
        (drawn.x0, drawn.x1, drawn.y0, drawn.y1),
        (4.5, 15.5, 17.0, 17.0)
    );
}

#[test]
fn regenerated_paths_preserve_create_reveal_progress() {
    let mut world = World::new();
    let empty = Arc::new(BezPath::new());
    let entity = world
        .spawn((
            Path2D(empty.clone()),
            PathSource(empty),
            LocalBounds(gaanim_math::Bounds3D::default()),
            crate::writing::PathReveal(0.25),
        ))
        .id();
    let mut full_path = BezPath::new();
    full_path.move_to((0.0, 0.0));
    full_path.line_to((100.0, 0.0));

    write_path(&mut world, entity, full_path);

    let source = world.get::<PathSource>(entity).unwrap();
    let visible = world.get::<Path2D>(entity).unwrap();
    assert_eq!(source.0.bounding_box().x1, 100.0);
    assert!(
        visible.0.bounding_box().x1 < 30.0,
        "every regenerated path must keep the partial Create trim"
    );
}

fn spawn_accelerating_simulation(world: &mut World, fixed_dt: f64) -> Entity {
    let velocity = Arc::new(Mutex::new(0.0));
    let step_velocity = velocity.clone();
    let reset_velocity = velocity.clone();
    let updater = Updater::new_simulation(
        move |dt, _elapsed, entity, world| {
            let mut velocity = step_velocity.lock().unwrap();
            *velocity += dt;
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                transform.translation.x += *velocity * dt;
            }
            true
        },
        move |_entity, _world| {
            *reset_velocity.lock().unwrap() = 0.0;
            true
        },
        fixed_dt,
    )
    .unwrap();
    world.spawn((SpatialTransform::default(), updater)).id()
}

#[test]
fn seek_rebuilds_fixed_simulations_before_dependent_frame_updaters() {
    let mut world = World::new();
    let simulated = Arc::new(Mutex::new(0.0));
    let observed = Arc::new(Mutex::new(0.0));

    let observed_value = observed.clone();
    let simulated_for_observer = simulated.clone();
    world.spawn((
        SpatialTransform::default(),
        Updater::new(move |_dt, _elapsed, _entity, _world| {
            *observed_value.lock().unwrap() = *simulated_for_observer.lock().unwrap();
            true
        }),
    ));

    let simulated_step = simulated.clone();
    let simulated_reset = simulated.clone();
    world.spawn((
        SpatialTransform::default(),
        Updater::new_simulation(
            move |dt, _elapsed, _entity, _world| {
                *simulated_step.lock().unwrap() += dt;
                true
            },
            move |_entity, _world| {
                *simulated_reset.lock().unwrap() = 0.0;
                true
            },
            0.25,
        )
        .unwrap(),
    ));

    seek_updaters(&mut world, 1.0);
    assert_eq!(*simulated.lock().unwrap(), 1.0);
    assert_eq!(
        *observed.lock().unwrap(),
        1.0,
        "derived parameters must observe the reconstructed simulation state"
    );
}

fn x(world: &World, entity: Entity) -> f64 {
    world.get::<SpatialTransform>(entity).unwrap().translation.x
}

#[test]
fn fixed_step_simulation_is_independent_of_render_frame_partitioning() {
    let mut one_frame = World::new();
    let one_frame_entity = spawn_accelerating_simulation(&mut one_frame, 1.0 / 240.0);
    advance_updaters_by(&mut one_frame, 1.0 / 30.0);

    let mut four_frames = World::new();
    let four_frames_entity = spawn_accelerating_simulation(&mut four_frames, 1.0 / 240.0);
    for _ in 0..4 {
        advance_updaters_by(&mut four_frames, 1.0 / 120.0);
    }

    assert!((x(&one_frame, one_frame_entity) - x(&four_frames, four_frames_entity)).abs() < 1e-12);
}

#[test]
fn simulation_seek_resets_and_replays_external_state() {
    let mut world = World::new();
    let entity = spawn_accelerating_simulation(&mut world, 0.01);

    advance_updaters_by(&mut world, 1.0);
    let at_one_second = x(&world, entity);

    seek_updaters(&mut world, 0.5);
    let first_half_second = x(&world, entity);
    assert!(first_half_second < at_one_second);

    seek_updaters(&mut world, 1.0);
    assert!((x(&world, entity) - at_one_second).abs() < 1e-12);

    seek_updaters(&mut world, 0.5);
    assert!((x(&world, entity) - first_half_second).abs() < 1e-12);
}

#[test]
fn simulation_waits_for_its_authored_start_time() {
    let mut world = World::new();
    let entity = spawn_accelerating_simulation(&mut world, 0.01);
    let updater = world
        .get::<Updater>(entity)
        .unwrap()
        .clone()
        .starting_at(2.0);
    world.entity_mut(entity).insert(updater);

    seek_updaters(&mut world, 1.5);
    assert_eq!(x(&world, entity), 0.0);

    seek_updaters(&mut world, 2.5);
    let after_half_second = x(&world, entity);
    assert!(after_half_second > 0.0);

    seek_updaters(&mut world, 1.0);
    assert_eq!(x(&world, entity), 0.0);

    seek_updaters(&mut world, 2.5);
    assert!((x(&world, entity) - after_half_second).abs() < 1e-12);
}

/// A trace sampled sparsely along a circle draws the circle, not the
/// polygon of its samples: halfway between two samples it is on the arc.
#[test]
fn traced_paths_draw_smooth_curves_through_their_samples() {
    use gaanim_core::kurbo::ParamCurve;
    let samples: Vec<DVec3> = (0..=8)
        .map(|k| {
            let angle = k as f64 * std::f64::consts::FRAC_PI_4 / 2.0;
            DVec3::new(10.0 * angle.cos(), 10.0 * angle.sin(), 0.0)
        })
        .collect();
    let path = traced_path_geometry(&samples);
    let segments: Vec<_> = path.segments().collect();
    assert_eq!(segments.len(), 8);
    for (segment, pair) in segments.iter().zip(samples.windows(2)) {
        assert_eq!(segment.start().x, pair[0].x);
        assert_eq!(segment.end().y, pair[1].y);
        let middle = segment.eval(0.5).to_vec2().hypot();
        // The chord's midpoint is 0.19 inside the circle.
        assert!((middle - 10.0).abs() < 0.02, "{middle}");
    }
}

#[test]
fn traced_paths_keep_their_direction_where_a_sample_has_almost_expired() {
    use gaanim_core::kurbo::{CubicBez, PathSeg};
    let arc = |angle: f64| DVec3::new(10.0 * angle.cos(), 10.0 * angle.sin(), 0.0);
    let samples: Vec<DVec3> = (1..=6).map(|k| arc(k as f64 * 0.1)).collect();
    // The tail moves along the chord to the next sample as it expires
    // (expire_trace_samples), and the source slows down at the head.
    for (tail, head) in [(1e-3, 0.05), (1e-7, 0.05), (0.5, 1e-3), (0.5, 1e-7)] {
        let mut points = samples.clone();
        points[0] = samples[1].lerp(samples[0], tail);
        let last = points.len() - 1;
        points[last] = samples[last - 1].lerp(samples[last], head);
        let path = traced_path_geometry(&points);
        for segment in path.segments() {
            let PathSeg::Cubic(CubicBez { p0, p1, p2, p3 }) = segment else {
                panic!("{segment:?}");
            };
            let chord = p3 - p0;
            // A segment too short to give the stroke a direction is left out.
            assert!(chord.hypot() > 1e-5, "{tail} {head}: {segment:?}");
            // Each segment leaves and enters along its chord, without
            // turning back at the ends of the trace.
            for tangent in [p1 - p0, p3 - p2] {
                let cosine = tangent.dot(chord) / (tangent.hypot() * chord.hypot());
                assert!(cosine > 0.9, "{tail} {head}: {cosine} in {segment:?}");
            }
        }
    }
}

#[test]
fn traced_path_starts_at_its_cursor_and_expires_old_samples() {
    let mut world = World::new();
    world.insert_resource(PlaybackState::default());
    let source = world.spawn(SpatialTransform::default()).id();
    let traced_path = TracedPath::new(source, 0.1, None)
        .starting_at(1.0)
        .with_dissipating_time(Some(1.0));
    let trace = world
        .spawn((traced_path, Path2D(Arc::new(BezPath::new()))))
        .id();

    world.resource_mut::<PlaybackState>().current_time = 0.5;
    traced_path_system(&mut world);
    assert!(world.get::<TracedPath>(trace).unwrap().points.is_empty());

    world.resource_mut::<PlaybackState>().current_time = 1.0;
    traced_path_system(&mut world);
    assert_eq!(world.get::<TracedPath>(trace).unwrap().sample_times, [1.0]);

    world
        .get_mut::<SpatialTransform>(source)
        .unwrap()
        .translation
        .x = 1.0;
    world.resource_mut::<PlaybackState>().current_time = 1.5;
    traced_path_system(&mut world);

    world
        .get_mut::<SpatialTransform>(source)
        .unwrap()
        .translation
        .x = 2.0;
    world.resource_mut::<PlaybackState>().current_time = 2.1;
    traced_path_system(&mut world);

    // At 2.1 the sample of 1.0 has expired: the trace starts where the
    // source was at the cutoff, 1.1, a fifth of the way to the next one.
    let traced_path = world.get::<TracedPath>(trace).unwrap();
    assert_eq!(traced_path.sample_times, [1.1, 1.5, 2.1]);
    let xs: Vec<f64> = traced_path.points.iter().map(|point| point.x).collect();
    assert!((xs[0] - 0.2).abs() < 1e-9, "{xs:?}");
    assert_eq!(&xs[1..], &[1.0, 2.0]);
}

#[test]
fn traced_path_samples_the_source_in_the_trace_local_space() {
    let mut world = World::new();
    world.insert_resource(PlaybackState::default());

    let source_parent = world
        .spawn(
            SpatialTransform::new_2d(120.0, -35.0)
                .with_rotation_2d(std::f64::consts::FRAC_PI_2)
                .with_scale_2d(2.0, 3.0),
        )
        .id();
    let source = world
        .spawn(SpatialTransform::new_2d(10.0, 4.0))
        .set_parent_in_place(source_parent)
        .id();

    let trace_parent = world
        .spawn(SpatialTransform::new_2d(-40.0, 25.0).with_scale_2d(0.5, 2.0))
        .id();
    let trace = world
        .spawn((
            SpatialTransform::new_2d(6.0, -8.0),
            TracedPath::new(source, 0.0, None),
            Path2D(Arc::new(BezPath::new())),
        ))
        .set_parent_in_place(trace_parent)
        .id();

    traced_path_system(&mut world);

    let expected_world = entity_world_matrix(source, &world)
        .unwrap()
        .transform_point3(DVec3::ZERO);
    let expected_local = tracking_world_to_local(trace, expected_world, &world);
    let sampled = world.get::<TracedPath>(trace).unwrap().points[0];
    assert!(sampled.distance(expected_local) < 1e-9);

    let path = world.get::<Path2D>(trace).unwrap();
    assert!(matches!(
        path.0.elements().first(),
        Some(gaanim_core::kurbo::PathEl::MoveTo(point))
            if DVec3::new(point.x, point.y, 0.0).distance(expected_local) < 1e-9
    ));
}

#[test]
fn simulation_rejects_invalid_fixed_steps() {
    for fixed_dt in [0.0, -0.01, f64::NAN, f64::INFINITY] {
        let result = Updater::new_simulation(
            |_dt, _elapsed, _entity, _world| true,
            |_entity, _world| true,
            fixed_dt,
        );
        assert_eq!(result.unwrap_err(), InvalidFixedStep);
    }
}

#[test]
fn anchored_endpoint_uses_current_nested_rotation_and_scale() {
    let mut world = World::new();
    let parent_transform = SpatialTransform::new_2d(10.0, 20.0)
        .with_rotation_2d(std::f64::consts::FRAC_PI_2)
        .scale_uniform(2.0);
    let parent = world.spawn(parent_transform).id();
    let child = world
        .spawn((
            SpatialTransform::new_2d(5.0, 0.0),
            LocalBounds(gaanim_math::Bounds3D::new_2d(-2.0, -1.0, 2.0, 1.0)),
            ChildOf(parent),
        ))
        .id();
    let endpoint = TrackingEndpoint::EntityAnchor {
        entity: child,
        normalized: DVec3::new(1.0, 1.0, 0.0),
        offset: DVec3::new(1.0, 0.0, 0.0),
    };
    let expected = parent_transform.to_mat4().transform_point3(
        SpatialTransform::new_2d(5.0, 0.0)
            .to_mat4()
            .transform_point3(DVec3::new(3.0, 1.0, 0.0)),
    );
    let actual = resolve_tracking_endpoint(&endpoint, &world).unwrap();
    assert!(actual.distance(expected) < 1e-9);

    world
        .get_mut::<SpatialTransform>(parent)
        .unwrap()
        .translation
        .x += 7.0;
    let moved = resolve_tracking_endpoint(&endpoint, &world).unwrap();
    assert!((moved.x - actual.x - 7.0).abs() < 1e-9);
}

#[test]
fn entity_bounds_include_renderable_descendants_of_structural_roots() {
    let mut world = World::new();
    let root = world.spawn(SpatialTransform::new_2d(10.0, 20.0)).id();
    world.spawn((
        SpatialTransform::new_2d(5.0, -3.0),
        LocalBounds(gaanim_math::Bounds3D::new_2d(-2.0, -1.0, 2.0, 1.0)),
        ChildOf(root),
    ));

    let bounds = resolve_entity_bounds(root, &world).expect("descendant bounds");
    assert!((bounds.min.x - 13.0).abs() < 1e-9);
    assert!((bounds.max.x - 17.0).abs() < 1e-9);
    assert!((bounds.min.y - 16.0).abs() < 1e-9);
    assert!((bounds.max.y - 18.0).abs() < 1e-9);
}

#[test]
fn entity_bounds_fall_back_to_propagated_structural_bounds() {
    let mut world = World::new();
    let expected = gaanim_math::Bounds3D::new_2d(-345.0, -95.0, -175.0, 75.0);
    let root = world.spawn(WorldBounds(expected)).id();

    assert_eq!(resolve_entity_bounds(root, &world), Some(expected));
}

#[test]
fn endpoint_distance_updates_signal_with_scale() {
    let mut world = World::new();
    let signal = world
        .spawn((
            crate::signals::FloatSignal::new(0.0),
            EndpointDistance {
                from: TrackingEndpoint::Static(DVec3::new(0.0, 0.0, 0.0)),
                to: TrackingEndpoint::Static(DVec3::new(3.0, 4.0, 8.0)),
                scale: 2.0,
            },
        ))
        .id();
    endpoint_distance_system(&mut world);
    assert_eq!(
        world
            .get::<crate::signals::FloatSignal>(signal)
            .unwrap()
            .value,
        10.0
    );
}

#[test]
fn dimension_side_ignores_the_direction_of_the_measured_segment() {
    let (bottom, top) = (DVec2::new(0.0, 0.0), DVec2::new(0.0, 4.0));
    let (left, right) = (DVec2::new(-2.0, 1.0), DVec2::new(2.0, 1.0));
    for (from, to) in [(bottom, top), (top, bottom)] {
        // The normal is (-dy, dx): a positive offset leaves from->to on its left.
        let normal_x = -(to - from).y.signum();
        let left_offset = DimensionSide::Left.signed_offset(0.5, from, to);
        let right_offset = DimensionSide::Right.signed_offset(-0.5, from, to);
        assert_eq!(left_offset * normal_x, -0.5);
        assert_eq!(right_offset * normal_x, 0.5);
    }
    for (from, to) in [(left, right), (right, left)] {
        let normal_y = (to - from).x.signum();
        assert_eq!(
            DimensionSide::Above.signed_offset(0.5, from, to) * normal_y,
            0.5
        );
        assert_eq!(
            DimensionSide::Below.signed_offset(0.5, from, to) * normal_y,
            -0.5
        );
    }
    // Along the requested direction no side is preferred.
    assert_eq!(DimensionSide::Above.signed_offset(-0.5, bottom, top), 0.5);
    assert_eq!(DimensionSide::resolve(None, -0.5, bottom, top), -0.5);
}

#[test]
fn dimension_label_follows_its_side_when_endpoints_swap() {
    let mut world = World::new();
    let label = world.spawn(SpatialTransform::default()).id();
    let placement = world
        .spawn(DimensionLabelPlacement {
            label,
            from: TrackingEndpoint::Static(DVec3::new(0.0, 0.0, 0.0)),
            to: TrackingEndpoint::Static(DVec3::new(0.0, 4.0, 0.0)),
            offset: 1.0,
            side: Some(DimensionSide::Right),
            gap: 0.5,
            orientation: DimensionLabelOrientation::Aligned,
            clear_label_width: false,
        })
        .id();
    dimension_label_placement_system(&mut world);
    let first = world.get::<SpatialTransform>(label).unwrap().translation;
    assert!((first.x - 1.5).abs() < 1e-12, "{first:?}");

    let mut placement = world.get_mut::<DimensionLabelPlacement>(placement).unwrap();
    let placement = &mut *placement;
    std::mem::swap(&mut placement.from, &mut placement.to);
    dimension_label_placement_system(&mut world);
    let swapped = world.get::<SpatialTransform>(label).unwrap().translation;
    assert!((swapped.x - 1.5).abs() < 1e-12, "{swapped:?}");
}

#[test]
fn zero_distance_and_negative_dimension_offset_are_stable() {
    let mut world = World::new();
    let point = DVec3::new(7.0, -3.0, 11.0);
    let signal = world
        .spawn((
            crate::signals::FloatSignal::new(123.0),
            EndpointDistance {
                from: TrackingEndpoint::Static(point),
                to: TrackingEndpoint::Static(point),
                scale: 0.5,
            },
        ))
        .id();
    endpoint_distance_system(&mut world);
    assert_eq!(
        world
            .get::<crate::signals::FloatSignal>(signal)
            .unwrap()
            .value,
        0.0
    );

    let label = world.spawn(SpatialTransform::default()).id();
    world.spawn(DimensionLabelPlacement {
        label,
        from: TrackingEndpoint::Static(DVec3::new(0.0, 0.0, 0.0)),
        to: TrackingEndpoint::Static(DVec3::new(10.0, 0.0, 0.0)),
        offset: -20.0,
        side: None,
        gap: 5.0,
        orientation: DimensionLabelOrientation::Upright,
        clear_label_width: true,
    });
    dimension_label_placement_system(&mut world);
    let transform = world.get::<SpatialTransform>(label).unwrap();
    assert!((transform.translation.x - 5.0).abs() < 1e-9);
    assert!((transform.translation.y + 25.0).abs() < 1e-9);
    assert!(transform.z_angle().abs() < 1e-9);
}

#[test]
fn upright_dimension_label_clears_steep_lines_by_its_width() {
    let bounds = gaanim_math::Bounds3D::new(DVec3::new(-0.5, -0.1, 0.0), DVec3::new(0.5, 0.1, 0.0));
    let mut world = World::new();
    let vertical = world
        .spawn((SpatialTransform::default(), LocalBounds(bounds)))
        .id();
    let horizontal = world
        .spawn((SpatialTransform::default(), LocalBounds(bounds)))
        .id();
    // Force labels reuse this placement without the width clearance.
    let force = world
        .spawn((SpatialTransform::default(), LocalBounds(bounds)))
        .id();
    for (label, to, clear_label_width) in [
        (vertical, DVec3::new(0.0, 2.0, 0.0), true),
        (horizontal, DVec3::new(2.0, 0.0, 0.0), true),
        (force, DVec3::new(0.0, 2.0, 0.0), false),
    ] {
        world.spawn(DimensionLabelPlacement {
            label,
            from: TrackingEndpoint::Static(DVec3::ZERO),
            to: TrackingEndpoint::Static(to),
            offset: 0.35,
            side: None,
            gap: 0.1,
            orientation: DimensionLabelOrientation::Upright,
            clear_label_width,
        });
    }
    dimension_label_placement_system(&mut world);

    // A bottom-to-top line puts positive offsets on its left. `gap`
    // separates the line from the label's nearest edge: its half width
    // on a steep line, its half height on a flat one.
    let vertical = world.get::<SpatialTransform>(vertical).unwrap().translation;
    assert!((vertical.x + (0.35 + 0.1 + 0.5)).abs() < 1e-9);
    assert!((vertical.y - 1.0).abs() < 1e-9);
    let horizontal = world
        .get::<SpatialTransform>(horizontal)
        .unwrap()
        .translation;
    assert!((horizontal.x - 1.0).abs() < 1e-9);
    assert!((horizontal.y - (0.35 + 0.1 + 0.1)).abs() < 1e-9);
    let force = world.get::<SpatialTransform>(force).unwrap().translation;
    assert!((force.x + 0.45).abs() < 1e-9);
}

#[test]
fn dimension_labels_clear_the_line_by_the_gap_from_their_box() {
    // A label box that is off-center from its origin, as text sitting on
    // its baseline is.
    let bounds =
        gaanim_math::Bounds3D::new(DVec3::new(-0.5, -0.05, 0.0), DVec3::new(0.5, 0.25, 0.0));
    let mut world = World::new();
    let upright = world
        .spawn((SpatialTransform::default(), LocalBounds(bounds)))
        .id();
    let aligned = world
        .spawn((SpatialTransform::default(), LocalBounds(bounds)))
        .id();
    for (label, orientation, to) in [
        (
            upright,
            DimensionLabelOrientation::Upright,
            DVec3::new(2.0, 0.0, 0.0),
        ),
        (
            aligned,
            DimensionLabelOrientation::Aligned,
            DVec3::new(0.0, 2.0, 0.0),
        ),
    ] {
        world.spawn(DimensionLabelPlacement {
            label,
            from: TrackingEndpoint::Static(DVec3::ZERO),
            to: TrackingEndpoint::Static(to),
            offset: 0.3,
            side: None,
            gap: 0.1,
            orientation,
            clear_label_width: true,
        });
    }
    dimension_label_placement_system(&mut world);
    let world_box = |world: &World, label: Entity| {
        bounds.transform_mat4(&world.get::<SpatialTransform>(label).unwrap().to_mat4())
    };
    let upright = world_box(&world, upright);
    assert!((upright.min.y - 0.4).abs() < 1e-9, "{upright:?}");
    assert!((upright.center().x - 1.0).abs() < 1e-9, "{upright:?}");
    // Along a bottom-to-top line the label turns a quarter and sits on
    // its left, clear of the line by the gap.
    let aligned = world_box(&world, aligned);
    assert!((aligned.max.x + 0.4).abs() < 1e-9, "{aligned:?}");
    assert!((aligned.center().y - 1.0).abs() < 1e-9, "{aligned:?}");
}

#[test]
fn aligned_dimension_label_stays_readable() {
    let mut world = World::new();
    let label = world.spawn(SpatialTransform::default()).id();
    world.spawn(DimensionLabelPlacement {
        label,
        from: TrackingEndpoint::Static(DVec3::new(10.0, 0.0, 0.0)),
        to: TrackingEndpoint::Static(DVec3::new(-10.0, 0.0, 0.0)),
        offset: 20.0,
        side: None,
        gap: 5.0,
        orientation: DimensionLabelOrientation::Aligned,
        clear_label_width: true,
    });
    dimension_label_placement_system(&mut world);
    let transform = world.get::<SpatialTransform>(label).unwrap();
    assert!((transform.translation.y + 25.0).abs() < 1e-9);
    assert!(transform.z_angle().abs() < 1e-9);
}

#[test]
fn rim_endpoints_leave_a_shape_where_its_frame_puts_its_edge() {
    let mut world = World::new();
    let parent = world
        .spawn(SpatialTransform::new_2d(1.0, 1.0).with_scale_2d(2.0, 2.0))
        .id();
    let node = world
        .spawn((
            SpatialTransform::new_2d(1.0, 0.0).with_scale_2d(1.0, 0.5),
            ChildOf(parent),
        ))
        .id();
    let rim = |toward: DVec3, world: &World| {
        resolve_tracking_endpoint(
            &TrackingEndpoint::Rim {
                entity: node,
                toward: Box::new(TrackingEndpoint::Static(toward)),
                radius: 0.5,
            },
            world,
        )
        .unwrap()
    };
    // The node's origin is at (3, 1), and the frames stretch its circle
    // of 0.5 to reach 1 to the side and 0.5 up.
    let right = rim(DVec3::new(10.0, 1.0, 0.0), &world);
    assert!(right.distance(DVec3::new(4.0, 1.0, 0.0)) < 1e-9, "{right}");
    let up = rim(DVec3::new(3.0, 9.0, 0.0), &world);
    assert!(up.distance(DVec3::new(3.0, 1.5, 0.0)) < 1e-9, "{up}");
    // A target at the origin gives the origin.
    let center = rim(DVec3::new(3.0, 1.0, 0.0), &world);
    assert!(
        center.distance(DVec3::new(3.0, 1.0, 0.0)) < 1e-9,
        "{center}"
    );
}

#[test]
fn expression_between_and_polar_endpoints_are_reactive() {
    let mut world = World::new();
    let parameter_id = ObjectId::from_raw(42);
    let parameter = world
        .spawn(crate::signals::FloatSignal::new(
            std::f64::consts::FRAC_PI_2,
        ))
        .id();
    let scalar = TrackingScalar {
        source: crate::reactive::ScalarSource::signal(parameter_id),
        parameters: vec![(parameter_id, parameter)],
    };
    let polar = TrackingEndpoint::Polar {
        origin: Box::new(TrackingEndpoint::Static(DVec3::new(10.0, 20.0, 0.0))),
        radius: TrackingScalar {
            source: crate::reactive::ScalarSource::constant(5.0),
            parameters: Vec::new(),
        },
        angle: scalar,
    };
    let point = resolve_tracking_endpoint(&polar, &world).unwrap();
    assert!(point.distance(DVec3::new(10.0, 25.0, 0.0)) < 1e-9);

    let midpoint = TrackingEndpoint::Between {
        from: Box::new(TrackingEndpoint::Static(DVec3::ZERO)),
        to: Box::new(polar),
        alpha: 0.5.into(),
        offset: DVec3::new(1.0, -1.0, 0.0),
    };
    let point = resolve_tracking_endpoint(&midpoint, &world).unwrap();
    assert!(point.distance(DVec3::new(6.0, 11.5, 0.0)) < 1e-9);

    let offset = TrackingEndpoint::Offset {
        origin: Box::new(TrackingEndpoint::Static(DVec3::new(-2.0, 8.0, 0.0))),
        dx: TrackingScalar {
            source: crate::reactive::ScalarSource::constant(4.0),
            parameters: Vec::new(),
        },
        dy: TrackingScalar {
            source: crate::reactive::ScalarSource::signal(parameter_id),
            parameters: vec![(parameter_id, parameter)],
        },
    };
    let point = resolve_tracking_endpoint(&offset, &world).unwrap();
    assert!(point.distance(DVec3::new(2.0, 8.0 + std::f64::consts::FRAC_PI_2, 0.0,)) < 1e-9);
}

#[test]
fn angle_signal_supports_minor_and_major_sweeps() {
    let mut world = World::new();
    let minor = world
        .spawn((
            crate::signals::FloatSignal::new(0.0),
            EndpointAngle {
                vertex: TrackingEndpoint::Static(DVec3::ZERO),
                from: TrackingRay::Direction(DVec3::X),
                to: TrackingRay::Direction(DVec3::Y),
                sweep: AngleSweep::Minor,
                scale: 180.0 / std::f64::consts::PI,
            },
        ))
        .id();
    let major = world
        .spawn((
            crate::signals::FloatSignal::new(0.0),
            EndpointAngle {
                vertex: TrackingEndpoint::Static(DVec3::ZERO),
                from: TrackingRay::Direction(DVec3::X),
                to: TrackingRay::Direction(DVec3::Y),
                sweep: AngleSweep::Major,
                scale: 180.0 / std::f64::consts::PI,
            },
        ))
        .id();
    endpoint_angle_system(&mut world);
    assert!(
        (world
            .get::<crate::signals::FloatSignal>(minor)
            .unwrap()
            .value
            - 90.0)
            .abs()
            < 1e-9
    );
    assert!(
        (world
            .get::<crate::signals::FloatSignal>(major)
            .unwrap()
            .value
            - 270.0)
            .abs()
            < 1e-9
    );
}

#[test]
fn local_expression_endpoint_respects_space_transform() {
    let mut world = World::new();
    let space = world
        .spawn(
            SpatialTransform::new_2d(10.0, 20.0)
                .with_scale_2d(2.0, 3.0)
                .with_rotation_2d(std::f64::consts::FRAC_PI_2),
        )
        .id();
    let scalar = |value| TrackingScalar {
        source: crate::reactive::ScalarSource::constant(value),
        parameters: Vec::new(),
    };
    let endpoint = TrackingEndpoint::LocalExpression {
        space,
        x: scalar(4.0),
        y: scalar(5.0),
        z: scalar(0.0),
    };

    let resolved = resolve_tracking_endpoint(&endpoint, &world).unwrap();
    assert!(resolved.distance(DVec3::new(-5.0, 28.0, 0.0)) < 1e-9);
}

#[test]
fn chained_rotation_bindings_resolve_in_one_pass() {
    let mut world = World::new();
    let motor = world
        .spawn(SpatialTransform::default().with_rotation_2d(0.3))
        .id();
    // The train runs motor -> w3 -> w2 -> w1 -> w0: every wheel was
    // spawned before its source, so one pass in entity order would read
    // stale sources.
    let wheels: Vec<Entity> = (0..4)
        .map(|_| world.spawn(SpatialTransform::default()).id())
        .collect();
    for (index, &wheel) in wheels.iter().enumerate() {
        let source = wheels.get(index + 1).copied().unwrap_or(motor);
        world.entity_mut(wheel).insert(RotationBinding {
            source,
            ratio: -0.5,
            phase: 0.1,
        });
    }
    mechanism_binding_system(&mut world);
    let mut expected = 0.3;
    for &wheel in wheels.iter().rev() {
        expected = expected * -0.5 + 0.1;
        let angle = turned_angle(wheel, &world, 0.0, &HashMap::new()).unwrap();
        assert!((angle - expected).abs() < 1e-9, "{angle} != {expected}");
    }
}

/// A spinning `Updater.rotate` turns what copies its rotation, and a
/// rack follows a Parameter-driven wheel past half a turn.
#[test]
fn rotation_bindings_read_spins_and_whole_turns() {
    let mut world = World::new();
    world.insert_resource(PlaybackState {
        current_time: 2.0,
        ..Default::default()
    });
    let mut motion = crate::procedural::ProceduralMotion::default();
    motion.push(
        crate::procedural::ProceduralLayer::Spin { speed: 0.5.into() },
        0.0,
    );
    let spinner = world.spawn((SpatialTransform::default(), motion)).id();
    let copy = world
        .spawn((
            SpatialTransform::default(),
            RotationBinding {
                source: spinner,
                ratio: 1.0,
                phase: 0.0,
            },
        ))
        .id();
    mechanism_binding_system(&mut world);
    let rotation = world.get::<SpatialTransform>(copy).unwrap().rotation;
    assert!((2.0 * rotation.z.atan2(rotation.w) - 1.0).abs() < 1e-9);

    let wheel = world.spawn(crate::signals::FloatSignal::new(0.0)).id();
    let turned = world.spawn(SpatialTransform::default()).id();
    let parameter = gaanim_core::ObjectId::from_raw(7);
    world.spawn(crate::PropertyBinding {
        target: turned,
        source: crate::ResolvedPropertySources {
            sources: crate::PropertySources::Rotation([
                0.0.into(),
                0.0.into(),
                crate::reactive::ScalarSource::signal(parameter),
            ]),
            parameters: vec![crate::PropertyParameter {
                logical: parameter,
                native: parameter,
                entity: wheel,
                initial: 0.0,
            }],
            anchor_offset: DVec3::ZERO,
            local_anchor: None,
        },
        start: 0.0,
        end: None,
        fallback: crate::PropertyValue::Rotation(DQuat::IDENTITY),
    });
    let rack = world
        .spawn((
            SpatialTransform::default(),
            RotationTranslationBinding {
                source: turned,
                axis: DVec3::X,
                scale: 1.0,
                base_position: None,
                base_angle: None,
            },
        ))
        .id();
    mechanism_binding_system(&mut world);
    world
        .get_mut::<crate::signals::FloatSignal>(wheel)
        .unwrap()
        .value = 3.0 * std::f64::consts::PI;
    mechanism_binding_system(&mut world);
    let x = world.get::<SpatialTransform>(rack).unwrap().translation.x;
    assert!((x - 3.0 * std::f64::consts::PI).abs() < 1e-9, "{x}");
}

#[test]
fn endpoint_follow_starts_at_its_cursor() {
    let mut world = World::new();
    world.insert_resource(PlaybackState {
        current_time: 1.0,
        ..Default::default()
    });
    let target = world
        .spawn((
            SpatialTransform::new_2d(-1.0, 0.0),
            EndpointFollow {
                endpoint: TrackingEndpoint::Static(DVec3::new(4.0, 2.0, 0.0)),
                offset: DVec3::ZERO,
                offset_space: FollowOffsetSpace::World,
                since: 2.0,
            },
        ))
        .id();
    endpoint_follow_system(&mut world);
    let at = |world: &World| world.get::<SpatialTransform>(target).unwrap().translation;
    assert_eq!(
        at(&world),
        DVec3::new(-1.0, 0.0, 0.0),
        "held before its cursor"
    );
    world.resource_mut::<PlaybackState>().current_time = 2.0;
    endpoint_follow_system(&mut world);
    assert_eq!(at(&world), DVec3::new(4.0, 2.0, 0.0));
}

#[test]
fn screen_followers_stay_where_the_resolved_camera_draws_their_point() {
    let mut world = World::new();
    world.insert_resource(PlaybackState::default());
    let mut camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1920, 1080);
    camera.set_perspective(0.8, 0.1, 100.0).unwrap();
    camera
        .look_at(DVec3::new(6.0, 4.0, 8.0), DVec3::ZERO, DVec3::Y)
        .unwrap();
    world.insert_resource(gaanim_math::ResolvedCamera {
        camera,
        viewport: Default::default(),
    });
    let point = DVec3::new(1.0, 1.0, 0.0);
    let screen = TrackingEndpoint::Screen(Box::new(TrackingEndpoint::Static(point)));
    let label = world
        .spawn((
            SpatialTransform::default(),
            EndpointFollow {
                endpoint: TrackingEndpoint::Offset {
                    origin: Box::new(screen),
                    dx: 0.0.into(),
                    dy: 0.5.into(),
                },
                offset: DVec3::ZERO,
                offset_space: FollowOffsetSpace::World,
                since: 0.0,
            },
        ))
        .id();
    let plain = world
        .spawn((
            SpatialTransform::default(),
            EndpointFollow {
                endpoint: TrackingEndpoint::Static(point),
                offset: DVec3::ZERO,
                offset_space: FollowOffsetSpace::World,
                since: 0.0,
            },
        ))
        .id();
    screen_follow_system(&mut world);
    let at = |entity| world.get::<SpatialTransform>(entity).unwrap().translation;
    let expected = camera.hud_point(point) + gaanim_core::glam::DVec2::new(0.0, 0.5);
    assert!((at(label).truncate() - expected).length() < 1e-9);
    assert!(expected.distance(point.truncate()) > 0.1);
    // Followers of points in the scene are left to `endpoint_follow_system`.
    assert_eq!(at(plain), DVec3::ZERO);
}

#[test]
fn endpoint_follow_converts_world_position_into_parent_space() {
    let mut world = World::new();
    let parent = world.spawn(SpatialTransform::new_2d(10.0, 20.0)).id();
    let target = world
        .spawn((
            SpatialTransform::default(),
            EndpointFollow {
                endpoint: TrackingEndpoint::Static(DVec3::new(30.0, 50.0, 0.0)),
                offset: DVec3::new(2.0, -3.0, 0.0),
                offset_space: FollowOffsetSpace::World,
                since: 0.0,
            },
        ))
        .id();
    world.entity_mut(target).set_parent_in_place(parent);
    endpoint_follow_system(&mut world);
    let transform = world.get::<SpatialTransform>(target).unwrap();
    assert!(transform.translation.distance(DVec3::new(22.0, 27.0, 0.0)) < 1e-9);
}

#[test]
fn position_bindings_read_a_source_positioned_by_follow() {
    let mut world = World::new();
    let source = world
        .spawn((
            SpatialTransform::default(),
            EndpointFollow {
                endpoint: TrackingEndpoint::Static(DVec3::new(3.0, 4.0, 0.0)),
                offset: DVec3::ZERO,
                offset_space: FollowOffsetSpace::World,
                since: 0.0,
            },
        ))
        .id();
    let bound = world
        .spawn((
            SpatialTransform::new_2d(-1.0, 0.0),
            crate::signals::PositionBinding::new(source, crate::signals::AxisMask::Y),
        ))
        .id();
    // A target that follows an endpoint itself keeps the followed position.
    let follower = world
        .spawn((
            SpatialTransform::default(),
            crate::signals::PositionBinding::new(source, crate::signals::AxisMask::XY),
            EndpointFollow {
                endpoint: TrackingEndpoint::Static(DVec3::new(-5.0, 6.0, 0.0)),
                offset: DVec3::ZERO,
                offset_space: FollowOffsetSpace::World,
                since: 0.0,
            },
        ))
        .id();

    evaluate_reactive_positions(&mut world, 0.0);

    let at = |entity| world.get::<SpatialTransform>(entity).unwrap().translation;
    assert!(at(bound).distance(DVec3::new(-1.0, 4.0, 0.0)) < 1e-9);
    assert!(at(follower).distance(DVec3::new(-5.0, 6.0, 0.0)) < 1e-9);
}

#[test]
fn tracking_line_respects_active_trim_window() {
    let mut world = World::new();
    let empty = Arc::new(BezPath::new());
    let line = world
        .spawn((
            SpatialTransform::default(),
            Path2D(empty.clone()),
            PathSource(empty),
            LocalBounds(gaanim_math::Bounds3D::default()),
            crate::writing::PathTrimWindow {
                start: 0.0,
                end: 0.0,
                offset: 0.0,
                sequential: false,
            },
            TrackingLine::new(
                TrackingEndpoint::Static(DVec3::ZERO),
                TrackingEndpoint::Static(DVec3::new(100.0, 0.0, 0.0)),
            ),
        ))
        .id();

    tracking_line_system(&mut world);
    assert!(
        world
            .get::<Path2D>(line)
            .expect("tracking path")
            .0
            .is_empty()
    );

    world
        .get_mut::<crate::writing::PathTrimWindow>(line)
        .expect("trim window")
        .end = 0.25;
    tracking_line_system(&mut world);
    let visible = world.get::<Path2D>(line).expect("trimmed tracking path");
    assert!(matches!(
        visible.0.elements().last(),
        Some(gaanim_core::kurbo::PathEl::LineTo(point)) if (point.x - 25.0).abs() < 1e-9
    ));
}

#[test]
fn tracking_line_respects_active_path_reveal() {
    let mut world = World::new();
    let empty = Arc::new(BezPath::new());
    let line = world
        .spawn((
            SpatialTransform::default(),
            Path2D(empty.clone()),
            PathSource(empty),
            LocalBounds(gaanim_math::Bounds3D::default()),
            crate::writing::PathReveal(0.5),
            TrackingLine::new(
                TrackingEndpoint::Static(DVec3::ZERO),
                TrackingEndpoint::Static(DVec3::new(100.0, 0.0, 0.0)),
            ),
        ))
        .id();

    tracking_line_system(&mut world);

    let source = world.get::<PathSource>(line).expect("full tracking path");
    assert!(matches!(
        source.0.elements().last(),
        Some(gaanim_core::kurbo::PathEl::LineTo(point)) if (point.x - 100.0).abs() < 1e-9
    ));
    let visible = world.get::<Path2D>(line).expect("revealed tracking path");
    assert!(matches!(
        visible.0.elements().last(),
        Some(gaanim_core::kurbo::PathEl::LineTo(point)) if (point.x - 50.0).abs() < 1e-9
    ));
}

fn sampled_world() -> World {
    let mut world = World::new();
    world.insert_resource(PlaybackState::default());
    world
}

#[test]
fn sampled_series_rejects_invalid_input() {
    assert!(
        SampledSeriesDriver::new(
            vec![],
            vec![],
            SampledProperty::TranslateX,
            SampledInterpolation::Linear,
            1.0,
            0.0
        )
        .is_err()
    );
    assert!(
        SampledSeriesDriver::new(
            vec![0.0, 1.0],
            vec![1.0],
            SampledProperty::TranslateX,
            SampledInterpolation::Linear,
            1.0,
            0.0
        )
        .is_err()
    );
    assert!(
        SampledSeriesDriver::new(
            vec![1.0, 0.0],
            vec![1.0, 2.0],
            SampledProperty::TranslateX,
            SampledInterpolation::Linear,
            1.0,
            0.0
        )
        .is_err()
    );
    assert!(
        SampledSeriesDriver::new(
            vec![0.0, f64::NAN],
            vec![1.0, 2.0],
            SampledProperty::TranslateX,
            SampledInterpolation::Linear,
            1.0,
            0.0
        )
        .is_err()
    );
}

#[test]
fn sampled_series_translation_is_relative_to_lazy_base() {
    let mut world = sampled_world();
    let entity = world
        .spawn(SpatialTransform::new_2d(-40.0, 10.0))
        .insert(SampledSeriesDrivers::from(
            SampledSeriesDriver::new(
                vec![0.0, 1.0, 2.0],
                vec![0.0, 4.0, -2.0],
                SampledProperty::TranslateX,
                SampledInterpolation::Linear,
                10.0,
                0.0,
            )
            .unwrap()
            .starting_at(1.0),
        ))
        .id();

    // Antes de start_at la serie no escribe: queda la pose declarada.
    world.resource_mut::<PlaybackState>().current_time = 0.5;
    sampled_series_system(&mut world);
    assert_eq!(
        world.get::<SpatialTransform>(entity).unwrap().translation,
        DVec3::new(-40.0, 10.0, 0.0)
    );

    // elapsed = 1.5 interpola entre 4.0 y -2.0 → 1.0.
    world.resource_mut::<PlaybackState>().current_time = 2.5;
    sampled_series_system(&mut world);
    assert_eq!(
        world.get::<SpatialTransform>(entity).unwrap().translation.x,
        -40.0 + 10.0
    );

    // Fuera de rango: clamp al último valor (-2.0) aunque el tiempo siga.
    world.resource_mut::<PlaybackState>().current_time = 9.0;
    sampled_series_system(&mut world);
    assert_eq!(
        world.get::<SpatialTransform>(entity).unwrap().translation.x,
        -40.0 - 20.0
    );
}

#[test]
fn sampled_series_channels_on_different_axes_coexist() {
    let series = |property| {
        SampledSeriesDriver::new(
            vec![0.0, 1.0],
            vec![0.0, 1.0],
            property,
            SampledInterpolation::Linear,
            10.0,
            0.0,
        )
        .unwrap()
    };
    let mut drivers = SampledSeriesDrivers::from(series(SampledProperty::TranslateX));
    drivers.attach(series(SampledProperty::TranslateY).starting_at(2.0));
    drivers.attach(series(SampledProperty::TranslateX));
    assert_eq!(
        drivers.0.len(),
        2,
        "same property replaces, new property adds"
    );

    // Un RemoveUpdater a t=1 solo congela los canales que ya corrían.
    drivers.stop_at(1.0);
    assert_eq!(
        drivers.get(SampledProperty::TranslateX).unwrap().stop_at,
        Some(1.0)
    );
    assert_eq!(
        drivers.get(SampledProperty::TranslateY).unwrap().stop_at,
        None
    );
    drivers
        .0
        .iter_mut()
        .for_each(|driver| driver.stop_at = None);

    let mut world = sampled_world();
    let entity = world
        .spawn((SpatialTransform::new_2d(1.0, 2.0), drivers))
        .id();
    world.resource_mut::<PlaybackState>().current_time = 2.5;
    sampled_series_system(&mut world);
    assert_eq!(
        world.get::<SpatialTransform>(entity).unwrap().translation,
        DVec3::new(11.0, 7.0, 0.0)
    );
}

#[test]
fn sampled_series_leaves_the_channel_alone_before_its_start() {
    // A fade_in wrote 0.25 at t = 0.5; the series only starts at t = 2.
    let mut world = sampled_world();
    let entity = world
        .spawn(gaanim_scene::Opacity(0.25))
        .insert(SampledSeriesDrivers::from(
            SampledSeriesDriver::new(
                vec![0.0, 1.0],
                vec![1.0, 0.0],
                SampledProperty::Opacity,
                SampledInterpolation::Linear,
                1.0,
                0.0,
            )
            .unwrap()
            .starting_at(2.0),
        ))
        .id();

    world.resource_mut::<PlaybackState>().current_time = 0.5;
    sampled_series_system(&mut world);
    assert_eq!(world.get::<gaanim_scene::Opacity>(entity).unwrap().0, 0.25);

    world.resource_mut::<PlaybackState>().current_time = 2.5;
    sampled_series_system(&mut world);
    assert_eq!(world.get::<gaanim_scene::Opacity>(entity).unwrap().0, 0.5);
}

#[test]
fn sampled_series_on_the_same_channel_takes_over_from_its_start() {
    let series = |values: Vec<f64>| {
        SampledSeriesDriver::new(
            vec![0.0, 1.0],
            values,
            SampledProperty::TranslateX,
            SampledInterpolation::Linear,
            1.0,
            0.0,
        )
        .unwrap()
    };
    let mut drivers = SampledSeriesDrivers::from(series(vec![0.0, 1.0]));
    drivers.attach(series(vec![5.0, 6.0]).starting_at(4.0));
    assert_eq!(drivers.0.len(), 2, "a later series keeps the earlier one");
    drivers.attach(series(vec![7.0, 8.0]).starting_at(4.0));
    assert_eq!(drivers.0.len(), 2, "a series at the same cursor replaces");

    let mut world = sampled_world();
    let entity = world.spawn((SpatialTransform::default(), drivers)).id();
    let x_at = |world: &mut World, time: f64| {
        world.resource_mut::<PlaybackState>().current_time = time;
        sampled_series_system(world);
        world.get::<SpatialTransform>(entity).unwrap().translation.x
    };
    assert_eq!(x_at(&mut world, 0.5), 0.5);
    assert_eq!(x_at(&mut world, 3.0), 1.0);
    assert_eq!(x_at(&mut world, 4.5), 7.5);
    assert_eq!(x_at(&mut world, 2.0), 1.0, "seeking back hands it back");
}

#[test]
fn sampled_series_holding_before_start_shows_its_first_sample() {
    let mut world = sampled_world();
    let entity = world
        .spawn(crate::signals::FloatSignal::new(99.0))
        .insert(SampledSeriesDrivers::from(
            SampledSeriesDriver::new(
                vec![0.0, 1.0],
                vec![3.0, 4.0],
                SampledProperty::Signal,
                SampledInterpolation::Linear,
                1.0,
                0.0,
            )
            .unwrap()
            .starting_at(2.0)
            .holding_before_start(),
        ))
        .id();
    sampled_series_system(&mut world);
    assert_eq!(
        world
            .get::<crate::signals::FloatSignal>(entity)
            .unwrap()
            .value,
        3.0
    );
}

#[test]
fn sampled_series_step_interpolation_holds_previous_value() {
    let mut world = sampled_world();
    let entity = world
        .spawn(SpatialTransform::default())
        .insert(SampledSeriesDrivers::from(
            SampledSeriesDriver::new(
                vec![0.0, 1.0],
                vec![1.0, 3.0],
                SampledProperty::TranslateY,
                SampledInterpolation::Step,
                1.0,
                0.0,
            )
            .unwrap(),
        ))
        .id();

    world.resource_mut::<PlaybackState>().current_time = 0.999;
    sampled_series_system(&mut world);
    assert_eq!(
        world.get::<SpatialTransform>(entity).unwrap().translation.y,
        1.0
    );

    world.resource_mut::<PlaybackState>().current_time = 1.0;
    sampled_series_system(&mut world);
    assert_eq!(
        world.get::<SpatialTransform>(entity).unwrap().translation.y,
        3.0
    );
}

#[test]
fn sampled_series_absolute_properties_and_determinism_across_seeks() {
    let mut world = sampled_world();
    let scaled = world
        .spawn(SpatialTransform::default())
        .insert(SampledSeriesDrivers::from(
            SampledSeriesDriver::new(
                vec![0.0, 1.0],
                vec![1.0, 3.0],
                SampledProperty::UniformScale,
                SampledInterpolation::Linear,
                0.5,
                0.0,
            )
            .unwrap(),
        ))
        .id();
    let signalled = world
        .spawn(crate::signals::FloatSignal::new(0.0))
        .insert(SampledSeriesDrivers::from(
            SampledSeriesDriver::new(
                vec![0.0, 2.0],
                vec![-1.0, 1.0],
                SampledProperty::Signal,
                SampledInterpolation::Linear,
                2.0,
                5.0,
            )
            .unwrap(),
        ))
        .id();

    let evaluate = |world: &mut World, time: f64| {
        world.resource_mut::<PlaybackState>().current_time = time;
        sampled_series_system(world);
        (
            world.get::<SpatialTransform>(scaled).unwrap().scale.x,
            world
                .get::<crate::signals::FloatSignal>(signalled)
                .unwrap()
                .value,
        )
    };

    let forward = evaluate(&mut world, 0.5);
    assert_eq!(forward, (1.0, 4.0));
    let later = evaluate(&mut world, 1.0);
    assert_eq!(later, (1.5, 5.0));

    // Seek hacia atrás y de nuevo hacia adelante: misma salida, sin replay.
    evaluate(&mut world, 0.0);
    assert_eq!(evaluate(&mut world, 0.5), forward);
    assert_eq!(evaluate(&mut world, 1.0), later);
    assert_eq!(evaluate(&mut world, 9.0), (1.5, 7.0));
}

#[test]
fn sampled_series_drives_rotation_and_opacity() {
    let mut world = sampled_world();
    let rotated = world
        .spawn(SpatialTransform::default().with_rotation_2d(0.5))
        .insert(SampledSeriesDrivers::from(
            SampledSeriesDriver::new(
                vec![0.0, 1.0],
                vec![0.0, 1.0],
                SampledProperty::RotateZ,
                SampledInterpolation::Linear,
                1.0,
                0.0,
            )
            .unwrap(),
        ))
        .id();
    let faded = world
        .spawn(SpatialTransform::default())
        .insert(gaanim_scene::Opacity(1.0))
        .insert(SampledSeriesDrivers::from(
            SampledSeriesDriver::new(
                vec![0.0, 1.0],
                vec![1.0, -1.0],
                SampledProperty::Opacity,
                SampledInterpolation::Linear,
                1.0,
                0.0,
            )
            .unwrap(),
        ))
        .id();

    world.resource_mut::<PlaybackState>().current_time = 1.0;
    sampled_series_system(&mut world);

    let rotation = world.get::<SpatialTransform>(rotated).unwrap().rotation;
    assert!((rotation.to_scaled_axis().z - 1.5).abs() < 1e-12);
    // La opacidad se clampea a [0, 1] aunque la serie la cruce.
    assert_eq!(world.get::<gaanim_scene::Opacity>(faded).unwrap().0, 0.0);
}

#[test]
fn a_fill_series_colors_a_group_through_its_ramp() {
    use gaanim_core::peniko::{Brush, Color};
    let mut world = World::new();
    world.insert_resource(PlaybackState::default());
    let red = Color::from_rgb8(255, 0, 0);
    let green = Color::from_rgb8(0, 255, 0);
    let solid = |color| gaanim_scene::FillBrush(Some(Brush::Solid(color)));
    let child = world.spawn(solid(Color::BLACK)).id();
    let unfilled = world.spawn(gaanim_scene::FillBrush(None)).id();
    let group = world
        .spawn(SampledSeriesDrivers::from(
            SampledSeriesDriver::new(
                vec![0.0, 2.0],
                vec![0.0, 20.0],
                SampledProperty::Fill,
                SampledInterpolation::Linear,
                1.0,
                0.0,
            )
            .unwrap()
            .with_colors(vec![red, green], None)
            .unwrap(),
        ))
        .id();
    world.entity_mut(child).insert(ChildOf(group));
    world.entity_mut(unfilled).insert(ChildOf(group));
    let fill = |world: &World, entity| {
        world
            .get::<gaanim_scene::FillBrush>(entity)
            .unwrap()
            .0
            .clone()
    };

    world.resource_mut::<PlaybackState>().current_time = 2.0;
    sampled_series_system(&mut world);
    assert_eq!(fill(&world, child), Some(Brush::Solid(green)));
    // Un hijo sin relleno sigue sin él.
    assert_eq!(fill(&world, unfilled), None);

    world.resource_mut::<PlaybackState>().current_time = 0.0;
    sampled_series_system(&mut world);
    assert_eq!(fill(&world, child), Some(Brush::Solid(red)));

    let invalid = SampledSeriesDriver::new(
        vec![0.0],
        vec![1.0],
        SampledProperty::Fill,
        SampledInterpolation::Linear,
        1.0,
        0.0,
    )
    .unwrap();
    assert!(invalid.clone().with_colors(Vec::new(), None).is_err());
    assert!(invalid.with_colors(vec![red], Some((1.0, 1.0))).is_err());
}
