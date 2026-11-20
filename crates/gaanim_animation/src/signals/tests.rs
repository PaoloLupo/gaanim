use super::*;
use bevy::prelude::{App, Update};
use std::sync::Arc;

/// A versioned regeneration rebuilds the path only when its version
/// changes, so a path written in between (a morph) is kept.
#[test]
fn versioned_regeneration_keeps_paths_written_while_the_version_holds() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let line = |x: f64| {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((x, 0.0));
        path
    };
    let version = Arc::new(AtomicU64::new(1));
    let (for_version, for_path) = (version.clone(), version.clone());
    let mut world = World::new();
    let entity = world
        .spawn((
            gaanim_scene::Path2D(Arc::new(BezPath::new())),
            gaanim_scene::PathSource(Arc::new(BezPath::new())),
            AlwaysRedrawRegen::versioned(
                move || for_version.load(Ordering::Acquire),
                move |_| line(1.0 + for_path.load(Ordering::Acquire) as f64),
            ),
        ))
        .id();
    let drawn = |world: &World| world.get::<gaanim_scene::Path2D>(entity).unwrap().0.clone();

    always_redraw_regen_system(&mut world);
    assert_eq!(*drawn(&world), line(2.0));
    world.get_mut::<gaanim_scene::Path2D>(entity).unwrap().0 = Arc::new(line(9.0));
    always_redraw_regen_system(&mut world);
    assert_eq!(
        *drawn(&world),
        line(9.0),
        "the same version keeps the written path"
    );
    version.store(2, Ordering::Release);
    always_redraw_regen_system(&mut world);
    assert_eq!(*drawn(&world), line(3.0), "a new version rebuilds it");
}

#[test]
fn reactive_readout_keeps_create_hidden_when_its_value_changes() {
    let parameter_id = gaanim_core::ObjectId::from_raw(11);
    let mut app = App::new();
    app.insert_resource(gaanim_text::font::FontRegistry::new());
    app.add_systems(Update, reactive_readout_update_system);

    let signal = app.world_mut().spawn(FloatSignal::new(44.2)).id();
    let empty = Arc::new(BezPath::new());
    let readout = app
        .world_mut()
        .spawn((
            ReactiveReadout {
                source: crate::reactive::ScalarSource::signal(parameter_id),
                parameters: vec![(parameter_id, signal)],
                format: ".1f".to_owned(),
                prefix: String::new(),
                suffix: String::new(),
                invalid: "—".to_owned(),
                decimal_separator: '.',
                font_family: "sans-serif".to_owned(),
                font_weight: None,
                font_size: 40.0,
                last_text: "—".to_owned(),
                last_path: empty.clone(),
                last_bounds: Bounds3D::default(),
                reserve: 0.0,
                reserve_advance: 0.0,
            },
            Path2D(empty.clone()),
            PathSource(empty),
            LocalBounds(Bounds3D::default()),
            TextBaseline::default(),
            crate::writing::PathReveal(0.0),
        ))
        .id();

    app.update();

    assert!(
        !app.world().get::<PathSource>(readout).unwrap().0.is_empty(),
        "the full numeric outline must remain available to the Create animation"
    );
    assert!(
        app.world().get::<Path2D>(readout).unwrap().0.is_empty(),
        "a reactive value update must not reveal the readout before Create starts"
    );
}

#[test]
fn reactive_readout_replaces_both_render_paths_when_signal_changes() {
    let parameter_id = gaanim_core::ObjectId::from_raw(7);
    let mut app = App::new();
    app.insert_resource(gaanim_text::font::FontRegistry::new());
    app.add_systems(Update, reactive_readout_update_system);

    let signal = app.world_mut().spawn(FloatSignal::new(2.5)).id();
    let initial_path = Arc::new(BezPath::new());
    let readout = app
        .world_mut()
        .spawn((
            ReactiveReadout {
                source: crate::reactive::ScalarSource::signal(parameter_id),
                parameters: vec![(parameter_id, signal)],
                format: ".1f".to_owned(),
                prefix: String::new(),
                suffix: String::new(),
                invalid: "—".to_owned(),
                decimal_separator: '.',
                font_family: "sans-serif".to_owned(),
                font_weight: None,
                font_size: 40.0,
                last_text: "1.0".to_owned(),
                last_path: initial_path.clone(),
                last_bounds: Bounds3D::default(),
                reserve: 0.0,
                reserve_advance: 0.0,
            },
            Path2D(initial_path.clone()),
            PathSource(initial_path),
            LocalBounds(Bounds3D::default()),
            TextBaseline::default(),
        ))
        .id();

    app.update();

    let component = app.world().get::<ReactiveReadout>(readout).unwrap();
    let path = app.world().get::<Path2D>(readout).unwrap();
    let source = app.world().get::<PathSource>(readout).unwrap();
    assert_eq!(component.last_text, "2.5");
    assert!(!path.0.elements().is_empty());
    assert!(Arc::ptr_eq(&path.0, &source.0));

    let expected_path = path.0.clone();
    let expected_bounds = app.world().get::<LocalBounds>(readout).unwrap().0;
    let restored_initial = Arc::new(BezPath::new());
    app.world_mut().entity_mut(readout).insert((
        Path2D(restored_initial.clone()),
        PathSource(restored_initial),
        LocalBounds(Bounds3D::default()),
    ));

    // Editor playback seeks from its t=0 snapshot every frame. The
    // formatted value may remain unchanged for several frames, but the
    // snapshot-restored geometry must not remain at the initial value.
    app.update();

    let path = app.world().get::<Path2D>(readout).unwrap();
    let source = app.world().get::<PathSource>(readout).unwrap();
    assert!(Arc::ptr_eq(&path.0, &expected_path));
    assert!(Arc::ptr_eq(&source.0, &expected_path));
    assert_eq!(
        app.world().get::<LocalBounds>(readout).unwrap().0,
        expected_bounds
    );
}

fn assert_same_outline(actual: &BezPath, expected: &BezPath) {
    let points = |path: &BezPath| {
        path.elements()
            .iter()
            .flat_map(|element| match *element {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
                PathEl::QuadTo(a, b) => vec![a, b],
                PathEl::CurveTo(a, b, c) => vec![a, b, c],
                PathEl::ClosePath => vec![],
            })
            .collect::<Vec<_>>()
    };
    let kinds = |path: &BezPath| {
        path.elements()
            .iter()
            .map(std::mem::discriminant)
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds(actual), kinds(expected));
    for (actual, expected) in points(actual).into_iter().zip(points(expected)) {
        assert!(
            actual.distance(expected) < 1e-9,
            "{actual:?} != {expected:?}"
        );
    }
}

/// Typst's own outline for `text`, placed like a readout.
fn typst_readout(
    registry: &gaanim_text::font::FontRegistry,
    text: &str,
    family: &str,
) -> (BezPath, f64) {
    let path =
        gaanim_text::typst_compiler::shape_typst_text_run(registry, text, family, None, 0.75)
            .unwrap()
            .path;
    let rect = path.bounding_box();
    let (path, _) = right_align_readout_path(path, Bounds3D::default());
    (path, -(rect.y0 + rect.y1) * 0.5)
}

#[test]
fn reactive_readout_resolves_family_like_scene_text() {
    let parameter_id = gaanim_core::ObjectId::from_raw(5);
    let mut app = App::new();
    app.insert_resource(gaanim_text::font::FontRegistry::new());
    app.add_systems(Update, reactive_readout_update_system);
    let signal = app.world_mut().spawn(FloatSignal::new(2.5)).id();
    let empty = Arc::new(BezPath::new());
    let readout = app
        .world_mut()
        .spawn((
            ReactiveReadout {
                source: crate::reactive::ScalarSource::signal(parameter_id),
                parameters: vec![(parameter_id, signal)],
                format: ".1f".to_owned(),
                prefix: String::new(),
                suffix: String::new(),
                invalid: "—".to_owned(),
                decimal_separator: '.',
                font_family: "Libertinus Serif".to_owned(),
                font_weight: None,
                font_size: 0.75,
                last_text: String::new(),
                last_path: empty.clone(),
                last_bounds: Bounds3D::default(),
                reserve: 0.0,
                reserve_advance: 0.0,
            },
            Path2D(empty.clone()),
            PathSource(empty),
            LocalBounds(Bounds3D::default()),
            TextBaseline::default(),
        ))
        .id();

    app.update();
    let registry = app.world().resource::<gaanim_text::font::FontRegistry>();

    // Libertinus is known to Typst but not by name to the legacy registry,
    // which silently fell back to a system sans face.
    let (expected, baseline) = typst_readout(registry, "2.5", "Libertinus Serif");
    assert_same_outline(
        &app.world().get::<PathSource>(readout).unwrap().0,
        &expected,
    );
    assert!((app.world().get::<TextBaseline>(readout).unwrap().0 - baseline).abs() < 1e-9);
    if let Ok((legacy, bounds)) =
        gaanim_text::shaper::compile_text_to_path(registry, "2.5", "Libertinus Serif", 0.75)
    {
        let (legacy, _) = right_align_readout_path(legacy, bounds);
        assert_ne!(legacy.bounding_box(), expected.bounding_box());
    }

    // Fixed prefix/suffix runs and padded numbers keep Typst's advances.
    let (path, bounds) =
        shape_readout_text(registry, "x = ", "  2.5", " m", "Libertinus Serif", 0.75).unwrap();
    let (path, _) = right_align_readout_path(path, bounds);
    assert_same_outline(
        &path,
        &typst_readout(registry, "x =   2.5 m", "Libertinus Serif").0,
    );
}

#[test]
fn reactive_readout_layout_preserves_equal_gaps_when_number_width_changes() {
    let label_id = gaanim_core::ObjectId::from_raw(11);
    let equals_id = gaanim_core::ObjectId::from_raw(12);
    let number_id = gaanim_core::ObjectId::from_raw(13);
    let unit_id = gaanim_core::ObjectId::from_raw(14);
    let mut app = App::new();
    app.add_systems(Update, reactive_readout_layout_system);

    let spawn_part = |app: &mut App, id, width: f64, min_y: f64, max_y: f64, baseline: f64| {
        app.world_mut()
            .spawn((
                MobjectId(id),
                LocalBounds(Bounds3D::new_2d(0.0, min_y, width, max_y)),
                TextBaseline(baseline),
                SpatialTransform::default(),
            ))
            .id()
    };
    let label = spawn_part(&mut app, label_id, 20.0, -9.0, 19.0, -1.0);
    let equals = spawn_part(&mut app, equals_id, 12.0, -5.0, 7.0, 1.0);
    let number = spawn_part(&mut app, number_id, 40.0, -12.0, 12.0, -3.0);
    let unit = spawn_part(&mut app, unit_id, 18.0, -7.0, 23.0, 2.0);
    app.world_mut().spawn(ReactiveReadoutLayout {
        label: Some(label_id),
        equals: Some(equals_id),
        number: number_id,
        unit: Some(unit_id),
        spacing: 10.0,
        align: 0.0,
    });

    app.update();

    let visual_edges = |app: &App, entity| {
        let bounds = app.world().get::<LocalBounds>(entity).unwrap().0;
        let transform = app.world().get::<SpatialTransform>(entity).unwrap();
        (
            bounds.min.x + transform.translation.x,
            bounds.max.x + transform.translation.x,
            bounds.min.y + transform.translation.y,
            bounds.max.y + transform.translation.y,
        )
    };
    let label_edges = visual_edges(&app, label);
    let equals_edges = visual_edges(&app, equals);
    let initial_number_edges = visual_edges(&app, number);
    let initial_unit_edges = visual_edges(&app, unit);
    assert_eq!(equals_edges.0 - label_edges.1, 10.0);
    assert_eq!(initial_number_edges.0 - equals_edges.1, 10.0);
    assert_eq!(initial_unit_edges.0 - initial_number_edges.1, 10.0);
    let world_baseline = |app: &App, entity| {
        app.world().get::<TextBaseline>(entity).unwrap().0
            + app
                .world()
                .get::<SpatialTransform>(entity)
                .unwrap()
                .translation
                .y
    };
    let expected_baseline = world_baseline(&app, number);
    assert_eq!(world_baseline(&app, label), expected_baseline);
    assert_eq!(world_baseline(&app, equals), expected_baseline);
    assert_eq!(world_baseline(&app, unit), expected_baseline);

    app.world_mut()
        .entity_mut(number)
        .insert(LocalBounds(Bounds3D::new_2d(0.0, 0.0, 70.0, 24.0)));
    app.update();

    let label_edges = visual_edges(&app, label);
    let equals_edges = visual_edges(&app, equals);
    let number_edges = visual_edges(&app, number);
    let unit_edges = visual_edges(&app, unit);
    assert_eq!(equals_edges.0 - label_edges.1, 10.0);
    assert_eq!(number_edges.0 - equals_edges.1, 10.0);
    assert_eq!(unit_edges.0 - number_edges.1, 10.0);
    assert_eq!(label_edges.0, -75.0);
    assert_eq!(unit_edges.1, 75.0);
}

#[test]
fn regenerated_lines_keep_the_part_a_reveal_shows() {
    let strip = |y: f32| gaanim_scene::LineListData {
        points: vec![[0.0, y, 0.0], [1.0, y, 0.0], [2.0, y, 0.0]],
        indices: None,
        strip: true,
        color: Color::BLACK,
        colors: None,
        revealed: None,
    };
    let mut world = World::new();
    let old = strip(0.0);
    let half = crate::tween::trim_line_strip(&old, 0.0, 0.5, 0.0).unwrap();
    assert_eq!(half.revealed, Some([0.0, 0.5]));
    let line = world
        .spawn((
            half,
            gaanim_scene::LineListSource(old),
            ReactiveLineRegen::new(move |_| strip(1.0)),
        ))
        .id();
    reactive_3d_regen_system(&mut world);
    let source = &world.get::<gaanim_scene::LineListSource>(line).unwrap().0;
    assert_eq!(source.points[2], [2.0, 1.0, 0.0]);
    let shown = world.get::<gaanim_scene::LineListData>(line).unwrap();
    assert_eq!(shown.revealed, Some([0.0, 0.5]));
    assert_eq!(shown.points.last(), Some(&[1.0, 1.0, 0.0]));
}

#[test]
fn reactive_readout_layout_spaces_parts_by_their_advance() {
    let ids = [31, 32, 33].map(gaanim_core::ObjectId::from_raw);
    let mut app = App::new();
    app.add_systems(Update, reactive_readout_layout_system);
    // Ink 10 wide inside advances that leave 1, 4 and 3 units beside it.
    let parts = [(0.0, 1.0), (4.0, 1.0), (3.0, 0.0)].map(|(left, right)| {
        (
            LocalBounds(Bounds3D::new_2d(0.0, -5.0, 10.0, 5.0)),
            TextAdvance {
                left: -left,
                right: 10.0 + right,
            },
        )
    });
    let entities = ids
        .iter()
        .zip(parts)
        .map(|(id, (bounds, advance))| {
            app.world_mut()
                .spawn((MobjectId(*id), bounds, advance, SpatialTransform::default()))
                .id()
        })
        .collect::<Vec<_>>();
    app.world_mut().spawn(ReactiveReadoutLayout {
        label: Some(ids[0]),
        equals: Some(ids[1]),
        number: ids[2],
        unit: None,
        spacing: 2.0,
        align: 0.0,
    });
    app.update();
    let ink = |entity| {
        let x = app
            .world()
            .get::<SpatialTransform>(entity)
            .unwrap()
            .translation
            .x;
        (x, x + 10.0)
    };
    let [label, equals, number] = [0, 1, 2].map(|index| ink(entities[index]));
    // Each gap is the spacing plus the room beside the ink on both sides.
    assert_eq!(equals.0 - label.1, 1.0 + 2.0 + 4.0);
    assert_eq!(number.0 - equals.1, 1.0 + 2.0 + 3.0);
    // The row's ink is centered.
    assert_eq!(label.0, -number.1);
}

#[test]
fn readout_numbers_keep_their_advance_and_reserve() {
    // Ink from 1 to 9 of a 10 long pen advance, ended at x = 0.
    let ink = Bounds3D::new_2d(1.0, 0.0, 9.0, 1.0);
    assert_eq!(
        readout_advance(ink, 10.0, 0.0),
        TextAdvance {
            left: -9.0,
            right: 1.0
        }
    );
    assert_eq!(readout_advance(ink, 10.0, 15.0).left, -14.0);
    // Ink that overhangs the pen widens the box to it.
    let overhang = Bounds3D::new_2d(-1.0, 0.0, 11.0, 1.0);
    assert_eq!(
        readout_advance(overhang, 10.0, 0.0),
        TextAdvance {
            left: -12.0,
            right: 0.0
        }
    );
}

#[test]
fn reactive_readout_layout_aligns_a_changing_number_on_its_start() {
    let number_id = gaanim_core::ObjectId::from_raw(21);
    let mut app = App::new();
    app.add_systems(Update, reactive_readout_layout_system);
    // Rolling numbers are right anchored: their outline ends at x = 0.
    let number = app
        .world_mut()
        .spawn((
            MobjectId(number_id),
            LocalBounds(Bounds3D::new_2d(-30.0, -5.0, 0.0, 5.0)),
            SpatialTransform::default(),
        ))
        .id();
    app.world_mut().spawn(ReactiveReadoutLayout {
        label: None,
        equals: None,
        number: number_id,
        unit: None,
        spacing: 0.0,
        align: -1.0,
    });
    let start = |app: &App| {
        app.world().get::<LocalBounds>(number).unwrap().0.min.x
            + app
                .world()
                .get::<SpatialTransform>(number)
                .unwrap()
                .translation
                .x
    };
    app.update();
    assert_eq!(start(&app), 0.0);
    app.world_mut()
        .entity_mut(number)
        .insert(LocalBounds(Bounds3D::new_2d(-80.0, -5.0, 0.0, 5.0)));
    app.update();
    assert_eq!(start(&app), 0.0);
}

#[test]
fn point_on_curve_uses_normalized_arc_length_and_clamps_tracker() {
    let mut path = BezPath::new();
    path.move_to(Point::new(0.0, 0.0));
    path.line_to(Point::new(100.0, 0.0));
    path.line_to(Point::new(100.0, 300.0));

    let mut world = World::new();
    let curve = world.spawn(gaanim_scene::Path2D(Arc::new(path))).id();
    let tracker = world.spawn(FloatSignal::new(0.5)).id();
    let fraction =
        crate::updaters::TrackingScalar::signal(gaanim_core::ObjectId::from_raw(1), tracker);
    let target = world
        .spawn((
            SpatialTransform::default(),
            PointOnCurve::new(curve, fraction.clone()),
        ))
        .id();

    point_on_curve_system(&mut world);
    assert_eq!(
        world.get::<SpatialTransform>(target).unwrap().translation,
        DVec3::new(100.0, 100.0, 0.0),
    );

    world.get_mut::<FloatSignal>(tracker).unwrap().value = 0.5;
    let tangent = world
        .spawn((
            SpatialTransform::default(),
            TangentOnCurve::new(curve, fraction.clone()),
        ))
        .id();
    tangent_on_curve_system(&mut world);
    let transform = world.get::<SpatialTransform>(tangent).unwrap();
    assert_eq!(transform.translation, DVec3::new(100.0, 100.0, 0.0));
    let direction = transform.rotation * DVec3::X;
    assert!(direction.x.abs() < 1e-9 && (direction.y - 1.0).abs() < 1e-9);

    let normal = world
        .spawn((
            SpatialTransform::default(),
            NormalOnCurve::new(curve, fraction.clone()),
        ))
        .id();
    normal_on_curve_system(&mut world);
    let direction = world.get::<SpatialTransform>(normal).unwrap().rotation * DVec3::X;
    assert!((direction.x + 1.0).abs() < 1e-9 && direction.y.abs() < 1e-9);

    let mut cubic = BezPath::new();
    cubic.move_to(Point::new(0.0, 0.0));
    cubic.curve_to(
        Point::new(0.0, 120.0),
        Point::new(120.0, 120.0),
        Point::new(120.0, 0.0),
    );
    assert_eq!(
        point_at_polyline_fraction(&cubic, 0.0),
        Some(Point::new(0.0, 0.0))
    );
    let end = point_at_polyline_fraction(&cubic, 1.0).expect("cubic endpoint");
    assert!((end.x - 120.0).abs() < 1e-9 && end.y.abs() < 1e-9);

    world.get_mut::<FloatSignal>(tracker).unwrap().value = 2.0;
    point_on_curve_system(&mut world);
    assert_eq!(
        world.get::<SpatialTransform>(target).unwrap().translation,
        DVec3::new(100.0, 300.0, 0.0),
    );
}

/// A curve moved to (3, 1) and turned a quarter carries its markers; a
/// marker inside a group is placed in the group's space.
#[test]
fn curve_markers_follow_the_curve_as_drawn() {
    let mut path = BezPath::new();
    path.move_to(Point::new(0.0, 0.0));
    path.line_to(Point::new(2.0, 0.0));

    let mut world = World::new();
    let moved = SpatialTransform {
        translation: DVec3::new(3.0, 1.0, 0.0),
        rotation: gaanim_core::glam::DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
        ..Default::default()
    };
    let curve = world
        .spawn((gaanim_scene::Path2D(Arc::new(path)), moved))
        .id();
    let half = crate::updaters::TrackingScalar::from(0.5);
    let free = world
        .spawn((
            SpatialTransform::default(),
            PointOnCurve::new(curve, half.clone()),
        ))
        .id();
    let shifted = SpatialTransform {
        translation: DVec3::new(1.0, 0.0, 0.0),
        ..Default::default()
    };
    let group = world.spawn(shifted).id();
    let grouped = world
        .spawn((
            SpatialTransform::default(),
            TangentOnCurve::new(curve, half),
        ))
        .id();
    world
        .entity_mut(grouped)
        .insert(bevy::prelude::ChildOf(group));

    point_on_curve_system(&mut world);
    tangent_on_curve_system(&mut world);
    let close = |a: DVec3, b: DVec3| (a - b).length() < 1e-9;
    let point = world.get::<SpatialTransform>(free).unwrap().translation;
    assert!(close(point, DVec3::new(3.0, 2.0, 0.0)), "{point:?}");
    let tangent = world.get::<SpatialTransform>(grouped).unwrap();
    assert!(
        close(tangent.translation, DVec3::new(2.0, 2.0, 0.0)),
        "{tangent:?}"
    );
    let direction = tangent.rotation * DVec3::X;
    assert!(close(direction, DVec3::Y), "{direction:?}");
}

/// At either end of an open curve, and across a closed curve's seam, the
/// osculating circle is still there: the samples stay on the curve.
#[test]
fn osculating_circles_reach_the_ends_and_the_seam() {
    // A quarter of a circle of radius 2, open.
    let mut quarter = BezPath::new();
    quarter.move_to(Point::new(2.0, 0.0));
    for step in 1..=256 {
        let angle = std::f64::consts::FRAC_PI_2 * step as f64 / 256.0;
        quarter.line_to(Point::new(2.0 * angle.cos(), 2.0 * angle.sin()));
    }
    for fraction in [0.0, 1.0] {
        let (center, radius) = osculating_circle(&quarter, fraction, 0.1).unwrap();
        assert!(center.to_vec2().hypot() < 1e-2 && (radius - 2.0).abs() < 1e-2);
    }
    let circle = gaanim_core::kurbo::Circle::new(Point::ZERO, 1.5).to_path(1e-4);
    for fraction in [0.0, 1.0] {
        let (center, radius) = osculating_circle(&circle, fraction, 0.02).unwrap();
        // Within the flattening of its Bézier arcs.
        assert!(center.to_vec2().hypot() < 5e-2 && (radius - 1.5).abs() < 5e-2);
    }
    let mut straight = BezPath::new();
    straight.move_to(Point::ZERO);
    straight.line_to(Point::new(4.0, 0.0));
    assert!(osculating_circle(&straight, 0.0, 0.2).is_none());
}
