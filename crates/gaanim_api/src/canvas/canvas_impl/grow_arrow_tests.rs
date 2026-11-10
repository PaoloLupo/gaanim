use super::*;
use bevy::prelude::{Entity, World};
use gaanim_core::kurbo::Shape;
use gaanim_scene::MobjectId;
use gaanim_timeline::clip::{ClipPayload, PropertyLensSpec};
use gaanim_timeline::snapshot::WorldSnapshot;
use gaanim_timeline::timeline::Timeline;

fn compile(canvas: &mut SceneModel) -> (World, Timeline) {
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let timeline = world.remove_resource::<Timeline>().expect("timeline");
    (world, timeline)
}

fn arrow_grow_targets(timeline: &Timeline) -> Vec<gaanim_core::ObjectId> {
    timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(animation)
                if matches!(animation.lens, PropertyLensSpec::ArrowGrow { .. }) =>
            {
                Some(animation.target)
            }
            _ => None,
        })
        .collect()
}

fn path_of(world: &World, entity: Entity) -> gaanim_core::kurbo::BezPath {
    (*world.get::<gaanim_scene::Path2D>(entity).expect("path").0).clone()
}

#[test]
fn grow_arrow_extends_from_tail_and_restores_exact_arrow() {
    let mut canvas = SceneModel::new(320, 180);
    let arrow = canvas
        .arrow_with_dimensions((-2.0, 0.0), (2.0, 0.0), 0.4, 0.3, 0.08, None)
        .expect("arrow");
    canvas.play(vec![arrow.animate().grow_arrow().duration(1.0)]);

    let (mut world, mut timeline) = compile(&mut canvas);
    let targets = arrow_grow_targets(&timeline);
    assert_eq!(targets.len(), 1, "a solid arrow schedules one grow clip");
    let entity = world
        .query::<(Entity, &MobjectId)>()
        .iter(&world)
        .find_map(|(entity, id)| (id.0 == targets[0]).then_some(entity))
        .expect("arrow entity");
    let full = gaanim_math::ArrowShape::Straight {
        start: gaanim_core::kurbo::Point::new(-2.0, 0.0),
        end: gaanim_core::kurbo::Point::new(2.0, 0.0),
        head_length: 0.4,
        head_width: 0.3,
        body_width: 0.08,
    }
    .path();

    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.0);
    assert!(path_of(&world, entity).elements().is_empty());

    timeline.seek(&mut world, 0.5);
    let mid = path_of(&world, entity).bounding_box();
    assert!((mid.x0 - -2.0).abs() < 1e-9, "tail stays anchored");
    assert!((mid.x1 - 0.0).abs() < 1e-9, "tip is halfway at t=0.5");
    assert!((mid.height() - 0.3).abs() < 1e-9, "head keeps its width");

    timeline.seek(&mut world, 1.0);
    assert_eq!(path_of(&world, entity), full);

    // Seeking backwards re-hides the arrow deterministically.
    timeline.seek(&mut world, 0.0);
    assert!(path_of(&world, entity).elements().is_empty());
}

#[test]
fn grow_arrow_grows_a_connector_along_its_live_polyline() {
    let mut canvas = SceneModel::new(320, 180);
    let connector = canvas
        .connector(
            CanvasEndpoint::Static(DVec3::new(0.0, 0.0, 0.0)),
            CanvasEndpoint::Static(DVec3::new(2.0, 2.0, 0.0)),
            vec![CanvasEndpoint::Static(DVec3::new(2.0, 0.0, 0.0))],
            0.4,
            0.3,
            0.05,
            None,
        )
        .expect("connector");
    canvas.play(vec![
        connector
            .animate()
            .grow_arrow()
            .duration(1.0)
            .rate_func(RateFunc::Linear),
    ]);

    let (mut world, mut timeline) = compile(&mut canvas);
    assert!(arrow_grow_targets(&timeline).is_empty());
    assert!(timeline.clips.values().any(|clip| matches!(
        &clip.payload,
        ClipPayload::Animation(animation)
            if matches!(animation.lens, PropertyLensSpec::ConnectorGrow { .. })
    )));
    let entity = world
            .query_filtered::<Entity, bevy::prelude::With<gaanim_animation::updaters::TrackingConnector>>()
            .single(&world)
            .expect("connector entity");

    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    let mut bounds_at = |time: f64| {
        timeline.seek(&mut world, time);
        gaanim_animation::tracking_line_system(&mut world);
        path_of(&world, entity)
    };
    assert!(
        bounds_at(0.0).elements().is_empty(),
        "hidden before growing"
    );
    let half = bounds_at(0.5).bounding_box();
    assert!(
        (half.x1 - 2.0).abs() < 1e-9 && half.x0.abs() < 1e-9,
        "{half:?}"
    );
    assert!(
        (half.height() - 0.3).abs() < 1e-9,
        "the head keeps its width"
    );
    let full = bounds_at(1.0).bounding_box();
    assert!((full.y1 - 2.0).abs() < 1e-9, "the tip reaches the endpoint");
    // Reverse seeks are exact.
    assert_eq!(bounds_at(0.5).bounding_box(), half);
    assert!(bounds_at(0.0).elements().is_empty());
}

#[test]
fn grow_arrow_on_non_arrow_falls_back_to_create() {
    let mut canvas = SceneModel::new(320, 180);
    let circle = canvas.circle(1.0);
    canvas.play(vec![circle.animate().grow_arrow().duration(1.0)]);

    let (_world, timeline) = compile(&mut canvas);
    assert!(arrow_grow_targets(&timeline).is_empty());
    assert!(timeline.clips.values().any(|clip| matches!(
        &clip.payload,
        ClipPayload::Animation(animation)
            if matches!(animation.lens, PropertyLensSpec::PathCompletion { .. })
    )));
}
