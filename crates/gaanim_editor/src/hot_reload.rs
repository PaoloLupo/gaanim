//! Hot-reload infrastructure for the Gaanim host.

use bevy::prelude::*;
use bevy_egui::egui;
use crossbeam_channel::Receiver;
use gaanim_api::host::ReloadPayload;
use gaanim_api::runtime;

use gaanim_editor::export::StashedReplay;
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
    pub compile_duration_seconds: Option<f64>,
    /// `Some(seconds_since_startup)` when the message was set.
    pub shown_at: Option<f64>,
}

impl Default for ReloadStatus {
    fn default() -> Self {
        Self {
            last_message: String::new(),
            compile_duration_seconds: None,
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
    reload_with(world, payload.canvas);

    // Restore playback position after rebuild.
    if let Some(mut tl) = world.get_resource_mut::<Timeline>() {
        let target = reload_target_time(saved_time, &tl);
        tl.seek_request = Some(target);
        tl.is_playing = if had_previous_timeline {
            was_playing
        } else {
            tl.cached_duration > 0.0
        };
    }

    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut status) = world.get_resource_mut::<ReloadStatus>() {
        status.compile_duration_seconds = Some(compile_duration);
        status.last_message = format!(
            "Hot reload · {:.2}s · {}x{}",
            compile_duration, width, height
        );
        status.shown_at = Some(now);
    }
    // Éxito limpia el error previo
    if let Some(mut err) = world.get_resource_mut::<ScriptError>() {
        err.message = None;
        err.updated_at = None;
    }
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
    // Defer keyframe capture to the next frame so that deferred Commands
    // (entity spawns, SceneMember inserts, etc.) are flushed first.
    world.insert_resource(gaanim_timeline::NeedsKeyframeCapture);
}

#[cfg(test)]
mod tests {
    use super::*;
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
