//! Hot-reload infrastructure for the Gaanim host.

use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_python::host::ReloadPayload;
use gaanim_python::runtime;
use gaanim_python::DeferredOp;
use gaanim_scene::MobjectId;
use gaanim_timeline::timeline::Timeline;
use crossbeam_channel::Receiver;

/// Bevy resource holding the receiver end of the host channel.
#[derive(Resource)]
pub struct ReloadReceiver {
    pub rx: Receiver<ReloadPayload>,
}

/// Optional human-readable status line shown in the editor.
#[derive(Resource, Default)]
pub struct ReloadStatus {
    pub last_message: String,
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
        world.despawn(e);
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
    let (saved_time, was_playing) = {
        let tl = world.resource::<Timeline>();
        (tl.current_time, tl.is_playing)
    };

    reload_with(world, payload.ops, payload.width, payload.height, payload.background);

    // Restore playback position after rebuild.
    if let Some(mut tl) = world.get_resource_mut::<Timeline>() {
        let target = saved_time.min(tl.cached_duration.max(0.0));
        tl.seek_request = Some(target);
        tl.is_playing = was_playing;
    }

    if let Some(mut status) = world.get_resource_mut::<ReloadStatus>() {
        status.last_message = format!("Reloaded scene ({}x{})", payload.width, payload.height);
    }
}

/// Rebuild the scene in `world` from a fresh set of ops, then schedule the
/// t=0 keyframe capture for the next frame (after deferred Commands flush).
pub fn reload_with(
    world: &mut World,
    ops: Vec<DeferredOp>,
    width: u32,
    height: u32,
    background: Option<gaanim_core::peniko::Color>,
) {
    clear_scene_entities(world);
    runtime::replay_into(world, ops, width, height, background);
    // Defer keyframe capture to the next frame so that deferred Commands
    // (entity spawns, SceneMember inserts, etc.) are flushed first.
    world.insert_resource(gaanim_timeline::NeedsKeyframeCapture);
}

/// egui panel showing the last reload status.
pub fn reload_status_overlay_system(
    mut ctx: bevy_egui::EguiContexts,
    status: Res<ReloadStatus>,
) {
    if status.last_message.is_empty() {
        return;
    }
    let Ok(ctx) = ctx.ctx_mut() else {
        return;
    };
    egui::Area::new("reload_status".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-8.0, 30.0))
        .interactable(false)
        .show(&ctx, |ui| {
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgba_premultiplied(20, 80, 40, 200))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&status.last_message)
                            .color(egui::Color32::WHITE)
                            .small(),
                    );
                });
        });
}
