use super::*;
use crate::canvas::{Anchor, Direction};
use gaanim_core::peniko::Color;

fn assert_same(scene: &SceneModel, handle: &DrawableHandle) {
    assert!(
        scene.isolated_declaration(handle.id).is_some(),
        "the fast path applies"
    );
    let fast = scene.bounds_of(handle).unwrap();
    let full = scene.compiled_bounds(handle.id).unwrap();
    for (a, b) in [
        (fast.min.x, full.min.x),
        (fast.min.y, full.min.y),
        (fast.max.x, full.max.x),
        (fast.max.y, full.max.y),
    ] {
        assert!((a - b).abs() < 1e-9, "{fast:?} != {full:?}");
    }
}

/// A scene with earlier, already animated content.
fn busy_scene() -> SceneModel {
    let mut scene = SceneModel::new(16.0, 9.0);
    let title = scene.text("Título").move_to(0.0, 3.0);
    let dot = scene.circle(0.4).fill(Color::WHITE);
    scene.play(vec![
        title.animate().write().duration(0.5),
        dot.animate().shift_by(2.0, 0.0).duration(0.5),
    ]);
    scene.segment("dos", None).unwrap();
    let other = scene.rect(1.0, 1.0).move_to(-3.0, 0.0);
    scene.play(vec![other.animate().rotate_by(0.5).duration(0.3)]);
    scene
}

#[test]
fn a_fresh_declaration_measures_like_the_compiled_scene() {
    let mut scene = busy_scene();
    let label = scene
        .text("Datos de diseño")
        .at_anchor(-6.0, -2.7, Anchor::Left)
        .scale_by(1.3)
        .rotate_by(0.2);
    assert_same(&scene, &label);

    // A pill: a text on a rounded rect, gathered in a group.
    let word = scene.text("Carga vertical").move_to(1.0, -1.0);
    let pill = scene
        .rounded_rect(3.0, 0.6, 0.3)
        .stroke(Color::WHITE, 0.02)
        .move_to(1.0, -1.0);
    let group = scene.group(&[&pill, &word]).move_to(2.0, -2.0);
    assert_same(&scene, &group);

    // Placed against another fresh declaration.
    let next = scene
        .text("Sismo moderado")
        .next_to(&label, Direction::Right, 0.18);
    assert_same(&scene, &next);
}

/// A decorated column holding a text and a sized spacer, inside a row
/// on the safe area beside another box, like a slide's cards.
fn card_row(scene: &mut SceneModel) -> (DrawableHandle, DrawableHandle, DrawableHandle) {
    use crate::canvas::{LayoutMemberSpec, LayoutSpec, LayoutWithin};
    use gaanim_core::glam::DVec2;
    use gaanim_layout::{Align, Insets, LayoutNodeKind, LayoutStyle, SizeRule};

    fn place(
        scene: &mut SceneModel,
        members: &[&DrawableHandle],
        decorated: bool,
        item: gaanim_layout::LayoutItemStyle,
        spec: LayoutSpec,
    ) -> DrawableHandle {
        let root = scene.group(members);
        for member in members {
            member.claim_layout(&root).unwrap();
        }
        if decorated {
            scene
                .decorate_layout(&root, Some(Color::WHITE.into()), None, 0.0)
                .unwrap();
        }
        root.set_layout_item(item);
        let snapshots = members
            .iter()
            .map(|member| LayoutMemberSpec {
                id: member.id,
                style: member.layout_item(),
            })
            .collect();
        scene.reflow_layout(&root, snapshots, spec, 1, None, None, None);
        root
    }

    let label = scene.text("Densidad de muros");
    let slot = scene.rect(0.1, 0.1).no_fill().no_stroke();
    slot.set_layout_item(gaanim_layout::LayoutItemStyle {
        shrink: 0.0,
        ..Default::default()
    });
    let card = place(
        scene,
        &[&slot, &label],
        true,
        gaanim_layout::LayoutItemStyle {
            grow: 1.0,
            ..Default::default()
        },
        LayoutSpec {
            kind: LayoutNodeKind::Column { wrap: false },
            style: LayoutStyle {
                padding: Insets::all(0.25),
                gap: DVec2::splat(0.15),
                height: SizeRule::Fill(1.0),
                ..Default::default()
            },
            within: LayoutWithin::Intrinsic,
        },
    );
    let other = scene.circle(0.6);
    let row = place(
        scene,
        &[&card, &other],
        false,
        Default::default(),
        LayoutSpec {
            kind: LayoutNodeKind::Row { wrap: false },
            style: LayoutStyle {
                width: SizeRule::Fill(1.0),
                height: SizeRule::Fill(1.0),
                padding: Insets::all(0.4),
                gap: DVec2::splat(0.3),
                align: Align::Stretch,
                ..Default::default()
            },
            within: LayoutWithin::Safe,
        },
    );
    (row, card, slot)
}

#[test]
fn a_fresh_box_tree_measures_like_the_compiled_scene() {
    let mut scene = busy_scene();
    let (row, card, slot) = card_row(&mut scene);
    assert_same(&scene, &slot);
    assert_same(&scene, &card);
    assert_same(&scene, &row);

    // A root box placed by one of its anchors beside the first tree.
    let (moved, _, inner) = card_row(&mut scene);
    let moved = moved.at_anchor(-6.0, 2.0, Anchor::Left);
    assert_same(&scene, &inner);
    assert_same(&scene, &moved);
    assert_same(&scene, &slot);
}

/// A decorated 0.4 x 0.2 card, smaller than its background's 1 x 1
/// placeholder, beside a circle of diameter 1.2: a 1.6 x 1.2 row.
fn decorated_row(scene: &mut SceneModel) -> (DrawableHandle, DrawableHandle) {
    use crate::canvas::{LayoutMemberSpec, LayoutSpec, LayoutWithin};
    use gaanim_layout::LayoutNodeKind;

    fn place(
        scene: &mut SceneModel,
        members: &[&DrawableHandle],
        decorated: bool,
    ) -> DrawableHandle {
        let root = scene.group(members);
        for member in members {
            member.claim_layout(&root).unwrap();
        }
        if decorated {
            scene
                .decorate_layout(&root, Some(Color::WHITE.into()), None, 0.1)
                .unwrap();
        }
        let snapshots = members
            .iter()
            .map(|member| LayoutMemberSpec {
                id: member.id,
                style: member.layout_item(),
            })
            .collect();
        let spec = LayoutSpec {
            kind: LayoutNodeKind::Row { wrap: false },
            style: Default::default(),
            within: LayoutWithin::Intrinsic,
        };
        scene.reflow_layout(&root, snapshots, spec, 1, None, None, None);
        root
    }

    let slot = scene.rect(0.4, 0.2);
    let card = place(scene, &[&slot], true);
    let dot = scene.circle(0.6);
    let row = place(scene, &[&card, &dot], false);
    (row, card)
}

fn assert_size(bounds: Bounds3D, width: f64, height: f64) {
    assert!(
        (bounds.width() - width).abs() < 1e-6 && (bounds.height() - height).abs() < 1e-6,
        "{bounds:?} is not {width} x {height}"
    );
}

/// A decorated box measures its layout box, not its background's
/// placeholder (#291).
#[test]
fn a_decorated_box_measures_its_layout_box() {
    let mut scene = SceneModel::new(16.0, 9.0);
    let (row, card) = decorated_row(&mut scene);
    assert_size(scene.bounds_of(&card).unwrap(), 0.4, 0.2);
    assert_size(scene.bounds_of(&row).unwrap(), 1.6, 1.2);
}

/// A frame around a box, alone or inside another box, follows the box
/// (#292).
#[test]
fn surrounding_rects_frame_boxes() {
    use bevy::prelude::With;

    let mut scene = SceneModel::new(16.0, 9.0);
    let (row, card) = decorated_row(&mut scene);
    let expected = [
        scene.bounds_of(&card).unwrap(),
        scene.bounds_of(&row).unwrap(),
    ];
    for target in [&card, &row] {
        scene
            .surrounding_rect(vec![target.bounds_target()], [0.0; 4], 0.0)
            .unwrap();
    }
    scene.wait(0.5);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    scene.compile(&mut world);
    world.flush();
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    timeline.seek(&mut world, 0.5);
    gaanim_animation::tracking_line_system(&mut world);
    gaanim_animation::surrounding_rect_system(&mut world);
    let mut drawn: Vec<Bounds3D> = world
        .query_filtered::<&gaanim_scene::LocalBounds, With<gaanim_animation::SurroundingRect>>()
        .iter(&world)
        .map(|bounds| bounds.0)
        .collect();
    drawn.sort_by(|a, b| a.width().total_cmp(&b.width()));
    assert_eq!(drawn.len(), 2);
    for (drawn, expected) in drawn.iter().zip(expected) {
        assert_size(*drawn, expected.width(), expected.height());
        assert!(
            (drawn.center() - expected.center()).length() < 1e-6,
            "{drawn:?} != {expected:?}"
        );
    }
}

/// A readout's row is laid out when the scene compiles, centered on the
/// group's origin as it is every frame, so a layout centers it in its
/// cell instead of placing its unlaid parts (#294).
#[test]
fn a_readout_measures_its_row_centered_on_its_origin() {
    let mut scene = SceneModel::new(16.0, 9.0);
    let number = scene.reactive_readout(
        gaanim_animation::ScalarSource::constant(42.0),
        ".0f",
        "",
        "%",
        "-",
        Some(0.5),
    );
    let label = scene.text_spec(
        gaanim_text::prelude::TextSpec::new(
            vec!["R".into()],
            None,
            gaanim_text::prelude::TextStyle {
                size: Some(0.5),
                ..Default::default()
            },
            gaanim_text::prelude::TextFlow::default(),
        )
        .unwrap(),
    );
    let widths = [
        scene.bounds_of(&label).unwrap().width(),
        scene.bounds_of(&number).unwrap().width(),
    ];
    let readout = scene.reactive_readout_group(Some(&label), None, &number, None, 0.1);
    let bounds = scene.bounds_of(&readout).unwrap();
    assert!(bounds.center().x.abs() < 1e-6, "{bounds:?}");
    // The parts are 0.1 apart from their advances: their inks, by that
    // and the room the font leaves beside the R and the 4.
    let room = bounds.width() - (widths[0] + 0.1 + widths[1]);
    assert!(room > 0.0 && room < 0.1, "{bounds:?} from {widths:?}");
}

#[test]
fn box_trees_with_history_compile_the_scene() {
    let mut scene = busy_scene();
    let (row, _, slot) = card_row(&mut scene);
    scene.play(vec![row.animate().shift_by(1.0, 0.0).duration(0.4)]);
    assert!(scene.isolated_declaration(slot.id).is_none());
    let bounds = scene.bounds_of(&slot).unwrap();
    assert!(bounds.min.x.is_finite(), "{bounds:?}");

    // A fresh tree whose box then reflows over time.
    let (row, card, fresh) = card_row(&mut scene);
    let members = [&card]
        .iter()
        .map(|member| crate::canvas::LayoutMemberSpec {
            id: member.id,
            style: member.layout_item(),
        })
        .collect();
    scene.reflow_layout(&row, members, Default::default(), 2, Some(0.5), None, None);
    assert!(scene.isolated_declaration(fresh.id).is_none());
}

/// The compilation the scene's measurements at the cursor currently
/// share.
fn measured_world(scene: &SceneModel) -> Option<u64> {
    let measurements = scene.measured.0.lock().unwrap();
    let (_, compiled) = measurements.cursor.as_ref()?;
    Some(compiled.generation)
}

#[test]
fn measurements_share_a_compilation_until_the_scene_changes() {
    let mut scene = busy_scene();
    let label = scene.text("medido").move_to(1.0, 1.0);
    let dot = scene.circle(0.3).move_to(-1.0, 0.0);
    scene.play(vec![label.animate().shift_by(1.0, 0.0).duration(0.4)]);
    assert!(scene.isolated_declaration(label.id).is_none());

    let first = scene.bounds_of(&label).unwrap();
    let world = measured_world(&scene).expect("a compiled measurement");
    assert_eq!(scene.bounds_of(&label).unwrap(), first);
    scene.bounds_of(&dot).unwrap();
    assert_eq!(measured_world(&scene), Some(world), "nothing changed");

    // A cut on an animated drawable moves it from the cursor on.
    let label = label.move_to(-2.0, 1.0);
    let moved = scene.bounds_of(&label).unwrap();
    assert!((moved.center().x + 2.0).abs() < 1e-6, "{moved:?}");
    let world = measured_world(&scene).unwrap();

    // A declaration changes what the scene compiles.
    let fresh = scene.rect(1.0, 1.0);
    assert_eq!(scene.bounds_of(&label).unwrap(), moved);
    assert_ne!(measured_world(&scene), Some(world));
    let world = measured_world(&scene).unwrap();
    fresh.fill(Color::BLACK);
    scene.bounds_of(&label).unwrap();
    assert_ne!(measured_world(&scene), Some(world), "a spec changed");

    // So does a scene-wide setting, which lives outside the authored state.
    let world = measured_world(&scene).unwrap();
    scene.background = Some(Color::BLACK);
    scene.bounds_of(&label).unwrap();
    assert_ne!(measured_world(&scene), Some(world));

    // Copies measure in the same compilation; a submitted scene drops it.
    let world = measured_world(&scene).unwrap();
    assert_eq!(scene.clone().bounds_of(&label).unwrap(), moved);
    assert_eq!(measured_world(&scene), Some(world));
    scene.render();
    assert_eq!(measured_world(&scene), None);
}

/// Box of `handle` measured at the cursor, and how the measurement got
/// its compilation, resuming `previous`.
fn measure_resuming(
    scene: &SceneModel,
    handle: &DrawableHandle,
    previous: Option<CompiledMeasure>,
) -> (Bounds3D, Measured, CompiledMeasure) {
    let (mut compiled, how) = scene.measure_at_cursor(previous);
    (compiled.bounds(handle.id).unwrap(), how, compiled)
}

/// A scene in three segments, with a label animated in each.
fn segmented_scene() -> (SceneModel, DrawableHandle) {
    let mut scene = busy_scene();
    let label = scene.text("medido").move_to(1.0, 1.0);
    scene.play(vec![label.animate().shift_by(1.0, 0.0).duration(0.4)]);
    scene.segment("tres", None).unwrap();
    let square = scene.rect(1.0, 1.0).move_to(2.0, -2.0);
    scene.play(vec![
        label.animate().shift_by(0.0, 1.0).duration(0.3),
        square.animate().rotate_by(0.4).duration(0.3),
    ]);
    (scene, label)
}

#[test]
fn measurements_resume_the_compilation_where_the_scene_changed() {
    let (mut scene, label) = segmented_scene();
    let (first, how, compiled) = measure_resuming(&scene, &label, None);
    assert_eq!(how, Measured::Scene);
    assert_eq!(scene.compiled_bounds(label.id).unwrap(), first);

    // Authoring continues in the last segment: only it compiles again.
    let fresh = scene.circle(0.5).move_to(-4.0, 2.0);
    scene.play(vec![
        label.animate().shift_by(-3.0, 0.0).duration(0.2),
        fresh.animate().shift_by(1.0, 0.0).duration(0.2),
    ]);
    let (moved, how, compiled) = measure_resuming(&scene, &label, Some(compiled));
    assert_eq!(how, Measured::Resumed);
    assert_eq!(scene.compiled_bounds(label.id).unwrap(), moved);
    assert!((moved.center().x - first.center().x + 3.0).abs() < 1e-9);
    let (circle, _, compiled) = measure_resuming(&scene, &fresh, Some(compiled));
    assert_eq!(scene.compiled_bounds(fresh.id).unwrap(), circle);

    // A new segment resumes from the start of the one before.
    scene.segment("cuatro", None).unwrap();
    let label = label.move_to(0.0, -1.0);
    let (cut, how, compiled) = measure_resuming(&scene, &label, Some(compiled));
    assert_eq!(how, Measured::Resumed);
    assert_eq!(scene.compiled_bounds(label.id).unwrap(), cut);

    // The same script run again resumes the last run's measurement.
    let (mut again, again_label) = segmented_scene();
    let again_fresh = again.circle(0.5).move_to(-4.0, 2.0);
    again.play(vec![
        again_label.animate().shift_by(-3.0, 0.0).duration(0.2),
        again_fresh.animate().shift_by(1.0, 0.0).duration(0.2),
    ]);
    let (bounds, how, _) = measure_resuming(&again, &again_label, Some(compiled));
    assert_eq!(how, Measured::Resumed);
    assert_eq!(bounds, moved);

    // An earlier segment changed: nothing before it is reused wrongly.
    let (mut edited, edited_label) = segmented_scene();
    let (_, _, compiled) = measure_resuming(&edited, &edited_label, None);
    let mut other = busy_scene();
    other.background = Some(Color::BLACK);
    let other_label = other.text("medido").move_to(3.0, 1.0);
    other.play(vec![other_label.animate().shift_by(1.0, 0.0).duration(0.4)]);
    let (bounds, how, _) = measure_resuming(&other, &other_label, Some(compiled));
    assert_eq!(how, Measured::Scene);
    assert_eq!(other.compiled_bounds(other_label.id).unwrap(), bounds);
    let _ = edited.rect(1.0, 1.0);
}

#[test]
fn a_cut_at_the_cursor_counts() {
    // The second segment's two plays end where the compiled clock
    // (0.5 + 0.3 + 0.4) is a few ULPs past the authored cursor
    // (0.5 + (0.3 + 0.4)); the cut starts at the compiled one.
    let mut scene = busy_scene();
    let label = scene.text("medido").move_to(1.0, 1.0);
    scene.play(vec![label.animate().shift_by(1.0, 0.0).duration(0.4)]);
    let label = label.move_to(-2.0, 1.0);
    let bounds = scene.bounds_of(&label).unwrap();
    assert!((bounds.center().x + 2.0).abs() < 1e-9, "{bounds:?}");
    assert_eq!(scene.compiled_bounds(label.id).unwrap(), bounds);

    // Animations that end at the cursor are measured at their end.
    let label = label.move_to(1.0, 1.0);
    scene.play(vec![label.animate().shift_by(0.5, 0.0).duration(0.3)]);
    let bounds = scene.bounds_of(&label).unwrap();
    assert!((bounds.center().x - 1.5).abs() < 1e-12, "{bounds:?}");
}

#[test]
fn a_box_tree_compiles_once_for_all_its_members() {
    let mut scene = busy_scene();
    let (row, card, slot) = card_row(&mut scene);
    let isolated_world = |scene: &SceneModel| {
        let measurements = scene.measured.0.lock().unwrap();
        let (_, compiled) = measurements.isolated.as_ref()?;
        Some(compiled.generation)
    };
    let boxes = [&slot, &card, &row].map(|member| {
        assert!(scene.isolated_declaration(member.id).is_some());
        scene.bounds_of(member).unwrap()
    });
    let world = isolated_world(&scene).expect("an isolated measurement");
    assert_eq!(scene.bounds_of(&slot).unwrap(), boxes[0]);
    assert_eq!(isolated_world(&scene), Some(world), "one box tree");

    let fresh = scene.text("suelto");
    scene.bounds_of(&fresh).unwrap();
    assert_ne!(isolated_world(&scene), Some(world));
    let world = isolated_world(&scene).unwrap();
    // Another closure, measured after a declaration: compiled again.
    assert_eq!(scene.bounds_of(&card).unwrap(), boxes[1]);
    assert_ne!(isolated_world(&scene), Some(world));
    assert_same(&scene, &row);
}

#[test]
fn declarations_with_history_compile_the_scene() {
    let mut scene = busy_scene();
    let label = scene.text("medido").move_to(1.0, 1.0);
    scene.play(vec![label.animate().shift_by(1.0, 0.0).duration(0.4)]);
    // Frozen and animated: only the compiled scene knows where it is.
    assert!(scene.isolated_declaration(label.id).is_none());
    let bounds = scene.bounds_of(&label).unwrap();
    assert!((bounds.center().x - 2.0).abs() < 1e-6, "{bounds:?}");

    // A later group may move it.
    let fresh = scene.text("suelto");
    let _group = scene.group(&[&fresh]);
    assert!(scene.isolated_declaration(fresh.id).is_none());
    // Placed against something with history.
    let placed = scene.text("junto").next_to(&label, Direction::Down, 0.2);
    assert!(scene.isolated_declaration(placed.id).is_none());
}
