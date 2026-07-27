use bevy::ecs::system::SystemParam;
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
    timeline.ignore_input =
        egui_wants.wants_keyboard_input() || egui_wants.wants_any_pointer_input();
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
                global_playback_keys_system,
                fps_overlay::fps_overlay_system,
                vsync::vsync_toggle_system,
            ),
        )
        .add_systems(
            Update,
            viewport_adjust_system.before(gaanim_scene::hierarchy::SceneSet::Bounds),
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
    /// Whether the window is pinned always-on-top.
    pub pinned_on_top: bool,
    /// Hover time on the seek bar (in seconds), used for tooltip display.
    seek_bar_hover: Option<f64>,
    /// Auto-hide animation progress (0.0 = hidden, 1.0 = fully visible).
    bar_visibility: f32,
    /// Whether the cursor is currently hovering the playback bar.
    bar_hovered: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected: None,
            timeline_widget: timeline_widget::TimelineWidget::new(),
            timeline_visible: false,
            pinned_on_top: false,
            seek_bar_hover: None,
            bar_visibility: 1.0, // start visible
            bar_hovered: false,
        }
    }
}

/// Bundle of read-only queries used by the editor UI, kept separate to avoid
/// exceeding Bevy's 16-parameter system limit.
#[derive(SystemParam)]
struct EditorQueries<'w, 's> {
    entity: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static MobjectId>,
            Option<&'static ObjectTag>,
        ),
    >,
    children: Query<'w, 's, &'static Children>,
    group: Query<'w, 's, &'static GroupMarker>,
    transform: Query<'w, 's, &'static gaanim_math::SpatialTransform>,
    fill: Query<'w, 's, &'static FillBrush>,
    stroke: Query<'w, 's, &'static StrokeBrush>,
    opacity: Query<'w, 's, &'static Opacity>,
    bounds: Query<'w, 's, &'static WorldBounds>,
    extra: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static MobjectId>,
            Option<&'static FloatSignal>,
            Option<&'static Updater>,
            Option<&'static DecimalNumber>,
        ),
    >,
}

fn editor_ui_system(
    mut ctx: bevy_egui::EguiContexts,
    mut state: ResMut<EditorState>,
    mut export_state: ResMut<export::ExportState>,
    mut timeline: ResMut<Timeline>,
    mut inset: ResMut<ViewportInset>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    camera: Option<Res<Camera>>,
    fps_overlay: Res<fps_overlay::FpsOverlay>,
    q: EditorQueries,
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
    for (entity, mobj_id, _) in &q.entity {
        let Some(oid) = mobj_id else {
            continue;
        };

        let pos = if let Ok(t) = q.transform.get(entity) {
            t.translation
        } else {
            glam::DVec3::ZERO
        };
        let scale = if let Ok(t) = q.transform.get(entity) {
            t.scale
        } else {
            glam::DVec3::ONE
        };
        let rotation_deg = if let Ok(t) = q.transform.get(entity) {
            2.0 * f64::atan2(t.rotation.z, t.rotation.w).to_degrees()
        } else {
            0.0
        };

        let fill_label = if let Ok(fb) = q.fill.get(entity) {
            brush_string(&fb.0)
        } else {
            "none".into()
        };

        let stroke_label = if let Ok(sb) = q.stroke.get(entity) {
            brush_string(&sb.brush)
        } else {
            "none".into()
        };

        let stroke_width = if let Ok(sb) = q.stroke.get(entity) {
            sb.style.width
        } else {
            0.0
        };

        let opacity = if let Ok(o) = q.opacity.get(entity) {
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
    for (entity, mobj_id, _) in &q.entity {
        if !q.group.contains(entity) {
            continue;
        }
        let Some(group_oid) = mobj_id else { continue };
        let Some(&group_tid) = mobject_to_track.get(&group_oid.0) else {
            continue;
        };
        if let Ok(children) = q.children.get(entity) {
            let child_tids: Vec<gaanim_timeline::clip::TrackId> = children
                .iter()
                .filter_map(|child| {
                    q.entity
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

    // Top bar removed — export button lives in the playback controls now.

    let mut signal_values: HashMap<ObjectId, f64> = HashMap::new();
    let mut updater_entities: HashSet<ObjectId> = HashSet::new();
    let mut signal_by_entity: HashMap<Entity, f64> = HashMap::new();
    let mut decimal_values: HashMap<ObjectId, f64> = HashMap::new();
    for (entity, mobj_id, signal, updater, decimal) in &q.extra {
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

    // ── Compact playback overlay (auto-hide) ──────────────────────────────
    {
        let scene_name: String = timeline
            .scene_at(timeline.current_time)
            .and_then(|id| timeline.scenes.get(id))
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let presentation_name = timeline.presentation_label();
        let total = timeline.cached_duration.max(0.0);
        let current = timeline.current_time.clamp(0.0, total);

        // Auto-hide logic: show when cursor is near the bottom edge.
        let vp = ctx.viewport_rect();
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let pointer_near_bottom = pointer.map(|p| p.y > vp.height() - 90.0).unwrap_or(false);
        let should_show = pointer_near_bottom || state.bar_hovered;
        let dt = ctx.input(|i| i.unstable_dt);
        let target_vis = if should_show { 1.0_f32 } else { 0.0_f32 };
        let speed = if should_show { 8.0_f32 } else { 3.0_f32 };
        state.bar_visibility += (target_vis - state.bar_visibility) * (speed * dt).min(1.0);
        if (state.bar_visibility - target_vis).abs() < 0.01 {
            state.bar_visibility = target_vis;
        }
        let vis = state.bar_visibility;

        if vis > 0.01 {
            let slide_offset = (1.0 - vis) * 30.0;
            let alpha_mul = vis;

            let area_resp = egui::Area::new("playback_overlay".into())
                .anchor(
                    egui::Align2::CENTER_BOTTOM,
                    egui::vec2(0.0, -panel_h + slide_offset),
                )
                .order(egui::Order::Foreground)
                .interactable(vis > 0.5)
                .show(ctx, |ui| {
                    let screen_w = vp.width();
                    let bar_w = (screen_w * 0.70).min(900.0).max(400.0);

                    let fill_alpha = (220.0 * alpha_mul) as u8;
                    let stroke_alpha = (100.0 * alpha_mul) as u8;
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_premultiplied(
                            12, 12, 18, fill_alpha,
                        ))
                        .corner_radius(12.0)
                        .inner_margin(egui::Margin {
                            left: 16,
                            right: 16,
                            top: 20,
                            bottom: 8,
                        })
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_premultiplied(60, 60, 80, stroke_alpha),
                        ))
                        .show(ui, |ui| {
                            ui.set_width(bar_w);

                            // Row 1: custom seek bar
                            let frac = if total > 0.0 {
                                (current / total) as f32
                            } else {
                                0.0
                            };
                            let total_f32 = total.max(0.001) as f32;
                            let loop_frac = timeline
                                .loop_range
                                .map(|(s, e)| (s as f32 / total_f32, e as f32 / total_f32));
                            let bp_fracs: Vec<f32> = timeline
                                .breakpoints
                                .iter()
                                .map(|&bp| bp as f32 / total_f32)
                                .collect();

                            let scene_segs: Vec<SceneSegment> = timeline
                                .scene_index
                                .iter()
                                .filter_map(|(&_start_time, &scene_id)| {
                                    let (s, e) = timeline.scene_bounds(scene_id)?;
                                    let name = timeline.scenes.get(scene_id)?.name.clone();
                                    Some(SceneSegment {
                                        name,
                                        start_frac: (s as f32 / total_f32).clamp(0.0, 1.0),
                                        end_frac: (e as f32 / total_f32).clamp(0.0, 1.0),
                                    })
                                })
                                .collect();

                            let seek_resp =
                                paint_seek_bar(ui, frac, loop_frac, &bp_fracs, &scene_segs, total);
                            if let Some(new_frac) = seek_resp.seek_to {
                                timeline.seek_request = Some(new_frac as f64 * total);
                            }
                            state.seek_bar_hover = seek_resp.hover_time;

                            // Row 2: controls
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                // Presentation position takes precedence over the internal scene name.
                                if let Some(presentation_name) = &presentation_name {
                                    let display = truncate_with_ellipsis(presentation_name, 28);
                                    ui.label(
                                        egui::RichText::new(display)
                                            .color(egui::Color32::from_rgb(160, 200, 255))
                                            .strong()
                                            .small(),
                                    );
                                    ui.add_space(8.0);
                                } else if !scene_name.is_empty() {
                                    let display = truncate_with_ellipsis(&scene_name, 20);
                                    ui.label(
                                        egui::RichText::new(display)
                                            .color(egui::Color32::from_rgb(160, 200, 255))
                                            .strong()
                                            .small(),
                                    );
                                    ui.add_space(8.0);
                                }

                                // Skip to start
                                transport_button(ui, "⏮", || {
                                    timeline.is_playing = false;
                                    timeline.seek_request = Some(0.0);
                                });

                                // Play / Pause
                                let play_sym = if timeline.is_playing { "⏸" } else { "▶" };
                                let play_btn = egui::Button::new(
                                    egui::RichText::new(play_sym).size(15.0).color(
                                        if timeline.is_playing {
                                            egui::Color32::from_rgb(120, 200, 255)
                                        } else {
                                            egui::Color32::from_rgb(200, 200, 210)
                                        },
                                    ),
                                )
                                .min_size(egui::vec2(30.0, 22.0))
                                .corner_radius(6.0)
                                .fill(egui::Color32::from_rgba_premultiplied(40, 40, 55, 180));
                                if ui.add(play_btn).clicked() {
                                    timeline.is_playing = !timeline.is_playing;
                                }

                                // Skip to end
                                transport_button(ui, "⏭", || {
                                    timeline.is_playing = false;
                                    timeline.seek_request = Some(total);
                                });

                                // Prev / Next scene
                                if !scene_segs.is_empty() {
                                    ui.add_space(2.0);
                                    let cur_scene_idx = scene_segs.iter().position(|s| {
                                        frac >= s.start_frac && frac < s.end_frac + 0.005
                                    });
                                    let has_prev = cur_scene_idx.map_or(false, |i| i > 0);
                                    let has_next =
                                        cur_scene_idx.map_or(false, |i| i + 1 < scene_segs.len());
                                    let before_first =
                                        !scene_segs.is_empty() && frac < scene_segs[0].start_frac;
                                    let after_last = !scene_segs.is_empty()
                                        && frac >= scene_segs.last().unwrap().end_frac - 0.005;

                                    // ◀ prev scene
                                    let prev_color = if has_prev || after_last {
                                        egui::Color32::from_rgb(170, 170, 180)
                                    } else {
                                        egui::Color32::from_rgb(70, 70, 80)
                                    };
                                    let prev_btn = egui::Button::new(
                                        egui::RichText::new("◀").size(10.0).color(prev_color),
                                    )
                                    .min_size(egui::vec2(18.0, 20.0))
                                    .corner_radius(4.0)
                                    .fill(egui::Color32::from_rgba_premultiplied(35, 35, 50, 140));
                                    if ui.add(prev_btn).on_hover_text("Previous scene").clicked() {
                                        let target = if after_last {
                                            scene_segs.last().map(|s| s.start_frac as f64 * total)
                                        } else if let Some(idx) = cur_scene_idx {
                                            if idx > 0 {
                                                Some(scene_segs[idx - 1].start_frac as f64 * total)
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };
                                        if let Some(t) = target {
                                            timeline.seek_request = Some(t);
                                        }
                                    }

                                    // ▶ next scene
                                    let next_color = if has_next || before_first {
                                        egui::Color32::from_rgb(170, 170, 180)
                                    } else {
                                        egui::Color32::from_rgb(70, 70, 80)
                                    };
                                    let next_btn = egui::Button::new(
                                        egui::RichText::new("▶").size(10.0).color(next_color),
                                    )
                                    .min_size(egui::vec2(18.0, 20.0))
                                    .corner_radius(4.0)
                                    .fill(egui::Color32::from_rgba_premultiplied(35, 35, 50, 140));
                                    if ui.add(next_btn).on_hover_text("Next scene").clicked() {
                                        let target = if before_first {
                                            Some(scene_segs[0].start_frac as f64 * total)
                                        } else if let Some(idx) = cur_scene_idx {
                                            if idx + 1 < scene_segs.len() {
                                                Some(scene_segs[idx + 1].start_frac as f64 * total)
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };
                                        if let Some(t) = target {
                                            timeline.seek_request = Some(t);
                                        }
                                    }
                                }

                                ui.add_space(6.0);

                                // Speed control
                                let speed = timeline.playback_rate;
                                let speed_label = if speed == 1.0 {
                                    "1x".to_string()
                                } else if speed < 1.0 {
                                    format!("{:.2}x", speed)
                                } else {
                                    format!("{:.1}x", speed)
                                };
                                let speed_color = if (speed - 1.0).abs() < f64::EPSILON {
                                    egui::Color32::from_rgb(170, 170, 180)
                                } else {
                                    egui::Color32::from_rgb(255, 200, 80)
                                };
                                let speed_btn = egui::Button::new(
                                    egui::RichText::new(format!("⚡ {}", speed_label))
                                        .size(11.0)
                                        .color(speed_color),
                                )
                                .min_size(egui::vec2(50.0, 20.0))
                                .corner_radius(4.0)
                                .fill(egui::Color32::from_rgba_premultiplied(35, 35, 50, 160));
                                let speed_resp = ui.add(speed_btn);
                                egui::Popup::from_toggle_button_response(&speed_resp)
                                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                    .show(|ui| {
                                        ui.set_min_width(200.0);
                                        ui.label(
                                            egui::RichText::new("Playback Speed")
                                                .size(11.0)
                                                .color(egui::Color32::from_rgb(160, 160, 175)),
                                        );
                                        ui.add_space(4.0);

                                        // Preset buttons
                                        let presets = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0];
                                        ui.horizontal(|ui| {
                                            for &p in &presets {
                                                let is_active = (speed - p).abs() < f64::EPSILON;
                                                let label = if p == p.floor() {
                                                    format!("{}x", p as i32)
                                                } else {
                                                    format!("{}x", p)
                                                };
                                                let btn = egui::Button::new(
                                                    egui::RichText::new(label).size(11.0).color(
                                                        if is_active {
                                                            egui::Color32::from_rgb(120, 200, 255)
                                                        } else {
                                                            egui::Color32::from_rgb(170, 170, 180)
                                                        },
                                                    ),
                                                )
                                                .min_size(egui::vec2(32.0, 22.0))
                                                .corner_radius(4.0)
                                                .fill(if is_active {
                                                    egui::Color32::from_rgba_premultiplied(
                                                        50, 70, 120, 200,
                                                    )
                                                } else {
                                                    egui::Color32::from_rgba_premultiplied(
                                                        35, 35, 50, 160,
                                                    )
                                                });
                                                if ui.add(btn).clicked() {
                                                    timeline.playback_rate = p;
                                                }
                                            }
                                        });

                                        ui.add_space(6.0);

                                        // Fine slider
                                        let mut rate = speed as f32;
                                        let slider = egui::Slider::new(&mut rate, 0.1..=5.0)
                                            .step_by(0.05)
                                            .text("x")
                                            .show_value(true);
                                        if ui.add(slider).changed() {
                                            timeline.playback_rate = rate as f64;
                                        }

                                        ui.add_space(2.0);
                                        if ui
                                            .small_button("Reset")
                                            .on_hover_text("Reset to 1x")
                                            .clicked()
                                        {
                                            timeline.playback_rate = 1.0;
                                        }
                                    });

                                ui.add_space(6.0);

                                // Time display
                                ui.monospace(
                                    egui::RichText::new(format!(
                                        "{} / {}",
                                        format_time(current),
                                        format_time(total),
                                    ))
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(180, 180, 190)),
                                );

                                ui.add_space(4.0);

                                // Loop toggle
                                let loop_on = timeline.loop_range.is_some();
                                let loop_color = if loop_on {
                                    egui::Color32::from_rgb(100, 200, 140)
                                } else {
                                    egui::Color32::from_rgb(100, 100, 110)
                                };
                                let loop_btn = egui::Button::new(
                                    egui::RichText::new("🔁").size(13.0).color(loop_color),
                                )
                                .min_size(egui::vec2(24.0, 20.0))
                                .corner_radius(4.0)
                                .fill(egui::Color32::TRANSPARENT);
                                if ui.add(loop_btn).on_hover_text("Loop").clicked() {
                                    if loop_on {
                                        timeline.loop_range = None;
                                    } else {
                                        timeline.loop_range = Some((0.0, timeline.cached_duration));
                                    }
                                }

                                // Right-aligned controls: timeline toggle + export + pin
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        // Timeline toggle
                                        let (lbl, icon) = if state.timeline_visible {
                                            ("Close", "✕")
                                        } else {
                                            ("Timeline", "☰")
                                        };
                                        let tl_btn = egui::Button::new(
                                            egui::RichText::new(format!("{} {}", icon, lbl))
                                                .size(11.0)
                                                .color(egui::Color32::from_rgb(150, 150, 165)),
                                        )
                                        .min_size(egui::vec2(70.0, 20.0))
                                        .corner_radius(4.0)
                                        .fill(egui::Color32::from_rgba_premultiplied(
                                            30, 30, 45, 140,
                                        ));
                                        if ui.add(tl_btn).clicked() {
                                            state.timeline_visible = !state.timeline_visible;
                                        }

                                        // Export button / progress
                                        if is_exporting {
                                            let pct_text =
                                                format!("{:.0}%", export_progress_pct * 100.0);
                                            ui.add(
                                                egui::ProgressBar::new(export_progress_pct)
                                                    .desired_width(80.0)
                                                    .text(pct_text),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{}/{}",
                                                    export_current, export_total
                                                ))
                                                .size(10.0)
                                                .color(egui::Color32::from_rgb(160, 160, 170)),
                                            );
                                        } else {
                                            let export_btn = egui::Button::new(
                                                egui::RichText::new("⬇ Export")
                                                    .size(11.0)
                                                    .color(egui::Color32::from_rgb(150, 200, 255)),
                                            )
                                            .min_size(egui::vec2(60.0, 20.0))
                                            .corner_radius(4.0)
                                            .fill(egui::Color32::from_rgba_premultiplied(
                                                30, 30, 55, 140,
                                            ));
                                            if ui
                                                .add(export_btn)
                                                .on_hover_text("Export animation")
                                                .clicked()
                                            {
                                                export_state.dialog_open = true;
                                            }
                                        }

                                        // Always-on-top pin toggle
                                        let pin_icon =
                                            if state.pinned_on_top { "📌" } else { "📍" };
                                        let pin_color = if state.pinned_on_top {
                                            egui::Color32::from_rgb(255, 200, 80)
                                        } else {
                                            egui::Color32::from_rgb(120, 120, 130)
                                        };
                                        let pin_btn = egui::Button::new(
                                            egui::RichText::new(pin_icon)
                                                .size(13.0)
                                                .color(pin_color),
                                        )
                                        .min_size(egui::vec2(24.0, 20.0))
                                        .corner_radius(4.0)
                                        .fill(egui::Color32::TRANSPARENT);
                                        if ui
                                            .add(pin_btn)
                                            .on_hover_text(if state.pinned_on_top {
                                                "Unpin window"
                                            } else {
                                                "Pin window on top"
                                            })
                                            .clicked()
                                        {
                                            state.pinned_on_top = !state.pinned_on_top;
                                            if let Ok(mut window) = windows.single_mut() {
                                                window.window_level = if state.pinned_on_top {
                                                    bevy::window::WindowLevel::AlwaysOnTop
                                                } else {
                                                    bevy::window::WindowLevel::Normal
                                                };
                                            }
                                        }
                                    },
                                );
                            });
                        });
                });
            // Keep bar visible while the pointer is inside the overlay area.
            state.bar_hovered = area_resp
                .response
                .hover_pos()
                .is_some_and(|p| area_resp.response.rect.contains(p));
        }
    }

    if let Some(track_id) = state.timeline_widget.selected_track {
        if let Some(track) = timeline.tracks.get(track_id)
            && let Some(obj_id) = track.object_id
        {
            for (entity, mobj_id, _) in &q.entity {
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
        && let Ok(bounds) = q.bounds.get(selected)
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

/// A scene's time range, precomputed for the seek bar.
struct SceneSegment {
    name: String,
    start_frac: f32,
    end_frac: f32,
}

/// Result from painting the custom seek bar.
struct SeekBarResponse {
    /// If Some, the user wants to seek to this fraction (0.0..=1.0).
    seek_to: Option<f32>,
    /// The time at the hover position, for tooltip display.
    hover_time: Option<f64>,
}

/// Paint a custom seek bar with progress fill, loop region, breakpoint markers,
/// scene sections, playhead handle, and hover time tooltip.
fn paint_seek_bar(
    ui: &mut egui::Ui,
    frac: f32,
    loop_frac: Option<(f32, f32)>,
    bp_fracs: &[f32],
    scenes: &[SceneSegment],
    total: f64,
) -> SeekBarResponse {
    let bar_h = 6.0_f32;
    let handle_r = 6.0_f32;
    let desired_h = 20.0_f32; // generous hit area

    let (response, painter) = ui.allocate_painter(
        egui::vec2(ui.available_width(), desired_h),
        egui::Sense::click_and_drag(),
    );
    let rect = response.rect;
    let bar_y = rect.center().y;
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, bar_y - bar_h / 2.0),
        egui::pos2(rect.max.x, bar_y + bar_h / 2.0),
    );

    // Colors
    let track_color = egui::Color32::from_rgba_premultiplied(50, 50, 60, 200);
    let fill_color = egui::Color32::from_rgb(90, 150, 255);
    let fill_color_light = egui::Color32::from_rgb(130, 180, 255);
    let loop_color = egui::Color32::from_rgba_premultiplied(80, 180, 120, 50);
    let bp_color = egui::Color32::from_rgb(255, 200, 60);
    let handle_color = egui::Color32::from_rgb(220, 220, 230);
    let handle_active = egui::Color32::from_rgb(120, 180, 255);

    // Track background
    painter.rect_filled(bar_rect, bar_h / 2.0, track_color);

    // Scene sections: alternating tinted regions with labels and dividers
    let scene_colors = [
        egui::Color32::from_rgba_premultiplied(60, 80, 120, 40),
        egui::Color32::from_rgba_premultiplied(80, 60, 120, 40),
    ];
    for (i, seg) in scenes.iter().enumerate() {
        let sx = bar_rect.min.x + seg.start_frac * bar_rect.width();
        let ex = bar_rect.min.x + seg.end_frac * bar_rect.width();
        let scene_rect = egui::Rect::from_min_max(
            egui::pos2(sx, bar_rect.min.y),
            egui::pos2(ex, bar_rect.max.y),
        );
        // Tinted background
        painter.rect_filled(scene_rect, 0.0, scene_colors[i % 2]);
        // Right divider line (skip if at the very end)
        if seg.end_frac < 0.99 {
            painter.line_segment(
                [
                    egui::pos2(ex, bar_rect.min.y - 2.0),
                    egui::pos2(ex, bar_rect.max.y + 2.0),
                ],
                egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_premultiplied(100, 100, 130, 100),
                ),
            );
        }
        // Scene label above the bar (only if wide enough)
        let seg_w = ex - sx;
        if seg_w > 30.0 && !seg.name.is_empty() {
            let label_x = (sx + ex) / 2.0;
            let label_y = bar_rect.min.y - 2.0;
            painter.text(
                egui::pos2(label_x, label_y),
                egui::Align2::CENTER_BOTTOM,
                &seg.name,
                egui::FontId::proportional(9.0),
                egui::Color32::from_rgba_premultiplied(160, 160, 180, 160),
            );
        }
    }

    // Loop region highlight
    if let Some((ls, le)) = loop_frac {
        let lx0 = bar_rect.min.x + ls * bar_rect.width();
        let lx1 = bar_rect.min.x + le * bar_rect.width();
        let loop_rect = egui::Rect::from_min_max(
            egui::pos2(lx0, bar_rect.min.y),
            egui::pos2(lx1, bar_rect.max.y),
        );
        painter.rect_filled(loop_rect, 0.0, loop_color);
    }

    // Progress fill
    if frac > 0.0 {
        let fill_w = frac * bar_rect.width();
        let fill_rect = egui::Rect::from_min_max(
            bar_rect.min,
            egui::pos2(bar_rect.min.x + fill_w, bar_rect.max.y),
        );
        painter.rect_filled(fill_rect, bar_h / 2.0, fill_color);
        // Subtle highlight on top half
        let highlight_rect = egui::Rect::from_min_max(
            fill_rect.min,
            egui::pos2(fill_rect.max.x, fill_rect.center().y),
        );
        painter.rect_filled(highlight_rect, bar_h / 2.0, fill_color_light);
    }

    // Breakpoint markers
    for &bp in bp_fracs {
        let bx = bar_rect.min.x + bp * bar_rect.width();
        let marker_size = 3.0;
        let diamond = vec![
            egui::pos2(bx, bar_rect.min.y - marker_size - 1.0),
            egui::pos2(bx + marker_size, bar_rect.center().y - 1.0),
            egui::pos2(bx, bar_rect.max.y + marker_size - 1.0),
            egui::pos2(bx - marker_size, bar_rect.center().y - 1.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            diamond,
            bp_color,
            egui::Stroke::NONE,
        ));
    }

    // Playhead handle
    let handle_x = bar_rect.min.x + frac * bar_rect.width();
    let handle_pos = egui::pos2(handle_x, bar_y);
    let is_hovering = response.hovered();
    let is_dragging = response.dragged();
    let handle_fill = if is_dragging {
        handle_active
    } else if is_hovering {
        handle_color
    } else {
        egui::Color32::from_rgb(180, 180, 190)
    };
    let stroke_color = if is_dragging || is_hovering {
        egui::Color32::from_rgb(255, 255, 255)
    } else {
        egui::Color32::from_rgba_premultiplied(200, 200, 210, 120)
    };
    painter.circle(
        handle_pos,
        if is_hovering || is_dragging {
            handle_r + 1.0
        } else {
            handle_r
        },
        handle_fill,
        egui::Stroke::new(1.5, stroke_color),
    );

    // Hover time tooltip
    let hover_time = if is_hovering || is_dragging {
        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        if let Some(pos) = pointer_pos {
            let hover_frac = ((pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
            let hover_secs = hover_frac as f64 * total;
            // Paint tooltip above the handle
            let tooltip_text = format_time(hover_secs);
            let tooltip_pos = egui::pos2(pos.x, bar_rect.min.y - 18.0);
            painter.text(
                tooltip_pos,
                egui::Align2::CENTER_BOTTOM,
                tooltip_text,
                egui::FontId::proportional(11.0),
                egui::Color32::from_rgb(230, 230, 240),
            );
            Some(hover_secs)
        } else {
            None
        }
    } else {
        None
    };

    // Interaction: click or drag to seek
    let mut seek_to = None;
    if response.clicked() || response.dragged() {
        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
        if let Some(pos) = pointer_pos {
            let new_frac = ((pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
            seek_to = Some(new_frac);
        }
    }

    SeekBarResponse {
        seek_to,
        hover_time,
    }
}

/// Truncate text to `max_chars` characters, appending "…" if truncated.
fn truncate_with_ellipsis(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    format!("{}…", chars[..max_chars].iter().collect::<String>())
}

/// Styled transport button (skip prev/next).
fn transport_button(ui: &mut egui::Ui, label: &str, on_click: impl FnOnce()) {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(13.0)
            .color(egui::Color32::from_rgb(170, 170, 180)),
    )
    .min_size(egui::vec2(24.0, 22.0))
    .corner_radius(5.0)
    .fill(egui::Color32::from_rgba_premultiplied(35, 35, 50, 160));
    if ui.add(btn).clicked() {
        on_click();
    }
}

/// Global playback keybindings that work regardless of timeline panel visibility.
fn global_playback_keys_system(
    egui_wants: Res<EguiWantsInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut timeline: ResMut<Timeline>,
) {
    if egui_wants.wants_keyboard_input() {
        return;
    }

    let total = timeline.cached_duration.max(0.0);

    if keys.just_pressed(KeyCode::Space) {
        timeline.is_playing = !timeline.is_playing;
    }

    if keys.just_pressed(KeyCode::Home) {
        timeline.is_playing = false;
        timeline.seek_request = Some(0.0);
    }

    if keys.just_pressed(KeyCode::End) {
        timeline.is_playing = false;
        timeline.seek_request = Some(total);
    }

    // Prev / Next scene via arrow keys
    if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::ArrowRight) {
        let go_next = keys.just_pressed(KeyCode::ArrowRight);
        let current = timeline.current_time.clamp(0.0, total);
        let total_f32 = total.max(0.001) as f32;
        let frac = (current as f32 / total_f32).clamp(0.0, 1.0);

        let scene_segs: Vec<(f32, f32)> = timeline
            .scene_index
            .iter()
            .filter_map(|(&_start_time, &scene_id)| {
                let (s, e) = timeline.scene_bounds(scene_id)?;
                Some((
                    (s as f32 / total_f32).clamp(0.0, 1.0),
                    (e as f32 / total_f32).clamp(0.0, 1.0),
                ))
            })
            .collect();

        if !scene_segs.is_empty() {
            let cur_scene_idx = scene_segs
                .iter()
                .position(|(s, e)| frac >= *s && frac < *e + 0.005);
            let before_first = frac < scene_segs[0].0;
            let after_last = frac >= scene_segs.last().unwrap().1 - 0.005;

            let target = if go_next {
                if before_first {
                    Some(scene_segs[0].0 as f64 * total)
                } else if let Some(idx) = cur_scene_idx {
                    if idx + 1 < scene_segs.len() {
                        Some(scene_segs[idx + 1].0 as f64 * total)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                // prev
                if after_last {
                    scene_segs.last().map(|(s, _)| *s as f64 * total)
                } else if let Some(idx) = cur_scene_idx {
                    if idx > 0 {
                        Some(scene_segs[idx - 1].0 as f64 * total)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some(t) = target {
                timeline.seek_request = Some(t);
            }
        }
    }
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

    if available_h < 1.0 {
        cam.viewport_offset_y = 0.0;
        cam.viewport_scale = 1.0;
        return;
    }

    // Always fit animation into the available area while preserving aspect ratio.
    let anim_w = cam.viewport_width as f64;
    let anim_h = cam.viewport_height as f64;
    let scale_x = window_w / anim_w;
    let scale_y = available_h / anim_h;
    cam.viewport_scale = scale_x.min(scale_y);

    // Shift the Vello centre upward so the animation sits above the timeline.
    // When there is no timeline panel (inset.bottom == 0) the offset is 0.
    cam.viewport_offset_y = -(inset.bottom as f64) / 2.0;
}
