//! Hot-reload infrastructure for the Gaanim host.

use bevy::prelude::*;
use bevy_egui::egui;
use crossbeam_channel::Receiver;
use gaanim_api::host::ReloadPayload;
use gaanim_api::runtime;
use std::time::Instant;

use gaanim_editor::{EditorState, export::StashedReplay};
use gaanim_scene::MobjectId;
use gaanim_timeline::timeline::Timeline;

/// Bevy resource holding the receiver end of the host channel.
#[derive(Resource)]
pub struct ReloadReceiver {
    pub rx: Receiver<ReloadPayload>,
}

/// Optional human-readable status line shown in the editor.
#[derive(Resource)]
pub struct ReloadStatus {
    pub last_message: String,
    /// Time spent executing Python up to `scene.render()`.
    pub compile_duration_seconds: Option<f64>,
    /// Time spent replaying the canvas, including text compilation.
    pub replay_duration_seconds: Option<f64>,
    /// `Some(seconds_since_startup)` when the message was set.
    pub shown_at: Option<f64>,
}

impl Default for ReloadStatus {
    fn default() -> Self {
        Self {
            last_message: String::new(),
            compile_duration_seconds: None,
            replay_duration_seconds: None,
            shown_at: None,
        }
    }
}

/// Ultimo traceback de error del script, mostrado en el editor.
#[derive(Resource, Default)]
pub struct ScriptError {
    pub message: Option<String>,
    pub updated_at: Option<f64>,
}

/// Receiver para tracebacks enviados por el hilo de Python.
#[derive(Resource)]
pub struct ScriptErrorReceiver {
    pub rx: Receiver<String>,
}

/// Despawn every mobject entity and reset the [`Timeline`]
/// to a clean state. This is the "clear canvas" step before replaying fresh ops.
///
/// The Camera2d + VelloView entity is intentionally kept alive so the window
/// surface and egui context remain valid across hot-reloads.
pub fn clear_scene_entities(world: &mut World) {
    let to_despawn: Vec<Entity> = {
        let mut q = world.query::<(Entity, &MobjectId)>();
        q.iter(world).map(|(e, _)| e).collect()
    };
    for e in to_despawn {
        if world.get_entity(e).is_ok() {
            world.despawn(e);
        }
    }
    // Canvas compilation allocates ObjectIds deterministically from zero on
    // every replay. Retained Vello fragments are keyed by those IDs, so keeping
    // the previous revision's cache can attach an old glyph or arrowhead to a
    // newly compiled (and initially hidden) object with the same ID.
    if let Some(mut cache) =
        world.get_resource_mut::<gaanim_renderer::pipeline::GaanimRenderCache>()
    {
        cache.fragment_cache.clear();
    }
    if let Some(mut tl) = world.get_resource_mut::<Timeline>() {
        let playback_rate = tl.playback_rate;
        let loop_range = tl.loop_range;
        *tl = Timeline::default();
        tl.playback_rate = playback_rate;
        tl.loop_range = loop_range;
        tl.is_playing = false;
    }
}

/// System: drains pending error strings and updates [`ScriptError`].
pub fn script_error_listener_system(world: &mut World) {
    let errors: Vec<String> = {
        let Some(rx_res) = world.get_resource::<ScriptErrorReceiver>() else {
            return;
        };
        rx_res.rx.try_iter().collect()
    };
    if errors.is_empty() {
        return;
    }
    let last = errors.last().expect("checked non-empty").clone();
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut err) = world.get_resource_mut::<ScriptError>() {
        err.message = Some(last);
        err.updated_at = Some(now);
    }
    // limpiar el badge de éxito para no solapar
    if let Some(mut status) = world.get_resource_mut::<ReloadStatus>() {
        status.last_message.clear();
        status.shown_at = None;
    }
}

/// System: drains pending [`ReloadPayload`]s and rebuilds the scene.
///
/// If the timeline was playing, the current playback position is preserved
/// across the reload so the user sees the same frame (with updated content).
pub fn reload_listener_system(world: &mut World) {
    let payloads: Vec<ReloadPayload> = {
        let Some(rx_res) = world.get_resource::<ReloadReceiver>() else {
            return;
        };
        rx_res.rx.try_iter().collect()
    };
    if payloads.is_empty() {
        return;
    }
    let payload = payloads.last().expect("checked non-empty").clone();

    // Snapshot playback state before tearing down entities.
    let (saved_time, was_playing, had_previous_timeline) = {
        let tl = world.resource::<Timeline>();
        (tl.current_time, tl.is_playing, tl.cached_duration > 0.0)
    };

    let width = payload.canvas.width;
    let height = payload.canvas.height;
    let compile_duration = payload.compile_duration.as_secs_f64();
    let replay_started_at = Instant::now();
    reload_with(world, payload.canvas);
    let replay_duration = replay_started_at.elapsed().as_secs_f64();

    // Restore playback position after rebuild.
    let mut reload_target = None;
    if let Some(mut tl) = world.get_resource_mut::<Timeline>() {
        let target = reload_target_time(saved_time, &tl);
        tl.seek_request = Some(target);
        tl.is_playing = if had_previous_timeline {
            was_playing
        } else {
            tl.cached_duration > 0.0
        };
        reload_target = Some(target);
    }
    if let Some(target) = reload_target
        && world.contains_resource::<EditorState>()
    {
        world.resource_scope(|world, mut editor_state: Mut<EditorState>| {
            if let Some(mut timeline) = world.get_resource_mut::<Timeline>() {
                editor_state.reconcile_segment_loop_after_reload(&mut timeline, target);
            }
        });
    }

    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut status) = world.get_resource_mut::<ReloadStatus>() {
        status.compile_duration_seconds = Some(compile_duration);
        status.replay_duration_seconds = Some(replay_duration);
        status.last_message =
            reload_status_message(compile_duration, replay_duration, width, height);
        eprintln!("[gaanim] {}", status.last_message);
        status.shown_at = Some(now);
    }
    // Éxito limpia el error previo
    if let Some(mut err) = world.get_resource_mut::<ScriptError>() {
        err.message = None;
        err.updated_at = None;
    }
}

fn reload_status_message(
    python_duration: f64,
    replay_duration: f64,
    width: u32,
    height: u32,
) -> String {
    format!(
        "Scene ready · Python {:.2}s · replay {:.2}s · total {:.2}s · {}x{}",
        python_duration,
        replay_duration,
        python_duration + replay_duration,
        width,
        height
    )
}

fn reload_target_time(saved_time: f64, timeline: &Timeline) -> f64 {
    saved_time.clamp(0.0, timeline.cached_duration.max(0.0))
}

/// Rebuild the scene in `world` from a fresh set of ops, then schedule the
/// t=0 keyframe capture for the next frame (after deferred Commands flush).
pub fn reload_with(world: &mut World, canvas: gaanim_api::canvas::Canvas) {
    clear_scene_entities(world);
    let revision = world
        .get_resource::<StashedReplay>()
        .map_or(1, |stash| stash.revision.wrapping_add(1).max(1));
    world.insert_resource(StashedReplay {
        canvas: Some(canvas.clone()),
        revision,
    });
    runtime::replay_canvas_into(world, canvas);
    // `replay_canvas_into` records entity spawns and component inserts in the
    // World's internal command queue.  Materialize them now: the deferred
    // keyframe capture runs in the Animation phase of this same update, after
    // this Input-phase reload system, and must not capture an empty baseline.
    world.flush();
    // Capture the fresh t=0 baseline later in this update, before timeline seek.
    world.insert_resource(gaanim_timeline::NeedsKeyframeCapture);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::ObjectId;
    use gaanim_scene::ObjectTag;
    use gaanim_timeline::timeline::SegmentMetadata;

    #[test]
    fn reload_keeps_fractional_time_inside_the_current_segment() {
        let mut timeline = Timeline::default();
        timeline.cached_duration = 8.0;
        timeline.set_segments(vec![SegmentMetadata {
            id: 7,
            name: "segment".to_owned(),
            notes: None,
            start_time: 3.0,
            end_time: 8.0,
            stops: Vec::new(),
        }]);

        let saved_time = 4.75;

        assert_eq!(reload_target_time(saved_time, &timeline), saved_time);
    }

    #[test]
    fn reload_clamps_time_when_the_new_script_is_shorter() {
        let mut timeline = Timeline::default();
        timeline.cached_duration = 4.0;

        assert_eq!(reload_target_time(4.75, &timeline), 4.0);
    }

    #[test]
    fn reload_status_separates_python_from_scene_replay() {
        assert_eq!(
            reload_status_message(0.125, 1.5, 1920, 1080),
            "Scene ready · Python 0.12s · replay 1.50s · total 1.62s · 1920x1080"
        );
    }

    #[test]
    fn repeated_hot_reload_clears_force_and_angle_labels_without_residue() {
        let mut world = World::new();
        world.insert_resource(Timeline::default());
        let editor_entity = world.spawn_empty().id();

        for revision in 0..2 {
            world.spawn((
                MobjectId(ObjectId::from_raw(10 + revision)),
                ObjectTag("force_at label".to_owned()),
            ));
            world.spawn((
                MobjectId(ObjectId::from_raw(20 + revision)),
                ObjectTag("theta angle label".to_owned()),
            ));

            clear_scene_entities(&mut world);

            assert_eq!(
                world.query::<&MobjectId>().iter(&world).count(),
                0,
                "every visual entity from the previous replay must be removed"
            );
            assert!(
                world.get_entity(editor_entity).is_ok(),
                "editor-owned entities must survive scene clearing"
            );
        }
    }

    #[test]
    fn hot_reload_discards_retained_fragments_before_object_ids_are_reused() {
        let mut world = World::new();
        world.insert_resource(Timeline::default());
        world.insert_resource(gaanim_renderer::pipeline::GaanimRenderCache::default());
        let reused_id = ObjectId::from_parts(12, 1);
        world.spawn((MobjectId(reused_id), ObjectTag("old force unit".to_owned())));
        world
            .resource_mut::<gaanim_renderer::pipeline::GaanimRenderCache>()
            .fragment_cache
            .insert(reused_id, Default::default());

        clear_scene_entities(&mut world);

        assert!(
            world
                .resource::<gaanim_renderer::pipeline::GaanimRenderCache>()
                .fragment_cache
                .is_empty(),
            "a new canvas reuses ObjectIds, so no retained geometry from the previous revision may survive"
        );
    }

    #[test]
    fn hot_reload_materializes_the_new_scene_before_keyframe_capture() {
        let mut world = World::new();
        world.insert_resource(Timeline::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());

        let mut canvas = gaanim_api::canvas::Canvas::new(320, 180);
        let source = canvas.dot(8.0);
        let _trail = canvas.traced_path(&source);
        canvas.wait(1.0);

        reload_with(&mut world, canvas);

        assert!(
            world.query::<&MobjectId>().iter(&world).count() > 0,
            "the deferred keyframe capture must see the replayed entities in this update"
        );
        assert_eq!(
            world
                .query::<&gaanim_animation::TracedPath>()
                .iter(&world)
                .count(),
            1,
            "the loop baseline must include reactive trail state"
        );
    }

    #[test]
    fn hot_reload_keeps_later_fade_text_hidden_before_playback_resumes() {
        let mut world = World::new();
        world.insert_resource(Timeline::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());

        let mut canvas = gaanim_api::canvas::Canvas::new(320, 180);
        let later = canvas.text("Later explanation");
        canvas.wait(1.0);
        canvas.play(vec![later.fade_in(0.5)]);
        canvas.wait(0.5);
        canvas.play(vec![later.fade_out(0.5)]);

        reload_with(&mut world, canvas);

        let compiled_id = ObjectId::from_raw(later.id.as_raw() - 1);
        let opacity = world
            .query::<(&MobjectId, &gaanim_scene::Opacity)>()
            .iter(&world)
            .find_map(|(id, opacity)| (id.0 == compiled_id).then_some(opacity.0))
            .expect("reloaded text root");
        assert_eq!(
            opacity, 0.0,
            "a text whose first entry is later on the timeline must not flash after hot reload"
        );
    }
}

/// How long the reload badge stays fully visible (seconds).
const RELOAD_BADGE_VISIBLE_SECS: f64 = 3.0;
/// How long the badge fades out (seconds).
const RELOAD_BADGE_FADE_SECS: f64 = 1.0;

/// egui panel showing the last reload status, auto-hiding after a few seconds.
pub fn reload_status_overlay_system(
    mut ctx: bevy_egui::EguiContexts,
    mut status: ResMut<ReloadStatus>,
    time: Res<Time>,
    presentation_mode: Option<Res<gaanim_editor::PresentationMode>>,
) {
    if presentation_mode.is_some_and(|mode| mode.active) {
        return;
    }
    let Some(shown_at) = status.shown_at else {
        return;
    };
    let elapsed = time.elapsed_secs_f64() - shown_at;
    let total_secs = RELOAD_BADGE_VISIBLE_SECS + RELOAD_BADGE_FADE_SECS;

    if elapsed >= total_secs {
        status.last_message.clear();
        status.shown_at = None;
        return;
    }

    let alpha_mul = if elapsed < RELOAD_BADGE_VISIBLE_SECS {
        1.0_f32
    } else {
        1.0 - ((elapsed - RELOAD_BADGE_VISIBLE_SECS) / RELOAD_BADGE_FADE_SECS) as f32
    };

    if alpha_mul < 0.01 {
        return;
    }

    let Ok(ctx) = ctx.ctx_mut() else {
        return;
    };
    let bg_alpha = (200.0 * alpha_mul) as u8;
    let text_alpha = (255.0 * alpha_mul) as u8;
    egui::Area::new("reload_status".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-8.0, 30.0))
        .interactable(false)
        .show(&ctx, |ui| {
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgba_premultiplied(20, 80, 40, bg_alpha))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&status.last_message)
                            .color(egui::Color32::from_rgba_premultiplied(
                                255, 255, 255, text_alpha,
                            ))
                            .size(14.0),
                    );
                });
        });
}

/// Panel de error que muestra el traceback completo del script.
///
/// Persiste hasta el próximo reload exitoso o hasta que el usuario lo cierre con `Esc` o el botón.
pub fn script_error_overlay_system(
    mut ctx: bevy_egui::EguiContexts,
    mut error: ResMut<ScriptError>,
) {
    let Some(msg) = error.message.clone() else {
        return;
    };
    let Ok(ctx) = ctx.ctx_mut() else {
        return;
    };

    // Esc cierra el panel
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        error.message = None;
        error.updated_at = None;
        return;
    }

    // Ventana centrada, con fondo oscuro y borde rojo (Foreground para no quedar detrás de toolbar)
    egui::Window::new("Error en script  •  Esc para cerrar")
        .id(egui::Id::new("script_error_window"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .resizable(true)
        .collapsible(false)
        .min_width(560.0)
        .min_height(220.0)
        .default_width(720.0)
        .default_height(380.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(28, 14, 14, 245))
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 60, 60)))
                .corner_radius(10.0)
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: egui::Color32::from_rgba_premultiplied(0, 0, 0, 120),
                })
                .inner_margin(egui::Margin::symmetric(10, 10)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Traceback")
                        .color(egui::Color32::from_rgb(255, 100, 100))
                        .strong()
                        .size(13.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(egui::RichText::new("Cerrar  Esc").size(11.0))
                        .on_hover_text("Descartar error (también Esc)")
                        .clicked()
                    {
                        error.message = None;
                        error.updated_at = None;
                        return;
                    }
                    if ui
                        .button(egui::RichText::new("Copiar").size(11.0))
                        .on_hover_text("Copiar traceback al portapapeles")
                        .clicked()
                    {
                        ctx.copy_text(msg.clone());
                    }
                });
            });
            ui.separator();
            // Area scrolleable con monoespaciado
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .stick_to_bottom(false)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut msg.clone())
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(14)
                            .lock_focus(true),
                    );
                });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Corrige el archivo y guarda — el editor reintentará automáticamente.",
                )
                .color(egui::Color32::from_rgb(180, 160, 160))
                .size(10.0)
                .italics(),
            );
        });
}
