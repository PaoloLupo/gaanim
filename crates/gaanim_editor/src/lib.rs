use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, egui, input::EguiWantsInput};
use gaanim_math::{Camera, CameraViewOverride, CameraViewport, ResolvedCamera};
use gaanim_scene::{GltfModelRoot, Mesh3DMarker, RenderOrder, WorldBounds};
use gaanim_timeline::timeline::{PlaybackStopPolicy, Timeline};
use ui_kit::{ButtonTone, Icon, PRIMARY_SIZE, ToggleColor, divider, icon_button, palette};

#[cfg(target_os = "linux")]
pub mod alsa_errors;
mod app_icon;
pub mod export;
mod fps_overlay;
pub mod frame_profile;
pub mod narration;
pub mod overlays;
mod presenter;
pub mod project_hub;
mod ui_kit;

fn sync_editor_input_ignore_system(
    egui_wants: Res<EguiWantsInput>,
    presentation_mode: Res<PresentationMode>,
    editor_state: Res<EditorState>,
    narration: Option<Res<narration::NarrationSession>>,
    mut timeline: ResMut<Timeline>,
    mut stop_policy: ResMut<PlaybackStopPolicy>,
) {
    // While recording narration, the recorder owns the keyboard and playback.
    if let Some(policy) = narration.as_ref().and_then(|session| session.stop_policy()) {
        timeline.ignore_input = true;
        *stop_policy = policy;
        return;
    }
    timeline.ignore_input = presentation_mode.active
        || egui_wants.wants_keyboard_input()
        || egui_wants.wants_any_pointer_input();
    *stop_policy = if presentation_mode.active
        || (!editor_state.continuous_preview && !editor_state.segment_loop.is_active())
    {
        PlaybackStopPolicy::Respect
    } else {
        PlaybackStopPolicy::Ignore
    };
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
    pub view: PreviewView,
    pub free_camera: Option<Camera>,
    pub needs_frame: bool,
    pub detected_3d: bool,
    /// Multiplicative zoom factor applied on top of the fit `viewport_scale`.
    pub user_zoom: f64,
    /// Pan offset in world coordinates applied to `Camera.position`.
    pub pan: glam::DVec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewView {
    #[default]
    CameraView,
    Free3D,
}

impl Default for PreviewInteractive {
    fn default() -> Self {
        Self {
            enabled: false,
            view: PreviewView::CameraView,
            free_camera: None,
            needs_frame: false,
            detected_3d: false,
            user_zoom: 1.0,
            pan: glam::DVec2::ZERO,
        }
    }
}

impl PreviewInteractive {
    fn set_enabled(&mut self, enabled: bool, authored_camera: Option<Camera>) {
        self.enabled = enabled;
        if !enabled {
            self.view = PreviewView::CameraView;
            return;
        }

        self.user_zoom = 1.0;
        self.pan = glam::DVec2::ZERO;
        if self.detected_3d {
            self.free_camera = authored_camera;
            self.view = if self.free_camera.is_some() {
                PreviewView::Free3D
            } else {
                PreviewView::CameraView
            };
        } else {
            self.free_camera = None;
            self.view = PreviewView::CameraView;
        }
    }

    fn toggle(&mut self, authored_camera: Option<Camera>) {
        self.set_enabled(!self.enabled, authored_camera);
    }

    pub fn reset(&mut self) {
        self.user_zoom = 1.0;
        self.pan = glam::DVec2::ZERO;
        self.needs_frame = true;
    }
}

/// Output frame inside the editor window, in logical window pixels.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ViewportFrame {
    pub origin: glam::DVec2,
    pub size: glam::DVec2,
    pub output_size: glam::DVec2,
}

impl ViewportFrame {
    fn window_to_output(&self, point: glam::DVec2) -> Option<glam::DVec2> {
        let local = point - self.origin;
        if local.x < 0.0 || local.y < 0.0 || local.x > self.size.x || local.y > self.size.y {
            return None;
        }
        Some(glam::DVec2::new(
            local.x / self.size.x.max(1.0) * self.output_size.x,
            local.y / self.size.y.max(1.0) * self.output_size.y,
        ))
    }
}

pub struct GaanimEditorPlugin;

/// Every egui context (main window, presenter) draws square corners, like
/// the controls painted by [`ui_kit`].
fn square_egui_corners_system(
    mut contexts: Query<&mut bevy_egui::EguiContext, Added<bevy_egui::EguiContext>>,
) {
    for mut context in &mut contexts {
        context.get_mut().all_styles_mut(ui_kit::square_corners);
    }
}

impl Plugin for GaanimEditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .add_plugins(app_icon::AppIconPlugin)
            .add_plugins(project_hub::ProjectHubPlugin)
            .add_systems(PreUpdate, square_egui_corners_system)
            .init_resource::<EditorState>()
            .init_resource::<export::ExportState>()
            .init_resource::<export::StashedReplay>()
            .init_resource::<presenter::PresenterThumbnailCache>()
            .init_resource::<presenter::PresenterOverviewState>()
            .init_resource::<presenter::PresenterPreferences>()
            .init_resource::<presenter::AudienceControlsState>()
            .init_resource::<presenter::PresentationTimer>()
            .init_resource::<fps_overlay::FpsOverlay>()
            .init_resource::<EditorFullscreenState>()
            .init_resource::<PresentationMode>()
            .init_resource::<AudienceBlank>()
            .init_resource::<ViewportInset>()
            .init_resource::<ViewportFrame>()
            .init_resource::<PreviewInteractive>()
            .init_resource::<overlays::EditorOverlays>()
            .init_resource::<narration::NarrationPanel>()
            .init_resource::<narration::NarrationSession>()
            .init_resource::<gaanim_media::PreviewAudioEnabled>()
            .add_systems(
                Update,
                (
                    sync_editor_input_ignore_system
                        .in_set(gaanim_scene::hierarchy::SceneSet::Input)
                        .before(gaanim_timeline::timeline_playback_system),
                    narration::narration_session_system
                        .in_set(gaanim_scene::hierarchy::SceneSet::Input)
                        .after(sync_editor_input_ignore_system)
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
                    editor_fullscreen_keys_system,
                    presenter::presentation_input_system
                        .after(sync_editor_input_ignore_system)
                        .before(gaanim_timeline::timeline_playback_system),
                    presenter::sync_presentation_timer_system,
                    sync_fullscreen_letterbox_color_system
                        .after(editor_fullscreen_keys_system)
                        .after(presentation_escape_system),
                    presentation_escape_system,
                    fps_overlay::fps_overlay_system,
                ),
            )
            .add_systems(Startup, presenter::spawn_presenter_window_system)
            .add_systems(
                Update,
                presenter::cleanup_presenter_before_window_close_system
                    .before(bevy::window::close_when_requested),
            )
            .add_systems(Update, presenter::sync_presenter_camera_system)
            .add_systems(
                Update,
                (
                    detect_3d_content_system,
                    frame_free_camera_system,
                    viewport_adjust_system,
                )
                    .chain()
                    .in_set(gaanim_scene::hierarchy::SceneSet::Camera)
                    .after(gaanim_timeline::camera_rig_system)
                    .before(gaanim_scene::systems::resolve_camera_system),
            )
            .add_systems(EguiPrimaryContextPass, editor_ui_system)
            .add_systems(
                EguiPrimaryContextPass,
                presenter::audience_playback_controls_system.after(editor_ui_system),
            )
            .add_systems(
                EguiPrimaryContextPass,
                presentation_blank_overlay_system
                    .after(presenter::audience_playback_controls_system),
            )
            .add_systems(EguiPrimaryContextPass, export::export_dialog_system)
            .add_systems(
                EguiPrimaryContextPass,
                (
                    narration::narration_panel_system,
                    narration::narration_overlay_system,
                )
                    .after(editor_ui_system),
            )
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

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct SegmentLoopState {
    segment_bounds: Option<(f64, f64)>,
    previous_range: Option<(f64, f64)>,
    previous_was_full_duration: bool,
}

impl SegmentLoopState {
    fn is_active(&self) -> bool {
        self.segment_bounds.is_some()
    }

    fn deactivate(&mut self, timeline: &mut Timeline) {
        timeline.loop_range = self.previous_range;
        self.segment_bounds = None;
        self.previous_range = None;
        self.previous_was_full_duration = false;
    }

    fn clamp_range(&self, start: f64, end: f64) -> Option<(f64, f64)> {
        let (segment_start, segment_end) = self.segment_bounds?;
        let start = start.clamp(segment_start, segment_end);
        let end = end.clamp(segment_start, segment_end);
        (end - start > 1e-6).then_some((start, end))
    }
}

#[derive(Resource, Debug, Default)]
struct EditorFullscreenState {
    previous_mode: Option<bevy::window::WindowMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackDensity {
    Wide,
    Compact,
    Minimal,
}

impl PlaybackDensity {
    fn for_width(width: f32) -> Self {
        if width >= 1080.0 {
            Self::Wide
        } else if width >= 560.0 {
            Self::Compact
        } else {
            Self::Minimal
        }
    }

    /// Floating bar width: nearly edge to edge so long timelines with many
    /// scenes get room, capped so it stays a comfortable reading line.
    fn overlay_width(self, viewport_width: f32) -> f32 {
        let margin = if self == Self::Wide { 48.0 } else { 16.0 };
        (viewport_width - margin).clamp(0.0, 1600.0)
    }
}

#[derive(Resource)]
pub struct EditorState {
    pub selected: Option<Entity>,
    /// Whether the window is pinned always-on-top.
    pub pinned_on_top: bool,
    /// Play through authored presentation stops during editor preview.
    pub continuous_preview: bool,
    /// Hover time on the seek bar (in seconds), used for tooltip display.
    seek_bar_hover: Option<f64>,
    /// Auto-hide animation progress (0.0 = hidden, 1.0 = fully visible).
    bar_visibility: f32,
    /// Whether the cursor is currently hovering the playback bar.
    bar_hovered: bool,
    /// Interaction target selected when a seek-bar drag starts.
    seek_bar_drag_target: Option<SeekBarDragTarget>,
    segment_loop: SegmentLoopState,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            selected: None,
            pinned_on_top: false,
            continuous_preview: false,
            seek_bar_hover: None,
            bar_visibility: 1.0, // start visible
            bar_hovered: false,
            seek_bar_drag_target: None,
            segment_loop: SegmentLoopState::default(),
        }
    }
}

impl EditorState {
    /// Re-resolve an active editor segment loop after a script hot reload.
    /// The timeline engine remains independent from this editor-only state.
    pub fn reconcile_segment_loop_after_reload(
        &mut self,
        timeline: &mut Timeline,
        target_time: f64,
    ) {
        if !self.segment_loop.is_active() {
            return;
        }
        let Some(range) = scene_loop_range_at(timeline, target_time) else {
            self.segment_loop.deactivate(timeline);
            return;
        };
        self.segment_loop.segment_bounds = Some(range);
        if self.segment_loop.previous_was_full_duration {
            self.segment_loop.previous_range = Some((0.0, timeline.cached_duration.max(0.0)));
        }
        timeline.loop_range = Some(range);
    }
}

// Opaque margins hide foreground geometry extending outside the fitted scene.
fn paint_viewport_letterbox(ctx: &egui::Context, viewport_frame: &ViewportFrame, opaque: bool) {
    if viewport_frame.size.x > 0.0 && viewport_frame.size.y > 0.0 {
        let screen = ctx.viewport_rect();
        let x0 = viewport_frame.origin.x as f32;
        let y0 = viewport_frame.origin.y as f32;
        let x1 = (viewport_frame.origin.x + viewport_frame.size.x) as f32;
        let y1 = (viewport_frame.origin.y + viewport_frame.size.y) as f32;
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("viewport_letterbox"),
        ));
        let shade = if opaque {
            egui::Color32::BLACK
        } else {
            egui::Color32::from_black_alpha(145)
        };
        painter.rect_filled(
            egui::Rect::from_min_max(screen.min, egui::pos2(screen.max.x, y0)),
            0.0,
            shade,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(screen.min.x, y1), screen.max),
            0.0,
            shade,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(screen.min.x, y0), egui::pos2(x0, y1)),
            0.0,
            shade,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x1, y0), egui::pos2(screen.max.x, y1)),
            0.0,
            shade,
        );
        if !opaque {
            painter.rect_stroke(
                egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)),
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_white_alpha(70)),
                egui::StrokeKind::Inside,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn editor_ui_system(
    mut ctx: bevy_egui::EguiContexts,
    mut presentation_mode: ResMut<PresentationMode>,
    mut state: ResMut<EditorState>,
    (mut export_state, mut narration_panel, narration_session): (
        ResMut<export::ExportState>,
        ResMut<narration::NarrationPanel>,
        Res<narration::NarrationSession>,
    ),
    mut fullscreen_state: ResMut<EditorFullscreenState>,
    mut timeline: ResMut<Timeline>,
    mut inset: ResMut<ViewportInset>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    mut commands: Commands,
    fps_overlay: Res<fps_overlay::FpsOverlay>,
    mut render_health: Option<ResMut<gaanim_renderer::prelude::RenderHealth>>,
    vello_diagnostics: Option<Res<gaanim_renderer::prelude::VelloDiagnostics>>,
    interactive: Res<PreviewInteractive>,
    viewport_frame: Res<ViewportFrame>,
    hub: Option<Res<project_hub::ProjectHubState>>,
    presenter_windows: Query<(), With<presenter::PresenterWindow>>,
) {
    if hub.is_some_and(|hub| hub.active) {
        inset.bottom = 0.0;
        return;
    }
    if presentation_mode.active {
        inset.bottom = 0.0;
    }
    let Ok(ctx) = ctx.ctx_mut() else {
        return;
    };

    let opaque = presentation_mode.active
        || windows
            .single()
            .is_ok_and(|window| !matches!(window.mode, bevy::window::WindowMode::Windowed));
    paint_viewport_letterbox(ctx, &viewport_frame, opaque);
    if presentation_mode.active {
        return;
    }
    // The teleprompter replaces the playback bar while recording narration.
    if narration_session.capturing() {
        inset.bottom = 0.0;
        return;
    }
    let narration_open = narration_panel.open;

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

    // Temporal snapping is intentionally unavailable while inspecting 3D.
    // Keep the ordinary 2D preference untouched.
    let snapping_allowed = !interactive.detected_3d;

    // ── Compact playback overlay (auto-hide) ──────────────────────────────
    let scene_name: String = timeline
        .scene_at(timeline.current_time)
        .and_then(|id| timeline.scenes.get(id))
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let presentation_name = timeline.segment_label();
    let active_scene_loop_range = current_scene_loop_range(&timeline);
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
    // El playback es un overlay flotante y nunca reduce el viewport.
    inset.bottom = 0.0;

    if vis > 0.01 {
        let slide_offset = (1.0 - vis) * 10.0;
        let screen_w = vp.width();
        let density = PlaybackDensity::for_width(screen_w);
        let overlay_w = density.overlay_width(screen_w);
        let bottom_gap = if density == PlaybackDensity::Minimal {
            6.0
        } else {
            12.0
        };

        let area_resp = egui::Area::new("playback_overlay".into())
            .anchor(
                egui::Align2::CENTER_BOTTOM,
                egui::vec2(0.0, slide_offset - bottom_gap),
            )
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_opacity(vis);
                let (horizontal_margin, vertical_margin) = match density {
                    PlaybackDensity::Wide => (16, 12),
                    PlaybackDensity::Compact => (12, 10),
                    PlaybackDensity::Minimal => (8, 8),
                };
                egui::Frame::new()
                    .fill(palette::PANEL)
                    .inner_margin(egui::Margin {
                        left: horizontal_margin,
                        right: horizontal_margin,
                        top: vertical_margin,
                        bottom: vertical_margin - 2,
                    })
                    .stroke(egui::Stroke::new(1.0, palette::PANEL_STROKE))
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 28,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(110),
                    })
                    .show(ui, |ui| {
                        let content_w = (overlay_w - 2.0 * horizontal_margin as f32).max(0.0);
                        ui.set_width(content_w);
                        ui.spacing_mut().item_spacing = egui::vec2(2.0, 8.0);

                        // Row 1: chapters + seek track
                        let frac = if total > 0.0 {
                            (current / total) as f32
                        } else {
                            0.0
                        };
                        let total_f32 = total.max(0.001) as f32;
                        let loop_frac = state.segment_loop.is_active().then(|| {
                            let (s, e) = timeline.loop_range.unwrap_or((0.0, 0.0));
                            (s as f32 / total_f32, e as f32 / total_f32)
                        });
                        let bp_fracs: Vec<f32> = timeline
                            .segments
                            .iter()
                            .flat_map(|segment| segment.stops.iter())
                            .map(|stop| stop.time as f32 / total_f32)
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

                        let seek_markers: Vec<SeekMarker> = timeline
                            .markers
                            .iter()
                            .map(|marker| SeekMarker {
                                frac: (marker.time as f32 / total_f32).clamp(0.0, 1.0),
                                time: marker.time,
                                name: marker.name.clone(),
                            })
                            .collect();
                        let snap_points: Vec<f32> = bp_fracs
                            .iter()
                            .copied()
                            .chain(seek_markers.iter().map(|marker| marker.frac))
                            .collect();

                        let seek_resp = paint_seek_bar(
                            ui,
                            frac,
                            loop_frac,
                            &bp_fracs,
                            &scene_segs,
                            &mut state.seek_bar_drag_target,
                            total,
                            snapping_allowed,
                            &seek_markers,
                        );
                        if let Some(time) = seek_resp.marker_jump {
                            timeline.seek_request = Some(time);
                        } else if let Some(new_frac) = seek_resp.seek_to {
                            let snapped_frac = snap_seek_fraction(
                                new_frac,
                                &scene_segs,
                                &snap_points,
                                snapping_allowed,
                            );
                            timeline.seek_request = Some(snapped_frac as f64 * total);
                        }
                        if let Some((ls, le)) = seek_resp.loop_drag {
                            let s = (ls as f64 * total).clamp(0.0, total);
                            let e = (le as f64 * total).clamp(0.0, total);
                            if let Some(range) = state.segment_loop.clamp_range(s.min(e), s.max(e))
                                && range.1 - range.0 > 0.05
                            {
                                timeline.loop_range = Some(range);
                            }
                        }
                        state.seek_bar_hover = seek_resp.hover_time;

                        // Row 2: transport · time · scene | toggles · window actions
                        let prev_scene = adjacent_scene_time(&scene_segs, frac, total, false);
                        let next_scene = adjacent_scene_time(&scene_segs, frac, total, true);
                        let has_scenes = !scene_segs.is_empty();
                        let scene_text = presentation_name
                            .clone()
                            .or_else(|| (!scene_name.is_empty()).then(|| scene_name.clone()))
                            .or_else(|| {
                                has_scenes.then(|| format!("{} escenas", scene_segs.len()))
                            });
                        let loop_on = state.segment_loop.is_active();
                        let is_playing = timeline.is_playing;
                        let playback_rate = timeline.playback_rate;
                        let pinned = state.pinned_on_top;
                        let continuous = state.continuous_preview;

                        let (left_actions, right_actions) = egui::Sides::new()
                            .height(PRIMARY_SIZE)
                            .spacing(12.0)
                            .shrink_left()
                            .truncate()
                            .show(
                                ui,
                                |ui| {
                                    let mut actions = Vec::new();
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    if density != PlaybackDensity::Minimal
                                        && icon_button(ui, Icon::SkipStart, ButtonTone::Ghost, true)
                                            .on_hover_text("Inicio · Home")
                                            .clicked()
                                    {
                                        actions.push(PlaybackAction::Jump(0.0));
                                    }
                                    if density != PlaybackDensity::Minimal
                                        && has_scenes
                                        && icon_button(
                                            ui,
                                            Icon::PrevScene,
                                            ButtonTone::Ghost,
                                            prev_scene.is_some(),
                                        )
                                        .on_hover_text("Escena anterior · ←")
                                        .clicked()
                                        && let Some(target) = prev_scene
                                    {
                                        actions.push(PlaybackAction::Seek(target));
                                    }
                                    ui.add_space(2.0);
                                    let play_icon = if is_playing {
                                        Icon::Pause
                                    } else {
                                        Icon::Play
                                    };
                                    if icon_button(ui, play_icon, ButtonTone::Primary, true)
                                        .on_hover_text("Reproducir / Pausa · Espacio")
                                        .clicked()
                                    {
                                        actions.push(PlaybackAction::TogglePlay);
                                    }
                                    ui.add_space(2.0);
                                    if density != PlaybackDensity::Minimal
                                        && has_scenes
                                        && icon_button(
                                            ui,
                                            Icon::NextScene,
                                            ButtonTone::Ghost,
                                            next_scene.is_some(),
                                        )
                                        .on_hover_text("Escena siguiente · →")
                                        .clicked()
                                        && let Some(target) = next_scene
                                    {
                                        actions.push(PlaybackAction::Seek(target));
                                    }
                                    if density != PlaybackDensity::Minimal
                                        && icon_button(ui, Icon::SkipEnd, ButtonTone::Ghost, true)
                                            .on_hover_text("Final · End")
                                            .clicked()
                                    {
                                        actions.push(PlaybackAction::Jump(total));
                                    }

                                    ui.add_space(10.0);
                                    let time_resp = ui
                                        .add(
                                            egui::Label::new(
                                                egui::RichText::new(format_time(current))
                                                    .monospace()
                                                    .size(13.0)
                                                    .color(palette::TEXT),
                                            )
                                            .selectable(false)
                                            .sense(egui::Sense::click()),
                                        )
                                        .on_hover_text("Click para copiar el timecode");
                                    if time_resp.clicked() {
                                        ui.ctx().copy_text(format_time(current));
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!(
                                                " / {}",
                                                format_time(total)
                                            ))
                                            .monospace()
                                            .size(13.0)
                                            .color(palette::TEXT_FAINT),
                                        )
                                        .selectable(false),
                                    );

                                    if density != PlaybackDensity::Minimal
                                        && let Some(text) = &scene_text
                                    {
                                        ui.add_space(6.0);
                                        divider(ui);
                                        ui.add_space(6.0);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(text)
                                                    .size(13.0)
                                                    .color(palette::TEXT_MUTED),
                                            )
                                            .selectable(false)
                                            .truncate(),
                                        );
                                    }
                                    actions
                                },
                                |ui| {
                                    let mut actions = Vec::new();
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    if density == PlaybackDensity::Wide {
                                        let pin_tone = if pinned {
                                            ButtonTone::On(ToggleColor::Stop)
                                        } else {
                                            ButtonTone::Ghost
                                        };
                                        if icon_button(ui, Icon::Pin, pin_tone, true)
                                            .on_hover_text(if pinned {
                                                "Desfijar ventana"
                                            } else {
                                                "Fijar ventana encima"
                                            })
                                            .clicked()
                                        {
                                            actions.push(PlaybackAction::TogglePin);
                                        }

                                        let narration_tone = if narration_open {
                                            ButtonTone::On(ToggleColor::Accent)
                                        } else {
                                            ButtonTone::Ghost
                                        };
                                        if icon_button(ui, Icon::Mic, narration_tone, true)
                                            .on_hover_text("Narración: grabar la voz de la escena")
                                            .clicked()
                                        {
                                            actions.push(PlaybackAction::ToggleNarration);
                                        }

                                        if is_exporting {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format!(
                                                        "{export_current}/{export_total}"
                                                    ))
                                                    .size(11.0)
                                                    .color(palette::TEXT_MUTED),
                                                )
                                                .selectable(false),
                                            );
                                            ui.add(
                                                egui::ProgressBar::new(export_progress_pct)
                                                    .desired_width(84.0)
                                                    .desired_height(6.0)
                                                    .fill(palette::ACCENT)
                                                    .corner_radius(0.0),
                                            );
                                        } else if icon_button(
                                            ui,
                                            Icon::Export,
                                            ButtonTone::Ghost,
                                            true,
                                        )
                                        .on_hover_text("Exportar animación")
                                        .clicked()
                                        {
                                            actions.push(PlaybackAction::OpenExport);
                                        }

                                        if icon_button(ui, Icon::Present, ButtonTone::Ghost, true)
                                            .on_hover_text("Presentar a pantalla completa")
                                            .clicked()
                                        {
                                            actions.push(PlaybackAction::Present);
                                        }
                                        if icon_button(
                                            ui,
                                            Icon::Fullscreen,
                                            ButtonTone::Ghost,
                                            true,
                                        )
                                        .on_hover_text("Pantalla completa · F11")
                                        .clicked() {
 actions.push(PlaybackAction::ToggleFullscreen);
 }
                                        divider(ui);

                                        let continuous_tone = if continuous {
                                            ButtonTone::On(ToggleColor::Accent)
                                        } else {
                                            ButtonTone::Ghost
                                        };
                                        if icon_button(
                                            ui,
                                            Icon::Continuous,
                                            continuous_tone,
                                            true,
                                        )
                                        .on_hover_text(
                                            "Reproducción continua: ignora los scene.stop() en el editor",
                                        )
                                        .clicked()
                                        {
                                            actions.push(PlaybackAction::ToggleContinuous);
                                        }
                                    } else {
                                        let more = icon_button(ui, Icon::More, ButtonTone::Ghost, true)
                                            .on_hover_text("Más controles");
                                        egui::Popup::menu(&more).show(|ui| {
                                            ui.set_min_width(180.0);
                                            if density == PlaybackDensity::Minimal {
                                                if ui.button("Inicio").clicked() {
                                                    actions.push(PlaybackAction::Jump(0.0));
                                                }
                                                if ui
                                                    .add_enabled(
                                                        prev_scene.is_some(),
                                                        egui::Button::new("Escena anterior"),
                                                    )
                                                    .clicked()
                                                    && let Some(target) = prev_scene
                                                {
                                                    actions.push(PlaybackAction::Seek(target));
                                                }
                                                if ui
                                                    .add_enabled(
                                                        next_scene.is_some(),
                                                        egui::Button::new("Escena siguiente"),
                                                    )
                                                    .clicked()
                                                    && let Some(target) = next_scene
                                                {
                                                    actions.push(PlaybackAction::Seek(target));
                                                }
                                                if ui.button("Final").clicked() {
                                                    actions.push(PlaybackAction::Jump(total));
                                                }
                                                ui.separator();
                                            }
                                            ui.menu_button(
                                                format!(
                                                    "Velocidad · {}",
                                                    format_rate(playback_rate)
                                                ),
                                                |ui| {
                                                    for rate in SPEED_PRESETS {
                                                        if ui
                                                            .selectable_label(
                                                                (playback_rate - rate).abs()
                                                                    < f64::EPSILON,
                                                                format_rate(rate),
                                                            )
                                                            .clicked()
                                                        {
                                                            actions.push(PlaybackAction::SetRate(rate));
                                                        }
                                                    }
                                                },
                                            );
                                            if ui
                                                .selectable_label(continuous, "Reproducción continua")
                                                .clicked()
                                            {
                                                actions.push(PlaybackAction::ToggleContinuous);
                                            }
                                            ui.separator();
                                            if ui.button("Pantalla completa · F11").clicked() {
 actions.push(PlaybackAction::ToggleFullscreen);
 }
                                            if ui.button("Presentar").clicked() {
                                                actions.push(PlaybackAction::Present);
                                            }
                                            if ui
                                                .selectable_label(narration_open, "Narración")
                                                .clicked()
                                            {
                                                actions.push(PlaybackAction::ToggleNarration);
                                            }
                                            if ui
                                                .add_enabled(
                                                    !is_exporting,
                                                    egui::Button::new("Exportar"),
                                                )
                                                .clicked()
                                            {
                                                actions.push(PlaybackAction::OpenExport);
                                            }
                                            if is_exporting {
                                                ui.label(format!(
                                                    "Exportando {:.0}% · {}/{}",
                                                    export_progress_pct * 100.0,
                                                    export_current,
                                                    export_total,
                                                ));
                                            }
                                            let pin_label = if pinned {
                                                "Desfijar ventana"
                                            } else {
                                                "Fijar ventana encima"
                                            };
                                            if ui.button(pin_label).clicked() {
                                                actions.push(PlaybackAction::TogglePin);
                                            }
                                        });
                                    }

                                    if density != PlaybackDensity::Minimal {
                                        let loop_tone = if loop_on {
                                            ButtonTone::On(ToggleColor::Loop)
                                        } else {
                                            ButtonTone::Ghost
                                        };
                                        if icon_button(
                                            ui,
                                            Icon::Loop,
                                            loop_tone,
                                            active_scene_loop_range.is_some(),
                                        )
                                        .on_hover_text(
                                            "Loop del segmento actual · L\nArrastra los tiradores para refinar",
                                        )
                                        .clicked()
                                            && let Some(range) = active_scene_loop_range
                                        {
                                            actions.push(PlaybackAction::ToggleLoop(range));
                                        }
                                    }

                                    if density == PlaybackDensity::Wide
                                        && let Some(rate) = speed_control(ui, playback_rate)
                                    {
                                        actions.push(PlaybackAction::SetRate(rate));
                                    }
                                    actions
                                },
                            );
                        for action in left_actions.into_iter().chain(right_actions) {
                            match action {
                                PlaybackAction::TogglePlay => {
                                    timeline.is_playing = !timeline.is_playing;
                                }
                                PlaybackAction::Seek(time) => timeline.seek_request = Some(time),
                                PlaybackAction::Jump(time) => {
                                    timeline.is_playing = false;
                                    timeline.seek_request = Some(time);
                                }
                                PlaybackAction::SetRate(rate) => timeline.playback_rate = rate,
                                PlaybackAction::ToggleContinuous => {
                                    state.continuous_preview = !state.continuous_preview;
                                }
                                PlaybackAction::ToggleLoop(range) => toggle_scene_loop_range(
                                    &mut state.segment_loop,
                                    &mut timeline,
                                    range,
                                ),
                                PlaybackAction::OpenExport => export_state.dialog_open = true,
                                PlaybackAction::ToggleNarration => {
                                    narration_panel.open = !narration_panel.open;
                                }
                                PlaybackAction::Present => start_presentation(
                                    &mut presentation_mode,
                                    &mut fullscreen_state,
                                    &mut windows,
                                    &mut commands,
                                    &presenter_windows,
                                ),
                                PlaybackAction::ToggleFullscreen => {
                                    if let Ok(mut window) = windows.single_mut() {
                                        toggle_editor_fullscreen(&mut window, &mut fullscreen_state);
                                    }
                                }
                                PlaybackAction::TogglePin => {
                                    toggle_pinned_on_top(&mut state, &mut windows);
                                }
                            }
                        }
                    });
            });
        // Keep the bar visible while the pointer is over it.
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());
        state.bar_hovered = hover_pos.is_some_and(|p| area_resp.response.rect.contains(p));
    }

    if fps_overlay::render_render_health(ctx, render_health.as_deref())
        && let Some(health) = render_health.as_deref_mut()
    {
        health.request_retry();
    }
    fps_overlay.render(ctx, vello_diagnostics.as_deref());
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

fn current_scene_loop_range(timeline: &Timeline) -> Option<(f64, f64)> {
    scene_loop_range_at(timeline, timeline.current_time)
}

fn scene_loop_range_at(timeline: &Timeline, time: f64) -> Option<(f64, f64)> {
    if let Some(position) = timeline.segment_position_at(time)
        && let Some(segment) = timeline
            .segments
            .iter()
            .find(|segment| segment.id == position.segment_id)
    {
        return Some((segment.start_time, segment.end_time));
    }
    timeline
        .scene_at(time)
        .and_then(|scene| timeline.scene_bounds(scene))
}

fn toggle_scene_loop_range(
    state: &mut SegmentLoopState,
    timeline: &mut Timeline,
    range: (f64, f64),
) {
    const EPSILON: f64 = 1e-6;
    let (start, end) = (range.0.min(range.1), range.0.max(range.1));
    if !start.is_finite() || !end.is_finite() || end - start <= EPSILON {
        return;
    }

    let same_range = state
        .segment_bounds
        .is_some_and(|(active_start, active_end)| {
            (active_start - start).abs() <= EPSILON && (active_end - end).abs() <= EPSILON
        });
    if same_range {
        state.deactivate(timeline);
        return;
    }

    if !state.is_active() {
        state.previous_range = timeline.loop_range;
        state.previous_was_full_duration =
            timeline.loop_range.is_some_and(|(old_start, old_end)| {
                old_start.abs() <= EPSILON
                    && (old_end - timeline.cached_duration.max(0.0)).abs() <= EPSILON
            });
    }
    state.segment_bounds = Some((start, end));
    timeline.loop_range = Some((start, end));
    timeline.seek_request = Some(start);
    timeline.is_playing = true;
}

/// A scene's time range, precomputed for the seek bar.
struct SceneSegment {
    name: String,
    start_frac: f32,
    end_frac: f32,
}

fn adjacent_scene_time(
    scenes: &[SceneSegment],
    fraction: f32,
    total: f64,
    next: bool,
) -> Option<f64> {
    let first = scenes.first()?;
    let last = scenes.last()?;
    let current = scenes
        .iter()
        .position(|scene| fraction >= scene.start_frac && fraction < scene.end_frac + 0.005);
    let target = if next {
        if fraction < first.start_frac {
            Some(first.start_frac)
        } else {
            current
                .and_then(|index| scenes.get(index + 1))
                .map(|scene| scene.start_frac)
        }
    } else if fraction >= last.end_frac - 0.005 {
        Some(last.start_frac)
    } else {
        current
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| scenes.get(index))
            .map(|scene| scene.start_frac)
    };
    target.map(|fraction| fraction as f64 * total)
}

/// Result from painting the custom seek bar.
struct SeekBarResponse {
    /// If Some, the user wants to seek to this fraction (0.0..=1.0).
    seek_to: Option<f32>,
    /// The time at the hover position, for tooltip display.
    hover_time: Option<f64>,
    /// If Some, the user dragged a loop handle to a new (start_frac, end_frac).
    loop_drag: Option<(f32, f32)>,
    /// Exact time of a `scene.marker` the user clicked; wins over `seek_to`.
    marker_jump: Option<f64>,
}

/// A `scene.marker` drawn on the seek bar.
struct SeekMarker {
    frac: f32,
    time: f64,
    name: String,
}

const MARKER_COLOR: egui::Color32 = egui::Color32::from_rgb(214, 140, 255);
const MARKER_HIT_RADIUS: f32 = 5.0;

/// Index of the marker under `x`, preferring the closest one.
fn marker_at(markers: &[SeekMarker], x: f32, x_at: impl Fn(f32) -> f32) -> Option<usize> {
    markers
        .iter()
        .enumerate()
        .map(|(index, marker)| (index, (x - x_at(marker.frac)).abs()))
        .filter(|(_, distance)| *distance < MARKER_HIT_RADIUS)
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeekBarDragTarget {
    Seek,
    LoopStart,
    LoopEnd,
}

const LOOP_HANDLE_HIT_RADIUS: f32 = 12.0;

fn seek_bar_drag_target(origin_x: f32, left_x: f32, right_x: f32) -> SeekBarDragTarget {
    let dist_left = (origin_x - left_x).abs();
    let dist_right = (origin_x - right_x).abs();

    if dist_left < LOOP_HANDLE_HIT_RADIUS && dist_left <= dist_right {
        SeekBarDragTarget::LoopStart
    } else if dist_right < LOOP_HANDLE_HIT_RADIUS {
        SeekBarDragTarget::LoopEnd
    } else {
        SeekBarDragTarget::Seek
    }
}

fn snap_seek_fraction(
    fraction: f32,
    scenes: &[SceneSegment],
    stops: &[f32],
    snapping_enabled: bool,
) -> f32 {
    if !snapping_enabled {
        return fraction;
    }
    const THRESHOLD: f32 = 0.015;
    for scene in scenes {
        if (fraction - scene.start_frac).abs() < THRESHOLD {
            return scene.start_frac;
        }
        if (fraction - scene.end_frac).abs() < THRESHOLD {
            return scene.end_frac;
        }
    }
    stops
        .iter()
        .copied()
        .find(|stop| (fraction - stop).abs() < THRESHOLD)
        .unwrap_or(fraction)
}

/// Paint the seek bar: a chapter lane (one chip per scene), a track split at
/// the same boundaries, stop dots, the loop region, and the playhead.
///
/// The design stays legible with dozens of scenes: chapters are separated by
/// gaps instead of ornaments, a label is drawn only when it fits its chip,
/// and the full name of any chapter is shown in the hover tooltip.
#[allow(clippy::too_many_arguments)]
fn paint_seek_bar(
    ui: &mut egui::Ui,
    frac: f32,
    loop_frac: Option<(f32, f32)>,
    bp_fracs: &[f32],
    scenes: &[SceneSegment],
    drag_target: &mut Option<SeekBarDragTarget>,
    total: f64,
    snapping_enabled: bool,
    markers: &[SeekMarker],
) -> SeekBarResponse {
    const LANE_H: f32 = 22.0;
    const LANE_GAP: f32 = 6.0;
    const TRACK_ZONE_H: f32 = 18.0;
    const CHAPTER_GAP: f32 = 2.0;

    let has_scenes = !scenes.is_empty();
    let lane_h = if has_scenes { LANE_H + LANE_GAP } else { 0.0 };
    let (response, painter) = ui.allocate_painter(
        egui::vec2(ui.available_width(), lane_h + TRACK_ZONE_H),
        egui::Sense::click_and_drag(),
    );
    let rect = response.rect;
    let x_at = |f: f32| rect.min.x + f * rect.width();
    let lane_rect = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + LANE_H));
    let bar_y = rect.min.y + lane_h + TRACK_ZONE_H / 2.0;
    let is_hovering = response.hovered();
    let is_dragging = response.dragged();
    let active = is_hovering || is_dragging;
    let track_h = if active { 6.0 } else { 4.0 };
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, bar_y - track_h / 2.0),
        egui::pos2(rect.max.x, bar_y + track_h / 2.0),
    );
    let playhead_x = x_at(frac);
    let pointer = response.hover_pos();
    let hover_frac = pointer.map(|pos| ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0));
    let scene_at = |f: f32| {
        scenes
            .iter()
            .position(|s| f >= s.start_frac && f < s.end_frac + 0.001)
    };
    let active_scene_idx = scene_at(frac).or_else(|| {
        // At the very end, keep the last scene highlighted.
        (frac >= 0.99)
            .then(|| scenes.iter().rposition(|s| frac >= s.start_frac))
            .flatten()
    });
    let hovered_scene_idx = hover_frac.and_then(scene_at);

    // ── Chapter lane ────────────────────────────────────────────────────
    let label_font = egui::FontId::proportional(11.5);
    let mut chips = Vec::with_capacity(scenes.len());
    let mut labeled = vec![false; scenes.len()];
    for (i, seg) in scenes.iter().enumerate() {
        let (sx, ex) = (x_at(seg.start_frac), x_at(seg.end_frac));
        if ex - sx < 1.0 {
            chips.push(None);
            continue;
        }
        let inset = if ex - sx > 4.0 * CHAPTER_GAP {
            CHAPTER_GAP / 2.0
        } else {
            0.0
        };
        let chip = egui::Rect::from_min_max(
            egui::pos2(sx + inset, lane_rect.min.y),
            egui::pos2(ex - inset, lane_rect.max.y),
        );
        chips.push(Some(chip));
        let is_active = active_scene_idx == Some(i);
        let is_hovered = hovered_scene_idx == Some(i);
        let base = if is_active {
            palette::ACCENT.gamma_multiply(0.16)
        } else if is_hovered {
            egui::Color32::from_white_alpha(22)
        } else {
            egui::Color32::from_white_alpha(10)
        };
        painter.rect_filled(chip, 0.0, base);
        if is_active {
            // Progress inside the current chapter.
            let progress_x = playhead_x.clamp(chip.min.x, chip.max.x);
            let clip = egui::Rect::from_min_max(chip.min, egui::pos2(progress_x, chip.max.y));
            painter
                .with_clip_rect(clip.intersect(painter.clip_rect()))
                .rect_filled(chip, 0.0, palette::ACCENT.gamma_multiply(0.22));
        }

        let name = scene_display_name(&seg.name);
        if name.is_empty() || chip.width() < 24.0 {
            continue;
        }
        let color = if is_active || is_hovered {
            palette::TEXT
        } else {
            palette::TEXT_MUTED
        };
        let galley = painter.layout_no_wrap(name.to_owned(), label_font.clone(), color);
        if galley.size().x <= chip.width() - 14.0 {
            painter.galley(chip.center() - galley.size() / 2.0, galley, color);
            labeled[i] = true;
        }
    }

    // Dense timelines: chapters too short for their own names are labeled
    // by the group they share a leading name with ("Fundamentos · …").
    for group in scene_groups(scenes) {
        if group.members.clone().any(|i| labeled[i]) {
            continue;
        }
        let spans: Vec<egui::Rect> = group.members.clone().filter_map(|i| chips[i]).collect();
        let (Some(first), Some(last)) = (spans.first(), spans.last()) else {
            continue;
        };
        let span = first.union(*last);
        let contains_active = active_scene_idx.is_some_and(|i| group.members.contains(&i));
        let color = if contains_active {
            palette::TEXT
        } else {
            palette::TEXT_MUTED
        };
        let galley = painter.layout_no_wrap(group.label.to_owned(), label_font.clone(), color);
        if galley.size().x <= span.width() - 10.0 {
            painter.galley(span.center() - galley.size() / 2.0, galley, color);
        }
    }

    // The current chapter always shows its full name: when it does not fit
    // its chip, it floats over the neighbouring chips inside the lane.
    if let Some(i) = active_scene_idx
        && !labeled[i]
        && let Some(chip) = chips[i]
    {
        let name = scene_display_name(&scenes[i].name);
        if !name.is_empty() {
            let pad = 8.0;
            let max_chars = ((lane_rect.width() - 2.0 * pad) / 6.0).max(4.0) as usize;
            let galley = painter.layout_no_wrap(
                truncate_with_ellipsis(name, max_chars),
                label_font.clone(),
                palette::TEXT,
            );
            let width = (galley.size().x + 2.0 * pad).min(lane_rect.width());
            let x = (chip.center().x - width / 2.0).clamp(
                lane_rect.min.x,
                (lane_rect.max.x - width).max(lane_rect.min.x),
            );
            let pill = egui::Rect::from_min_size(
                egui::pos2(x, lane_rect.min.y),
                egui::vec2(width, lane_rect.height()),
            );
            painter.rect_filled(pill, 0.0, palette::SURFACE);
            painter.rect_filled(pill, 0.0, palette::ACCENT.gamma_multiply(0.22));
            painter.rect_stroke(
                pill,
                0.0,
                egui::Stroke::new(1.0, palette::ACCENT.gamma_multiply(0.55)),
                egui::StrokeKind::Inside,
            );
            painter.galley(pill.center() - galley.size() / 2.0, galley, palette::TEXT);
        }
    }

    // ── Track, split at chapter boundaries ──────────────────────────────
    let min_piece_px = 3.0 * CHAPTER_GAP;
    let mut cuts: Vec<f32> = scenes
        .iter()
        .flat_map(|s| [s.start_frac, s.end_frac])
        .filter(|f| *f > 0.001 && *f < 0.999)
        .collect();
    cuts.sort_by(f32::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() * rect.width() < min_piece_px);
    let bounds: Vec<f32> = std::iter::once(0.0)
        .chain(cuts)
        .chain(std::iter::once(1.0))
        .collect();
    let track_color = egui::Color32::from_white_alpha(if active { 34 } else { 26 });
    for piece in bounds.windows(2) {
        let x0 = x_at(piece[0])
            + if piece[0] > 0.0 {
                CHAPTER_GAP / 2.0
            } else {
                0.0
            };
        let x1 = x_at(piece[1])
            - if piece[1] < 1.0 {
                CHAPTER_GAP / 2.0
            } else {
                0.0
            };
        if x1 - x0 < 1.0 {
            continue;
        }
        let piece_rect = egui::Rect::from_min_max(
            egui::pos2(x0, bar_rect.min.y),
            egui::pos2(x1, bar_rect.max.y),
        );
        painter.rect_filled(piece_rect, 0.0, track_color);
        let fill_x = playhead_x.min(x1);
        if fill_x > x0 {
            let clip =
                egui::Rect::from_min_max(piece_rect.min, egui::pos2(fill_x, piece_rect.max.y));
            painter
                .with_clip_rect(clip.intersect(painter.clip_rect()))
                .rect_filled(piece_rect, 0.0, palette::ACCENT);
        }
    }

    // ── Loop region ─────────────────────────────────────────────────────
    let loop_x = loop_frac.map(|(ls, le)| (x_at(ls), x_at(le)));
    if let Some((lx0, lx1)) = loop_x {
        let band = egui::Rect::from_min_max(
            egui::pos2(lx0, bar_y - TRACK_ZONE_H / 2.0 + 1.0),
            egui::pos2(lx1, bar_y + TRACK_ZONE_H / 2.0 - 1.0),
        );
        painter.rect_filled(band, 0.0, palette::LOOP.gamma_multiply(0.14));
        for hx in [lx0, lx1] {
            let handle = egui::Rect::from_center_size(egui::pos2(hx, bar_y), egui::vec2(5.0, 16.0));
            painter.rect_filled(handle, 0.0, palette::LOOP);
            painter.line_segment(
                [egui::pos2(hx, bar_y - 4.0), egui::pos2(hx, bar_y + 4.0)],
                egui::Stroke::new(1.0, egui::Color32::from_black_alpha(120)),
            );
        }
        if let Some(pos) = pointer
            && (pos.y - bar_y).abs() < 12.0
            && ((pos.x - lx0).abs() < 8.0 || (pos.x - lx1).abs() < 8.0)
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }

    // ── Stops: quiet squares under the track ───────────────────────────────
    let stop_y = bar_y + TRACK_ZONE_H / 2.0 - 2.0;
    for &bp in bp_fracs {
        let bx = x_at(bp);
        let near_pointer = pointer.is_some_and(|pos| (pos.x - bx).abs() < 4.0);
        let color = if bp <= frac + 1e-4 {
            palette::STOP.gamma_multiply(0.45)
        } else {
            palette::STOP.gamma_multiply(0.9)
        };
        painter.rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(bx, stop_y),
                egui::Vec2::splat(if near_pointer { 5.0 } else { 3.0 }),
            ),
            0.0,
            color,
        );
    }

    // ── Markers: small downward triangles above the track ─────────────────
    let in_track_zone = |pos: egui::Pos2| pos.y > lane_rect.max.y || !has_scenes;
    let hovered_marker = pointer
        .filter(|pos| in_track_zone(*pos))
        .and_then(|pos| marker_at(markers, pos.x, x_at));
    let marker_top = bar_y - TRACK_ZONE_H / 2.0;
    for (index, marker) in markers.iter().enumerate() {
        let mx = x_at(marker.frac);
        let half = if hovered_marker == Some(index) {
            5.0
        } else {
            3.5
        };
        let color = if marker.frac <= frac + 1e-4 {
            MARKER_COLOR.gamma_multiply(0.55)
        } else {
            MARKER_COLOR
        };
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(mx - half, marker_top),
                egui::pos2(mx + half, marker_top),
                egui::pos2(mx, marker_top + half * 1.3),
            ],
            color,
            egui::Stroke::NONE,
        ));
    }
    if hovered_marker.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let snap_points: Vec<f32> = bp_fracs
        .iter()
        .copied()
        .chain(markers.iter().map(|marker| marker.frac))
        .collect();
    let bp_fracs = snap_points.as_slice();

    // ── Hover guide + snap indicator ────────────────────────────────────
    let mut hover_time = None;
    if let (Some(pos), Some(hf)) = (pointer, hover_frac) {
        let hover_secs = hf as f64 * total;
        hover_time = Some(hover_secs);
        if !is_dragging {
            painter.line_segment(
                [
                    egui::pos2(pos.x, rect.min.y),
                    egui::pos2(pos.x, bar_rect.max.y),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_white_alpha(46)),
            );
        }
        if snapping_enabled {
            let snapped = snap_seek_fraction(hf, scenes, bp_fracs, true);
            if snapped != hf {
                let sx = x_at(snapped);
                painter.line_segment(
                    [
                        egui::pos2(sx, rect.min.y),
                        egui::pos2(sx, bar_rect.max.y + 3.0),
                    ],
                    egui::Stroke::new(1.5, palette::STOP),
                );
            }
        }

        // Tooltip: "Scene · 0:12.34", kept inside the bar horizontally.
        let name = hovered_marker
            .map(|i| markers[i].name.as_str())
            .or_else(|| hovered_scene_idx.map(|i| scene_display_name(&scenes[i].name)))
            .unwrap_or("");
        let mut job = egui::text::LayoutJob::default();
        if !name.is_empty() {
            job.append(
                &truncate_with_ellipsis(name, 40),
                0.0,
                egui::TextFormat::simple(egui::FontId::proportional(12.0), palette::TEXT),
            );
            job.append(
                "  ",
                0.0,
                egui::TextFormat::simple(egui::FontId::proportional(12.0), palette::TEXT),
            );
        }
        job.append(
            &format_time(hover_secs),
            0.0,
            egui::TextFormat::simple(egui::FontId::monospace(12.0), palette::TEXT_MUTED),
        );
        let galley = painter.layout_job(job);
        let pad = egui::vec2(9.0, 5.0);
        let size = galley.size() + pad * 2.0;
        let x = (pos.x - size.x / 2.0).clamp(rect.min.x, (rect.max.x - size.x).max(rect.min.x));
        let tip = egui::Rect::from_min_size(egui::pos2(x, rect.min.y - size.y - 10.0), size);
        painter.rect_filled(
            tip.translate(egui::vec2(0.0, 2.0)),
            0.0,
            egui::Color32::from_black_alpha(80),
        );
        painter.rect_filled(tip, 0.0, egui::Color32::from_rgb(30, 32, 40));
        painter.rect_stroke(
            tip,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(18)),
            egui::StrokeKind::Inside,
        );
        painter.galley(tip.min + pad, galley, palette::TEXT);
    }

    // ── Playhead ────────────────────────────────────────────────────────
    if has_scenes {
        painter.line_segment(
            [
                egui::pos2(playhead_x, lane_rect.min.y),
                egui::pos2(playhead_x, bar_y),
            ],
            egui::Stroke::new(1.5, egui::Color32::from_white_alpha(150)),
        );
    }
    let knob_side = if active { 12.0 } else { 10.0 };
    let knob =
        egui::Rect::from_center_size(egui::pos2(playhead_x, bar_y), egui::Vec2::splat(knob_side));
    painter.rect_filled(
        knob.translate(egui::vec2(0.0, 1.0)).expand(1.0),
        0.0,
        egui::Color32::from_black_alpha(90),
    );
    painter.rect_filled(knob, 0.0, egui::Color32::WHITE);
    if is_dragging {
        painter.rect_stroke(
            knob.expand(3.0),
            0.0,
            egui::Stroke::new(2.0, palette::ACCENT.gamma_multiply(0.5)),
            egui::StrokeKind::Outside,
        );
    }

    if has_scenes
        && let Some(pos) = pointer
        && lane_rect.contains(pos)
        && hovered_scene_idx.is_some()
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // ── Interaction: chapter → scene start, loop handle → range, else seek
    let mut seek_to = None;
    let mut loop_drag = None;
    let mut marker_jump = None;
    let frac_at = |x: f32| ((x - rect.min.x) / rect.width()).clamp(0.0, 1.0);

    if response.clicked() {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let on_loop_handle = loop_x
                .is_some_and(|(lx0, lx1)| (pos.x - lx0).abs() < 10.0 || (pos.x - lx1).abs() < 10.0);
            let clicked_marker = in_track_zone(pos)
                .then(|| marker_at(markers, pos.x, x_at))
                .flatten();
            if let Some(index) = clicked_marker {
                marker_jump = Some(markers[index].time);
            } else if !on_loop_handle {
                let chapter = (has_scenes && lane_rect.contains(pos))
                    .then(|| scene_at(frac_at(pos.x)))
                    .flatten();
                seek_to = Some(chapter.map_or(frac_at(pos.x), |i| scenes[i].start_frac));
            }
        }
    } else if is_dragging {
        let pos = response
            .interact_pointer_pos()
            .or_else(|| ui.input(|i| i.pointer.interact_pos()));
        if response.drag_started() {
            let target = match (loop_x, pos) {
                (Some((lx0, lx1)), Some(pos)) => {
                    let origin = pos - response.total_drag_delta().unwrap_or(egui::Vec2::ZERO);
                    seek_bar_drag_target(origin.x, lx0, lx1)
                }
                _ => SeekBarDragTarget::Seek,
            };
            *drag_target = Some(target);
        }
        if let Some(pos) = pos {
            let new_frac = frac_at(pos.x);
            // The target is fixed when the drag starts; crossing a loop edge
            // while scrubbing must not turn the drag into a handle drag.
            match (loop_frac, loop_x) {
                (Some((ls, le)), Some((lx0, lx1))) => {
                    let target = (*drag_target).unwrap_or_else(|| {
                        let origin = pos - response.total_drag_delta().unwrap_or(egui::Vec2::ZERO);
                        seek_bar_drag_target(origin.x, lx0, lx1)
                    });
                    match target {
                        SeekBarDragTarget::LoopStart => {
                            loop_drag = Some((new_frac.clamp(0.0, le - 0.005), le));
                        }
                        SeekBarDragTarget::LoopEnd => {
                            loop_drag = Some((ls, new_frac.clamp(ls + 0.005, 1.0)));
                        }
                        SeekBarDragTarget::Seek => seek_to = Some(new_frac),
                    }
                }
                _ => seek_to = Some(new_frac),
            }
        }
    }

    if response.drag_stopped() || ui.input(|i| !i.pointer.primary_down()) {
        *drag_target = None;
    }

    SeekBarResponse {
        seek_to,
        hover_time,
        loop_drag,
        marker_jump,
    }
}

/// Truncate text to `max_chars` characters, appending "…" if truncated.
/// Name shown for a scene: `gaanim.sections` prefixes its segments with
/// `"{key} · {visit} · {step} · "`, which is dropped for display.
fn scene_display_name(name: &str) -> &str {
    let mut parts = name.splitn(4, " · ");
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(_), Some(visit), Some(step), Some(rest))
            if visit.parse::<u32>().is_ok() && step.parse::<u32>().is_ok() =>
        {
            rest
        }
        _ => name,
    }
}

/// Consecutive scenes whose display names share a leading `"Name · "`.
struct SceneGroup<'a> {
    label: &'a str,
    members: std::ops::Range<usize>,
}

fn scene_groups(scenes: &[SceneSegment]) -> Vec<SceneGroup<'_>> {
    fn lead(scene: &SceneSegment) -> Option<&str> {
        scene_display_name(&scene.name)
            .split_once(" · ")
            .map(|(lead, _)| lead)
    }
    let mut groups: Vec<SceneGroup<'_>> = Vec::new();
    for (index, scene) in scenes.iter().enumerate() {
        let Some(label) = lead(scene) else {
            continue;
        };
        match groups.last_mut() {
            Some(group) if group.label == label && group.members.end == index => {
                group.members.end = index + 1;
            }
            _ => groups.push(SceneGroup {
                label,
                members: index..index + 1,
            }),
        }
    }
    groups
}

fn truncate_with_ellipsis(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    format!("{}…", chars[..max_chars].iter().collect::<String>())
}

/// A control-row interaction, applied after the row is laid out so the left
/// and right sides can be built without borrowing editor state twice.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PlaybackAction {
    TogglePlay,
    /// Seek while keeping the current play state.
    Seek(f64),
    /// Pause and seek (start / end of the timeline).
    Jump(f64),
    SetRate(f64),
    ToggleContinuous,
    ToggleLoop((f64, f64)),
    OpenExport,
    ToggleNarration,
    Present,
    ToggleFullscreen,
    TogglePin,
}

const SPEED_PRESETS: [f64; 6] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0];

fn format_rate(rate: f64) -> String {
    if (rate - rate.round()).abs() < 1e-9 {
        format!("{rate:.0}×")
    } else {
        let text = format!("{rate:.2}");
        format!("{}×", text.trim_end_matches('0'))
    }
}

/// Speed chip with a presets popup. Returns a newly chosen rate.
fn speed_control(ui: &mut egui::Ui, rate: f64) -> Option<f64> {
    let is_default = (rate - 1.0).abs() < f64::EPSILON;
    let chip = egui::Button::new(
        egui::RichText::new(format_rate(rate))
            .size(12.5)
            .monospace()
            .color(if is_default {
                palette::TEXT_MUTED
            } else {
                palette::STOP
            }),
    )
    .min_size(egui::vec2(44.0, 26.0))
    .corner_radius(0.0)
    .stroke(egui::Stroke::NONE)
    .fill(egui::Color32::from_white_alpha(10));
    let response = ui.add(chip).on_hover_text("Velocidad · Alt + rueda");
    let mut chosen = None;
    egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(220.0);
            ui.label(
                egui::RichText::new("Velocidad")
                    .size(11.0)
                    .color(palette::TEXT_MUTED),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for preset in SPEED_PRESETS {
                    let selected = (rate - preset).abs() < f64::EPSILON;
                    let button = egui::Button::new(
                        egui::RichText::new(format_rate(preset))
                            .size(11.5)
                            .color(if selected {
                                palette::TEXT
                            } else {
                                palette::TEXT_MUTED
                            }),
                    )
                    .min_size(egui::vec2(32.0, 24.0))
                    .corner_radius(0.0)
                    .stroke(egui::Stroke::NONE)
                    .fill(if selected {
                        palette::ACCENT.gamma_multiply(0.35)
                    } else {
                        egui::Color32::from_white_alpha(8)
                    });
                    if ui.add(button).clicked() {
                        chosen = Some(preset);
                    }
                }
            });
            ui.add_space(6.0);
            let mut fine = rate as f32;
            if ui
                .add(
                    egui::Slider::new(&mut fine, 0.1..=5.0)
                        .step_by(0.05)
                        .suffix("×"),
                )
                .changed()
            {
                chosen = Some(fine as f64);
            }
        });
    chosen
}

fn start_presentation(
    presentation_mode: &mut PresentationMode,
    fullscreen_state: &mut EditorFullscreenState,
    windows: &mut Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    commands: &mut Commands,
    presenter_windows: &Query<(), With<presenter::PresenterWindow>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    fullscreen_state.previous_mode = None;
    window.mode =
        bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current);
    presentation_mode.active = true;
    if presenter_windows.is_empty() {
        presenter::spawn_presenter_window(commands);
    }
}

fn toggle_pinned_on_top(
    state: &mut EditorState,
    windows: &mut Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    state.pinned_on_top = !state.pinned_on_top;
    if let Ok(mut window) = windows.single_mut() {
        window.window_level = if state.pinned_on_top {
            bevy::window::WindowLevel::AlwaysOnTop
        } else {
            bevy::window::WindowLevel::Normal
        };
    }
}

fn toggle_editor_fullscreen(window: &mut Window, state: &mut EditorFullscreenState) {
    if matches!(window.mode, bevy::window::WindowMode::Windowed) {
        state.previous_mode = Some(window.mode);
        window.mode =
            bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current);
    } else {
        window.mode = state
            .previous_mode
            .take()
            .unwrap_or(bevy::window::WindowMode::Windowed);
    }
}

fn editor_fullscreen_keys_system(
    egui_wants: Res<EguiWantsInput>,
    keys: Res<ButtonInput<KeyCode>>,
    presentation_mode: Res<PresentationMode>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    mut state: ResMut<EditorFullscreenState>,
) {
    if !editor_shortcuts_allowed(presentation_mode.active, egui_wants.wants_keyboard_input()) {
        return;
    }
    if let Ok(mut window) = windows.single_mut() {
        let escape_from_fullscreen = keys.just_pressed(KeyCode::Escape)
            && !matches!(window.mode, bevy::window::WindowMode::Windowed);
        if keys.just_pressed(KeyCode::F11) || escape_from_fullscreen {
            toggle_editor_fullscreen(&mut window, &mut state);
        }
    }
}

fn editor_shortcuts_allowed(presentation_active: bool, wants_keyboard_input: bool) -> bool {
    !presentation_active && !wants_keyboard_input
}

/// Global playback keybindings that work regardless of timeline panel visibility.
fn global_playback_keys_system(
    egui_wants: Res<EguiWantsInput>,
    keys: Res<ButtonInput<KeyCode>>,
    presentation_mode: Res<PresentationMode>,
    narration: Option<Res<narration::NarrationSession>>,
    mut state: ResMut<EditorState>,
    mut timeline: ResMut<Timeline>,
) {
    if !editor_shortcuts_allowed(presentation_mode.active, egui_wants.wants_keyboard_input())
        || narration.is_some_and(|session| session.capturing())
    {
        return;
    }

    if keys.just_pressed(KeyCode::KeyL)
        && let Some(range) = current_scene_loop_range(&timeline)
    {
        toggle_scene_loop_range(&mut state.segment_loop, &mut timeline, range);
    }

    // Explicit stop navigation is owned by `interactive_stop_input_system`.
    if timeline
        .segments
        .iter()
        .any(|segment| !segment.stops.is_empty())
    {
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

/// Keep the unused area around a fullscreen fitted canvas neutral even when
/// the authored scene uses a colored, gradient, or shader background.
fn sync_fullscreen_letterbox_color_system(
    presentation_mode: Res<PresentationMode>,
    primary_window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut cameras: Query<(
        &mut bevy::camera::Camera,
        Option<&gaanim_renderer::pipeline::GaanimFullWindowClearCamera>,
        Option<&gaanim_renderer::pipeline::GaanimPbrCamera>,
    )>,
) {
    let fullscreen = presentation_mode.active
        || primary_window
            .single()
            .is_ok_and(|window| !matches!(window.mode, bevy::window::WindowMode::Windowed));
    for (mut camera, full_window_clear, pbr) in &mut cameras {
        if full_window_clear.is_some() {
            camera.clear_color = if fullscreen {
                bevy::camera::ClearColorConfig::Custom(Color::BLACK)
            } else {
                bevy::camera::ClearColorConfig::Default
            };
        } else if pbr.is_some() {
            camera.clear_color = if fullscreen {
                bevy::camera::ClearColorConfig::Default
            } else {
                bevy::camera::ClearColorConfig::None
            };
        }
    }
}

/// Escape leaves audience mode and restores the editor chrome in a window.
#[allow(clippy::too_many_arguments)]
fn presentation_escape_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut presentation_mode: ResMut<PresentationMode>,
    mut blank: ResMut<AudienceBlank>,
    mut overview: ResMut<presenter::PresenterOverviewState>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    presenter_windows: Query<Entity, With<presenter::PresenterWindow>>,
    presenter_cameras: Query<Entity, With<presenter::PresenterCamera>>,
    mut commands: Commands,
) {
    if !presentation_mode.active || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if overview.open {
        overview.open = false;
        overview.query.clear();
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
    authored_camera: Option<Res<Camera>>,
    presentation_mode: Res<PresentationMode>,
) {
    if egui_wants.wants_keyboard_input() {
        return;
    }
    // Don't interfere with presentation mode's own Esc handling when blanking.
    if presentation_mode.active {
        return;
    }

    if keys.just_pressed(KeyCode::KeyI) {
        interactive.toggle(authored_camera.as_deref().copied());
    } else if interactive.enabled && keys.just_pressed(KeyCode::Escape) {
        interactive.set_enabled(false, None);
    } else if interactive.enabled && keys.just_pressed(KeyCode::KeyR) {
        interactive.reset();
    } else if interactive.enabled && keys.just_pressed(KeyCode::KeyF) {
        interactive.needs_frame = true;
    } else if interactive.enabled
        && interactive.free_camera.is_some()
        && keys.just_pressed(KeyCode::Numpad0)
    {
        interactive.view = match interactive.view {
            PreviewView::CameraView => PreviewView::Free3D,
            PreviewView::Free3D => PreviewView::CameraView,
        };
    }
}

/// Pan (drag) and zoom (wheel) when interactive mode is enabled.
/// Wheel zooms; middle/right or left drag pans (left only in interactive mode).
/// Also updates the system cursor to Grab/Grabbing while interactive.
/// For perspective cameras: Right-drag orbits, Middle/Shift+Left pan, Wheel dolly.
fn preview_interactive_input_system(
    mut interactive: ResMut<PreviewInteractive>,
    authored_camera: Option<Res<Camera>>,
    viewport: Res<CameraViewport>,
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
        commands
            .entity(win_entity)
            .remove::<bevy::window::CursorIcon>();
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

    let mut cam = match interactive.view {
        PreviewView::Free3D => interactive.free_camera,
        PreviewView::CameraView => authored_camera.as_deref().copied(),
    };
    let Some(ref mut cam) = cam else {
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
                let _ = cam.dolly(factor);
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
                cam.pan_screen_delta_with_viewport(kdelta, *viewport);
            }
        } else {
            let proj_zoom = match cam.projection {
                gaanim_math::Projection::Orthographic { zoom } => zoom,
                _ => 1.0,
            };
            let effective = (viewport.scale * proj_zoom).max(0.1);
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
        if interactive.view == PreviewView::Free3D {
            interactive.free_camera = Some(*cam);
        }
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
            let _ = cam.orbit_around_target(delta.x * 0.005, -delta.y * 0.005);
        } else if is_panning_3d {
            cam.pan_screen_delta_with_viewport(delta, *viewport);
        }
    } else {
        let proj_zoom = match cam.projection {
            gaanim_math::Projection::Orthographic { zoom } => zoom,
            _ => 1.0,
        };
        let effective = (viewport.scale * proj_zoom).max(0.1);
        interactive.pan.x -= delta.x / effective;
        interactive.pan.y += delta.y / effective;
    }
    if interactive.view == PreviewView::Free3D {
        interactive.free_camera = Some(*cam);
    }
}

#[allow(clippy::too_many_arguments)]
fn editor_picking_system(
    egui_wants: Res<EguiWantsInput>,
    camera: Option<Res<ResolvedCamera>>,
    viewport_frame: Res<ViewportFrame>,
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

    let is_perspective = matches!(
        camera.projection,
        gaanim_math::Projection::Perspective { .. }
    );
    if !is_perspective && interactive.enabled {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Some(viewport_pos) =
        viewport_frame.window_to_output(glam::DVec2::new(cursor_pos.x as f64, cursor_pos.y as f64))
    else {
        state.selected = None;
        return;
    };
    let viewport = camera.viewport;
    let camera = &camera.camera;

    let mut best_z = i32::MIN;
    let mut best_entity: Option<Entity> = None;
    let mut best_t: f64 = f64::INFINITY;

    if is_perspective {
        let (origin, dir) = camera.screen_to_ray(viewport_pos);
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
        let mut picking_camera = *camera;
        let fit_scale = (viewport_frame.size.x / viewport_frame.output_size.x.max(1.0)).max(1e-6);
        if let gaanim_math::Projection::Orthographic { ref mut zoom } = picking_camera.projection {
            *zoom *= viewport.scale / fit_scale;
        }
        let world_pos = picking_camera.screen_to_world(viewport_pos);
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

fn ray_aabb_intersect(
    origin: glam::DVec3,
    dir: glam::DVec3,
    bounds: gaanim_math::Bounds3D,
) -> Option<f64> {
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

#[allow(clippy::type_complexity)]
fn detect_3d_content_system(
    mut interactive: ResMut<PreviewInteractive>,
    primitives: Query<(), Or<(With<Mesh3DMarker>, With<GltfModelRoot>)>>,
) {
    if interactive.detected_3d || primitives.is_empty() {
        return;
    }
    interactive.detected_3d = true;
}

fn frame_free_camera_system(
    authored: Option<Res<Camera>>,
    state: Res<EditorState>,
    mut interactive: ResMut<PreviewInteractive>,
    bounds: Query<(Entity, &WorldBounds)>,
) {
    if !interactive.needs_frame {
        return;
    }
    let selected = state.selected;
    let mut combined: Option<gaanim_math::Bounds3D> = None;
    for (entity, bounds) in &bounds {
        if selected.is_some() && selected != Some(entity) {
            continue;
        }
        combined = Some(match combined {
            Some(current) => current.union(&bounds.0),
            None => bounds.0,
        });
    }
    if combined.is_none() && selected.is_some() {
        for (_, bounds) in &bounds {
            combined = Some(match combined {
                Some(current) => current.union(&bounds.0),
                None => bounds.0,
            });
        }
    }
    let Some(bounds) = combined else { return };
    let authored = authored
        .as_deref()
        .copied()
        .unwrap_or_else(|| Camera::ortho_2d(1280, 720));
    let fov = std::f64::consts::FRAC_PI_4;
    let mut camera = Camera::perspective_3d(authored.viewport_width, authored.viewport_height, fov);
    let center = bounds.center();
    let radius = (bounds.size().length() * 0.5).max(0.5);
    let distance = (radius / (fov * 0.5).tan() * 1.35).max(2.0);
    let direction = glam::DVec3::new(1.0, 0.75, 1.25).normalize();
    let _ = camera.look_at(center + direction * distance, center, glam::DVec3::Y);
    interactive.free_camera = Some(camera);
    interactive.needs_frame = false;
}

/// System: composes an editor presentation camera so the animation fits in the
/// area above UI panels (the timeline at the bottom).
///
/// When [`PreviewInteractive`] is enabled, its `user_zoom` and `pan` are
/// composed on top of the fit scale so the user can inspect the scene
/// without losing the aspect-ratio fit on window resize.
#[allow(clippy::too_many_arguments)]
fn viewport_adjust_system(
    inset: Res<ViewportInset>,
    interactive: Res<PreviewInteractive>,
    authored: Option<Res<Camera>>,
    rig: Option<Res<gaanim_math::CameraRigCamera>>,
    mut view_override: ResMut<CameraViewOverride>,
    mut camera_viewport: ResMut<CameraViewport>,
    mut viewport_frame: ResMut<ViewportFrame>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    presentation: Res<PresentationMode>,
) {
    let Some(base_camera) = rig
        .as_deref()
        .map(|rig| rig.0)
        .or_else(|| authored.as_deref().copied())
    else {
        return;
    };
    let mut cam = if interactive.enabled && interactive.view == PreviewView::Free3D {
        interactive.free_camera.unwrap_or(base_camera)
    } else {
        base_camera
    };
    let Ok(window) = windows.single() else { return };

    let window_h = window.height() as f64;
    let window_w = window.width() as f64;
    let anim_w = cam.viewport_width as f64;
    let anim_h = cam.viewport_height as f64;
    if anim_w < 1.0 || anim_h < 1.0 || window_w < 1.0 || window_h < 1.0 {
        return;
    }

    if presentation.active {
        let fit_scale = (window_w / anim_w).min(window_h / anim_h);
        let frame_size = glam::DVec2::new(anim_w * fit_scale, anim_h * fit_scale);
        viewport_frame.origin = glam::DVec2::new(
            (window_w - frame_size.x) * 0.5,
            (window_h - frame_size.y) * 0.5,
        );
        viewport_frame.size = frame_size;
        viewport_frame.output_size = glam::DVec2::new(anim_w, anim_h);
        view_override.0 = None;
        camera_viewport.scale = fit_scale;
        camera_viewport.offset_y = 0.0;
        return;
    }

    let available_h = window_h - inset.bottom as f64;

    let is_perspective = matches!(cam.projection, gaanim_math::Projection::Perspective { .. });
    if available_h < 1.0 {
        view_override.0 = interactive.enabled.then_some(cam);
        return;
    }

    // Always fit animation into the available area while preserving aspect ratio.
    let scale_x = window_w / anim_w;
    let scale_y = available_h / anim_h;
    let fit_scale = scale_x.min(scale_y);
    let frame_size = glam::DVec2::new(anim_w * fit_scale, anim_h * fit_scale);
    viewport_frame.origin = glam::DVec2::new(
        (window_w - frame_size.x) * 0.5,
        (available_h - frame_size.y) * 0.5,
    );
    viewport_frame.size = frame_size;
    viewport_frame.output_size = glam::DVec2::new(anim_w, anim_h);

    if is_perspective {
        camera_viewport.scale = fit_scale;
    } else if interactive.enabled {
        camera_viewport.scale = fit_scale * interactive.user_zoom.clamp(0.1, 20.0);
        cam.position.x += interactive.pan.x;
        cam.position.y += interactive.pan.y;
    } else {
        camera_viewport.scale = fit_scale;
        // Leave cam.position as set by the timeline (CameraZoom/Position lenses).
    }

    // Shift the Vello centre upward so the animation sits above the timeline.
    // When there is no timeline panel (inset.bottom == 0) the offset is 0.
    camera_viewport.offset_y = -(inset.bottom as f64) / 2.0;
    view_override.0 = interactive.enabled.then_some(cam);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_timeline::timeline::SegmentMetadata;

    #[test]
    fn fullscreen_letterbox_occludes_overflow_on_all_four_edges() {
        let ctx = egui::Context::default();
        let frame = ViewportFrame {
            origin: glam::DVec2::new(20.0, 60.0),
            size: glam::DVec2::new(160.0, 80.0),
            output_size: glam::DVec2::new(160.0, 80.0),
        };
        for opaque in [false, true] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(200.0, 200.0),
                    )),
                    ..Default::default()
                },
                |ui| paint_viewport_letterbox(ui.ctx(), &frame, opaque),
            );
            output.textures_delta.clear();
            let masks: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::Rect(rect) = &shape.shape {
                        (rect.fill != egui::Color32::TRANSPARENT).then_some(rect)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(masks.len(), 4);
            for point in [
                egui::pos2(100.0, 59.0),
                egui::pos2(100.0, 141.0),
                egui::pos2(19.0, 100.0),
                egui::pos2(181.0, 100.0),
            ] {
                let mask = masks.iter().find(|mask| mask.rect.contains(point)).unwrap();
                assert_eq!(mask.fill.a(), if opaque { 255 } else { 145 });
            }
            assert!(
                masks
                    .iter()
                    .all(|mask| !mask.rect.contains(egui::pos2(100.0, 100.0)))
            );
        }
    }

    #[test]
    fn scene_loop_range_resolves_first_and_last_semantic_scenes() {
        let mut timeline = Timeline::new();
        timeline.set_segments(vec![
            SegmentMetadata {
                id: 1,
                name: "first".into(),
                notes: None,
                start_time: 0.0,
                end_time: 1.25,
                stops: Vec::new(),
            },
            SegmentMetadata {
                id: 2,
                name: "last".into(),
                notes: None,
                start_time: 1.25,
                end_time: 3.0,
                stops: Vec::new(),
            },
        ]);

        timeline.current_time = 0.25;
        assert_eq!(current_scene_loop_range(&timeline), Some((0.0, 1.25)));
        timeline.current_time = 2.5;
        assert_eq!(current_scene_loop_range(&timeline), Some((1.25, 3.0)));
    }

    #[test]
    fn scene_loop_replaces_a_previous_range_and_repeated_toggle_disables_it() {
        let mut timeline = Timeline::new();
        timeline.cached_duration = 8.0;
        timeline.loop_range = Some((0.0, 8.0));
        timeline.playback_rate = 1.75;
        let mut state = SegmentLoopState::default();

        toggle_scene_loop_range(&mut state, &mut timeline, (2.0, 4.5));
        assert_eq!(timeline.loop_range, Some((2.0, 4.5)));
        assert_eq!(timeline.seek_request, Some(2.0));
        assert!(timeline.is_playing);
        assert_eq!(timeline.playback_rate, 1.75);
        assert!(state.is_active());

        timeline.seek_request = None;
        toggle_scene_loop_range(&mut state, &mut timeline, (2.0, 4.5));
        assert_eq!(timeline.loop_range, Some((0.0, 8.0)));
        assert_eq!(timeline.seek_request, None);
        assert_eq!(timeline.playback_rate, 1.75);
        assert!(!state.is_active());
    }

    #[test]
    fn zero_duration_scene_never_creates_an_invalid_loop() {
        let mut timeline = Timeline::new();
        timeline.loop_range = Some((0.0, 2.0));
        timeline.is_playing = true;
        let mut state = SegmentLoopState::default();

        toggle_scene_loop_range(&mut state, &mut timeline, (1.0, 1.0));

        assert_eq!(timeline.loop_range, Some((0.0, 2.0)));
        assert_eq!(timeline.seek_request, None);
        assert!(timeline.is_playing);
        assert!(!state.is_active());
    }

    #[test]
    fn scene_loop_falls_back_to_authored_scene_bounds() {
        use gaanim_timeline::clip::ClipPayload;

        let mut timeline = Timeline::new();
        let scene = timeline.add_scene("legacy");
        timeline.index_scene(scene, 2.0);
        let track = timeline.add_track("scene", 0);
        timeline.add_clip(track, 2.0, 0.0, ClipPayload::SceneStart(scene));
        timeline.add_clip(track, 5.0, 0.0, ClipPayload::SceneEnd(scene));

        assert_eq!(scene_loop_range_at(&timeline, 3.0), Some((2.0, 5.0)));
    }

    #[test]
    fn hot_reload_re_resolves_or_disables_the_active_segment_loop() {
        let mut timeline = Timeline::new();
        timeline.cached_duration = 4.0;
        timeline.loop_range = Some((0.0, 4.0));
        let mut state = EditorState::default();
        toggle_scene_loop_range(&mut state.segment_loop, &mut timeline, (1.0, 2.0));

        timeline.cached_duration = 5.0;
        timeline.set_segments(vec![SegmentMetadata {
            id: 1,
            name: "changed".into(),
            notes: None,
            start_time: 0.75,
            end_time: 2.5,
            stops: Vec::new(),
        }]);
        state.reconcile_segment_loop_after_reload(&mut timeline, 1.5);
        assert_eq!(timeline.loop_range, Some((0.75, 2.5)));

        timeline.set_segments(Vec::new());
        state.reconcile_segment_loop_after_reload(&mut timeline, 1.5);
        assert!(!state.segment_loop.is_active());
        assert_eq!(timeline.loop_range, Some((0.0, 5.0)));
    }

    #[test]
    fn active_segment_loop_ignores_stops_without_enabling_continuous_preview() {
        let mut app = App::new();
        let mut editor_state = EditorState::default();
        editor_state.segment_loop.segment_bounds = Some((1.0, 2.0));
        app.init_resource::<bevy_egui::input::EguiWantsInput>()
            .init_resource::<Timeline>()
            .init_resource::<PlaybackStopPolicy>()
            .insert_resource(editor_state)
            .insert_resource(PresentationMode::default())
            .add_systems(Update, sync_editor_input_ignore_system);

        app.update();

        assert_eq!(
            *app.world().resource::<PlaybackStopPolicy>(),
            PlaybackStopPolicy::Ignore
        );
        assert!(!app.world().resource::<EditorState>().continuous_preview);
    }

    #[test]
    fn l_toggles_the_current_segment_even_when_it_contains_stops() {
        let mut timeline = Timeline::new();
        timeline.cached_duration = 3.0;
        timeline.current_time = 1.5;
        timeline.set_segments(vec![SegmentMetadata {
            id: 1,
            name: "loop me".into(),
            notes: None,
            start_time: 1.0,
            end_time: 2.0,
            stops: vec![gaanim_timeline::timeline::SegmentStop {
                name: Some("pause".into()),
                time: 1.5,
            }],
        }]);
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy_egui::input::EguiWantsInput>()
            .init_resource::<EditorState>()
            .insert_resource(PresentationMode::default())
            .insert_resource(timeline)
            .add_systems(Update, global_playback_keys_system);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyL);

        app.update();

        assert_eq!(
            app.world().resource::<Timeline>().loop_range,
            Some((1.0, 2.0))
        );
        assert!(
            app.world()
                .resource::<EditorState>()
                .segment_loop
                .is_active()
        );
    }

    #[test]
    fn playback_shortcuts_do_not_change_segment_loop_in_presentation_mode() {
        let mut timeline = Timeline::new();
        timeline.current_time = 0.5;
        timeline.set_segments(vec![SegmentMetadata {
            id: 1,
            name: "present".into(),
            notes: None,
            start_time: 0.0,
            end_time: 1.0,
            stops: Vec::new(),
        }]);
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy_egui::input::EguiWantsInput>()
            .init_resource::<EditorState>()
            .insert_resource(PresentationMode { active: true })
            .insert_resource(timeline)
            .add_systems(Update, global_playback_keys_system);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyL);

        app.update();

        assert!(
            !app.world()
                .resource::<EditorState>()
                .segment_loop
                .is_active()
        );
    }

    #[test]
    fn editor_shortcuts_are_blocked_while_egui_captures_the_keyboard() {
        assert!(editor_shortcuts_allowed(false, false));
        assert!(!editor_shortcuts_allowed(false, true));
        assert!(!editor_shortcuts_allowed(true, false));
    }

    #[test]
    fn playback_density_and_width_follow_the_responsive_breakpoints() {
        assert_eq!(PlaybackDensity::for_width(1280.0), PlaybackDensity::Wide);
        assert_eq!(PlaybackDensity::for_width(1080.0), PlaybackDensity::Wide);
        assert_eq!(PlaybackDensity::for_width(1079.0), PlaybackDensity::Compact);
        assert_eq!(PlaybackDensity::for_width(900.0), PlaybackDensity::Compact);
        assert_eq!(PlaybackDensity::for_width(560.0), PlaybackDensity::Compact);
        assert_eq!(PlaybackDensity::for_width(559.0), PlaybackDensity::Minimal);

        for width in [1280.0, 800.0, 480.0, 320.0] {
            let density = PlaybackDensity::for_width(width);
            assert!(density.overlay_width(width) <= width);
            assert!(density.overlay_width(width) >= 0.0);
        }
    }

    #[test]
    fn fullscreen_toggle_restores_windowed_mode() {
        let mut window = Window::default();
        let mut state = EditorFullscreenState::default();

        toggle_editor_fullscreen(&mut window, &mut state);
        assert!(matches!(
            window.mode,
            bevy::window::WindowMode::BorderlessFullscreen(_)
        ));

        toggle_editor_fullscreen(&mut window, &mut state);
        assert!(matches!(window.mode, bevy::window::WindowMode::Windowed));
        assert!(state.previous_mode.is_none());
    }

    #[test]
    fn escape_restores_windowed_mode_from_editor_fullscreen() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy_egui::input::EguiWantsInput>()
            .insert_resource(EditorFullscreenState {
                previous_mode: Some(bevy::window::WindowMode::Windowed),
            })
            .insert_resource(PresentationMode::default())
            .add_systems(Update, editor_fullscreen_keys_system);
        app.world_mut().spawn((
            Window {
                mode: bevy::window::WindowMode::BorderlessFullscreen(
                    bevy::window::MonitorSelection::Current,
                ),
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);

        app.update();

        let window = app
            .world_mut()
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .single(app.world())
            .expect("primary window");
        assert!(matches!(window.mode, bevy::window::WindowMode::Windowed));
    }

    #[test]
    fn f11_does_not_replace_presentation_mode() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy_egui::input::EguiWantsInput>()
            .init_resource::<EditorFullscreenState>()
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, editor_fullscreen_keys_system);
        app.world_mut()
            .spawn((Window::default(), bevy::window::PrimaryWindow));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F11);

        app.update();

        let window = app
            .world_mut()
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .single(app.world())
            .expect("primary window");
        assert!(matches!(window.mode, bevy::window::WindowMode::Windowed));
        assert!(app.world().resource::<PresentationMode>().active);
    }

    #[test]
    fn escape_exits_presentation_fullscreen() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<AudienceBlank>()
            .init_resource::<presenter::PresenterOverviewState>()
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, presentation_escape_system);
        app.world_mut().spawn((
            Window {
                mode: bevy::window::WindowMode::BorderlessFullscreen(
                    bevy::window::MonitorSelection::Current,
                ),
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);

        app.update();

        assert!(!app.world().resource::<PresentationMode>().active);
        let window = app
            .world_mut()
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .single(app.world())
            .expect("primary window");
        assert!(matches!(window.mode, bevy::window::WindowMode::Windowed));
    }

    #[test]
    fn section_segments_display_their_step_name() {
        assert_eq!(
            scene_display_name("fundamentos · 1 · 3 · Fundamentos · resistencia"),
            "Fundamentos · resistencia"
        );
        assert_eq!(scene_display_name("Portada"), "Portada");
        // Only the numeric visit/step prefix of gaanim.sections is dropped.
        assert_eq!(scene_display_name("A · b · c · d"), "A · b · c · d");
    }

    #[test]
    fn dense_chapters_are_grouped_by_their_leading_name() {
        let scene = |name: &str| SceneSegment {
            name: name.into(),
            start_frac: 0.0,
            end_frac: 0.0,
        };
        let scenes = [
            scene("Portada"),
            scene("problematica · 1 · 1 · Problemática · contexto"),
            scene("problematica · 1 · 2 · Problemática · datos"),
            scene("fundamentos · 1 · 1 · Fundamentos · densidad"),
            scene("fundamentos · 1 · 2 · Fundamentos · resistencia"),
            scene("fundamentos · 1 · 3 · Fundamentos · deriva"),
            scene("Cierre"),
        ];
        let groups: Vec<_> = scene_groups(&scenes)
            .into_iter()
            .map(|group| (group.label, group.members))
            .collect();
        assert_eq!(groups, vec![("Problemática", 1..3), ("Fundamentos", 3..6)]);
    }

    #[test]
    fn seek_bar_hits_the_closest_scene_marker() {
        let markers = [0.25_f32, 0.3].map(|frac| SeekMarker {
            frac,
            time: frac as f64 * 10.0,
            name: format!("m{frac}"),
        });
        let x_at = |frac: f32| frac * 100.0;
        assert_eq!(marker_at(&markers, 26.0, x_at), Some(0));
        assert_eq!(marker_at(&markers, 28.5, x_at), Some(1));
        assert_eq!(marker_at(&markers, 50.0, x_at), None);
    }

    #[test]
    fn compact_seek_snapping_is_bypassed_for_3d_content() {
        let scenes = [SceneSegment {
            name: "scene".into(),
            start_frac: 0.25,
            end_frac: 0.75,
        }];
        let near_boundary = 0.251;

        assert_eq!(snap_seek_fraction(near_boundary, &scenes, &[], true), 0.25);
        assert_eq!(
            snap_seek_fraction(near_boundary, &scenes, &[], false),
            near_boundary
        );
    }

    #[test]
    fn viewport_frame_rejects_letterbox_and_maps_output_pixels() {
        let frame = ViewportFrame {
            origin: glam::DVec2::new(100.0, 50.0),
            size: glam::DVec2::new(640.0, 360.0),
            output_size: glam::DVec2::new(1280.0, 720.0),
        };
        assert_eq!(
            frame.window_to_output(glam::DVec2::new(100.0, 50.0)),
            Some(glam::DVec2::ZERO)
        );
        assert_eq!(
            frame.window_to_output(glam::DVec2::new(740.0, 410.0)),
            Some(glam::DVec2::new(1280.0, 720.0))
        );
        assert_eq!(frame.window_to_output(glam::DVec2::new(99.0, 200.0)), None);
    }

    #[test]
    fn viewport_fit_does_not_mask_the_reactive_camera_rig() {
        let authored = Camera::ortho_2d(1280, 720);
        let mut rig = authored;
        rig.position.x = 240.0;

        let mut app = App::new();
        app.insert_resource(ViewportInset::default())
            .insert_resource(PreviewInteractive::default())
            .insert_resource(authored)
            .insert_resource(gaanim_math::CameraRigCamera(rig))
            .insert_resource(CameraViewOverride::default())
            .insert_resource(CameraViewport::default())
            .insert_resource(ViewportFrame::default())
            .insert_resource(PresentationMode::default())
            .insert_resource(export::ExportState::default())
            .add_systems(Update, viewport_adjust_system);
        app.world_mut().spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(1280, 720),
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));

        app.update();

        assert_eq!(
            app.world().resource::<CameraViewOverride>().0,
            None,
            "viewport fitting must not override follow, shake, or camera bindings"
        );
    }

    #[test]
    fn active_export_preserves_the_editor_viewport_fit() {
        let mut export_state = export::ExportState::default();
        export_state.active = true;
        let mut app = App::new();
        app.insert_resource(ViewportInset { bottom: 120.0 })
            .insert_resource(PreviewInteractive::default())
            .insert_resource(Camera::ortho_2d(1280, 720))
            .insert_resource(CameraViewOverride::default())
            .insert_resource(CameraViewport::default())
            .insert_resource(ViewportFrame::default())
            .insert_resource(PresentationMode::default())
            .insert_resource(export_state)
            .add_systems(Update, viewport_adjust_system);
        app.world_mut().spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(1280, 720),
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));

        app.update();

        assert_eq!(app.world().resource::<CameraViewport>().scale, 5.0 / 6.0);
        assert_eq!(app.world().resource::<ViewportFrame>().size.y, 600.0);
    }

    #[test]
    fn presentation_viewport_fits_the_scene_to_the_fullscreen_window() {
        let camera = Camera::ortho_2d(1280, 720);
        let mut app = App::new();
        app.insert_resource(ViewportInset::default())
            .insert_resource(PreviewInteractive::default())
            .insert_resource(camera)
            .insert_resource(CameraViewOverride::default())
            .insert_resource(CameraViewport::default())
            .insert_resource(ViewportFrame::default())
            .insert_resource(PresentationMode { active: true })
            .insert_resource(export::ExportState::default())
            .add_systems(Update, viewport_adjust_system);
        app.world_mut().spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(2560, 1600),
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));

        app.update();

        assert_eq!(app.world().resource::<CameraViewport>().scale, 2.0);
        let frame = app.world().resource::<ViewportFrame>();
        assert_eq!(frame.origin, glam::DVec2::new(0.0, 80.0));
        assert_eq!(frame.size, glam::DVec2::new(2560.0, 1440.0));
    }

    #[test]
    fn presentation_uses_black_outside_the_fitted_scene() {
        let mut app = App::new();
        app.insert_resource(PresentationMode { active: true })
            .add_systems(Update, sync_fullscreen_letterbox_color_system);
        let clear_camera = app
            .world_mut()
            .spawn((
                bevy::camera::Camera::default(),
                gaanim_renderer::pipeline::GaanimFullWindowClearCamera,
            ))
            .id();
        let pbr_camera = app
            .world_mut()
            .spawn((
                bevy::camera::Camera::default(),
                gaanim_renderer::pipeline::GaanimPbrCamera,
            ))
            .id();

        app.update();

        let camera = app
            .world()
            .get::<bevy::camera::Camera>(clear_camera)
            .expect("full-window clear camera");
        assert!(matches!(
            camera.clear_color,
            bevy::camera::ClearColorConfig::Custom(color) if color == Color::BLACK
        ));
        let pbr = app
            .world()
            .get::<bevy::camera::Camera>(pbr_camera)
            .expect("PBR camera");
        assert!(matches!(
            pbr.clear_color,
            bevy::camera::ClearColorConfig::Default
        ));

        app.world_mut().resource_mut::<PresentationMode>().active = false;
        app.update();
        let camera = app
            .world()
            .get::<bevy::camera::Camera>(clear_camera)
            .expect("full-window clear camera");
        assert!(matches!(
            camera.clear_color,
            bevy::camera::ClearColorConfig::Default
        ));
        let pbr = app
            .world()
            .get::<bevy::camera::Camera>(pbr_camera)
            .expect("PBR camera");
        assert!(matches!(
            pbr.clear_color,
            bevy::camera::ClearColorConfig::None
        ));
    }

    #[test]
    fn editor_fullscreen_uses_black_outside_the_fitted_scene() {
        let mut app = App::new();
        app.insert_resource(PresentationMode { active: false })
            .add_systems(Update, sync_fullscreen_letterbox_color_system);
        app.world_mut().spawn((
            Window {
                mode: bevy::window::WindowMode::BorderlessFullscreen(
                    bevy::window::MonitorSelection::Current,
                ),
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));
        let clear_camera = app
            .world_mut()
            .spawn((
                bevy::camera::Camera::default(),
                gaanim_renderer::pipeline::GaanimFullWindowClearCamera,
            ))
            .id();
        let pbr_camera = app
            .world_mut()
            .spawn((
                bevy::camera::Camera::default(),
                gaanim_renderer::pipeline::GaanimPbrCamera,
            ))
            .id();

        app.update();

        let clear = app
            .world()
            .get::<bevy::camera::Camera>(clear_camera)
            .expect("full-window clear camera");
        assert!(matches!(
            clear.clear_color,
            bevy::camera::ClearColorConfig::Custom(color) if color == Color::BLACK
        ));
        let pbr = app
            .world()
            .get::<bevy::camera::Camera>(pbr_camera)
            .expect("PBR camera");
        assert!(matches!(
            pbr.clear_color,
            bevy::camera::ClearColorConfig::Default
        ));

        let mut query = app
            .world_mut()
            .query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>();
        query
            .single_mut(app.world_mut())
            .expect("primary window")
            .mode = bevy::window::WindowMode::Windowed;
        app.update();

        let clear = app
            .world()
            .get::<bevy::camera::Camera>(clear_camera)
            .expect("full-window clear camera");
        assert!(matches!(
            clear.clear_color,
            bevy::camera::ClearColorConfig::Default
        ));
        let pbr = app
            .world()
            .get::<bevy::camera::Camera>(pbr_camera)
            .expect("PBR camera");
        assert!(matches!(
            pbr.clear_color,
            bevy::camera::ClearColorConfig::None
        ));
    }

    #[test]
    fn three_d_interaction_restarts_from_authored_camera_every_time() {
        let mut app = App::new();
        let mut authored = Camera::perspective_3d(1280, 720, 0.8);
        authored.position = glam::DVec3::new(1.0, 2.0, 3.0);
        app.init_resource::<PreviewInteractive>()
            .init_resource::<EditorState>()
            .insert_resource(authored)
            .add_systems(
                Update,
                (detect_3d_content_system, frame_free_camera_system).chain(),
            );
        let entity = app
            .world_mut()
            .spawn((
                Mesh3DMarker,
                WorldBounds(gaanim_math::Bounds3D::new_3d(
                    -1.0, -2.0, -3.0, 1.0, 2.0, 3.0,
                )),
            ))
            .id();
        app.update();
        let preview = app.world().resource::<PreviewInteractive>();
        assert!(preview.detected_3d);
        assert!(!preview.enabled);
        assert_eq!(preview.view, PreviewView::CameraView);
        assert_eq!(preview.free_camera, None);

        app.world_mut()
            .resource_mut::<PreviewInteractive>()
            .set_enabled(true, Some(authored));
        let preview = app.world().resource::<PreviewInteractive>();
        assert!(preview.enabled);
        assert_eq!(preview.view, PreviewView::Free3D);
        assert_eq!(preview.free_camera, Some(authored));

        let mut latest_authored = authored;
        latest_authored.position = glam::DVec3::new(-4.0, 5.0, 6.0);
        {
            let mut preview = app.world_mut().resource_mut::<PreviewInteractive>();
            preview.free_camera.as_mut().unwrap().position = glam::DVec3::splat(99.0);
            preview.user_zoom = 3.0;
            preview.pan = glam::DVec2::splat(20.0);
            preview.set_enabled(false, None);
            preview.set_enabled(true, Some(latest_authored));
        }
        let preview = app.world().resource::<PreviewInteractive>();
        assert_eq!(preview.free_camera, Some(latest_authored));
        assert_eq!(preview.user_zoom, 1.0);
        assert_eq!(preview.pan, glam::DVec2::ZERO);

        app.world_mut().despawn(entity);

        let mut two_d = App::new();
        two_d
            .init_resource::<PreviewInteractive>()
            .add_systems(Update, detect_3d_content_system);
        two_d.update();
        assert!(!two_d.world().resource::<PreviewInteractive>().enabled);
    }

    #[test]
    fn presentation_mode_overrides_continuous_preview_policy() {
        let mut app = App::new();
        let mut editor_state = EditorState::default();
        editor_state.continuous_preview = true;
        app.init_resource::<bevy_egui::input::EguiWantsInput>()
            .init_resource::<Timeline>()
            .init_resource::<PlaybackStopPolicy>()
            .insert_resource(editor_state)
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, sync_editor_input_ignore_system);

        app.update();
        assert_eq!(
            *app.world().resource::<PlaybackStopPolicy>(),
            PlaybackStopPolicy::Respect
        );
        assert!(app.world().resource::<Timeline>().ignore_input);

        app.world_mut().resource_mut::<PresentationMode>().active = false;
        app.update();
        assert_eq!(
            *app.world().resource::<PlaybackStopPolicy>(),
            PlaybackStopPolicy::Ignore
        );
    }

    #[test]
    fn seek_drag_keeps_timeline_target_when_reaching_loop_handle() {
        assert_eq!(
            seek_bar_drag_target(50.0, 0.0, 100.0),
            SeekBarDragTarget::Seek
        );
    }
}
