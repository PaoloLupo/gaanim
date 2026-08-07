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
pub mod overlays;
mod presenter;
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

/// Interactive preview state for the animation viewport.
///
/// When enabled, the user can pan and zoom the preview camera
/// independently of the timeline-driven camera. `I` enters the mode,
/// `Esc` exits, `R` resets to the animation's current camera.
#[derive(Resource, Debug, Clone)]
pub struct PreviewInteractive {
    pub enabled: bool,
    /// Multiplicative zoom factor applied on top of the fit `viewport_scale`.
    pub user_zoom: f64,
    /// Pan offset in world coordinates applied to `Camera.position`.
    pub pan: glam::DVec2,
}

impl Default for PreviewInteractive {
    fn default() -> Self {
        Self {
            enabled: false,
            user_zoom: 1.0,
            pan: glam::DVec2::ZERO,
        }
    }
}

impl PreviewInteractive {
    pub fn reset(&mut self) {
        self.user_zoom = 1.0;
        self.pan = glam::DVec2::ZERO;
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
        .init_resource::<export::StashedReplay>()
        .init_resource::<presenter::PresenterThumbnailCache>()
        .init_resource::<fps_overlay::FpsOverlay>()
        .init_resource::<vsync::VsyncState>()
        .init_resource::<PresentationMode>()
        .init_resource::<AudienceBlank>()
        .init_resource::<ViewportInset>()
        .init_resource::<PreviewInteractive>()
        .init_resource::<overlays::EditorOverlays>()
        .add_systems(
            Update,
            (
                sync_editor_input_ignore_system
                    .in_set(gaanim_scene::hierarchy::SceneSet::Input)
                    .before(gaanim_timeline::timeline_playback_system),
                preview_mode_keys_system
                    .in_set(gaanim_scene::hierarchy::SceneSet::Input)
                    .after(sync_editor_input_ignore_system),
                preview_interactive_input_system
                    .in_set(gaanim_scene::hierarchy::SceneSet::Input)
                    .after(preview_mode_keys_system),
                editor_picking_system,
                overlays::overlays_toggle_keys_system,
                global_playback_keys_system,
                presentation_blank_shortcuts_system,
                presentation_escape_system,
                fps_overlay::fps_overlay_system,
                vsync::vsync_toggle_system,
            ),
        )
        .add_systems(Startup, presenter::spawn_presenter_window_system)
        .add_systems(Update, presenter::sync_presenter_camera_system)
        .add_systems(
            Update,
            viewport_adjust_system
                .in_set(gaanim_scene::hierarchy::SceneSet::Bounds)
                .before(gaanim_renderer::pipeline::sync_gaanim_camera_to_bevy_system),
        )
        .add_systems(EguiPrimaryContextPass, editor_ui_system)
        .add_systems(
            EguiPrimaryContextPass,
            presentation_blank_overlay_system.after(editor_ui_system),
        )
        .add_systems(EguiPrimaryContextPass, export::export_dialog_system)
        .add_systems(
            EguiPrimaryContextPass,
            (
                overlays::overlays_settings_ui_system,
                overlays::scene_overlays_system,
            )
                .after(editor_ui_system),
        )
        .add_systems(
            presenter::PresenterEguiPass,
            presenter::presenter_view_system,
        );
    }
}

/// Whether the primary window is currently an audience-facing presentation.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct PresentationMode {
    pub active: bool,
}

/// Emergency audience-screen blanking used during a live presentation.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AudienceBlank {
    #[default]
    None,
    Black,
    White,
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
    mut presentation_mode: ResMut<PresentationMode>,
    mut state: ResMut<EditorState>,
    mut export_state: ResMut<export::ExportState>,
    mut timeline: ResMut<Timeline>,
    mut inset: ResMut<ViewportInset>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    mut commands: Commands,
    camera: Option<Res<Camera>>,
    fps_overlay: Res<fps_overlay::FpsOverlay>,
    interactive: Res<PreviewInteractive>,
    q: EditorQueries,
) {
    if presentation_mode.active {
        inset.bottom = 0.0;
        return;
    }
    let Ok(ctx) = ctx.ctx_mut() else {
        return;
    };

    // Interactive mode banner (esquina para no tapar toolbar de overlays en CENTER_TOP)
    if interactive.enabled {
        egui::Area::new("interactive_banner".into())
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8.0, 8.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_premultiplied(30, 90, 50, 230))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_premultiplied(80, 180, 120, 180),
                    ))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("MODO INTERACTIVO")
                                    .color(egui::Color32::from_rgb(160, 255, 180))
                                    .strong()
                                    .size(12.0),
                            );
                            ui.separator();
                            ui.label(
                                egui::RichText::new("Esc: salir  ·  R: reset cámara  ·  Rueda: zoom  ·  Arrastrar: pan")
                                    .color(egui::Color32::from_rgb(230, 240, 230))
                                    .size(11.0),
                            );
                            ui.label(
                                egui::RichText::new(format!("  {:.0}%", interactive.user_zoom * 100.0))
                                    .color(egui::Color32::from_rgb(255, 220, 120))
                                    .size(11.0),
                            );
                        });
                    });
            });
    }

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

    // ── Compact playback overlay (auto-hide) ──────────────────────────────
    let scene_name: String = timeline
        .scene_at(timeline.current_time)
        .and_then(|id| timeline.scenes.get(id))
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let presentation_name = timeline.presentation_label();
    let total = timeline.cached_duration.max(0.0);
    let current = timeline.current_time.clamp(0.0, total);

    // Auto-hide: compacto, no tapa contenido (usa ViewportInset)
    let vp = ctx.viewport_rect();
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    let pointer_near_bottom = pointer.map(|p| p.y > vp.height() - 60.0).unwrap_or(false);
    let should_show = pointer_near_bottom || state.bar_hovered;
    let dt = ctx.input(|i| i.unstable_dt);
    let target_vis = if should_show { 1.0_f32 } else { 0.0_f32 };
    let speed = if should_show { 10.0_f32 } else { 4.0_f32 };
    state.bar_visibility += (target_vis - state.bar_visibility) * (speed * dt).min(1.0);
    if (state.bar_visibility - target_vis).abs() < 0.01 {
        state.bar_visibility = target_vis;
    }
    let vis = state.bar_visibility;
    // Solo la timeline docked reserva espacio. El playback es overlay flotante
    // y no debe modificar viewport_scale/offset para que el preview no "salte"
    // al aparecer/desaparecer.
    let panel_h = timeline_response
        .as_ref()
        .map(|r| r.response.rect.height())
        .unwrap_or(0.0);
    inset.bottom = panel_h;

    if vis > 0.01 {
        let slide_offset = (1.0 - vis) * 8.0;
        let alpha_mul = vis;

        let area_resp = egui::Area::new("playback_overlay".into())
            .anchor(
                egui::Align2::CENTER_BOTTOM,
                egui::vec2(0.0, -panel_h + slide_offset),
            )
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                let screen_w = vp.width();
                // Más ancho para comodidad, sin tapar bordes
                let bar_w = (screen_w * 0.82).min(980.0).max(640.0);
                let avail_w = (screen_w - 24.0).max(bar_w);
                let final_w = bar_w.min(avail_w);

                let fill_alpha = (245.0 * alpha_mul) as u8;
                let stroke_alpha = (110.0 * alpha_mul) as u8;
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_premultiplied(
                        22, 22, 30, fill_alpha,
                    ))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 16,
                        top: 10,
                        bottom: 8,
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_premultiplied(65, 65, 80, stroke_alpha),
                    ))
                    .show(ui, |ui| {
                        ui.set_width(final_w);
                        ui.spacing_mut().item_spacing.y = 6.0;

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
                            // snapping visual magnético: imán suave a bordes de escena / breakpoints
                            let mut snapped_frac = new_frac;
                            let snap_frac = 0.015; // ~10px en ancho típico
                            for seg in &scene_segs {
                                if (new_frac - seg.start_frac).abs() < snap_frac {
                                    snapped_frac = seg.start_frac;
                                    break;
                                }
                                if (new_frac - seg.end_frac).abs() < snap_frac {
                                    snapped_frac = seg.end_frac;
                                    break;
                                }
                            }
                            if snapped_frac == new_frac {
                                for &bp in &bp_fracs {
                                    if (new_frac - bp).abs() < snap_frac {
                                        snapped_frac = bp;
                                        break;
                                    }
                                }
                            }
                            timeline.seek_request = Some(snapped_frac as f64 * total);
                        }
                        if let Some((ls, le)) = seek_resp.loop_drag {
                            let s = (ls as f64 * total).clamp(0.0, total);
                            let e = (le as f64 * total).clamp(0.0, total);
                            if (e - s).abs() > 0.05 {
                                timeline.loop_range = Some((s.min(e), s.max(e)));
                            }
                        }
                        if seek_resp.loop_toggle {
                            if timeline.loop_range.is_some() {
                                timeline.loop_range = None;
                            } else {
                                timeline.loop_range = Some((0.0, timeline.cached_duration.max(0.01)));
                            }
                        }
                        state.seek_bar_hover = seek_resp.hover_time;

                        // Row 2: controles (escalados)
                        ui.add_space(4.0);
                        egui::ScrollArea::horizontal()
                            .auto_shrink([false, true])
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                            )
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                            // ── Grupo A: Transporte (pill)
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgba_premultiplied(38, 38, 52, 110))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::symmetric(6, 4))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(60, 60, 75, 90)))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        transport_button(ui, "⏮", || {
                                            timeline.is_playing = false;
                                            timeline.seek_request = Some(0.0);
                                        });

                                        // Play / Pause (más grande)
                                        let play_sym = if timeline.is_playing { "⏸" } else { "▶" };
                                        let play_btn = egui::Button::new(
                                            egui::RichText::new(play_sym).size(14.0).color(
                                                if timeline.is_playing {
                                                    egui::Color32::from_rgb(120, 200, 255)
                                                } else {
                                                    egui::Color32::from_rgb(200, 200, 210)
                                                },
                                            ),
                                        )
                                        .min_size(egui::vec2(34.0, 26.0))
                                        .corner_radius(7.0)
                                        .fill(egui::Color32::from_rgba_premultiplied(40, 40, 55, 180));
                                        if ui.add(play_btn).on_hover_text("Espacio: Play/Pausa").clicked() {
                                            timeline.is_playing = !timeline.is_playing;
                                        }

                                        transport_button(ui, "⏭", || {
                                            timeline.is_playing = false;
                                            timeline.seek_request = Some(total);
                                        });

                                        if !scene_segs.is_empty() {
                                            ui.separator();
                                            let cur_scene_idx = scene_segs.iter().position(|s| {
                                                frac >= s.start_frac && frac < s.end_frac + 0.005
                                            });
                                            let has_prev = cur_scene_idx.map_or(false, |i| i > 0);
                                            let has_next = cur_scene_idx.map_or(false, |i| i + 1 < scene_segs.len());
                                            let before_first = !scene_segs.is_empty() && frac < scene_segs[0].start_frac;
                                            let after_last = !scene_segs.is_empty() && frac >= scene_segs.last().unwrap().end_frac - 0.005;

                                            let prev_color = if has_prev || after_last {
                                                egui::Color32::from_rgb(170, 170, 180)
                                            } else {
                                                egui::Color32::from_rgb(70, 70, 80)
                                            };
                                            let prev_btn = egui::Button::new(
                                                egui::RichText::new("‹").size(16.0).color(prev_color),
                                            )
                                            .min_size(egui::vec2(24.0, 24.0))
                                            .corner_radius(6.0)
                                            .fill(egui::Color32::from_rgba_premultiplied(38, 38, 55, 150));
                                            if ui.add(prev_btn).on_hover_text("← Anterior escena (Flecha izq.)").clicked() {
                                                let target = if after_last {
                                                    scene_segs.last().map(|s| s.start_frac as f64 * total)
                                                } else if let Some(idx) = cur_scene_idx {
                                                    if idx > 0 { Some(scene_segs[idx - 1].start_frac as f64 * total) } else { None }
                                                } else { None };
                                                if let Some(t) = target { timeline.seek_request = Some(t); }
                                            }

                                            let next_color = if has_next || before_first {
                                                egui::Color32::from_rgb(170, 170, 180)
                                            } else {
                                                egui::Color32::from_rgb(70, 70, 80)
                                            };
                                            let next_btn = egui::Button::new(
                                                egui::RichText::new("›").size(16.0).color(next_color),
                                            )
                                            .min_size(egui::vec2(24.0, 24.0))
                                            .corner_radius(6.0)
                                            .fill(egui::Color32::from_rgba_premultiplied(38, 38, 55, 150));
                                            if ui.add(next_btn).on_hover_text("Siguiente escena →").clicked() {
                                                let target = if before_first {
                                                    Some(scene_segs[0].start_frac as f64 * total)
                                                } else if let Some(idx) = cur_scene_idx {
                                                    if idx + 1 < scene_segs.len() { Some(scene_segs[idx + 1].start_frac as f64 * total) } else { None }
                                                } else { None };
                                                if let Some(t) = target { timeline.seek_request = Some(t); }
                                            }
                                        }
                                    });
                                });

                            ui.add_space(6.0);

                            // ── Grupo B: Info central (speed · tiempo · loop · escena)
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgba_premultiplied(32, 34, 46, 120))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(65, 68, 90, 80)))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;

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
                                    .size(13.0)
                                    .color(speed_color),
                            )
                            .min_size(egui::vec2(58.0, 24.0))
                            .corner_radius(4.0)
                            .fill(egui::Color32::from_rgba_premultiplied(35, 35, 50, 160));
                            let speed_resp = ui.add(speed_btn);
                            let speed_hovered = speed_resp.on_hover_text("Velocidad (Alt+Rueda) · Click para presets");
                            egui::Popup::from_toggle_button_response(&speed_hovered)
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

                            ui.add_space(3.0);

                            // Time display (más grande) — click copia timecode
                            let time_resp = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "{} / {}",
                                        format_time(current),
                                        format_time(total),
                                    ))
                                    .size(12.5)
                                    .monospace()
                                    .color(egui::Color32::from_rgb(185, 185, 195)),
                                )
                                .selectable(false)
                                .sense(egui::Sense::click()),
                            );
                            if time_resp.on_hover_text("Click para copiar timecode · Arrastrar para scrub").clicked() {
                                ui.ctx().copy_text(format_time(current));
                            }

                            ui.add_space(4.0);

                            // Loop toggle — arrastrable en la barra
                            let loop_on = timeline.loop_range.is_some();
                            let loop_color = if loop_on {
                                egui::Color32::from_rgb(100, 200, 140)
                            } else {
                                egui::Color32::from_rgb(100, 100, 110)
                            };
                            let loop_btn = egui::Button::new(
                                egui::RichText::new("🔁").size(14.0).color(loop_color),
                            )
                            .min_size(egui::vec2(28.0, 24.0))
                            .corner_radius(4.0)
                            .fill(egui::Color32::TRANSPARENT);
                            if ui.add(loop_btn).on_hover_text("Loop (L) · Arrastra los tiradores en la barra · Doble-click en barra alterna").clicked() {
                                if loop_on {
                                    timeline.loop_range = None;
                                } else {
                                    timeline.loop_range = Some((0.0, timeline.cached_duration));
                                }
                            }

                            // Scene name — ahora después del loop (más grande)
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(6.0);
                            if let Some(presentation_name) = &presentation_name {
                                let display = truncate_with_ellipsis(presentation_name, 22);
                                ui.label(
                                    egui::RichText::new(display)
                                        .color(egui::Color32::from_rgb(170, 210, 255))
                                        .strong()
                                        .size(13.0),
                                );
                                ui.add_space(4.0);
                            } else if !scene_name.is_empty() {
                                let display = truncate_with_ellipsis(&scene_name, 20);
                                let scene_text = display;
                                let is_active_scene = !scene_segs.is_empty();
                                let scene_color = if is_active_scene {
                                    egui::Color32::from_rgb(170, 210, 255)
                                } else {
                                    egui::Color32::from_rgb(150, 180, 220)
                                };
                                ui.label(
                                    egui::RichText::new(scene_text)
                                        .color(scene_color)
                                        .strong()
                                        .size(13.0),
                                );
                                ui.add_space(4.0);
                            } else if !scene_segs.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!("{} scenes", scene_segs.len()))
                                        .color(egui::Color32::from_rgb(120, 130, 150))
                                        .size(12.0),
                                );
                                ui.add_space(4.0);
                            } else if scene_segs.is_empty() {
                                // Estado vacío: sin escenas
                                ui.label(
                                    egui::RichText::new("Sin escenas")
                                        .color(egui::Color32::from_rgb(110, 115, 135))
                                        .italics()
                                        .size(11.0),
                                );
                                ui.add_space(2.0);
                            }
                                    });
                                });

                            ui.add_space(6.0);

                            // ── Grupo C: Acciones derecha ──
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgba_premultiplied(34, 32, 44, 100))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::symmetric(6, 4))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(60, 60, 75, 70)))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
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
                                            .size(12.5)
                                            .color(egui::Color32::from_rgb(150, 150, 165)),
                                    )
                                    .min_size(egui::vec2(68.0, 24.0))
                                    .corner_radius(4.0)
                                    .fill(egui::Color32::from_rgba_premultiplied(
                                        30, 30, 45, 140,
                                    ));
                                    if ui.add(tl_btn).clicked() {
                                        state.timeline_visible = !state.timeline_visible;
                                    }

                                    let present_btn = egui::Button::new(
                                        egui::RichText::new("▶ Present")
                                            .size(12.5)
                                            .color(egui::Color32::from_rgb(150, 215, 255)),
                                    )
                                    .min_size(egui::vec2(72.0, 24.0))
                                    .corner_radius(4.0)
                                    .fill(egui::Color32::from_rgba_premultiplied(
                                        35, 70, 95, 180,
                                    ));
                                    if ui
                                        .add(present_btn)
                                        .on_hover_text(
                                            "Start fullscreen audience mode on this monitor",
                                        )
                                        .clicked()
                                    {
                                        if let Ok(mut window) = windows.single_mut() {
                                            window.mode =
                                                bevy::window::WindowMode::BorderlessFullscreen(
                                                    bevy::window::MonitorSelection::Current,
                                                );
                                            presentation_mode.active = true;
                                            presenter::spawn_presenter_window(&mut commands);
                                        }
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
                                                .size(12.5)
                                                .color(egui::Color32::from_rgb(150, 200, 255)),
                                        )
                                        .min_size(egui::vec2(62.0, 24.0))
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
                                            .size(14.0)
                                            .color(pin_color),
                                    )
                                    .min_size(egui::vec2(26.0, 24.0))
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
                                    });
                                });
                            });
                            // cerrar grupos
                         });
                     });
                 });
             });
        // Keep bar visible while pointer is over overlay (no desaparece si cursor está encima)
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());
        state.bar_hovered =
                hover_pos.is_some_and(|p| area_resp.response.rect.contains(p));
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
    /// If Some, the user dragged a loop handle to a new (start_frac, end_frac).
    loop_drag: Option<(f32, f32)>,
    /// Double-click on bar toggles loop.
    loop_toggle: bool,
}

/// Paint a custom seek bar with progress fill, loop region, breakpoint markers,
/// scene sections, playhead handle, and hover time tooltip.
///
/// When scenes exist, a dedicated scene lane is rendered **encima de la
/// línea de tiempo**: each scene occupies its time span, its name is
/// centered above the track, and the boundary between scenes is marked
/// with a crisp tick that connects the lane to the seek bar.
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
    let has_scenes = !scenes.is_empty();
    let scene_lane_h = if has_scenes { 28.0_f32 } else { 0.0 };
    let desired_h = 16.0_f32 + scene_lane_h;

    let (response, painter) = ui.allocate_painter(
        egui::vec2(ui.available_width(), desired_h),
        egui::Sense::click_and_drag(),
    );
    let rect = response.rect;
    // Bar is anchored below the scene lane so it never overlaps labels.
    let bar_y = if has_scenes {
        rect.min.y + scene_lane_h + 8.0
    } else {
        rect.center().y
    };
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

    // ── Scene lane (encima de la línea) ────────────────────────────────
    let active_scene_idx = if has_scenes {
        scenes
            .iter()
            .position(|s| frac >= s.start_frac && frac < s.end_frac + 0.002)
            .or_else(|| {
                if frac >= 0.99 {
                    scenes
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| frac >= s.start_frac)
                        .map(|(i, _)| i)
                        .last()
                } else {
                    None
                }
            })
    } else {
        None
    };

    if has_scenes {
        let lane_rect = egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.min.y),
            egui::pos2(rect.max.x, rect.min.y + scene_lane_h - 2.0),
        );
        // Subtle lane background so scene chips stand out even for very short scenes
        painter.rect_filled(lane_rect, 6.0, egui::Color32::from_rgba_premultiplied(28, 28, 36, 180));
        painter.rect_stroke(
            lane_rect,
            6.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(45, 45, 58, 120)),
            egui::StrokeKind::Inside,
        );

        let lane_colors = [
            egui::Color32::from_rgba_premultiplied(70, 95, 150, 95),
            egui::Color32::from_rgba_premultiplied(95, 70, 150, 95),
        ];
        let lane_colors_active = [
            egui::Color32::from_rgba_premultiplied(90, 130, 210, 140),
            egui::Color32::from_rgba_premultiplied(120, 90, 210, 140),
        ];

        for (i, seg) in scenes.iter().enumerate() {
            let sx = rect.min.x + seg.start_frac * rect.width();
            let ex = rect.min.x + seg.end_frac * rect.width();
            let seg_w = ex - sx;
            if seg_w < 1.0 {
                continue;
            }
            let is_active = active_scene_idx == Some(i);

            // Lane chip for this scene (inset 1px to leave gap between scenes)
            let inset = 1.5;
            let chip_rect = egui::Rect::from_min_max(
                egui::pos2(sx + inset, lane_rect.min.y + 3.0),
                egui::pos2((ex - inset).max(sx + inset + 4.0), lane_rect.max.y - 2.0),
            );
            let bg = if is_active {
                lane_colors_active[i % 2]
            } else {
                lane_colors[i % 2]
            };
            painter.rect_filled(chip_rect, 5.0, bg);
            // progreso intra-escena (innovador): fill sutil dentro del chip activo
            if is_active {
                let seg_range = (seg.end_frac - seg.start_frac).max(1e-6);
                let prog = ((frac - seg.start_frac) / seg_range).clamp(0.0, 1.0);
                if prog > 0.001 {
                    let prog_w = prog * chip_rect.width();
                    // clip al radius del chip
                    let prog_rect = egui::Rect::from_min_max(
                        chip_rect.min,
                        egui::pos2(chip_rect.min.x + prog_w, chip_rect.max.y),
                    );
                    // usamos un rect con mismo radio; el exceso se recorta visualmente
                    painter.rect_filled(prog_rect, 5.0, egui::Color32::from_rgba_premultiplied(120, 180, 255, 45));
                }
                painter.rect_stroke(
                    chip_rect,
                    5.0,
                    egui::Stroke::new(1.4, egui::Color32::from_rgb(120, 180, 255)),
                    egui::StrokeKind::Inside,
                );
            }

            // Vertical boundary tick between scenes (visible seam + connector to bar)
            if i + 1 < scenes.len() && seg.end_frac < 0.995 {
                // Sutura elegante entre escenas: línea sutil + perla
                painter.line_segment(
                    [
                        egui::pos2(ex, lane_rect.max.y),
                        egui::pos2(ex, bar_rect.min.y),
                    ],
                    egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_premultiplied(160, 170, 195, 110),
                    ),
                );
                let dot_y = lane_rect.center().y;
                let dot_pos = egui::pos2(ex, dot_y);
                // sombra suave
                painter.circle_filled(dot_pos + egui::vec2(0.7, 0.7), 4.2, egui::Color32::from_black_alpha(45));
                // anillo exterior
                painter.circle_filled(dot_pos, 4.0, egui::Color32::from_rgba_premultiplied(38, 42, 58, 210));
                // perla interior
                painter.circle_filled(dot_pos, 2.4, egui::Color32::from_rgb(215, 222, 240));
                painter.circle(
                    dot_pos,
                    4.0,
                    egui::Color32::TRANSPARENT,
                    egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(90, 100, 130, 130)),
                );
            }

            // Scene name centered in the chip (truncate to fit width) — más grande
            if seg_w > 28.0 && !seg.name.is_empty() {
                // ~7px per char at 11pt
                let max_chars = ((seg_w - 12.0) / 7.0).floor() as usize;
                let label = if max_chars >= 3 {
                    truncate_with_ellipsis(&seg.name, max_chars.max(3))
                } else {
                    String::new()
                };
                if !label.is_empty() {
                    let label_x = (sx + ex) / 2.0;
                    let label_y = chip_rect.center().y;
                    let text_color = if is_active {
                        egui::Color32::from_rgb(235, 245, 255)
                    } else {
                        egui::Color32::from_rgba_premultiplied(205, 210, 230, 195)
                    };
                    // Subtle shadow for legibility
                    painter.text(
                        egui::pos2(label_x + 0.5, label_y + 0.5),
                        egui::Align2::CENTER_CENTER,
                        &label,
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_black_alpha(130),
                    );
                    painter.text(
                        egui::pos2(label_x, label_y),
                        egui::Align2::CENTER_CENTER,
                        &label,
                        egui::FontId::proportional(if is_active { 12.0 } else { 11.0 }),
                        text_color,
                    );
                }
            }
        }
    }

    // Track background
    painter.rect_filled(bar_rect, bar_h / 2.0, track_color);

    // Keep the tinted bar segments for context (slightly stronger than before)
    let scene_colors_bar = [
        egui::Color32::from_rgba_premultiplied(60, 80, 120, 45),
        egui::Color32::from_rgba_premultiplied(80, 60, 120, 45),
    ];
    for (i, seg) in scenes.iter().enumerate() {
        let sx = bar_rect.min.x + seg.start_frac * bar_rect.width();
        let ex = bar_rect.min.x + seg.end_frac * bar_rect.width();
        let scene_rect = egui::Rect::from_min_max(
            egui::pos2(sx, bar_rect.min.y),
            egui::pos2(ex, bar_rect.max.y),
        );
        painter.rect_filled(scene_rect, 0.0, scene_colors_bar[i % 2]);
        if seg.end_frac < 0.99 {
            painter.line_segment(
                [
                    egui::pos2(ex, bar_rect.min.y - 1.0),
                    egui::pos2(ex, bar_rect.max.y + 1.0),
                ],
                egui::Stroke::new(
                    1.2,
                    egui::Color32::from_rgba_premultiplied(140, 140, 170, 130),
                ),
            );
        }
    }

    // Loop region highlight + handles arrastrables
    if let Some((ls, le)) = loop_frac {
        let lx0 = bar_rect.min.x + ls * bar_rect.width();
        let lx1 = bar_rect.min.x + le * bar_rect.width();
        let loop_rect = egui::Rect::from_min_max(
            egui::pos2(lx0, bar_rect.min.y - 1.0),
            egui::pos2(lx1, bar_rect.max.y + 1.0),
        );
        painter.rect_filled(loop_rect, 2.0, loop_color);
        // borde sutil
        painter.rect_stroke(loop_rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(100, 180, 140, 110)), egui::StrokeKind::Inside);
        // handles: pills verticales en los bordes
        let handle_w = 6.0;
        let handle_h = bar_rect.height() + 8.0;
        let hy = bar_rect.center().y - handle_h / 2.0;
        for &hx in &[lx0, lx1] {
            let hrect = egui::Rect::from_min_max(
                egui::pos2(hx - handle_w / 2.0, hy),
                egui::pos2(hx + handle_w / 2.0, hy + handle_h),
            );
            // fondo
            painter.rect_filled(hrect, 3.0, egui::Color32::from_rgba_premultiplied(45, 55, 65, 210));
            painter.rect_stroke(hrect, 3.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 200, 140)), egui::StrokeKind::Inside);
            // grip lines
            for dy in [-3.0, 0.0, 3.0] {
                painter.line_segment(
                    [egui::pos2(hx - 2.0, bar_rect.center().y + dy), egui::pos2(hx + 2.0, bar_rect.center().y + dy)],
                    egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(180, 220, 200, 170)),
                );
            }
        }
        // cursor feedback sobre handles
        if let Some(hover_pos) = response.hover_pos() {
            let near_left = (hover_pos.x - lx0).abs() < 8.0 && (hover_pos.y - bar_rect.center().y).abs() < 12.0;
            let near_right = (hover_pos.x - lx1).abs() < 8.0 && (hover_pos.y - bar_rect.center().y).abs() < 12.0;
            if near_left || near_right {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }
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

    // Cursor feedback for lane: show hand when hovering a scene chip
    if has_scenes {
        if let Some(hover_pos) = response.hover_pos() {
            let lane_rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y),
                egui::pos2(rect.max.x, rect.min.y + scene_lane_h),
            );
            if lane_rect.contains(hover_pos) {
                // Check if over any scene chip (seg_w > 1)
                let over_scene = scenes.iter().any(|seg| {
                    let sx = rect.min.x + seg.start_frac * rect.width();
                    let ex = rect.min.x + seg.end_frac * rect.width();
                    hover_pos.x >= sx && hover_pos.x <= ex
                });
                if over_scene {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        }
    }

    // Hover tooltip — ahora con escena + imán visual
    let hover_time = if is_hovering || is_dragging {
        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        if let Some(pos) = pointer_pos {
            let hover_frac = ((pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
            let hover_secs = hover_frac as f64 * total;

            // snapping visual: detecta borde cercano
            let snap_thr = 0.015;
            let mut snap_frac: Option<f32> = None;
            let mut snap_label: Option<&str> = None;
            for seg in scenes {
                if (hover_frac - seg.start_frac).abs() < snap_thr {
                    snap_frac = Some(seg.start_frac);
                    snap_label = Some(&seg.name);
                    break;
                }
                if (hover_frac - seg.end_frac).abs() < snap_thr {
                    snap_frac = Some(seg.end_frac);
                    snap_label = None;
                    break;
                }
            }
            if snap_frac.is_none() {
                for &bp in bp_fracs {
                    if (hover_frac - bp).abs() < snap_thr {
                        snap_frac = Some(bp);
                        break;
                    }
                }
            }
            if let Some(sf) = snap_frac {
                let sx = bar_rect.min.x + sf * bar_rect.width();
                painter.line_segment(
                    [egui::pos2(sx, bar_rect.min.y - 6.0), egui::pos2(sx, bar_rect.max.y + 6.0)],
                    egui::Stroke::new(1.6, egui::Color32::from_rgba_premultiplied(255, 210, 110, 200)),
                );
                // pequeño imán
                painter.text(
                    egui::pos2(sx, bar_rect.min.y - 8.0),
                    egui::Align2::CENTER_BOTTOM,
                    "🧲",
                    egui::FontId::proportional(10.0),
                    egui::Color32::from_rgb(255, 210, 110),
                );
                // si hay snap, el tooltip puede indicar el borde
                let _ = snap_label;
            }

            // texto del tooltip: "Escena · m:ss"
            let hover_scene = scenes.iter().find(|s| hover_frac >= s.start_frac && hover_frac < s.end_frac + 0.001).map(|s| s.name.as_str());
            let tooltip_text = if let Some(name) = hover_scene {
                // truncar nombre si es muy largo
                let short = truncate_with_ellipsis(name, 16);
                format!("{} · {}", short, format_time(hover_secs))
            } else {
                format_time(hover_secs)
            };

            let tooltip_y = if has_scenes {
                rect.min.y - 6.0
            } else {
                bar_rect.min.y - 20.0
            };
            let tooltip_pos = egui::pos2(pos.x, tooltip_y);
            let galley = painter.layout_no_wrap(
                tooltip_text.clone(),
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
            let pad = egui::vec2(8.0, 4.0);
            let bg_rect = egui::Rect::from_center_size(tooltip_pos + egui::vec2(0.0, -galley.size().y / 2.0), galley.size() + pad * 2.0);
            painter.rect_filled(bg_rect, 5.0, egui::Color32::from_rgba_premultiplied(20, 20, 28, 230));
            painter.rect_stroke(bg_rect, 5.0, egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(70, 70, 85, 160)), egui::StrokeKind::Inside);
            painter.text(
                tooltip_pos,
                egui::Align2::CENTER_BOTTOM,
                tooltip_text,
                egui::FontId::proportional(12.0),
                egui::Color32::from_rgb(235, 235, 245),
            );
            Some(hover_secs)
        } else {
            None
        }
    } else {
        None
    };

    // Interaction: lane chip → inicio escena, handle loop → ajusta loop, resto → seek
    let mut seek_to = None;
    let mut loop_drag = None;
    let mut loop_toggle = false;

    // doble-click en la barra alterna loop
    if response.double_clicked() {
        loop_toggle = true;
    }

    if response.clicked() {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let lane_rect = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y),
                egui::pos2(rect.max.x, rect.min.y + scene_lane_h),
            );
            let clicked_lane = has_scenes && lane_rect.contains(pos);
            // prioridad: handle de loop si está cerca
            let mut handled_loop = false;
            if let Some((ls, le)) = loop_frac {
                let lx0 = bar_rect.min.x + ls * bar_rect.width();
                let lx1 = bar_rect.min.x + le * bar_rect.width();
                let near_left = (pos.x - lx0).abs() < 10.0;
                let near_right = (pos.x - lx1).abs() < 10.0;
                if near_left || near_right {
                    // click directo en handle no hace seek, espera drag; ignoramos
                    handled_loop = true;
                }
            }
            if !handled_loop {
                if clicked_lane {
                    let mut snapped = None;
                    for seg in scenes {
                        let sx = rect.min.x + seg.start_frac * rect.width();
                        let ex = rect.min.x + seg.end_frac * rect.width();
                        if pos.x >= sx && pos.x <= ex {
                            snapped = Some(seg.start_frac);
                            break;
                        }
                    }
                    if let Some(s) = snapped {
                        seek_to = Some(s);
                    } else {
                        let new_frac = ((pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
                        seek_to = Some(new_frac);
                    }
                } else {
                    let new_frac = ((pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
                    seek_to = Some(new_frac);
                }
            }
        }
    } else if response.dragged() {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let new_frac = ((pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
            // si hay loop y el drag está cerca de un handle, interpreta como ajuste de loop
            if let Some((ls, le)) = loop_frac {
                let lx0 = bar_rect.min.x + ls * bar_rect.width();
                let lx1 = bar_rect.min.x + le * bar_rect.width();
                // decide handle por proximidad inicial: elige el más cercano al cursor
                let dist_left = (pos.x - lx0).abs();
                let dist_right = (pos.x - lx1).abs();
                // heurística: cerca del borde → loop, resto → seek
                let near_edge = dist_left < 12.0 || dist_right < 12.0;
                if near_edge {
                    // ajusta el handle más cercano
                    if dist_left < dist_right {
                        let clamped = new_frac.clamp(0.0, le - 0.005);
                        loop_drag = Some((clamped, le));
                    } else {
                        let clamped = new_frac.clamp(ls + 0.005, 1.0);
                        loop_drag = Some((ls, clamped));
                    }
                } else {
                    // arrastre general → seek continuo (con snapping visual abajo)
                    seek_to = Some(new_frac);
                }
            } else {
                seek_to = Some(new_frac);
            }
        }
    }

    SeekBarResponse {
        seek_to,
        hover_time,
        loop_drag,
        loop_toggle,
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

/// Styled transport button (skip prev/next) — escalado.
fn transport_button(ui: &mut egui::Ui, label: &str, on_click: impl FnOnce()) {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .size(13.0)
            .color(egui::Color32::from_rgb(170, 170, 180)),
    )
    .min_size(egui::vec2(28.0, 24.0))
    .corner_radius(4.0)
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

    // Presentation navigation is owned by `presentation_input_system`, which
    // advances by semantic slides and reveal steps rather than raw scenes.
    if !timeline.presentation.is_empty() {
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

fn presentation_blank_shortcuts_system(
    keys: Res<ButtonInput<KeyCode>>,
    presentation_mode: Res<PresentationMode>,
    mut blank: ResMut<AudienceBlank>,
) {
    if !presentation_mode.active {
        *blank = AudienceBlank::None;
        return;
    }
    if keys.just_pressed(KeyCode::KeyB) {
        *blank = if *blank == AudienceBlank::Black {
            AudienceBlank::None
        } else {
            AudienceBlank::Black
        };
    }
    if keys.just_pressed(KeyCode::KeyW) {
        *blank = if *blank == AudienceBlank::White {
            AudienceBlank::None
        } else {
            AudienceBlank::White
        };
    }
}

fn presentation_blank_overlay_system(
    mut contexts: bevy_egui::EguiContexts,
    presentation_mode: Res<PresentationMode>,
    blank: Res<AudienceBlank>,
) {
    if !presentation_mode.active || *blank == AudienceBlank::None {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let rect = ctx.viewport_rect();
    let color = match *blank {
        AudienceBlank::Black => egui::Color32::BLACK,
        AudienceBlank::White => egui::Color32::WHITE,
        AudienceBlank::None => return,
    };
    egui::Area::new(egui::Id::new("gaanim-audience-blank"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.painter().rect_filled(rect, 0.0, color);
            ui.allocate_rect(rect, egui::Sense::hover());
        });
}

/// Escape leaves audience mode and restores the editor chrome in a window.
fn presentation_escape_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut presentation_mode: ResMut<PresentationMode>,
    mut blank: ResMut<AudienceBlank>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    presenter_windows: Query<Entity, With<presenter::PresenterWindow>>,
    presenter_cameras: Query<Entity, With<presenter::PresenterCamera>>,
    mut commands: Commands,
) {
    if !presentation_mode.active || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if *blank != AudienceBlank::None {
        *blank = AudienceBlank::None;
        return;
    }
    if let Ok(mut window) = windows.single_mut() {
        window.mode = bevy::window::WindowMode::Windowed;
        presentation_mode.active = false;
        for entity in &presenter_windows {
            commands.entity(entity).despawn();
        }
        for entity in &presenter_cameras {
            commands.entity(entity).despawn();
        }
    }
}

/// Toggle interactive preview with `I`, exit with `Esc`, reset with `R`.
fn preview_mode_keys_system(
    egui_wants: Res<EguiWantsInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut interactive: ResMut<PreviewInteractive>,
    presentation_mode: Res<PresentationMode>,
) {
    if egui_wants.wants_keyboard_input() {
        return;
    }
    // Don't interfere with presentation mode's own Esc handling when blanking.
    if presentation_mode.active {
        return;
    }

    if !interactive.enabled && keys.just_pressed(KeyCode::KeyI) {
        interactive.enabled = true;
    } else if interactive.enabled && keys.just_pressed(KeyCode::Escape) {
        interactive.enabled = false;
        interactive.reset();
    } else if interactive.enabled && keys.just_pressed(KeyCode::KeyR) {
        interactive.reset();
    }
}

/// Pan (drag) and zoom (wheel) when interactive mode is enabled.
/// Wheel zooms; middle/right or left drag pans (left only in interactive mode).
/// Also updates the system cursor to Grab/Grabbing while interactive.
/// For perspective cameras: Right-drag orbits, Middle/Shift+Left pan, Wheel dolly.
fn preview_interactive_input_system(
    mut interactive: ResMut<PreviewInteractive>,
    camera: Option<ResMut<Camera>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut prev_cursor: Local<Option<glam::DVec2>>,
    mut commands: Commands,
    window_entity: Query<Entity, With<bevy::window::PrimaryWindow>>,
) {
    // Cursor feedback: hand when interactive, grabbing while dragging.
    let Ok(win_entity) = window_entity.single() else {
        return;
    };
    if !interactive.enabled {
        commands.entity(win_entity).remove::<bevy::window::CursorIcon>();
        *prev_cursor = None;
        return;
    }
    let is_panning_now = mouse_button.pressed(MouseButton::Middle)
        || mouse_button.pressed(MouseButton::Right)
        || mouse_button.pressed(MouseButton::Left);
    let icon = if is_panning_now {
        bevy::window::CursorIcon::System(bevy::window::SystemCursorIcon::Grabbing)
    } else {
        bevy::window::CursorIcon::System(bevy::window::SystemCursorIcon::Grab)
    };
    commands.entity(win_entity).insert(icon);

    let Some(mut cam) = camera else {
        *prev_cursor = None;
        return;
    };
    let Ok(window) = windows.single() else {
        *prev_cursor = None;
        return;
    };

    let is_perspective = matches!(cam.projection, gaanim_math::Projection::Perspective { .. });

    // --- Zoom / Dolly with mouse wheel ---
    let wheel_delta = scroll.delta.y;
    if wheel_delta.abs() > f32::EPSILON {
        let step = match scroll.unit {
            bevy::input::mouse::MouseScrollUnit::Line => wheel_delta * 0.12,
            bevy::input::mouse::MouseScrollUnit::Pixel => wheel_delta / 100.0 * 0.12,
        };
        let step = step.clamp(-0.6, 0.6);
        if step.abs() > 1e-6 {
            let factor = (1.0 - step as f64).clamp(0.5, 2.0);
            if is_perspective {
                cam.dolly(factor);
            } else {
                interactive.user_zoom = (interactive.user_zoom * factor).clamp(0.1, 20.0);
            }
        }
    }

    // --- Keyboard pan fallback ---
    {
        if is_perspective {
            let mut kdelta = glam::DVec2::ZERO;
            let speed = 5.0;
            if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
                kdelta.x -= speed;
            }
            if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
                kdelta.x += speed;
            }
            if keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW) {
                kdelta.y += speed;
            }
            if keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS) {
                kdelta.y -= speed;
            }
            if kdelta.length_squared() > 1e-9 {
                cam.pan_screen_delta(kdelta);
            }
        } else {
            let proj_zoom = match cam.projection {
                gaanim_math::Projection::Orthographic { zoom } => zoom,
                _ => 1.0,
            };
            let effective = (cam.viewport_scale * proj_zoom).max(0.1);
            let speed = 400.0 / effective * time.delta_secs_f64();
            let mut kdelta = glam::DVec2::ZERO;
            if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
                kdelta.x -= speed;
            }
            if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
                kdelta.x += speed;
            }
            if keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW) {
                kdelta.y += speed;
            }
            if keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS) {
                kdelta.y -= speed;
            }
            if kdelta.length_squared() > 1e-9 {
                interactive.pan += kdelta;
            }
        }
    }

    // --- Mouse drag: orbit / pan ---
    let cur = window
        .cursor_position()
        .map(|p| glam::DVec2::new(p.x as f64, p.y as f64));

    let is_orbiting = is_perspective && mouse_button.pressed(MouseButton::Right);
    let is_panning_3d = is_perspective
        && (mouse_button.pressed(MouseButton::Middle)
            || (mouse_button.pressed(MouseButton::Left)
                && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))));
    let is_panning_2d = !is_perspective
        && (mouse_button.pressed(MouseButton::Middle)
            || mouse_button.pressed(MouseButton::Right)
            || mouse_button.pressed(MouseButton::Left));
    let is_dragging = is_orbiting || is_panning_3d || is_panning_2d;

    if !is_dragging {
        *prev_cursor = cur;
        return;
    }
    let mut delta_opt: Option<glam::DVec2> = None;
    if let (Some(cur_pos), Some(prev)) = (cur, *prev_cursor) {
        let d = cur_pos - prev;
        if d.length_squared() > 1e-9 {
            delta_opt = Some(d);
        }
        *prev_cursor = Some(cur_pos);
    } else if let Some(cur_pos) = cur {
        *prev_cursor = Some(cur_pos);
    }
    if delta_opt.is_none() && motion.delta.length_squared() > 1e-9 {
        let m = motion.delta;
        delta_opt = Some(glam::DVec2::new(m.x as f64, m.y as f64));
    }
    let Some(delta) = delta_opt else {
        return;
    };
    if is_perspective {
        if is_orbiting {
            cam.orbit_around_target(delta.x * 0.005, -delta.y * 0.005);
        } else if is_panning_3d {
            cam.pan_screen_delta(delta);
        }
    } else {
        let proj_zoom = match cam.projection {
            gaanim_math::Projection::Orthographic { zoom } => zoom,
            _ => 1.0,
        };
        let effective = (cam.viewport_scale * proj_zoom).max(0.1);
        interactive.pan.x -= delta.x / effective;
        interactive.pan.y += delta.y / effective;
    }
}

fn editor_picking_system(
    egui_wants: Res<EguiWantsInput>,
    camera: Option<Res<Camera>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    entities: Query<(Entity, &WorldBounds, Option<&RenderOrder>)>,
    mut state: ResMut<EditorState>,
    interactive: Res<PreviewInteractive>,
) {
    let Some(camera) = camera else { return };
    if egui_wants.wants_any_pointer_input() {
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let is_perspective = matches!(camera.projection, gaanim_math::Projection::Perspective { .. });
    if !is_perspective && interactive.enabled {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let mut best_z = i32::MIN;
    let mut best_entity: Option<Entity> = None;
    let mut best_t: f64 = f64::INFINITY;

    if is_perspective {
        let (origin, dir) = camera.screen_to_ray(glam::DVec2::new(cursor_pos.x as f64, cursor_pos.y as f64));
        for (entity, bounds, render_order) in &entities {
            if let Some(t) = ray_aabb_intersect(origin, dir, bounds.0) {
                if t < best_t {
                    best_t = t;
                    best_entity = Some(entity);
                } else if (t - best_t).abs() < 1e-6 {
                    // Tie-break by render order
                    let z = render_order.map(|ro| ro.z_index).unwrap_or(0);
                    if z >= best_z {
                        best_z = z;
                        best_entity = Some(entity);
                    }
                }
            }
        }
    } else {
        let world_pos =
            camera.screen_to_world(glam::DVec2::new(cursor_pos.x as f64, cursor_pos.y as f64));
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
    }

    state.selected = best_entity;
}

fn ray_aabb_intersect(origin: glam::DVec3, dir: glam::DVec3, bounds: gaanim_math::Bounds3D) -> Option<f64> {
    let mut t_min = f64::NEG_INFINITY;
    let mut t_max = f64::INFINITY;
    for i in 0..3 {
        let (o, d, min, max) = match i {
            0 => (origin.x, dir.x, bounds.min.x, bounds.max.x),
            1 => (origin.y, dir.y, bounds.min.y, bounds.max.y),
            2 => (origin.z, dir.z, bounds.min.z, bounds.max.z),
            _ => unreachable!(),
        };
        if d.abs() < 1e-9 {
            if o < min || o > max {
                return None;
            }
        } else {
            let t1 = (min - o) / d;
            let t2 = (max - o) / d;
            let t_near = t1.min(t2);
            let t_far = t1.max(t2);
            t_min = t_min.max(t_near);
            t_max = t_max.min(t_far);
            if t_min > t_max {
                return None;
            }
        }
    }
    if t_max < 0.0 {
        return None;
    }
    Some(if t_min >= 0.0 { t_min } else { t_max })
}

/// System: adjusts the [`Camera`] resource so the animation preview fits in the
/// area above UI panels (the timeline at the bottom).
///
/// When [`PreviewInteractive`] is enabled, its `user_zoom` and `pan` are
/// composed on top of the fit scale so the user can inspect the scene
/// without losing the aspect-ratio fit on window resize.
fn viewport_adjust_system(
    inset: Res<ViewportInset>,
    interactive: Res<PreviewInteractive>,
    mut camera: Option<ResMut<Camera>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(ref mut cam) = camera else { return };
    let Ok(window) = windows.single() else { return };

    let window_h = window.height() as f64;
    let window_w = window.width() as f64;
    let available_h = window_h - inset.bottom as f64;

    let is_perspective = matches!(cam.projection, gaanim_math::Projection::Perspective { .. });
    if available_h < 1.0 {
        cam.viewport_offset_y = 0.0;
        cam.viewport_scale = 1.0;
        if interactive.enabled && !is_perspective {
            cam.position.x += interactive.pan.x;
            cam.position.y += interactive.pan.y;
        }
        return;
    }

    // Always fit animation into the available area while preserving aspect ratio.
    let anim_w = cam.viewport_width as f64;
    let anim_h = cam.viewport_height as f64;
    let scale_x = window_w / anim_w;
    let scale_y = available_h / anim_h;
    let fit_scale = scale_x.min(scale_y);

    if is_perspective {
        // For perspective, interactive orbit/pan/dolly already mutated Camera directly.
        cam.viewport_scale = fit_scale;
    } else if interactive.enabled {
        cam.viewport_scale = fit_scale * interactive.user_zoom.clamp(0.1, 20.0);
        cam.position.x += interactive.pan.x;
        cam.position.y += interactive.pan.y;
    } else {
        cam.viewport_scale = fit_scale;
        // Leave cam.position as set by the timeline (CameraZoom/Position lenses).
    }

    // Shift the Vello centre upward so the animation sits above the timeline.
    // When there is no timeline panel (inset.bottom == 0) the offset is 0.
    cam.viewport_offset_y = -(inset.bottom as f64) / 2.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_screen_shortcut_only_affects_active_presentations() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<AudienceBlank>()
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, presentation_blank_shortcuts_system);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyB);
        app.update();
        assert_eq!(
            *app.world().resource::<AudienceBlank>(),
            AudienceBlank::Black
        );

        app.world_mut().resource_mut::<PresentationMode>().active = false;
        app.update();
        assert_eq!(
            *app.world().resource::<AudienceBlank>(),
            AudienceBlank::None
        );
    }
}
