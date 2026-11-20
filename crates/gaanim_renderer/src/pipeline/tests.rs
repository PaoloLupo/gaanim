use super::*;

fn rect_path(x0: f64, y0: f64, x1: f64, y1: f64) -> Arc<kurbo::BezPath> {
    Arc::new(kurbo::Rect::new(x0, y0, x1, y1).to_path(0.1))
}

/// Frames captured while a mesh is unchanged hold it in the same `Arc`,
/// by which recordings keep its upload and its table entry; a changed
/// mesh, even changed straight after a capture, is captured anew.
#[test]
fn captures_keep_unchanged_meshes_in_the_same_arc() {
    let mut world = World::new();
    let mesh = gaanim_scene::TriangleMeshData {
        vertices: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
        normals: None,
        uvs: None,
        color: None,
        colors: None,
        material: None,
        surface: None,
        blend: None,
    };
    let entity = world
        .spawn((
            mesh,
            gaanim_scene::Mesh3DMarker,
            Visible,
            GlobalSpatialTransform::default(),
            GlobalOpacity(1.0),
        ))
        .id();
    let camera = gaanim_math::Camera::ortho_2d(1280, 720);
    let captured = |world: &mut World| {
        let three_d = capture_frame(world, Some(&camera)).three_d.unwrap();
        three_d.sources[0].mesh.clone().unwrap()
    };
    let first = captured(&mut world);
    assert!(Arc::ptr_eq(&first, &captured(&mut world)));
    world
        .get_mut::<gaanim_scene::TriangleMeshData>(entity)
        .unwrap()
        .indices = vec![0, 2, 1];
    let changed = captured(&mut world);
    assert!(!Arc::ptr_eq(&first, &changed));
    assert_eq!(changed.indices, [0, 2, 1]);
    assert!(Arc::ptr_eq(&changed, &captured(&mut world)));
}

/// A light shades from where its entity is, dimmed by its opacity.
#[test]
fn illumination_places_lights_and_dims_them_by_opacity() {
    let light = gaanim_scene::Light3D {
        kind: gaanim_scene::LightKind3D::Point,
        color: peniko::Color::WHITE,
        intensity: 2.0,
    };
    let at = gaanim_math::SpatialTransform::new_2d(3.0, -1.0);
    let transform = GlobalSpatialTransform::from_local(&at);
    let lit = illumination(
        None,
        None,
        None,
        [(&light, &transform, &GlobalOpacity(0.5))].into_iter(),
    );
    assert_eq!(lit.lights.len(), 1);
    let shown = lit.lights[0];
    assert_eq!(
        shown.position,
        gaanim_core::glam::DVec3::new(3.0, -1.0, 0.0)
    );
    assert!(!shown.directional);
    assert!((shown.color[0] - 1.0).abs() < 1e-6, "{shown:?}");
    let hidden = illumination(
        None,
        None,
        None,
        [(&light, &transform, &GlobalOpacity(0.0))].into_iter(),
    );
    assert!(hidden.lights.is_empty());
}

#[test]
fn blur_taps_sample_a_bounded_gaussian_with_a_solid_core() {
    for sigma in [0.001, 0.05, 0.3, 4.0] {
        let taps: Vec<_> = blur_taps(sigma, 0.8).collect();
        assert!((13..=160).contains(&taps.len()), "{} taps", taps.len());
        let max_radius = taps
            .iter()
            .map(|((x, y), _)| x.hypot(*y))
            .fold(0.0, f64::max);
        assert!(max_radius <= BLUR_TAP_RADIUS * sigma + 1e-9);
        let core = 1.0 - taps.iter().map(|(_, a)| 1.0 - a).product::<f32>();
        assert!(
            (core - BLUR_CORE_COVERAGE * 0.8).abs() < 1e-3,
            "core {core}"
        );
    }
    // Wider blurs use more taps so neighbouring copies stay close together.
    assert!(blur_taps(0.3, 1.0).count() > blur_taps(0.02, 1.0).count());
}

#[test]
fn group_z_index_lifts_its_whole_subtree() {
    // Regression for #25: group([box, text]).z_index(5) must draw above a
    // root circle with z_index(2); glyphs inside a text with z 6 must stay
    // above a sibling box with z 5.
    let mut world = World::new();
    let order = |z_index, creation_order| RenderOrder {
        z_index,
        creation_order,
    };
    let group = world.spawn(order(5, 1)).id();
    let text = world.spawn((order(1, 3), ChildOf(group))).id();
    let glyph = world.spawn((order(0, 4), ChildOf(text))).id();
    let boxed = world.spawn((order(0, 2), ChildOf(group))).id();

    let stacked = |world: &World, entity: Entity| {
        stacked_render_order(
            *world.get::<RenderOrder>(entity).unwrap(),
            entity,
            |e| world.get::<ChildOf>(e).map(ChildOf::parent),
            |e| world.get::<RenderOrder>(e).map_or(0, |order| order.z_index),
        )
    };
    let circle = order(2, 5);
    assert_eq!(stacked(&world, boxed), order(5, 2));
    assert_eq!(stacked(&world, glyph), order(6, 4));
    assert!(stacked(&world, boxed).z_index > circle.z_index);
    assert!(stacked(&world, glyph).z_index > stacked(&world, boxed).z_index);
}

#[test]
fn a_named_layer_draws_above_every_z_index_behind_it() {
    let mut world = World::new();
    let overlay = world.spawn(gaanim_scene::ZLayer(1)).id();
    let inside = world.spawn(ChildOf(overlay)).id();
    let own = world
        .spawn((gaanim_scene::ZLayer(-1), ChildOf(overlay)))
        .id();
    let plain = world.spawn_empty().id();
    let layer = |entity| {
        inherited_z_layer(
            entity,
            |e| world.get::<ChildOf>(e).map(ChildOf::parent),
            |e| world.get::<gaanim_scene::ZLayer>(e).map(|layer| layer.0),
        )
    };
    assert_eq!([layer(inside), layer(own), layer(plain)], [1, -1, 0]);

    let mut lens = group_member(inside, 0.0, 1.0, 1.0);
    lens.z_layer = 1;
    lens.render_order.z_index = -5;
    let mut wall = group_member(plain, 1.0, 1.0, 1.0);
    wall.render_order.z_index = 400;
    assert_eq!(
        ExtractedElement::draw_order(&lens, &wall),
        std::cmp::Ordering::Greater
    );
    wall.z_layer = 1;
    assert_eq!(
        ExtractedElement::draw_order(&lens, &wall),
        std::cmp::Ordering::Less
    );
}

fn assert_affine_near(actual: kurbo::Affine, expected: kurbo::Affine) {
    let (actual, expected) = (actual.as_coeffs(), expected.as_coeffs());
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, e)| (a - e).abs() < 1e-9),
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn camera_view_transform_maps_the_framed_region_onto_the_screen() {
    let unit = |w: f64, h: f64| kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0);
    let screen = (unit(4.0, 2.0), kurbo::Affine::translate((-4.0, -1.0)));

    let source = (unit(2.0, 1.0), kurbo::Affine::translate((3.0, 2.0)));
    let content = camera_view_transform(source, screen, CameraViewFit::Contain).unwrap();
    assert_eq!(
        content * kurbo::Point::new(3.0, 2.0),
        kurbo::Point::new(-4.0, -1.0)
    );
    assert_eq!(
        content * kurbo::Point::new(4.0, 2.5),
        kurbo::Point::new(-2.0, 0.0)
    );

    // Shrinking the source zooms in; rotating it turns the view the other way.
    let zoomed = (source.0, source.1 * kurbo::Affine::scale(0.5));
    let zoomed = camera_view_transform(zoomed, screen, CameraViewFit::Contain).unwrap();
    assert_affine_near(
        zoomed,
        kurbo::Affine::translate((-4.0, -1.0))
            * kurbo::Affine::scale(4.0)
            * kurbo::Affine::translate((-3.0, -2.0)),
    );
    let turned = (source.0, source.1 * kurbo::Affine::rotate(0.5));
    let turned = camera_view_transform(turned, screen, CameraViewFit::Contain).unwrap();
    assert_affine_near(
        turned,
        kurbo::Affine::translate((-4.0, -1.0))
            * kurbo::Affine::scale(2.0)
            * kurbo::Affine::rotate(-0.5)
            * kurbo::Affine::translate((-3.0, -2.0)),
    );

    // A square source on a 4×2 screen: contain keeps it whole, cover
    // fills the screen and stretch does both by distorting it.
    let square = (unit(1.0, 1.0), kurbo::Affine::IDENTITY);
    let scale_of = |fit| {
        let [a, _, _, d, _, _] = camera_view_transform(square, screen, fit)
            .unwrap()
            .as_coeffs();
        (a, d)
    };
    assert_eq!(scale_of(CameraViewFit::Contain), (2.0, 2.0));
    assert_eq!(scale_of(CameraViewFit::Cover), (4.0, 4.0));
    assert_eq!(scale_of(CameraViewFit::Stretch), (4.0, 2.0));

    let line = (
        kurbo::Rect::new(0.0, 0.0, 3.0, 0.0),
        kurbo::Affine::IDENTITY,
    );
    assert!(camera_view_transform(line, screen, CameraViewFit::Contain).is_none());
    let flat = (unit(1.0, 1.0), kurbo::Affine::scale_non_uniform(1.0, 0.0));
    assert!(camera_view_transform(flat, screen, CameraViewFit::Contain).is_none());
}

/// A world with a framed square, the frame, an excluded marker and a
/// stroked screen showing the frame's view at twice its size.
fn centered_rect(w: f64, h: f64) -> kurbo::Rect {
    kurbo::Rect::new(-w / 2.0, -h / 2.0, w / 2.0, h / 2.0)
}

/// A filled rectangle drawable at `at`, created `order`-th.
fn spawn_shape(
    world: &mut World,
    id: u64,
    at: (f64, f64),
    shape: kurbo::Rect,
    order: u64,
) -> Entity {
    let local = gaanim_math::SpatialTransform::new_2d(at.0, at.1);
    world
        .spawn((
            MobjectId(ObjectId::from_raw(id)),
            local,
            GlobalSpatialTransform::from_local(&local),
            GlobalOpacity(1.0),
            RenderOrder {
                z_index: 0,
                creation_order: order,
            },
            RenderLayer::Vello2D,
            Path2D(Arc::new(shape.to_path(0.01))),
            LocalBounds(gaanim_math::Bounds3D::new_2d(
                shape.x0, shape.y0, shape.x1, shape.y1,
            )),
            WorldBounds(gaanim_math::Bounds3D::new_2d(
                shape.x0 + at.0,
                shape.y0 + at.1,
                shape.x1 + at.0,
                shape.y1 + at.1,
            )),
            FillBrush(Some(peniko::Brush::Solid(peniko::Color::WHITE))),
            Visible,
        ))
        .id()
}

/// A world with a framed square, the frame, an excluded marker and a
/// stroked screen showing the frame's view at twice its size.
fn camera_view_world(world: &mut World) -> Entity {
    spawn_shape(world, 1, (3.5, 2.0), centered_rect(0.4, 0.4), 1);
    let frame = spawn_shape(world, 2, (3.0, 2.0), centered_rect(2.0, 1.0), 2);
    let marker = spawn_shape(world, 3, (2.5, 2.0), centered_rect(0.2, 0.2), 3);
    let screen = spawn_shape(world, 4, (-4.0, -1.0), centered_rect(4.0, 2.0), 4);
    world.entity_mut(screen).insert((
        StrokeBrush::new(peniko::Color::BLACK, 0.05),
        CameraView {
            source: frame,
            fit: CameraViewFit::Contain,
            background: CameraViewBackground::None,
            exclude: vec![marker],
            zoom: None,
            layers: Vec::new(),
            since: 0.0,
            until: None,
            camera_3d: None,
        },
    ));
    screen
}

/// Adds a bone on the `xray` view layer inside the frame and a second
/// screen whose camera looks at the frame's center with the zoom of a
/// float signal and shows that layer.
fn explicit_zoom_world(world: &mut World, zoom: f64) -> (Entity, Entity) {
    let frame = world
        .query::<(Entity, &MobjectId)>()
        .iter(world)
        .find(|(_, id)| id.0 == ObjectId::from_raw(2))
        .unwrap()
        .0;
    let bone = spawn_shape(world, 5, (3.2, 1.8), centered_rect(0.2, 0.2), 5);
    world.entity_mut(bone).insert(ViewLayer(Arc::from("xray")));
    let signal_id = ObjectId::from_raw(50);
    let signal = world
        .spawn((
            MobjectId(signal_id),
            gaanim_animation::FloatSignal::new(zoom),
        ))
        .id();
    let lens = spawn_shape(world, 6, (4.0, -3.0), centered_rect(2.0, 1.0), 6);
    world.entity_mut(lens).insert(CameraView {
        source: frame,
        fit: CameraViewFit::Contain,
        background: CameraViewBackground::None,
        exclude: Vec::new(),
        zoom: Some(gaanim_animation::TrackingScalar {
            source: gaanim_animation::ScalarSource::Signal(signal_id),
            parameters: vec![(signal_id, signal)],
        }),
        layers: vec![Arc::from("xray")],
        since: 0.0,
        until: None,
        camera_3d: None,
    });
    (lens, signal)
}

/// Draw positions of the encoded transforms equal to `affine`.
fn transform_indices(scene: &vello::Scene, affine: kurbo::Affine) -> Vec<usize> {
    let expected = affine.as_coeffs();
    scene
        .encoding()
        .transforms
        .iter()
        .enumerate()
        .filter(|(_, transform)| {
            transform
                .matrix
                .iter()
                .chain(&transform.translation)
                .zip(expected)
                .all(|(a, e)| (f64::from(*a) - e).abs() < 1e-5)
        })
        .map(|(index, _)| index)
        .collect()
}

fn transform_index(scene: &vello::Scene, affine: kurbo::Affine) -> Option<usize> {
    transform_indices(scene, affine).first().copied()
}

#[test]
fn camera_views_draw_what_their_source_frames_below_the_screen_stroke() {
    let mut world = World::new();
    camera_view_world(&mut world);
    let scene = compile_scene_from_world(&mut world, None);
    let view = kurbo::Affine::translate((-4.0, -1.0))
        * kurbo::Affine::scale(2.0)
        * kurbo::Affine::translate((-3.0, -2.0));
    let screen = kurbo::Affine::translate((-4.0, -1.0));

    let seen = transform_index(&scene, view * kurbo::Affine::translate((3.5, 2.0)))
        .expect("the framed square is drawn inside the screen");
    let stroke = *transform_indices(&scene, screen).last().unwrap();
    assert!(seen < stroke, "the screen stroke must cover the view");
    // The frame, the screen and excluded subtrees stay out of the view.
    for hidden in [(3.0, 2.0), (2.5, 2.0), (-4.0, -1.0)] {
        assert!(
            transform_index(&scene, view * kurbo::Affine::translate(hidden)).is_none(),
            "{hidden:?} is visible in the view"
        );
    }
}

#[test]
fn camera_views_follow_morphed_screen_and_frame_geometry() {
    let mut world = World::new();
    let screen = camera_view_world(&mut world);
    let frame = world
        .query::<(Entity, &MobjectId)>()
        .iter(&world)
        .find(|(_, id)| id.0 == ObjectId::from_raw(2))
        .unwrap()
        .0;
    // A morph rewrites the geometry but not the spawn bounds: the frame
    // halves to 1 × 0.5 and the screen doubles to 8 × 4.
    let morph = |shape: kurbo::Rect| {
        let path = Arc::new(shape.to_path(0.01));
        (Path2D(Arc::clone(&path)), PathSource(path))
    };
    world
        .entity_mut(frame)
        .insert(morph(centered_rect(1.0, 0.5)));
    world
        .entity_mut(screen)
        .insert(morph(centered_rect(8.0, 4.0)));
    let scene = compile_scene_from_world(&mut world, None);
    let view = kurbo::Affine::translate((-4.0, -1.0))
        * kurbo::Affine::scale(8.0)
        * kurbo::Affine::translate((-3.0, -2.0));
    assert!(
        transform_index(&scene, view * kurbo::Affine::translate((3.5, 2.0))).is_some(),
        "the view zooms by the morphed sizes"
    );
}

#[test]
fn camera_views_keep_what_they_see_from_main_camera_culling() {
    let mut world = World::new();
    camera_view_world(&mut world);
    // The main camera looks only at the screen; the framed square lies
    // beyond its culling margin.
    let mut camera = gaanim_math::Camera::ortho_2d_frame(4.0, 2.25, 640, 360);
    camera.position = gaanim_core::glam::DVec3::new(-4.0, -1.0, 0.0);
    let view = kurbo::Affine::translate((-4.0, -1.0))
        * kurbo::Affine::scale(2.0)
        * kurbo::Affine::translate((-3.0, -2.0));
    let scene = compile_scene_from_world(&mut world, Some(&camera));
    assert!(transform_index(&scene, view * kurbo::Affine::translate((3.5, 2.0))).is_some());
}

#[test]
fn explicit_zoom_looks_at_the_source_center_with_its_rotation() {
    let screen = (
        centered_rect(4.0, 2.0),
        kurbo::Affine::translate((-4.0, -1.0)),
    );
    let source_affine = kurbo::Affine::translate((3.0, 2.0)) * kurbo::Affine::rotate(0.5);
    let source = (kurbo::Rect::new(0.0, 0.0, 2.0, 2.0), source_affine);
    let center = source_affine * kurbo::Point::new(1.0, 1.0);

    let content = explicit_camera_view_transform(source, screen, 3.0).unwrap();
    assert_affine_near(
        content,
        kurbo::Affine::translate((-4.0, -1.0))
            * kurbo::Affine::scale(3.0)
            * kurbo::Affine::rotate(-0.5)
            * kurbo::Affine::translate(-center.to_vec2()),
    );
    // The source's size plays no part.
    let larger = (kurbo::Rect::new(-1.0, -1.0, 3.0, 3.0), source_affine);
    assert_affine_near(
        explicit_camera_view_transform(larger, screen, 3.0).unwrap(),
        content,
    );
    // A screen shrunk to 1 / zoom shows the scene at its real size.
    let small = (screen.0, screen.1 * kurbo::Affine::scale(1.0 / 3.0));
    let real = explicit_camera_view_transform(source, small, 3.0).unwrap();
    assert!((real.determinant() - 1.0).abs() < 1e-9);
    for zoom in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(explicit_camera_view_transform(source, screen, zoom).is_none());
    }
}

#[test]
fn view_layers_draw_only_inside_the_camera_views_that_list_them() {
    let mut world = World::new();
    camera_view_world(&mut world);
    let (lens, _) = explicit_zoom_world(&mut world, 2.5);
    let scene = compile_scene_from_world(&mut world, None);
    let bone = kurbo::Affine::translate((3.2, 1.8));
    let frame_view = kurbo::Affine::translate((-4.0, -1.0))
        * kurbo::Affine::scale(2.0)
        * kurbo::Affine::translate((-3.0, -2.0));
    let lens_view = kurbo::Affine::translate((4.0, -3.0))
        * kurbo::Affine::scale(2.5)
        * kurbo::Affine::translate((-3.0, -2.0));

    assert!(
        transform_index(&scene, bone).is_none(),
        "drawn by the main camera"
    );
    assert!(
        transform_index(&scene, frame_view * bone).is_none(),
        "drawn without its layer"
    );
    assert!(
        transform_index(&scene, lens_view * bone).is_some(),
        "missing from its layer's view"
    );
    // The explicit view shows the ordinary scene as well.
    let square = kurbo::Affine::translate((3.5, 2.0));
    assert!(transform_index(&scene, lens_view * square).is_some());

    world
        .entity_mut(lens)
        .get_mut::<CameraView>()
        .unwrap()
        .layers
        .clear();
    let scene = compile_scene_from_world(&mut world, None);
    assert!(transform_index(&scene, lens_view * bone).is_none());
}

#[test]
fn retained_camera_view_screens_match_a_fresh_compile() {
    let mut app = App::new();
    let screen = camera_view_world(app.world_mut());
    let (_, signal) = explicit_zoom_world(app.world_mut(), 2.5);
    app.init_resource::<GaanimRenderCache>()
        .add_systems(Update, gaanim_render_system);
    let assert_fresh = |app: &mut App, label: &str| {
        app.update();
        let fresh = compile_scene_from_world(app.world_mut(), None);
        let live = app
            .world_mut()
            .query::<&VelloScene2d>()
            .single(app.world())
            .unwrap();
        assert!(
            live.encoding().path_data == fresh.encoding().path_data
                && live.encoding().draw_data == fresh.encoding().draw_data
                && live.encoding().transforms == fresh.encoding().transforms,
            "{label}: retained frame differs from a fresh compile"
        );
    };
    assert_fresh(&mut app, "screen");
    assert_fresh(&mut app, "retained screen");
    app.world_mut()
        .get_mut::<gaanim_animation::FloatSignal>(signal)
        .unwrap()
        .value = 4.0;
    assert_fresh(&mut app, "zoom changed");
    // Ending the view returns the stroke to the retained fragment.
    app.world_mut().entity_mut(screen).remove::<CameraView>();
    assert_fresh(&mut app, "plain shape");
}

#[test]
fn snapshots_draw_the_view_they_froze_live_as_in_a_fresh_compile() {
    let mut app = App::new();
    let screen = camera_view_world(app.world_mut());
    let mut entities = FrozenEntities::default();
    let frozen = freeze_camera_view(
        app.world_mut(),
        screen,
        &[],
        Arc::from("\u{1}snapshot"),
        &mut entities,
    )
    .expect("the screen shows its view");
    assert_eq!(frozen.len(), 1, "the view shows the framed square only");
    // The framed square moves away after the snapshot.
    let square = app
        .world_mut()
        .query::<(Entity, &MobjectId)>()
        .iter(app.world())
        .find(|(_, id)| id.0 == ObjectId::from_raw(1))
        .unwrap()
        .0;
    let moved = gaanim_math::SpatialTransform::new_2d(3.0, 1.0);
    app.world_mut()
        .entity_mut(square)
        .insert(GlobalSpatialTransform::from_local(&moved));
    let key = ObjectId::from_raw(90);
    let photo = spawn_shape(app.world_mut(), 7, (4.0, 3.0), centered_rect(4.0, 2.0), 7);
    app.world_mut().entity_mut(photo).insert((
        StrokeBrush::new(peniko::Color::BLACK, 0.05),
        CameraSnapshot {
            screen,
            at: 0.0,
            key,
        },
    ));
    app.world_mut()
        .insert_resource(FrozenViews::new([(key, frozen)]));
    app.init_resource::<GaanimRenderCache>()
        .add_systems(Update, gaanim_render_system);
    app.update();
    let fresh = compile_scene_from_world(app.world_mut(), None);
    let live = app
        .world_mut()
        .query::<&VelloScene2d>()
        .single(app.world())
        .unwrap();
    assert!(
        live.encoding().path_data == fresh.encoding().path_data
            && live.encoding().draw_data == fresh.encoding().draw_data
            && live.encoding().transforms == fresh.encoding().transforms,
        "the live frame differs from a fresh compile"
    );
    // The snapshot shows the square where the view saw it, below its
    // stroke, carried by the snapshot's transform.
    let seen = kurbo::Affine::translate((4.0, 3.0))
        * kurbo::Affine::scale(2.0)
        * kurbo::Affine::translate((-3.0, -2.0));
    let frozen_square = transform_index(&fresh, seen * kurbo::Affine::translate((3.5, 2.0)))
        .expect("the snapshot draws the square it froze");
    let stroke = *transform_indices(&fresh, kurbo::Affine::translate((4.0, 3.0)))
        .last()
        .unwrap();
    assert!(
        frozen_square < stroke,
        "the snapshot stroke must cover its view"
    );
    assert!(
        transform_index(&fresh, seen * kurbo::Affine::translate((3.0, 1.0))).is_none(),
        "the snapshot does not follow the square"
    );
    // The frozen square stays out of the main camera.
    assert!(transform_index(&fresh, kurbo::Affine::translate((3.5, 2.0))).is_none());
}

#[test]
fn coordinate_view_keeps_the_stroke_pen_unscaled_in_headless_output() {
    let mut world = World::new();
    let view_local = gaanim_math::SpatialTransform::default().with_scale_2d(3.0, 1.0);
    let view = world
        .spawn((
            view_local,
            GlobalSpatialTransform::from_local(&view_local),
            gaanim_scene::CoordinateViewRole::View,
        ))
        .id();
    let path = kurbo::Line::new((2.0, 1.0), (2.0, 5.0)).to_path(0.01);
    world.spawn((
        MobjectId(ObjectId::from_raw(91)),
        gaanim_math::SpatialTransform::default(),
        GlobalSpatialTransform::from_local(&view_local),
        GlobalOpacity(1.0),
        RenderOrder::default(),
        RenderLayer::Vello2D,
        Path2D(Arc::new(path)),
        StrokeBrush::new(peniko::Color::BLACK, 0.03),
        Visible,
        ChildOf(view),
    ));
    let scene = compile_scene_from_world(&mut world, None);
    assert_eq!(
        scene.encoding().transforms[0].matrix,
        [1.0, 0.0, 0.0, 1.0],
        "the domain zoom must move the path without widening the stroke pen"
    );
}

#[test]
fn fragments_changed_while_hidden_are_rebuilt_when_shown_again() {
    let mut app = App::new();
    app.init_resource::<GaanimRenderCache>()
        .add_systems(Update, gaanim_render_system);
    let id = ObjectId::from_raw(93);
    let full = kurbo::BezPath::from_svg("M 0 0 L 4 0 L 4 3").unwrap();
    let entity = app
        .world_mut()
        .spawn((
            MobjectId(id),
            GlobalSpatialTransform::default(),
            GlobalOpacity(1.0),
            RenderOrder::default(),
            RenderLayer::Vello2D,
            Path2D(Arc::new(full)),
            StrokeBrush {
                brush: Some(peniko::Brush::Solid(peniko::Color::BLACK)),
                style: kurbo::Stroke::new(0.1),
            },
            Visible,
        ))
        .id();
    app.update();
    let drawn = app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();

    // A rewind hides the object and resets it to its undrawn state while
    // the render system does not visit it, as before a write/create clip.
    app.world_mut().entity_mut(entity).remove::<Visible>();
    app.update();
    app.world_mut()
        .entity_mut(entity)
        .insert(Path2D(Arc::new(kurbo::BezPath::new())));
    app.update();
    app.update();
    app.world_mut().entity_mut(entity).insert(Visible);
    app.update();

    let shown = &app.world().resource::<GaanimRenderCache>().fragment_cache[&id];
    assert!(!Arc::ptr_eq(&drawn, shown), "the drawn fragment was reused");
    let fresh = compile_scene_from_world(app.world_mut(), None);
    let live = app
        .world_mut()
        .query::<&VelloScene2d>()
        .single(app.world())
        .unwrap();
    assert_eq!(live.encoding().path_data, fresh.encoding().path_data);
}

#[test]
fn scaling_strokes_follow_the_scale_of_their_group() {
    use gaanim_math::SpatialTransform;
    let mut app = App::new();
    app.init_resource::<GaanimRenderCache>().add_systems(
        Update,
        (
            gaanim_scene::transform_propagation_system,
            gaanim_render_system,
        )
            .chain(),
    );
    let group = app
        .world_mut()
        .spawn((
            SpatialTransform::default().with_scale_2d(3.0, 3.0),
            GlobalSpatialTransform::default(),
        ))
        .id();
    let path = kurbo::BezPath::from_svg("M 0 0 L 1 1 L 2 0").unwrap();
    let style = kurbo::Stroke::new(0.04);
    let brush = peniko::Brush::Solid(peniko::Color::BLACK);
    let mut spawn = |raw: u64, scales: bool| {
        let id = ObjectId::from_raw(raw);
        let mut entity = app.world_mut().spawn((
            MobjectId(id),
            SpatialTransform::default(),
            GlobalSpatialTransform::default(),
            GlobalOpacity(1.0),
            RenderOrder::default(),
            RenderLayer::Vello2D,
            Path2D(Arc::new(path.clone())),
            StrokeBrush {
                brush: Some(brush.clone()),
                style: style.clone(),
            },
            Visible,
            ChildOf(group),
        ));
        if scales {
            entity.insert(StrokeScalesWithObject);
        }
        id
    };
    let fixed = spawn(93, false);
    let scaling = spawn(94, true);
    app.update();
    let cache = app.world().resource::<GaanimRenderCache>();
    // The scaling stroke keeps its local pen, which the group then
    // scales; the fixed one is drawn through the scene-unit correction.
    let mut local = vello::Scene::new();
    local.stroke(&style, kurbo::Affine::IDENTITY, &brush, None, &path);
    assert_eq!(
        cache.fragment_cache[&scaling].encoding().path_data,
        local.encoding().path_data
    );
    assert_ne!(
        cache.fragment_cache[&fixed].encoding().path_data,
        local.encoding().path_data
    );
    let headless = compile_scene_from_world(app.world_mut(), None);
    let live = app
        .world_mut()
        .query::<&VelloScene2d>()
        .single(app.world())
        .unwrap();
    assert_eq!(headless.encoding().path_data, live.encoding().path_data);
}

#[test]
fn coordinate_view_rebuilds_strokes_on_zoom_and_rewind_but_reuses_them_on_pan() {
    use gaanim_math::SpatialTransform;
    let mut app = App::new();
    app.init_resource::<GaanimRenderCache>().add_systems(
        Update,
        (
            gaanim_scene::transform_propagation_system,
            gaanim_render_system,
        )
            .chain(),
    );
    let view = app
        .world_mut()
        .spawn((
            SpatialTransform::default(),
            GlobalSpatialTransform::default(),
            gaanim_scene::CoordinateViewRole::View,
        ))
        .id();
    let id = ObjectId::from_raw(92);
    let path = kurbo::BezPath::from_svg("M 2 1 L 2 5 L 6 5 L 8 7 Q 9 9 6 10").unwrap();
    let style = kurbo::Stroke::new(0.04).with_dashes(0.1, [0.2, 0.1]);
    let brush = peniko::Brush::Solid(peniko::Color::BLACK);
    let entity = app
        .world_mut()
        .spawn((
            MobjectId(id),
            SpatialTransform::default(),
            GlobalSpatialTransform::default(),
            GlobalOpacity(1.0),
            RenderOrder::default(),
            RenderLayer::Vello2D,
            Path2D(Arc::new(path.clone())),
            StrokeBrush {
                brush: Some(brush.clone()),
                style: style.clone(),
            },
            Visible,
            ChildOf(view),
        ))
        .id();
    app.update();
    let original = app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();
    for (sx, sy) in [(3.0, 1.0), (1.0, 2.0), (2.0, 2.0), (1.0, 1.0), (3.0, 1.0)] {
        app.world_mut()
            .entity_mut(view)
            .insert(SpatialTransform::default().with_scale_2d(sx, sy));
        app.update();
        let fragment = app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();
        assert!(!Arc::ptr_eq(&original, &fragment));
        let expected_path = kurbo::Affine::scale_non_uniform(sx, sy) * &path;
        let mut expected = vello::Scene::new();
        expected.stroke(
            &style,
            kurbo::Affine::IDENTITY,
            &brush,
            None,
            &expected_path,
        );
        assert_eq!(
            fragment.encoding().path_data,
            expected.encoding().path_data,
            "dash lengths and curve geometry must be generated after domain zoom"
        );
        let headless = compile_scene_from_world(app.world_mut(), None);
        let live = app
            .world_mut()
            .query::<&VelloScene2d>()
            .single(app.world())
            .unwrap();
        assert_eq!(headless.encoding().path_data, live.encoding().path_data);
        assert_eq!(headless.encoding().transforms, live.encoding().transforms);
        assert_eq!(
            headless.encoding().transforms[0].matrix,
            [1.0, 0.0, 0.0, 1.0]
        );
        app.update();
        assert!(Arc::ptr_eq(
            &fragment,
            &app.world().resource::<GaanimRenderCache>().fragment_cache[&id]
        ));
        app.world_mut()
            .get_mut::<SpatialTransform>(view)
            .unwrap()
            .translation
            .x += 2.0;
        app.update();
        assert!(Arc::ptr_eq(
            &fragment,
            &app.world().resource::<GaanimRenderCache>().fragment_cache[&id]
        ));
    }
    app.world_mut().get_mut::<GlobalOpacity>(entity).unwrap().0 = 0.0;
    app.world_mut()
        .entity_mut(view)
        .insert(SpatialTransform::default());
    app.update();
    assert!(
        !app.world()
            .resource::<GaanimRenderCache>()
            .fragment_cache
            .contains_key(&id)
    );
    app.world_mut().get_mut::<GlobalOpacity>(entity).unwrap().0 = 1.0;
    app.update();
    let restored = &app.world().resource::<GaanimRenderCache>().fragment_cache[&id];
    assert_eq!(original.encoding().path_data, restored.encoding().path_data);
    assert_eq!(
        original.encoding().transforms,
        restored.encoding().transforms
    );
}

#[test]
fn draw_animation_lighting_preserves_logical_stroke_width() {
    let brush = peniko::Brush::Solid(peniko::Color::from_rgb8(40, 100, 240));
    let style = kurbo::Stroke::new(0.03);

    for wave in [0.0, 0.25, 0.5, 1.0] {
        let (_, animated_style) = animated_stroke_paint(&brush, &style, wave);
        assert_eq!(animated_style.width, style.width);
    }
}

#[test]
fn changing_fill_level_rebuilds_its_retained_fragment() {
    let mut app = App::new();
    app.init_resource::<GaanimRenderCache>()
        .add_systems(Update, gaanim_render_system);
    let id = ObjectId::from_raw(47);
    let entity = app
        .world_mut()
        .spawn((
            MobjectId(id),
            GlobalSpatialTransform::default(),
            GlobalOpacity(1.0),
            RenderOrder::default(),
            RenderLayer::Vello2D,
            Path2D(rect_path(0.0, 0.0, 20.0, 20.0)),
            PathSource(rect_path(0.0, 0.0, 20.0, 20.0)),
            FillBrush::color(peniko::Color::from_rgb8(251, 146, 60)),
            StrokeBrush::transparent(),
            FillLevel(0.0),
            Visible,
        ))
        .id();

    app.update();
    let first = app
        .world()
        .resource::<GaanimRenderCache>()
        .fragment_cache
        .get(&id)
        .expect("initial fragment")
        .clone();

    app.world_mut().get_mut::<FillLevel>(entity).unwrap().0 = 0.55;
    app.update();
    let second = app
        .world()
        .resource::<GaanimRenderCache>()
        .fragment_cache
        .get(&id)
        .expect("rebuilt fragment")
        .clone();

    assert!(
        !Arc::ptr_eq(&first, &second),
        "a retained fragment must not survive a fill-level change"
    );
}

#[test]
fn seek_replay_of_identical_values_keeps_the_retained_fragment() {
    let mut app = App::new();
    app.init_resource::<GaanimRenderCache>()
        .add_systems(Update, gaanim_render_system);
    let id = ObjectId::from_raw(49);
    let entity = app
        .world_mut()
        .spawn((
            MobjectId(id),
            GlobalSpatialTransform::default(),
            GlobalOpacity(1.0),
            RenderOrder::default(),
            RenderLayer::Vello2D,
            Path2D(rect_path(0.0, 0.0, 20.0, 20.0)),
            PathSource(rect_path(0.0, 0.0, 20.0, 20.0)),
            FillBrush::color(peniko::Color::WHITE),
            StrokeBrush::transparent(),
            Visible,
        ))
        .id();
    app.update();
    let fragment =
        |app: &App| app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();
    let retained = fragment(&app);

    // An exact seek restores the keyframe and replays completed clips,
    // rewriting equal values that Bevy still reports as changed.
    let mut entity_mut = app.world_mut().entity_mut(entity);
    entity_mut.get_mut::<Path2D>().unwrap().0 = rect_path(0.0, 0.0, 20.0, 20.0);
    entity_mut.get_mut::<PathSource>().unwrap().0 = rect_path(0.0, 0.0, 20.0, 20.0);
    entity_mut.insert(FillBrush::color(peniko::Color::WHITE));
    entity_mut.insert(StrokeBrush::transparent());
    app.update();
    assert!(Arc::ptr_eq(&retained, &fragment(&app)));

    app.world_mut().get_mut::<Path2D>(entity).unwrap().0 = rect_path(0.0, 0.0, 10.0, 20.0);
    app.update();
    assert!(!Arc::ptr_eq(&retained, &fragment(&app)));
}

#[test]
fn hidden_geometry_changes_invalidate_fragment_before_fade_in() {
    let mut app = App::new();
    app.init_resource::<GaanimRenderCache>()
        .add_systems(Update, gaanim_render_system);
    let id = ObjectId::from_raw(48);
    let entity = app
        .world_mut()
        .spawn((
            MobjectId(id),
            GlobalSpatialTransform::default(),
            GlobalOpacity(1.0),
            RenderOrder::default(),
            RenderLayer::Vello2D,
            Path2D(rect_path(0.0, 0.0, 20.0, 20.0)),
            FillBrush::color(peniko::Color::WHITE),
            StrokeBrush::transparent(),
            Visible,
        ))
        .id();
    app.update();
    let last_frame = app.world().resource::<GaanimRenderCache>().fragment_cache[&id].clone();
    app.world_mut().get_mut::<GlobalOpacity>(entity).unwrap().0 = 0.0;
    app.world_mut().get_mut::<Path2D>(entity).unwrap().0 = rect_path(30.0, 0.0, 40.0, 10.0);
    app.update();
    assert!(
        !app.world()
            .resource::<GaanimRenderCache>()
            .fragment_cache
            .contains_key(&id)
    );
    // The geometry stays unchanged during the fade; only opacity changes.
    app.world_mut().get_mut::<GlobalOpacity>(entity).unwrap().0 = 0.5;
    app.update();
    let first_frame = &app.world().resource::<GaanimRenderCache>().fragment_cache[&id];
    assert_ne!(
        first_frame.encoding().path_data,
        last_frame.encoding().path_data
    );
}

#[test]
fn live_boolean_system_runs_with_overlapping_path_queries() {
    let mut app = App::new();
    let left = app
        .world_mut()
        .spawn((
            Path2D(rect_path(0.0, 0.0, 10.0, 10.0)),
            GlobalSpatialTransform::default(),
        ))
        .id();
    let right = app
        .world_mut()
        .spawn((
            Path2D(rect_path(5.0, 0.0, 15.0, 10.0)),
            GlobalSpatialTransform::default(),
        ))
        .id();
    let result = app
        .world_mut()
        .spawn((
            BooleanBinding {
                sources: vec![left, right],
                op: gaanim_objects::boolean::BooleanOp::Intersection,
                tolerance: 0.25,
                rule: gaanim_objects::boolean::BooleanFillRule::NonZero,
            },
            GlobalSpatialTransform::default(),
            Path2D::default(),
            PathSource::default(),
            LocalBounds::default(),
        ))
        .id();
    app.add_systems(Update, resolve_dynamic_boolean_system);

    app.update();

    let bounds = app.world().get::<Path2D>(result).unwrap().0.bounding_box();
    assert_eq!(bounds, kurbo::Rect::new(5.0, 0.0, 10.0, 10.0));
}

#[test]
fn fill_level_system_tracks_level_and_direction() {
    let mut app = App::new();
    let mask = app
        .world_mut()
        .spawn((
            Path2D(rect_path(0.0, 0.0, 100.0, 80.0)),
            GlobalSpatialTransform::default(),
        ))
        .id();
    let fill = app
        .world_mut()
        .spawn((
            FillLevelBinding {
                sources: vec![mask],
                direction: FillDirection::Up,
            },
            FillLevel(0.25),
            GlobalSpatialTransform::default(),
            Path2D::default(),
            PathSource::default(),
            LocalBounds::default(),
        ))
        .id();
    app.add_systems(Update, resolve_fill_level_system);

    app.update();
    let quarter = app.world().get::<Path2D>(fill).unwrap().0.bounding_box();
    assert_eq!(quarter, kurbo::Rect::new(0.0, 0.0, 100.0, 20.0));

    app.world_mut().get_mut::<FillLevel>(fill).unwrap().0 = 1.0;
    app.update();
    let full = app.world().get::<Path2D>(fill).unwrap().0.bounding_box();
    assert_eq!(full, kurbo::Rect::new(0.0, 0.0, 100.0, 80.0));
}

#[test]
fn zero_path_reveal_is_rendered_as_empty_geometry() {
    let mut tip = WriteTipGlow {
        completion: 0.0,
        ..WriteTipGlow::default()
    };
    assert!(path_reveal_is_empty(Some(&tip)));
    tip.completion = 0.001;
    assert!(!path_reveal_is_empty(Some(&tip)));
    assert!(!path_reveal_is_empty(None));
}

#[test]
fn zero_global_opacity_is_not_extracted() {
    assert!(opacity_is_empty(&GlobalOpacity(0.0)));
    assert!(!opacity_is_empty(&GlobalOpacity(0.001)));
}

#[test]
fn opacity_layers_use_local_finite_bounds_instead_of_a_full_scene_sentinel() {
    let bounds = WorldBounds(gaanim_math::Bounds3D::new_2d(-12.0, -8.0, 18.0, 14.0));
    let fallback = kurbo::Rect::new(-4096.0, -4096.0, 4096.0, 4096.0);
    let rect = opacity_layer_bounds(None, Some(&bounds), fallback, 1.0);

    assert!(rect.x0 <= -12.0 && rect.y0 <= -8.0);
    assert!(rect.x1 >= 18.0 && rect.y1 >= 14.0);
    assert!(rect.width() < 64.0);
    assert!(rect.height() < 64.0);
}

#[test]
fn aligned_dashes_keep_their_gaps_under_the_doubled_pen() {
    let mut style = kurbo::Stroke::new(0.1).with_dashes(0.2, [0.3, 0.2]);
    style.start_cap = kurbo::Cap::Round;
    style.end_cap = kurbo::Cap::Round;
    let pen = clipped_pen(&style);
    // Each cap of the doubled pen reaches 0.05 further: dashes lose 0.1,
    // gaps gain it, and the pattern starts 0.05 later.
    assert_eq!(pen.dash_pattern.to_vec(), vec![0.3 - 0.1, 0.2 + 0.1]);
    assert!((pen.dash_offset - 0.15).abs() < 1e-12);
    // A dash shorter than the width shrinks to a dot; odd patterns repeat.
    let mut dots = kurbo::Stroke::new(0.1).with_dashes(0.0, [0.04, 0.2, 0.3]);
    dots.start_cap = kurbo::Cap::Square;
    let pen = clipped_pen(&dots);
    let lengths: Vec<f64> = pen
        .dash_pattern
        .iter()
        .map(|v| (v * 1e6).round() / 1e6)
        .collect();
    assert_eq!(lengths, vec![0.0, 0.27, 0.2, 0.14, 0.1, 0.37]);
    // Butt caps reach no further: the pattern is kept.
    let butt = kurbo::Stroke::new(0.1)
        .with_caps(kurbo::Cap::Butt)
        .with_dashes(0.0, [0.3, 0.2]);
    assert_eq!(clipped_pen(&butt).dash_pattern.to_vec(), vec![0.3, 0.2]);
}

#[test]
fn zero_length_dashes_with_caps_become_slivers_that_draw_dots() {
    let mut dotted = kurbo::Stroke::new(0.1).with_dashes(0.0, [0.0, 0.3]);
    dotted.start_cap = kurbo::Cap::Round;
    dotted.end_cap = kurbo::Cap::Round;
    let pen = crate::stroke::drawable_pen(&dotted);
    assert!(pen.dash_pattern[0] > 0.0 && pen.dash_pattern[0] < 1e-3);
    assert_eq!(pen.dash_pattern[1], 0.3);
    // Without caps a zero dash stays empty; positive dashes are kept.
    let butt = kurbo::Stroke::new(0.1)
        .with_caps(kurbo::Cap::Butt)
        .with_dashes(0.0, [0.0, 0.3]);
    assert!(matches!(
        crate::stroke::drawable_pen(&butt),
        std::borrow::Cow::Borrowed(_)
    ));
    let mut dashed = kurbo::Stroke::new(0.1).with_dashes(0.0, [0.2, 0.3]);
    dashed.end_cap = kurbo::Cap::Round;
    assert!(matches!(
        crate::stroke::drawable_pen(&dashed),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn aligned_strokes_double_the_pen_that_keeps_only_one_half() {
    let stroke = kurbo::Stroke::new(0.3);
    assert_eq!(clipped_pen(&stroke).width, 0.6);
    // Only an outside stroke reaches beyond the contour.
    assert_eq!(aligned_pen(&stroke, StrokeAlign::Inside).width, 0.3);
    assert_eq!(aligned_pen(&stroke, StrokeAlign::Center).width, 0.3);
    assert_eq!(aligned_pen(&stroke, StrokeAlign::Outside).width, 0.6);

    // The opacity layer must reach the whole authored width outside.
    let square = kurbo::Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1);
    let pen = aligned_pen(&stroke, StrokeAlign::Outside);
    let (_, reach) = fragment_extent(
        &square,
        kurbo::Affine::IDENTITY,
        Some(&pen),
        None,
        None,
        None,
        None,
    )
    .expect("visible square");
    assert!(reach >= 0.3, "{reach}");
}

#[test]
fn opacity_layers_cover_the_drawn_stroke_and_effects() {
    let mut line = kurbo::BezPath::new();
    line.move_to((0.0, 0.0));
    line.line_to((3.0, 0.0));
    let stroke = kurbo::Stroke::new(0.2).with_join(kurbo::Join::Bevel);
    let transform = kurbo::Affine::translate((-6.0, 1.0)) * kurbo::Affine::scale(2.0);
    let glow = Glow {
        radius: 0.05,
        ..Glow::default()
    };
    let extent = fragment_extent(
        &line,
        transform,
        Some(&stroke),
        None,
        None,
        Some(&glow),
        None,
    );
    let (rect, reach) = extent.expect("visible line");
    assert_eq!(rect, kurbo::Rect::new(-6.0, 1.0, 0.0, 1.0));
    // Half the width, diagonally at square caps, plus the glow, scaled.
    let norm = 8.0_f64.sqrt();
    let expected = (0.1 * std::f64::consts::SQRT_2 + 0.05) * norm;
    assert!((reach - expected).abs() < 1e-12, "{reach}");

    let fallback = kurbo::Rect::new(-8.0, -4.5, 8.0, 4.5);
    let layer = opacity_layer_bounds(extent, None, fallback, 0.01);
    assert!(layer.y0 < 1.0 - 0.2 && layer.y1 > 1.0 + 0.2);
    assert!(layer.height() < 1.5, "{layer:?}");
    let empty = kurbo::BezPath::new();
    assert!(fragment_extent(&empty, transform, None, None, None, None, None).is_none());
}

#[test]
fn opacity_runs_clip_to_their_own_elements_not_the_frame() {
    let element = |rect| ExtractedElement {
        soft: Default::default(),
        persistent: false,
        post_exempt: false,
        entity: Entity::PLACEHOLDER,
        recipe: None,
        lottie: None,
        transform: kurbo::Affine::IDENTITY,
        opacity: 0.5,
        opacity_bounds: rect,
        opacity_extent: None,
        opacity_group: Entity::PLACEHOLDER,
        group_opacity: 1.0,
        group_shadow: None,
        effect_shadow: None,
        effect_root: None,
        effect_extent: None,
        matte_root: None,
        matte_of: None,
        glass_root: None,
        outline: None,
        render_order: RenderOrder::default(),
        z_layer: 0,
        scene: Arc::new(vello::Scene::new()),
        clip_mask: None,
        blend: None,
        transition_side: Default::default(),
        lineage: Vec::new(),
        view_bounds: None,
        in_views: true,
        layer: None,
        screen: None,
        echo_rank: 0,
        tip: false,
        backdrop: None,
    };
    let run = [
        element(kurbo::Rect::new(-6.0, 1.0, -3.0, 1.2)),
        element(kurbo::Rect::new(-5.0, 1.1, -2.0, 1.4)),
    ];
    assert_eq!(
        opacity_run_bounds(&run),
        kurbo::Rect::new(-6.0, 1.0, -2.0, 1.4)
    );
}

/// A filled square at `x` as a member of the group `root`.
fn group_member(root: Entity, x: f64, opacity: f32, group_opacity: f32) -> ExtractedElement {
    let path = kurbo::Rect::new(x, 0.0, x + 1.0, 1.0).to_path(0.1);
    let mut scene = vello::Scene::new();
    scene.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::WHITE,
        None,
        &path,
    );
    ExtractedElement {
        soft: Default::default(),
        persistent: false,
        post_exempt: false,
        entity: Entity::PLACEHOLDER,
        recipe: None,
        lottie: None,
        transform: kurbo::Affine::IDENTITY,
        opacity,
        opacity_bounds: kurbo::Rect::new(x - 1.0, -1.0, x + 2.0, 2.0),
        opacity_extent: None,
        opacity_group: root,
        group_opacity,
        group_shadow: None,
        effect_shadow: None,
        effect_root: None,
        effect_extent: None,
        matte_root: None,
        matte_of: None,
        glass_root: None,
        outline: None,
        render_order: RenderOrder::default(),
        z_layer: 0,
        scene: Arc::new(scene),
        clip_mask: None,
        blend: None,
        transition_side: Default::default(),
        lineage: Vec::new(),
        view_bounds: None,
        in_views: true,
        layer: None,
        screen: None,
        echo_rank: 0,
        tip: false,
        backdrop: None,
    }
}

#[test]
fn drawables_left_out_of_the_post_process_compose_apart() {
    let kept = group_member(Entity::from_raw_u32(21).unwrap(), -2.0, 1.0, 1.0);
    let mut exempt = group_member(Entity::from_raw_u32(22).unwrap(), 2.0, 1.0, 1.0);
    exempt.post_exempt = true;
    let paths = |scene: &vello::Scene| scene.encoding().n_paths;
    let alone = compose_frame(std::slice::from_ref(&kept), None, None, 1.0, 0.0, None);
    assert!(alone.unprocessed.is_none());

    let mut composed = compose_frame(&[kept, exempt], None, None, 1.0, 0.0, None);
    assert_eq!(paths(&composed.scene), paths(&alone.scene));
    let layer = composed
        .unprocessed
        .clone()
        .expect("the exempt drawable has a layer");
    assert!(paths(&layer) > 0);
    // With a post-process the layer stays apart; without one it joins
    // the frame on top.
    assert!(composed.clone().take_unprocessed(true).is_some());
    assert!(composed.take_unprocessed(false).is_none());
    assert_eq!(paths(&composed.scene), paths(&alone.scene) + paths(&layer));
}

/// Spawn the half-opaque image with the default shadow and a shader
/// effect of `passes`.
fn spawn_shadowed_effect(world: &mut World, passes: Vec<crate::post_process::PostProcessPass>) {
    world.spawn((
        MobjectId(ObjectId::from_raw(31)),
        gaanim_math::SpatialTransform::default(),
        GlobalSpatialTransform::default(),
        GlobalOpacity(1.0),
        RenderOrder::default(),
        RenderLayer::Vello2D,
        Path2D(rect_path(0.0, 0.0, 1.0, 1.0)),
        crate::fragment::tests::half_opaque_image(),
        DropShadow::default(),
        crate::object_effects::ShaderEffect {
            post: crate::post_process::CanvasPostProcess {
                passes,
                ..Default::default()
            },
            margin: 0.2,
        },
        Visible,
    ));
}

fn identity_pass() -> crate::post_process::PostProcessPass {
    crate::post_process::PostProcessShader::new(
            "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> { return gaanim_scene(uv); }",
        )
        .unwrap()
        .into()
}

/// The effect's texture holds the drawable without the shadow its
/// fragment draws; the shadow is blurred from the texture, after it, and
/// `scene` draws it beneath the texture.
fn assert_shadow_cast_from_effect(
    effects: &[crate::object_effects::EffectLayer],
    scene: &vello::Scene,
) {
    use crate::fragment::tests::image_draws;
    let texture = effects
        .iter()
        .position(|layer| layer.request.passes.len() == 1)
        .expect("the effect's texture");
    let texture_id = effects[texture].image.data.id();
    assert!(
        effects
            .iter()
            .filter(|layer| layer.fixed)
            .all(|layer| image_draws(&effects[texture].scene, layer.image.data.id()) == 0)
    );
    let cast = effects
        .iter()
        .position(|layer| !layer.fixed && image_draws(&layer.scene, texture_id) == 1)
        .expect("the shadow cast from the effect");
    assert!(cast > texture);
    assert_eq!(image_draws(scene, effects[cast].image.data.id()), 1);
    assert_eq!(image_draws(scene, texture_id), 1);
}

#[test]
fn a_shader_effect_casts_its_drawables_shadow_from_what_it_draws() {
    let mut world = World::new();
    spawn_shadowed_effect(&mut world, vec![identity_pass()]);
    let frame = compile_frame_from_world(&mut world, None);
    assert_shadow_cast_from_effect(&frame.effects, &frame.scene);

    // Without an active pass the drawable is drawn plainly, shadow and all.
    let mut world = World::new();
    spawn_shadowed_effect(&mut world, Vec::new());
    let plain = compile_frame_from_world(&mut world, None);
    let shadow = plain
        .effects
        .iter()
        .find(|layer| layer.fixed)
        .expect("the fragment's shadow");
    assert_eq!(
        crate::fragment::tests::image_draws(&plain.scene, shadow.image.data.id()),
        1
    );
}

#[test]
fn a_shader_effect_casts_its_members_shared_shadow_once() {
    let compose = |second: DropShadow| {
        let mut world = World::new();
        let root = world
            .spawn((
                gaanim_math::SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                crate::object_effects::ShaderEffect {
                    post: crate::post_process::CanvasPostProcess {
                        passes: vec![identity_pass()],
                        ..Default::default()
                    },
                    margin: 0.2,
                },
            ))
            .id();
        for (raw, x, shadow) in [(41, 0.0, DropShadow::default()), (42, 2.0, second)] {
            world.spawn((
                MobjectId(ObjectId::from_raw(raw)),
                gaanim_math::SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                RenderOrder::default(),
                RenderLayer::Vello2D,
                Path2D(rect_path(x, 0.0, x + 1.0, 1.0)),
                FillBrush(Some(peniko::Brush::Solid(peniko::Color::WHITE))),
                shadow,
                Visible,
                ChildOf(root),
            ));
        }
        compile_frame_from_world(&mut world, None)
    };
    // Members with the same shadow, as a group's style gives them.
    let shared = compose(DropShadow::default());
    assert_shadow_cast_from_effect(&shared.effects, &shared.scene);

    // Different shadows stay inside the texture, with their members.
    let mixed = compose(DropShadow {
        blur_radius: 0.2,
        ..DropShadow::default()
    });
    let texture = mixed
        .effects
        .iter()
        .find(|layer| layer.request.passes.len() == 1)
        .expect("the effect's texture");
    let texture_id = texture.image.data.id();
    assert!(
        mixed
            .effects
            .iter()
            .all(|layer| crate::fragment::tests::image_draws(&layer.scene, texture_id) == 0),
        "nothing is cast from the texture"
    );
    assert!(
        mixed
            .effects
            .iter()
            .filter(|layer| layer.fixed)
            .any(|layer| {
                crate::fragment::tests::image_draws(&texture.scene, layer.image.data.id()) > 0
            }),
        "the texture draws the members' shadows"
    );
}

#[test]
fn the_live_canvas_casts_a_shader_effects_shadow_from_what_it_draws() {
    let mut app = App::new();
    app.init_resource::<GaanimRenderCache>()
        .init_resource::<EffectLayers>()
        .add_systems(Update, gaanim_render_system);
    spawn_shadowed_effect(app.world_mut(), vec![identity_pass()]);
    app.update();
    let effects = app.world().resource::<EffectLayers>().0.clone();
    let live = app
        .world_mut()
        .query::<&VelloScene2d>()
        .single(app.world())
        .unwrap();
    assert_shadow_cast_from_effect(&effects, live);
}

#[test]
fn a_shadow_in_world_turns_and_scales_with_its_drawable() {
    let shadow = DropShadow {
        offset: gaanim_core::glam::DVec2::new(0.1, 0.0),
        blur_radius: 0.05,
        ..DropShadow::default()
    };
    let placed = kurbo::Affine::translate((4.0, 2.0))
        * kurbo::Affine::rotate(std::f64::consts::FRAC_PI_2)
        * kurbo::Affine::scale(2.0);
    let world = shadow_in_world(&shadow, placed);
    assert!((world.offset - gaanim_core::glam::DVec2::new(0.0, 0.2)).length() < 1e-12);
    assert!((world.blur_radius - 0.1).abs() < 1e-12);
    assert_eq!(world.color, shadow.color);
}

#[test]
fn a_shader_effect_draws_its_drawable_as_one_image_in_its_place() {
    let root = Entity::from_raw_u32(11).unwrap();
    let other = Entity::from_raw_u32(12).unwrap();
    let member = |x: f64, effect: Option<Entity>| {
        let mut element = group_member(root, x, 1.0, 1.0);
        element.effect_root = effect;
        element.effect_extent = Some(kurbo::Rect::new(x, 0.0, x + 1.0, 1.0));
        element
    };
    let mut elements = vec![
        member(-3.0, None),
        member(0.0, Some(root)),
        member(1.5, Some(root)),
        member(5.0, None),
        member(3.0, Some(root)),
    ];
    let shader = crate::post_process::PostProcessShader::new(
            "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> { return gaanim_scene(uv); }",
        )
        .unwrap();
    let effect = crate::object_effects::ShaderEffect {
        post: crate::post_process::CanvasPostProcess {
            passes: vec![shader.into()],
            ..Default::default()
        },
        margin: 0.5,
    };
    let effects = evaluate_effects(
        &elements,
        1.0,
        |entity| (entity == root).then(|| effect.clone()),
        |_| None,
    );
    assert_eq!(effects.len(), 1);
    let layers = divert_effects(&mut elements, &effects, 1.0, 10.0);
    // The three members became one element, where the first one was.
    assert_eq!(elements.len(), 3);
    assert!(elements.iter().all(|element| element.effect_root.is_none()));
    assert_eq!(
        elements[1].view_bounds,
        Some(kurbo::Rect::new(-0.5, -0.5, 4.5, 1.5))
    );
    assert_eq!(layers.len(), 1);
    let layer = &layers[0];
    assert_eq!((layer.image.width, layer.image.height), (50, 20));
    assert_eq!(layer.request.frame, kurbo::Rect::new(0.0, 0.0, 50.0, 20.0));
    // The texture's top-left pixel is the bounds' top-left corner.
    assert_eq!(
        layer.to_pixels * kurbo::Point::new(-0.5, 1.5),
        kurbo::Point::ZERO
    );
    // A depth-of-field blur turns its scene-unit sigma into pixels at
    // a density low enough to keep it within a few texels.
    let blur = crate::object_effects::depth_of_field_effect(0.5).unwrap();
    let mut blurred = vec![member(0.0, Some(root))];
    let effects = evaluate_effects(
        &blurred,
        1.0,
        |entity| (entity == root).then(|| blur.clone()),
        |_| None,
    );
    assert_eq!(effects[0].margin, 1.5);
    assert_eq!(effects[0].passes[0].1, vec![0.5, 1.0]);
    let layers = divert_effects(&mut blurred, &effects, 1.0, 100.0);
    let density = crate::object_effects::MAX_DEPTH_BLUR_PIXELS / 0.5;
    assert_eq!(layers[0].request.passes[0].1, vec![8.0, 1.0]);
    assert_eq!(layers[0].request.passes[1].1, vec![8.0, 0.0]);
    assert_eq!(
        u32::from(layers[0].image.width),
        ((1.0 + 2.0 * 1.5) * density).ceil() as u32
    );
    // A drawable without the component is drawn plainly.
    let mut plain = vec![member(0.0, Some(other))];
    let none = evaluate_effects(&plain, 1.0, |_| None, |_| None);
    assert!(divert_effects(&mut plain, &none, 1.0, 10.0).is_empty());
    assert_eq!(plain.len(), 1);
}

#[test]
fn members_of_a_text_draw_their_shared_shadow_once() {
    let root = Entity::from_raw_u32(7).unwrap();
    let shadow = DropShadow {
        offset: gaanim_core::glam::DVec2::new(0.1, -0.1),
        blur_radius: 0.1,
        color: peniko::Color::from_rgba8(0, 0, 0, 128),
    };
    let glyphs: Vec<ExtractedElement> = (0..12)
        .map(|index| {
            let mut glyph = group_member(root, f64::from(index) * 1.2, 1.0, 1.0);
            glyph.group_shadow = Some(Arc::new(crate::fragment::GroupShadow {
                shadow: shadow.clone(),
                path: Arc::new(kurbo::Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1)),
            }));
            glyph.transform = kurbo::Affine::translate((f64::from(index) * 1.2, 0.0));
            glyph
        })
        .collect();
    assert_eq!(shadow_run_end(&glyphs, 0), Some(12));
    let mut composed = vello::Scene::new();
    append_element_run(&mut composed, &glyphs, None);
    // One blurred shadow opens two layers, not two per glyph.
    assert_eq!(composed.encoding().n_clips, 4);

    // A translucent text keeps its shadow, in the layer of its opacity.
    let faded: Vec<ExtractedElement> = glyphs
        .iter()
        .map(|glyph| ExtractedElement {
            soft: Default::default(),
            opacity: 0.5,
            group_opacity: 0.5,
            ..glyph.clone()
        })
        .collect();
    let mut composed = vello::Scene::new();
    append_element_run(&mut composed, &faded, None);
    assert_eq!(composed.encoding().n_clips, 6);
}

#[test]
fn a_fading_group_is_one_layer_over_its_members() {
    let root = Entity::from_raw_u32(9).unwrap();
    // An opaque body and a translucent highlight, the group at 50%.
    let members = [
        group_member(root, 0.0, 0.5, 0.5),
        group_member(root, 0.5, 0.25, 0.5),
    ];
    assert_eq!(group_fade_run_end(&members, 0), Some(2));
    // Members that share one opacity layer anyway need no group layer.
    let even = [
        group_member(root, 0.0, 0.5, 0.5),
        group_member(root, 0.5, 0.5, 0.5),
    ];
    assert_eq!(group_fade_run_end(&even, 0), None);
    let inside = members[1].inside_group();
    assert!((inside.opacity - 0.5).abs() < 1e-6 && inside.group_opacity == 1.0);
}

#[test]
fn consecutive_glyphs_with_the_same_opacity_share_one_compositor_run() {
    let element = |opacity| ExtractedElement {
        soft: Default::default(),
        persistent: false,
        post_exempt: false,
        entity: Entity::PLACEHOLDER,
        recipe: None,
        lottie: None,
        transform: kurbo::Affine::IDENTITY,
        opacity,
        opacity_bounds: kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        opacity_extent: None,
        opacity_group: Entity::PLACEHOLDER,
        group_opacity: 1.0,
        group_shadow: None,
        effect_shadow: None,
        effect_root: None,
        effect_extent: None,
        matte_root: None,
        matte_of: None,
        glass_root: None,
        outline: None,
        render_order: RenderOrder::default(),
        z_layer: 0,
        scene: Arc::new(vello::Scene::new()),
        clip_mask: None,
        blend: None,
        transition_side: Default::default(),
        lineage: Vec::new(),
        view_bounds: None,
        in_views: true,
        layer: None,
        screen: None,
        echo_rank: 0,
        tip: false,
        backdrop: None,
    };
    let elements = vec![element(0.5), element(0.5), element(0.5), element(0.75)];

    assert_eq!(opacity_run_end(&elements, 0), 3);
    assert_eq!(opacity_run_end(&elements, 3), 4);
}

#[test]
fn a_translucent_single_solid_paint_is_faded_without_a_layer() {
    let element = |scene: vello::Scene| ExtractedElement {
        soft: Default::default(),
        persistent: false,
        post_exempt: false,
        entity: Entity::PLACEHOLDER,
        recipe: None,
        lottie: None,
        transform: kurbo::Affine::translate((2.0, 0.0)),
        opacity: 0.5,
        opacity_bounds: kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        opacity_extent: None,
        opacity_group: Entity::PLACEHOLDER,
        group_opacity: 1.0,
        group_shadow: None,
        effect_shadow: None,
        effect_root: None,
        effect_extent: None,
        matte_root: None,
        matte_of: None,
        glass_root: None,
        outline: None,
        render_order: RenderOrder::default(),
        z_layer: 0,
        scene: Arc::new(scene),
        clip_mask: None,
        blend: None,
        transition_side: Default::default(),
        lineage: Vec::new(),
        view_bounds: None,
        in_views: true,
        layer: None,
        screen: None,
        echo_rank: 0,
        tip: false,
        backdrop: None,
    };
    let circle = kurbo::Circle::new((0.0, 0.0), 1.0);
    let red = peniko::Color::from_rgba8(200, 0, 0, 255);
    let mut fill = vello::Scene::new();
    fill.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        red,
        None,
        &circle,
    );
    let mut outlined = fill.clone();
    outlined.stroke(
        &kurbo::Stroke::new(0.1),
        kurbo::Affine::IDENTITY,
        peniko::Color::WHITE,
        None,
        &circle,
    );

    let mut composed = vello::Scene::new();
    append_element_run(&mut composed, &[element(fill)], None);
    let encoding = composed.encoding();
    assert_eq!(encoding.n_clips, 0);
    assert_eq!(
        encoding.draw_data,
        vec![u32::from_le_bytes([100, 0, 0, 128])]
    );

    // Fill and stroke overlap, so only a layer composites them correctly.
    let mut composed = vello::Scene::new();
    append_element_run(&mut composed, &[element(outlined)], None);
    // The begin and end markers of one layer.
    assert_eq!(composed.encoding().n_clips, 2);
}

#[test]
fn blended_elements_never_join_a_shared_opacity_layer() {
    let element = |blend| ExtractedElement {
        soft: Default::default(),
        persistent: false,
        post_exempt: false,
        entity: Entity::PLACEHOLDER,
        recipe: None,
        lottie: None,
        transform: kurbo::Affine::IDENTITY,
        opacity: 0.5,
        opacity_bounds: kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        opacity_extent: None,
        opacity_group: Entity::PLACEHOLDER,
        group_opacity: 1.0,
        group_shadow: None,
        effect_shadow: None,
        effect_root: None,
        effect_extent: None,
        matte_root: None,
        matte_of: None,
        glass_root: None,
        outline: None,
        render_order: RenderOrder::default(),
        z_layer: 0,
        scene: Arc::new(vello::Scene::new()),
        clip_mask: None,
        blend,
        transition_side: Default::default(),
        lineage: Vec::new(),
        view_bounds: None,
        in_views: true,
        layer: None,
        screen: None,
        echo_rank: 0,
        tip: false,
        backdrop: None,
    };
    let multiply = Some(peniko::BlendMode::from(peniko::Mix::Multiply));
    let elements = vec![element(None), element(multiply), element(None)];

    // A shared source-over layer would composite the multiply band
    // against the group instead of the backdrop.
    assert_eq!(opacity_run_end(&elements, 0), 1);
}

#[test]
fn elements_clipped_by_the_same_mask_share_one_clip_layer() {
    let mask = |source: u32, invert: bool| ClipMask {
        sources: vec![Entity::from_raw_u32(source).unwrap()],
        invert,
        ..ClipMask::default()
    };
    let element = |clip_mask| ExtractedElement {
        soft: Default::default(),
        persistent: false,
        post_exempt: false,
        entity: Entity::PLACEHOLDER,
        recipe: None,
        lottie: None,
        transform: kurbo::Affine::IDENTITY,
        opacity: 1.0,
        opacity_bounds: kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        opacity_extent: None,
        opacity_group: Entity::PLACEHOLDER,
        group_opacity: 1.0,
        group_shadow: None,
        effect_shadow: None,
        effect_root: None,
        effect_extent: None,
        matte_root: None,
        matte_of: None,
        glass_root: None,
        outline: None,
        render_order: RenderOrder::default(),
        z_layer: 0,
        scene: Arc::new(vello::Scene::new()),
        clip_mask,
        blend: None,
        transition_side: Default::default(),
        lineage: Vec::new(),
        view_bounds: None,
        in_views: true,
        layer: None,
        screen: None,
        echo_rank: 0,
        tip: false,
        backdrop: None,
    };
    let elements = vec![
        element(Some(mask(1, false))),
        element(Some(mask(1, false))),
        element(Some(mask(2, false))),
        element(Some(mask(2, true))),
        element(Some(mask(2, true))),
        element(None),
    ];

    assert_eq!(shared_clip_run_end(&elements, 0), 2);
    assert_eq!(shared_clip_run_end(&elements, 2), 3);
    // Inverted masks depend on each element's bounds and stay separate.
    assert_eq!(shared_clip_run_end(&elements, 3), 4);
    assert_eq!(shared_clip_run_end(&elements, 5), 6);
}

#[test]
fn shader_transitions_compose_each_side_apart_with_neither_side_above() {
    use gaanim_math::SpatialTransform;
    let mut world = World::new();
    let mut spawn = |raw: u64, corners: usize| {
        let mut path = kurbo::BezPath::new();
        path.move_to((0.0, 0.0));
        for index in 1..corners {
            path.line_to((index as f64, (index % 2) as f64));
        }
        path.close_path();
        world
            .spawn((
                MobjectId(ObjectId::from_raw(raw)),
                SpatialTransform::default(),
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                RenderOrder::default(),
                RenderLayer::Vello2D,
                Path2D(Arc::new(path)),
                FillBrush(Some(peniko::Brush::Solid(peniko::Color::WHITE))),
                Visible,
            ))
            .id()
    };
    let outgoing = spawn(1, 3);
    let incoming = spawn(2, 5);
    let _persistent = spawn(3, 7);
    let shader = gaanim_scene::TransitionShaderFrame {
        shader: Arc::new(gaanim_scene::TransitionShader {
            source: String::new(),
            uniforms: Vec::new(),
            values: Vec::new(),
            data: None,
        }),
        progress: 0.4,
        values: Vec::new(),
    };
    let mut frame = gaanim_scene::SceneTransitionFrame {
        shader: Some(shader.clone()),
        ..Default::default()
    };
    frame.outgoing.insert(outgoing);
    frame.incoming.insert(incoming);
    world.insert_resource(frame);

    let composed = compile_frame_from_world(&mut world, None);
    let segments = |scene: &vello::Scene| scene.encoding().n_path_segments;
    let transition = composed.transition.clone().expect("shader layers");
    assert_eq!(transition.shader, shader);
    // A triangle, a pentagon and a heptagon, one per scene.
    assert_eq!(segments(&composed.scene), 3);
    assert_eq!(segments(&transition.incoming), 5);
    assert_eq!(segments(&transition.above), 7);
    // Without the GPU blend, all three draw in one scene, the incoming
    // side inside a cross-fade layer (its clip is a rectangle).
    assert_eq!(
        segments(&compile_scene_from_world(&mut world, None)),
        15 + 4
    );
}

#[test]
fn transition_masks_are_isolated_layers_that_clip_nested_blend_layers() {
    // vello's clip-only layer (`DrawBeginClip::CLIP_BLEND_MODE`) does not
    // clip nested blend layers, such as the stroke layer of a text glyph.
    const CLIP_ONLY: u32 = 0x8003;
    let rect = kurbo::Rect::new(0.0, 0.0, 1.0, 1.0);
    for fade in [
        None,
        Some((kurbo::Point::ZERO, kurbo::Point::new(1.0, 0.0))),
    ] {
        let mask = gaanim_scene::TransitionMask {
            path: rect.to_path(0.1),
            rule: peniko::Fill::NonZero,
            fade,
        };
        let mut scene = vello::Scene::new();
        push_transition_mask(&mut scene, &mask);
        pop_transition_mask(&mut scene, &mask);
        let draw_data = &scene.encoding().draw_data;
        assert_ne!(draw_data.first(), Some(&CLIP_ONLY), "fade: {fade:?}");
        assert!(!draw_data.contains(&CLIP_ONLY));
    }
}

#[test]
fn shader_background_raster_size_tracks_the_interactive_viewport_scale() {
    let background = CanvasBackground {
        paint: BackgroundPaint::solid(peniko::Color::BLACK),
        segment_paints: Vec::new(),
        pixel_size: (1280, 720),
        bounds: gaanim_math::Bounds3D::new_2d(-640.0, -360.0, 640.0, 360.0),
        parameters: Vec::new(),
    };
    let camera = gaanim_math::ResolvedCamera::new(
        gaanim_math::Camera::ortho_2d(1280, 720),
        gaanim_math::CameraViewport {
            scale: 0.5,
            offset_y: 0.0,
            offset_x: 0.0,
        },
    );

    assert_eq!(
        interactive_background_pixel_size(&background, Some(&camera), None),
        (640, 360)
    );

    let high_zoom_camera = gaanim_math::ResolvedCamera::new(
        gaanim_math::Camera::ortho_2d(1280, 720),
        gaanim_math::CameraViewport {
            scale: 8.0,
            offset_y: 0.0,
            offset_x: 0.0,
        },
    );
    assert_eq!(
        interactive_background_pixel_size(&background, Some(&high_zoom_camera), None),
        (8192, 4608)
    );
}

#[test]
fn an_image_background_covers_the_frame_centered() {
    let rect = kurbo::Rect::new(-8.0, -4.5, 8.0, 4.5);
    // A square image covers the 16 x 9 frame at 16 x 16: it is cropped
    // above and below, and its first row is the top.
    let cover = image_cover_transform(rect, 100, 100);
    let top_left = cover * kurbo::Point::new(0.0, 0.0);
    let bottom_right = cover * kurbo::Point::new(100.0, 100.0);
    assert!((top_left - kurbo::Point::new(-8.0, 8.0)).hypot() < 1e-9);
    assert!((bottom_right - kurbo::Point::new(8.0, -8.0)).hypot() < 1e-9);
    // A wide image is cropped at its sides.
    let wide = image_cover_transform(rect, 400, 100) * kurbo::Point::new(0.0, 0.0);
    assert!((wide - kurbo::Point::new(-18.0, 4.5)).hypot() < 1e-9);
}

#[test]
fn canvas_background_geometry_matches_the_authored_scene_bounds() {
    let background = CanvasBackground {
        paint: BackgroundPaint::solid(peniko::Color::BLACK),
        segment_paints: Vec::new(),
        pixel_size: (960, 540),
        bounds: gaanim_math::Bounds3D::new_2d(-480.0, -270.0, 480.0, 270.0),
        parameters: Vec::new(),
    };
    let (rect, transform) = canvas_background_geometry(&background);

    assert_eq!(rect, kurbo::Rect::new(-480.0, -270.0, 480.0, 270.0));
    assert_eq!(transform, kurbo::Affine::IDENTITY);
}

#[test]
fn segment_backgrounds_override_and_fall_back_at_exact_timeline_positions() {
    let default = peniko::Color::from_rgb8(10, 20, 30);
    let first = peniko::Color::from_rgb8(40, 50, 60);
    let third = peniko::Color::from_rgb8(70, 80, 90);
    let background = CanvasBackground {
        paint: BackgroundPaint::solid(default),
        segment_paints: vec![
            SegmentBackgroundPaint {
                start_time: 0.0,
                end_time: 1.0,
                paint: Some(BackgroundPaint::solid(first)),
                hold_at_end: true,
            },
            SegmentBackgroundPaint {
                start_time: 1.0,
                end_time: 2.0,
                paint: None,
                hold_at_end: false,
            },
            SegmentBackgroundPaint {
                start_time: 2.0,
                end_time: 3.0,
                paint: Some(BackgroundPaint::solid(third)),
                hold_at_end: false,
            },
        ],
        pixel_size: (960, 540),
        bounds: gaanim_math::Bounds3D::new_2d(-480.0, -270.0, 480.0, 270.0),
        parameters: Vec::new(),
    };

    assert_eq!(background.paint_at(0.5).fallback_color(), first);
    assert_eq!(background.paint_at(1.0).fallback_color(), first);
    assert_eq!(background.paint_at(1.1).fallback_color(), default);
    assert_eq!(background.paint_at(2.0).fallback_color(), third);
}

#[test]
fn shader_backgrounds_draw_the_render_device_texture_when_available() {
    let shader = crate::background::ShaderBackground::new(
        "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return vec4<f32>(uv, time + resolution.x * 0.0, 1.0);\n}",
        peniko::Color::BLACK,
    )
    .unwrap();
    let solid = peniko::Color::from_rgb8(10, 20, 30);
    let background = CanvasBackground {
        paint: BackgroundPaint::Shader(shader),
        segment_paints: vec![SegmentBackgroundPaint {
            start_time: 2.0,
            end_time: 3.0,
            paint: Some(BackgroundPaint::solid(solid)),
            hold_at_end: false,
        }],
        pixel_size: (960, 540),
        bounds: gaanim_math::Bounds3D::new_2d(-480.0, -270.0, 480.0, 270.0),
        parameters: Vec::new(),
    };
    let (rect, _) = canvas_background_geometry(&background);

    let mut requests = Vec::new();
    let (brush, transform) = resolve_canvas_background_brush(
        &background,
        rect,
        (480, 270),
        1.0,
        1.0,
        0.0,
        Some(&mut requests),
    );
    let [request] = requests.as_slice() else {
        panic!(
            "the shader frame is drawn on the render device: {}",
            requests.len()
        );
    };
    assert_eq!(request.time(), 1.0);
    let peniko::Brush::Image(image) = brush else {
        panic!("expected the placeholder image brush, got {brush:?}");
    };
    assert_eq!(image.image.data.id(), request.image().data.id());
    assert_eq!((image.image.width, image.image.height), (480, 270));
    let transform = transform.unwrap();
    assert_eq!(
        transform * kurbo::Point::ZERO,
        kurbo::Point::new(rect.x0, rect.y1),
        "the first shader row is the top edge of the y-up world"
    );
    assert_eq!(
        transform * kurbo::Point::new(480.0, 270.0),
        kurbo::Point::new(rect.x1, rect.y0)
    );

    // A resting presentation animates the shader further, in the same
    // segment.
    let mut requests = Vec::new();
    resolve_canvas_background_brush(
        &background,
        rect,
        (480, 270),
        1.0,
        1.0,
        4.0,
        Some(&mut requests),
    );
    assert_eq!(
        requests.first().expect("still the shader segment").time(),
        5.0
    );

    let mut requests = Vec::new();
    let (brush, _) = resolve_canvas_background_brush(
        &background,
        rect,
        (480, 270),
        2.5,
        2.5,
        4.0,
        Some(&mut requests),
    );
    assert!(requests.is_empty(), "solid segments need no shader frame");
    assert!(matches!(brush, peniko::Brush::Solid(color) if color == solid));
}

#[test]
fn glass_shows_the_shader_background_texture_the_frame_draws() {
    let shader = crate::background::ShaderBackground::new(
        "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return vec4<f32>(uv, time + resolution.x * 0.0, 1.0);\n}",
        peniko::Color::BLACK,
    )
    .unwrap();
    let background = CanvasBackground {
        paint: BackgroundPaint::Shader(shader.clone()),
        segment_paints: Vec::new(),
        pixel_size: (480, 270),
        bounds: gaanim_math::Bounds3D::new_2d(-8.0, -4.5, 8.0, 4.5),
        parameters: Vec::new(),
    };
    let canvas = Some((&background, background.pixel_size));
    let root = Entity::from_raw_u32(21).unwrap();
    let mut lens = group_member(root, 0.0, 1.0, 1.0);
    lens.glass_root = Some(root);
    lens.outline = Some(Arc::new(kurbo::Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1)));
    let mut elements = vec![group_member(Entity::PLACEHOLDER, -3.0, 1.0, 1.0), lens];
    let glasses = [CapturedGlass {
        root,
        glass: crate::object_effects::Glass::LIQUID,
    }];

    let mut requests = Vec::new();
    let layers = divert_glass(
        &mut elements,
        &glasses,
        canvas,
        1.0,
        0.0,
        10.0,
        Some(&mut requests),
    );
    assert_eq!(layers.len(), 1);
    let drawn = requests
        .first()
        .cloned()
        .expect("the glass reads the texture the renderer draws");
    // The canvas behind the glass draws the same texture.
    compose_frame(&elements, None, canvas, 1.0, 0.0, Some(&mut requests));
    assert_eq!(requests.len(), 1);
    assert!(requests[0].draws_same(&drawn));
    assert!(!shader.has_cpu_copy(), "nothing is copied through the CPU");

    // Another frame of the shader in the same scene cannot share the
    // texture, so it is copied through the CPU.
    let paint = fill_canvas_background(
        &mut vello::Scene::new(),
        &background,
        background.pixel_size,
        2.0,
        0.0,
        Some(&mut requests),
    );
    assert_eq!(requests.len(), 1);
    assert!(requests[0].draws_same(&drawn));
    assert!(!matches!(
        &paint.brush,
        peniko::Brush::Image(image) if image.image.data.id() == drawn.image().data.id()
    ));
    assert!(shader.has_cpu_copy());
}

#[test]
fn both_segments_of_a_transition_draw_their_shader_backgrounds_on_the_gpu() {
    let shader = |red: f32| {
        crate::background::ShaderBackground::new(
                format!(
                    "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {{\n\
                     return vec4<f32>({red:.1}, uv.y, time * 0.0 + resolution.x * 0.0, 1.0);\n}}"
                ),
                peniko::Color::BLACK,
            )
            .unwrap()
    };
    let (outgoing, incoming) = (shader(1.0), shader(0.0));
    let image = |shader: &crate::background::ShaderBackground| {
        shader.gpu_request(480, 270, 0.0).unwrap().image().data.id()
    };
    let (outgoing_image, incoming_image) = (image(&outgoing), image(&incoming));
    let segment = |start_time, end_time, shader: &crate::background::ShaderBackground| {
        SegmentBackgroundPaint {
            start_time,
            end_time,
            paint: Some(BackgroundPaint::Shader(shader.clone())),
            hold_at_end: false,
        }
    };
    let background = CanvasBackground {
        paint: BackgroundPaint::solid(peniko::Color::BLACK),
        segment_paints: vec![segment(0.0, 2.0, &outgoing), segment(2.0, 4.0, &incoming)],
        pixel_size: (480, 270),
        bounds: gaanim_math::Bounds3D::new_2d(-8.0, -4.5, 8.0, 4.5),
        parameters: Vec::new(),
    };
    let canvas = Some((&background, background.pixel_size));
    let blend = gaanim_scene::TransitionShaderFrame {
        shader: Arc::new(gaanim_scene::TransitionShader {
            source: String::new(),
            uniforms: Vec::new(),
            values: Vec::new(),
            data: None,
        }),
        progress: 0.5,
        values: Vec::new(),
    };
    let reveal = gaanim_scene::TransitionMask {
        path: kurbo::Rect::new(-2.0, -2.0, 2.0, 2.0).to_path(0.1),
        rule: peniko::Fill::NonZero,
        fade: None,
    };
    // A shader transition draws each side over its own background; a
    // geometric one draws the incoming background inside its reveal.
    for frame in [
        gaanim_scene::SceneTransitionFrame {
            shader: Some(blend),
            backgrounds: Some((1.9, 2.1)),
            ..Default::default()
        },
        gaanim_scene::SceneTransitionFrame {
            incoming_mask: Some(reveal),
            backgrounds: Some((1.9, 2.1)),
            ..Default::default()
        },
    ] {
        let mut requests = Vec::new();
        compose_frame(&[], Some(&frame), canvas, 2.0, 0.0, Some(&mut requests));
        assert_eq!(requests.len(), 2, "one texture per segment");
        assert_eq!(requests[0].image().data.id(), outgoing_image);
        assert_eq!(requests[1].image().data.id(), incoming_image);
        // Both keep animating through the transition.
        assert_eq!((requests[0].time(), requests[1].time()), (2.0, 2.0));
    }
    assert!(!outgoing.has_cpu_copy() && !incoming.has_cpu_copy());
}

#[test]
fn shader_background_uv_starts_at_the_top_left_of_the_displayed_canvas() {
    let Some(mut gpu) = crate::background::test_gpu::test_gpu() else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    let shader = crate::background::ShaderBackground::new(
        "fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return vec4<f32>(uv, time * 0.0 + resolution.x * 0.0, 1.0);\n}",
        peniko::Color::BLACK,
    )
    .unwrap();
    let (width, height) = (64, 36);
    let background = CanvasBackground {
        paint: BackgroundPaint::Shader(shader),
        segment_paints: Vec::new(),
        pixel_size: (width, height),
        bounds: gaanim_math::Bounds3D::new_2d(-8.0, -4.5, 8.0, 4.5),
        parameters: Vec::new(),
    };
    // The editor and exports both display world +y upwards.
    let world_to_screen = kurbo::Affine::new([4.0, 0.0, 0.0, -4.0, 32.0, 18.0]);
    let target = gpu.target(width, height);
    let mut backgrounds = crate::background::GpuShaderBackgrounds::default();
    for resident in [false, true] {
        let mut requests = Vec::new();
        let mut world = vello::Scene::new();
        fill_canvas_background(
            &mut world,
            &background,
            (width, height),
            0.0,
            0.0,
            resident.then_some(&mut requests),
        );
        if resident {
            backgrounds.prepare(&gpu.device, &gpu.queue, &mut gpu.renderer, &requests);
        }
        let mut screen = vello::Scene::new();
        screen.append(&world, Some(world_to_screen));
        gpu.render(&screen, &target);
        let pixels = gpu.read(&target);
        // Red and green carry uv at each pixel centre.
        let uv = |x: u32, y: u32| {
            let index = ((y * width + x) * 4) as usize;
            (pixels[index], pixels[index + 1])
        };
        let corners = [
            uv(0, 0),
            uv(width - 1, 0),
            uv(0, height - 1),
            uv(width - 1, height - 1),
        ];
        let low = |value: u8| value < 8;
        let high = |value: u8| value > 247;
        assert!(
            low(corners[0].0) && low(corners[0].1),
            "top-left is uv (0, 0): {corners:?}, resident: {resident}"
        );
        assert!(
            high(corners[1].0) && low(corners[1].1),
            "top-right: {corners:?}"
        );
        assert!(
            low(corners[2].0) && high(corners[2].1),
            "bottom-left: {corners:?}"
        );
        assert!(
            high(corners[3].0) && high(corners[3].1),
            "bottom-right: {corners:?}"
        );
    }
}

fn window(width: u32, height: u32) -> Window {
    Window {
        resolution: (width, height).into(),
        ..default()
    }
}

#[test]
fn perspective_viewport_keeps_canvas_size_in_taller_window() {
    let cam = gaanim_math::Camera::perspective_3d(1280, 720, 0.7);
    let viewport = gaanim_math::CameraViewport::default();

    let original = fitted_canvas_viewport(&cam, viewport, &window(1280, 720)).unwrap();
    let taller = fitted_canvas_viewport(&cam, viewport, &window(1280, 1000)).unwrap();

    assert_eq!(original.physical_size, UVec2::new(1280, 720));
    assert_eq!(taller.physical_size, original.physical_size);
    assert_eq!(taller.physical_position, UVec2::new(0, 140));
}

#[test]
fn perspective_viewport_scales_with_the_fitted_logical_canvas() {
    let cam = gaanim_math::Camera::perspective_3d(1280, 720, 0.7);
    let viewport = gaanim_math::CameraViewport {
        scale: 0.5,
        offset_y: 0.0,
        offset_x: 0.0,
    };

    let viewport = fitted_canvas_viewport(&cam, viewport, &window(640, 500)).unwrap();

    assert_eq!(viewport.physical_size, UVec2::new(640, 360));
    assert_eq!(viewport.physical_position, UVec2::new(0, 70));
}

#[test]
fn perspective_viewport_offset_is_measured_in_window_pixels() {
    let cam = gaanim_math::Camera::perspective_3d(1280, 720, 0.8);
    let mapping = gaanim_math::CameraViewport {
        scale: 0.5,
        offset_y: -100.0,
        offset_x: 0.0,
    };
    let viewport = fitted_canvas_viewport(&cam, mapping, &window(1280, 720)).unwrap();

    assert_eq!(viewport.physical_size, UVec2::new(640, 360));
    assert_eq!(viewport.physical_position, UVec2::new(320, 80));
}
