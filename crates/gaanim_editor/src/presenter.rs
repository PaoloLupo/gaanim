//! Presenter view hosted in a second native window.

use bevy::{
    camera::RenderTarget,
    ecs::schedule::ScheduleLabel,
    prelude::*,
    window::{
        ClosingWindow, PrimaryWindow, WindowCloseRequested, WindowClosed, WindowCreated, WindowRef,
        WindowResolution,
    },
};
use bevy_egui::{EguiContext, EguiSchedule, egui, input::EguiWantsInput};
use crossbeam_channel::{Receiver, TryRecvError, bounded};
use gaanim_export::prelude::{AspectRatioPreset, ExportConfig, capture_scene_direct};
use gaanim_timeline::timeline::Timeline;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::{AudienceBlank, PresentationMode, export::StashedReplay};

/// Dedicated egui schedule for the presenter window.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PresenterEguiPass;

#[derive(Component)]
pub(crate) struct PresenterWindow;

#[derive(Component)]
pub(crate) struct PresenterCamera {
    window: Entity,
}

/// Ephemeral controls for navigating a presentation by segment name.
#[derive(Resource, Default)]
pub(crate) struct PresenterOverviewState {
    pub(crate) open: bool,
    pub(crate) query: String,
    focus_search: bool,
    texture_revision: u64,
    texture_generation: u64,
    uploaded_camera: Option<Entity>,
    textures: HashMap<(u32, ThumbnailMoment), egui::TextureHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ThumbnailMoment {
    Entry,
    Stop(u32),
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentationAction {
    Advance,
    Previous,
    TogglePlayback,
    Home,
    End,
    ToggleOverview,
    ToggleBlack,
    ToggleWhite,
    ReopenPresenter,
}

/// Interaction state for the audience-facing playback dock. Keeping this
/// separate from egui's aggregate input state lets clicks on the primary
/// window coexist with the independently focused Presenter View.
#[derive(Resource, Default)]
pub(crate) struct AudienceControlsState {
    pointer_over: bool,
}

#[derive(Debug)]
struct ThumbnailPixels {
    segment_id: u32,
    moment: ThumbnailMoment,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

type ThumbnailResult = Result<Vec<ThumbnailPixels>, String>;

/// Async thumbnail state. Captures use a fresh headless world, so generating
/// the overview never seeks or mutates the audience's live presentation.
#[derive(Resource, Default)]
pub(crate) struct PresenterThumbnailCache {
    requested_revision: u64,
    requested_dimensions: (u32, u32),
    request_attempts: u8,
    pixel_revision: u64,
    pixel_dimensions: (u32, u32),
    pixel_generation: u64,
    pixels: Vec<ThumbnailPixels>,
    receiver: Option<Receiver<(u64, ThumbnailResult)>>,
    error: Option<String>,
}

impl PresenterThumbnailCache {
    fn request(&mut self, stash: &StashedReplay, timeline: &Timeline, max_edge: u32) {
        // Never replace the receiver of an in-flight render. Hot reload and
        // resize can ask for a newer generation while the GPU worker is still
        // active; dropping that receiver used to orphan the worker and launch
        // overlapping GPU captures.
        if self.receiver.is_some() {
            return;
        }
        let Some(canvas) = stash.canvas.clone() else {
            return;
        };
        let (preview_width, preview_height) = canvas.frame.preview_pixel_size();
        let dimensions = thumbnail_dimensions(preview_width, preview_height, max_edge);
        if stash.revision == 0 || timeline.segments.is_empty() {
            return;
        }

        let revision = stash.revision;
        let request_changed =
            revision != self.requested_revision || dimensions != self.requested_dimensions;
        if request_changed {
            self.requested_revision = revision;
            self.requested_dimensions = dimensions;
            self.request_attempts = 0;
        }
        if (self.pixel_revision == revision && self.pixel_dimensions == dimensions)
            || self.request_attempts >= 2
        {
            return;
        }

        let requests = timeline
            .segments
            .iter()
            .flat_map(|segment| {
                let mut cues = Vec::with_capacity(segment.stops.len() + 2);
                cues.push((
                    segment.id,
                    ThumbnailMoment::Entry,
                    entry_segment_time(segment.start_time, segment.end_time),
                ));
                cues.extend(segment.stops.iter().enumerate().map(|(index, stop)| {
                    (segment.id, ThumbnailMoment::Stop(index as u32), stop.time)
                }));
                cues.push((
                    segment.id,
                    ThumbnailMoment::Complete,
                    representative_segment_time(segment.start_time, segment.end_time),
                ));
                cues
            })
            .collect::<Vec<_>>();
        let times = requests
            .iter()
            .map(|(_, _, time)| *time)
            .collect::<Vec<_>>();
        let (width, height) = dimensions;
        let (sender, receiver) = bounded(1);

        self.request_attempts += 1;
        self.receiver = Some(receiver);
        self.error = None;

        let spawn_result = std::thread::Builder::new()
            .name("gaanim-presenter-thumbnails".to_string())
            .spawn(move || {
                let mut config = ExportConfig::new("presenter-thumbnails.png");
                config.width = width;
                config.height = height;
                config.aspect_ratio = AspectRatioPreset::Custom;
                config.headless = true;
                let result = capture_scene_direct(config, &times, move |world| {
                    gaanim_api::runtime::replay_canvas_into(world, canvas)
                })
                .map(|frames| {
                    requests
                        .into_iter()
                        .zip(frames)
                        .map(|((segment_id, moment, _), frame)| ThumbnailPixels {
                            segment_id,
                            moment,
                            width: frame.width,
                            height: frame.height,
                            rgba: frame.rgba,
                        })
                        .collect()
                })
                .map_err(|error| error.to_string());
                let _ = sender.send((revision, result));
            });

        if let Err(error) = spawn_result {
            self.receiver = None;
            self.error = Some(format!("could not start thumbnail renderer: {error}"));
        }
    }

    fn receive(&mut self) -> Option<(u64, ThumbnailResult)> {
        let result = match self.receiver.as_ref()?.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.error = Some("thumbnail renderer stopped unexpectedly".to_string());
                self.receiver = None;
                None
            }
        };
        if result.is_some() {
            self.receiver = None;
        }
        result
    }

    fn is_loading(&self) -> bool {
        self.receiver.is_some()
    }

    fn store(&mut self, revision: u64, frames: Vec<ThumbnailPixels>) {
        self.pixel_revision = revision;
        self.pixel_dimensions = frames
            .first()
            .map(|frame| (frame.width, frame.height))
            .unwrap_or(self.requested_dimensions);
        self.pixel_generation = self.pixel_generation.wrapping_add(1).max(1);
        self.pixels = frames;
        self.error = None;
    }

    fn fail(&mut self, error: String) {
        self.error = Some(error);
    }

    fn retry(&mut self) {
        if self.receiver.is_none() {
            self.requested_revision = 0;
            self.requested_dimensions = (0, 0);
            self.request_attempts = 0;
            self.error = None;
        }
    }
}

fn thumbnail_dimensions(canvas_width: u32, canvas_height: u32, max_edge: u32) -> (u32, u32) {
    let max_edge = max_edge.max(1) as f64;
    let width = canvas_width.max(1) as f64;
    let height = canvas_height.max(1) as f64;
    let scale = max_edge / width.max(height);
    (
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
}

fn desired_thumbnail_edge(viewport_width: f32, pixels_per_point: f32) -> u32 {
    let physical_preview_width = viewport_width.max(1.0) * 0.66 * pixels_per_point.max(1.0);
    let quantized = (physical_preview_width / 160.0).ceil() * 160.0;
    quantized.clamp(960.0, 1600.0) as u32
}

fn thumbnail_upload_required(
    overview: &PresenterOverviewState,
    cache: &PresenterThumbnailCache,
    camera_entity: Entity,
    revision: u64,
) -> bool {
    cache.pixel_revision == revision
        && (overview.uploaded_camera != Some(camera_entity)
            || overview.texture_revision != revision
            || overview.texture_generation != cache.pixel_generation)
}

/// Wall-clock timer for one presentation session. It survives closing and
/// reopening Presenter View and resets automatically for the next session.
#[derive(Resource, Default)]
pub(crate) struct PresentationTimer {
    started_at: Option<Instant>,
    was_active: bool,
}

impl PresentationTimer {
    fn elapsed(&self) -> Duration {
        self.started_at
            .map(|started_at| started_at.elapsed())
            .unwrap_or_default()
    }

    fn reset(&mut self) {
        self.started_at = Some(Instant::now());
    }
}

pub(crate) fn sync_presentation_timer_system(
    presentation_mode: Res<PresentationMode>,
    mut timer: ResMut<PresentationTimer>,
) {
    if presentation_mode.active && !timer.was_active {
        timer.reset();
    }
    timer.was_active = presentation_mode.active;
}

fn format_stopwatch(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60
    )
}

fn format_timeline_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0).round() as u64;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn cue_label(
    segment: &gaanim_timeline::timeline::SegmentMetadata,
    stop_index: Option<usize>,
) -> String {
    stop_index
        .and_then(|index| segment.stops.get(index).map(|stop| (index, stop)))
        .map(|(index, stop)| {
            stop.name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Cue {}", index + 1))
        })
        .unwrap_or_else(|| "Start".to_string())
}

fn next_cue_label(
    current_segment_id: Option<u32>,
    next_segment: &gaanim_timeline::timeline::SegmentMetadata,
    stop_index: Option<usize>,
) -> String {
    let cue = cue_label(next_segment, stop_index);
    if current_segment_id == Some(next_segment.id) {
        cue
    } else {
        format!("{} / {}", next_segment.name, cue)
    }
}

fn audience_controls_visible(
    presentation_active: bool,
    audience_blank: AudienceBlank,
    audience_focused: bool,
    pointer_over_controls: bool,
    cursor_in_dock_zone: bool,
) -> bool {
    presentation_active
        && audience_blank == AudienceBlank::None
        && audience_focused
        && (pointer_over_controls || cursor_in_dock_zone)
}

fn cursor_in_audience_dock_zone(
    cursor_position: Option<Vec2>,
    window_width: f32,
    window_height: f32,
) -> bool {
    let Some(cursor) = cursor_position else {
        return false;
    };
    let content_width = (window_width - 48.0).clamp(280.0, 920.0);
    let dock_width = content_width + 32.0;
    let left = (window_width - dock_width) * 0.5;
    let right = left + dock_width;
    cursor.x >= left
        && cursor.x <= right
        && cursor.y >= (window_height - 100.0).max(0.0)
        && cursor.y <= window_height
}

fn representative_segment_time(start_time: f64, end_time: f64) -> f64 {
    if end_time > start_time + 1e-4 {
        (end_time - 1e-4).max(start_time)
    } else {
        start_time
    }
}

fn entry_segment_time(start_time: f64, end_time: f64) -> f64 {
    if end_time > start_time + 2e-4 {
        (start_time + 1e-4).min(end_time - 1e-4)
    } else {
        start_time
    }
}

fn apply_presenter_style(ctx: &egui::Context) {
    use egui::{Color32, FontFamily, FontId, TextStyle};

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(18);
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(7, 11, 22);
    style.visuals.window_fill = Color32::from_rgb(13, 20, 36);
    style.visuals.extreme_bg_color = Color32::from_rgb(5, 8, 16);
    style.visuals.faint_bg_color = Color32::from_rgb(17, 26, 46);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(22, 33, 57);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(39, 58, 92);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(56, 83, 132);
    style.visuals.selection.bg_fill = Color32::from_rgb(69, 105, 174);
    style.visuals.hyperlink_color = Color32::from_rgb(110, 168, 254);
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(30.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(18.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(16.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(17.0, FontFamily::Monospace),
    );
    ctx.set_global_style(style);
}

/// Create the speaker-facing window only for `gaanim --present`.
pub(crate) fn spawn_presenter_window_system(
    presentation_mode: Res<PresentationMode>,
    presenter_windows: Query<(), With<PresenterWindow>>,
    mut commands: Commands,
) {
    if !presentation_mode.active || !presenter_windows.is_empty() {
        return;
    }

    spawn_presenter_window(&mut commands);
}

/// Spawn the speaker-facing window from either startup or an editor command.
pub(crate) fn spawn_presenter_window(commands: &mut Commands) {
    commands.spawn((
        Window {
            title: "Gaanim — Presenter View".to_string(),
            resolution: WindowResolution::new(1180, 760),
            resizable: true,
            ..default()
        },
        PresenterWindow,
    ));
}

/// Attach/detach the egui camera in lockstep with the native winit window.
///
/// Waiting for `WindowCreated` prevents bevy_egui from observing a render
/// target before winit can resolve it.
pub(crate) fn sync_presenter_camera_system(
    mut created: MessageReader<WindowCreated>,
    mut closed: MessageReader<WindowClosed>,
    presenter_windows: Query<(), With<PresenterWindow>>,
    presenter_cameras: Query<(Entity, &PresenterCamera)>,
    mut commands: Commands,
) {
    for event in created.read() {
        if presenter_windows.get(event.window).is_err()
            || presenter_cameras
                .iter()
                .any(|(_, camera)| camera.window == event.window)
        {
            continue;
        }
        commands.spawn((
            Camera3d::default(),
            Camera::default(),
            gaanim_scene::AuthoritativeCameraView,
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            RenderTarget::Window(WindowRef::Entity(event.window)),
            EguiSchedule::new(PresenterEguiPass),
            PresenterCamera {
                window: event.window,
            },
        ));
    }

    for event in closed.read() {
        for (entity, camera) in &presenter_cameras {
            if camera.window == event.window {
                // This is a fallback for programmatic removals. Native close
                // requests are handled before the Window is despawned below.
                commands.entity(entity).try_despawn();
            }
        }
    }
}

/// Remove presenter cameras while their render target still exists.
///
/// Bevy despawns a closing Window in Update, runs `camera_system` in
/// PostUpdate, and only emits `WindowClosed` in Last. Waiting for
/// `WindowClosed` therefore leaves one frame with a dangling render target.
pub(crate) fn cleanup_presenter_before_window_close_system(
    mut close_requests: MessageReader<WindowCloseRequested>,
    closing_windows: Query<Entity, (With<PresenterWindow>, With<ClosingWindow>)>,
    presenter_windows: Query<(), With<PresenterWindow>>,
    presenter_cameras: Query<(Entity, &PresenterCamera)>,
    mut overview: ResMut<PresenterOverviewState>,
    mut commands: Commands,
) {
    let mut closing = close_requests
        .read()
        .filter_map(|event| {
            presenter_windows
                .get(event.window)
                .is_ok()
                .then_some(event.window)
        })
        .collect::<Vec<_>>();
    closing.extend(closing_windows.iter());
    closing.sort_unstable();
    closing.dedup();
    if !closing.is_empty() {
        overview.open = false;
        overview.query.clear();
    }

    for (camera_entity, camera) in &presenter_cameras {
        if closing.contains(&camera.window) {
            commands.entity(camera_entity).try_despawn();
        }
    }
}

fn apply_presentation_action(
    action: PresentationAction,
    timeline: &mut Timeline,
    audience_blank: &mut AudienceBlank,
    overview: &mut PresenterOverviewState,
) {
    match action {
        PresentationAction::Advance => {
            if timeline.is_playing {
                timeline.seek_request = Some(
                    timeline
                        .next_stop(timeline.current_time)
                        .unwrap_or(timeline.cached_duration),
                );
                timeline.is_playing = false;
            } else {
                timeline.is_playing = true;
            }
        }
        PresentationAction::Previous => {
            timeline.is_playing = false;
            timeline.seek_request =
                Some(timeline.previous_stop(timeline.current_time).unwrap_or(0.0));
        }
        PresentationAction::TogglePlayback => timeline.is_playing = !timeline.is_playing,
        PresentationAction::Home => {
            timeline.is_playing = false;
            timeline.seek_request = Some(0.0);
        }
        PresentationAction::End => {
            timeline.is_playing = false;
            timeline.seek_request = Some(timeline.cached_duration);
        }
        PresentationAction::ToggleOverview => {
            overview.open = !overview.open;
            overview.focus_search = overview.open;
        }
        PresentationAction::ToggleBlack => {
            *audience_blank = if *audience_blank == AudienceBlank::Black {
                AudienceBlank::None
            } else {
                AudienceBlank::Black
            };
        }
        PresentationAction::ToggleWhite => {
            *audience_blank = if *audience_blank == AudienceBlank::White {
                AudienceBlank::None
            } else {
                AudienceBlank::White
            };
        }
        PresentationAction::ReopenPresenter => {}
    }
}

/// Route presentation shortcuts according to the focused native window.
#[allow(clippy::too_many_arguments)]
pub(crate) fn presentation_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    egui_wants: Res<EguiWantsInput>,
    presentation_mode: Res<PresentationMode>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    presenter_windows: Query<&Window, With<PresenterWindow>>,
    audience_controls: Res<AudienceControlsState>,
    timeline: Option<ResMut<Timeline>>,
    mut audience_blank: ResMut<AudienceBlank>,
    mut overview: ResMut<PresenterOverviewState>,
    mut commands: Commands,
) {
    if !presentation_mode.active {
        return;
    }
    let Some(mut timeline) = timeline else {
        return;
    };

    let primary_focused = primary_window.single().is_ok_and(|window| window.focused);
    let presenter_focused = presenter_windows.iter().any(|window| window.focused);
    if !primary_focused && !presenter_focused {
        return;
    }

    let keyboard_captured = presenter_focused && egui_wants.wants_keyboard_input();
    let pointer_captured = (presenter_focused && egui_wants.wants_any_pointer_input())
        || (primary_focused && audience_controls.pointer_over);
    let mut actions = Vec::new();

    if !keyboard_captured {
        if keys.just_pressed(KeyCode::ArrowRight)
            || keys.just_pressed(KeyCode::Space)
            || keys.just_pressed(KeyCode::Enter)
        {
            actions.push(PresentationAction::Advance);
        }
        if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::Backspace) {
            actions.push(PresentationAction::Previous);
        }
        if keys.just_pressed(KeyCode::Home) {
            actions.push(PresentationAction::Home);
        }
        if keys.just_pressed(KeyCode::End) {
            actions.push(PresentationAction::End);
        }
        if keys.just_pressed(KeyCode::KeyO) {
            actions.push(PresentationAction::ToggleOverview);
        }
        if keys.just_pressed(KeyCode::KeyB) {
            actions.push(PresentationAction::ToggleBlack);
        }
        if keys.just_pressed(KeyCode::KeyW) {
            actions.push(PresentationAction::ToggleWhite);
        }
        if keys.just_pressed(KeyCode::KeyP) && presenter_windows.is_empty() {
            actions.push(PresentationAction::ReopenPresenter);
        }
    }
    if primary_focused && !pointer_captured && mouse.just_pressed(MouseButton::Left) {
        actions.push(PresentationAction::Advance);
    }

    for action in actions {
        if action == PresentationAction::ReopenPresenter {
            spawn_presenter_window(&mut commands);
        } else {
            apply_presentation_action(action, &mut timeline, &mut audience_blank, &mut overview);
        }
    }
}

/// Compact playback dock rendered over the fullscreen audience window.
///
/// It deliberately exposes only presentation-safe navigation and uses the
/// same reducer as Presenter View and keyboard shortcuts.
pub(crate) fn audience_playback_controls_system(
    mut contexts: bevy_egui::EguiContexts,
    presentation_mode: Res<PresentationMode>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    mut audience_controls: ResMut<AudienceControlsState>,
    mut timeline: ResMut<Timeline>,
    mut audience_blank: ResMut<AudienceBlank>,
    mut overview: ResMut<PresenterOverviewState>,
) {
    let (audience_focused, cursor_in_dock_zone) = primary_window
        .single()
        .map(|window| {
            (
                window.focused,
                cursor_in_audience_dock_zone(
                    window.cursor_position(),
                    window.width(),
                    window.height(),
                ),
            )
        })
        .unwrap_or((false, false));
    if !audience_controls_visible(
        presentation_mode.active,
        *audience_blank,
        audience_focused,
        audience_controls.pointer_over,
        cursor_in_dock_zone,
    ) {
        audience_controls.pointer_over = false;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let total = timeline.cached_duration.max(0.0);
    let current = timeline.current_time.clamp(0.0, total);
    let progress = if total > 0.0 {
        (current / total) as f32
    } else {
        0.0
    };
    let current_cue = timeline
        .segment_position_at(current)
        .and_then(|position| {
            timeline
                .segments
                .iter()
                .find(|segment| segment.id == position.segment_id)
                .map(|segment| cue_label(segment, position.stop_index))
        })
        .unwrap_or_else(|| "Presentation".to_string());
    let format_time = |seconds: f64| {
        let seconds = seconds.max(0.0).round() as u64;
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    };
    let mut actions = Vec::new();

    let response = egui::Area::new("audience-playback-controls".into())
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(8, 14, 27, 238))
                .corner_radius(16.0)
                .inner_margin(egui::Margin::symmetric(16, 12))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_premultiplied(91, 143, 255, 150),
                ))
                .show(ui, |ui| {
                    let width = (ctx.viewport_rect().width() - 48.0).clamp(280.0, 920.0);
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing = egui::vec2(9.0, 7.0);
                    ui.horizontal(|ui| {
                        let status_color = if timeline.is_playing {
                            egui::Color32::from_rgb(105, 220, 155)
                        } else {
                            egui::Color32::from_rgb(255, 209, 102)
                        };
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(17, 27, 47))
                            .corner_radius(8.0)
                            .inner_margin(egui::Margin::symmetric(9, 7))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(if timeline.is_playing {
                                        "PLAYING"
                                    } else {
                                        "READY"
                                    })
                                    .strong()
                                    .size(11.0)
                                    .color(status_color),
                                );
                            });
                        if ui
                            .add_sized(
                                [58.0, 34.0],
                                egui::Button::new("Start")
                                    .fill(egui::Color32::from_rgb(22, 33, 57))
                                    .corner_radius(8.0),
                            )
                            .clicked()
                        {
                            actions.push(PresentationAction::Home);
                        }
                        if ui
                            .add_sized(
                                [84.0, 34.0],
                                egui::Button::new("Previous")
                                    .fill(egui::Color32::from_rgb(22, 33, 57))
                                    .corner_radius(8.0),
                            )
                            .clicked()
                        {
                            actions.push(PresentationAction::Previous);
                        }
                        let primary_label = if timeline.is_playing {
                            "Pause"
                        } else {
                            "Advance"
                        };
                        if ui
                            .add_sized(
                                [112.0, 36.0],
                                egui::Button::new(primary_label)
                                    .fill(egui::Color32::from_rgb(70, 112, 207))
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        egui::Color32::from_rgb(125, 168, 255),
                                    ))
                                    .corner_radius(8.0),
                            )
                            .clicked()
                        {
                            actions.push(if timeline.is_playing {
                                PresentationAction::TogglePlayback
                            } else {
                                PresentationAction::Advance
                            });
                        }
                        if ui
                            .add_sized(
                                [50.0, 34.0],
                                egui::Button::new("End")
                                    .fill(egui::Color32::from_rgb(22, 33, 57))
                                    .corner_radius(8.0),
                            )
                            .clicked()
                        {
                            actions.push(PresentationAction::End);
                        }
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width().max(150.0));
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("CURRENT CUE")
                                        .strong()
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(145, 190, 255)),
                                );
                                ui.label(egui::RichText::new(&current_cue).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.monospace(format!(
                                            "{} / {}",
                                            format_time(current),
                                            format_time(total)
                                        ));
                                    },
                                );
                            });
                            ui.add(
                                egui::ProgressBar::new(progress.clamp(0.0, 1.0))
                                    .desired_height(6.0)
                                    .fill(egui::Color32::from_rgb(91, 143, 255)),
                            );
                        });
                    });
                });
        });
    audience_controls.pointer_over = response.response.contains_pointer();

    for action in actions {
        apply_presentation_action(action, &mut timeline, &mut audience_blank, &mut overview);
    }
}

/// Speaker-facing controls and semantic presentation information.
fn cue_texture(
    overview: &PresenterOverviewState,
    segment_id: u32,
    stop_index: Option<usize>,
) -> Option<egui::TextureHandle> {
    let moment = stop_index
        .map(|index| ThumbnailMoment::Stop(index as u32))
        .unwrap_or(ThumbnailMoment::Entry);
    overview.textures.get(&(segment_id, moment)).cloned()
}

fn show_cue_image(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    max_width: f32,
    max_height: f32,
    empty_message: &str,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(max_width.max(1.0), max_height.max(1.0)),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 10.0, egui::Color32::from_rgb(3, 6, 13));
    ui.painter().rect_stroke(
        rect,
        10.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(31, 45, 69)),
        egui::StrokeKind::Inside,
    );
    if let Some(texture) = texture {
        let texture_size = texture.size_vec2();
        let scale = (rect.width() / texture_size.x).min(rect.height() / texture_size.y);
        let image_size = texture_size * scale;
        let image_rect = egui::Rect::from_center_size(rect.center(), image_size);
        egui::Image::new(texture).paint_at(ui, image_rect);
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            empty_message,
            egui::FontId::proportional(16.0),
            egui::Color32::from_rgb(112, 126, 149),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn show_current_cue(
    ui: &mut egui::Ui,
    segment: &gaanim_timeline::timeline::SegmentMetadata,
    segment_index: usize,
    segment_count: usize,
    stop_index: Option<usize>,
    texture: Option<&egui::TextureHandle>,
    preview_message: &str,
    requested_seek: &mut Option<f64>,
    timeline: &Timeline,
) {
    let cue_name = cue_label(segment, stop_index);
    let cue_number = stop_index.map(|index| index + 2).unwrap_or(1);
    let cue_count = segment.stops.len() + 1;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("CURRENT CUE")
                .strong()
                .size(12.0)
                .color(egui::Color32::from_rgb(145, 190, 255)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.monospace(format!("CUE {:02} / {:02}", cue_number, cue_count));
            ui.label(
                egui::RichText::new(format!(
                    "SEGMENT {:02} / {:02}",
                    segment_index + 1,
                    segment_count
                ))
                .size(11.0)
                .color(egui::Color32::from_rgb(112, 126, 149)),
            );
        });
    });
    ui.label(
        egui::RichText::new(cue_name)
            .strong()
            .size(28.0)
            .color(egui::Color32::from_rgb(238, 244, 255)),
    );
    let preview_height = (ui.available_height() - 92.0).clamp(180.0, 520.0);
    show_cue_image(
        ui,
        texture,
        ui.available_width(),
        preview_height,
        preview_message,
    );
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        let entry_active = stop_index.is_none();
        let entry = egui::Button::new("Start").selected(entry_active);
        if ui.add(entry).clicked() {
            *requested_seek = timeline.segment_time_indexed(&segment.name, None);
        }
        for (index, stop) in segment.stops.iter().enumerate() {
            let label = stop
                .name
                .as_deref()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Cue {}", index + 1));
            if ui
                .add(egui::Button::new(label).selected(stop_index == Some(index)))
                .clicked()
            {
                *requested_seek = timeline.segment_time_indexed(&segment.name, Some(index));
            }
        }
    });
}

fn show_speaker_column(
    ui: &mut egui::Ui,
    notes: Option<&str>,
    next_label: Option<&str>,
    next_texture: Option<&egui::TextureHandle>,
    preview_message: &str,
) {
    ui.label(
        egui::RichText::new("UP NEXT")
            .strong()
            .size(12.0)
            .color(egui::Color32::from_rgb(145, 190, 255)),
    );
    ui.label(
        egui::RichText::new(next_label.unwrap_or("End of presentation"))
            .strong()
            .size(20.0),
    );
    let next_message = if next_label.is_some() {
        preview_message
    } else {
        "Presentation complete"
    };
    show_cue_image(ui, next_texture, ui.available_width(), 150.0, next_message);
    ui.add_space(16.0);
    ui.label(
        egui::RichText::new("SPEAKER NOTES")
            .strong()
            .size(12.0)
            .color(egui::Color32::from_rgb(255, 209, 102)),
    );
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(13, 20, 36))
        .corner_radius(9.0)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(ui.available_height().max(160.0))
                .show(ui, |ui| {
                    let notes = notes.filter(|notes| !notes.trim().is_empty());
                    let mut text =
                        egui::RichText::new(notes.unwrap_or("No speaker notes for this segment."))
                            .size(20.0);
                    if notes.is_none() {
                        text = text.color(egui::Color32::from_rgb(126, 140, 163));
                    }
                    ui.label(text.line_height(Some(28.0)));
                });
        });
}

#[allow(clippy::too_many_arguments)]
fn show_presenter_status(
    ui: &mut egui::Ui,
    current_segment: Option<(usize, &str)>,
    segment_count: usize,
    current_time: f64,
    total_time: f64,
    is_playing: bool,
    audience_blank: AudienceBlank,
    talk_elapsed: &str,
    local_time: &str,
    compact: bool,
    presentation_timer: &mut PresentationTimer,
) {
    let status_text = if is_playing { "PLAYING" } else { "READY" };
    let status_color = if is_playing {
        egui::Color32::from_rgb(105, 220, 155)
    } else {
        egui::Color32::from_rgb(255, 209, 102)
    };
    let progress = if total_time > 0.0 {
        (current_time / total_time) as f32
    } else {
        0.0
    };

    if compact {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(status_text)
                    .strong()
                    .color(status_color),
            );
            if let Some((index, segment_name)) = current_segment {
                ui.separator();
                ui.label(egui::RichText::new(segment_name).strong().size(17.0));
                ui.weak(format!("{} / {}", index + 1, segment_count));
            }
        });
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(talk_elapsed)
                    .monospace()
                    .strong()
                    .size(22.0)
                    .color(egui::Color32::from_rgb(145, 190, 255)),
            );
            if ui.small_button("Reset").clicked() {
                presentation_timer.reset();
            }
            ui.separator();
            ui.monospace(format!(
                "{} / {}",
                format_timeline_time(current_time),
                format_timeline_time(total_time)
            ));
            ui.weak(format!("Local {local_time}"));
        });
    } else {
        ui.horizontal(|ui| {
            let left_width = (ui.available_width() - 290.0).max(360.0);
            ui.allocate_ui_with_layout(
                egui::vec2(left_width, 64.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.horizontal(|ui| {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(17, 27, 47))
                            .corner_radius(7.0)
                            .inner_margin(egui::Margin::symmetric(9, 5))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(status_text)
                                        .strong()
                                        .size(11.0)
                                        .color(status_color),
                                );
                            });
                        if let Some((index, segment_name)) = current_segment {
                            ui.label(egui::RichText::new(segment_name).strong().size(19.0));
                            ui.label(
                                egui::RichText::new(format!(
                                    "SEGMENT {} / {}",
                                    index + 1,
                                    segment_count
                                ))
                                .size(11.0)
                                .color(egui::Color32::from_rgb(112, 126, 149)),
                            );
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("TIMELINE")
                                .strong()
                                .size(10.0)
                                .color(egui::Color32::from_rgb(112, 126, 149)),
                        );
                        ui.monospace(format!(
                            "{} / {}",
                            format_timeline_time(current_time),
                            format_timeline_time(total_time)
                        ));
                        match audience_blank {
                            AudienceBlank::Black => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(255, 209, 102),
                                    "AUDIENCE BLACK",
                                );
                            }
                            AudienceBlank::White => {
                                ui.colored_label(
                                    egui::Color32::from_rgb(255, 209, 102),
                                    "AUDIENCE WHITE",
                                );
                            }
                            AudienceBlank::None => {}
                        }
                    });
                },
            );
            ui.separator();
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("SESSION TIMER")
                        .strong()
                        .size(10.0)
                        .color(egui::Color32::from_rgb(112, 126, 149)),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(talk_elapsed)
                            .monospace()
                            .strong()
                            .size(24.0)
                            .color(egui::Color32::from_rgb(145, 190, 255)),
                    );
                    if ui
                        .add(
                            egui::Button::new("Reset")
                                .fill(egui::Color32::from_rgb(22, 33, 57))
                                .corner_radius(7.0),
                        )
                        .clicked()
                    {
                        presentation_timer.reset();
                    }
                });
                ui.label(
                    egui::RichText::new(format!("Local time  {local_time}"))
                        .monospace()
                        .size(11.0)
                        .color(egui::Color32::from_rgb(137, 151, 174)),
                );
            });
        });
    }

    ui.add(
        egui::ProgressBar::new(progress.clamp(0.0, 1.0))
            .desired_height(6.0)
            .fill(egui::Color32::from_rgb(91, 143, 255)),
    );
}

/// Focus-first speaker cockpit for live presentations.
pub(crate) fn presenter_view_system(
    mut contexts: Query<(Entity, &mut EguiContext), With<PresenterCamera>>,
    mut timeline: ResMut<Timeline>,
    mut audience_blank: ResMut<AudienceBlank>,
    replay_stash: Res<StashedReplay>,
    mut thumbnail_cache: ResMut<PresenterThumbnailCache>,
    mut overview: ResMut<PresenterOverviewState>,
    mut presentation_timer: ResMut<PresentationTimer>,
) {
    let Ok((camera_entity, mut context)) = contexts.single_mut() else {
        return;
    };
    let ctx = context.get_mut();
    apply_presenter_style(ctx);

    if overview.texture_revision != replay_stash.revision {
        overview.textures.clear();
        overview.uploaded_camera = None;
    }
    let max_thumbnail_edge =
        desired_thumbnail_edge(ctx.viewport_rect().width(), ctx.pixels_per_point());
    thumbnail_cache.request(&replay_stash, &timeline, max_thumbnail_edge);
    if let Some((revision, result)) = thumbnail_cache.receive()
        && revision == replay_stash.revision
    {
        match result {
            Ok(frames) => thumbnail_cache.store(revision, frames),
            Err(error) => thumbnail_cache.fail(error),
        }
    }
    if thumbnail_upload_required(
        &overview,
        &thumbnail_cache,
        camera_entity,
        replay_stash.revision,
    ) {
        overview.textures.clear();
        for frame in &thumbnail_cache.pixels {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &frame.rgba,
            );
            let texture = ctx.load_texture(
                format!(
                    "presenter-cue-{}-{:?}-{}-{}",
                    frame.segment_id,
                    frame.moment,
                    replay_stash.revision,
                    thumbnail_cache.pixel_generation
                ),
                image,
                egui::TextureOptions::LINEAR,
            );
            overview
                .textures
                .insert((frame.segment_id, frame.moment), texture);
        }
        overview.texture_revision = replay_stash.revision;
        overview.texture_generation = thumbnail_cache.pixel_generation;
        overview.uploaded_camera = Some(camera_entity);
    }

    let current_time = timeline.current_time;
    let current_position = timeline.segment_position_at(current_time);
    let current = current_position.and_then(|position| {
        timeline
            .segments
            .iter()
            .position(|segment| segment.id == position.segment_id)
            .map(|index| (index, position, timeline.segments[index].clone()))
    });
    let current_texture = current
        .as_ref()
        .and_then(|(_, position, segment)| cue_texture(&overview, segment.id, position.stop_index));
    let current_segment_id = current.as_ref().map(|(_, _, segment)| segment.id);
    let next = timeline.next_stop(current_time).and_then(|time| {
        let position = timeline.segment_position_at(time)?;
        let segment = timeline
            .segments
            .iter()
            .find(|segment| segment.id == position.segment_id)?
            .clone();
        Some((
            next_cue_label(current_segment_id, &segment, position.stop_index),
            cue_texture(&overview, segment.id, position.stop_index),
        ))
    });

    let talk_elapsed = format_stopwatch(presentation_timer.elapsed());
    let local_time = chrono::Local::now().format("%H:%M:%S").to_string();
    let mut actions = Vec::new();
    let mut requested_seek = None;
    let compact_header = ctx.viewport_rect().width() < 900.0;
    let preview_message = if thumbnail_cache.error.is_some() {
        "Preview unavailable — retry below"
    } else {
        "Rendering cue preview…"
    };

    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        "presenter-viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::top("presenter-status")
        .exact_size(if compact_header { 112.0 } else { 94.0 })
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(7, 11, 22))
                .inner_margin(egui::Margin::symmetric(20, 10)),
        )
        .show(&mut viewport_ui, |ui| {
            show_presenter_status(
                ui,
                current
                    .as_ref()
                    .map(|(index, _, segment)| (*index, segment.name.as_str())),
                timeline.segments.len(),
                current_time,
                timeline.cached_duration,
                timeline.is_playing,
                *audience_blank,
                &talk_elapsed,
                &local_time,
                compact_header,
                &mut presentation_timer,
            );
        });

    egui::Panel::bottom("presenter-dock")
        .exact_size(72.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(9, 15, 28))
                .inner_margin(egui::Margin::symmetric(18, 13)),
        )
        .show(&mut viewport_ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if ui
                    .add_sized(
                        [118.0, 42.0],
                        egui::Button::new("Previous")
                            .fill(egui::Color32::from_rgb(22, 33, 57))
                            .corner_radius(8.0),
                    )
                    .on_hover_text("Left arrow or Backspace")
                    .clicked()
                {
                    actions.push(PresentationAction::Previous);
                }
                let primary_label = if timeline.is_playing {
                    "Pause"
                } else {
                    "Advance"
                };
                if ui
                    .add_sized(
                        [164.0, 42.0],
                        egui::Button::new(primary_label)
                            .fill(egui::Color32::from_rgb(70, 112, 207))
                            .stroke(egui::Stroke::new(
                                1.0,
                                egui::Color32::from_rgb(125, 168, 255),
                            ))
                            .corner_radius(8.0),
                    )
                    .on_hover_text("Space, Enter, or Right arrow")
                    .clicked()
                {
                    actions.push(if timeline.is_playing {
                        PresentationAction::TogglePlayback
                    } else {
                        PresentationAction::Advance
                    });
                }
                if ui
                    .add_sized(
                        [124.0, 42.0],
                        egui::Button::new("Overview  O")
                            .fill(egui::Color32::from_rgb(22, 33, 57))
                            .corner_radius(8.0),
                    )
                    .clicked()
                {
                    actions.push(PresentationAction::ToggleOverview);
                }
                ui.separator();
                if ui
                    .add_sized(
                        [96.0, 42.0],
                        egui::Button::new("Black  B")
                            .selected(*audience_blank == AudienceBlank::Black)
                            .fill(egui::Color32::from_rgb(22, 33, 57))
                            .corner_radius(8.0),
                    )
                    .clicked()
                {
                    actions.push(PresentationAction::ToggleBlack);
                }
                if ui
                    .add_sized(
                        [96.0, 42.0],
                        egui::Button::new("White  W")
                            .selected(*audience_blank == AudienceBlank::White)
                            .fill(egui::Color32::from_rgb(22, 33, 57))
                            .corner_radius(8.0),
                    )
                    .clicked()
                {
                    actions.push(PresentationAction::ToggleWhite);
                }
                if ui.available_width() > 190.0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("SPACE / ENTER / RIGHT to advance")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(112, 126, 149)),
                        );
                    });
                }
            });
        });

    let wide = ctx.viewport_rect().width() >= 900.0;
    if wide {
        egui::Panel::right("presenter-notes")
            .exact_size((ctx.viewport_rect().width() * 0.34).clamp(300.0, 470.0))
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(9, 15, 28))
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(&mut viewport_ui, |ui| {
                show_speaker_column(
                    ui,
                    current
                        .as_ref()
                        .and_then(|(_, _, segment)| segment.notes.as_deref()),
                    next.as_ref().map(|(label, _)| label.as_str()),
                    next.as_ref().and_then(|(_, texture)| texture.as_ref()),
                    preview_message,
                );
            });
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(7, 11, 22))
                .inner_margin(egui::Margin::same(20)),
        )
        .show(&mut viewport_ui, |ui| {
            if let Some((index, position, segment)) = &current {
                if wide {
                    show_current_cue(
                        ui,
                        segment,
                        *index,
                        timeline.segments.len(),
                        position.stop_index,
                        current_texture.as_ref(),
                        preview_message,
                        &mut requested_seek,
                        &timeline,
                    );
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        show_current_cue(
                            ui,
                            segment,
                            *index,
                            timeline.segments.len(),
                            position.stop_index,
                            current_texture.as_ref(),
                            preview_message,
                            &mut requested_seek,
                            &timeline,
                        );
                        ui.add_space(18.0);
                        show_speaker_column(
                            ui,
                            segment.notes.as_deref(),
                            next.as_ref().map(|(label, _)| label.as_str()),
                            next.as_ref().and_then(|(_, texture)| texture.as_ref()),
                            preview_message,
                        );
                    });
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.heading("No presentation segments are defined.");
                });
            }
            if thumbnail_cache.is_loading() && overview.textures.is_empty() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.small("Rendering cue previews…");
                });
            } else if let Some(error) = thumbnail_cache.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 140, 140),
                        format!("Cue previews unavailable: {error}"),
                    );
                    if ui.button("Retry previews").clicked() {
                        thumbnail_cache.retry();
                    }
                });
            }
        });

    if overview.open {
        let rect = ctx.viewport_rect().shrink(12.0);
        egui::Window::new("Presentation overview")
            .title_bar(false)
            .resizable(false)
            .fixed_rect(rect)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Jump to a cue");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close  Esc").clicked() {
                            actions.push(PresentationAction::ToggleOverview);
                        }
                    });
                });
                let search = ui.add(
                    egui::TextEdit::singleline(&mut overview.query)
                        .hint_text("Search segments…")
                        .desired_width(f32::INFINITY),
                );
                if overview.focus_search {
                    search.request_focus();
                    overview.focus_search = false;
                }
                ui.add_space(10.0);
                let query = overview.query.trim().to_lowercase();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (index, segment) in timeline.segments.iter().enumerate() {
                            if !query.is_empty() && !segment.name.to_lowercase().contains(&query) {
                                continue;
                            }
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(13, 20, 36))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(43, 58, 82)))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_width(250.0);
                                    let texture = overview
                                        .textures
                                        .get(&(segment.id, ThumbnailMoment::Complete));
                                    show_cue_image(ui, texture, 250.0, 140.0, preview_message);
                                    if ui
                                        .button(format!("{:02}  {}", index + 1, segment.name))
                                        .clicked()
                                    {
                                        requested_seek =
                                            timeline.segment_time_indexed(&segment.name, None);
                                    }
                                    ui.horizontal_wrapped(|ui| {
                                        for (stop_index, stop) in segment.stops.iter().enumerate() {
                                            let label = stop
                                                .name
                                                .as_deref()
                                                .map(str::to_owned)
                                                .unwrap_or_else(|| {
                                                    format!("Cue {}", stop_index + 1)
                                                });
                                            if ui.small_button(label).clicked() {
                                                requested_seek = timeline.segment_time_indexed(
                                                    &segment.name,
                                                    Some(stop_index),
                                                );
                                            }
                                        }
                                    });
                                });
                        }
                    });
                });
            });
    }

    for action in actions {
        apply_presentation_action(action, &mut timeline, &mut audience_blank, &mut overview);
    }
    if let Some(time) = requested_seek {
        timeline.is_playing = false;
        timeline.seek_request = Some(time);
        overview.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AudienceControlsState, PresentationAction, PresentationTimer, PresenterCamera,
        PresenterOverviewState, PresenterThumbnailCache, PresenterWindow, ThumbnailMoment,
        ThumbnailPixels, apply_presentation_action, audience_controls_visible,
        cleanup_presenter_before_window_close_system, cue_label, cursor_in_audience_dock_zone,
        desired_thumbnail_edge, entry_segment_time, format_stopwatch, next_cue_label,
        presentation_input_system, representative_segment_time, sync_presentation_timer_system,
        thumbnail_dimensions, thumbnail_upload_required,
    };
    use crate::{AudienceBlank, PresentationMode};
    use bevy::{
        camera::Camera,
        prelude::*,
        window::{PrimaryWindow, WindowCloseRequested},
    };
    use bevy_egui::input::EguiWantsInput;
    use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop, Timeline};
    use std::time::{Duration, Instant};

    #[test]
    fn close_request_removes_presenter_camera_before_the_window() {
        let mut app = App::new();
        app.add_message::<WindowCloseRequested>()
            .init_resource::<PresenterOverviewState>()
            .add_systems(Update, cleanup_presenter_before_window_close_system);
        let window = app
            .world_mut()
            .spawn((Window::default(), PresenterWindow))
            .id();
        let camera = app
            .world_mut()
            .spawn((Camera::default(), PresenterCamera { window }))
            .id();
        app.world_mut()
            .write_message(WindowCloseRequested { window });

        app.update();

        assert!(app.world().get_entity(camera).is_err());
        assert!(app.world().get_entity(window).is_ok());
    }

    #[test]
    fn presenter_actions_share_one_advance_semantics() {
        let mut timeline = Timeline::default();
        timeline.cached_duration = 2.0;
        timeline.set_segments(vec![SegmentMetadata {
            id: 1,
            name: "demo".to_string(),
            notes: None,
            start_time: 0.0,
            end_time: 2.0,
            stops: vec![SegmentStop {
                name: Some("cue".to_string()),
                time: 1.0,
            }],
        }]);
        let mut blank = AudienceBlank::None;
        let mut overview = PresenterOverviewState::default();

        apply_presentation_action(
            PresentationAction::Advance,
            &mut timeline,
            &mut blank,
            &mut overview,
        );
        assert!(timeline.is_playing);

        apply_presentation_action(
            PresentationAction::Advance,
            &mut timeline,
            &mut blank,
            &mut overview,
        );
        assert!(!timeline.is_playing);
        assert_eq!(timeline.seek_request, Some(1.0));
    }

    #[test]
    fn cue_labels_prefer_authored_names_and_avoid_repeating_the_current_segment() {
        let segment = SegmentMetadata {
            id: 1,
            name: "Reveal in steps".to_string(),
            notes: None,
            start_time: 0.0,
            end_time: 3.0,
            stops: vec![
                SegmentStop {
                    name: Some("named-segments".to_string()),
                    time: 1.0,
                },
                SegmentStop {
                    name: None,
                    time: 2.0,
                },
            ],
        };
        assert_eq!(cue_label(&segment, None), "Start");
        assert_eq!(cue_label(&segment, Some(0)), "named-segments");
        assert_eq!(cue_label(&segment, Some(1)), "Cue 2");
        assert_eq!(next_cue_label(Some(1), &segment, Some(0)), "named-segments");
        assert_eq!(
            next_cue_label(Some(2), &segment, Some(0)),
            "Reveal in steps / named-segments"
        );
    }

    #[test]
    fn audience_controls_require_the_fullscreen_window_to_have_focus() {
        assert!(audience_controls_visible(
            true,
            AudienceBlank::None,
            true,
            false,
            true
        ));
        assert!(audience_controls_visible(
            true,
            AudienceBlank::None,
            true,
            true,
            false
        ));
        assert!(!audience_controls_visible(
            true,
            AudienceBlank::None,
            true,
            false,
            false
        ));
        assert!(!audience_controls_visible(
            true,
            AudienceBlank::None,
            false,
            true,
            true
        ));
        assert!(!audience_controls_visible(
            true,
            AudienceBlank::Black,
            true,
            true,
            true
        ));
        assert!(!audience_controls_visible(
            false,
            AudienceBlank::None,
            true,
            true,
            true
        ));
    }

    #[test]
    fn audience_dock_reveals_only_inside_its_bottom_hover_zone() {
        assert!(cursor_in_audience_dock_zone(
            Some(Vec2::new(960.0, 1040.0)),
            1920.0,
            1080.0
        ));
        assert!(!cursor_in_audience_dock_zone(
            Some(Vec2::new(960.0, 700.0)),
            1920.0,
            1080.0
        ));
        assert!(!cursor_in_audience_dock_zone(
            Some(Vec2::new(100.0, 1040.0)),
            1920.0,
            1080.0
        ));
        assert!(!cursor_in_audience_dock_zone(None, 1920.0, 1080.0));
    }

    #[test]
    fn focused_audience_window_receives_advance_shortcuts() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<EguiWantsInput>()
            .init_resource::<Timeline>()
            .init_resource::<AudienceBlank>()
            .init_resource::<AudienceControlsState>()
            .init_resource::<PresenterOverviewState>()
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, presentation_input_system);
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowRight);

        app.update();

        assert!(app.world().resource::<Timeline>().is_playing);
    }

    #[test]
    fn presentation_input_skips_frames_without_a_timeline() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<EguiWantsInput>()
            .init_resource::<AudienceBlank>()
            .init_resource::<AudienceControlsState>()
            .init_resource::<PresenterOverviewState>()
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, presentation_input_system);
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowRight);

        app.update();

        assert!(!app.world().contains_resource::<Timeline>());
    }

    #[test]
    fn focused_presenter_window_receives_advance_shortcuts() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<EguiWantsInput>()
            .init_resource::<Timeline>()
            .init_resource::<AudienceBlank>()
            .init_resource::<AudienceControlsState>()
            .init_resource::<PresenterOverviewState>()
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, presentation_input_system);
        app.world_mut().spawn((
            Window {
                focused: false,
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            PresenterWindow,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);

        app.update();

        assert!(app.world().resource::<Timeline>().is_playing);
    }

    #[test]
    fn audience_click_advances_when_the_audience_window_is_focused() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<EguiWantsInput>()
            .init_resource::<Timeline>()
            .init_resource::<AudienceBlank>()
            .init_resource::<AudienceControlsState>()
            .init_resource::<PresenterOverviewState>()
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, presentation_input_system);
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);

        app.update();

        assert!(app.world().resource::<Timeline>().is_playing);
    }

    #[test]
    fn audience_control_click_does_not_also_advance_the_slide() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<EguiWantsInput>()
            .init_resource::<Timeline>()
            .init_resource::<AudienceBlank>()
            .insert_resource(AudienceControlsState { pointer_over: true })
            .init_resource::<PresenterOverviewState>()
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, presentation_input_system);
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);

        app.update();

        assert!(!app.world().resource::<Timeline>().is_playing);
    }

    #[test]
    fn reopen_shortcut_does_not_duplicate_an_existing_presenter() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<EguiWantsInput>()
            .init_resource::<Timeline>()
            .init_resource::<AudienceBlank>()
            .init_resource::<AudienceControlsState>()
            .init_resource::<PresenterOverviewState>()
            .insert_resource(PresentationMode { active: true })
            .add_systems(Update, presentation_input_system);
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut().spawn((Window::default(), PresenterWindow));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyP);

        app.update();

        let mut query = app
            .world_mut()
            .query_filtered::<Entity, With<PresenterWindow>>();
        assert_eq!(query.iter(app.world()).count(), 1);
    }

    #[test]
    fn thumbnail_dimensions_preserve_common_aspect_ratios() {
        assert_eq!(thumbnail_dimensions(1920, 1080, 960), (960, 540));
        assert_eq!(thumbnail_dimensions(1080, 1920, 960), (540, 960));
        assert_eq!(thumbnail_dimensions(0, 0, 960), (960, 960));
    }

    #[test]
    fn thumbnail_resolution_adapts_to_viewport_and_dpi() {
        assert_eq!(desired_thumbnail_edge(1180.0, 1.0), 960);
        assert_eq!(desired_thumbnail_edge(1180.0, 2.0), 1600);
        assert_eq!(desired_thumbnail_edge(4000.0, 2.0), 1600);
    }

    #[test]
    fn raw_thumbnail_cache_reuploads_for_a_reopened_presenter_camera() {
        let mut app = App::new();
        let old_camera = app.world_mut().spawn_empty().id();
        let reopened_camera = app.world_mut().spawn_empty().id();
        let mut cache = PresenterThumbnailCache::default();
        cache.store(
            7,
            vec![ThumbnailPixels {
                segment_id: 1,
                moment: ThumbnailMoment::Entry,
                width: 2,
                height: 1,
                rgba: vec![0; 8],
            }],
        );
        let overview = PresenterOverviewState {
            texture_revision: 7,
            texture_generation: cache.pixel_generation,
            uploaded_camera: Some(old_camera),
            ..default()
        };

        assert!(!thumbnail_upload_required(&overview, &cache, old_camera, 7));
        assert!(thumbnail_upload_required(
            &overview,
            &cache,
            reopened_camera,
            7
        ));
        assert_eq!(cache.pixels.len(), 1);
    }

    #[test]
    fn thumbnail_cache_keeps_one_gpu_worker_during_hot_reload() {
        let mut cache = PresenterThumbnailCache {
            requested_revision: 1,
            requested_dimensions: (320, 180),
            request_attempts: 1,
            ..default()
        };
        let (_sender, receiver) = crossbeam_channel::bounded(1);
        cache.receiver = Some(receiver);
        let stash = crate::export::StashedReplay {
            canvas: Some(gaanim_api::canvas::SceneModel::new(1920, 1080)),
            revision: 2,
        };
        let mut timeline = Timeline::new();
        timeline.set_segments(vec![SegmentMetadata {
            id: 1,
            name: "updated".into(),
            notes: None,
            start_time: 0.0,
            end_time: 1.0,
            stops: Vec::new(),
        }]);

        cache.request(&stash, &timeline, 960);

        assert_eq!(cache.requested_revision, 1);
        assert_eq!(cache.requested_dimensions, (320, 180));
        assert_eq!(cache.request_attempts, 1);
    }

    #[test]
    fn failed_thumbnail_cache_can_be_retried_explicitly() {
        let mut cache = PresenterThumbnailCache {
            requested_revision: 4,
            requested_dimensions: (960, 540),
            request_attempts: 2,
            error: Some("adapter temporarily unavailable".into()),
            ..default()
        };

        cache.retry();

        assert_eq!(cache.requested_revision, 0);
        assert_eq!(cache.requested_dimensions, (0, 0));
        assert_eq!(cache.request_attempts, 0);
        assert!(cache.error.is_none());
    }

    #[test]
    fn presentation_timer_starts_on_session_entry_and_can_reset() {
        let mut app = App::new();
        app.init_resource::<PresentationTimer>()
            .insert_resource(PresentationMode { active: false })
            .add_systems(Update, sync_presentation_timer_system);
        app.update();
        assert!(
            app.world()
                .resource::<PresentationTimer>()
                .started_at
                .is_none()
        );

        app.world_mut().resource_mut::<PresentationMode>().active = true;
        app.update();
        assert!(
            app.world()
                .resource::<PresentationTimer>()
                .started_at
                .is_some()
        );

        app.world_mut()
            .resource_mut::<PresentationTimer>()
            .started_at = Some(Instant::now() - Duration::from_secs(65));
        assert_eq!(
            format_stopwatch(app.world().resource::<PresentationTimer>().elapsed()),
            "00:01:05"
        );
        app.world_mut().resource_mut::<PresentationTimer>().reset();
        assert!(app.world().resource::<PresentationTimer>().elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn representative_time_stays_inside_the_segment() {
        assert_eq!(representative_segment_time(2.0, 2.0), 2.0);
        let time = representative_segment_time(2.0, 5.0);
        assert!(time >= 2.0 && time < 5.0);
    }

    #[test]
    fn entry_time_stays_inside_the_segment() {
        assert_eq!(entry_segment_time(2.0, 2.0), 2.0);
        let time = entry_segment_time(2.0, 5.0);
        assert!(time > 2.0 && time < 5.0);
    }
}
