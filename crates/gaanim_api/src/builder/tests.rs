use super::*;
use bevy::ecs::world::CommandQueue;
use bevy::prelude::World;

fn square_path(offset: f64) -> std::sync::Arc<gaanim_core::kurbo::BezPath> {
    let mut path = gaanim_core::kurbo::BezPath::new();
    path.move_to((offset, 0.0));
    path.line_to((offset + 10.0, 0.0));
    path.line_to((offset + 10.0, 10.0));
    path.line_to((offset, 10.0));
    path.close_path();
    std::sync::Arc::new(path)
}

fn hierarchy_child(
    id: ObjectId,
    entity: Entity,
    path: std::sync::Arc<gaanim_core::kurbo::BezPath>,
    character: char,
) -> HierarchyChild {
    HierarchyChild {
        id,
        entity,
        span: gaanim_scene::components::TextSpan {
            character,
            char_index: 0,
            source_range: (0..character.len_utf8()).into(),
        },
        ligature: None,
        path,
        bounds: Bounds3D::default(),
        transform: SpatialTransform::default(),
        fill: Some(Brush::Solid(Color::WHITE)),
        stroke: StrokeBrush::transparent(),
    }
}

fn hierarchy_state(
    entity: Entity,
    path: std::sync::Arc<gaanim_core::kurbo::BezPath>,
    children: Vec<HierarchyChild>,
) -> MobjectState {
    MobjectState {
        fill_level: 0.0,
        path,
        bounds: Bounds3D::default(),
        transform: SpatialTransform::default(),
        opacity: 1.0,
        fill: Some(Brush::Solid(Color::WHITE)),
        stroke: StrokeBrush::transparent(),
        entity,
        children: children.iter().map(|child| child.id).collect(),
        child_spans: children,
        parent: None,
        exclude_from_parent_draw: false,
    }
}

#[test]
fn flattened_text_proxy_expands_stroke_only_fraction_rules() {
    let mut rule = gaanim_core::kurbo::BezPath::new();
    rule.move_to((-10.0, 0.0));
    rule.line_to((10.0, 0.0));
    let child = HierarchyChild {
        id: ObjectId::from_parts(1, 1),
        entity: Entity::PLACEHOLDER,
        span: gaanim_scene::components::TextSpan {
            character: '_',
            char_index: 0,
            source_range: (0..0).into(),
        },
        ligature: None,
        path: std::sync::Arc::new(rule),
        bounds: Bounds3D::default(),
        transform: SpatialTransform::default(),
        fill: None,
        stroke: StrokeBrush::new(Color::BLACK, 2.0),
    };

    let flattened = compose_child_paths(&[child]);
    let bounds = flattened.bounding_box();
    assert!(
        bounds.height() >= 1.9,
        "fraction rule collapsed to an unfillable line: {bounds:?}"
    );
    assert!(
        flattened.elements().len() > 2,
        "fraction rule was not expanded into outline geometry"
    );
}

#[test]
fn typst_fraction_rule_contributes_area_to_the_transform_proxy() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let equation = builder.typst(
        "sum_(k=1)^n k = (n(n+1)) / 2",
        true,
        None,
        None,
        Some(32.0),
        Some(32.0),
    );
    let state = builder.states.get(equation.id).expect("compiled equation");
    let centerline_element_count: usize = state
        .child_spans
        .iter()
        .map(|child| child.path.elements().len())
        .sum();
    let fraction_rules = state
        .child_spans
        .iter()
        .filter(|child| child.fill.is_none() && child.stroke.brush.is_some())
        .count();

    assert!(
        fraction_rules > 0,
        "Typst fraction must expose a stroke-only rule"
    );
    assert!(
        state.path.elements().len() > centerline_element_count,
        "flattened transform proxy kept only the fraction centerline"
    );
}

#[test]
fn draw_group_slots_follow_order() {
    use crate::anim::DrawOrder;
    assert_eq!(draw_group_slots(4, DrawOrder::Forward, 0), [0, 1, 2, 3]);
    assert_eq!(draw_group_slots(4, DrawOrder::Reverse, 0), [3, 2, 1, 0]);
    assert_eq!(draw_group_slots(5, DrawOrder::Center, 0), [2, 1, 0, 1, 2]);
    assert_eq!(draw_group_slots(4, DrawOrder::Center, 0), [1, 0, 0, 1]);
    assert_eq!(draw_group_slots(1, DrawOrder::Center, 0), [0]);
    assert!(draw_group_slots(0, DrawOrder::Random, 0).is_empty());

    let random = draw_group_slots(12, DrawOrder::Random, 0);
    assert_eq!(random, draw_group_slots(12, DrawOrder::Random, 0));
    assert_ne!(random, draw_group_slots(12, DrawOrder::Random, 7));
    let mut sorted = random.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..12).collect::<Vec<_>>());
    assert_ne!(random, (0..12).collect::<Vec<_>>());
}

#[test]
fn bounds_edge_point_pins_edges_and_corners() {
    use gaanim_core::glam::DVec3;
    use gaanim_layout::Direction;
    let bounds = Bounds3D::new_2d(-1.0, -2.0, 3.0, 4.0);
    let anchor = |direction: Direction| bounds_edge_point(bounds, direction.to_vector());
    assert_eq!(anchor(Direction::Down), DVec3::new(1.0, -2.0, 0.0));
    assert_eq!(anchor(Direction::Right), DVec3::new(3.0, 1.0, 0.0));
    assert!(anchor(Direction::UpLeft).abs_diff_eq(DVec3::new(-1.0, 4.0, 0.0), 1e-12));
    assert!(
        anchor(Direction::Custom(DVec3::new(2.0, 1.0, 0.0)))
            .abs_diff_eq(DVec3::new(3.0, 2.5, 0.0), 1e-12)
    );
    assert_eq!(anchor(Direction::Custom(DVec3::ZERO)), bounds.center());
}

#[test]
fn draw_item_slots_start_group_members_together() {
    use crate::anim::{DrawAnimationConfig, DrawOrder};
    let [a, b, c, d, e] = [1, 2, 3, 4, 5].map(ObjectId::from_raw);

    let (items, slots) = draw_item_slots(vec![a, b, c], None);
    assert_eq!((items, slots), (vec![a, b, c], vec![0, 1, 2]));

    let config = DrawAnimationConfig {
        groups: Some(vec![vec![c, a], vec![b, ObjectId::from_raw(99)]]),
        order: DrawOrder::Reverse,
        ..Default::default()
    };
    // `d` and `e` are drawn but ungrouped; the unknown id is ignored.
    let (items, slots) = draw_item_slots(vec![a, b, c, d, e], Some(&config));
    assert_eq!(items, [e, d, b, c, a]);
    assert_eq!(slots, [0, 1, 2, 3, 3]);
}

#[test]
fn adaptive_lag_ratio_matches_manim_formula() {
    assert!((adaptive_lag_ratio(1) - 0.2).abs() < f64::EPSILON);
    assert!((adaptive_lag_ratio(2) - 0.2).abs() < f64::EPSILON);
    assert!((adaptive_lag_ratio(40) - 0.1).abs() < 1e-9);
    // Grouped writes: a few groups in turn, many overlapping.
    assert!((grouped_lag_ratio(3) - 1.0).abs() < f64::EPSILON);
    assert!((grouped_lag_ratio(8) - 0.5).abs() < f64::EPSILON);
}

#[test]
fn equation_residual_collapses_into_its_visual_center() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let state = hierarchy_state(entity, square_path(40.0), Vec::new());
    let collapsed = SceneBuilder::equation_center_collapsed_transform(&state);
    let visual_center = gaanim_core::kurbo::Point::new(45.0, 5.0);
    let before = state.transform.to_affine_2d() * visual_center;
    let after = collapsed.to_affine_2d() * visual_center;

    assert_eq!(collapsed.scale, DVec3::ZERO);
    assert!((before.x - after.x).abs() < 1e-9);
    assert!((before.y - after.y).abs() < 1e-9);
    assert_eq!(after, visual_center);
}

#[test]
fn typst_selection_source_uses_codex_and_typst_syntax() {
    assert_eq!(typst_math_selection_source("g sin(theta)"), "g sin(θ)");
    assert_eq!(typst_math_selection_source("theta''"), "θ″");
    assert_eq!(typst_math_selection_source("sum F_t"), "∑ F_t");
    assert_eq!(typst_math_selection_source("arrow.r.long"), "⟶");
    assert_eq!(typst_math_selection_source("a <= b"), "a ≤ b");
    assert_eq!(typst_math_selection_source("sin(x)"), "sin(x)");
    assert_eq!(typst_math_selection_source("dot(theta)"), "θ\u{307}");
    assert_eq!(typst_math_selection_source("hat(x)^2"), "x\u{302}^2");
    assert_eq!(
        typst_math_selection_source("\"bloques vistos\" + 3"),
        "bloques vistos + 3"
    );
}

/// Fractions and the functions that only lay out their arguments select
/// the glyphs they draw, not their call syntax.
#[test]
fn typst_selection_source_lays_out_fractions_and_layout_calls() {
    let glyphs = |source: &str| normalize_text_selection(&typst_math_selection_source(source));
    assert_eq!(glyphs("frac(d, d x) f"), "ddxf");
    assert_eq!(glyphs("x/2"), "x2");
    assert_eq!(glyphs("(a + b)/(c d)"), "a+bcd");
    assert_eq!(glyphs("sqrt(x) + abs(y)"), "\u{221A}x+|y|");
    assert_eq!(glyphs("bold(v) binom(n, k)"), "v(nk)");
    // Other calls keep their source, as functions applied in math do.
    assert_eq!(glyphs("sin(x) + f(x)"), "sin(x)+f(x)");
}

/// An equation part that holds a fraction selects its glyphs, so frames,
/// fills and part reveals reach it.
#[test]
fn typst_selection_resolves_fractions() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);
    let equation = builder.typst(
        "$frac(d, d x) [f g] = f frac(d g, d x) + (a + b)/c$",
        false,
        None,
        None,
        Some(32.0),
        None,
    );
    let rendered = builder
        .states
        .get(equation.id)
        .expect("compiled equation state")
        .child_spans
        .iter()
        .map(|child| child.span.character)
        .collect::<String>();
    let derivative = builder
        .select_occurrence(equation, "frac(d, d x) [f g]", Some(0))
        .child_ids;
    let product = builder
        .select_occurrence(equation, "f frac(d g, d x)", Some(0))
        .child_ids;
    let slash = builder
        .select_occurrence(equation, "(a + b)/c", Some(0))
        .child_ids;
    // d, d, x, [, f, g, ]; f, d, g, d, x; a, +, b, c; each with its bar.
    assert_eq!(derivative.len(), 8, "rendered={rendered:?}");
    assert_eq!(product.len(), 6, "rendered={rendered:?}");
    assert_eq!(slash.len(), 5, "rendered={rendered:?}");
    assert!(derivative.iter().all(|id| !product.contains(id)));
    // Part of a fraction leaves its bar out.
    let numerator = builder
        .select_occurrence(equation, "d g", Some(0))
        .child_ids;
    assert_eq!(numerator.len(), 2, "rendered={rendered:?}");
}

#[test]
fn typst_selection_resolves_semantic_source_that_renders_as_symbols() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);
    let source = "$ - m text(fill: #rgb(\"ffd700\"), g sin(theta)) = m L theta'' $";
    let equation = builder.typst(source, false, None, None, Some(32.0), None);

    let gravity = builder
        .select_occurrence(equation, "g sin(theta)", Some(0))
        .child_ids;
    let acceleration = builder
        .select_occurrence(equation, "theta''", Some(0))
        .child_ids;
    let state = builder
        .states
        .get(equation.id)
        .expect("compiled equation state");
    let rendered = state
        .child_spans
        .iter()
        .map(|child| child.span.character)
        .collect::<String>();
    assert!(
        !gravity.is_empty(),
        "semantic Typst source must resolve even when theta renders as θ; rendered={rendered:?}"
    );
    assert!(
        !acceleration.is_empty(),
        "Typst prime syntax must resolve to its rendered Unicode glyph; rendered={rendered:?}"
    );
    assert!(gravity.len() < state.child_spans.len());
    assert!(acceleration.len() < state.child_spans.len());
    assert!(gravity.iter().all(|id| !acceleration.contains(id)));

    let symbols = builder.typst(
        "$sum F_t arrow.r.long theta.alt$",
        false,
        None,
        None,
        Some(32.0),
        None,
    );
    assert!(
        !builder
            .select_occurrence(symbols, "sum F_t", Some(0))
            .child_ids
            .is_empty()
    );
    assert!(
        !builder
            .select_occurrence(symbols, "arrow.r.long", Some(0))
            .child_ids
            .is_empty()
    );
    assert!(
        !builder
            .select_occurrence(symbols, "theta.alt", Some(0))
            .child_ids
            .is_empty()
    );
}

#[test]
fn typst_selection_resolves_math_accents() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);
    let equation = builder.typst(
        "$m_2 ell_2 dot(theta)_2^2 sin(theta_1 - theta_2) = 0$",
        false,
        None,
        None,
        Some(32.0),
        None,
    );
    let rendered = builder
        .states
        .get(equation.id)
        .expect("compiled equation state")
        .child_spans
        .iter()
        .map(|child| child.span.character)
        .collect::<String>();
    let whole = builder
        .select_occurrence(equation, "dot(theta)_2^2 sin(theta_1 - theta_2)", None)
        .child_ids;
    let accent = builder
        .select_occurrence(equation, "dot(theta)_2^2", None)
        .child_ids;
    let sine = builder
        .select_occurrence(equation, "sin(theta_1 - theta_2)", None)
        .child_ids;
    assert!(!accent.is_empty(), "rendered={rendered:?}");
    assert!(!sine.is_empty(), "rendered={rendered:?}");
    assert_eq!(
        whole.len(),
        accent.len() + sine.len(),
        "rendered={rendered:?}"
    );
    assert!(accent.iter().all(|id| whole.contains(id)));
}

#[test]
fn literal_selection_matches_follow_selection_normalization() {
    let text = "La casa, la ca sa";
    let at = |ranges: Vec<std::ops::Range<usize>>| -> Vec<&str> {
        ranges.into_iter().map(|range| &text[range]).collect()
    };
    assert_eq!(at(literal_selection_matches(text, "la")), ["La", "la"]);
    assert_eq!(
        at(literal_selection_matches(text, "casa")),
        ["casa", "ca sa"]
    );
    assert_eq!(
        at(literal_selection_matches(text, "casa, la")),
        ["casa, la"]
    );
    assert!(literal_selection_matches(text, "casa la").is_empty());
    assert!(literal_selection_matches(text, " ").is_empty());
}

#[test]
fn glyphs_after_a_line_break_record_their_own_characters() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let text = builder.typst(
        "primera linea\nsegunda con otra frase\ntercera",
        false,
        None,
        None,
        Some(32.0),
        None,
    );
    let state = builder.states.get(text.id).expect("compiled text");
    let drawn: String = state
        .child_spans
        .iter()
        .flat_map(|child| child.characters())
        .collect();
    assert_eq!(drawn, "primeralineasegundaconotrafrasetercera");

    let phrase = builder.select(text, "otra frase").child_ids;
    assert_eq!(phrase.len(), 9);
    assert_eq!(builder.select(text, "tercera").child_ids.len(), 7);
}

#[test]
fn selections_match_every_character_a_ligature_draws() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let text = builder.typst("uno filtrado dos", false, None, None, Some(32.0), None);
    let state = builder.states.get(text.id).expect("compiled text");
    let ligature = state
        .child_spans
        .iter()
        .find(|child| child.ligature.is_some())
        .expect("the default font joins fi into one glyph");
    assert_eq!(ligature.characters().collect::<String>(), "fi");
    let ligature = ligature.id;

    let word = builder.select(text, "filtrado").child_ids;
    assert_eq!(word.len(), 7, "fi is one glyph");
    assert!(word.contains(&ligature));
    // A fragment that starts or ends inside the ligature takes its glyph.
    assert!(
        builder
            .select(text, "iltrado")
            .child_ids
            .contains(&ligature)
    );
    assert!(builder.select(text, "uno f").child_ids.contains(&ligature));
}

#[test]
fn typst_selection_preserves_literal_precedence_and_occurrence() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let repeated = builder.typst("$theta + theta$", false, None, None, Some(32.0), None);
    let first = builder
        .select_occurrence(repeated, "theta", Some(0))
        .child_ids;
    let second = builder
        .select_occurrence(repeated, "theta", Some(1))
        .child_ids;
    assert!(!first.is_empty());
    assert!(!second.is_empty());
    assert!(first.iter().all(|id| !second.contains(id)));

    let mixed = builder.typst(
        "#text(\"theta\") $theta$",
        false,
        None,
        None,
        Some(32.0),
        None,
    );
    let literal = builder.select(mixed, "theta").child_ids;
    let state = builder.states.get(mixed.id).expect("compiled mixed text");
    let selected_text = literal
        .iter()
        .filter_map(|id| {
            state
                .child_spans
                .iter()
                .find(|child| child.id == *id)
                .map(|child| child.span.character)
        })
        .collect::<String>();
    assert_eq!(selected_text, "theta");
}

/// `indicate` pulses a glyph in place: a highlighted term slid across its
/// neighbors when it scaled about its origin.
#[test]
fn indicate_scales_in_place_around_visual_center() {
    assert_pulses_in_place(AnimationType::Indicate {
        color: None,
        scale_factor: 1.1,
    });
}

fn assert_pulses_in_place(anim_type: AnimationType) {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let parent_id = builder.next_id();
    let target_id = builder.next_id();
    let entity = builder.commands.spawn_empty().id();
    builder.states.insert(
        target_id,
        MobjectState {
            fill_level: 0.0,
            path: square_path(40.0),
            // This mimics a glyph whose bounds have already been centered
            // relative to its textual parent, while its path remains local.
            bounds: Bounds3D::new_2d(0.0, 0.0, 10.0, 10.0),
            transform: SpatialTransform::identity(),
            opacity: 1.0,
            fill: Some(Brush::Solid(Color::WHITE)),
            stroke: StrokeBrush::transparent(),
            entity,
            child_spans: Vec::new(),
            children: Vec::new(),
            parent: Some(parent_id),
            exclude_from_parent_draw: false,
        },
    );

    builder.play(AnimationBuilder {
        target: target_id,
        anim_type,
        duration: 1.0,
        delay: 0.0,
        rate_func: gaanim_math::RateFunc::ThereAndBack,
    });

    let state = builder.states.get(target_id).unwrap();
    assert_eq!(state.transform.anchor, DVec3::new(45.0, 5.0, 0.0));

    let translates = builder.timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::Translation { .. },
                ..
            }) if *target == target_id
        )
    });
    assert!(!translates, "a pulse must not move the target");

    let scales: Vec<_> = builder
        .timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::Scale { to, .. },
                ..
            }) if *target == target_id => Some(*to),
            _ => None,
        })
        .collect();
    assert!(scales.iter().any(|scale| (scale.x - 1.1).abs() < 1e-9));
}

#[test]
fn hierarchical_transform_flattens_once_and_preserves_source_identity() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let source_id = builder.next_id();
    let source_child_id = builder.next_id();
    let target_id = builder.next_id();
    let target_child_id = builder.next_id();
    let source_entity = builder.commands.spawn_empty().id();
    let source_child_entity = builder.commands.spawn_empty().id();
    let target_entity = builder.commands.spawn_empty().id();
    let target_child_entity = builder.commands.spawn_empty().id();
    let source_path = square_path(0.0);
    let target_path = square_path(40.0);
    let source_child = hierarchy_child(
        source_child_id,
        source_child_entity,
        source_path.clone(),
        'A',
    );
    let target_child = hierarchy_child(
        target_child_id,
        target_child_entity,
        target_path.clone(),
        '∑',
    );

    builder.states.insert(
        source_child_id,
        hierarchy_state(source_child_entity, source_path.clone(), Vec::new()),
    );
    builder.states.insert(
        target_child_id,
        hierarchy_state(target_child_entity, target_path.clone(), Vec::new()),
    );
    builder.states.insert(
        source_id,
        hierarchy_state(source_entity, source_path, vec![source_child]),
    );
    builder.states.insert(
        target_id,
        hierarchy_state(target_entity, target_path.clone(), vec![target_child]),
    );
    builder.text_metrics.insert(
        source_id,
        gaanim_text::prelude::TextMetrics {
            first_baseline: -0.1,
            line_count: 1,
            ..Default::default()
        },
    );
    let target_metrics = gaanim_text::prelude::TextMetrics {
        first_baseline: -0.3,
        line_count: 1,
        ..Default::default()
    };
    builder.text_metrics.insert(target_id, target_metrics);

    builder.play(AnimationBuilder {
        target: source_id,
        anim_type: AnimationType::Transform {
            target: target_id,
            pairs: Vec::new(),
        },
        duration: 1.0,
        delay: 0.0,
        rate_func: gaanim_math::RateFunc::Linear,
    });

    let source_after = builder.states.get(source_id).expect("source state");
    assert_eq!(source_after.entity, source_entity);
    assert_eq!(source_after.path, target_path);
    assert!(source_after.child_spans.is_empty());

    let animated_path_targets: Vec<_> = builder
        .timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::PathMorph { .. },
                ..
            }) if clip.duration > 0.0 => Some(*target),
            _ => None,
        })
        .collect();
    assert_eq!(animated_path_targets, vec![source_id]);
    assert!(!animated_path_targets.contains(&source_child_id));
    assert!(!animated_path_targets.contains(&target_child_id));

    let synthesized_stroke = builder.timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::StrokeColor { .. }
                    | PropertyLensSpec::StrokeWidth { .. },
                ..
            }) if *target == source_id
        )
    });
    assert!(
        !synthesized_stroke,
        "no-stroke text/math morph must not create a white outline"
    );
    assert_eq!(builder.text_metrics.get(&source_id), Some(&target_metrics));
    assert!(builder.timeline.clips.values().any(|clip| matches!(
        &clip.payload,
        ClipPayload::SetTextBaseline { target, baseline }
            if *target == source_id && *baseline == target_metrics.first_baseline
    )));
}

#[test]
fn delayed_transform_starts_at_its_resolved_sequence_offset() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let source_id = builder.next_id();
    let target_id = builder.next_id();
    let source_entity = builder.commands.spawn_empty().id();
    let target_entity = builder.commands.spawn_empty().id();
    builder.states.insert(
        source_id,
        hierarchy_state(source_entity, square_path(0.0), Vec::new()),
    );
    builder.states.insert(
        target_id,
        hierarchy_state(target_entity, square_path(40.0), Vec::new()),
    );

    builder.play_at_current_time(AnimationBuilder {
        target: source_id,
        anim_type: AnimationType::Transform {
            target: target_id,
            pairs: Vec::new(),
        },
        duration: 1.0,
        delay: 2.0,
        rate_func: gaanim_math::RateFunc::Linear,
    });

    let path_morph_start = builder
        .timeline
        .clips
        .values()
        .find_map(|clip| match &clip.payload {
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::PathMorph { .. },
                ..
            }) if *target == source_id && clip.duration > 0.0 => Some(clip.start),
            _ => None,
        })
        .expect("path morph clip");
    assert_eq!(path_morph_start, 2.0);
}

#[test]
fn no_fill_transforms_do_not_materialize_a_white_fill() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let source_id = builder.next_id();
    let target_id = builder.next_id();
    let source_entity = builder.commands.spawn_empty().id();
    let target_entity = builder.commands.spawn_empty().id();
    let replacement_source_id = builder.next_id();
    let replacement_target_id = builder.next_id();
    let replacement_source_entity = builder.commands.spawn_empty().id();
    let replacement_target_entity = builder.commands.spawn_empty().id();
    let mut source = hierarchy_state(source_entity, square_path(0.0), Vec::new());
    source.fill = None;
    let mut target = hierarchy_state(target_entity, square_path(40.0), Vec::new());
    target.fill = None;
    let mut replacement_source =
        hierarchy_state(replacement_source_entity, square_path(80.0), Vec::new());
    replacement_source.fill = None;
    let mut replacement_target =
        hierarchy_state(replacement_target_entity, square_path(120.0), Vec::new());
    replacement_target.fill = None;
    builder.states.insert(source_id, source);
    builder.states.insert(target_id, target);
    builder
        .states
        .insert(replacement_source_id, replacement_source);
    builder
        .states
        .insert(replacement_target_id, replacement_target);

    builder.play(AnimationBuilder {
        target: source_id,
        anim_type: AnimationType::Transform {
            target: target_id,
            pairs: Vec::new(),
        },
        duration: 1.0,
        delay: 0.0,
        rate_func: gaanim_math::RateFunc::Linear,
    });
    builder.play(AnimationBuilder {
        target: replacement_source_id,
        anim_type: AnimationType::ReplacementTransform {
            target: replacement_target_id,
        },
        duration: 1.0,
        delay: 0.0,
        rate_func: gaanim_math::RateFunc::Linear,
    });

    assert!(builder.states.get(source_id).unwrap().fill.is_none());
    assert!(
        builder
            .states
            .get(replacement_source_id)
            .unwrap()
            .fill
            .is_none()
    );
    assert!(!builder.timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::FillColor { .. },
                ..
            }) if *target == source_id || *target == replacement_source_id
        )
    }));
}

#[test]
fn transform_keeps_the_source_opacity_over_a_hidden_target() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let source_id = builder.next_id();
    let target_id = builder.next_id();
    let source_entity = builder.commands.spawn_empty().id();
    let target_entity = builder.commands.spawn_empty().id();
    let mut source = hierarchy_state(source_entity, square_path(0.0), Vec::new());
    source.opacity = 0.8;
    let mut target = hierarchy_state(target_entity, square_path(40.0), Vec::new());
    target.opacity = 0.0;
    builder.states.insert(source_id, source);
    builder.states.insert(target_id, target);

    builder.play(AnimationBuilder {
        target: source_id,
        anim_type: AnimationType::Transform {
            target: target_id,
            pairs: Vec::new(),
        },
        duration: 1.0,
        delay: 0.0,
        rate_func: gaanim_math::RateFunc::Linear,
    });

    assert_eq!(builder.states.get(source_id).unwrap().opacity, 0.8);
    assert!(!builder.timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::Opacity { .. },
                ..
            }) if *target == source_id
        )
    }));
}

#[test]
fn write_keeps_synthesized_outline_temporary() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);
    let id = builder.next_id();
    let entity = builder.commands.spawn_empty().id();
    builder
        .states
        .insert(id, hierarchy_state(entity, square_path(0.0), Vec::new()));

    builder.play(AnimationBuilder {
        target: id,
        anim_type: AnimationType::Write {
            config: crate::anim::DrawAnimationConfig::default(),
        },
        duration: 1.0,
        delay: 0.0,
        rate_func: gaanim_math::RateFunc::Linear,
    });

    assert!(builder.states.get(id).unwrap().stroke.brush.is_none());
    assert!(builder.timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::StrokeWidth { from, to },
                ..
            }) if *target == id && *from <= 0.05 && *to == 0.0
        )
    }));
}

#[test]
fn create_traces_then_smoothly_fades_the_fill() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);
    let id = builder.next_id();
    let entity = builder.commands.spawn_empty().id();
    builder
        .states
        .insert(id, hierarchy_state(entity, square_path(0.0), Vec::new()));

    builder.play(AnimationBuilder {
        target: id,
        anim_type: AnimationType::Create {
            config: crate::anim::DrawAnimationConfig::default(),
        },
        duration: 1.0,
        delay: 0.0,
        rate_func: gaanim_math::RateFunc::Linear,
    });

    let fill_fade = builder
        .timeline
        .clips
        .values()
        .find_map(|clip| match &clip.payload {
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::FillDrawProgress { from, to },
                rate_func,
                ..
            }) if *target == id && *from == 0.0 && *to == 1.0 => {
                Some((clip.start, clip.duration, rate_func.clone()))
            }
            _ => None,
        })
        .expect("Create must fade a closed shape's fill instead of revealing it abruptly");

    assert!((fill_fade.0 - 0.7).abs() < 1e-9);
    assert!((fill_fade.1 - 0.3).abs() < 1e-9);
    assert!(matches!(fill_fade.2, gaanim_math::RateFunc::Smooth));
}

#[test]
fn create_uses_the_full_duration_for_unfilled_paths() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);
    let id = builder.next_id();
    let entity = builder.commands.spawn_empty().id();
    let mut state = hierarchy_state(entity, square_path(0.0), Vec::new());
    state.fill = None;
    builder.states.insert(id, state);

    builder.play(AnimationBuilder {
        target: id,
        anim_type: AnimationType::Create {
            config: crate::anim::DrawAnimationConfig::default(),
        },
        duration: 1.0,
        delay: 0.0,
        rate_func: gaanim_math::RateFunc::DoubleSmooth,
    });

    let trace_duration = builder
        .timeline
        .clips
        .values()
        .find_map(|clip| match &clip.payload {
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::PathCompletion { from, to },
                ..
            }) if *target == id && *from == 0.0 && *to == 1.0 => Some(clip.duration),
            _ => None,
        })
        .expect("unfilled Create path trace");

    assert!((trace_duration - 1.0).abs() < 1e-9);
    assert!(!builder.timeline.clips.values().any(|clip| {
        matches!(
            &clip.payload,
            ClipPayload::Animation(AnimationSpec {
                target,
                lens: PropertyLensSpec::FillDrawProgress { to, .. },
                ..
            }) if *target == id && *to == 1.0
        )
    }));
}

#[test]
fn unstyled_group_preserves_child_fill_after_style_propagation() {
    let mut world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let gold = Color::from_rgb8(0xff, 0xd7, 0x00);
    let child = builder.rectangle(40.0, 30.0).fill(gold).spawn();
    let child_entity = builder.states.get(child.id).unwrap().entity;
    let group = builder.group(&[child]);
    let group_entity = builder.states.get(group.id).unwrap().entity;

    drop(builder);
    queue.apply(&mut world);

    let expected = FillBrush::color(gold);
    assert_eq!(world.get::<FillBrush>(child_entity), Some(&expected));
    assert!(world.get::<FillBrush>(group_entity).is_none());

    let mut schedule = bevy::prelude::Schedule::default();
    schedule.add_systems(gaanim_scene::systems::style_propagation_system);
    schedule.run(&mut world);

    assert_eq!(world.get::<FillBrush>(child_entity), Some(&expected));
}

#[test]
fn group_preserves_the_authored_child_coordinate_frame() {
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let child = builder.rectangle(40.0, 30.0).at(120.0, -45.0).spawn();
    let authored = builder.states.get(child.id).unwrap().transform;
    let group = builder.group(&[child]);

    assert_eq!(builder.states.get(child.id).unwrap().transform, authored);
    assert_eq!(builder.get_world_transform(child.id), authored);
    assert_eq!(
        builder.states.get(group.id).unwrap().transform.to_mat4(),
        gaanim_core::glam::DMat4::IDENTITY,
        "creating a group must not replace the coordinate system used by child updaters"
    );
}

#[test]
fn remove_from_group_preserves_world_transform_and_visibility() {
    let mut world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = Timeline::new();
    let fonts = FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    let mut builder = SceneBuilder::new(&mut commands, &mut timeline, &fonts, &text_config);

    let child = builder.rectangle(40.0, 30.0).at(120.0, -45.0).spawn();
    let child_entity = builder.states.get(child.id).unwrap().entity;
    let group = builder.group(&[child]);
    let before = builder.get_world_transform(child.id);

    builder.remove_from_group(group, child);

    let child_state = builder.states.get(child.id).unwrap();
    assert_eq!(child_state.parent, None);
    assert_eq!(child_state.transform, before);
    assert_eq!(child_state.opacity, 1.0);
    assert!(
        !builder
            .states
            .get(group.id)
            .unwrap()
            .children
            .contains(&child.id)
    );

    drop(builder);
    queue.apply(&mut world);
    assert!(
        world
            .get::<bevy::ecs::hierarchy::ChildOf>(child_entity)
            .is_none()
    );
    assert_eq!(world.get::<Opacity>(child_entity), Some(&Opacity(1.0)));
}
