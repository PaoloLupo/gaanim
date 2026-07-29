//! Hot-reload infrastructure for the Gaanim host.

use bevy::prelude::*;
use bevy_egui::egui;
use crossbeam_channel::Receiver;
use gaanim_api::host::ReloadPayload;
use gaanim_api::runtime;

use crate::export::StashedReplay;
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
    /// `Some(seconds_since_startup)` when the message was set.
    pub shown_at: Option<f64>,
}

impl Default for ReloadStatus {
    fn default() -> Self {
        Self {
            last_message: String::new(),
            shown_at: None,
        }
    }
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
    let (saved_time, saved_presentation_position, was_playing, had_previous_timeline) = {
        let tl = world.resource::<Timeline>();
        (
            tl.current_time,
            tl.presentation_position,
            tl.is_playing,
            tl.cached_duration > 0.0,
        )
    };

    let width = payload.canvas.width;
    let height = payload.canvas.height;
    reload_with(world, payload.canvas);

    // Restore playback position after rebuild.
    if let Some(mut tl) = world.get_resource_mut::<Timeline>() {
        let target = saved_presentation_position
            .and_then(|position| tl.presentation_time(position))
            .unwrap_or_else(|| saved_time.min(tl.cached_duration.max(0.0)));
        tl.seek_request = Some(target);
        tl.is_playing = if had_previous_timeline {
            was_playing
        } else {
            tl.cached_duration > 0.0
        };
    }

    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut status) = world.get_resource_mut::<ReloadStatus>() {
        status.last_message = format!("Reloaded scene ({}x{})", width, height);
        status.shown_at = Some(now);
    }
}

/// Rebuild the scene in `world` from a fresh set of ops, then schedule the
/// t=0 keyframe capture for the next frame (after deferred Commands flush).
pub fn reload_with(world: &mut World, canvas: gaanim_api::canvas::Canvas) {
    clear_scene_entities(world);
    world.insert_resource(StashedReplay(Some(canvas.clone())));
    runtime::replay_canvas_into(world, canvas);
    // Defer keyframe capture to the next frame so that deferred Commands
    // (entity spawns, SceneMember inserts, etc.) are flushed first.
    world.insert_resource(gaanim_timeline::NeedsKeyframeCapture);
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
                            .small(),
                    );
                });
        });
}
