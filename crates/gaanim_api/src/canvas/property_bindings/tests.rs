use super::*;
use crate::canvas::SceneModel;
use bevy::prelude::{Entity, With, Without, World};
use gaanim_animation::FloatSignal;
use gaanim_math::{RateFunc, SpatialTransform};
use gaanim_scene::Opacity;
use gaanim_timeline::{snapshot::WorldSnapshot, timeline::Timeline};

#[test]
fn reactive_fill_level_seeks_rebinds_and_fixed_cuts() {
    let mut canvas = SceneModel::new(16, 9);
    let amount = canvas.parameter(0.0).unwrap();
    let other = canvas.parameter(0.8).unwrap();
    let mask = canvas.rect(2.0, 2.0);
    let fill = canvas
        .fill_level(
            &mask,
            gaanim_core::peniko::Color::WHITE.into(),
            0.1,
            super::super::FillLevelDirection::Up,
            false,
        )
        .unwrap()
        .set_fill_level(amount.source())
        .unwrap();
    assert!(fill.animate().try_fill_level(0.5).is_err());
    assert!(canvas.circle(1.0).set_fill_level(amount.source()).is_err());
    canvas.play(vec![
        amount
            .animate()
            .set(1.5)
            .duration(1.0)
            .rate_func(RateFunc::Linear),
    ]);
    let fill = fill.set_fill_level(other.source()).unwrap();
    canvas.wait(1.0);
    fill.set_fill_level(0.4).unwrap();
    canvas.wait(1.0);
    let (mut world, mut timeline, _) = compile(&canvas);
    let entity = world
        .query_filtered::<Entity, With<gaanim_scene::FillLevel>>()
        .single(&world)
        .unwrap();
    for (time, expected) in [
        (0.0, 0.0),
        (0.5, 0.75),
        (0.9, 1.0),
        (1.0, 0.8),
        (2.0, 0.4),
        (1.5, 0.8),
        (0.5, 0.75),
        (2.5, 0.4),
    ] {
        timeline.seek(&mut world, time);
        assert!(
            (world.get::<gaanim_scene::FillLevel>(entity).unwrap().0 - expected).abs() < 1e-8,
            "fill at {time}"
        );
    }
}

#[test]
fn connector_follows_anchors_through_reverse_seek() {
    use super::super::{Anchor, CanvasEndpoint};
    use gaanim_core::kurbo::Shape;
    let mut canvas = SceneModel::new(16, 9);
    let target = canvas.rect(2.0, 2.0).move_to(3.0, 0.0);
    canvas
        .connector(
            CanvasEndpoint::Static(DVec3::new(-3.0, 0.0, 0.0)),
            target.anchor_point(Anchor::Left, DVec3::ZERO).into(),
            vec![CanvasEndpoint::Static(DVec3::new(0.0, -1.0, 0.0))],
            0.18,
            0.15,
            0.036,
            None,
        )
        .unwrap();
    canvas.play(vec![
        target
            .animate()
            .shift_by(2.0, 0.0)
            .duration(1.0)
            .rate_func(RateFunc::Linear),
    ]);
    let (mut world, mut timeline, _) = compile(&canvas);
    let entity = world
        .query_filtered::<Entity, With<gaanim_animation::updaters::TrackingConnector>>()
        .single(&world)
        .unwrap();
    for (time, tip_x) in [(0.0, 2.0), (0.5, 3.0), (1.0, 4.0), (0.5, 3.0), (0.0, 2.0)] {
        timeline.seek(&mut world, time);
        // Runtime schedules derived geometry after timeline evaluation.
        gaanim_animation::tracking_line_system(&mut world);
        let path = &world.get::<gaanim_scene::PathSource>(entity).unwrap().0;
        assert!(
            (path.bounding_box().x1 - tip_x).abs() < 1e-8,
            "tip at {time}"
        );
    }
    let mut foreign = SceneModel::new(16, 9);
    let anchor = foreign
        .rect(1.0, 1.0)
        .anchor_point(Anchor::Left, DVec3::ZERO);
    assert!(
        canvas
            .connector(
                anchor.into(),
                CanvasEndpoint::Static(DVec3::ZERO),
                vec![],
                0.18,
                0.15,
                0.036,
                None
            )
            .is_err()
    );
}

#[test]
fn layout_card_background_and_ports_survive_reflow_and_seek() {
    use super::super::{Anchor, LayoutMemberSpec, LayoutSpec};
    use gaanim_core::kurbo::Shape;
    let mut canvas = SceneModel::new(16, 9);
    let content = canvas.rect(2.0, 1.0);
    let card = canvas
        .group(&[&content])
        .move_to(-3.0, 1.0)
        .with_port("out", Anchor::Right, DVec3::new(0.1, 0.0, 0.0))
        .unwrap();
    assert!(
        card.clone()
            .with_port("out", Anchor::Left, DVec3::ZERO)
            .is_err()
    );
    assert!(card.port("missing").is_err());
    assert!(
        card.clone()
            .with_port("", Anchor::Left, DVec3::ZERO)
            .is_err()
    );
    let background = canvas
        .decorate_layout(
            &card,
            Some(gaanim_core::peniko::Color::WHITE.into()),
            None,
            0.08,
        )
        .unwrap();
    content.claim_layout(&card).unwrap();
    let members = vec![LayoutMemberSpec {
        id: content.id,
        style: Default::default(),
    }];
    let spec = LayoutSpec {
        style: gaanim_layout::LayoutStyle {
            padding: gaanim_layout::Insets::all(0.25),
            ..Default::default()
        },
        ..Default::default()
    };
    canvas.reflow_layout(&card, members.clone(), spec.clone(), 1, None, None, None);
    canvas.wait(1.0);
    let mut wide = spec;
    wide.style.width = gaanim_layout::SizeRule::Fixed(4.0);
    canvas.set_group_members(&card, &[&content]);
    canvas.reflow_layout(&card, members, wide, 2, None, None, None);
    canvas.wait(1.0);
    let point = card.port("out").unwrap();
    canvas
        .connector(
            point.into(),
            super::super::CanvasEndpoint::Static(DVec3::new(6.0, 0.0, 0.0)),
            vec![],
            0.18,
            0.15,
            0.036,
            None,
        )
        .unwrap();
    let (mut world, mut timeline, content_entity) = compile(&canvas);
    let entity = world
        .query_filtered::<Entity, With<gaanim_animation::updaters::LayoutBackground>>()
        .single(&world)
        .unwrap();
    world.insert_resource(gaanim_animation::PlaybackState::default());
    for (time, width) in [(0.0, 2.5), (1.5, 4.0), (0.5, 2.5), (1.5, 4.0)] {
        timeline.seek(&mut world, time);
        gaanim_animation::tracking_line_system(&mut world);
        let path = &world.get::<gaanim_scene::PathSource>(entity).unwrap().0;
        assert!(
            (path.bounding_box().width() - width).abs() < 1e-8,
            "card background at {time}: {:?}",
            path.bounding_box()
        );
        assert!(world.get::<bevy::prelude::ChildOf>(entity).is_some());
        let content_x = world
            .get::<gaanim_math::SpatialTransform>(content_entity)
            .unwrap()
            .translation
            .x;
        let expected_x = if time < 1.0 { 0.0 } else { -0.75 };
        assert!(
            (content_x - expected_x).abs() < 1e-8,
            "content at {time}: {content_x}"
        );
        assert_eq!(
            world
                .get::<gaanim_math::SpatialTransform>(entity)
                .unwrap()
                .translation,
            DVec3::ZERO
        );
    }
    let _ = background;
}

fn compile(canvas: &SceneModel) -> (World, Timeline, Entity) {
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let target = world
        .query_filtered::<Entity, (With<Opacity>, Without<FloatSignal>)>()
        .iter(&world)
        .next()
        .unwrap();
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    (world, timeline, target)
}

/// The opacity drawn: the authored opacity times a binding's factor.
fn drawn_opacity(world: &World, entity: Entity) -> f64 {
    let reactive = world
        .get::<gaanim_scene::ReactiveOpacity>(entity)
        .map_or(1.0, |opacity| opacity.0);
    f64::from(world.get::<Opacity>(entity).unwrap().0 * reactive)
}

#[test]
fn delayed_source_target_and_following_relative_clip_use_exact_start_values() {
    let mut canvas = SceneModel::new(320, 180);
    let parameter = canvas.parameter(0.0).unwrap();
    let dot = canvas.circle(0.1);
    let movement = dot
        .animate()
        .move_to(&parameter, 0.0)
        .delay(1.0)
        .duration(1.0)
        .rate_func(RateFunc::Linear);
    canvas
        .play_items(vec![
            parameter
                .animate()
                .set(8.0)
                .duration(4.0)
                .rate_func(RateFunc::Linear)
                .into(),
            movement.into(),
        ])
        .unwrap();
    canvas
        .play_items(vec![
            dot.animate()
                .shift_by(10.0, 0.0)
                .duration(1.0)
                .rate_func(RateFunc::Linear)
                .into(),
        ])
        .unwrap();
    let (mut world, mut timeline, entity) = compile(&canvas);
    for (time, expected) in [(1.5, 1.0), (5.0, 12.0), (4.5, 7.0), (0.0, 0.0), (2.0, 2.0)] {
        timeline.seek(&mut world, time);
        assert!(
            (world.get::<SpatialTransform>(entity).unwrap().translation.x - expected).abs() < 1e-9,
            "time {time}"
        );
    }
}

#[test]
fn delayed_drawable_destination_is_captured_at_its_actual_start() {
    let mut canvas = SceneModel::new(320, 180);
    let dot = canvas.circle(0.1);
    let destination = canvas.circle(0.2);
    canvas
        .play_items(vec![
            destination
                .animate()
                .move_to(8.0, 0.0)
                .duration(4.0)
                .rate_func(RateFunc::Linear)
                .into(),
            dot.animate()
                .move_to_drawable(&destination)
                .unwrap()
                .delay(1.0)
                .duration(1.0)
                .rate_func(RateFunc::Linear)
                .into(),
        ])
        .unwrap();
    canvas
        .play_items(vec![
            dot.animate()
                .shift_by(10.0, 0.0)
                .duration(1.0)
                .rate_func(RateFunc::Linear)
                .into(),
        ])
        .unwrap();
    let (mut world, mut timeline, entity) = compile(&canvas);
    for (time, expected) in [
        (0.0, 0.0),
        (1.5, 1.0),
        (5.0, 12.0),
        (4.5, 7.0),
        (1.5, 1.0),
        (0.0, 0.0),
    ] {
        timeline.seek(&mut world, time);
        assert!(
            (world.get::<SpatialTransform>(entity).unwrap().translation.x - expected).abs() < 1e-9,
            "time {time}"
        );
    }
}

#[test]
fn replacement_binding_and_fixed_cut_restore_at_exact_boundaries() {
    let mut canvas = SceneModel::new(320, 180);
    let first = canvas.parameter(0.2).unwrap();
    let second = canvas.parameter(0.8).unwrap();
    let dot = canvas
        .circle(0.1)
        .bind_property(PropertySources::Opacity(first.source()))
        .unwrap();
    canvas.wait(1.0);
    let dot = dot
        .bind_property(PropertySources::Opacity(second.source()))
        .unwrap();
    canvas.wait(1.0);
    dot.opacity(0.4);
    canvas.wait(1.0);
    let (mut world, mut timeline, entity) = compile(&canvas);
    for (time, expected) in [
        (0.0, 0.2),
        (0.999999, 0.2),
        (1.0, 0.8),
        (2.0, 0.4),
        (1.5, 0.8),
        (0.0, 0.2),
    ] {
        timeline.seek(&mut world, time);
        assert!(
            (drawn_opacity(&world, entity) - expected).abs() < 1e-6,
            "time {time}"
        );
    }
}

#[test]
fn nested_rust_sources_retain_scene_ownership_even_when_local_ids_match() {
    let mut first = SceneModel::new(320, 180);
    let foreign = first.parameter(0.5).unwrap();
    let mut second = SceneModel::new(320, 180);
    let local = second.parameter(0.5).unwrap();
    assert_eq!(foreign.drawable().id, local.drawable().id);
    let source = gaanim_animation::ScalarSource::Function(
        gaanim_animation::ReactiveFunction::from_sources(0, 1, vec![foreign.source()], |v| {
            Ok(vec![v[0]])
        })
        .map_scalar(|v| v * 2.0)
        .unwrap(),
    );
    let dot = second.circle(0.1);
    assert!(
        dot.clone()
            .bind_property(PropertySources::Opacity(source.clone()))
            .is_err()
    );
    assert!(
        dot.animate()
            .property_source(PropertySources::Opacity(source))
            .is_err()
    );
}

#[test]
fn rust_absolute_setters_accept_mixed_sources_and_fixed_values_end_them() {
    let mut canvas = SceneModel::new(320, 180);
    let p = canvas.parameter(0.5).unwrap();
    let dot = canvas
        .circle(0.1)
        .move_to_3d(&p, 2.0, 3.0)
        .rotate_to_3d(0.0, 0.0, &p)
        .scale_to_3d(1.0, &p, 1.0)
        .opacity(&p);
    canvas.wait(1.0);
    dot.move_to_3d(4.0, 5.0, 6.0)
        .rotate_to(0.0)
        .scale_to(1.0)
        .opacity(1.0_f32);
    canvas.wait(1.0);
    let (mut world, mut timeline, entity) = compile(&canvas);
    for time in [0.5, 1.5, 0.5] {
        timeline.seek(&mut world, time);
        let transform = world.get::<SpatialTransform>(entity).unwrap();
        if time < 1.0 {
            assert_eq!(transform.translation, DVec3::new(0.5, 2.0, 3.0));
            assert_eq!(transform.scale, DVec3::new(1.0, 0.5, 1.0));
            assert!(
                (transform.rotation - gaanim_core::glam::DQuat::from_rotation_z(0.5)).length()
                    < 1e-9
            );
            assert_eq!(drawn_opacity(&world, entity), 0.5);
        } else {
            assert_eq!(transform.translation, DVec3::new(4.0, 5.0, 6.0));
            assert_eq!(transform.scale, DVec3::ONE);
            assert_eq!(drawn_opacity(&world, entity), 1.0);
        }
    }
}

#[test]
fn fixed_setter_replaces_binding_with_reversible_cut() {
    let mut canvas = SceneModel::new(320, 180);
    let parameter = canvas.parameter(0.2).unwrap();
    let dot = canvas
        .circle(0.1)
        .bind_property(PropertySources::Opacity(parameter.source()))
        .unwrap();
    assert!(
        canvas
            .play_items(vec![dot.animate().opacity(0.5).into()])
            .is_err()
    );
    canvas
        .play_items(vec![
            parameter
                .animate()
                .set(0.8)
                .duration(1.0)
                .rate_func(RateFunc::Linear)
                .into(),
        ])
        .unwrap();
    dot.opacity(0.4);
    canvas.wait(1.0);
    let (mut world, mut timeline, entity) = compile(&canvas);
    for (time, expected) in [(0.5, 0.5), (1.5, 0.4), (0.25, 0.35)] {
        timeline.seek(&mut world, time);
        assert!((drawn_opacity(&world, entity) - expected).abs() < 1e-6);
    }
}

#[test]
fn entrances_and_exits_multiply_a_reactive_opacity() {
    let mut canvas = SceneModel::new(320, 180);
    let weight = canvas.parameter(0.5).unwrap();
    // The bound value replaces a fixed opacity.
    let dot = canvas
        .circle(0.1)
        .opacity(0.3_f32)
        .bind_property(PropertySources::Opacity(weight.source()))
        .unwrap();
    canvas.wait(1.0);
    // A fixed opacity target still takes the channel over.
    assert!(
        canvas
            .play_items(vec![dot.animate().opacity(0.2).into()])
            .is_err()
    );
    let linear = |anim: Anim| -> crate::canvas::PlayItem {
        anim.duration(1.0).rate_func(RateFunc::Linear).into()
    };
    for anim in [
        dot.animate().fade_in(),
        weight.animate().set(1.0),
        dot.animate().fade_out(),
        dot.animate().write(),
    ] {
        canvas.play_items(vec![linear(anim)]).unwrap();
    }
    let (mut world, mut timeline, entity) = compile(&canvas);
    for (time, expected) in [
        // Hidden until its entrance, then the parameter times the fade.
        (0.5, 0.0),
        (1.5, 0.25),
        (2.0, 0.5),
        (2.5, 0.75),
        (3.5, 0.5),
        (3.75, 0.25),
        // `write` shows it again at the parameter's value; its outline
        // hides it until drawn.
        (4.5, 1.0),
        (1.5, 0.25),
        (0.0, 0.0),
    ] {
        timeline.seek(&mut world, time);
        assert!(
            (drawn_opacity(&world, entity) - expected).abs() < 1e-6,
            "time {time}: {}",
            drawn_opacity(&world, entity)
        );
    }
}
