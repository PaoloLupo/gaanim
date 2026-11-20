use super::*;
use crate::canvas::{LayoutMemberSpec, LayoutSpec, LayoutWithin};
use gaanim_text::prelude::TextRole;

#[test]
fn replay_applies_the_paper_text_theme() {
    let mut canvas = SceneModel::new(640, 360);
    canvas
        .set_theme("paper")
        .expect("paper is a built-in theme");

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    replay_canvas_into(&mut world, canvas);

    assert_eq!(
        world.resource::<gaanim_text::prelude::TextConfig>().roles[&TextRole::Body].fill_color,
        gaanim_core::peniko::Color::BLACK
    );
}

#[test]
fn replay_exposes_authored_audio_to_preview() {
    let path =
        std::env::temp_dir().join(format!("gaanim-preview-audio-{}.wav", std::process::id()));
    std::fs::write(&path, b"fixture").unwrap();
    let mut canvas = SceneModel::new(640, 360);
    canvas.wait(1.5);
    let audio = canvas.audio(&path, Some(2.0), 0.5, 0.1, 0.2).unwrap();
    canvas.play_items(vec![audio.into()]).unwrap();

    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    replay_canvas_into(&mut world, canvas);
    world.flush();

    let tracks = &world.resource::<gaanim_media::PreviewAudioTracks>().0;
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].start_time, 1.5);
    assert_eq!(tracks[0].volume, 0.5);
    let _ = std::fs::remove_file(path);
}

#[test]
fn invalid_typst_math_in_layout_does_not_remove_runtime_resources() {
    let mut canvas = SceneModel::new(640, 360);
    let equation = canvas.text("$integral alpha dt + 2 = 0$");
    let column = canvas.group(&[&equation]);
    equation.claim_layout(&column).unwrap();
    canvas.reflow_layout(
        &column,
        vec![LayoutMemberSpec {
            id: equation.id,
            style: gaanim_layout::LayoutItemStyle::default(),
        }],
        LayoutSpec {
            kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
            style: gaanim_layout::LayoutStyle {
                width: gaanim_layout::SizeRule::Fill(1.0),
                align: gaanim_layout::Align::Start,
                ..Default::default()
            },
            within: LayoutWithin::Safe,
        },
        1,
        None,
        None,
        None,
    );
    let diagnostics = canvas.clone();
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());

    replay_canvas_into(&mut world, canvas);

    assert!(world.contains_resource::<Timeline>());
    assert!(world.contains_resource::<gaanim_text::font::FontRegistry>());
    assert!(world.contains_resource::<gaanim_text::prelude::TextConfig>());
    assert!(
        diagnostics
            .check_layout()
            .iter()
            .any(|message| message.contains("unknown variable: dt")),
        "expected the Typst failure to remain available as a layout diagnostic"
    );
}

#[test]
fn camera_stack_clears_full_target_before_vello() {
    let canvas = SceneModel::new(640, 360);
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    world.spawn((
        Camera2d,
        VelloView,
        bevy::prelude::Camera {
            clear_color: ClearColorConfig::Default,
            ..default()
        },
    ));

    replay_canvas_into(&mut world, canvas);
    world.flush();

    let mut clear_query =
        world.query_filtered::<&bevy::prelude::Camera, With<GaanimFullWindowClearCamera>>();
    let clear = clear_query
        .single(&world)
        .expect("full-window clear camera");
    assert_eq!(clear.order, -1);
    assert!(matches!(clear.clear_color, ClearColorConfig::Default));
    assert!(clear.viewport.is_none());

    let mut three_d = world.query_filtered::<Entity, With<Camera3d>>();
    assert_eq!(
        three_d.iter(&world).count(),
        0,
        "3D content draws with Vello"
    );

    let mut vello_query =
        world.query_filtered::<&bevy::prelude::Camera, (With<Camera2d>, With<VelloView>)>();
    let vello = vello_query.single(&world).expect("Vello camera");
    assert_eq!(vello.order, 1);
    assert!(matches!(vello.clear_color, ClearColorConfig::None));
}

/// One authored change to the incremental test deck.
#[derive(Clone, Copy)]
enum Edit {
    None,
    /// Replace the body text of a slide.
    Retitle(usize),
    /// Add an extra object to a slide, shifting every later ObjectId.
    Insert(usize),
    /// Group the persistent logo from a slide, reparenting a kept entity.
    Adopt(usize),
}

fn incremental_deck(slides: usize, edits: &[Edit]) -> SceneModel {
    let mut canvas = SceneModel::new(640, 360);
    let logo = canvas.circle(0.3).move_to(2.5, 1.2);
    canvas.persist(&logo).unwrap();
    for index in 0..slides {
        canvas.segment(format!("Slide {index}"), None).unwrap();
        let retitled = edits
            .iter()
            .filter(|edit| matches!(edit, Edit::Retitle(slide) if *slide == index))
            .count();
        let title = canvas.text(&format!("Slide {index} revision {retitled}"));
        let equation = canvas.text(&format!("$x^{index} + {retitled} = y$"));
        let marker = canvas.square(0.4).move_to(-2.0, -1.0);
        let group = canvas.group(&[&equation, &marker]);
        for edit in edits {
            if matches!(edit, Edit::Insert(slide) if *slide == index) {
                let extra = canvas.text("inserted");
                canvas.play(vec![extra.animate().fade_in().duration(0.25)]);
            }
            if matches!(edit, Edit::Adopt(slide) if *slide == index) {
                let badge = canvas.text("logo");
                let adopted = canvas.group(&[&logo, &badge]);
                canvas.play(vec![adopted.animate().shift_by(0.0, -0.5).duration(0.25)]);
            }
        }
        canvas.play(vec![title.animate().fade_in().duration(0.5)]);
        canvas.play(vec![group.animate().shift_by(0.5, 0.0).duration(0.5)]);
        canvas.play(vec![logo.animate().rotate_by(0.2).duration(0.25)]);
        canvas.stop(Some(format!("end {index}"))).unwrap();
    }
    canvas
}

fn incremental_world() -> World {
    let mut world = World::new();
    world.insert_resource(Timeline::new());
    world.insert_resource(gaanim_text::font::FontRegistry::new());
    world.insert_resource(gaanim_text::prelude::TextConfig::default());
    world.insert_resource(gaanim_renderer::pipeline::GaanimRenderCache::default());
    world
}

/// The editor's scene clear, reduced to what these tests observe.
fn clear_scene(world: &mut World) {
    world.remove_resource::<RetainedReplay>();
    let entities: Vec<Entity> = world
        .query_filtered::<Entity, SceneOwned>()
        .iter(world)
        .collect();
    for entity in entities {
        if world.get_entity(entity).is_ok() {
            world.despawn(entity);
        }
    }
    world
        .resource_mut::<gaanim_renderer::pipeline::GaanimRenderCache>()
        .fragment_cache
        .clear();
    *world.resource_mut::<Timeline>() = Timeline::default();
}

fn hot_reload(world: &mut World, canvas: SceneModel) -> ReplayKind {
    let kind = replay_canvas_incremental(world, canvas, true, clear_scene);
    world.flush();
    gaanim_timeline::capture_reload_keyframe(world);
    kind
}

fn seek(world: &mut World, time: f64) {
    let mut timeline = world.remove_resource::<Timeline>().unwrap();
    timeline.seek(world, time);
    world.insert_resource(timeline);
}

/// Observable state at several times, plus the compiled timeline shape.
fn observe(world: &mut World) -> Vec<String> {
    let (duration, segments, clips) = {
        let timeline = world.resource::<Timeline>();
        (
            timeline.cached_duration,
            timeline.segments.clone(),
            timeline.clips.len(),
        )
    };
    let mut observed = vec![format!("{duration} {clips} {segments:?}")];
    for step in 0..=12 {
        let time = duration * f64::from(step) / 12.0;
        seek(world, time);
        let snapshot = gaanim_timeline::snapshot::WorldSnapshot::capture(world);
        let mut entities = snapshot.entities.into_iter().collect::<Vec<_>>();
        entities.sort_by_key(|(id, _)| id.as_raw());
        observed.push(format!("{time}: {entities:?} {:?}", snapshot.camera));
    }
    observed
}

fn full_replay_observation(canvas: SceneModel) -> Vec<String> {
    let mut world = incremental_world();
    replay_canvas_into(&mut world, canvas);
    world.flush();
    gaanim_timeline::capture_reload_keyframe(&mut world);
    observe(&mut world)
}

fn assert_matches_full_replay(world: &mut World, edits: &[Edit], slides: usize) {
    let incremental = observe(world);
    let full = full_replay_observation(incremental_deck(slides, edits));
    assert_eq!(incremental.len(), full.len());
    for (incremental, full) in incremental.iter().zip(&full) {
        assert_eq!(incremental, full);
    }
}

#[test]
fn a_reused_revision_takes_the_notes_of_the_new_script() {
    let root = tempfile::tempdir().unwrap();
    let script = root.path().join("guion.md");
    let deck = |notes: &str| {
        std::fs::write(&script, notes).unwrap();
        let mut canvas = SceneModel::new(640, 360);
        canvas.narration_script(&script).unwrap();
        canvas.segment("Portada", None).unwrap();
        canvas.wait(1.0);
        canvas
    };
    let mut world = incremental_world();
    assert_eq!(
        hot_reload(
            &mut world,
            deck(
                "## Portada
Hola.
"
            )
        ),
        ReplayKind::Full
    );
    assert_eq!(
        hot_reload(
            &mut world,
            deck(
                "## portada
Buenos días.
"
            )
        ),
        ReplayKind::Unchanged { segments: 1 }
    );
    assert_eq!(
        world.resource::<Timeline>().segments[0].notes.as_deref(),
        Some("Buenos días.")
    );
}

#[test]
fn identical_revisions_have_identical_fingerprints() {
    let fonts = gaanim_text::font::FontRegistry::new();
    let text = gaanim_text::prelude::TextConfig::default();
    let first = incremental_deck(4, &[]).fingerprints(&text, &fonts);
    let second = incremental_deck(4, &[]).fingerprints(&text, &fonts);
    assert_eq!(first, second);
    // The persistent logo opens an implicit segment before the slides.
    assert_eq!(first.shared_prefix(&second), 5);
    let edited = incremental_deck(4, &[Edit::Retitle(2)]).fingerprints(&text, &fonts);
    assert_eq!(first.shared_prefix(&edited), 3);
}

#[test]
fn incremental_reload_matches_a_full_replay_after_each_edit() {
    let slides = 5;
    // Slide `n` is segment `n + 1`, after the implicit logo segment.
    let segments = slides + 1;
    let mut world = incremental_world();
    let mut edits = vec![Edit::None];
    assert_eq!(
        hot_reload(&mut world, incremental_deck(slides, &edits)),
        ReplayKind::Full
    );
    // The first edit locates the checkpoint; later edits there reuse it.
    edits.push(Edit::Retitle(3));
    assert_eq!(
        hot_reload(&mut world, incremental_deck(slides, &edits)),
        ReplayKind::Full
    );
    assert_matches_full_replay(&mut world, &edits, slides);

    let reused = |reused| ReplayKind::Incremental { reused, segments };
    for (edit, kind) in [
        (Edit::Retitle(3), reused(4)),
        (Edit::Insert(3), reused(4)),
        (Edit::Retitle(4), reused(4)),
        (Edit::Retitle(4), reused(5)),
        // The same deck again: the world already holds it.
        (Edit::None, ReplayKind::Unchanged { segments }),
        (Edit::None, ReplayKind::Unchanged { segments }),
        (Edit::Retitle(4), reused(5)),
    ] {
        // Playback changes kept entities before the next reload.
        seek(&mut world, 1.3);
        edits.push(edit);
        assert_eq!(
            hot_reload(&mut world, incremental_deck(slides, &edits)),
            kind
        );
        assert_matches_full_replay(&mut world, &edits, slides);
    }

    // An edit before the checkpoint needs every segment again.
    edits.push(Edit::Retitle(1));
    assert_eq!(
        hot_reload(&mut world, incremental_deck(slides, &edits)),
        ReplayKind::Full
    );
    assert_matches_full_replay(&mut world, &edits, slides);
    edits.push(Edit::Insert(2));
    assert_eq!(
        hot_reload(&mut world, incremental_deck(slides, &edits)),
        ReplayKind::Incremental {
            reused: 2,
            segments
        }
    );
    assert_matches_full_replay(&mut world, &edits, slides);
}

#[test]
fn later_segments_that_modify_kept_entities_still_match_a_full_replay() {
    let slides = 4;
    let mut world = incremental_world();
    let mut edits = vec![Edit::None];
    hot_reload(&mut world, incremental_deck(slides, &edits));
    edits.push(Edit::Retitle(2));
    hot_reload(&mut world, incremental_deck(slides, &edits));
    // Slide 3 reparents the logo that the implicit first segment spawned.
    edits.push(Edit::Adopt(3));
    seek(&mut world, 1.0);
    let kind = hot_reload(&mut world, incremental_deck(slides, &edits));
    assert_matches_full_replay(&mut world, &edits, slides);
    // Undoing the adoption must also restore the logo exactly.
    edits.retain(|edit| !matches!(edit, Edit::Adopt(_)));
    edits.push(Edit::Retitle(3));
    seek(&mut world, 1.0);
    hot_reload(&mut world, incremental_deck(slides, &edits));
    assert_matches_full_replay(&mut world, &edits, slides);
    assert_eq!(
        kind,
        ReplayKind::Full,
        "reparenting a kept entity must fall back to a full replay"
    );
}

#[test]
fn a_reload_probes_camera_holds_again_only_to_recompile_camera_moves_on_endpoints() {
    use crate::canvas::CanvasEndpoint;
    use gaanim_animation::FollowOffsetSpace;
    use gaanim_core::glam::DVec3;
    // A follow of a drawable an updater moves, then a caption, and
    // perhaps a pan to it.
    fn scene(caption: &str, pan: bool) -> SceneModel {
        let mut canvas = SceneModel::new(640, 360);
        canvas.segment("follow", None).unwrap();
        let comet = canvas.circle(0.3);
        comet.add_updater(crate::canvas::UpdaterPreset::Orbit {
            cx: 0.0,
            cy: 0.0,
            radius: 2.0,
            speed: 1.0,
        });
        let follow = canvas.camera_follow_endpoint(
            CanvasEndpoint::Entity(comet.id),
            DVec3::ZERO,
            FollowOffsetSpace::World,
            0.0,
            1.0,
        );
        canvas.play(vec![follow]);
        canvas.segment("caption", None).unwrap();
        let caption = canvas.text(caption);
        canvas.play(vec![caption.animate().fade_in().duration(0.5)]);
        if pan {
            let pan = canvas.camera_pan_to_endpoint(CanvasEndpoint::Entity(comet.id), 1.0);
            canvas.play(vec![pan]);
        }
        canvas
    }
    let probes = |world: &World| Arc::clone(&world.resource::<ProbedCameraHolds>().probes);
    let mut world = incremental_world();
    hot_reload(&mut world, scene("first", false));
    let found = probes(&world);
    assert!(!found.is_empty(), "the follow's hold is probed");
    // The edit recompiles no camera move: the follow ends where it did.
    hot_reload(&mut world, scene("second", false));
    assert!(Arc::ptr_eq(&found, &probes(&world)));
    hot_reload(&mut world, scene("second", false));
    assert!(Arc::ptr_eq(&found, &probes(&world)));
    assert_eq!(
        observe(&mut world),
        full_replay_observation(scene("second", false))
    );
    // The edit recompiles a pan to the comet, which the probe must find.
    hot_reload(&mut world, scene("second", true));
    assert!(probes(&world).len() > found.len());
    assert_eq!(
        observe(&mut world),
        full_replay_observation(scene("second", true))
    );
}

/// A themed, branded deck with transitions, layouts, editorial parts,
/// camera moves and stops, like a presentation authored with Sections.
fn thesis_deck(slides: usize, edits: &[Edit]) -> SceneModel {
    let mut canvas = SceneModel::new(1920, 1080);
    canvas.set_theme("presentation").unwrap();
    canvas.branding = Some(crate::canvas::PresentationBrand {
        footer: Some("THESIS".into()),
        ..Default::default()
    });
    for index in 0..slides {
        let transition = (index > 0)
            .then_some(gaanim_timeline::transition::TransitionType::CrossFade { duration: 0.3 });
        canvas
            .segment(format!("Slide {index}"), transition)
            .unwrap();
        let revision = edits
            .iter()
            .filter(|edit| matches!(edit, Edit::Retitle(slide) if *slide == index))
            .count();
        let title = canvas.text(&format!("Result {index}, revision {revision}"));
        let body = canvas.text("Error drops by $23 %$ against the baseline.");
        let column = canvas.group(&[&title, &body]);
        title.claim_layout(&column).unwrap();
        body.claim_layout(&column).unwrap();
        canvas.reflow_layout(
            &column,
            [&title, &body]
                .into_iter()
                .map(|member| LayoutMemberSpec {
                    id: member.id,
                    style: gaanim_layout::LayoutItemStyle::default(),
                })
                .collect(),
            LayoutSpec {
                kind: gaanim_layout::LayoutNodeKind::Column { wrap: false },
                style: gaanim_layout::LayoutStyle {
                    width: gaanim_layout::SizeRule::Fill(1.0),
                    align: gaanim_layout::Align::Start,
                    ..Default::default()
                },
                within: LayoutWithin::Safe,
            },
            1,
            None,
            None,
            None,
        );
        let chip = canvas
            .chip(crate::canvas::ChipSpec::new(format!("Step {index}")))
            .unwrap();
        let mut card = crate::canvas::CardSpec::new(format!("Finding {index}"));
        card.body = Some("Stable across seeds.".into());
        let card = canvas.card(card).unwrap();
        canvas.play(vec![column.animate().fade_in().duration(0.4)]);
        let zoom = canvas.camera_zoom_to(1.0 + 0.05 * index as f64, 0.3);
        canvas.play(vec![zoom]);
        canvas.stop(Some(format!("claim {index}"))).unwrap();
        canvas.play(vec![
            chip.animate().fade_in().duration(0.2),
            card.animate().fade_in().duration(0.2),
        ]);
        canvas.stop(Some(format!("evidence {index}"))).unwrap();
    }
    canvas
}

/// Slides whose objects appear with draw animations (write/create) in a
/// stagger, like a card of equations, behind cross-fade transitions.
fn drawn_cards_deck(slides: usize, revision: usize) -> SceneModel {
    let mut canvas = SceneModel::new(1920, 1080);
    for index in 0..slides {
        let transition = (index > 0)
            .then_some(gaanim_timeline::transition::TransitionType::CrossFade { duration: 0.45 });
        canvas
            .segment(format!("Cards {index}"), transition)
            .unwrap();
        // Only the last slide changes between revisions.
        let revision = if index + 1 == slides { revision } else { 0 };
        let title = canvas.text(&format!("Card {index} revision {revision}"));
        canvas.play(vec![title.animate().fade_in().duration(0.3)]);
        for card in 0..3 {
            let x = -5.0 + 5.0 * card as f64;
            let frame = canvas.square(3.0).move_to(x, 0.0);
            let equation = canvas
                .text(&format!("$V_{card} <= 0.55 V_m$"))
                .move_to(x, 0.5);
            let divider = canvas.line(x - 1.2, -0.5, x + 1.2, -0.5);
            let value = canvas.text(&format!("{card}.00 tonf")).move_to(x, -1.0);
            let plan = crate::canvas::Composition::stagger(
                vec![
                    crate::canvas::Composition::leaf(frame.fade_in(0.3)),
                    crate::canvas::Composition::leaf(equation.write(0.6)),
                    crate::canvas::Composition::leaf(divider.create(0.3)),
                    crate::canvas::Composition::leaf(value.fade_in(0.3)),
                ],
                0.12,
            )
            .unwrap();
            canvas
                .play_composition_configured(plan, None, None)
                .unwrap();
        }
        canvas.stop(Some(format!("end {index}"))).unwrap();
    }
    canvas
}

/// Every entity's observable state at `time`, sorted by ObjectId.
fn state_at(world: &mut World, time: f64) -> String {
    seek(world, time);
    let snapshot = gaanim_timeline::snapshot::WorldSnapshot::capture(world);
    let mut entities = snapshot.entities.into_iter().collect::<Vec<_>>();
    entities.sort_by_key(|(id, _)| id.as_raw());
    format!("{entities:?}")
}

/// Real-time playback at 60 fps from `from` to `to`, as the editor ticks it.
fn play(world: &mut World, from: f64, to: f64) {
    const DT: f64 = 1.0 / 60.0;
    world.insert_resource(gaanim_animation::PlaybackState {
        is_playing: true,
        scaled_dt: DT,
        current_time: from,
        updaters_current: false,
    });
    let mut time = from;
    while time < to {
        time = (time + DT).min(to);
        seek(world, time);
        world
            .resource_mut::<gaanim_animation::PlaybackState>()
            .current_time = time;
    }
    world.insert_resource(gaanim_animation::PlaybackState {
        is_playing: false,
        scaled_dt: 0.0,
        current_time: to,
        updaters_current: false,
    });
}

/// The state at each time must not depend on what was shown before it.
fn assert_seek_history_independent(world: &mut World) {
    let duration = world.resource::<Timeline>().cached_duration;
    let steps = 60;
    let times: Vec<f64> = (0..=steps)
        .map(|step| duration * f64::from(step) / f64::from(steps))
        .collect();
    // Reference: every time reached by one jump from the end.
    let reference: Vec<String> = times
        .iter()
        .map(|&time| {
            state_at(world, duration);
            state_at(world, time)
        })
        .collect();
    // Real-time playback from the start must show the same frames.
    state_at(world, 0.0);
    let mut previous = 0.0;
    for (index, &time) in times.iter().enumerate() {
        play(world, previous, time);
        previous = time;
        assert_eq!(
            state_at(world, time),
            reference[index],
            "state at {time:.3}s during playback differs from a direct seek"
        );
    }
}

/// A world that renders every update with the retained fragment cache.
fn render_app() -> App {
    let mut app = App::new();
    app.insert_resource(Timeline::new())
        .insert_resource(gaanim_text::font::FontRegistry::new())
        .insert_resource(gaanim_text::prelude::TextConfig::default())
        .init_resource::<gaanim_renderer::pipeline::GaanimRenderCache>()
        .add_systems(
            Update,
            (
                gaanim_scene::transform_propagation_system,
                gaanim_renderer::pipeline::gaanim_render_system,
            )
                .chain(),
        );
    app
}

/// Render the frame at `time` and compare the retained scene with a
/// scene compiled from scratch.
fn assert_retained_frame_matches(app: &mut App, time: f64, label: &str) {
    seek(app.world_mut(), time);
    app.world_mut()
        .resource_mut::<gaanim_animation::PlaybackState>()
        .current_time = time;
    app.update();
    let fresh = gaanim_renderer::pipeline::compile_scene_from_world(app.world_mut(), None);
    let live = app
        .world_mut()
        .query::<&gaanim_renderer::prelude::VelloScene2d>()
        .single(app.world())
        .unwrap();
    assert!(
        live.encoding().path_data == fresh.encoding().path_data
            && live.encoding().draw_data == fresh.encoding().draw_data,
        "{label}: retained frame at {time:.3}s differs from a fresh compile"
    );
}

#[test]
fn retained_fragments_match_a_fresh_compile_during_playback() {
    let mut app = render_app();
    app.insert_resource(gaanim_animation::PlaybackState {
        is_playing: true,
        scaled_dt: 1.0 / 60.0,
        current_time: 0.0,
        updaters_current: false,
    });
    hot_reload(app.world_mut(), drawn_cards_deck(3, 0));
    let duration = app.world().resource::<Timeline>().cached_duration;
    let frames = (duration * 60.0).ceil() as usize;
    for pass in ["first playback", "replay after rewind"] {
        for frame in 0..=frames {
            assert_retained_frame_matches(&mut app, frame as f64 / 60.0, pass);
        }
    }
    // Scrub backwards across the cards, then play again after a reload.
    for step in (0..=20).rev() {
        assert_retained_frame_matches(&mut app, duration * f64::from(step) / 20.0, "scrub");
    }
    hot_reload(app.world_mut(), drawn_cards_deck(3, 1));
    for frame in 0..=frames {
        assert_retained_frame_matches(&mut app, frame as f64 / 60.0, "after reload");
    }
}

/// A leaderboard reordered by a keyed magic move, then carried into a
/// second segment by a keyed magic-move transition.
fn magic_move_deck() -> SceneModel {
    use crate::canvas::{DrawableHandle, MagicMoveKey, MagicMoveUnmatched};
    let mut canvas = SceneModel::new(640, 360);
    let board = |canvas: &mut SceneModel, rows: &[(&str, f64)]| {
        let bars: Vec<DrawableHandle> = rows
            .iter()
            .enumerate()
            .map(|(rank, (name, score))| {
                canvas
                    .rect(*score, 0.6)
                    .move_to(score / 2.0 - 3.0, 1.5 - rank as f64)
                    .named(*name)
            })
            .collect();
        canvas.group(&bars.iter().collect::<Vec<_>>())
    };
    canvas.segment("before", None).unwrap();
    let before = board(&mut canvas, &[("ana", 4.0), ("bo", 3.0), ("cy", 2.0)]);
    canvas.wait(0.5);
    let after = board(&mut canvas, &[("bo", 5.0), ("ana", 3.5), ("dee", 1.0)]);
    let anim = before
        .magic_move_to(
            &after,
            MagicMoveKey::Name,
            MagicMoveUnmatched::Fade,
            Some(0.8),
        )
        .unwrap();
    canvas.play(vec![anim]);
    canvas.wait(0.5);
    canvas
        .segment(
            "next",
            Some(gaanim_timeline::transition::TransitionType::magic_move(
                0.6,
                MagicMoveKey::Name,
            )),
        )
        .unwrap();
    board(&mut canvas, &[("dee", 2.0), ("bo", 1.0)]);
    canvas.wait(1.0);
    canvas
}

#[test]
fn magic_move_seeks_like_real_time_playback() {
    let mut world = incremental_world();
    hot_reload(&mut world, magic_move_deck());
    assert_seek_history_independent(&mut world);
}

#[test]
fn drawn_objects_stay_hidden_until_their_animation_after_any_seek() {
    let mut world = incremental_world();
    hot_reload(&mut world, drawn_cards_deck(3, 0));
    assert_seek_history_independent(&mut world);

    // Playback before an edit, then an incremental reload of the last slide.
    play(&mut world, 0.0, 2.0);
    hot_reload(&mut world, drawn_cards_deck(3, 0));
    play(&mut world, 2.0, 9.0);
    hot_reload(&mut world, drawn_cards_deck(3, 1));
    assert_seek_history_independent(&mut world);
}

#[test]
fn themed_presentation_reloads_incrementally_like_a_full_replay() {
    let slides = 4;
    let fonts = gaanim_text::font::FontRegistry::new();
    let text = gaanim_text::prelude::TextConfig::default();
    assert_eq!(
        thesis_deck(slides, &[])
            .fingerprints(&text, &fonts)
            .shared_prefix(&thesis_deck(slides, &[]).fingerprints(&text, &fonts)),
        slides,
        "equal presentations must fingerprint equally, including themes and layouts"
    );

    let mut world = incremental_world();
    let mut edits = vec![Edit::None];
    hot_reload(&mut world, thesis_deck(slides, &edits));
    edits.push(Edit::Retitle(2));
    assert_eq!(
        hot_reload(&mut world, thesis_deck(slides, &edits)),
        ReplayKind::Full
    );
    let reused = |reused| ReplayKind::Incremental {
        reused,
        segments: slides,
    };
    for (edit, kind) in [
        (Edit::Retitle(2), reused(2)),
        (Edit::Retitle(3), reused(2)),
        (Edit::None, ReplayKind::Unchanged { segments: slides }),
        (Edit::Retitle(3), reused(3)),
    ] {
        seek(&mut world, 2.0);
        edits.push(edit);
        assert_eq!(hot_reload(&mut world, thesis_deck(slides, &edits)), kind);
        let incremental = observe(&mut world);
        let mut full = incremental_world();
        replay_canvas_into(&mut full, thesis_deck(slides, &edits));
        full.flush();
        gaanim_timeline::capture_reload_keyframe(&mut full);
        assert_eq!(incremental, observe(&mut full));
    }
}

/// A counter declared first, animated in its own slide and, after
/// `Edit::Insert(n)`, again in slide `n`. Compilation writes the tween
/// windows of the counter onto its entity after every segment. The extra
/// animation ends at the same value, so only those windows change.
fn counter_deck(slides: usize, edits: &[Edit]) -> SceneModel {
    let mut canvas = SceneModel::new(640, 360);
    canvas.segment("Counter", None).unwrap();
    let count = canvas.parameter(0.0).unwrap();
    canvas
        .rolling_number(
            count.source(),
            gaanim_animation::RollingNumberOptions {
                mode: gaanim_animation::RollingMode::Continuous,
                ..Default::default()
            },
        )
        .unwrap();
    canvas.play(vec![count.animate().set(42.0).duration(1.0)]);
    canvas.stop(Some("counted".into())).unwrap();
    for index in 1..slides {
        canvas.segment(format!("Slide {index}"), None).unwrap();
        let retitled = edits
            .iter()
            .filter(|edit| matches!(edit, Edit::Retitle(slide) if *slide == index))
            .count();
        let title = canvas.text(&format!("Slide {index} revision {retitled}"));
        canvas.play(vec![title.animate().fade_in().duration(0.5)]);
        if edits
            .iter()
            .any(|edit| matches!(edit, Edit::Insert(slide) if *slide == index))
        {
            canvas.play(vec![count.animate().set(7.0).duration(0.5)]);
            canvas.play(vec![count.animate().set(42.0).duration(0.5)]);
        }
        canvas.stop(Some(format!("end {index}"))).unwrap();
    }
    canvas
}

fn rolling_tweens(world: &mut World) -> Vec<Option<Vec<(f64, f64)>>> {
    let mut tweens = world
        .query_filtered::<(
            &gaanim_scene::MobjectId,
            Option<&gaanim_animation::RollingTweens>,
        ), With<gaanim_animation::RollingNumber>>()
        .iter(world)
        .map(|(id, tweens)| (id.0, tweens.map(|tweens| tweens.0.clone())))
        .collect::<Vec<_>>();
    tweens.sort_by_key(|(id, _)| *id);
    tweens.into_iter().map(|(_, tweens)| tweens).collect()
}

#[test]
fn a_counter_before_the_checkpoint_still_reloads_incrementally() {
    let slides = 4;
    let mut world = incremental_world();
    let mut edits = vec![Edit::None];
    hot_reload(&mut world, counter_deck(slides, &edits));
    edits.push(Edit::Retitle(3));
    assert_eq!(
        hot_reload(&mut world, counter_deck(slides, &edits)),
        ReplayKind::Full
    );
    // Slide 3 animates the counter again, then stops doing so: the kept
    // counter must gain and lose those tween windows like a full replay.
    for edit in [Edit::Retitle(3), Edit::Insert(3), Edit::Retitle(3)] {
        seek(&mut world, 1.5);
        if matches!(edit, Edit::Retitle(_)) {
            edits.retain(|edit| !matches!(edit, Edit::Insert(_)));
        }
        edits.push(edit);
        assert_eq!(
            hot_reload(&mut world, counter_deck(slides, &edits)),
            ReplayKind::Incremental {
                reused: 3,
                segments: slides
            }
        );
        let mut full = incremental_world();
        replay_canvas_into(&mut full, counter_deck(slides, &edits));
        full.flush();
        gaanim_timeline::capture_reload_keyframe(&mut full);
        assert_eq!(rolling_tweens(&mut world), rolling_tweens(&mut full));
        assert_eq!(observe(&mut world), observe(&mut full));
    }
}

/// A persistent badge that every slide fades out and back in, like a
/// logo hidden on section dividers. Compiling a fade-in sets the badge's
/// opacity to zero, so every slide changes an entity the first one spawned.
fn badge_deck(slides: usize, edits: &[Edit]) -> SceneModel {
    let mut canvas = SceneModel::new(640, 360);
    let badge = canvas.circle(0.2).move_to(2.5, 1.2);
    canvas.persist(&badge).unwrap();
    for index in 0..slides {
        canvas.segment(format!("Slide {index}"), None).unwrap();
        let retitled = edits
            .iter()
            .filter(|edit| matches!(edit, Edit::Retitle(slide) if *slide == index))
            .count();
        let title = canvas.text(&format!("Slide {index} revision {retitled}"));
        canvas.play(vec![badge.animate().fade_out().duration(0.25)]);
        canvas.play(vec![title.animate().fade_in().duration(0.5)]);
        if !edits
            .iter()
            .any(|edit| matches!(edit, Edit::Insert(slide) if *slide == index))
        {
            canvas.play(vec![badge.animate().fade_in().duration(0.25)]);
        }
        canvas.stop(Some(format!("end {index}"))).unwrap();
    }
    canvas
}

#[test]
fn restorable_changes_to_kept_entities_still_reload_incrementally() {
    let slides = 5;
    // Slide `n` is segment `n + 1`, after the implicit badge segment.
    let segments = slides + 1;
    let mut world = incremental_world();
    let mut edits = vec![Edit::None];
    hot_reload(&mut world, badge_deck(slides, &edits));
    edits.push(Edit::Retitle(2));
    assert_eq!(
        hot_reload(&mut world, badge_deck(slides, &edits)),
        ReplayKind::Full
    );
    // Slide 2 stops fading the badge back in, and later starts again.
    for (edit, reused) in [
        (Edit::Retitle(2), 3),
        (Edit::Insert(2), 3),
        (Edit::Retitle(2), 3),
        (Edit::Retitle(4), 3),
        (Edit::Retitle(4), 5),
    ] {
        seek(&mut world, 2.2);
        if matches!(edit, Edit::Retitle(2)) {
            edits.retain(|edit| !matches!(edit, Edit::Insert(_)));
        }
        edits.push(edit);
        assert_eq!(
            hot_reload(&mut world, badge_deck(slides, &edits)),
            ReplayKind::Incremental { reused, segments }
        );
        let mut full = incremental_world();
        replay_canvas_into(&mut full, badge_deck(slides, &edits));
        full.flush();
        gaanim_timeline::capture_reload_keyframe(&mut full);
        assert_eq!(observe(&mut world), observe(&mut full));
    }
}

#[test]
fn opaque_callbacks_are_never_reused() {
    let fonts = gaanim_text::font::FontRegistry::new();
    let text = gaanim_text::prelude::TextConfig::default();
    let with_rate = || {
        let mut canvas = SceneModel::new(640, 360);
        canvas.segment("First", None).unwrap();
        let dot = canvas.circle(0.2);
        canvas.play(vec![dot.animate().shift_by(1.0, 0.0).rate_func(
            gaanim_math::RateFunc::Custom(std::sync::Arc::new(|t| t * t)),
        )]);
        canvas.segment("Second", None).unwrap();
        canvas.wait(1.0);
        canvas
    };
    let first = with_rate();
    let second = with_rate();
    assert_eq!(
        first
            .fingerprints(&text, &fonts)
            .shared_prefix(&second.fingerprints(&text, &fonts)),
        0,
        "two different closures must never count as the same segment"
    );
}
