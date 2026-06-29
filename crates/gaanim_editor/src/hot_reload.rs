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

/// Despawn every mobject + camera/VelloView entity and reset the [`Timeline`]
/// to a clean state. This is the "clear canvas" step before replaying fresh ops.
pub fn clear_scene_entities(world: &mut World) {
    let mut to_despawn: Vec<Entity> = {
        let mut q = world.query::<(Entity, &MobjectId)>();
        q.iter(world).map(|(e, _)| e).collect()
    };
    // Despawn the old Camera2d (replay_into spawns a fresh one), but
    // keep VelloView alive — it owns the window surface.
    {
        let mut q = world.query_filtered::<Entity, With<Camera2d>>();
        to_despawn.extend(q.iter(world));
    }
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

    reload_with(world, payload.ops, payload.width, payload.height, payload.background);

    if let Some(mut status) = world.get_resource_mut::<ReloadStatus>() {
        status.last_message = format!("Reloaded scene ({}x{})", payload.width, payload.height);
    }
}

/// Rebuild the scene in `world` from a fresh set of ops, then recapture the
/// t=0 keyframe.
pub fn reload_with(
    world: &mut World,
    ops: Vec<DeferredOp>,
    width: u32,
    height: u32,
    background: Option<gaanim_core::peniko::Color>,
) {
    clear_scene_entities(world);
    runtime::replay_into(world, ops, width, height, background);
    gaanim_timeline::capture_initial_keyframe(world);
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
