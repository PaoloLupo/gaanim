//! Runtime replay helpers for canonical `gaanim_api` descriptions.

use bevy::prelude::*;
use gaanim_math::Camera;
use gaanim_renderer::pipeline::GaanimFullWindowClearCamera;
use gaanim_renderer::prelude::VelloView;
use gaanim_timeline::timeline::Timeline;

use crate::canvas::{CompileCheckpoint, SceneFingerprints, SceneModel, SegmentMarker};
use gaanim_scene::prelude::{ArchetypeId, ComponentId, Tick};
use std::sync::{Arc, Mutex};

/// Replay a [`SceneModel`] into a Bevy world.
///
/// This is the canonical runtime bridge used by the editor/hot-reload host and
/// by future scripting language bindings. Bindings should not duplicate replay
/// logic; they should construct `SceneModel` and call into this module indirectly.
pub fn replay_canvas_into(world: &mut World, canvas: SceneModel) {
    replay_prepared(world, &canvas, |commands, timeline, fonts, text_config| {
        canvas.compile_into(commands, timeline, fonts, text_config);
    });
}

/// Resolve the scene's text theme and fonts, ensure the preview cameras, and
/// run `compile` against the world's timeline. Returns `None` when a required
/// runtime resource is missing.
fn replay_prepared<R>(
    world: &mut World,
    canvas: &SceneModel,
    compile: impl FnOnce(
        &mut Commands,
        &mut Timeline,
        &gaanim_text::font::FontRegistry,
        &gaanim_text::prelude::TextConfig,
    ) -> R,
) -> Option<R> {
    let (width, height) = canvas.frame.preview_pixel_size();
    let frame = canvas.frame;

    let mut timeline = match world.remove_resource::<Timeline>() {
        Some(res) => res,
        None => {
            bevy::prelude::error!("Timeline resource missing");
            return None;
        }
    };
    let mut font_registry = match world.remove_resource::<gaanim_text::font::FontRegistry>() {
        Some(res) => res,
        None => {
            bevy::prelude::error!("FontRegistry resource missing");
            world.insert_resource(timeline);
            return None;
        }
    };
    let mut text_config = match world.remove_resource::<gaanim_text::prelude::TextConfig>() {
        Some(res) => res,
        None => {
            bevy::prelude::error!("TextConfig resource missing");
            world.insert_resource(timeline);
            world.insert_resource(font_registry);
            return None;
        }
    };
    text_config = canvas.scene_text_config(&text_config);
    canvas.register_theme_fonts(&mut font_registry);

    let result = {
        let has_camera_2d = world
            .query_filtered::<Entity, With<Camera2d>>()
            .iter(world)
            .next()
            .is_some();
        let has_clear_camera = world
            .query_filtered::<Entity, With<GaanimFullWindowClearCamera>>()
            .iter(world)
            .next()
            .is_some();

        // A Camera2d retained by the project hub must use the same overlay
        // policy as a camera spawned directly for a script.
        let mut vello_cameras =
            world.query_filtered::<&mut bevy::prelude::Camera, (With<Camera2d>, With<VelloView>)>();
        for mut camera in vello_cameras.iter_mut(world) {
            camera.order = 1;
            camera.clear_color = ClearColorConfig::None;
        }

        let mut commands = world.commands();
        commands.insert_resource(Camera::ortho_2d_frame(
            frame.width,
            frame.height,
            width,
            height,
        ));
        // Spawn the Vello camera first. bevy_egui assigns the primary context
        // to the first camera created, so this must be the camera that renders
        // last and therefore owns the egui pass.
        if !has_camera_2d {
            commands.spawn((
                Camera2d,
                VelloView,
                bevy::prelude::Camera {
                    order: 1,
                    clear_color: ClearColorConfig::None,
                    ..default()
                },
                bevy::core_pipeline::tonemapping::Tonemapping::None,
            ));
        }
        if !has_clear_camera {
            // Clear the complete render target before the canvas is drawn.
            // RenderLayers::none keeps this camera color-only.
            commands.spawn((
                Camera2d,
                GaanimFullWindowClearCamera,
                bevy::prelude::Camera {
                    order: -1,
                    clear_color: ClearColorConfig::Default,
                    ..default()
                },
                bevy::camera::visibility::RenderLayers::none(),
            ));
        }

        compile(&mut commands, &mut timeline, &font_registry, &text_config)
    };

    let cached_duration = timeline.cached_duration;
    world.insert_resource(timeline);
    world.insert_resource(font_registry);
    world.insert_resource(text_config);

    if let Some(mut tl) = world.get_resource_mut::<Timeline>() {
        tl.loop_range = Some((0.0, cached_duration));
    }
    Some(result)
}

/// Entities that belong to a replayed scene and are removed with it.
pub type SceneOwned = Or<(
    With<gaanim_scene::MobjectId>,
    With<gaanim_animation::PropertyBinding>,
    With<gaanim_animation::CameraBinding>,
)>;

/// Compilation state kept between hot reloads of one scene so that the next
/// revision recompiles only from its first changed segment.
#[derive(Resource)]
pub struct RetainedReplay {
    /// The revision materialized in the world. Holding it keeps alive every
    /// callback whose address its fingerprints recorded.
    _canvas: SceneModel,
    fingerprints: SceneFingerprints,
    checkpoint: Option<RetainedCheckpoint>,
}

struct RetainedCheckpoint {
    compile: CompileCheckpoint,
    /// Entities spawned after this tick were compiled from the checkpoint on.
    spawned_after: Tick,
    /// The entities spawned before the checkpoint, as compiled up to it.
    /// Later segments may only have changed what restoring this undoes.
    base: gaanim_timeline::snapshot::WorldSnapshot,
    /// Their archetypes then: an entity since moved to another one lost or
    /// gained a component, which no change tick records.
    archetypes: std::collections::HashMap<Entity, ArchetypeId>,
}

impl RetainedCheckpoint {
    /// The checkpoint `record` marked, if it captured one.
    fn marked(compile: CompileCheckpoint, record: &mut MarkerRecord) -> Option<Self> {
        Some(Self {
            compile,
            spawned_after: record.tick?,
            base: record.snapshot.take()?,
            archetypes: record.existing.iter().copied().collect(),
        })
    }
}

/// How a hot reload rebuilt the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayKind {
    /// Every segment was compiled.
    Full,
    /// The first `reused` of `segments` segments were kept from the previous
    /// revision; only the rest were compiled.
    Incremental { reused: usize, segments: usize },
    /// Nothing that the compilation reads changed: the world and its
    /// timeline, with their keyframes and checkpoints, already hold the
    /// `segments` segments of this revision, so nothing was compiled.
    Unchanged { segments: usize },
}

/// Replay a new revision of the scene already in `world`, compiling only from
/// its first changed segment when that provably equals a full replay.
///
/// Otherwise `clear_scene` removes the previous scene and every segment is
/// compiled. Either way the world keeps a [`RetainedReplay`] for the next
/// revision; `allow_reuse = false` forces a full replay but still retains it.
/// When nothing that the compilation reads changed, the world is left as it
/// is ([`ReplayKind::Unchanged`]).
///
/// Unless nothing changed, the t=0 keyframe must then be captured with
/// [`gaanim_timeline::capture_reload_keyframe`] (or the deferred capture that
/// `NeedsKeyframeCapture` schedules) once the new entities exist.
///
/// `GAANIM_RELOAD_PROFILE=1` prints how long each phase took and the
/// segments that took longest to compile.
pub fn replay_canvas_incremental(
    world: &mut World,
    canvas: SceneModel,
    allow_reuse: bool,
    clear_scene: impl FnOnce(&mut World),
) -> ReplayKind {
    let mut timings = ReloadTimings::start();
    let previous = world.remove_resource::<RetainedReplay>();
    let Some(fingerprints) = scene_fingerprints(world, &canvas) else {
        clear_scene(world);
        replay_canvas_into(world, canvas);
        timings.phase("full compile");
        timings.report(ReplayKind::Full);
        return ReplayKind::Full;
    };
    timings.phase("fingerprints");
    let previous = match previous {
        Some(previous) if allow_reuse && previous.fingerprints.unchanged(&fingerprints) => {
            // The previous revision's entities, timeline and callbacks are
            // this one's.
            let kind = ReplayKind::Unchanged {
                segments: fingerprints.segment_count(),
            };
            world.insert_resource(previous);
            timings.report(kind);
            return kind;
        }
        Some(previous) if allow_reuse => {
            match try_incremental(world, &canvas, &fingerprints, previous, &mut timings) {
                Ok(kind) => {
                    timings.report(kind);
                    return kind;
                }
                Err(previous) => previous,
            }
        }
        previous => previous.map(|previous| previous.fingerprints),
    };
    clear_scene(world);
    timings.phase("clear");
    replay_full_retained(world, canvas, fingerprints, previous.as_ref());
    timings.phase("full compile");
    timings.report(ReplayKind::Full);
    ReplayKind::Full
}

/// How long each phase of a replay took, printed with
/// `GAANIM_RELOAD_PROFILE=1`.
struct ReloadTimings {
    phases: Option<(std::time::Instant, Vec<(&'static str, f64)>)>,
}

impl ReloadTimings {
    fn start() -> Self {
        let enabled = crate::canvas::reload_profile_enabled();
        if enabled {
            crate::canvas::time_segments();
        }
        Self {
            phases: enabled.then(|| (std::time::Instant::now(), Vec::new())),
        }
    }

    /// End the phase named `name` now.
    fn phase(&mut self, name: &'static str) {
        if let Some((started, phases)) = &mut self.phases {
            let now = std::time::Instant::now();
            phases.push((name, (now - *started).as_secs_f64() * 1000.0));
            *started = now;
        }
    }

    fn report(&self, kind: ReplayKind) {
        let Some((_, phases)) = &self.phases else {
            return;
        };
        let phases = phases
            .iter()
            .map(|(name, ms)| format!("{name} {ms:.1} ms"))
            .collect::<Vec<_>>()
            .join(", ");
        gaanim_core::console::info("profile", format!("replay {kind:?}: {phases}"));
        let mut segments = crate::canvas::take_segment_times();
        segments.sort_by_key(|(_, time)| std::cmp::Reverse(*time));
        if !segments.is_empty() {
            let slowest = segments
                .iter()
                .take(5)
                .map(|(name, time)| format!("`{name}` {:.1} ms", time.as_secs_f64() * 1000.0))
                .collect::<Vec<_>>()
                .join(", ");
            gaanim_core::console::info(
                "profile",
                format!("{} segments compiled; slowest: {slowest}", segments.len()),
            );
        }
    }
}

/// Drop `value` on another thread: freeing a large scene takes a while
/// that whoever replaced it need not wait.
pub(crate) fn drop_elsewhere<T: Send + 'static>(value: T) {
    let _ = std::thread::Builder::new()
        .name("gaanim-drop".into())
        .spawn(move || drop(value));
}

fn scene_fingerprints(world: &mut World, canvas: &SceneModel) -> Option<SceneFingerprints> {
    let text_config =
        canvas.scene_text_config(world.get_resource::<gaanim_text::prelude::TextConfig>()?);
    let mut fonts = world.get_resource_mut::<gaanim_text::font::FontRegistry>()?;
    canvas.register_theme_fonts(&mut fonts);
    Some(canvas.fingerprints(&text_config, &fonts))
}

/// Scene-owned entities present when a marker ran, the tick after which
/// later commands are stamped, whether the segments after the marker touched
/// those entities (see [`segments_end_marker`]) and, for a checkpoint, their
/// compiled state when it ran.
#[derive(Default)]
struct MarkerRecord {
    tick: Option<Tick>,
    existing: Vec<(Entity, ArchetypeId)>,
    touched: Option<bool>,
    snapshot: Option<gaanim_timeline::snapshot::WorldSnapshot>,
}

/// Record the entities that exist when the marker runs; `snapshot` also
/// captures their state, the base of a [`RetainedCheckpoint`].
fn segment_marker(record: Arc<Mutex<MarkerRecord>>, snapshot: bool) -> SegmentMarker {
    Box::new(move |world: &mut World| {
        let mut query = world.query_filtered::<Entity, SceneOwned>();
        let existing = query
            .iter(world)
            .map(|entity| (entity, world.entity(entity).archetype().id()))
            .collect();
        let snapshot = snapshot.then(|| gaanim_timeline::snapshot::WorldSnapshot::capture(world));
        let tick = world.change_tick();
        // Every later command is stamped with a newer tick.
        world.increment_change_tick();
        *record.lock().expect("segment marker poisoned") = MarkerRecord {
            tick: Some(tick),
            existing,
            touched: None,
            snapshot,
        };
    })
}

/// Runs after the last segment, before compilation derives state from the
/// whole timeline (rolling-number tween windows) and applies it to every
/// entity, kept ones included. That pass rebuilds such state completely on
/// every compile, so only what the segments did counts against reuse.
fn segments_end_marker(records: Vec<Arc<Mutex<MarkerRecord>>>) -> SegmentMarker {
    Box::new(move |world: &mut World| {
        for record in &records {
            let mut record = record.lock().expect("segment marker poisoned");
            record.touched = Some(touched_since(world, &record));
        }
    })
}

/// Whether the segments after the marker despawned an entity that existed
/// when it ran, or changed, added or removed one of its components that
/// restoring a snapshot does not set back.
fn marker_entities_touched(record: &Mutex<MarkerRecord>) -> bool {
    record
        .lock()
        .expect("segment marker poisoned")
        .touched
        .unwrap_or(true)
}

fn touched_since(world: &World, record: &MarkerRecord) -> bool {
    let Some(tick) = record.tick else {
        return true;
    };
    let this_run = world.read_change_tick();
    let restorable = |component: ComponentId| {
        world
            .components()
            .get_info(component)
            .and_then(|info| info.type_id())
            .is_some_and(gaanim_timeline::snapshot::restores_component)
    };
    record.existing.iter().any(|&(entity, archetype)| {
        let Ok(entity) = world.get_entity(entity) else {
            return true;
        };
        let now = entity.archetype();
        if now.id() != archetype {
            let Some(before) = world.archetypes().get(archetype) else {
                return true;
            };
            let added = now
                .components()
                .iter()
                .filter(|&&component| !before.contains(component));
            let removed = before
                .components()
                .iter()
                .filter(|&&component| !now.contains(component));
            if added
                .chain(removed)
                .any(|&component| !restorable(component))
            {
                return true;
            }
        }
        now.components().iter().any(|&component| {
            !restorable(component)
                && entity
                    .get_change_ticks_by_id(component)
                    .is_some_and(|ticks| ticks.is_changed(tick, this_run))
        })
    })
}

/// Compile every segment and retain fingerprints plus, when a previous
/// revision shows where editing happens, a checkpoint at its first change.
fn replay_full_retained(
    world: &mut World,
    canvas: SceneModel,
    fingerprints: SceneFingerprints,
    previous: Option<&SceneFingerprints>,
) {
    let checkpoint_at = previous
        .map(|previous| previous.shared_prefix(&fingerprints))
        .filter(|&index| index > 0);
    let record = Arc::new(Mutex::new(MarkerRecord::default()));
    let markers = checkpoint_at
        .map(|index| {
            vec![
                (index, segment_marker(record.clone(), true)),
                (
                    fingerprints.segment_count(),
                    segments_end_marker(vec![record.clone()]),
                ),
            ]
        })
        .unwrap_or_default();
    let checkpoint = replay_prepared(world, &canvas, |commands, timeline, fonts, text_config| {
        canvas.compile_resumable(
            commands,
            timeline,
            fonts,
            text_config,
            None,
            checkpoint_at,
            markers,
        )
    })
    .flatten();
    world.flush();
    let checkpoint = checkpoint
        .filter(|_| !marker_entities_touched(&record))
        .and_then(|compile| {
            RetainedCheckpoint::marked(
                compile,
                &mut record.lock().expect("segment marker poisoned"),
            )
        });
    world.insert_resource(RetainedReplay {
        _canvas: canvas,
        fingerprints,
        checkpoint,
    });
}

/// Replay only the segments after the retained checkpoint. `Err` returns the
/// previous fingerprints when a full replay is required; the world may then
/// hold a partial scene that the caller must clear.
fn try_incremental(
    world: &mut World,
    canvas: &SceneModel,
    fingerprints: &SceneFingerprints,
    previous: RetainedReplay,
    timings: &mut ReloadTimings,
) -> Result<ReplayKind, Option<SceneFingerprints>> {
    let RetainedReplay {
        fingerprints: previous_fingerprints,
        checkpoint,
        ..
    } = previous;
    let shared = previous_fingerprints.shared_prefix(fingerprints);
    let Some(checkpoint) =
        checkpoint.filter(|checkpoint| checkpoint.compile.cursor.next_segment <= shared)
    else {
        return Err(Some(previous_fingerprints));
    };
    // Bevy clamps very old change ticks, after which spawn order is no longer
    // observable; a checkpoint that old is rebuilt instead.
    let age = world
        .change_tick()
        .get()
        .wrapping_sub(checkpoint.spawned_after.get());
    if age > u32::MAX / 4 {
        return Err(Some(previous_fingerprints));
    }
    let resume_at = checkpoint.compile.cursor.next_segment;

    // Remove everything the previous revision compiled from the checkpoint on.
    let this_run = world.change_tick();
    let mut stale = Vec::new();
    let mut query =
        world.query_filtered::<(Entity, Option<&gaanim_scene::MobjectId>), SceneOwned>();
    for (entity, id) in query.iter(world) {
        if world
            .entity(entity)
            .spawn_tick()
            .is_newer_than(checkpoint.spawned_after, this_run)
        {
            stale.push((entity, id.map(|id| id.0)));
        }
    }
    let stale_ids = stale.iter().filter_map(|(_, id)| *id).collect::<Vec<_>>();
    for (entity, _) in stale {
        if world.get_entity(entity).is_ok() {
            world.despawn(entity);
        }
    }
    timings.phase("despawn");
    if let Some(mut cache) =
        world.get_resource_mut::<gaanim_renderer::pipeline::GaanimRenderCache>()
    {
        // Later segments reuse these IDs for new content.
        for id in stale_ids {
            cache.fragment_cache.remove(&id);
        }
    }
    world.remove_resource::<gaanim_animation::CustomAnimationDiagnostics>();
    world.remove_resource::<gaanim_animation::PropertyBindingDiagnostics>();
    world.remove_resource::<gaanim_animation::PropertySignalTimeline>();
    world.remove_resource::<gaanim_animation::PropertySignalStops>();
    // Playback and the segments the previous revision compiled after the
    // checkpoint changed the kept entities; set them back to the checkpoint.
    // The segments compiled next apply their own changes again, so the t=0
    // keyframe is then captured from the world as after a full replay.
    let this_run = world.read_change_tick();
    // The compile works on copies of the checkpoint's timeline and cursor,
    // made while the kept entities are restored.
    let (mut restored, cursor) = std::thread::scope(|scope| {
        let copies = scope.spawn(|| {
            (
                checkpoint.compile.timeline.clone(),
                checkpoint.compile.cursor.clone(),
            )
        });
        checkpoint.base.restore_selected(world, |world, _, entity| {
            let entity = world.entity(entity);
            checkpoint.archetypes.get(&entity.id()) != Some(&entity.archetype().id())
                || entity.archetype().components().iter().any(|&component| {
                    entity
                        .get_change_ticks_by_id(component)
                        .is_some_and(|ticks| ticks.is_changed(checkpoint.spawned_after, this_run))
                })
        });
        copies.join().expect("copying the checkpoint panicked")
    });
    let mut timeline = world.resource_mut::<Timeline>();
    restored.playback_rate = timeline.playback_rate;
    restored.loop_range = timeline.loop_range;
    restored.is_playing = false;
    drop_elsewhere(std::mem::replace(&mut *timeline, restored));
    timings.phase("restore");

    // Move the checkpoint to this revision's first change so the next edit
    // at the same place recompiles as little as possible; a measurement
    // moves it to where authoring continues.
    let target = match goal {
        ReplayGoal::Reload => shared,
        ReplayGoal::Measure => fingerprints.segment_count().saturating_sub(1),
    };
    let advance_to = (target > resume_at).then_some(target);
    let kept_record = Arc::new(Mutex::new(MarkerRecord::default()));
    let advanced_record = Arc::new(Mutex::new(MarkerRecord::default()));
    let mut markers = vec![(resume_at, segment_marker(kept_record.clone(), false))];
    if let Some(index) = advance_to {
        markers.push((index, segment_marker(advanced_record.clone(), true)));
    }
    markers.push((
        fingerprints.segment_count(),
        segments_end_marker(vec![kept_record.clone(), advanced_record.clone()]),
    ));
    let advanced = replay_prepared(world, canvas, |commands, timeline, fonts, text_config| {
        canvas.compile_resumable(
            commands,
            timeline,
            fonts,
            text_config,
            Some(cursor),
            advance_to,
            markers,
        )
    });
    world.flush();
    timings.phase("compile");
    let Some(advanced) = advanced else {
        return Err(Some(previous_fingerprints));
    };
    if marker_entities_touched(&kept_record) {
        // A later segment changed kept entities beyond what the checkpoint
        // restores; only a full replay undoes what it did to them.
        return Err(Some(previous_fingerprints));
    }

    let checkpoint = match advanced {
        Some(compile) if !marker_entities_touched(&advanced_record) => RetainedCheckpoint::marked(
            compile,
            &mut advanced_record.lock().expect("segment marker poisoned"),
        )
        .unwrap_or(checkpoint),
        _ => checkpoint,
    };
    world.insert_resource(RetainedReplay {
        _canvas: canvas.clone(),
        fingerprints: fingerprints.clone(),
        checkpoint: Some(checkpoint),
    });
    Ok(ReplayKind::Incremental {
        reused: resume_at,
        segments: fingerprints.segment_count(),
    })
}

#[cfg(test)]
mod tests {
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
            let transition =
                (index > 0).then_some(gaanim_timeline::transition::TransitionType::CrossFade {
                    duration: 0.3,
                });
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
            let transition =
                (index > 0).then_some(gaanim_timeline::transition::TransitionType::CrossFade {
                    duration: 0.45,
                });
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
            .magic_move_to(&after, MagicMoveKey::Name, MagicMoveUnmatched::Fade, 0.8)
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
}
