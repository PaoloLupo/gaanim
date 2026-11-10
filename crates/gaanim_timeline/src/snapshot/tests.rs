use super::*;
use bevy::prelude::{
    BuildChildrenTransformExt, Changed, ChildOf, GlobalTransform, Schedule, Transform, Vec3, World,
};
use std::sync::Arc;

/// The capture as it read each component by entity, to check the
/// query-based one against.
fn capture_by_lookup(world: &mut World) -> WorldSnapshot {
    let mut entities = bevy::platform::collections::HashMap::new();
    let camera = world.get_resource::<gaanim_math::Camera>().copied();
    let depth_of_field = world.get_resource::<gaanim_scene::DepthOfField>().copied();
    let camera_states = world
        .get_resource::<CapturedCameraStates>()
        .map(|states| states.0.clone())
        .unwrap_or_default();

    // Query all entities with a MobjectId component
    let mut query = world.query::<(Entity, &MobjectId)>();

    let mut captured_data = Vec::new();

    for (entity, mobj_id) in query.iter(world) {
        // Echo copies are re-evaluated from their sources after every seek.
        if world.get::<gaanim_animation::EchoGhost>(entity).is_some() {
            continue;
        }
        let obj_id = mobj_id.0;
        // Find parent entity's ObjectId if parent is set
        let parent_entity = world
            .get::<bevy::prelude::ChildOf>(entity)
            .map(|c| c.parent());
        let parent_id = parent_entity.and_then(|p| world.get::<MobjectId>(p).copied().map(|m| m.0));

        let transform = world
            .get::<SpatialTransform>(entity)
            .copied()
            .unwrap_or_default();
        let opacity = world.get::<Opacity>(entity).map(|o| o.0).unwrap_or(1.0);
        let has_fill_component = world.get::<FillBrush>(entity).is_some();
        let fill = world.get::<FillBrush>(entity).and_then(|f| f.0.clone());
        let stroke = world
            .get::<StrokeBrush>(entity)
            .and_then(|s| s.brush.clone());
        let stroke_style = world.get::<StrokeBrush>(entity).map(|s| s.style.clone());
        let render_order_opt = world.get::<RenderOrder>(entity);
        let render_order = render_order_opt.map(|r| r.z_index).unwrap_or(0);
        let creation_order = render_order_opt.map(|r| r.creation_order).unwrap_or(0);
        let render_layer = world
            .get::<RenderLayer>(entity)
            .copied()
            .unwrap_or(RenderLayer::Vello2D);
        let visible = world.get::<Visible>(entity).is_some();
        let is_group = world.get::<gaanim_scene::GroupMarker>(entity).is_some();

        let mut tags = Vec::new();
        if let Some(tag) = world.get::<ObjectTag>(entity) {
            tags.push(tag.0.clone());
        }

        captured_data.push((
            obj_id,
            EntitySnapshot {
                id: obj_id,
                parent: parent_id,
                transform,
                opacity,
                fill,
                has_fill_component,
                stroke,
                stroke_style,
                render_order,
                creation_order,
                render_layer,
                visible,
                tags,
                path2d: world.get::<Path2D>(entity).map(|p| p.0.clone()),
                path_source: world.get::<PathSource>(entity).map(|p| p.0.clone()),
                fill_draw_progress: world
                    .get::<gaanim_animation::FillDrawProgress>(entity)
                    .map(|p| p.0),
                fill_level: world.get::<FillLevel>(entity).map(|level| level.0),
                surrounding_rect: world
                    .get::<gaanim_animation::SurroundingRect>(entity)
                    .cloned(),
                connector_progress: world
                    .get::<gaanim_animation::updaters::TrackingConnector>(entity)
                    .map(|connector| connector.progress)
                    .or_else(|| {
                        world
                            .get::<gaanim_animation::updaters::TrackingLine3D>(entity)
                            .map(|line| line.progress)
                    }),
                write_tip_glow: world.get::<gaanim_animation::WriteTipGlow>(entity).cloned(),
                path_reveal: world
                    .get::<gaanim_animation::PathReveal>(entity)
                    .map(|p| p.0),
                path_trim_window: world
                    .get::<gaanim_animation::PathTrimWindow>(entity)
                    .copied(),
                float_signal: world
                    .get::<gaanim_animation::FloatSignal>(entity)
                    .map(|s| s.value),
                material_3d: world.get::<gaanim_scene::Material3D>(entity).copied(),
                media_frame: world.get::<gaanim_scene::MediaFrame>(entity).copied(),
                coordinate_view_role: world
                    .get::<gaanim_scene::CoordinateViewRole>(entity)
                    .copied(),
                traced_path_points: world
                    .get::<gaanim_animation::TracedPath>(entity)
                    .map(|t| t.points.clone())
                    .or_else(|| {
                        world
                            .get::<gaanim_animation::TracedPath3D>(entity)
                            .map(|t| t.points.clone())
                    }),
                traced_path_sample_times: world
                    .get::<gaanim_animation::TracedPath>(entity)
                    .map(|t| t.sample_times.clone())
                    .or_else(|| {
                        world
                            .get::<gaanim_animation::TracedPath3D>(entity)
                            .map(|t| t.sample_times.clone())
                    }),
                is_group,
                global_transform: world.get::<GlobalSpatialTransform>(entity).copied(),
                local_bounds: world.get::<LocalBounds>(entity).copied(),
                world_bounds: world.get::<WorldBounds>(entity).copied(),
                global_opacity: world.get::<GlobalOpacity>(entity).copied(),
                scene: world.get::<SceneMember>(entity).map(|s| s.0),
            },
        ));
    }

    for (id, snapshot) in captured_data {
        entities.insert(id, Box::new(snapshot));
    }

    WorldSnapshot {
        entities,
        camera,
        camera_states,
        depth_of_field,
    }
}

fn changed_count<T: bevy::prelude::Component>(world: &mut World) -> usize {
    world
        .query_filtered::<bevy::prelude::Entity, Changed<T>>()
        .iter(world)
        .count()
}

#[test]
fn the_query_capture_matches_reading_each_component() {
    use gaanim_core::peniko::Color;
    let mut world = World::new();
    let scene = crate::timeline::Timeline::default().add_scene("first");
    let path = Arc::new(gaanim_core::kurbo::BezPath::from_svg("M0,0 L10,10").unwrap());
    let group = world
        .spawn((
            MobjectId(ObjectId::from_parts(1, 1)),
            gaanim_scene::GroupMarker,
            SpatialTransform::default(),
            Visible,
            SceneMember(scene),
        ))
        .id();
    let unregistered_parent = world.spawn(SpatialTransform::default()).id();
    for index in 0..40u32 {
        let mut entity = world.spawn((
            MobjectId(ObjectId::from_parts(10 + index, 1)),
            SpatialTransform::default(),
            Opacity(0.25 * (index % 4) as f32),
        ));
        if index % 2 == 0 {
            entity.insert((
                Path2D(path.clone()),
                PathSource(path.clone()),
                FillBrush::color(Color::from_rgb8(index as u8, 0, 0)),
                Visible,
            ));
        }
        if index % 3 == 0 {
            entity.insert((
                StrokeBrush::new(Color::BLACK, 1.0 + f64::from(index)),
                RenderOrder {
                    z_index: index as i32,
                    creation_order: u64::from(index),
                },
                ObjectTag(format!("tag {index}")),
            ));
        }
        if index % 5 == 0 {
            entity.insert((
                FillLevel(0.5),
                gaanim_animation::FloatSignal::new(f64::from(index)),
                gaanim_animation::PathReveal(0.3),
                gaanim_animation::FillDrawProgress(0.7),
                LocalBounds(gaanim_math::Bounds3D::new_2d(0.0, 0.0, 1.0, 2.0)),
                GlobalOpacity(0.5),
                GlobalSpatialTransform::default(),
            ));
        }
        if index % 7 == 0 {
            entity.insert(gaanim_animation::EchoGhost {
                source: ObjectId::from_parts(10, 1),
                lag: 0.5,
                opacity: 0.5,
                parent: None,
                rank: 1,
                hold: false,
                motion_sources: std::sync::Arc::default(),
                window: None,
            });
        }
        let entity = entity.id();
        match index % 4 {
            0 => world.entity_mut(entity).set_parent_in_place(group),
            1 => world
                .entity_mut(entity)
                .set_parent_in_place(unregistered_parent),
            _ => &mut world.entity_mut(entity),
        };
    }
    assert_eq!(
        WorldSnapshot::capture(&mut world),
        capture_by_lookup(&mut world)
    );
}

#[test]
fn coordinate_view_roles_survive_snapshot_recreation() {
    use gaanim_scene::CoordinateViewRole;
    let mut world = World::new();
    for (index, role) in [CoordinateViewRole::View, CoordinateViewRole::Label]
        .into_iter()
        .enumerate()
    {
        world.spawn((
            MobjectId(ObjectId::from_parts(index as u32 + 1, 1)),
            SpatialTransform::default(),
            role,
        ));
    }
    let snapshot = WorldSnapshot::capture(&mut world);
    world.clear_entities();
    snapshot.restore(&mut world);
    for captured in snapshot.entities.values() {
        let role = world
            .query::<(&MobjectId, &CoordinateViewRole)>()
            .iter(&world)
            .find_map(|(id, role)| (id.0 == captured.id).then_some(*role));
        assert_eq!(role, captured.coordinate_view_role);
    }
}

#[test]
fn restoring_snapshot_rewinds_fill_level() {
    let mut world = World::new();
    let entity = world
        .spawn((MobjectId(ObjectId::from_parts(1, 1)), FillLevel(0.2)))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);

    world.get_mut::<FillLevel>(entity).unwrap().0 = 0.9;
    snapshot.restore(&mut world);

    assert_eq!(world.get::<FillLevel>(entity), Some(&FillLevel(0.2)));
}

#[test]
fn restoring_an_identical_svg_like_snapshot_does_not_change_render_components() {
    let mut world = World::new();
    let path = Arc::new(gaanim_core::kurbo::BezPath::from_svg("M0,0 L10,10").unwrap());
    let fill = FillBrush::color(gaanim_core::peniko::Color::from_rgb8(0x2d, 0x7d, 0xff));
    let stroke = StrokeBrush::new(gaanim_core::peniko::Color::BLACK, 1.5);

    // Model 100 `scene.svg()` copies, each with several leaf paths. This
    // deliberately exercises the cache-invalidating components without a
    // brittle wall-clock assertion.
    for copy in 0..100 {
        let group = world
            .spawn((
                MobjectId(ObjectId::from_parts(copy, 0)),
                gaanim_scene::GroupMarker,
            ))
            .id();
        for leaf in 1..=4 {
            let child = world
                .spawn((
                    MobjectId(ObjectId::from_parts(copy, leaf)),
                    Path2D(path.clone()),
                    PathSource(path.clone()),
                    fill.clone(),
                    stroke.clone(),
                ))
                .id();
            world.entity_mut(child).set_parent_in_place(group);
        }
    }

    let snapshot = WorldSnapshot::capture(&mut world);
    world.clear_trackers();
    snapshot.restore(&mut world);

    assert_eq!(changed_count::<Path2D>(&mut world), 0);
    assert_eq!(changed_count::<PathSource>(&mut world), 0);
    assert_eq!(changed_count::<FillBrush>(&mut world), 0);
    assert_eq!(changed_count::<StrokeBrush>(&mut world), 0);
}

#[test]
fn restoring_an_identical_hierarchy_reconciles_native_transforms() {
    let mut world = World::new();
    let parent = world
        .spawn((
            MobjectId(ObjectId::from_parts(10, 0)),
            Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            GlobalTransform::from(Transform::from_translation(Vec3::new(10.0, 0.0, 0.0))),
        ))
        .id();
    let child = world
        .spawn((
            MobjectId(ObjectId::from_parts(11, 0)),
            Transform::from_translation(Vec3::ZERO),
            GlobalTransform::from(Transform::from_translation(Vec3::new(15.0, 0.0, 0.0))),
        ))
        .id();
    world.entity_mut(child).set_parent_in_place(parent);
    let snapshot = WorldSnapshot::capture(&mut world);

    world
        .entity_mut(child)
        .insert(Transform::from_translation(Vec3::splat(99.0)));
    snapshot.restore(&mut world);

    assert_eq!(
        world.get::<Transform>(child).unwrap().translation,
        Vec3::new(5.0, 0.0, 0.0),
        "restoring an unchanged ChildOf must still reconcile Bevy Transform from GlobalTransform"
    );

    world.clear_trackers();
    snapshot.restore(&mut world);
    assert_eq!(changed_count::<ChildOf>(&mut world), 0);
    assert_eq!(changed_count::<Transform>(&mut world), 0);
}

#[test]
fn restoring_an_identical_snapshot_marks_local_state_for_propagation() {
    let mut world = World::new();
    world.spawn((
        MobjectId(ObjectId::from_parts(12, 0)),
        SpatialTransform::default(),
        Opacity::default(),
    ));
    let snapshot = WorldSnapshot::capture(&mut world);

    world.clear_trackers();
    snapshot.restore(&mut world);

    assert_eq!(changed_count::<SpatialTransform>(&mut world), 1);
    assert_eq!(changed_count::<Opacity>(&mut world), 1);
}

#[test]
fn restoring_a_different_snapshot_updates_components_and_hierarchy() {
    let mut world = World::new();
    let parent_id = ObjectId::from_parts(1, 0);
    let child_id = ObjectId::from_parts(1, 1);
    let parent = world.spawn(MobjectId(parent_id)).id();
    let child = world
        .spawn((
            MobjectId(child_id),
            Path2D(Arc::new(
                gaanim_core::kurbo::BezPath::from_svg("M0,0 L1,1").unwrap(),
            )),
            PathSource(Arc::new(
                gaanim_core::kurbo::BezPath::from_svg("M0,0 L1,1").unwrap(),
            )),
            FillBrush::color(gaanim_core::peniko::Color::from_rgb8(0xff, 0x00, 0x00)),
            StrokeBrush::new(gaanim_core::peniko::Color::BLACK, 1.0),
        ))
        .id();
    world.entity_mut(child).set_parent_in_place(parent);
    let snapshot = WorldSnapshot::capture(&mut world);

    let other_parent = world.spawn(MobjectId(ObjectId::from_parts(2, 0))).id();
    world.entity_mut(child).insert((
        Path2D(Arc::new(
            gaanim_core::kurbo::BezPath::from_svg("M0,0 L2,2").unwrap(),
        )),
        PathSource(Arc::new(
            gaanim_core::kurbo::BezPath::from_svg("M0,0 L2,2").unwrap(),
        )),
        FillBrush::color(gaanim_core::peniko::Color::from_rgb8(0x00, 0x00, 0xff)),
        StrokeBrush::new(gaanim_core::peniko::Color::BLACK, 2.0),
    ));
    world.entity_mut(child).set_parent_in_place(other_parent);
    world.clear_trackers();
    snapshot.restore(&mut world);

    assert_eq!(
        world.get::<ChildOf>(child).map(|parent| parent.parent()),
        Some(parent)
    );
    assert_eq!(changed_count::<Path2D>(&mut world), 1);
    assert_eq!(changed_count::<PathSource>(&mut world), 1);
    assert_eq!(changed_count::<FillBrush>(&mut world), 1);
    assert_eq!(changed_count::<StrokeBrush>(&mut world), 1);
    assert_eq!(changed_count::<ChildOf>(&mut world), 1);
}

#[test]
fn restoring_an_unstyled_group_does_not_clear_child_fills() {
    let mut world = World::new();
    let group_id = ObjectId::from_parts(1, 1);
    let child_id = ObjectId::from_parts(2, 1);
    let gold = gaanim_core::peniko::Color::from_rgb8(0xff, 0xd7, 0x00);

    let group = world
        .spawn((MobjectId(group_id), gaanim_scene::GroupMarker))
        .id();
    let child = world
        .spawn((MobjectId(child_id), FillBrush::color(gold)))
        .id();
    world.entity_mut(child).set_parent_in_place(group);

    let snapshot = WorldSnapshot::capture(&mut world);
    assert!(!snapshot.entities[&group_id].has_fill_component);

    // Simulate the pre-fix replay state that injected a transparent fill
    // into a group before restoring the timeline snapshot.
    world.entity_mut(group).insert(FillBrush::transparent());
    snapshot.restore(&mut world);

    assert!(world.get::<FillBrush>(group).is_none());

    let expected = FillBrush::color(gold);
    let mut schedule = Schedule::default();
    schedule.add_systems(gaanim_scene::systems::style_propagation_system);
    schedule.run(&mut world);
    assert_eq!(world.get::<FillBrush>(child), Some(&expected));
}

#[test]
fn restoring_a_snapshot_resets_write_tip_progress() {
    let mut world = World::new();
    let id = ObjectId::from_parts(3, 1);
    let entity = world
        .spawn((
            MobjectId(id),
            gaanim_animation::WriteTipGlow {
                completion: 0.0,
                ..Default::default()
            },
        ))
        .id();
    let snapshot = WorldSnapshot::capture(&mut world);

    world
        .get_mut::<gaanim_animation::WriteTipGlow>(entity)
        .unwrap()
        .completion = 0.5;
    snapshot.restore(&mut world);

    assert_eq!(
        world
            .get::<gaanim_animation::WriteTipGlow>(entity)
            .unwrap()
            .completion,
        0.0,
        "seeking before a Write clip must not retain its previous partial glow"
    );
}

#[test]
fn snapshot_restores_complete_authored_camera_state() {
    let mut world = World::new();
    let mut authored = gaanim_math::Camera::perspective_3d(1920, 1080, 0.9);
    authored
        .look_at(
            gaanim_core::glam::DVec3::new(7.0, 4.0, 9.0),
            gaanim_core::glam::DVec3::new(1.0, 2.0, -3.0),
            gaanim_core::glam::DVec3::Y,
        )
        .unwrap();
    world.insert_resource(authored);
    let snapshot = WorldSnapshot::capture(&mut world);

    let mut changed = gaanim_math::Camera::ortho_2d(320, 240);
    changed.position = gaanim_core::glam::DVec3::splat(42.0);
    changed.target = gaanim_core::glam::DVec3::X;
    changed.up = gaanim_core::glam::DVec3::Z;
    world.insert_resource(changed);
    snapshot.restore(&mut world);

    assert_eq!(*world.resource::<gaanim_math::Camera>(), authored);
}
