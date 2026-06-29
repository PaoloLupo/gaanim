use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, egui, input::EguiWantsInput};
use gaanim_core::id::ObjectId;
use gaanim_core::peniko;
use gaanim_math::Camera;
use gaanim_scene::{
    FillBrush, GroupMarker, MobjectId, ObjectTag, Opacity, RenderOrder, StrokeBrush, WorldBounds,
};
use gaanim_timeline::timeline::Timeline;
use std::collections::{HashMap, HashSet};

use gaanim_animation::signals::FloatSignal;
use gaanim_animation::updaters::Updater;
use gaanim_api::DecimalNumber;

pub mod export;
mod fps_overlay;
mod timeline_widget;
mod vsync;

fn sync_editor_input_ignore_system(
    egui_wants: Res<EguiWantsInput>,
    mut timeline: ResMut<Timeline>,
) {
    timeline.ignore_input = egui_wants.wants_keyboard_input() || egui_wants.wants_any_pointer_input();
}

/// Pixel height of UI panels that the animation viewport must avoid.
///
/// Updated every frame by `editor_ui_system` and consumed by `viewport_adjust_system`
/// to scale + offset the Camera so the preview renders in the remaining space.
#[derive(Resource)]
pub struct ViewportInset {
    /// Space consumed by the bottom timeline panel (pixels).
    pub bottom: f32,
}

impl Default for ViewportInset {
    fn default() -> Self {
        Self { bottom: 0.0 }
    }
}

pub struct GaanimEditorPlugin;

impl Plugin for GaanimEditorPlugin {
    fn build(&self, app: &mut App) {
        #[allow(deprecated)]
        app.add_plugins(EguiPlugin {
            enable_multipass_for_primary_context: false,
            ..default()
        })
        .init_resource::<EditorState>()
        .init_resource::<export::ExportState>()
        .insert_resource(export::StashedReplay(None))
        .init_resource::<fps_overlay::FpsOverlay>()
        .init_resource::<vsync::VsyncState>()
        .init_resource::<ViewportInset>()
        .add_systems(
            Update,
            (
                sync_editor_input_ignore_system
                    .in_set(gaanim_scene::hierarchy::SceneSet::Input)
                    .before(gaanim_timeline::timeline_playback_system),
                editor_picking_system,
                fps_overlay::fps_overlay_system,
                vsync::vsync_toggle_system,
            ),
        )
        .add_systems(
            Update,
            viewport_adjust_system.after(gaanim_scene::hierarchy::SceneSet::Extraction),
        )
        .add_systems(EguiPrimaryContextPass, editor_ui_system)
        .add_systems(EguiPrimaryContextPass, export::export_dialog_system);
    }
}

#[derive(Resource)]
pub struct EditorState {
    pub selected: Option<Entity>,
    pub timeline_widget: timeline_widget::TimelineWidget,
    /// Whether the full timeline panel is visible. Defaults to `false` so the
    /// animation preview occupies the full window; the compact playback overlay
    /// is shown instead.
    pub timeline_visible: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected: None,
            timeline_widget: timeline_widget::TimelineWidget::new(),
            timeline_visible: false,
        }
    }
}

fn editor_ui_system(
    mut ctx: bevy_egui::EguiContexts,
    mut state: ResMut<EditorState>,
    mut export_state: ResMut<export::ExportState>,
    mut timeline: ResMut<Timeline>,
    mut inset: ResMut<ViewportInset>,
    camera: Option<Res<Camera>>,
    fps_overlay: Res<fps_overlay::FpsOverlay>,
    entity_query: Query<(Entity, Option<&MobjectId>, Option<&ObjectTag>)>,
    children_query: Query<&Children>,
    group_query: Query<&GroupMarker>,
    transform_query: Query<&gaanim_math::SpatialTransform>,
    fill_query: Query<&FillBrush>,
    stroke_query: Query<&StrokeBrush>,
    opacity_query: Query<&Opacity>,
    bounds_query: Query<&WorldBounds>,
    extra_query: Query<(
        Entity,
        Option<&MobjectId>,
        Option<&FloatSignal>,
        Option<&Updater>,
        Option<&DecimalNumber>,
    )>,
) {
    let Ok(ctx) = ctx.ctx_mut() else {
        return;
    };

    let is_exporting = export_state.active;
    let (export_progress_pct, export_current, export_total) = if is_exporting {
        if let Ok(lock) = export_state.progress_shared.lock() {
            if let Some(ref p) = *lock {
                let pct = if p.total_frames > 0 {
                    p.current_frame as f32 / p.total_frames as f32
                } else {
                    0.0
                };
                (pct, p.current_frame, p.total_frames)
            } else {
                (0.0, 0, 0)
            }
        } else {
            (0.0, 0, 0)
        }
    } else {
        (0.0, 0, 0)
    };

    let mut property_values: HashMap<ObjectId, timeline_widget::PropertyValues> = HashMap::new();
    for (entity, mobj_id, _) in &entity_query {
        let Some(oid) = mobj_id else {
            continue;
        };

        let pos = if let Ok(t) = transform_query.get(entity) {
            t.translation
        } else {
            glam::DVec3::ZERO
        };
        let scale = if let Ok(t) = transform_query.get(entity) {
            t.scale
        } else {
            glam::DVec3::ONE
        };
        let rotation_deg = if let Ok(t) = transform_query.get(entity) {
            2.0 * f64::atan2(t.rotation.z, t.rotation.w).to_degrees()
        } else {
            0.0
        };

        let fill_label = if let Ok(fb) = fill_query.get(entity) {
            brush_string(&fb.0)
        } else {
            "none".into()
        };

        let stroke_label = if let Ok(sb) = stroke_query.get(entity) {
            brush_string(&sb.brush)
        } else {
            "none".into()
        };

        let stroke_width = if let Ok(sb) = stroke_query.get(entity) {
            sb.style.width
        } else {
            0.0
        };

        let opacity = if let Ok(o) = opacity_query.get(entity) {
            o.0
        } else {
            1.0
        };

        property_values.insert(
            oid.0,
            timeline_widget::PropertyValues {
                pos_x: pos.x,
                pos_y: pos.y,
                pos_z: pos.z,
                scale_x: scale.x,
                scale_y: scale.y,
                scale_z: scale.z,
                rotation_deg,
                fill_label,
                stroke_label,
                stroke_width,
                opacity,
            },
        );
    }

    let mobject_to_track: HashMap<gaanim_core::id::ObjectId, gaanim_timeline::clip::TrackId> =
        timeline
            .tracks
            .iter()
            .filter_map(|(tid, t)| t.object_id.map(|oid| (oid, tid)))
            .collect();
    let mut group_children: HashMap<
        gaanim_timeline::clip::TrackId,
        Vec<gaanim_timeline::clip::TrackId>,
    > = HashMap::new();
    for (entity, mobj_id, _) in &entity_query {
        if !group_query.contains(entity) {
            continue;
        }
        let Some(group_oid) = mobj_id else { continue };
        let Some(&group_tid) = mobject_to_track.get(&group_oid.0) else {
            continue;
        };
        if let Ok(children) = children_query.get(entity) {
            let child_tids: Vec<gaanim_timeline::clip::TrackId> = children
                .iter()
                .filter_map(|child| {
                    entity_query
                        .get(child)
                        .ok()
                        .and_then(|(_, mid, _)| mid)
                        .and_then(|oid| mobject_to_track.get(&oid.0))
                        .copied()
                })
                .collect();
            if !child_tids.is_empty() {
                group_children.insert(group_tid, child_tids);
            }
        }
    }

    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Export...").clicked() {
                    export_state.dialog_open = true;
                    ui.close();
                }
            });

            if is_exporting {
                ui.add(
                    egui::ProgressBar::new(export_progress_pct)
                        .desired_width(140.0)
                        .text(format!("{:.0}%", export_progress_pct * 100.0)),
                );
                ui.label(format!("Frame {}/{}", export_current, export_total));
            } else if let Some(selected) = state.selected {
                let name = entity_query
                    .get(selected)
                    .ok()
                    .and_then(|(_, _, tag)| tag.map(|t| t.0.as_str()))
                    .unwrap_or("???");
                ui.label(format!("Selected: {name}"));
            } else {
                ui.label("Gaanim Editor");
            }
        });
    });

    let mut signal_values: HashMap<ObjectId, f64> = HashMap::new();
    let mut updater_entities: HashSet<ObjectId> = HashSet::new();
    let mut signal_by_entity: HashMap<Entity, f64> = HashMap::new();
    let mut decimal_values: HashMap<ObjectId, f64> = HashMap::new();
    for (entity, mobj_id, signal, updater, decimal) in &extra_query {
        let Some(mobj_id) = mobj_id else { continue };
        let oid = mobj_id.0;
        if let Some(signal) = signal {
            signal_values.insert(oid, signal.value);
            signal_by_entity.insert(entity, signal.value);
        }
        if updater.is_some() {
            updater_entities.insert(oid);
        }
        if let Some(decimal) = decimal {
            let val = signal_by_entity
                .get(&decimal.signal_entity)
                .copied()
                .or(decimal.last_value)
                .unwrap_or(0.0);
            decimal_values.insert(oid, val);
        }
    }

    // ── Full timeline panel (only when explicitly toggled on) ──────────────
    let timeline_response = if state.timeline_visible {
        Some(
            egui::TopBottomPanel::bottom("timeline")
                .resizable(true)
                .default_height(200.0)
                .min_height(100.0)
                .show(ctx, |ui| {
                    state.timeline_widget.show(
                        ui,
                        &mut timeline,
                        &property_values,
                        &group_children,
                        &signal_values,
                        &updater_entities,
                        &decimal_values,
                    );
                }),
        )
    } else {
        None
    };

    // Publish the timeline panel height so the renderer can shrink the viewport.
    let panel_h = timeline_response
        .as_ref()
        .map(|r| r.response.rect.height())
        .unwrap_or(0.0);
    inset.bottom = panel_h;

    // ── Compact playback overlay ──────────────────────────────────────────
    {
        let scene_name: String = timeline
            .scene_at(timeline.current_time)
            .and_then(|id| timeline.scenes.get(id))
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let total = timeline.cached_duration.max(0.0);
        let current = timeline.current_time.clamp(0.0, total);

        egui::Area::new("playback_overlay".into())
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -panel_h))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                let screen_w = ctx.viewport_rect().width();
                let bar_w = (screen_w * 0.70).min(900.0).max(400.0);

                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_premultiplied(18, 18, 24, 210))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::symmetric(14, 6))
                    .show(ui, |ui| {
                        ui.set_width(bar_w);
                        // Row 1: seekable progress bar
                        let frac = if total > 0.0 {
                            (current / total) as f32
                        } else {
                            0.0
                        };
                        // Use a draggable slider for a smooth seek bar.
                        let mut seek_frac = frac;
                        let slider = egui::Slider::new(&mut seek_frac, 0.0..=1.0)
                            .show_value(false)
                            .step_by(0.001);
                        let resp = ui.add(slider);
                        if resp.dragged() {
                            timeline.seek_request = Some(seek_frac as f64 * total);
                        }
                        // Row 2: controls
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(scene_name)
                                    .color(egui::Color32::from_rgb(160, 200, 255))
                                    .strong()
                                    .small(),
                            );
                            ui.add_space(8.0);
                            let sym = if timeline.is_playing { "⏸" } else { "▶" };
                            if ui.button(egui::RichText::new(sym).size(16.0)).clicked() {
                                timeline.is_playing = !timeline.is_playing;
                            }
                            ui.monospace(format!(
                                "{} / {}",
                                format_time(current),
                                format_time(total),
                            ));
                            ui.add_space(4.0);
                            let loop_on = timeline.loop_range.is_some();
                            let lc = if loop_on {
                                egui::Color32::from_rgb(100, 200, 140)
                            } else {
                                egui::Color32::GRAY
                            };
                            if ui
                                .button(egui::RichText::new("🔁").color(lc).size(14.0))
                                .clicked()
                            {
                                if loop_on {
                                    timeline.loop_range = None;
                                } else {
                                    timeline.loop_range =
                                        Some((0.0, timeline.cached_duration));
                                }
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let lbl = if state.timeline_visible {
                                        "✕ Close"
                                    } else {
                                        "☰ Timeline"
                                    };
                                    if ui
                                        .button(
                                            egui::RichText::new(lbl)
                                                .small()
                                                .color(egui::Color32::LIGHT_GRAY),
                                        )
                                        .clicked()
                                    {
                                        state.timeline_visible = !state.timeline_visible;
                                    }
                                },
                            );
                        });
                    });
            });
    }

    if let Some(track_id) = state.timeline_widget.selected_track {
        if let Some(track) = timeline.tracks.get(track_id)
            && let Some(obj_id) = track.object_id
        {
            for (entity, mobj_id, _) in &entity_query {
                if let Some(mid) = mobj_id
                    && mid.0 == obj_id
                {
                    state.selected = Some(entity);
                    break;
                }
            }
        }
        state.timeline_widget.selected_track = None;
    }

    if let Some(selected) = state.selected
        && let Some(camera) = camera.as_ref()
        && let Ok(bounds) = bounds_query.get(selected)
    {
        let corners = [
            glam::DVec3::new(bounds.0.min.x, bounds.0.min.y, 0.0),
            glam::DVec3::new(bounds.0.max.x, bounds.0.min.y, 0.0),
            glam::DVec3::new(bounds.0.max.x, bounds.0.max.y, 0.0),
            glam::DVec3::new(bounds.0.min.x, bounds.0.max.y, 0.0),
        ];

        let screen: Vec<egui::Pos2> = corners
            .iter()
            .map(|c| {
                let s = camera.world_to_screen(*c);
                egui::Pos2::new(s.x as f32, s.y as f32)
            })
            .collect();

        let color = egui::Color32::from_rgba_premultiplied(68, 160, 255, 180);
        let stroke = egui::Stroke::new(2.0, color);
        egui::Area::new("viewport_selection".into())
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(false)
            .show(ctx, |ui| {
                let vp = ctx.viewport_rect();
                let _ = ui.allocate_space(vp.size());
                let p = ui.painter();
                for i in 0..4 {
                    p.line_segment([screen[i], screen[(i + 1) % 4]], stroke);
                }
                let cs = 6.0;
                for &c in &screen {
                    p.line_segment(
                        [
                            egui::Pos2::new(c.x - cs, c.y),
                            egui::Pos2::new(c.x + cs, c.y),
                        ],
                        stroke,
                    );
                    p.line_segment(
                        [
                            egui::Pos2::new(c.x, c.y - cs),
                            egui::Pos2::new(c.x, c.y + cs),
                        ],
                        stroke,
                    );
                }
            });
    }

    fps_overlay.render(ctx);
}

fn brush_string(brush: &Option<peniko::Brush>) -> String {
    match brush {
        Some(peniko::Brush::Solid(color)) => {
            let rgba = color.to_rgba8();
            format!("#{:02X}{:02X}{:02X}{:02X}", rgba.r, rgba.g, rgba.b, rgba.a)
        }
        Some(peniko::Brush::Gradient(_)) => "<gradient>".into(),
        Some(peniko::Brush::Image(_)) => "<image>".into(),
        None => "none".into(),
    }
}

/// Format seconds as `M:SS.ss` for the playback overlay.
fn format_time(seconds: f64) -> String {
    if seconds < 0.0 {
        return "0:00.00".into();
    }
    let total_cs = (seconds * 100.0).round() as u64;
    let mins = total_cs / 6000;
    let secs = (total_cs % 6000) / 100;
    let cs = total_cs % 100;
    format!("{}:{:02}.{:02}", mins, secs, cs)
}

fn editor_picking_system(
    egui_wants: Res<EguiWantsInput>,
    camera: Option<Res<Camera>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    entities: Query<(Entity, &WorldBounds, Option<&RenderOrder>)>,
    mut state: ResMut<EditorState>,
) {
    let Some(camera) = camera else { return };
    if egui_wants.wants_any_pointer_input() {
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let world_pos =
        camera.screen_to_world(glam::DVec2::new(cursor_pos.x as f64, cursor_pos.y as f64));

    let mut best_z = i32::MIN;
    let mut best_entity: Option<Entity> = None;

    for (entity, bounds, render_order) in &entities {
        if bounds
            .0
            .contains(glam::DVec3::new(world_pos.x, world_pos.y, 0.0))
        {
            let z = render_order.map(|ro| ro.z_index).unwrap_or(0);
            if z >= best_z {
                best_z = z;
                best_entity = Some(entity);
            }
        }
    }

    state.selected = best_entity;
}

/// System: adjusts the [`Camera`] resource so the animation preview fits in the
/// area above UI panels (the timeline at the bottom).
///
/// Runs after the Vello extraction phase so the transform is ready before
/// `bevy_vello` submits the scene to the GPU.
fn viewport_adjust_system(
    inset: Res<ViewportInset>,
    mut camera: Option<ResMut<Camera>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(ref mut cam) = camera else { return };
    let Ok(window) = windows.single() else { return };

    let window_h = window.height() as f64;
    let window_w = window.width() as f64;
    let available_h = window_h - inset.bottom as f64;

    if inset.bottom < 1.0 || available_h < 1.0 {
        cam.viewport_offset_y = 0.0;
        cam.viewport_scale = 1.0;
        return;
    }

    // Fit animation into the available area while preserving aspect ratio.
    let anim_w = cam.viewport_width as f64;
    let anim_h = cam.viewport_height as f64;
    let scale_x = window_w / anim_w;
    let scale_y = available_h / anim_h;
    cam.viewport_scale = scale_x.min(scale_y);

    // Shift the Vello centre upward so the animation sits above the timeline.
    cam.viewport_offset_y = -(inset.bottom as f64) / 2.0;
}
