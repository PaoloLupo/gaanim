use super::*;

#[test]
fn progress_labels_settle_on_whole_digits() {
    // Float noise and repeating fractions never leave a wheel between digits.
    assert_eq!(rounded_label_value(0.29 * 100.0, 1.0), 29.0);
    assert_eq!(rounded_label_value(100.0 / 3.0, 1.0), 33.0);
    assert_eq!(rounded_label_value(66.7, 1.0), 67.0);
    assert!((rounded_label_value(92.5, 0.1) - 92.5).abs() < 1e-9);
    let rolling = rounded_label_value(12.5, 1.0);
    assert!(rolling > 12.0 && rolling < 13.0);
    // A countdown shows the seconds left and rolls into the next one.
    assert_eq!(countdown_label_value(10.0), 10.0);
    assert_eq!(countdown_label_value(9.99), 10.0);
    assert_eq!(countdown_label_value(9.5), 10.0);
    assert_eq!(countdown_label_value(9.0), 9.0);
    assert_eq!(countdown_label_value(0.0), 0.0);
    let rolling = countdown_label_value(9.05);
    assert!(rolling > 9.0 && rolling < 10.0);
    assert!(countdown_label_value(f64::NAN).is_nan());
}

#[test]
fn progress_rings_compile_an_arc_that_follows_their_parameter() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let mut other = SceneModel::new(16.0, 9.0);
    let foreign = other.parameter(1.0).unwrap();
    assert!(canvas.progress_arc(foreign.source(), 1.0, 1.0).is_err());
    assert!(
        canvas
            .progress_arc(ScalarSource::constant(0.5), 0.0, 1.0)
            .is_err()
    );
    let invalid = ProgressRingOptions {
        width: -1.0,
        ..Default::default()
    };
    assert!(canvas.progress_ring(0.0, invalid).is_err());

    let ring = canvas
        .progress_ring(
            0.25,
            ProgressRingOptions {
                radius: 2.0,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(ring.track.is_some() && ring.label.is_some());
    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let (arc, path, bounds, stroke) = world
        .query::<(
            &gaanim_animation::ProgressArc,
            &gaanim_scene::Path2D,
            &gaanim_scene::LocalBounds,
            &gaanim_scene::StrokeBrush,
        )>()
        .single(&world)
        .unwrap();
    assert_eq!(arc.maximum, 1.0);
    assert_eq!(*path.0, gaanim_animation::progress_arc_path(2.0, 0.25));
    assert_eq!(
        bounds.0,
        gaanim_math::Bounds3D::new_2d(-2.0, -2.0, 2.0, 2.0)
    );
    assert_eq!(stroke.style.start_cap, gaanim_core::kurbo::Cap::Round);
    // The percentage label is a rolling number of the scaled parameter.
    let rolling = world
        .query::<&gaanim_animation::RollingNumber>()
        .single(&world)
        .unwrap();
    assert_eq!(rolling.options.suffix, "%");
}

#[test]
fn rolling_number_compiles_native_geometry_and_validates_sources() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let options = gaanim_animation::RollingNumberOptions::default();
    assert!(
        canvas
            .rolling_number(ScalarSource::constant(f64::NAN), options.clone())
            .is_err()
    );
    let mut other = SceneModel::new(16.0, 9.0);
    let foreign = other.parameter(1.0).unwrap();
    assert!(
        canvas
            .rolling_number(foreign.source(), options.clone())
            .is_err()
    );
    let parameter = canvas.parameter(99.5).unwrap();
    canvas.rolling_number(parameter.source(), options).unwrap();
    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let (rolling, path, bounds) = world
        .query::<(
            &gaanim_animation::RollingNumber,
            &gaanim_scene::Path2D,
            &gaanim_scene::LocalBounds,
        )>()
        .single(&world)
        .unwrap();
    let (expected, expected_bounds) = rolling.geometry(99.5);
    let text_config = world.resource::<gaanim_text::prelude::TextConfig>();
    assert_eq!(
        rolling.options.font_family.as_deref(),
        Some(
            text_config.roles[&gaanim_text::prelude::TextRole::Body]
                .font_family
                .as_str()
        )
    );
    assert_eq!(*path.0, expected);
    assert_eq!(bounds.0, expected_bounds);
}

#[test]
fn continuous_rolling_number_settles_outside_parameter_tweens() {
    use gaanim_animation::{RollingMode, RollingNumber, RollingTweens};
    let mut canvas = SceneModel::new(16.0, 9.0);
    let parameter = canvas.parameter(0.0).unwrap();
    canvas
        .rolling_number(
            parameter.source(),
            gaanim_animation::RollingNumberOptions {
                decimals: 1,
                mode: RollingMode::Continuous,
                ..Default::default()
            },
        )
        .unwrap();
    canvas.wait(0.5);
    canvas.play(vec![parameter.animate().set(68.8).duration(2.0)]);
    canvas.wait(0.5);
    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let (rolling, tweens) = world
        .query::<(&RollingNumber, &RollingTweens)>()
        .single(&world)
        .unwrap();
    assert_eq!(tweens.0.len(), 1);
    let (start, duration) = tweens.0[0];
    assert!((start - 0.5).abs() < 1e-9 && (duration - 2.0).abs() < 1e-9);
    assert_eq!(tweens.continuous_weight(0.4), 0.0);
    assert_eq!(tweens.continuous_weight(1.5), 1.0);
    assert_eq!(tweens.continuous_weight(2.5), 0.0);
    // After the tween, wheels read exactly like an odometer at the final value.
    let mut odometer = rolling.clone();
    odometer.options.mode = RollingMode::Odometer;
    let settled = rolling
        .blended_geometry(68.8, tweens.continuous_weight(3.0))
        .0;
    assert_eq!(settled, odometer.geometry(68.8).0);
    assert_ne!(rolling.geometry(68.8).0, settled);
}

#[test]
fn rolling_number_inherits_custom_body_font_and_preserves_explicit_override() {
    for font in [None, Some("New Computer Modern")] {
        let mut canvas = SceneModel::new(16.0, 9.0);
        // The host text configuration only applies without a theme.
        canvas.clear_theme();
        canvas
            .rolling_number(
                ScalarSource::constant(42.0),
                gaanim_animation::RollingNumberOptions {
                    font_family: font.map(str::to_owned),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        let mut config = gaanim_text::prelude::TextConfig::default();
        config
            .roles
            .get_mut(&gaanim_text::prelude::TextRole::Body)
            .unwrap()
            .font_family = "monospace".into();
        world.insert_resource(config);
        canvas.compile(&mut world);
        world.flush();
        let rolling = world
            .query::<&gaanim_animation::RollingNumber>()
            .single(&world)
            .unwrap();
        assert_eq!(
            rolling.options.font_family.as_deref(),
            Some(font.unwrap_or("monospace"))
        );
    }
}

#[test]
fn rolling_number_baseline_anchors_ignore_wheel_padding_and_handle_transforms() {
    use gaanim_core::kurbo::Shape;
    use gaanim_text::prelude::TextAnchor;
    for anchor in [
        TextAnchor::BaselineLeft,
        TextAnchor::BaselineCenter,
        TextAnchor::BaselineRight,
    ] {
        for line_height in [1.0, 2.0] {
            let mut canvas = SceneModel::new(16.0, 9.0);
            canvas
                .rolling_number(
                    ScalarSource::constant(2.0),
                    gaanim_animation::RollingNumberOptions {
                        min_digits: 2,
                        font_size: 1.0,
                        line_height,
                        ..Default::default()
                    },
                )
                .unwrap()
                .at_text_anchor(-2.87, 1.12, anchor)
                .scale_to(1.3)
                .rotate_to(0.2);
            let mut world = bevy::prelude::World::new();
            world.insert_resource(gaanim_timeline::timeline::Timeline::new());
            world.insert_resource(gaanim_text::font::FontRegistry::new());
            world.insert_resource(gaanim_text::prelude::TextConfig::default());
            canvas.compile(&mut world);
            world.flush();
            let (rolling, bounds, baseline, transform) = world
                .query::<(
                    &gaanim_animation::RollingNumber,
                    &gaanim_scene::LocalBounds,
                    &gaanim_scene::TextBaseline,
                    &gaanim_math::SpatialTransform,
                )>()
                .single(&world)
                .unwrap();
            let digits = gaanim_text::typst_compiler::compile_typst_text_to_path(
                world.resource::<gaanim_text::font::FontRegistry>(),
                "0123456789",
                rolling.options.font_family.as_deref().unwrap(),
                rolling.options.weight,
                1.0,
            )
            .unwrap();
            let ink = digits.bounding_box();
            assert!((baseline.0 + (ink.y0 + ink.y1) * 0.5).abs() < 1e-9);
            let x = match anchor {
                TextAnchor::BaselineLeft => bounds.0.min.x,
                TextAnchor::BaselineCenter => bounds.0.center().x,
                TextAnchor::BaselineRight => bounds.0.max.x,
            };
            let actual = transform
                .to_mat4()
                .transform_point3(DVec3::new(x, baseline.0, 0.0));
            assert!(
                actual.distance(DVec3::new(-2.87, 1.12, 0.0)) < 1e-9,
                "{anchor:?}: {actual:?}"
            );
        }
    }
}

/// Whether a semantic layer draws nothing; groups are empty when every
/// member is, as Cartesian grid and tick layers wrap their paths.
fn layer_is_empty(canvas: &SceneModel, handle: &DrawableHandle) -> bool {
    use super::super::ops::SharedObjectSpec;

    fn spec_is_empty(canvas: &SceneModel, spec: &SharedObjectSpec) -> bool {
        let children = {
            let spec = spec.lock().expect("layer spec poisoned");
            match &spec.kind {
                SpawnKind::SvgPath(path) => return path.path.elements().is_empty(),
                SpawnKind::LineSegments3D { points, .. } => return points.is_empty(),
                SpawnKind::Group(children) | SpawnKind::GroupNoCenter(children) => children.clone(),
                // Text and other members always draw something.
                _ => return false,
            }
        };
        let members: Vec<SharedObjectSpec> = {
            let state = canvas.state.lock().expect("canvas state poisoned");
            children
                .iter()
                .map(|id| {
                    state
                        .segments
                        .iter()
                        .flat_map(|segment| &segment.ops)
                        .find_map(|op| match op {
                            Op::Spawn(spec)
                                if spec.lock().expect("object spec poisoned").id == *id =>
                            {
                                Some(spec.clone())
                            }
                            _ => None,
                        })
                        .expect("layer member must have a spawn spec")
                })
                .collect()
        };
        members.iter().all(|member| spec_is_empty(canvas, member))
    }
    spec_is_empty(canvas, &handle.spec)
}

fn group_child_translations(canvas: &SceneModel, group: &DrawableHandle) -> Vec<DVec3> {
    let group_spec = group.spec.lock().expect("group spec poisoned");
    let children = match &group_spec.kind {
        SpawnKind::Group(children) | SpawnKind::GroupNoCenter(children) => children,
        _ => panic!("expected a group"),
    };
    let children = children.clone();
    drop(group_spec);
    let state = canvas.state.lock().expect("canvas state poisoned");
    children
        .iter()
        .map(|id| {
            let spec = state
                .active()
                .ops
                .iter()
                .find_map(|op| match op {
                    Op::Spawn(spec) if spec.lock().expect("object spec poisoned").id == *id => {
                        Some(spec.clone())
                    }
                    _ => None,
                })
                .expect("label must have a spawn spec");
            spec.lock()
                .expect("label spec poisoned")
                .layout_ops
                .iter()
                .rev()
                .find_map(|op| match op {
                    super::super::types::LayoutOp::SetTranslation(value) => Some(*value),
                    super::super::types::LayoutOp::MoveAnchorTo { target, .. } => Some(*target),
                    _ => None,
                })
                .expect("label must have an authored translation")
        })
        .collect()
}

#[test]
fn cartesian_visibility_keeps_disabled_layers_available_and_mapping_stable() {
    let mut canvas = SceneModel::new(640, 360);
    let x = Axis::linear(-2.0, 2.0)
        .unwrap()
        .ticks(1.0)
        .unwrap()
        .minor_ticks(2)
        .label("x");
    let y = Axis::linear(-1.0, 1.0)
        .unwrap()
        .ticks(0.5)
        .unwrap()
        .label("y");
    let space = canvas
        .coordinate_axes_with_visibility(
            x,
            y,
            Some(400.0),
            Some(200.0),
            CartesianVisibility {
                x_grid: true,
                y_grid: false,
                x_axis: false,
                y_axis: false,
                x_ticks: false,
                y_ticks: false,
                x_numbers: false,
                y_numbers: false,
                x_labels: false,
                y_labels: false,
            },
        )
        .unwrap();

    assert_eq!(space.data_to_local(1.0, 0.5).unwrap(), (100.0, 50.0));
    assert!(!layer_is_empty(
        &canvas,
        space.layer(SpaceLayer::MajorGrid).unwrap()
    ));
    assert!(!layer_is_empty(
        &canvas,
        space.layer(SpaceLayer::MinorGrid).unwrap()
    ));
    for layer in [
        SpaceLayer::Axes,
        SpaceLayer::Ticks,
        SpaceLayer::Numbers,
        SpaceLayer::Labels,
    ] {
        assert!(layer_is_empty(&canvas, space.layer(layer).unwrap()));
    }
}

#[test]
fn three_dimensional_visibility_filters_grid_planes_and_annotations() {
    let mut canvas = SceneModel::new(640, 360);
    let space = canvas
        .coordinate_axes_3d_with_visibility(
            Axis::linear(-2.0, 2.0)
                .unwrap()
                .ticks(1.0)
                .unwrap()
                .label("x"),
            Axis::linear(-2.0, 2.0)
                .unwrap()
                .ticks(1.0)
                .unwrap()
                .label("y"),
            Axis::linear(-2.0, 2.0)
                .unwrap()
                .ticks(1.0)
                .unwrap()
                .label("z"),
            [4.0, 4.0, 4.0],
            Cartesian3DVisibility {
                x_axis: false,
                y_axis: false,
                z_axis: false,
                xy_grid: true,
                xz_grid: false,
                yz_grid: false,
                x_ticks: false,
                y_ticks: false,
                z_ticks: false,
                x_numbers: false,
                y_numbers: false,
                z_numbers: false,
                x_labels: false,
                y_labels: false,
                z_labels: false,
            },
        )
        .unwrap();

    for layer in [
        SpaceLayer::Axes,
        SpaceLayer::Ticks,
        SpaceLayer::Numbers,
        SpaceLayer::Labels,
    ] {
        assert!(layer_is_empty(&canvas, space.layer(layer).unwrap()));
    }
    assert!(!layer_is_empty(
        &canvas,
        space.layer(SpaceLayer::MajorGrid).unwrap()
    ));
    // Only the xy plane draws, built on the box's lowest face.
    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let points: Vec<[f32; 3]> = world
        .query::<&gaanim_scene::LineListData>()
        .iter(&world)
        .flat_map(|line| line.points.clone())
        .collect();
    assert!(!points.is_empty());
    assert!(
        points
            .iter()
            .all(|point| (point[2] + 2.0).abs() < f32::EPSILON)
    );
}

#[test]
fn polar_and_number_line_visibility_keep_empty_layers_addressable() {
    let mut canvas = SceneModel::new(640, 360);
    let polar = canvas
        .coordinate_polar_plane_with_visibility(
            Axis::linear(0.0, 4.0)
                .unwrap()
                .ticks(1.0)
                .unwrap()
                .label("r"),
            160.0,
            8,
            PolarVisibility {
                rings: false,
                spokes: false,
                axes: false,
                numbers: false,
                labels: false,
            },
        )
        .unwrap();
    for layer in [
        SpaceLayer::MajorGrid,
        SpaceLayer::Axes,
        SpaceLayer::Numbers,
        SpaceLayer::Labels,
    ] {
        assert!(layer_is_empty(&canvas, polar.layer(layer).unwrap()));
    }

    let spokes_only = canvas
        .coordinate_polar_plane_with_visibility(
            Axis::linear(0.0, 4.0)
                .unwrap()
                .ticks(1.0)
                .unwrap()
                .label("r"),
            160.0,
            8,
            PolarVisibility {
                rings: false,
                spokes: true,
                axes: false,
                numbers: false,
                labels: true,
            },
        )
        .unwrap();
    let grid = spokes_only.layer(SpaceLayer::MajorGrid).unwrap();
    let spec = grid.spec.lock().expect("polar grid spec poisoned");
    let SpawnKind::SvgPath(path) = &spec.kind else {
        panic!("expected a polar grid path")
    };
    assert_eq!(path.path.elements().len(), 16);
    drop(spec);
    assert!(!layer_is_empty(
        &canvas,
        spokes_only.layer(SpaceLayer::Labels).unwrap()
    ));

    let line = canvas
        .coordinate_number_line_with_visibility(
            Axis::linear(0.0, 4.0)
                .unwrap()
                .ticks(1.0)
                .unwrap()
                .label("t"),
            Some(400.0),
            NumberLineVisibility {
                axis: false,
                ticks: false,
                numbers: false,
                labels: false,
            },
        )
        .unwrap();
    for layer in [
        SpaceLayer::Axes,
        SpaceLayer::Ticks,
        SpaceLayer::Numbers,
        SpaceLayer::Labels,
    ] {
        assert!(layer_is_empty(&canvas, line.layer(layer).unwrap()));
    }
}

fn group_child_anchors(canvas: &SceneModel, group: &DrawableHandle) -> Vec<Option<Anchor>> {
    let group_spec = group.spec.lock().expect("group spec poisoned");
    let children = match &group_spec.kind {
        SpawnKind::Group(children) | SpawnKind::GroupNoCenter(children) => children.clone(),
        _ => panic!("expected a group"),
    };
    drop(group_spec);
    let state = canvas.state.lock().expect("canvas state poisoned");
    children
        .iter()
        .map(|id| {
            let spec = state
                .active()
                .ops
                .iter()
                .find_map(|op| match op {
                    Op::Spawn(spec) if spec.lock().expect("object spec poisoned").id == *id => {
                        Some(spec.clone())
                    }
                    _ => None,
                })
                .expect("label must have a spawn spec");
            spec.lock()
                .expect("label spec poisoned")
                .layout_ops
                .iter()
                .rev()
                .find_map(|op| match op {
                    super::super::types::LayoutOp::MoveAnchorTo { anchor, .. } => Some(*anchor),
                    _ => None,
                })
        })
        .collect()
}

fn group_child_rotations(canvas: &SceneModel, group: &DrawableHandle) -> Vec<f64> {
    let group_spec = group.spec.lock().expect("group spec poisoned");
    let children = match &group_spec.kind {
        SpawnKind::Group(children) | SpawnKind::GroupNoCenter(children) => children,
        _ => panic!("expected a group"),
    };
    let children = children.clone();
    drop(group_spec);
    let state = canvas.state.lock().expect("canvas state poisoned");
    children
        .iter()
        .map(|id| {
            let spec = state
                .active()
                .ops
                .iter()
                .find_map(|op| match op {
                    Op::Spawn(spec) if spec.lock().expect("object spec poisoned").id == *id => {
                        Some(spec.clone())
                    }
                    _ => None,
                })
                .expect("label must have a spawn spec");
            spec.lock()
                .expect("label spec poisoned")
                .layout_ops
                .iter()
                .rev()
                .find_map(|op| match op {
                    super::super::types::LayoutOp::SetRotation(value) => Some(*value),
                    _ => None,
                })
                .expect("label must have an authored rotation")
        })
        .collect()
}

#[test]
fn cartesian_axis_titles_default_beyond_positive_ends_and_support_multiline_text() {
    let mut canvas = SceneModel::new(640, 360);
    let space = canvas
        .coordinate_axes(
            Axis::category(["Ladrillo\no bloque".into(), "Piedra\ncon barro".into()])
                .unwrap()
                .label("Material predominante"),
            Axis::linear(0.0, 70.0)
                .unwrap()
                .ticks(10.0)
                .unwrap()
                .label("Viviendas\nprueba (%)"),
            Some(640.0),
            Some(360.0),
            true,
        )
        .unwrap();
    let labels = group_child_translations(
        &canvas,
        space.layer(SpaceLayer::Labels).expect("axis titles"),
    );
    let rotations = group_child_rotations(
        &canvas,
        space.layer(SpaceLayer::Labels).expect("axis titles"),
    );
    let anchors = group_child_anchors(
        &canvas,
        space.layer(SpaceLayer::Labels).expect("axis titles"),
    );

    assert_eq!(labels.len(), 2);
    assert!((labels[0].x - 320.12).abs() < 1e-9);
    assert!((labels[1].y - 180.12).abs() < 1e-9);
    assert_eq!(anchors, [Some(Anchor::Left), Some(Anchor::Bottom)]);
    assert_eq!(rotations, [0.0, 0.0], "axis titles remain upright");
    let axis_origin = space
        .map
        .data_to_local(space.map.x.crossing_value(), space.map.y.crossing_value())
        .unwrap();
    assert!((labels[0].y - axis_origin.y).abs() < 1e-9);
    assert!((labels[1].x - axis_origin.x).abs() < 1e-9);
}

#[test]
fn multiline_category_ticks_align_their_first_line_with_single_line_ticks() {
    let mut canvas = SceneModel::new(400, 200);
    // Unthemed axes keep the unscaled tick-number size.
    canvas.clear_theme();
    let space = canvas
        .coordinate_axes(
            Axis::category(["Una línea".into(), "Dos\nlíneas".into()]).unwrap(),
            Axis::linear(0.0, 1.0).unwrap(),
            Some(400.0),
            Some(200.0),
            true,
        )
        .unwrap();
    let numbers = group_child_translations(
        &canvas,
        space.layer(SpaceLayer::Numbers).expect("axis tick labels"),
    );

    let expected_offset = canvas.x_tick_label_extra_offset("Dos\nlíneas", 1.0)
        - canvas.x_tick_label_extra_offset("Una línea", 1.0);
    assert!(
        ((numbers[0].y - numbers[1].y) - expected_offset).abs() < 1e-9,
        "multiline labels grow away from the axis without changing their nearest-line gap"
    );
}

#[test]
fn single_line_x_tick_labels_use_a_logical_gap() {
    let canvas = SceneModel::new(16.0, 9.0);
    assert_eq!(canvas.x_tick_label_extra_offset("Categoría", 1.0), 0.04);
}

#[test]
fn centered_axis_titles_use_conventional_outer_sides() {
    let mut canvas = SceneModel::new(400, 200);
    let space = canvas
        .coordinate_axes(
            Axis::category(["Una línea".into(), "Dos\nlíneas".into()])
                .unwrap()
                .label("Categoría")
                .label_position(AxisLabelPosition::Center),
            Axis::linear(-10.0, 10.0)
                .unwrap()
                .label("Valor")
                .label_position(AxisLabelPosition::Center),
            Some(400.0),
            Some(200.0),
            true,
        )
        .unwrap();
    let numbers = group_child_translations(
        &canvas,
        space.layer(SpaceLayer::Numbers).expect("axis tick labels"),
    );
    let labels = group_child_translations(
        &canvas,
        space.layer(SpaceLayer::Labels).expect("axis titles"),
    );
    let rotations = group_child_rotations(
        &canvas,
        space.layer(SpaceLayer::Labels).expect("axis titles"),
    );

    assert!(numbers[0].y < 0.0 && numbers[1].y < numbers[0].y);
    assert_eq!(labels.len(), 2);
    assert!(
        labels[0].y < numbers[1].y,
        "centered x title clears tick labels below"
    );
    assert!(
        labels[1].x < -200.0,
        "centered y title stays left of the axis"
    );
    assert!(labels[0].x.abs() < 1e-9 && labels[1].y.abs() < 1e-9);
    assert_eq!(rotations[0], 0.0);
    assert!((rotations[1] - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
}

#[test]
fn cartesian_axis_titles_can_move_to_the_axis_end() {
    let mut canvas = SceneModel::new(400, 200);
    let space = canvas
        .coordinate_axes(
            Axis::linear(0.0, 4.0)
                .unwrap()
                .label("x")
                .label_position(AxisLabelPosition::End),
            Axis::linear(0.0, 2.0)
                .unwrap()
                .label("y")
                .label_position(AxisLabelPosition::End),
            Some(400.0),
            Some(200.0),
            true,
        )
        .unwrap();
    let labels = group_child_translations(
        &canvas,
        space.layer(SpaceLayer::Labels).expect("axis titles"),
    );
    let rotations = group_child_rotations(
        &canvas,
        space.layer(SpaceLayer::Labels).expect("axis titles"),
    );
    let anchors = group_child_anchors(
        &canvas,
        space.layer(SpaceLayer::Labels).expect("axis titles"),
    );

    assert_eq!(labels.len(), 2);
    assert_eq!(rotations, [0.0, 0.0]);
    assert_eq!(labels[0].x, 200.12);
    assert_eq!(labels[1].y, 100.12);
    assert_eq!(anchors, [Some(Anchor::Left), Some(Anchor::Bottom)]);
}

#[test]
fn vertical_axis_numbers_are_right_aligned_at_the_tick_gap() {
    let mut canvas = SceneModel::new(400, 200);
    // Unthemed axes keep the unscaled tick-number size.
    canvas.clear_theme();
    let y = Axis::linear(-12500.0, 12500.0)
        .unwrap()
        .ticks(5000.0)
        .unwrap();
    let space = canvas
        .coordinate_axes(
            Axis::linear(0.0, 10000.0).unwrap().ticks(2500.0).unwrap(),
            y.clone(),
            Some(400.0),
            Some(200.0),
            true,
        )
        .unwrap();
    let numbers = group_child_translations(
        &canvas,
        space.layer(SpaceLayer::Numbers).expect("axis tick labels"),
    );
    // The x crossing (0) hides the y number at zero.
    let labels = ["\u{2212}10000", "\u{2212}5000", "5000", "10000"];
    let y_numbers = &numbers[numbers.len() - labels.len()..];
    let gap_edge = -200.0 - y.style_value().tick_length - 0.12;
    let widths = labels.map(|label| canvas.tick_number_width(label, 1.0));
    assert!(widths[0] > widths[2], "labels of different length differ");
    for ((label, number), width) in labels.iter().zip(y_numbers).zip(widths) {
        let right = number.x + width * 0.5;
        assert!(
            (right - gap_edge).abs() < 1e-9,
            "{label} ends at {right}, expected {gap_edge}"
        );
    }
    // x numbers stay centered on their ticks.
    assert!((numbers[1].x - (-100.0)).abs() < 1e-9);
}

fn svg_stroke_color(handle: &DrawableHandle) -> Color {
    let spec = handle.spec.lock().expect("object spec poisoned");
    let SpawnKind::SvgPath(path) = &spec.kind else {
        panic!("expected an SVG path")
    };
    let Some(gaanim_core::peniko::Brush::Solid(color)) = &path.stroke.brush else {
        panic!("expected a solid stroke")
    };
    *color
}

#[test]
fn paper_theme_colors_number_line_parts_unless_authored() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_theme("paper").unwrap();
    let themed = canvas
        .coordinate_number_line(Axis::linear(0.0, 4.0).unwrap(), Some(400.0))
        .unwrap();
    assert_eq!(
        svg_stroke_color(themed.layer(SpaceLayer::Axes).unwrap()),
        Color::BLACK
    );
    assert_eq!(
        svg_stroke_color(themed.layer(SpaceLayer::Ticks).unwrap()),
        Color::BLACK
    );

    let width_only = canvas
        .coordinate_number_line(
            Axis::linear(0.0, 4.0)
                .unwrap()
                .style_patch(gaanim_visualization::AxisStylePatch {
                    width: Some(7.0),
                    ..Default::default()
                }),
            Some(400.0),
        )
        .unwrap();
    assert_eq!(
        svg_stroke_color(width_only.layer(SpaceLayer::Axes).unwrap()),
        Color::BLACK,
        "a non-color override must not disconnect the axis from its theme color"
    );

    let authored = Color::from_rgb8(0xA1, 0x23, 0x45);
    let overridden = canvas
        .coordinate_number_line(
            Axis::linear(0.0, 4.0)
                .unwrap()
                .style_patch(gaanim_visualization::AxisStylePatch {
                    color: Some(authored),
                    tick_color: Some(authored),
                    ..Default::default()
                }),
            Some(400.0),
        )
        .unwrap();
    assert_eq!(
        svg_stroke_color(overridden.layer(SpaceLayer::Axes).unwrap()),
        authored
    );
    assert_eq!(
        svg_stroke_color(overridden.layer(SpaceLayer::Ticks).unwrap()),
        authored
    );
}

#[test]
fn number_line_point_ref_uses_reactive_local_coordinates() {
    let mut canvas = SceneModel::new(640, 360);
    let line = canvas
        .coordinate_number_line(
            Axis::linear(0.0, std::f64::consts::TAU).unwrap(),
            Some(600.0),
        )
        .unwrap();
    let point = line
        .point_ref(
            ScalarSource::constant(std::f64::consts::PI),
            ScalarSource::constant(42.0),
        )
        .unwrap();

    let CanvasEndpoint::LocalNumberLine {
        space,
        axis,
        length,
        value,
        normal_offset,
    } = point.0
    else {
        panic!("number-line points must stay in the line's local frame");
    };
    assert_eq!(space, line.drawable().id);
    assert_eq!(axis.domain(), (0.0, std::f64::consts::TAU));
    assert_eq!(length, 600.0);
    assert_eq!(value.evaluate(0.0, |_| None).unwrap(), std::f64::consts::PI);
    assert_eq!(normal_offset.evaluate(0.0, |_| None).unwrap(), 42.0);
}

#[test]
fn number_line_default_placement_keeps_its_authored_axis_origin() {
    let mut canvas = SceneModel::new(640, 360);
    let line = canvas
        .coordinate_number_line(
            Axis::linear(0.0, std::f64::consts::TAU)
                .unwrap()
                .ticks(std::f64::consts::PI)
                .unwrap(),
            Some(600.0),
        )
        .unwrap();
    line.drawable().clone().move_to_default(-250.0, 0.0);

    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let transform = world
        .query::<(
            &gaanim_math::SpatialTransform,
            Option<&bevy::prelude::ChildOf>,
        )>()
        .iter(&world)
        .find_map(|(transform, parent)| parent.is_none().then_some(transform))
        .expect("number-line root must be compiled");
    assert_eq!(transform.translation, DVec3::new(-250.0, 0.0, 0.0));
}

#[test]
fn number_line_function_uses_one_reactive_path() {
    let mut canvas = SceneModel::new(640, 360);
    let amplitude = canvas.parameter(1.0).unwrap();
    let line = canvas
        .coordinate_number_line(Axis::linear(0.0, 6.0).unwrap(), Some(480.0))
        .unwrap();
    canvas
        .number_line_reactive_plot(
            &line,
            ReactiveFunction::new(
                1,
                1,
                vec![gaanim_animation::ReactiveInput::Signal(
                    amplitude.drawable().id,
                )],
                |values| Ok(vec![values[1] * values[0].sin()]),
            ),
            (0.0, 6.0),
            80.0,
            None,
            Sampling::Fixed { samples: 128 },
        )
        .unwrap();

    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    assert_eq!(
        world
            .query::<&gaanim_animation::AlwaysRedrawRegen>()
            .iter(&world)
            .count(),
        1,
        "a sampled number-line function must remain one retained path",
    );
}

#[test]
fn coordinate_view_preserves_glyph_shape_during_animation_and_rewind() {
    use bevy::prelude::Schedule;
    use gaanim_math::GlobalSpatialTransform;
    use gaanim_scene::prelude::{ChildOf, World};

    let mut canvas = SceneModel::new(16.0, 9.0);
    let space = canvas
        .coordinate_axes(
            Axis::linear(-4.0, 4.0)
                .unwrap()
                .ticks(1.0)
                .unwrap()
                .label("x"),
            Axis::linear(-3.0, 3.0)
                .unwrap()
                .ticks(1.0)
                .unwrap()
                .label("y"),
            Some(10.0),
            Some(5.0),
            true,
        )
        .unwrap()
        .scale_to(1.2)
        .rotate_to(0.2)
        .move_to(1.0, -0.5);
    canvas.play(vec![
        space
            .view_to_animation((-1.0, 3.0), (-3.0, 3.0))
            .unwrap()
            .duration(1.0),
    ]);
    canvas.play(vec![
        space
            .view_to_animation((-4.0, 4.0), (-1.0, 2.0))
            .unwrap()
            .duration(1.0),
    ]);
    canvas.wait(0.2);
    space.view_to((-4.0, 4.0), (-3.0, 3.0)).unwrap();
    canvas.wait(0.2);

    let mut world = World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world
        .remove_resource::<gaanim_timeline::timeline::Timeline>()
        .unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );
    let mut propagation = Schedule::default();
    propagation.add_systems(gaanim_scene::transform_propagation_system);
    timeline.seek(&mut world, 0.0);
    propagation.run(&mut world);
    let glyphs: Vec<_> = world
        .query::<(
            gaanim_scene::prelude::Entity,
            &gaanim_scene::components::TextSpan,
            &GlobalSpatialTransform,
        )>()
        .iter(&world)
        .map(|(entity, _, global)| (entity, global.mat4))
        .collect();
    assert!(!glyphs.is_empty(), "axes must compile real text glyphs");

    for time in [0.5, 1.0, 1.5, 2.0, 2.3, 0.5, 0.0, 1.5] {
        timeline.seek(&mut world, time);
        propagation.run(&mut world);
        for (entity, initial) in &glyphs {
            let current = world.get::<GlobalSpatialTransform>(*entity).unwrap().mat4;
            for direction in [DVec3::X, DVec3::Y] {
                assert!(
                    (current.transform_vector3(direction) - initial.transform_vector3(direction))
                        .length()
                        < 1e-9,
                    "domain view deformed a glyph at t={time}"
                );
            }
            // Glyphs must still inherit their text group's corrected transform.
            let parent = world.get::<ChildOf>(*entity).unwrap().parent();
            let expected = world.get::<GlobalSpatialTransform>(parent).unwrap().mat4
                * world
                    .get::<gaanim_math::SpatialTransform>(*entity)
                    .unwrap()
                    .to_mat4();
            assert!(current.abs_diff_eq(expected, 1e-9));
        }
    }
}

#[test]
fn curves_resample_for_every_authored_view_window() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let space = canvas
        .coordinate_axes(
            Axis::linear(0.0, 1.0).unwrap(),
            Axis::linear(0.0, 10.0).unwrap(),
            Some(8.0),
            Some(5.0),
            true,
        )
        .unwrap();
    let line = || ReactiveFunction::new(1, 1, Vec::new(), |values| Ok(vec![10.0 * values[0]]));
    let automatic = canvas
        .reactive_plot_in_view(&space, line(), None, Sampling::default())
        .unwrap();
    let explicit = canvas
        .reactive_plot_in_view(&space, line(), Some((0.2, 0.6)), Sampling::default())
        .unwrap();
    let curve = |handle: &DrawableHandle| {
        let frozen = handle
            .state
            .lock()
            .unwrap()
            .frozen_spawn_specs
            .get(&handle.id)
            .map(|spec| spec.kind.clone());
        let live = handle.spec.lock().unwrap().kind.clone();
        let read = |kind: SpawnKind| match kind {
            SpawnKind::ReactivePlot {
                domain,
                sampling:
                    Sampling::Adaptive {
                        tolerance,
                        max_depth,
                        ..
                    },
                ..
            } => (domain, tolerance, max_depth),
            other => panic!("expected an adaptive plot, got {other:?}"),
        };
        let live = read(live);
        if let Some(frozen) = frozen {
            assert_eq!(read(frozen), live, "frozen and live specs must agree");
        }
        live
    };
    assert_eq!(curve(&automatic), ((0.0, 1.0), 0.0075, 8));

    // Played first, so the declaration is frozen; zoom out to 0..1.6.
    canvas.play(vec![automatic.animate().fade_in().duration(0.5)]);
    canvas
        .coordinate_view_to(&space, (0.0, 1.6), (0.0, 16.0))
        .unwrap();
    assert_eq!(
        curve(&automatic).0,
        (0.0, 1.6),
        "the line reaches the window edge"
    );
    assert_eq!(curve(&explicit).0, (0.2, 0.6), "explicit domains are kept");

    // Zoom in 4x on y: the tolerance keeps the on-screen accuracy.
    canvas
        .coordinate_view_to_animation(&space, (0.0, 1.0), (4.0, 6.5))
        .unwrap();
    let (domain, tolerance, depth) = curve(&automatic);
    assert_eq!(domain, (0.0, 1.6));
    assert!((tolerance - 0.0075 / 4.0).abs() < 1e-15);
    assert_eq!(depth, 10);
    assert!((curve(&explicit).1 - 0.0075 / 4.0).abs() < 1e-15);

    // A curve declared after the views starts with their reach.
    let late = canvas
        .reactive_plot_in_view(&space, line(), None, Sampling::default())
        .unwrap();
    assert_eq!(curve(&late).0, (0.0, 1.6));
}

#[test]
fn data_to_scene_follows_the_view_and_the_space_transform() {
    use bevy::prelude::Schedule;
    use gaanim_math::GlobalSpatialTransform;
    use gaanim_scene::prelude::World;

    let mut canvas = SceneModel::new(16.0, 9.0);
    let space = canvas
        .coordinate_axes(
            Axis::linear(-4.0, 4.0).unwrap(),
            Axis::linear(-3.0, 3.0).unwrap(),
            Some(10.0),
            Some(5.0),
            true,
        )
        .unwrap()
        .scale_to(1.2)
        .rotate_to(0.2)
        .move_to(1.0, -0.5);
    // Two markers on the same data point; found again below by radius.
    canvas
        .circle(0.05)
        .at_coordinate(space.coord(2.0, 1.5).unwrap());
    canvas.circle(0.07).follow_endpoint(
        space.data_to_scene(2.0, 1.5).unwrap().0,
        DVec3::ZERO,
        gaanim_animation::FollowOffsetSpace::World,
    );
    assert!(space.data_to_scene(f64::NAN, 0.0).is_err());
    canvas.play(vec![
        space
            .view_to_animation((0.0, 4.0), (0.0, 3.0))
            .unwrap()
            .duration(1.0),
    ]);
    canvas.wait(0.2);

    let mut world = World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world
        .remove_resource::<gaanim_timeline::timeline::Timeline>()
        .unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );
    // Identify the two markers by their distinct radii.
    let mut by_radius = |radius: f64| {
        world
            .query::<(gaanim_scene::prelude::Entity, &gaanim_scene::LocalBounds)>()
            .iter(&world)
            .find(|(_, bounds)| (bounds.0.width() - 2.0 * radius).abs() < 1e-9)
            .map(|(entity, _)| entity)
            .expect("compiled marker")
    };
    let (pinned, follower) = (by_radius(0.05), by_radius(0.07));
    let mut propagation = Schedule::default();
    propagation.add_systems(gaanim_scene::transform_propagation_system);
    let mut positions = Vec::new();
    for time in [0.0, 0.5, 1.0, 1.2, 0.3] {
        timeline.seek(&mut world, time);
        propagation.run(&mut world);
        gaanim_animation::endpoint_follow_system(&mut world);
        propagation.run(&mut world);
        let at = |entity| {
            world
                .get::<GlobalSpatialTransform>(entity)
                .unwrap()
                .mat4
                .transform_point3(DVec3::ZERO)
        };
        let (expected, actual) = (at(pinned), at(follower));
        assert!(
            (expected - actual).length() < 1e-9,
            "t={time}: {actual:?} != {expected:?}"
        );
        positions.push(actual);
    }
    assert!(
        (positions[0] - positions[2]).length() > 0.1,
        "the view change must move the data point in the scene"
    );
}

/// `place` with an anchor puts that point of the box on the data point,
/// then moves it by an offset in scene units, in a scaled, turned space.
#[test]
fn coordinates_place_by_an_anchor_and_a_scene_offset() {
    use bevy::prelude::Schedule;
    use gaanim_math::GlobalSpatialTransform;
    use gaanim_scene::prelude::World;

    let mut canvas = SceneModel::new(16.0, 9.0);
    let space = canvas
        .coordinate_axes(
            Axis::linear(-4.0, 4.0).unwrap(),
            Axis::linear(-3.0, 3.0).unwrap(),
            Some(10.0),
            Some(5.0),
            true,
        )
        .unwrap()
        .scale_to(1.2)
        .rotate_to(0.2)
        .move_to(1.0, -0.5);
    let point = space.coord(2.0, 1.5).unwrap();
    // Centered on the data point, as before.
    canvas.circle(0.0371).at_coordinate(point);
    canvas.rect(0.83, 0.4).at_coordinate_by(
        point,
        Some(gaanim_layout::Anchor::Left),
        DVec2::new(0.5, 0.0),
    );
    let mut world = World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut propagation = Schedule::default();
    propagation.add_systems(gaanim_scene::transform_propagation_system);
    propagation.run(&mut world);
    let mut by_width = |width: f64| {
        world
            .query::<(gaanim_scene::prelude::Entity, &gaanim_scene::LocalBounds)>()
            .iter(&world)
            .find(|(_, bounds)| (bounds.0.width() - width).abs() < 1e-9)
            .map(|(entity, bounds)| (entity, bounds.0))
            .expect("compiled shape")
    };
    let ((marker, _), (entity, bounds)) = (by_width(0.0742), by_width(0.83));
    let to_scene = |entity| world.get::<GlobalSpatialTransform>(entity).unwrap().mat4;
    let data = to_scene(marker).transform_point3(DVec3::ZERO);
    let left = to_scene(entity).transform_point3(DVec3::new(bounds.min.x, bounds.center().y, 0.0));
    assert!(
        (left - (data + DVec3::new(0.5, 0.0, 0.0))).length() < 1e-9,
        "{left:?} is not 0.5 right of {data:?}"
    );
}

#[test]
fn data_marks_clip_to_the_plot_window_unless_opted_out() {
    let mut canvas = SceneModel::new(16.0, 9.0);
    let space = canvas
        .coordinate_axes(
            Axis::linear(0.0, 1.0).unwrap(),
            Axis::linear(0.0, 1.0).unwrap(),
            Some(4.0),
            Some(4.0),
            true,
        )
        .unwrap();
    let mask = space.data_mask.clone().expect("Cartesian spaces clip data");
    let plot = canvas
        .function_plot(&space, (0.0, 1.0), Sampling::default(), |x| Some(3.0 * x))
        .unwrap();
    let free = canvas
        .function_plot(&space, (0.0, 1.0), Sampling::default(), Some)
        .unwrap()
        .no_clip();
    let clips = |target: ObjectId| {
        let state = canvas.state.lock().unwrap();
        state
            .active()
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::SetClip {
                    target: id, mask, ..
                } if *id == target => Some(*mask),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(clips(plot.id), vec![Some(mask.id)]);
    assert_eq!(clips(free.id), vec![Some(mask.id), None]);
}

#[test]
fn automatic_ticks_regenerate_for_the_view_and_keep_glyph_shape() {
    use bevy::prelude::Schedule;
    use gaanim_math::GlobalSpatialTransform;
    use gaanim_scene::prelude::World;

    let mut canvas = SceneModel::new(16.0, 9.0);
    let space = canvas
        .coordinate_axes(
            Axis::linear(0.0, 0.4).unwrap(),
            Axis::linear(0.0, 4.0).unwrap().ticks(1.0).unwrap(),
            Some(9.0),
            Some(6.0),
            true,
        )
        .unwrap();
    // Regenerated ticks must follow the space wherever it was placed.
    space.drawable().clone().move_to(0.5, -0.4);
    canvas.wait(0.2);
    let zoom = canvas
        .coordinate_view_to_animation(&space, (0.0, 0.2), (0.0, 2.0))
        .unwrap()
        .duration(1.0);
    canvas.play(vec![zoom]);

    let mut world = World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world
        .remove_resource::<gaanim_timeline::timeline::Timeline>()
        .unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );
    let mut levels_schedule = Schedule::default();
    levels_schedule.add_systems(gaanim_scene::systems::coordinate_tick_level_system);
    let mut propagation = Schedule::default();
    propagation.add_systems(gaanim_scene::transform_propagation_system);
    let mut run = |world: &mut World| {
        levels_schedule.run(world);
        propagation.run(world);
    };
    let levels = |world: &mut World| {
        let mut levels: Vec<_> = world
            .query::<(&gaanim_scene::CoordinateTickLevel, &gaanim_scene::Opacity)>()
            .iter(world)
            .map(|(level, opacity)| (level.axis, level.generation, opacity.0))
            .collect();
        levels.sort_by(|a, b| a.partial_cmp(b).unwrap());
        levels.dedup();
        levels
    };

    timeline.seek(&mut world, 0.0);
    run(&mut world);
    let glyphs: Vec<_> = world
        .query::<(
            gaanim_scene::prelude::Entity,
            &gaanim_scene::components::TextSpan,
            &GlobalSpatialTransform,
        )>()
        .iter(&world)
        .map(|(entity, _, global)| (entity, global.mat4))
        .collect();
    let start = levels(&mut world);
    // x regenerates (auto step); y keeps its fixed step and one generation.
    assert!(
        start.contains(&(0, 0, 1.0)) && start.contains(&(0, 1, 0.0)),
        "{start:?}"
    );
    assert!(
        start.iter().all(|level| level.0 == 0 || level.1 == 0),
        "{start:?}"
    );

    timeline.seek(&mut world, 1.2);
    run(&mut world);
    let end = levels(&mut world);
    assert!(
        end.contains(&(0, 0, 0.0)) && end.contains(&(0, 1, 1.0)),
        "{end:?}"
    );
    assert!(end.contains(&(1, 0, 1.0)), "{end:?}");
    let mut level_frames: Vec<_> = world
            .query_filtered::<(&gaanim_scene::CoordinateTickLevel, &GlobalSpatialTransform), bevy::prelude::Without<gaanim_scene::CoordinateViewRole>>()
            .iter(&world)
            .filter(|(level, _)| level.axis == 0)
            .map(|(level, global)| (level.generation, global.mat4))
            .collect();
    level_frames.sort_by_key(|(generation, _)| *generation);
    let (_, first) = level_frames[0];
    for (generation, frame) in &level_frames {
        assert!(
            frame.abs_diff_eq(first, 1e-9),
            "x tick generation {generation} is offset from generation 0"
        );
    }
    for (entity, initial) in &glyphs {
        let current = world.get::<GlobalSpatialTransform>(*entity).unwrap().mat4;
        for direction in [DVec3::X, DVec3::Y] {
            assert!(
                (current.transform_vector3(direction) - initial.transform_vector3(direction))
                    .length()
                    < 1e-9,
                "the zoom deformed a tick-number glyph"
            );
        }
    }
}

#[test]
fn coordinate_view_preserves_scatter_marker_radius() {
    use bevy::prelude::Schedule;
    use gaanim_math::GlobalSpatialTransform;
    use gaanim_scene::prelude::{ChildOf, World};

    let mut canvas = SceneModel::new(16.0, 9.0);
    let space = canvas
        .coordinate_axes(
            Axis::linear(0.0, 0.55).unwrap(),
            Axis::linear(0.5, 4.5).unwrap(),
            Some(5.6),
            Some(3.5),
            true,
        )
        .unwrap();
    let xs = [Some(0.05), Some(0.2), Some(0.4)];
    let ys = [Some(1.0), Some(2.5), Some(4.0)];
    canvas
        .data_scatter(&space, &xs, &ys, 0.055, NonFinitePolicy::Gap)
        .unwrap();
    canvas.play(vec![
        space
            .view_to_animation((0.0, 0.2), (0.5, 4.5))
            .unwrap()
            .duration(1.0),
    ]);

    let mut world = World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world
        .remove_resource::<gaanim_timeline::timeline::Timeline>()
        .unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );
    let mut propagation = Schedule::default();
    propagation.add_systems(gaanim_scene::transform_propagation_system);
    timeline.seek(&mut world, 0.0);
    propagation.run(&mut world);

    // Markers are the only circular paths with the authored radius.
    let markers: Vec<_> = world
        .query::<(
            gaanim_scene::prelude::Entity,
            &gaanim_scene::prelude::Path2D,
        )>()
        .iter(&world)
        .filter(|(_, path)| {
            let bounds = path.0.bounding_box();
            (bounds.width() - 0.11).abs() < 1e-6 && (bounds.height() - 0.11).abs() < 1e-6
        })
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(markers.len(), xs.len());
    let initial: Vec<_> = markers
        .iter()
        .map(|marker| world.get::<GlobalSpatialTransform>(*marker).unwrap().mat4)
        .collect();

    for time in [0.5, 1.0, 0.0] {
        timeline.seek(&mut world, time);
        propagation.run(&mut world);
        for (marker, initial) in markers.iter().zip(&initial) {
            let current = world.get::<GlobalSpatialTransform>(*marker).unwrap().mat4;
            for direction in [DVec3::X, DVec3::Y] {
                assert!(
                    (current.transform_vector3(direction) - initial.transform_vector3(direction))
                        .length()
                        < 1e-9,
                    "domain view deformed a scatter marker at t={time}"
                );
            }
            // The marker center still follows the zoomed data position.
            let parent = world.get::<ChildOf>(*marker).unwrap().parent();
            let local = world
                .get::<gaanim_math::SpatialTransform>(*marker)
                .unwrap()
                .translation;
            let expected = world
                .get::<GlobalSpatialTransform>(parent)
                .unwrap()
                .mat4
                .transform_point3(local);
            assert!((current.w_axis.truncate() - expected).length() < 1e-9);
        }
    }
}

#[test]
fn animated_view_keeps_plot_area_clips_axes_and_updates_local_mapping() {
    use bevy::prelude::Schedule;
    use gaanim_math::GlobalSpatialTransform;
    use gaanim_scene::prelude::{ChildOf, World};

    let mut canvas = SceneModel::new(16.0, 9.0);
    let space = canvas
        .coordinate_axes_with_visibility(
            Axis::linear(0.0, 0.55)
                .unwrap()
                .ticks(0.1)
                .unwrap()
                .label("x"),
            Axis::linear(0.5, 4.5).unwrap().ticks(1.0).unwrap(),
            Some(5.6),
            Some(3.3),
            CartesianVisibility {
                x_grid: false,
                y_grid: true,
                ..Default::default()
            },
        )
        .unwrap()
        .move_to(-3.6, 0.0);
    let before = space.data_to_local(0.2, 0.5).unwrap();
    let animation = space.view_to_animation((0.0, 0.2), (0.5, 4.5)).unwrap();
    // Describing the animation must not move the authoring cursor.
    assert_eq!(space.data_to_local(0.2, 0.5).unwrap(), before);
    canvas.play(vec![animation.duration(1.0)]);
    let (x, y) = space.data_to_local(0.2, 0.5).unwrap();
    assert!((x - 2.8).abs() < 1e-9 && (y + 1.65).abs() < 1e-9);
    let (x, y) = space.local_to_data(2.8, -1.65).unwrap();
    assert!((x - 0.2).abs() < 1e-9 && (y - 0.5).abs() < 1e-9);

    let mut world = World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world
        .remove_resource::<gaanim_timeline::timeline::Timeline>()
        .unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );
    let mut propagation = Schedule::default();
    propagation.add_systems(gaanim_scene::transform_propagation_system);
    use gaanim_scene::CoordinateViewRole;
    let roles: Vec<_> = world
        .query::<(gaanim_scene::prelude::Entity, &CoordinateViewRole)>()
        .iter(&world)
        .map(|(entity, role)| (entity, *role))
        .collect();
    let views: Vec<_> = roles
        .iter()
        .filter(|(_, role)| *role == CoordinateViewRole::View)
        .map(|(entity, _)| *entity)
        .collect();
    assert_eq!(views.len(), 1);
    let view = views[0];
    let in_view = |world: &World, mut entity| {
        while let Some(parent) = world.get::<ChildOf>(entity) {
            entity = parent.parent();
            if entity == view {
                return true;
            }
        }
        false
    };
    // Axis titles stay outside the rescaled view; numbers stay inside it.
    let labels: Vec<_> = roles
        .iter()
        .filter(|(_, role)| *role == CoordinateViewRole::Label)
        .map(|(entity, _)| in_view(&world, *entity))
        .collect();
    assert!(labels.contains(&true) && labels.contains(&false));

    timeline.seek(&mut world, 1.0);
    propagation.run(&mut world);
    // The zoomed window keeps the authored plot rectangle.
    let origin = world
        .get::<GlobalSpatialTransform>(view)
        .unwrap()
        .mat4
        .transform_point3(DVec3::new(-2.8, -1.65, 0.0));
    assert!((origin - DVec3::new(-6.4, -1.65, 0.0)).length() < 1e-9);

    // Clipped view paths use masks outside the view that end at the window.
    let clipped: Vec<_> = world
        .query::<(
            gaanim_scene::prelude::Entity,
            &gaanim_renderer::effects::ClipMask,
            &gaanim_scene::prelude::Path2D,
            &GlobalSpatialTransform,
        )>()
        .iter(&world)
        .map(|(entity, mask, path, global)| {
            let mut path = (*path.0).clone();
            path.apply_affine(global.affine_2d);
            (entity, mask.sources.clone(), path.bounding_box())
        })
        .collect();
    let (axes, sources, axes_world) = clipped
        .into_iter()
        .max_by(|a, b| a.2.x1.total_cmp(&b.2.x1))
        .expect("axis layers are clipped");
    assert!(in_view(&world, axes));
    assert!(axes_world.x1 > 8.0, "the zoom overflows the x axis");
    let mut mask_world = BezPath::new();
    for source in sources {
        assert!(!in_view(&world, source));
        let mut path = (*world
            .get::<gaanim_scene::prelude::Path2D>(source)
            .unwrap()
            .0)
            .clone();
        path.apply_affine(
            world
                .get::<GlobalSpatialTransform>(source)
                .unwrap()
                .affine_2d,
        );
        mask_world.extend(path);
    }
    assert!(
        mask_world.bounding_box().x1 < -0.8 + 0.1,
        "the mask ends at the plot edge"
    );
    let numbers = space.layer(SpaceLayer::Numbers).unwrap();
    assert!(
        canvas.state.lock().unwrap().active().ops.iter().any(
            |op| matches!(op, Op::SetClip { target, mask: Some(_), .. } if *target == numbers.id)
        ),
        "axis numbers are clipped"
    );
}

#[test]
fn animated_view_targets_internal_view_and_preserves_layout_root() {
    let mut canvas = SceneModel::new(640, 360);
    let space = canvas
        .coordinate_axes(
            Axis::linear(-4.0, 4.0).unwrap(),
            Axis::linear(-3.0, 3.0).unwrap(),
            Some(520.0),
            Some(280.0),
            true,
        )
        .unwrap()
        .move_to(37.0, -18.0);

    let coordinate = space.coord(1.0, 2.0).unwrap();
    let animation = space
        .view_to_animation((-2.0, 2.0), (-1.5, 1.5))
        .unwrap()
        .duration(1.2);

    assert_ne!(space.root.id, space.view.id);
    assert_eq!(coordinate.space, space.view.id);
    assert_eq!(animation.inner.target, space.view.id);
}

#[test]
fn animated_view_rejects_non_affine_scales() {
    let mut canvas = SceneModel::new(640, 360);
    let space = canvas
        .coordinate_axes(
            Axis::log(0.1, 10.0, 10.0).unwrap(),
            Axis::linear(-3.0, 3.0).unwrap(),
            Some(520.0),
            Some(280.0),
            false,
        )
        .unwrap();

    assert!(matches!(
        space.view_to_animation((0.2, 5.0), (-1.0, 1.0)),
        Err(VisualizationError::UnsupportedAnimatedView)
    ));
}

#[test]
fn parameter_drives_scalar_source_and_animation() {
    let mut canvas = SceneModel::new(640, 360);
    let parameter = canvas.parameter(1.5).unwrap();
    parameter.set(2.25).unwrap();

    assert_eq!(
        parameter
            .source()
            .evaluate(0.0, |id| (id == parameter.drawable().id)
                .then_some(parameter.current()))
            .unwrap(),
        2.25
    );

    let animation = parameter.animate().set(4.0);
    assert_eq!(animation.inner.target, parameter.drawable().id);
    assert_eq!(parameter.current(), 2.25);
    assert!(matches!(
        parameter.set(f64::NAN),
        Err(VisualizationError::InvalidParameter)
    ));
}

#[test]
fn three_dimensional_lines_honor_stroke_color() {
    let mut canvas = SceneModel::new(640, 360);
    let expected = Color::from_rgb8(0xE7, 0x4C, 0x3C);
    canvas
        .polyline_3d(vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]])
        .stroke(expected, 3.0);

    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let colors: Vec<Color> = world
        .query::<&gaanim_scene::LineListData>()
        .iter(&world)
        .map(|line| line.color)
        .collect();
    assert_eq!(colors, [expected]);
}

#[test]
fn three_dimensional_grids_and_axes_take_the_sides_the_camera_shows() {
    use bevy::prelude::App;
    use gaanim_core::glam::DVec3;
    let axis = || Axis::linear(-2.0, 2.0).unwrap().ticks(1.0).unwrap();
    let mut canvas = SceneModel::new(640, 360);
    canvas
        .coordinate_axes_3d(axis(), axis(), axis(), [4.0, 4.0, 4.0], true)
        .unwrap();
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin);
    canvas.compile(app.world_mut());
    app.finish();
    app.cleanup();

    // From each camera, the faces behind the box along x, y and z.
    for (eye, behind) in [
        (DVec3::new(9.0, -7.0, 5.0), [-2.0, 2.0, -2.0]),
        (DVec3::new(-9.0, 7.0, 5.0), [2.0, -2.0, -2.0]),
        (DVec3::new(9.0, 7.0, -5.0), [-2.0, -2.0, 2.0]),
    ] {
        let mut camera = gaanim_math::Camera::perspective_3d(640, 360, 0.8);
        camera.look_at(eye, DVec3::ZERO, DVec3::Z).unwrap();
        app.world_mut()
            .insert_resource(gaanim_math::CameraViewOverride(Some(camera)));
        app.update();

        let world = app.world_mut();
        let mut lines = world.query::<(
            &gaanim_scene::LineListData,
            &gaanim_math::GlobalSpatialTransform,
            &gaanim_scene::GlobalOpacity,
            Option<&gaanim_scene::AxisBoxPart>,
        )>();
        // Every drawn segment in world coordinates, to a micrometre.
        let mut segments = Vec::new();
        for (line, global, opacity, part) in lines.iter(world) {
            if opacity.0 <= f32::EPSILON {
                continue;
            }
            for segment in line.points.as_chunks::<2>().0 {
                let [from, to] = segment.map(|point| {
                    let point = global
                        .mat4
                        .transform_point3(DVec3::from(point.map(f64::from)));
                    (point * 1e6).round().as_i64vec3().to_array()
                });
                segments.push(if from <= to { [from, to] } else { [to, from] });
            }
            if let Some(gaanim_scene::AxisBoxPart {
                role: gaanim_scene::AxisBoxRole::Plane { normal },
                ..
            }) = part
            {
                for point in &line.points {
                    let at = global
                        .mat4
                        .transform_point3(DVec3::from(point.map(f64::from)));
                    assert!(
                        (at[*normal] - behind[*normal]).abs() < 1e-6,
                        "{eye:?}: the plane across {normal} at {at:?}"
                    );
                }
            }
        }
        let count = segments.len();
        segments.sort_unstable();
        segments.dedup();
        assert_eq!(
            segments.len(),
            count,
            "{eye:?}: two lines lie on top of each other"
        );
    }
}

#[test]
fn fixed_three_dimensional_axes_keep_the_lowest_corner() {
    let axis = || Axis::linear(-2.0, 2.0).unwrap().ticks(1.0).unwrap();
    let mut canvas = SceneModel::new(640, 360);
    canvas
        .coordinate_axes_3d_with_sides(
            axis(),
            axis(),
            axis(),
            [4.0, 4.0, 4.0],
            Cartesian3DVisibility::default(),
            AxisBoxSides::Fixed,
        )
        .unwrap();
    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    // Nothing follows the camera, and no grid line lies on an axis or
    // on another plane's line. (The y and z ticks at the lowest corner
    // still cross x there on top of each other, as they always have.)
    assert_eq!(
        world
            .query::<&gaanim_scene::AxisBoxPart>()
            .iter(&world)
            .count(),
        0
    );
    let mut segments: Vec<[[i64; 3]; 2]> = world
        .query::<&gaanim_scene::LineListData>()
        .iter(&world)
        .flat_map(|line| line.points.as_chunks::<2>().0.to_vec())
        .map(|segment| {
            let [from, to] =
                segment.map(|point| point.map(|value| (f64::from(value) * 1e6).round() as i64));
            if from <= to { [from, to] } else { [to, from] }
        })
        .filter(|[from, to]| {
            let length: i64 = from.iter().zip(to).map(|(a, b)| (a - b).abs()).sum();
            length > 1_000_000
        })
        .collect();
    let count = segments.len();
    segments.sort_unstable();
    segments.dedup();
    assert_eq!(segments.len(), count);
    // The three walls at the lowest faces: no grid line reaches past
    // them toward the camera.
    assert!(
        segments
            .iter()
            .flatten()
            .all(|point| point.iter().any(|&value| value == -2_000_000))
    );
}

#[test]
fn native_three_dimensional_spawns_select_hybrid_rendering() {
    let mut canvas = SceneModel::new(640, 360);
    assert!(!canvas.has_3d_content());

    canvas.polyline_3d(vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]);

    assert!(canvas.has_3d_content());
}

#[test]
fn batched_parametric_surfaces_match_per_point_ones() {
    let point = |u: f64, v: f64| [u.cos() * v, u.sin() * v, u * 0.1];
    let meshes = [false, true].map(|batched| {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes_3d(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                [4.0, 4.0, 4.0],
                false,
            )
            .unwrap();
        let (u_domain, v_domain, resolution) = ((0.0, 3.0), (0.5, 1.5), [4, 3]);
        let grid = (0..4)
            .flat_map(|iu| (0..3).map(move |iv| (iu as f64, 0.5 + iv as f64 * 0.5)))
            .collect::<Vec<_>>();
        let (function, color) = if batched {
            let points = grid
                .iter()
                .flat_map(|&(u, v)| point(u, v))
                .collect::<Vec<_>>();
            let values = grid.iter().map(|&(u, v)| u + v).collect::<Vec<_>>();
            (
                ReactiveFunction::new(0, 36, Vec::new(), move |_| Ok(points.clone())),
                ReactiveFunction::new(0, 12, Vec::new(), move |_| Ok(values.clone())),
            )
        } else {
            (
                ReactiveFunction::new(2, 3, Vec::new(), move |uv| Ok(point(uv[0], uv[1]).to_vec())),
                ReactiveFunction::new(2, 1, Vec::new(), |uv| Ok(vec![uv[0] + uv[1]])),
            )
        };
        canvas
            .reactive_parametric_surface_plot(
                &space,
                function,
                u_domain,
                v_domain,
                resolution,
                super::super::SurfaceColor::Function(color),
                None,
                Some((0.0, 5.0)),
                Some(gaanim_scene::Material3D::default()),
            )
            .unwrap();
        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        world
            .query::<&gaanim_scene::TriangleMeshData>()
            .single(&world)
            .unwrap()
            .clone()
    });
    assert_eq!(meshes[0].vertices, meshes[1].vertices);
    assert_eq!(meshes[0].indices, meshes[1].indices);
    assert_eq!(meshes[0].colors, meshes[1].colors);
    // Both sides of 2 x 3 cells of two triangles each, over 12 vertices.
    assert_eq!(meshes[0].vertices.len(), 24);
    assert_eq!(meshes[0].indices.len(), 2 * 2 * 3 * 2 * 3);
}

#[test]
fn batched_surfaces_match_per_point_ones() {
    let height = |x: f64, y: f64| (x * x - y * y) * 0.25;
    let meshes = [false, true].map(|batched| {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes_3d(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-1.0, 1.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                [4.0, 2.0, 4.0],
                false,
            )
            .unwrap();
        let resolution = [5, 3];
        let function = if batched {
            // Every height at once, in the grid's order; one is missing.
            let mut heights = space
                .surface_grid(resolution)
                .iter()
                .map(|&[x, y]| height(x, y))
                .collect::<Vec<_>>();
            heights[0] = f64::MAX;
            ReactiveFunction::new(0, 15, Vec::new(), move |_| Ok(heights.clone()))
        } else {
            ReactiveFunction::new(2, 1, Vec::new(), move |xy| {
                let missing = xy[0] == -2.0 && xy[1] == -1.0;
                Ok(vec![if missing {
                    f64::NAN
                } else {
                    height(xy[0], xy[1])
                }])
            })
        };
        canvas
            .reactive_surface_plot(&space, function, resolution, None, None)
            .unwrap();
        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        world
            .query::<&gaanim_scene::TriangleMeshData>()
            .iter(&world)
            .find(|mesh| mesh.vertices.len() == 15)
            .expect("the surface")
            .clone()
    });
    let finite = |mesh: &gaanim_scene::TriangleMeshData| {
        mesh.vertices
            .iter()
            .map(|v| v.map(|value| if value.is_finite() { value } else { 0.0 }))
            .collect::<Vec<_>>()
    };
    assert_eq!(finite(&meshes[0]), finite(&meshes[1]));
    assert_eq!(meshes[0].indices, meshes[1].indices);
    // 4 x 2 cells of two triangles, less the corner one with the hole.
    assert_eq!(meshes[0].indices.len(), 7 * 2 * 3);
}

#[test]
fn lit_surfaces_draw_a_second_side_wound_the_other_way() {
    let meshes = [None, Some(gaanim_scene::Material3D::default())].map(|material| {
        let mut canvas = SceneModel::new(640, 360);
        let space = canvas
            .coordinate_axes_3d(
                Axis::linear(-2.0, 2.0).unwrap(),
                Axis::linear(-1.0, 1.0).unwrap(),
                Axis::linear(-2.0, 2.0).unwrap(),
                [4.0, 2.0, 4.0],
                false,
            )
            .unwrap();
        let function = ReactiveFunction::new(2, 1, Vec::new(), |xy| Ok(vec![xy[0] * xy[1] * 0.25]));
        canvas
            .reactive_surface_plot(&space, function, [5, 3], None, material)
            .unwrap();
        let mut world = bevy::prelude::World::new();
        world.insert_resource(gaanim_timeline::timeline::Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        world
            .query::<&gaanim_scene::TriangleMeshData>()
            .single(&world)
            .unwrap()
            .clone()
    });
    let [unlit, lit] = meshes;
    assert_eq!(unlit.material, None);
    assert_eq!(lit.material, Some(gaanim_scene::Material3D::default()));
    // The front side is the unlit sheet; the back copies its vertices
    // and colors, and runs each triangle the other way.
    let count = unlit.vertices.len();
    assert_eq!(lit.vertices[..count], unlit.vertices[..]);
    assert_eq!(lit.vertices[count..], unlit.vertices[..]);
    let colors = unlit.colors.as_ref().unwrap();
    assert_eq!(lit.colors.as_ref().unwrap()[count..], colors[..]);
    let front = unlit.indices.len();
    assert_eq!(lit.indices.len(), 2 * front);
    assert_eq!(lit.indices[..front], unlit.indices[..]);
    let offset = count as u32;
    for (back, front) in lit.indices[front..]
        .as_chunks::<3>()
        .0
        .iter()
        .zip(unlit.indices.as_chunks::<3>().0)
    {
        assert_eq!(
            *back,
            [front[0] + offset, front[2] + offset, front[1] + offset]
        );
    }
}

#[test]
fn surfaces_include_a_batched_colored_wireframe_fallback() {
    let mut canvas = SceneModel::new(640, 360);
    let space = canvas
        .coordinate_axes_3d(
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-2.0, 2.0).unwrap(),
            [4.0, 4.0, 4.0],
            false,
        )
        .unwrap();
    canvas
        .surface_plot(&space, [4, 4], |x, y| Some(x * y * 0.25))
        .unwrap();

    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    assert_eq!(
        world
            .query::<&gaanim_scene::TriangleMeshData>()
            .iter(&world)
            .count(),
        1
    );
    assert!(
        world
            .query::<&gaanim_scene::LineListData>()
            .iter(&world)
            .any(|line| line.colors.as_ref().is_some_and(|colors| {
                !line.strip && !line.points.is_empty() && colors.len() == line.points.len()
            }))
    );
}

#[test]
fn declarative_scatter_keeps_one_reactive_batch_for_ten_thousand_rows() {
    let mut canvas = SceneModel::new(1920, 1080);
    let table = gaanim_visualization::DataTable::numeric([
        (
            "x".to_owned(),
            (0..10_000).map(|index| index as f64).collect(),
        ),
        (
            "y".to_owned(),
            (0..10_000)
                .map(|index| (index as f64 * 0.01).sin())
                .collect(),
        ),
    ])
    .unwrap();
    let spec = ChartSpec::new(table, None)
        .unwrap()
        .mark(MarkKind::Point, BTreeMap::new())
        .encode(Channel::X, Encoding::field("x"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("y"))
        .unwrap();

    let chart = canvas.chart(spec).unwrap();
    assert_eq!(chart.layer("marks").unwrap().id, chart.marks.id);
    assert!(!canvas.has_3d_content());

    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    assert_eq!(
        world
            .query::<&gaanim_animation::AlwaysRedrawRegen>()
            .iter(&world)
            .count(),
        1,
        "row count must not change the number of reactive mark entities",
    );
}

#[test]
fn three_dimensional_axes_accept_nonlinear_scales_and_expose_layers() {
    let mut canvas = SceneModel::new(1280, 720);
    let space = canvas
        .coordinate_axes_3d(
            Axis::log(0.1, 1_000.0, 10.0).unwrap(),
            Axis::symlog(-100.0, 100.0, 10.0, 1.0).unwrap(),
            Axis::power(0.0, 16.0, 0.5).unwrap(),
            [10.0, 8.0, 6.0],
            true,
        )
        .unwrap();

    for layer in [
        SpaceLayer::MajorGrid,
        SpaceLayer::Axes,
        SpaceLayer::Ticks,
        SpaceLayer::Numbers,
        SpaceLayer::Labels,
    ] {
        assert!(space.layer(layer).is_some());
    }
    let point = [10.0, -12.5, 9.0];
    let local = space.data_to_local(point).unwrap();
    let restored = space.local_to_data(local).unwrap();
    for (actual, expected) in restored.into_iter().zip(point) {
        assert!((actual - expected).abs() < 1e-9);
    }
}

#[test]
fn three_dimensional_numbers_and_labels_are_drawn_at_text_size() {
    let mut canvas = SceneModel::new(1920, 1080);
    canvas.set_theme("gaanim").unwrap();
    let axis = |label: &str| {
        Axis::linear(-2.0, 2.0)
            .unwrap()
            .ticks(1.0)
            .unwrap()
            .label(label)
    };
    canvas
        .coordinate_axes_3d(axis("X"), axis("Y"), axis("Z"), [4.0, 4.0, 3.0], false)
        .unwrap();
    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    // Each billboard is a tick number or an axis title, at the size of
    // the theme's text rather than below a pixel.
    use bevy::prelude::With;
    use gaanim_math::SpatialTransform;
    use gaanim_scene::{Billboard, LocalBounds};
    let heights: Vec<f64> = world
        .query_filtered::<(&SpatialTransform, &LocalBounds), With<Billboard>>()
        .iter(&world)
        .map(|(transform, bounds)| (bounds.0.max.y - bounds.0.min.y) * transform.scale.y)
        .collect();
    // Five numbers along each axis, which follow the camera to edges
    // that share no corner by three, and three titles.
    assert_eq!(heights.len(), 18);
    assert!(
        heights.iter().all(|height| (0.15..=0.6).contains(height)),
        "{heights:?}"
    );
}

#[test]
fn declarative_marks_receive_a_visible_default_series_style() {
    let table = gaanim_visualization::DataTable::numeric([
        ("x".to_owned(), vec![0.0, 1.0, 2.0]),
        ("y".to_owned(), vec![1.0, 2.0, 1.5]),
    ])
    .unwrap();
    let base = ChartSpec::new(table, None)
        .unwrap()
        .encode(Channel::X, Encoding::field("x"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("y"))
        .unwrap();

    let mut point_canvas = SceneModel::new(640, 360);
    let points = point_canvas.chart(base.clone()).unwrap();
    assert!(
        points
            .marks
            .spec
            .lock()
            .expect("point mark spec poisoned")
            .fill
            .is_some(),
        "point marks without a color encoding must still be visible",
    );

    let mut line_canvas = SceneModel::new(640, 360);
    let line = line_canvas
        .chart(base.mark(MarkKind::Line, BTreeMap::new()))
        .unwrap();
    assert!(
        line.marks
            .spec
            .lock()
            .expect("line mark spec poisoned")
            .stroke
            .is_some(),
        "line marks without a color encoding must still be visible",
    );
}

#[test]
fn declarative_axes_render_above_bar_marks() {
    let table = gaanim_visualization::DataTable::numeric([
        ("x".to_owned(), vec![0.0, 1.0, 2.0]),
        ("y".to_owned(), vec![3.0, 2.0, 1.0]),
    ])
    .unwrap();
    let spec = ChartSpec::new(table, None)
        .unwrap()
        .mark(MarkKind::Bar, BTreeMap::new())
        .encode(Channel::X, Encoding::field("x"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("y"))
        .unwrap();

    let mut canvas = SceneModel::new(640, 360);
    let chart = canvas.chart(spec).unwrap();
    let mark_z = chart
        .marks
        .spec
        .lock()
        .expect("bar mark spec poisoned")
        .z_index;
    // Group z_index is inherited by the renderer, so the structural layers
    // inside the view carry the lift while the view stays at scene level.
    let axes_spec = chart.axes.spec.lock().expect("axes spec poisoned").clone();
    assert_eq!(
        axes_spec.z_index, 0,
        "the chart must not rise above unrelated scene content",
    );
    let SpawnKind::GroupNoCenter(children) = &axes_spec.kind else {
        panic!("chart axes layer must be a structural group");
    };
    let state = canvas.state.lock().expect("canvas state poisoned");
    let layer_z = children
        .iter()
        .filter_map(|id| state.object_specs.get(id))
        .map(|spec| spec.lock().expect("layer spec poisoned").z_index)
        .max()
        .expect("axes view has layers");
    assert!(
        layer_z > mark_z,
        "semantic axes must remain readable when a bar intersects them",
    );
}

#[test]
fn chart_axes_layer_targets_the_complete_structural_view() {
    let table = gaanim_visualization::DataTable::numeric([
        ("x".to_owned(), vec![0.0, 1.0, 2.0]),
        ("y".to_owned(), vec![3.0, 2.0, 1.0]),
    ])
    .unwrap();
    let spec = ChartSpec::new(table, None)
        .unwrap()
        .mark(MarkKind::Bar, BTreeMap::new())
        .encode(Channel::X, Encoding::field("x"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("y"))
        .unwrap();

    let mut canvas = SceneModel::new(640, 360);
    let chart = canvas.chart(spec).unwrap();
    let axes = chart.layer("axes").expect("chart axes layer");
    assert!(
        matches!(
            axes.spec.lock().expect("axes spec poisoned").kind,
            SpawnKind::GroupNoCenter(_)
        ),
        "the chart axes layer must include grid, axes, ticks, numbers, and labels",
    );
}

use gaanim_core::peniko::Brush;

fn grouped_chart_fills(
    canvas: &SceneModel,
    chart: &ChartHandle,
) -> Vec<(Option<Brush>, Option<Brush>, usize)> {
    let SpawnKind::GroupNoCenter(children) = chart.marks.spec.lock().unwrap().kind.clone() else {
        panic!("per-row colours must produce a group of marks");
    };
    let state = canvas.state.lock().unwrap();
    children
        .iter()
        .map(|id| {
            let spec = state.object_specs[id].lock().unwrap();
            let SpawnKind::DataMark { source, .. } = &spec.kind else {
                panic!("every group child is a data mark");
            };
            (
                spec.fill.clone(),
                spec.stroke.as_ref().map(|(brush, _)| brush.clone()),
                source.snapshot().len(),
            )
        })
        .collect()
}

fn grouped_table() -> gaanim_visualization::DataTable {
    gaanim_visualization::DataTable::new([
        (
            "x".to_owned(),
            gaanim_visualization::Column::Numeric(vec![Some(0.0), Some(1.0), Some(2.0), Some(3.0)]),
        ),
        (
            "y".to_owned(),
            gaanim_visualization::Column::Numeric(vec![Some(1.0), Some(3.0), Some(2.0), Some(4.0)]),
        ),
        (
            "group".to_owned(),
            gaanim_visualization::Column::Text(vec![
                Some("A".into()),
                Some("B".into()),
                Some("A".into()),
                Some("C".into()),
            ]),
        ),
        (
            "weight".to_owned(),
            gaanim_visualization::Column::Numeric(vec![Some(0.2), Some(0.2), Some(1.0), Some(1.0)]),
        ),
    ])
    .unwrap()
}

fn grouped_colors() -> [Color; 3] {
    [
        Color::from_rgb8(0xE1, 0x1D, 0x48),
        Color::from_rgb8(0x25, 0x63, 0xEB),
        Color::from_rgb8(0x16, 0xA3, 0x4A),
    ]
}

fn grouped_spec(kind: MarkKind) -> ChartSpec {
    ChartSpec::new(grouped_table(), None)
        .unwrap()
        .mark(kind, BTreeMap::new())
        .encode(Channel::X, Encoding::field("x"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("y"))
        .unwrap()
        .encode(
            Channel::Color,
            Encoding::scaled_field(
                "group",
                gaanim_visualization::ScaleSpec::category(["A".into(), "B".into(), "C".into()])
                    .unwrap()
                    .colors(grouped_colors()),
            ),
        )
        .unwrap()
}

#[test]
fn two_dimensional_points_take_each_row_colour_like_three_dimensional_ones() {
    let mut canvas = SceneModel::new(640, 360);
    let chart = canvas.chart(grouped_spec(MarkKind::Point)).unwrap();
    let fills = grouped_chart_fills(&canvas, &chart);
    let solid = |color: Color| Some(Brush::Solid(color));
    let [a, b, c] = grouped_colors();
    assert_eq!(
        fills,
        vec![
            (solid(a), None, 2),
            (solid(b), None, 1),
            (solid(c), None, 1)
        ],
        "one batch per resolved colour, in first-row order"
    );
    let batch = grouped_spec(MarkKind::Point).batch().unwrap();
    let colors: Vec<_> = batch.data.iter().map(|datum| datum.color).collect();
    assert_eq!(colors, vec![Some(a), Some(b), Some(a), Some(c)]);
}

#[test]
fn point_opacity_fields_become_per_row_alpha() {
    let spec = grouped_spec(MarkKind::Point)
        .encode(Channel::Opacity, Encoding::field("weight"))
        .unwrap();
    let mut canvas = SceneModel::new(640, 360);
    let chart = canvas.chart(spec).unwrap();
    let alphas: Vec<(u8, usize)> = grouped_chart_fills(&canvas, &chart)
        .into_iter()
        .map(|(fill, _, rows)| match fill {
            Some(Brush::Solid(color)) => (color.to_rgba8().a, rows),
            other => panic!("unexpected fill {other:?}"),
        })
        .collect();
    // A appears at 0.2 and 1.0 opacity, B at 0.2, C at 1.0.
    assert_eq!(alphas, vec![(51, 1), (51, 1), (255, 1), (255, 1)]);
}

#[test]
fn categorical_lines_draw_one_series_per_category() {
    for kind in [MarkKind::Line, MarkKind::Step, MarkKind::Area] {
        let mut canvas = SceneModel::new(640, 360);
        let chart = canvas.chart(grouped_spec(kind)).unwrap();
        let groups = grouped_chart_fills(&canvas, &chart);
        let rows: Vec<usize> = groups.iter().map(|(_, _, rows)| *rows).collect();
        assert_eq!(rows, vec![2, 1, 1], "{kind:?}: series A has two rows");
        let [a, ..] = grouped_colors();
        let painted = if kind == MarkKind::Area {
            &groups[0].0
        } else {
            &groups[0].1
        };
        assert_eq!(painted, &Some(Brush::Solid(a)), "{kind:?}");
    }
}

#[test]
fn uniform_colour_charts_keep_a_single_mark() {
    let spec = ChartSpec::new(grouped_table(), None)
        .unwrap()
        .mark(MarkKind::Point, BTreeMap::new())
        .encode(Channel::X, Encoding::field("x"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("y"))
        .unwrap();
    let mut canvas = SceneModel::new(640, 360);
    let chart = canvas.chart(spec).unwrap();
    assert!(matches!(
        chart.marks.spec.lock().unwrap().kind,
        SpawnKind::DataMark { .. }
    ));
}

#[test]
fn colour_legends_list_each_category_with_its_colour() {
    let spec = grouped_spec(MarkKind::Point).guide(
        Channel::Color,
        gaanim_visualization::GuideSpec::Legend {
            title: Some("Grupo".into()),
        },
    );
    let mut canvas = SceneModel::new(640, 360);
    let chart = canvas.chart(spec).unwrap();
    let guides = chart.guides.expect("legend guide");
    let SpawnKind::GroupNoCenter(items) = guides.spec.lock().unwrap().kind.clone() else {
        panic!("legend is a group");
    };
    // Title, then a swatch and a name per category.
    assert_eq!(items.len(), 1 + 2 * 3);
    let state = canvas.state.lock().unwrap();
    let swatches: Vec<_> = items[1..]
        .iter()
        .step_by(2)
        .map(|id| state.object_specs[id].lock().unwrap().fill.clone())
        .collect();
    let [a, b, c] = grouped_colors();
    assert_eq!(
        swatches,
        vec![
            Some(Brush::Solid(a)),
            Some(Brush::Solid(b)),
            Some(Brush::Solid(c))
        ]
    );
}

#[test]
fn categorical_bars_batch_by_resolved_color_and_expose_labels() {
    let table = gaanim_visualization::DataTable::new([
        (
            "method".to_owned(),
            gaanim_visualization::Column::Text(vec![
                Some("baseline".into()),
                Some("cached".into()),
                Some("gpu".into()),
            ]),
        ),
        (
            "elapsed".to_owned(),
            gaanim_visualization::Column::Numeric(vec![Some(48.0), Some(-12.0), Some(21.0)]),
        ),
        (
            "kind".to_owned(),
            gaanim_visualization::Column::Text(vec![
                Some("slow".into()),
                Some("slow".into()),
                Some("fast".into()),
            ]),
        ),
    ])
    .unwrap();
    let spec = ChartSpec::new(table, None)
        .unwrap()
        .mark(
            MarkKind::Bar,
            BTreeMap::from([
                (
                    "label_position".into(),
                    ConstantValue::Text("inside".into()),
                ),
                ("label_offset".into(), ConstantValue::Number(12.0)),
            ]),
        )
        .encode(Channel::X, Encoding::field("method"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("elapsed"))
        .unwrap()
        .encode(Channel::Label, Encoding::field("elapsed"))
        .unwrap()
        .encode(
            Channel::Color,
            Encoding::scaled_field(
                "kind",
                gaanim_visualization::ScaleSpec::category(["slow".into(), "fast".into()])
                    .unwrap()
                    .colors([
                        Color::from_rgb8(0x33, 0x66, 0xCC),
                        Color::from_rgb8(0xDD, 0x55, 0x55),
                    ]),
            ),
        )
        .unwrap();

    let mut canvas = SceneModel::new(640, 360);
    let chart = canvas.chart(spec).unwrap();
    assert!(chart.layer("labels").is_some());
    let mark_spec = chart.marks.spec.lock().expect("bar mark spec poisoned");
    let SpawnKind::GroupNoCenter(batches) = &mark_spec.kind else {
        panic!("coloured bars must be grouped vector batches");
    };
    assert_eq!(batches.len(), 2, "equal resolved colours share one batch");
}

#[test]
fn bar_labels_follow_signed_extremes_inside_and_outside() {
    let table = gaanim_visualization::DataTable::new([
        (
            "method".to_owned(),
            gaanim_visualization::Column::Text(vec![Some("up".into()), Some("down".into())]),
        ),
        (
            "value".to_owned(),
            gaanim_visualization::Column::Numeric(vec![Some(40.0), Some(-10.0)]),
        ),
    ])
    .unwrap();
    let base = ChartSpec::new(table, None)
        .unwrap()
        .encode(Channel::X, Encoding::field("method"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("value"))
        .unwrap()
        .encode(Channel::Label, Encoding::field("value"))
        .unwrap()
        .axis(Channel::Y, Axis::linear(-20.0, 50.0).unwrap())
        .unwrap();

    let mut outside_canvas = SceneModel::new(640, 360);
    let outside = outside_canvas
        .chart(base.clone().mark(
            MarkKind::Bar,
            BTreeMap::from([("label_offset".into(), ConstantValue::Number(12.0))]),
        ))
        .unwrap();
    let outside_labels = group_child_translations(
        &outside_canvas,
        outside.layer("labels").expect("labels should materialize"),
    );

    let mut inside_canvas = SceneModel::new(640, 360);
    let inside = inside_canvas
        .chart(base.mark(
            MarkKind::Bar,
            BTreeMap::from([
                (
                    "label_position".into(),
                    ConstantValue::Text("inside".into()),
                ),
                ("label_offset".into(), ConstantValue::Number(12.0)),
            ]),
        ))
        .unwrap();
    let inside_labels = group_child_translations(
        &inside_canvas,
        inside.layer("labels").expect("labels should materialize"),
    );

    assert!(
        outside_labels[0].y > inside_labels[0].y,
        "positive labels invert inside"
    );
    assert!(
        outside_labels[1].y < inside_labels[1].y,
        "negative labels invert inside"
    );
    assert_eq!(
        outside_labels[0].y - inside_labels[0].y,
        24.0,
        "the positive label moves by twice its offset",
    );
    assert_eq!(
        inside_labels[1].y - outside_labels[1].y,
        24.0,
        "the negative label moves by twice its offset",
    );
}

use std::f64::consts::{FRAC_PI_2, TAU};

/// The arc sweeps an arc chart's compiled timeline holds.
fn arc_sweeps(canvas: &SceneModel) -> Vec<(gaanim_math::ArcSlice, gaanim_math::ArcSlice)> {
    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    world
        .resource::<gaanim_timeline::timeline::Timeline>()
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            gaanim_timeline::clip::ClipPayload::Animation(animation) => match animation.lens {
                gaanim_timeline::clip::PropertyLensSpec::ArcSweep { from, to } => Some((from, to)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn pie(ids: &[&str], shares: &[f64]) -> ChartSpec {
    let table = gaanim_visualization::DataTable::new([
        (
            "id".to_owned(),
            gaanim_visualization::Column::Text(
                ids.iter().map(|id| Some((*id).to_owned())).collect(),
            ),
        ),
        (
            "share".to_owned(),
            gaanim_visualization::Column::Numeric(
                shares.iter().map(|share| Some(*share)).collect(),
            ),
        ),
    ])
    .unwrap();
    ChartSpec::new(table, Some("id".to_owned()))
        .unwrap()
        .mark(MarkKind::Arc, BTreeMap::new())
        .encode(Channel::Theta, Encoding::field("share"))
        .unwrap()
}

#[test]
fn an_arc_chart_grows_every_slice_from_its_start_angle() {
    let mut canvas = SceneModel::new(640, 360);
    let chart = canvas
        .chart(pie(&["a", "b", "c"], &[1.0, 1.0, 2.0]))
        .unwrap();
    canvas
        .play_composition_configured(chart.grow().unwrap(), Some(1.0), None)
        .unwrap();
    let sweeps = arc_sweeps(&canvas);
    assert_eq!(sweeps.len(), 3);
    // Each slice opens in its place, from its edge nearest 12 o'clock.
    for (from, to) in &sweeps {
        assert_eq!((from.start, from.end), (to.end, to.end));
        assert!(to.end > to.start);
    }
    assert!(sweeps.iter().any(|(_, to)| to.end == FRAC_PI_2));
    let mut line = SceneModel::new(640, 360);
    let table = gaanim_visualization::DataTable::numeric([
        ("x".to_owned(), vec![0.0, 1.0]),
        ("y".to_owned(), vec![0.0, 1.0]),
    ])
    .unwrap();
    let spec = ChartSpec::new(table, None)
        .unwrap()
        .mark(MarkKind::Line, BTreeMap::new())
        .encode(Channel::X, Encoding::field("x"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("y"))
        .unwrap();
    assert_eq!(
        line.chart(spec).unwrap().grow().err(),
        Some(VisualizationError::NotAnArcChart)
    );
}

#[test]
fn arc_slices_without_a_color_encoding_take_the_theme_palette() {
    let mut canvas = SceneModel::new(640, 360);
    canvas.set_theme("gaanim").unwrap();
    let okabe = gaanim_core::Palette::named("okabe_ito").unwrap();
    canvas.theme_style.as_mut().unwrap().categorical = okabe.clone();
    let chart = canvas
        .chart(pie(&["a", "b", "c"], &[1.0, 1.0, 2.0]))
        .unwrap();
    for (index, (slice, _, _)) in chart.arcs.iter().enumerate() {
        let fill = slice.spec.lock().unwrap().fill.clone();
        assert_eq!(fill, Some(Brush::Solid(okabe.color(index))));
    }
}

#[test]
fn arc_slices_transition_to_the_slice_with_their_key() {
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas
        .chart(pie(&["a", "b", "c"], &[1.0, 1.0, 2.0]))
        .unwrap();
    let target = canvas
        .chart(pie(&["b", "c", "d"], &[3.0, 1.0, 1.0]))
        .unwrap();
    canvas.play(vec![
        source
            .transition_to(&target, MatchPolicy::Key, TransitionFallback::Error)
            .unwrap(),
    ]);
    let sweep = |slice: &gaanim_math::ArcSlice| slice.end - slice.start;
    let sweeps = arc_sweeps(&canvas);
    // "b" goes from a quarter of the circle to three fifths of it.
    assert!(sweeps.iter().any(|(from, to)| {
        (sweep(from) - TAU / 4.0).abs() < 1e-9 && (sweep(to) - TAU * 0.6).abs() < 1e-9
    }));
    // "c" goes from half the circle to a fifth of it.
    assert!(sweeps.iter().any(|(from, to)| {
        (sweep(from) - TAU / 2.0).abs() < 1e-9 && (sweep(to) - TAU / 5.0).abs() < 1e-9
    }));
    // "a" closes and "d" opens, each on its middle angle.
    assert!(sweeps.iter().any(|(from, to)| {
        (sweep(from) - TAU / 4.0).abs() < 1e-9
            && sweep(to) == 0.0
            && (to.start - from.mid_angle()).abs() < 1e-9
    }));
    assert!(sweeps.iter().any(|(from, to)| {
        sweep(from) == 0.0
            && (sweep(to) - TAU / 5.0).abs() < 1e-9
            && (from.start - to.mid_angle()).abs() < 1e-9
    }));
}

#[test]
fn cross_renderer_chart_transition_uses_hierarchy_aware_handoff() {
    let table = gaanim_visualization::DataTable::numeric([
        ("x".to_owned(), vec![0.0, 1.0, 0.0, 1.0]),
        ("y".to_owned(), vec![0.0, 0.0, 1.0, 1.0]),
        ("value".to_owned(), vec![0.0, 1.0, 1.0, 0.0]),
    ])
    .unwrap();
    let heatmap = ChartSpec::new(table.clone(), None)
        .unwrap()
        .mark(MarkKind::Heatmap, BTreeMap::new())
        .encode(Channel::X, Encoding::field("x"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("y"))
        .unwrap()
        .encode(Channel::Color, Encoding::field("value"))
        .unwrap();
    let surface = ChartSpec::new(table, None)
        .unwrap()
        .mark(MarkKind::Surface, BTreeMap::new())
        .encode(Channel::X, Encoding::field("x"))
        .unwrap()
        .encode(Channel::Y, Encoding::field("y"))
        .unwrap()
        .encode(Channel::Z, Encoding::field("value"))
        .unwrap();

    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.chart(heatmap).unwrap();
    let target = canvas.chart(surface).unwrap();
    let animation = source
        .transition_to(&target, MatchPolicy::Index, TransitionFallback::Error)
        .unwrap();
    assert!(matches!(
        animation.inner.anim_type,
        crate::anim::AnimationType::FadeTransform { .. }
    ));
}

#[test]
fn same_dimension_chart_transition_stays_visible_during_the_clip() {
    use gaanim_scene::prelude::{ChildOf, Entity, World};
    let spec = |values: Vec<f64>| {
        let table = gaanim_visualization::DataTable::numeric([
            ("x".to_owned(), vec![0.0, 1.0, 2.0]),
            ("value".to_owned(), values),
        ])
        .unwrap();
        ChartSpec::new(table, None)
            .unwrap()
            .mark(MarkKind::Bar, BTreeMap::new())
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("value"))
            .unwrap()
    };
    let mut canvas = SceneModel::new(640, 360);
    let source = canvas.chart(spec(vec![18.0, 42.0, 31.0])).unwrap();
    let identical = canvas.chart(spec(vec![18.0, 42.0, 31.0])).unwrap();
    canvas.wait(0.5);
    canvas.play(vec![
        source
            .transition_to(&identical, MatchPolicy::Index, TransitionFallback::Error)
            .unwrap()
            .duration(1.0),
    ]);
    canvas.wait(0.5);

    let mut world = World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world
        .remove_resource::<gaanim_timeline::timeline::Timeline>()
        .unwrap();
    timeline.add_keyframe(
        0.0,
        gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
    );
    let visible_paths = |world: &mut World| {
        let paths: Vec<Entity> = world
            .query::<(Entity, &gaanim_scene::Path2D)>()
            .iter(world)
            .filter(|(_, path)| !path.0.elements().is_empty())
            .map(|(entity, _)| entity)
            .collect();
        paths
            .into_iter()
            .filter(|&entity| {
                let mut opacity = 1.0;
                let mut current = Some(entity);
                while let Some(node) = current {
                    opacity *= world
                        .get::<gaanim_scene::Opacity>(node)
                        .map_or(1.0, |value| value.0);
                    current = world.get::<ChildOf>(node).map(ChildOf::parent);
                }
                opacity > 0.99
            })
            .count()
    };

    timeline.seek(&mut world, 0.25);
    let before = visible_paths(&mut world);
    assert!(before > 3, "the source chart draws its marks and axes");
    for time in [0.75, 1.0, 1.25, 1.75] {
        timeline.seek(&mut world, time);
        assert_eq!(visible_paths(&mut world), before, "at {time}");
    }
}

#[test]
fn coordinate_space_write_reveals_semantic_layers_in_parallel() {
    let mut canvas = SceneModel::new(640, 360);
    let space = canvas
        .coordinate_axes(
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-1.0, 1.0).unwrap(),
            Some(400.0),
            Some(200.0),
            true,
        )
        .unwrap();

    let animation = space
        .drawable()
        .animate()
        .write()
        .duration(1.5)
        .lag_ratio(0.0);
    let crate::anim::AnimationType::Write { config } = animation.inner.anim_type else {
        panic!("coordinate-space write should remain a Write animation");
    };
    assert_eq!(config.lag_ratio, Some(0.0));
    assert_eq!(animation.inner.duration, 1.5);
}

#[test]
fn composed_vector_field_materializes_arrows_streams_and_particles() {
    let mut canvas = SceneModel::new(640, 360);
    let space = canvas
        .coordinate_axes(
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-2.0, 2.0).unwrap(),
            Some(400.0),
            Some(300.0),
            false,
        )
        .unwrap();
    let field = canvas.vector_field_2d(&space, |[x, y]| Some([-y, x]));
    let arrows = canvas
        .arrow_vector_field_2d(&field, [5, 5], ArrowFieldOptions::default())
        .unwrap();
    let style = StreamLinesStyle {
        integration: StreamlineOptions {
            max_time: 0.3,
            ..Default::default()
        },
        ..Default::default()
    };
    let streams = canvas.stream_lines_2d(&field, [4, 4], style).unwrap();
    let particles = canvas
        .flow_particles_2d(
            &field,
            4,
            FlowParticleOptions {
                duration: 0.5,
                integration: StreamlineOptions {
                    direction: gaanim_visualization::StreamDirection::Forward,
                    max_time: 0.25,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();

    assert_ne!(arrows.drawable().id, streams.drawable().id);
    let stream_flow = streams.flow(0.5, 0.1);
    assert!(!stream_flow.is_empty());
    assert_eq!(stream_flow.len(), streams.flow_lines.len() * 2);
    assert!(stream_flow.iter().all(|animation| {
        streams
            .flow_lines
            .iter()
            .any(|line| line.id == animation.inner.target)
            && streams
                .lines
                .iter()
                .all(|line| line.id != animation.inner.target)
    }));
    assert!(streams.flow_lines.iter().all(|line| {
        let spec = line.spec.lock().unwrap();
        spec.opacity == 0.0 && spec.opacity_overridden
    }));
    assert_eq!(
        stream_flow
            .iter()
            .filter(|animation| matches!(
                animation.inner.anim_type,
                crate::anim::AnimationType::ShowPassingFlash { .. }
            ))
            .count(),
        streams.flow_lines.len()
    );
    assert!(matches!(
        streams
            .drawable()
            .animate()
            .write()
            .duration(0.5)
            .inner
            .anim_type,
        crate::anim::AnimationType::Write { .. }
    ));
    assert!(matches!(
        particles
            .drawable()
            .animate()
            .create()
            .duration(0.5)
            .inner
            .anim_type,
        crate::anim::AnimationType::Create { .. }
    ));
    assert!(matches!(
        particles
            .drawable()
            .animate()
            .fade_out()
            .duration(0.5)
            .inner
            .anim_type,
        crate::anim::AnimationType::FadeOut
    ));
    assert_eq!(particles.flow().len(), 4);
    assert!(particles.flow().iter().all(|animation| matches!(
        animation.inner.anim_type,
        crate::anim::AnimationType::MoveAlongPath { .. }
    )));
    let particle_ids = match &particles.drawable.spec.lock().unwrap().kind {
        SpawnKind::GroupNoCenter(children) => children.clone(),
        other => panic!("expected particle group, got {other:?}"),
    };
    let state = canvas.state.lock().unwrap();
    assert!(particle_ids.iter().all(|id| state.active().ops.iter().any(
            |op| matches!(op, Op::Spawn(spec) if spec.lock().unwrap().id == *id && spec.lock().unwrap().defer_visibility_until_play)
        )));
}

#[test]
fn vector_field_explicit_colors_win_over_theme_plot_rules() {
    fn authored_and_resolved_stroke(
        handle: &DrawableHandle,
        theme: &crate::canvas::CanvasTheme,
    ) -> (gaanim_core::peniko::Brush, gaanim_core::peniko::Brush) {
        let authored = handle.spec.lock().expect("field spec poisoned").clone();
        let authored_stroke = match &authored.kind {
            SpawnKind::SvgPath(path) => path
                .stroke
                .brush
                .clone()
                .expect("field paths have a stroke"),
            other => panic!("expected an SVG field path, got {other:?}"),
        };
        let resolved = theme.resolve_object(&authored).unwrap();
        let resolved_stroke = resolved
            .stroke
            .expect("the resolved field path should retain a stroke")
            .0;
        (authored_stroke, resolved_stroke)
    }

    let mut canvas = SceneModel::new(640, 360);
    canvas.set_theme("technical").unwrap();
    let theme = canvas.theme_style.clone().unwrap();
    let space = canvas
        .coordinate_axes(
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-2.0, 2.0).unwrap(),
            Some(400.0),
            Some(300.0),
            false,
        )
        .unwrap();
    let field = canvas.vector_field_2d(&space, |[x, y]| Some([-y, x]));
    let arrows = canvas
        .arrow_vector_field_2d(&field, [3, 3], ArrowFieldOptions::default())
        .unwrap();
    let streams = canvas
        .stream_lines_2d(
            &field,
            [3, 3],
            StreamLinesStyle {
                integration: StreamlineOptions {
                    max_time: 0.2,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();

    let arrow_id = match &arrows.drawable.spec.lock().unwrap().kind {
        SpawnKind::GroupNoCenter(children) => children[0],
        other => panic!("expected arrow group, got {other:?}"),
    };
    let arrow_spec = canvas
        .state
        .lock()
        .unwrap()
        .active()
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Spawn(spec) if spec.lock().unwrap().id == arrow_id => Some(spec.clone()),
            _ => None,
        })
        .expect("arrow glyph should have a spawn spec");
    let arrow = DrawableHandle::new(
        arrow_id,
        arrow_spec.lock().unwrap().kind.clone(),
        arrows.drawable.state.clone(),
        arrows.drawable.segment_idx,
    );
    *arrow.spec.lock().unwrap() = arrow_spec.lock().unwrap().clone();
    let (authored, resolved) = authored_and_resolved_stroke(&arrow, &theme);
    assert_eq!(resolved, authored, "theme replaced an arrow colormap color");

    let (authored, resolved) = authored_and_resolved_stroke(&streams.lines[0], &theme);
    assert_eq!(
        resolved, authored,
        "theme replaced a streamline colormap color"
    );
}

#[test]
fn three_dimensional_advection_uses_native_polyline_lens() {
    let mut canvas = SceneModel::new(640, 360);
    let space = canvas
        .coordinate_axes_3d(
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-2.0, 2.0).unwrap(),
            [4.0, 4.0, 4.0],
            false,
        )
        .unwrap();
    let field = canvas.vector_field_3d(&space, |[x, y, z]| Some([-y, x, 0.2 - z]));
    let target = canvas
        .sphere(0.05, 8, 6, gaanim_scene::Material3D::matte(Color::WHITE))
        .unwrap();
    let animation = canvas
        .advect_3d(
            &field,
            &target,
            [1.0, 0.0, 0.0],
            StreamlineOptions {
                direction: gaanim_visualization::StreamDirection::Forward,
                max_time: 0.3,
                ..Default::default()
            },
            0.5,
        )
        .unwrap();
    assert!(matches!(
        animation.inner.anim_type,
        crate::anim::AnimationType::MoveAlongPath3D { .. }
    ));
}

#[test]
fn parameter_fields_compile_geometry_regenerators() {
    let mut canvas = SceneModel::new(640, 360);
    let parameter = canvas.parameter(1.0).unwrap();
    let space = canvas
        .coordinate_axes(
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-2.0, 2.0).unwrap(),
            Some(400.0),
            Some(300.0),
            false,
        )
        .unwrap();
    let field = canvas
        .vector_field_2d_reactive(
            &space,
            ReactiveFunction::new(
                2,
                2,
                vec![gaanim_animation::ReactiveInput::Signal(
                    parameter.drawable().id,
                )],
                |values| Ok(vec![-values[1] * values[2], values[0] * values[2]]),
            ),
        )
        .unwrap();
    canvas
        .arrow_vector_field_2d(&field, [3, 3], ArrowFieldOptions::default())
        .unwrap();
    canvas
        .stream_lines_2d(
            &field,
            [3, 3],
            StreamLinesStyle {
                integration: StreamlineOptions {
                    max_time: 0.2,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
    let space_3d = canvas
        .coordinate_axes_3d(
            Axis::linear(-1.0, 1.0).unwrap(),
            Axis::linear(-1.0, 1.0).unwrap(),
            Axis::linear(-1.0, 1.0).unwrap(),
            [2.0, 2.0, 2.0],
            false,
        )
        .unwrap();
    let field_3d = canvas
        .vector_field_3d_reactive(
            &space_3d,
            ReactiveFunction::new(
                3,
                3,
                vec![gaanim_animation::ReactiveInput::Signal(
                    parameter.drawable().id,
                )],
                |values| {
                    Ok(vec![
                        -values[1] * values[3],
                        values[0] * values[3],
                        -values[2] * values[3],
                    ])
                },
            ),
        )
        .unwrap();
    canvas
        .arrow_vector_field_3d(&field_3d, [2, 2, 2], ArrowFieldOptions::default())
        .unwrap();

    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    let regenerators = world
        .query::<&gaanim_animation::Updater>()
        .iter(&world)
        .count();
    assert!(
        regenerators > 1,
        "every field representation should remain reactive"
    );
}

#[test]
fn reactive_parametric_curves_and_surfaces_compile_regenerators() {
    let mut canvas = SceneModel::new(640, 360);
    let parameter = canvas.parameter(1.0).unwrap();
    let input = vec![gaanim_animation::ReactiveInput::Signal(
        parameter.drawable().id,
    )];
    let space = canvas
        .coordinate_axes(
            Axis::linear(-2.0, 2.0).unwrap(),
            Axis::linear(-2.0, 2.0).unwrap(),
            Some(400.0),
            Some(300.0),
            false,
        )
        .unwrap();
    canvas
        .reactive_parametric_plot(
            &space,
            ReactiveFunction::new(1, 2, input.clone(), |values| {
                Ok(vec![values[0], values[0] * values[1]])
            }),
            (-1.0, 1.0),
            Sampling::Fixed { samples: 16 },
        )
        .unwrap();
    let space_3d = canvas
        .coordinate_axes_3d(
            Axis::linear(-1.0, 1.0).unwrap(),
            Axis::linear(-1.0, 1.0).unwrap(),
            Axis::linear(-1.0, 1.0).unwrap(),
            [2.0, 2.0, 2.0],
            false,
        )
        .unwrap();
    canvas
        .reactive_parametric_plot_3d(
            &space_3d,
            ReactiveFunction::new(1, 3, input.clone(), |values| {
                Ok(vec![values[0], values[0] * values[1], 0.0])
            }),
            (-1.0, 1.0),
            16,
        )
        .unwrap();
    canvas
        .reactive_surface_plot(
            &space_3d,
            ReactiveFunction::new(2, 1, input, |values| {
                Ok(vec![values[2] * values[0] * values[1]])
            }),
            [4, 4],
            None,
            None,
        )
        .unwrap();

    let mut world = bevy::prelude::World::new();
    world.insert_resource(gaanim_timeline::timeline::Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();

    assert_eq!(
        world
            .query::<&gaanim_animation::AlwaysRedrawRegen>()
            .iter(&world)
            .count(),
        1
    );
    assert_eq!(
        world
            .query::<&gaanim_animation::ReactiveLineRegen>()
            .iter(&world)
            .count(),
        1
    );
    assert_eq!(
        world
            .query::<&gaanim_animation::ReactiveMeshRegen>()
            .iter(&world)
            .count(),
        1
    );
}
