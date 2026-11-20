use super::*;

#[test]
fn selector_shapes_reveal_or_pass_a_wave() {
    for shape in [
        SelectorShape::Square,
        SelectorShape::Ramp,
        SelectorShape::Smooth,
        SelectorShape::EaseIn,
        SelectorShape::EaseOut,
    ] {
        assert_eq!(shape.influence(0.0), 1.0, "{shape:?} starts out");
        assert_eq!(shape.influence(1.0), 0.0, "{shape:?} ends at rest");
    }
    for shape in [SelectorShape::Triangle, SelectorShape::Round] {
        assert!(shape.influence(0.0).abs() < 1e-12);
        assert!((shape.influence(0.5) - 1.0).abs() < 1e-12);
        assert!(shape.influence(1.0).abs() < 1e-12);
    }
    assert!((SelectorShape::Smooth.influence(0.5) - 0.5).abs() < 1e-12);
    assert_eq!(SelectorShape::Square.influence(0.49), 1.0);
    assert_eq!(SelectorShape::Square.influence(0.51), 0.0);
    // Ramp keeps an easing's overshoot.
    assert!(SelectorShape::Ramp.influence(1.1) < 0.0);
    assert_eq!(
        SelectorShape::parse("ease_out"),
        Some(SelectorShape::EaseOut)
    );
    assert_eq!(SelectorShape::parse("wobble"), None);
}

#[test]
fn unit_rates_follow_their_window_and_hold_outside() {
    let rate = unit_rate(
        SelectorShape::Ramp,
        &RateFunc::Linear,
        (0.25, 0.75),
        0.0,
        1.0,
        false,
    );
    assert!((rate.evaluate(0.0) - 1.0).abs() < 1e-12);
    assert!((rate.evaluate(0.25) - 1.0).abs() < 1e-12);
    assert!((rate.evaluate(0.5) - 0.5).abs() < 1e-9);
    assert!(rate.evaluate(0.75).abs() < 1e-12);
    assert!(rate.evaluate(1.0).abs() < 1e-12);
    // Evaluation is a pure function of progress: seeks repeat exactly.
    assert_eq!(rate.evaluate(0.4).to_bits(), rate.evaluate(0.4).to_bits());

    // A backward range enters the window from its far end.
    let back = unit_rate(
        SelectorShape::Ramp,
        &RateFunc::Linear,
        (0.25, 0.75),
        1.0,
        0.0,
        false,
    );
    assert!(back.evaluate(0.0).abs() < 1e-12);
    assert!((back.evaluate(0.5) - 0.5).abs() < 1e-9);
    assert!((back.evaluate(1.0) - 1.0).abs() < 1e-12);

    // The unit easing shapes the transition inside the window.
    let eased = unit_rate(
        SelectorShape::Ramp,
        &RateFunc::EaseOut(EasingCurve::Cubic),
        (0.0, 1.0),
        0.0,
        1.0,
        false,
    );
    assert!(eased.evaluate(0.5) < 0.2);

    // An exit runs forward from rest to the out state.
    let exit = unit_rate(
        SelectorShape::Ramp,
        &RateFunc::Linear,
        (0.25, 0.75),
        0.0,
        1.0,
        true,
    );
    assert!(exit.evaluate(0.0).abs() < 1e-12);
    assert!((exit.evaluate(0.5) - 0.5).abs() < 1e-9);
    assert!((exit.evaluate(1.0) - 1.0).abs() < 1e-12);

    // A frozen range holds one influence.
    let frozen = unit_rate(
        SelectorShape::Ramp,
        &RateFunc::Linear,
        (0.0, 1.0),
        0.5,
        0.5,
        false,
    );
    assert!((frozen.evaluate(0.0) - 0.5).abs() < 1e-12);
    assert!((frozen.evaluate(1.0) - 0.5).abs() < 1e-12);
}

#[test]
fn spans_fit_the_requested_stagger() {
    // Three units 0.1 s apart in one second: each unit gets 0.8 s.
    assert!((unit_span(1.0, 3, Some(0.1)) - 0.8).abs() < 1e-12);
    assert_eq!(unit_span(1.0, 1, Some(0.1)), 1.0);
    assert_eq!(unit_span(1.0, 40, Some(1.0)), MIN_UNIT_SPAN);
    assert_eq!(unit_span(0.0, 3, Some(0.1)), 1.0);
    let adaptive = unit_span(1.0, 10, None);
    assert!(adaptive > 0.0 && adaptive < 1.0);
}

#[test]
fn random_slots_are_seeded_permutations() {
    let a = unit_slots(12, DrawOrder::Random, 7);
    assert_eq!(a, unit_slots(12, DrawOrder::Random, 7));
    assert_ne!(a, unit_slots(12, DrawOrder::Random, 8));
    let mut sorted = a.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..12).collect::<Vec<_>>());
    assert_eq!(
        unit_slots(12, DrawOrder::Random, 0),
        crate::builder::draw_group_slots(12, DrawOrder::Random, 0)
    );
    assert_eq!(unit_slots(3, DrawOrder::Reverse, 5), [2, 1, 0]);
}

#[test]
fn rows_split_wrapped_lines_and_tracking_spreads_from_the_anchor() {
    let glyph = |x: f64, y: f64| kurbo::Rect::new(x, y, x + 0.4, y + 0.5);
    let boxes = vec![
        glyph(0.0, 0.0),
        glyph(0.5, 0.0),
        glyph(1.0, 0.0),
        // Wrapped onto the next row, below the first.
        glyph(0.0, -0.8),
        glyph(0.5, -0.8),
    ];
    let rows = split_rows(&[vec![0, 1, 2, 3, 4]], &boxes);
    assert_eq!(rows, vec![vec![0, 1, 2], vec![3, 4]]);

    let centered = tracking_factors(&rows, &boxes, 0.5, 5);
    assert_eq!(centered, [-1.0, 0.0, 1.0, -0.5, 0.5]);
    let left = tracking_factors(&rows, &boxes, 0.0, 5);
    assert_eq!(left, [0.0, 1.0, 2.0, 0.0, 1.0]);
}

#[test]
fn a_comma_stays_in_its_row() {
    let glyph = |x: f64, y0: f64, y1: f64| kurbo::Rect::new(x, y0, x + 0.4, y1);
    let boxes = vec![
        glyph(0.0, 0.0, 0.5),
        glyph(0.5, 0.0, 0.5),
        // A comma: low, below the middle of the row, but further along.
        glyph(1.0, -0.1, 0.1),
        glyph(1.5, 0.0, 0.5),
    ];
    let rows = split_rows(&[vec![0, 1, 2, 3]], &boxes);
    assert_eq!(rows, vec![vec![0, 1, 2, 3]]);
    assert_eq!(
        tracking_factors(&rows, &boxes, 0.0, 4),
        [0.0, 1.0, 2.0, 3.0]
    );
}

#[test]
fn masks_stay_fixed_while_the_glyph_moves() {
    let rest = SpatialTransform::new_2d(1.0, 0.0);
    let out = SpatialTransform::new_2d(1.0, -1.0);
    let lens = UnitMaskLens {
        rect: kurbo::Rect::new(0.0, 0.0, 2.0, 1.0),
        rest,
        out,
    };
    assert!(lens.mask_at(0.0).is_none());
    // Halfway the glyph sits 0.5 lower, so in its own space the mask is
    // 0.5 higher; seen from the parent it has not moved.
    let bounds = lens.mask_at(0.5).unwrap().bounding_box();
    assert!((bounds.y0 - 0.5).abs() < 1e-9 && (bounds.y1 - 1.5).abs() < 1e-9);
    assert!((bounds.x0 + 1.0).abs() < 1e-9);
}

fn compiled(canvas: &super::super::SceneModel) -> gaanim_timeline::timeline::Timeline {
    use bevy::ecs::world::CommandQueue;
    use bevy::prelude::Commands;
    let world = World::new();
    let mut queue = CommandQueue::default();
    let mut commands = Commands::new(&mut queue, &world);
    let mut timeline = gaanim_timeline::timeline::Timeline::new();
    let fonts = gaanim_text::font::FontRegistry::new();
    let text_config = gaanim_text::prelude::TextConfig::default();
    canvas.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
    timeline
}

/// `(target, start, lens debug, rate at 0, rate at 1)` of animator clips.
fn animator_clips(
    timeline: &gaanim_timeline::timeline::Timeline,
) -> Vec<(ObjectId, f64, String, f64, f64)> {
    let mut clips: Vec<_> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(spec) if spec.label.as_deref() == Some("TextAnimator") => {
                Some((
                    spec.target,
                    clip.start,
                    format!("{:?}", spec.lens),
                    spec.rate_func.evaluate(0.0),
                    spec.rate_func.evaluate(1.0),
                ))
            }
            _ => None,
        })
        .collect();
    clips.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    clips
}

#[test]
fn backward_seeks_keep_units_hidden_until_their_reveal() {
    use gaanim_scene::MobjectId;
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    for style in [TextRevealStyle::SlideUp, TextRevealStyle::Blur] {
        let mut canvas = super::super::SceneModel::new(640, 360);
        let text = canvas.text("uno dos");
        canvas.wait(1.0);
        canvas.play(vec![
            text.animate()
                .text_reveal(TextRevealUnit::Word, style, true, 0.045)
                .unwrap()
                .duration(0.9),
        ]);
        canvas.wait(0.5);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
        let glyph = animator_clips(&timeline)[0].0;
        let entity = world
            .query::<(Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == glyph).then_some(entity))
            .expect("glyph entity");
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        let state = |world: &World| {
            format!(
                "{:?} {:?} {:?} {:?}",
                world.get::<ClipMask>(entity),
                world.get::<GaussianBlur>(entity),
                world.get::<SpatialTransform>(entity),
                world.get::<Opacity>(entity),
            )
        };

        // Before and at the start of the reveal, as first played.
        let mut fresh = Vec::new();
        for time in [0.5, 1.0] {
            timeline.seek(&mut world, time);
            fresh.push(state(&world));
        }
        // Returning from the revealed text, as slide navigation does.
        for (time, expected) in [0.5, 1.0].into_iter().zip(&fresh) {
            timeline.seek(&mut world, 2.4);
            timeline.seek(&mut world, time);
            assert_eq!(&state(&world), expected, "{style:?} at {time}");
        }
    }
}

#[test]
fn a_delayed_blur_in_keeps_its_glyphs_hidden_during_the_delay() {
    use gaanim_scene::MobjectId;
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    // Declared after a wait, so the text appears at 1 s; the sweep
    // starts at 3 s.
    let mut canvas = super::super::SceneModel::new(640, 360);
    canvas.wait(1.0);
    let text = canvas.text("GETTER");
    canvas.play(vec![
        text.animate()
            .blur_in(0.3, TextRevealUnit::Grapheme, 0.1)
            .unwrap()
            .duration(1.0)
            .delay(2.0),
    ]);
    canvas.wait(0.5);

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    canvas.compile(&mut world);
    world.flush();
    let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
    let glyph = animator_clips(&timeline)[0].0;
    let entity = world
        .query::<(Entity, &MobjectId)>()
        .iter(&world)
        .find_map(|(entity, id)| (id.0 == glyph).then_some(entity))
        .expect("glyph entity");
    timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
    let opacity = |world: &World| world.get::<Opacity>(entity).map(|opacity| opacity.0);
    let blur = |world: &World| world.get::<GaussianBlur>(entity).map(|blur| blur.sigma);

    // In the delay, from the start and back from the end.
    for from in [0.0, 4.4] {
        timeline.seek(&mut world, from);
        timeline.seek(&mut world, 2.0);
        assert_eq!(opacity(&world), Some(0.0), "seek from {from}");
        assert_eq!(blur(&world), Some(0.3), "seek from {from}");
    }
    // Once revealed, the glyph is sharp and opaque.
    timeline.seek(&mut world, 4.4);
    assert_eq!(opacity(&world), Some(1.0));
}

#[test]
fn segment_checkpoints_keep_reveals_hidden_or_shown_like_a_seek_from_the_start() {
    use gaanim_scene::MobjectId;
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    // Slides whose title a reveal brings in after a short lead, like a
    // section divider staggering its elements.
    let mut canvas = super::super::SceneModel::new(640, 360);
    for index in 0..4 {
        canvas.segment(format!("Slide {index}"), None).unwrap();
        let rule = canvas.square(0.4);
        let title = canvas.text(&format!("Slide {index} title"));
        canvas.play(vec![rule.animate().fade_in().duration(0.3)]);
        canvas.play(vec![
            title
                .animate()
                .text_reveal(TextRevealUnit::Word, TextRevealStyle::SlideUp, true, 0.06)
                .unwrap()
                .duration(0.7),
        ]);
        canvas.stop(Some(format!("end {index}"))).unwrap();
    }
    let compiled = |checkpoints: bool| {
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
        timeline.segment_checkpoints = checkpoints;
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        (world, timeline)
    };
    let state = |world: &mut World| {
        let mut glyphs = world
            .query::<(&MobjectId, Option<&ClipMask>, &SpatialTransform)>()
            .iter(world)
            .map(|(id, mask, transform)| format!("{:?} {mask:?} {transform:?}", id.0))
            .collect::<Vec<_>>();
        glyphs.sort();
        glyphs
    };
    let (mut world, mut timeline) = compiled(true);
    let (mut reference_world, mut reference) = compiled(false);
    let starts: Vec<f64> = timeline.segments.iter().map(|s| s.start_time).collect();
    let end = timeline.cached_duration;
    // Play every slide, then jump around as slide navigation does: back
    // to a slide already shown, forward over reveals, and back again.
    let mut path: Vec<f64> = (0..=40).map(|step| end * f64::from(step) / 40.0).collect();
    for &start in [starts[2], starts[1], starts[3], starts[1], starts[2]].iter() {
        path.extend([start, start + 0.05, start + 0.2, start + 0.35, end]);
    }
    for time in path {
        timeline.seek(&mut world, time);
        reference.seek(&mut reference_world, time);
        assert_eq!(
            state(&mut world),
            state(&mut reference_world),
            "state at {time:.3}s differs from a seek without checkpoints"
        );
    }
}

#[test]
fn line_reveal_gives_every_glyph_its_own_line() {
    let mut canvas = super::super::SceneModel::new(640, 360);
    let text = canvas.text("para la\nautomatizacion\nconfinada");
    canvas.play(vec![
        text.animate()
            .text_reveal(TextRevealUnit::Line, TextRevealStyle::SlideUp, true, 0.3)
            .unwrap()
            .duration(1.5),
    ]);
    let timeline = compiled(&canvas);
    // Influence of every glyph halfway through, in glyph order.
    let mut influence: Vec<(ObjectId, f64)> = timeline
        .clips
        .values()
        .filter_map(|clip| match &clip.payload {
            ClipPayload::Animation(spec)
                if matches!(spec.lens, PropertyLensSpec::Translation { .. }) =>
            {
                Some((spec.target, spec.rate_func.evaluate(0.35)))
            }
            _ => None,
        })
        .collect();
    influence.sort_by_key(|(id, _)| *id);
    // "confinada" draws fi as one ligature glyph.
    assert_eq!(influence.len(), 28, "{influence:?}");
    let lines: Vec<Vec<f64>> = [0..6, 6..20, 20..28]
        .into_iter()
        .map(|range| influence[range].iter().map(|(_, value)| *value).collect())
        .collect();
    for line in &lines {
        assert!(line.iter().all(|value| value == &line[0]), "{lines:?}");
    }
    assert!(
        lines[0][0] < lines[1][0] && lines[1][0] <= lines[2][0],
        "{lines:?}"
    );
}

#[test]
fn line_reveal_masks_each_line_and_lands_at_rest() {
    let mut canvas = super::super::SceneModel::new(640, 360);
    let text = canvas.text("uno dos\ntres");
    canvas.play(vec![
        text.animate()
            .text_reveal(TextRevealUnit::Line, TextRevealStyle::SlideUp, true, 0.2)
            .unwrap()
            .duration(1.0),
    ]);
    let clips = animator_clips(&compiled(&canvas));
    let translations: Vec<_> = clips
        .iter()
        .filter(|clip| clip.2.starts_with("Translation"))
        .collect();
    let masks: Vec<_> = clips
        .iter()
        .filter(|clip| clip.2.contains("UnitMaskLens"))
        .collect();
    // One translation and one mask per visible glyph ("unodostres").
    assert_eq!(translations.len(), 10);
    assert_eq!(masks.len(), 10);
    // Every glyph starts out of place and ends at rest.
    assert!(
        translations
            .iter()
            .all(|clip| (clip.3 - 1.0).abs() < 1e-9 && clip.4.abs() < 1e-9)
    );
    assert!(clips.iter().all(|clip| clip.2.find("Opacity").is_none()));
}

#[test]
fn staggered_units_enter_in_order_and_conceal_leaves_them_out() {
    let influence_at = |anim: fn(Anim) -> Anim, t: f64| {
        let mut canvas = super::super::SceneModel::new(640, 360);
        let text = canvas.text("ab cd");
        canvas.play(vec![anim(text.animate()).duration(1.0)]);
        let timeline = compiled(&canvas);
        let mut opacity: Vec<(ObjectId, f64, f64)> = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(spec)
                    if matches!(spec.lens, PropertyLensSpec::Opacity { .. }) =>
                {
                    Some((
                        spec.target,
                        spec.rate_func.evaluate(t),
                        spec.rate_func.evaluate(1.0),
                    ))
                }
                _ => None,
            })
            .collect();
        opacity.sort_by_key(|clip| clip.0);
        opacity
    };
    let reveal = influence_at(
        |anim| {
            anim.text_reveal(TextRevealUnit::Word, TextRevealStyle::Fade, true, 0.3)
                .unwrap()
        },
        0.5,
    );
    assert_eq!(reveal.len(), 4);
    // Halfway, the first word is further along (less out) than the second.
    assert!(reveal[0].1 < reveal[2].1);
    assert_eq!(reveal[0].1, reveal[1].1);
    assert!(reveal.iter().all(|clip| clip.2.abs() < 1e-9));

    let conceal = influence_at(
        |anim| {
            anim.text_conceal(TextRevealUnit::Word, TextRevealStyle::Fade, true, 0.3)
                .unwrap()
        },
        0.5,
    );
    // The first word leaves first, like the reveal's order, and every
    // glyph ends hidden.
    assert!(conceal[0].1 > conceal[2].1);
    assert!(conceal.iter().all(|clip| (clip.2 - 1.0).abs() < 1e-9));
}

#[test]
fn slide_conceal_keeps_moving_the_way_the_reveal_moved() {
    // Vertical travel (`to.y - from.y`) of every glyph translation.
    let travel = |anim: fn(Anim) -> Anim| {
        let mut canvas = super::super::SceneModel::new(640, 360);
        let text = canvas.text("uno\ndos");
        canvas.play(vec![anim(text.animate()).duration(1.0)]);
        animator_clips(&compiled(&canvas))
            .into_iter()
            .filter(|clip| clip.2.starts_with("Translation"))
            .filter_map(|clip| {
                let y = |value: &str| value.split(',').nth(1)?.trim().parse::<f64>().ok();
                let from = clip.2.split("from: DVec3(").nth(1)?;
                let to = clip.2.split("to: DVec3(").nth(1)?;
                Some((y(to)? - y(from)?, clip.3, clip.4))
            })
            .collect::<Vec<_>>()
    };
    let reveal = travel(|anim| {
        anim.text_reveal(TextRevealUnit::Line, TextRevealStyle::SlideUp, true, 0.2)
            .unwrap()
    });
    let conceal = travel(|anim| {
        anim.text_conceal(TextRevealUnit::Line, TextRevealStyle::SlideUp, true, 0.2)
            .unwrap()
    });
    assert_eq!(reveal.len(), 6);
    assert_eq!(conceal.len(), 6);
    // The reveal rises from below (its out state is lower)...
    assert!(reveal.iter().all(|clip| clip.0 < 0.0 && clip.1 > 0.99));
    // ...and the conceal starts at rest and leaves upward.
    assert!(
        conceal
            .iter()
            .all(|clip| clip.0 > 0.0 && clip.1.abs() < 1e-9 && (clip.2 - 1.0).abs() < 1e-9)
    );
    let down = travel(|anim| {
        anim.text_conceal(TextRevealUnit::Line, TextRevealStyle::SlideDown, true, 0.2)
            .unwrap()
    });
    assert!(down.iter().all(|clip| clip.0 < 0.0));
}

#[test]
fn tracking_is_absolute_and_spreads_around_the_anchor() {
    let mut canvas = super::super::SceneModel::new(640, 360);
    let text = canvas.text("abc");
    let text = text.tracking(0.4).unwrap();
    canvas.play(vec![text.animate().tracking(0.0).unwrap().duration(1.0)]);
    let clips = animator_clips(&compiled(&canvas));
    let shifts: Vec<f64> = clips
        .iter()
        .filter(|clip| clip.2.starts_with("Translation"))
        .filter_map(|clip| {
            let from = clip.2.split("from: DVec3(").nth(1)?;
            let to = clip.2.split("to: DVec3(").nth(1)?;
            let x = |value: &str| value.split(',').next()?.trim().parse::<f64>().ok();
            Some(x(to)? - x(from)?)
        })
        .collect();
    // Left-aligned: the first glyph stays and the others move one and
    // two gaps when tracking opens, then move back when it closes.
    let mut opened: Vec<f64> = shifts.iter().copied().filter(|s| *s > 0.0).collect();
    let mut closed: Vec<f64> = shifts.iter().map(|s| -s).filter(|s| *s > 0.0).collect();
    opened.sort_by(f64::total_cmp);
    closed.sort_by(f64::total_cmp);
    assert_eq!(opened.len(), 2, "{shifts:?}");
    assert!((opened[1] - 2.0 * opened[0]).abs() < 1e-9, "{shifts:?}");
    assert_eq!(opened.len(), closed.len());
    assert!(
        opened
            .iter()
            .zip(&closed)
            .all(|(a, b)| (a - b).abs() < 1e-9)
    );
}

#[test]
fn animator_rejects_non_text_targets() {
    let mut canvas = super::super::SceneModel::new(640, 360);
    let circle = canvas.circle(1.0);
    assert_eq!(
        circle
            .animator(
                TextRevealUnit::Grapheme,
                SelectorShape::Smooth,
                DrawOrder::Forward,
                0
            )
            .unwrap_err(),
        TextAnimatorError::NotText("animator")
    );
    assert!(
        circle
            .animate()
            .blur_in(0.3, TextRevealUnit::Grapheme, 0.02)
            .is_err()
    );
}

#[test]
fn reveal_styles_describe_their_out_state() {
    let masked = TextRevealStyle::SlideUp.out_state(true);
    assert_eq!(masked.relative_offset, DVec2::new(0.0, -1.0));
    assert_eq!(masked.opacity, None);
    let unmasked = TextRevealStyle::SlideDown.out_state(false);
    assert!(unmasked.relative_offset.y > 0.0);
    assert_eq!(unmasked.opacity, Some(0.0));
    assert!(TextRevealStyle::Blur.out_state(true).blur.is_some());
    assert!(!TextRevealStyle::Fade.masks());
    assert_eq!(
        TextRevealStyle::parse("slide_up"),
        Some(TextRevealStyle::SlideUp)
    );
    assert_eq!(TextRevealStyle::parse("wipe"), None);
}
