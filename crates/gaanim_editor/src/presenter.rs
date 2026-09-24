//! Presenter view hosted in a second native window.

mod thumbnails;

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
use gaanim_timeline::timeline::{SegmentMetadata, Timeline};
use std::time::{Duration, Instant};

pub(crate) use thumbnails::PresenterThumbnailCache;
use thumbnails::{
    PreviewStatus, ThumbnailKey, ThumbnailMoment, desired_thumbnail_edge, entry_segment_time,
};

use crate::ui_kit::{
    ButtonTone, Icon, PRIMARY_SIZE, caption, chip, divider, icon_button, icon_button_sized,
    paint_icon, palette as kit_palette, pill_toggle, small_button,
};
use crate::{AudienceBlank, PresentationMode, export::StashedReplay, truncate_with_ellipsis};

/// Dedicated egui schedule for the presenter window.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PresenterEguiPass;

#[derive(Component)]
pub(crate) struct PresenterWindow;

#[derive(Component)]
pub(crate) struct PresenterCamera {
    window: Entity,
}

/// Ephemeral controls for navigating a presentation by slide name.
#[derive(Resource, Default)]
pub(crate) struct PresenterOverviewState {
    pub(crate) open: bool,
    pub(crate) query: String,
    focus_search: bool,
}

const NOTES_SIZE_MIN: f32 = 14.0;
const NOTES_SIZE_MAX: f32 = 44.0;

/// Speaker-adjustable Presenter View settings for the current session.
#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct PresenterPreferences {
    notes_size: f32,
}

impl Default for PresenterPreferences {
    fn default() -> Self {
        Self { notes_size: 22.0 }
    }
}

impl PresenterPreferences {
    fn adjust_notes_size(&mut self, delta: f32) {
        self.notes_size = (self.notes_size + delta).clamp(NOTES_SIZE_MIN, NOTES_SIZE_MAX);
    }
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

/// Presenter View colors, aligned with the editor's `ui_kit` palette. Text
/// stays brighter than in the editor: the speaker reads it at a glance from
/// a distance, often on a dimmed laptop screen.
mod palette {
    use crate::ui_kit::palette as kit;
    use bevy_egui::egui::Color32;

    pub(super) const BACKGROUND: Color32 = Color32::from_rgb(11, 12, 16);
    pub(super) const PANEL: Color32 = Color32::from_rgb(16, 17, 22);
    pub(super) const SURFACE: Color32 = kit::SURFACE;
    pub(super) const RAISED: Color32 = Color32::from_rgb(31, 33, 41);
    pub(super) const BORDER: Color32 = Color32::from_rgb(38, 40, 50);
    pub(super) const PREVIEW: Color32 = Color32::from_rgb(5, 6, 9);
    pub(super) const TEXT: Color32 = kit::TEXT;
    pub(super) const MUTED: Color32 = Color32::from_rgb(162, 167, 180);
    pub(super) const FAINT: Color32 = Color32::from_rgb(112, 117, 130);
    pub(super) const ACCENT: Color32 = kit::ACCENT;
    pub(super) const ACCENT_FILL: Color32 = Color32::from_rgb(66, 116, 222);
    pub(super) const LIVE: Color32 = kit::LOOP;
    pub(super) const WARN: Color32 = kit::STOP;
    pub(super) const DANGER: Color32 = kit::DANGER;
}

// ---------------------------------------------------------------------------
// Presentation semantics shared by Presenter View and the audience dock.
// ---------------------------------------------------------------------------

/// Where the presentation currently rests, in speaker terms: a slide
/// (segment) and, optionally, the last step (stop) it reached.
#[derive(Debug, Clone)]
struct SlideView {
    index: usize,
    segment: SegmentMetadata,
    stop_index: Option<usize>,
}

impl SlideView {
    fn at(timeline: &Timeline, time: f64) -> Option<Self> {
        let position = timeline.segment_position_at(time)?;
        let index = timeline
            .segments
            .iter()
            .position(|segment| segment.id == position.segment_id)?;
        Some(Self {
            index,
            segment: timeline.segments[index].clone(),
            stop_index: position.stop_index,
        })
    }

    fn thumbnail_key(&self) -> ThumbnailKey {
        (
            self.segment.id,
            self.stop_index
                .map(|index| ThumbnailMoment::Stop(index as u32))
                .unwrap_or(ThumbnailMoment::Entry),
        )
    }
}

fn authored_step_name(segment: &SegmentMetadata, index: usize) -> Option<&str> {
    segment
        .stops
        .get(index)
        .and_then(|stop| stop.name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn step_name(segment: &SegmentMetadata, index: usize) -> String {
    authored_step_name(segment, index)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Step {}", index + 1))
}

/// One line describing where the slide is, e.g. `Step 2 of 3 · reveal`.
fn step_caption(segment: &SegmentMetadata, stop_index: Option<usize>) -> String {
    let count = segment.stops.len();
    match stop_index {
        Some(index) => {
            let progress = format!("Step {} of {}", index + 1, count);
            match authored_step_name(segment, index) {
                Some(name) => format!("{progress} · {name}"),
                None => progress,
            }
        }
        None if count == 0 => "No steps · plays straight through".to_string(),
        None if count == 1 => "Slide start · 1 step".to_string(),
        None => format!("Slide start · {count} steps"),
    }
}

/// The next resting point, described relative to the current slide.
#[derive(Debug, Clone, PartialEq)]
struct NextCue {
    title: String,
    detail: String,
    key: ThumbnailKey,
}

fn next_cue(timeline: &Timeline, time: f64, current_segment: Option<u32>) -> Option<NextCue> {
    let target = timeline.next_stop(time)?;
    let next = SlideView::at(timeline, target)?;
    let Some(index) = next.stop_index else {
        return Some(NextCue {
            title: next.segment.name.clone(),
            detail: format!("Slide {} of {}", next.index + 1, timeline.segments.len()),
            key: next.thumbnail_key(),
        });
    };
    let (title, detail) = if current_segment == Some(next.segment.id) {
        (
            step_name(&next.segment, index),
            format!(
                "Same slide · step {} of {}",
                index + 1,
                next.segment.stops.len()
            ),
        )
    } else {
        (
            next.segment.name.clone(),
            format!(
                "Slide {} of {} · {}",
                next.index + 1,
                timeline.segments.len(),
                step_caption(&next.segment, Some(index))
            ),
        )
    };
    Some(NextCue {
        title,
        detail,
        key: next.thumbnail_key(),
    })
}

/// Time that shows a slide's own initial state.
///
/// When the previous slide ends on a terminal stop, the shared boundary
/// belongs to that stop, so seeking to it would keep the previous slide on
/// screen. Direct navigation then lands just inside this slide, matching the
/// slide's entry preview.
fn segment_entry_time(timeline: &Timeline, segment: &SegmentMetadata) -> f64 {
    let owns_start = timeline
        .segment_position_at(segment.start_time)
        .is_some_and(|position| position.segment_id == segment.id);
    if owns_start {
        segment.start_time
    } else {
        entry_segment_time(segment.start_time, segment.end_time)
    }
}

fn segment_matches(segment: &SegmentMetadata, query: &str) -> bool {
    query.is_empty()
        || segment.name.to_lowercase().contains(query)
        || segment
            .notes
            .as_deref()
            .is_some_and(|notes| notes.to_lowercase().contains(query))
        || segment.stops.iter().any(|stop| {
            stop.name
                .as_deref()
                .is_some_and(|name| name.to_lowercase().contains(query))
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackStatus {
    Playing,
    Paused,
    Finished,
}

impl PlaybackStatus {
    fn of(timeline: &Timeline) -> Self {
        if timeline.is_playing {
            Self::Playing
        } else if timeline.cached_duration > 0.0
            && timeline.current_time >= timeline.cached_duration - 1e-6
        {
            Self::Finished
        } else {
            Self::Paused
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Playing => "PLAYING",
            Self::Paused => "PAUSED",
            Self::Finished => "END",
        }
    }

    fn color(self) -> egui::Color32 {
        match self {
            Self::Playing => palette::LIVE,
            Self::Paused => palette::WARN,
            Self::Finished => palette::MUTED,
        }
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

fn apply_presenter_style(ctx: &egui::Context) {
    use egui::{Color32, FontFamily, FontId, TextStyle};

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(12.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.window_margin = egui::Margin::same(18);
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = palette::BACKGROUND;
    style.visuals.window_fill = palette::PANEL;
    style.visuals.window_stroke = egui::Stroke::new(1.0, palette::BORDER);
    style.visuals.window_corner_radius = egui::CornerRadius::same(12);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(12);
    style.visuals.extreme_bg_color = Color32::from_rgb(8, 9, 12);
    style.visuals.faint_bg_color = palette::SURFACE;
    style.visuals.widgets.noninteractive.fg_stroke.color = palette::TEXT;
    style.visuals.widgets.noninteractive.bg_stroke.color = palette::BORDER;
    style.visuals.widgets.inactive.fg_stroke.color = palette::TEXT;
    style.visuals.widgets.inactive.bg_fill = palette::RAISED;
    style.visuals.widgets.inactive.weak_bg_fill = palette::RAISED;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 45, 56);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(42, 45, 56);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.active.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(52, 56, 70);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(52, 56, 70);
    style.visuals.selection.bg_fill = palette::ACCENT_FILL;
    style.visuals.selection.stroke.color = Color32::WHITE;
    style.visuals.hyperlink_color = palette::ACCENT;
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(28.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(17.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(16.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(16.0, FontFamily::Monospace),
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
                        .unwrap_or_else(|| timeline.playback_end()),
                );
                timeline.is_playing = false;
            } else {
                timeline.is_playing = true;
            }
        }
        PresentationAction::Previous => {
            timeline.is_playing = false;
            timeline.seek_request = Some(
                timeline
                    .previous_stop(timeline.current_time)
                    .unwrap_or_else(|| timeline.playback_start()),
            );
        }
        PresentationAction::TogglePlayback => timeline.is_playing = !timeline.is_playing,
        PresentationAction::Home => {
            timeline.is_playing = false;
            timeline.seek_request = Some(timeline.playback_start());
        }
        PresentationAction::End => {
            timeline.is_playing = false;
            timeline.seek_request = Some(timeline.playback_end());
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
        if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::Enter) {
            actions.push(PresentationAction::Advance);
        }
        // Space plays or pauses; it never skips ahead to the next step.
        if keys.just_pressed(KeyCode::Space) {
            actions.push(PresentationAction::TogglePlayback);
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

// ---------------------------------------------------------------------------
// Shared widgets.
// ---------------------------------------------------------------------------

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(caption(text));
}

fn status_pill(ui: &mut egui::Ui, status: PlaybackStatus) {
    let color = status.color();
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, color);
                ui.label(
                    egui::RichText::new(status.label())
                        .strong()
                        .size(11.5)
                        .extra_letter_spacing(1.0)
                        .color(color),
                );
            });
        });
}

/// A labelled monospace value for a right-to-left row, read as `LABEL 00:00`.
fn readout_rtl(ui: &mut egui::Ui, label: &str, value: &str, color: egui::Color32) {
    ui.label(
        egui::RichText::new(value)
            .monospace()
            .size(22.0)
            .color(color),
    );
    ui.label(caption(label));
}

fn paint_badge(
    painter: &egui::Painter,
    anchor: egui::Pos2,
    align_right: bool,
    text: &str,
    fill: egui::Color32,
    color: egui::Color32,
    dot: Option<egui::Color32>,
) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.0), color);
    let dot_w = if dot.is_some() { 14.0 } else { 0.0 };
    let size = galley.size() + egui::vec2(20.0 + dot_w, 10.0);
    let min = if align_right {
        egui::pos2(anchor.x - size.x, anchor.y)
    } else {
        anchor
    };
    let rect = egui::Rect::from_min_size(min, size);
    painter.rect_filled(rect, size.y / 2.0, fill);
    if let Some(dot) = dot {
        painter.circle_filled(egui::pos2(rect.min.x + 14.0, rect.center().y), 3.5, dot);
    }
    painter.galley(rect.min + egui::vec2(10.0 + dot_w, 5.0), galley, color);
}

/// Slide-by-slide progress: one block per slide, the current block filled up
/// to the playhead and step ticks inside each block. Returns a seek target
/// when an interactive bar is clicked.
fn paint_slide_progress(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    current_time: f64,
    height: f32,
    interactive: bool,
) -> Option<f64> {
    let sense = if interactive {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let width = ui.available_width().max(40.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    let painter = ui.painter_at(rect.expand(2.0));
    let radius = height * 0.5;
    let track = egui::Color32::from_white_alpha(24);
    let segments = &timeline.segments;
    if segments.is_empty() {
        let total = timeline.cached_duration;
        let fraction = if total > 0.0 {
            (current_time / total).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        painter.rect_filled(rect, radius, track);
        let mut filled = rect;
        filled.set_width(rect.width() * fraction);
        painter.rect_filled(filled, radius, palette::ACCENT);
        return None;
    }

    let count = segments.len();
    let gap = if count > 60 { 1.0 } else { 2.0 };
    let block = ((rect.width() - gap * (count - 1) as f32) / count as f32).max(1.0);
    let current_index = SlideView::at(timeline, current_time).map(|slide| slide.index);
    let hover_x = response.hover_pos().map(|pos| pos.x);
    let mut hovered = None;
    for (index, segment) in segments.iter().enumerate() {
        let left = rect.left() + index as f32 * (block + gap);
        let block_rect =
            egui::Rect::from_min_size(egui::pos2(left, rect.top()), egui::vec2(block, height));
        let span = segment.end_time - segment.start_time;
        let fraction_of = |time: f64| {
            if span > 1e-9 {
                ((time - segment.start_time) / span).clamp(0.0, 1.0) as f32
            } else {
                1.0
            }
        };
        let is_hovered =
            hover_x.is_some_and(|x| x >= left - gap * 0.5 && x < left + block + gap * 0.5);
        let (fill, color) = match current_index {
            Some(current) if index < current => (1.0, palette::ACCENT.gamma_multiply(0.45)),
            Some(current) if index == current => (fraction_of(current_time), palette::ACCENT),
            _ => (0.0, palette::ACCENT),
        };
        let base = if is_hovered && interactive {
            egui::Color32::from_white_alpha(44)
        } else {
            track
        };
        painter.rect_filled(block_rect, radius.min(block / 2.0), base);
        if fill > 0.0 {
            let clip = egui::Rect::from_min_max(
                block_rect.min,
                egui::pos2(block_rect.left() + block * fill, block_rect.bottom()),
            );
            painter
                .with_clip_rect(clip.intersect(painter.clip_rect()))
                .rect_filled(block_rect, radius.min(block / 2.0), color);
        }
        if block >= 8.0 {
            for stop in &segment.stops {
                let fraction = fraction_of(stop.time);
                if fraction < 0.98 {
                    let x = block_rect.left() + block * fraction;
                    painter.line_segment(
                        [
                            egui::pos2(x, block_rect.top()),
                            egui::pos2(x, block_rect.bottom()),
                        ],
                        egui::Stroke::new(1.5, palette::PANEL),
                    );
                }
            }
        }
        if is_hovered {
            hovered = Some(index);
        }
    }

    let hovered = hovered?;
    let segment = &segments[hovered];
    let hint = if interactive {
        format!(
            "Slide {} · {}\nClick to jump here",
            hovered + 1,
            segment.name
        )
    } else {
        format!("Slide {} · {}", hovered + 1, segment.name)
    };
    let response = response.on_hover_text_at_pointer(hint);
    (interactive && response.clicked()).then(|| segment_entry_time(timeline, segment))
}

#[derive(Default, Clone)]
struct CuePreview {
    texture: Option<egui::TextureHandle>,
    stale: bool,
}

impl CuePreview {
    fn lookup(cache: &PresenterThumbnailCache, key: ThumbnailKey, revision: u64) -> Self {
        cache
            .texture(key, revision)
            .map(|(texture, stale)| Self {
                texture: Some(texture),
                stale,
            })
            .unwrap_or_default()
    }

    fn aspect(&self) -> f32 {
        self.texture
            .as_ref()
            .map(|texture| {
                let size = texture.size_vec2();
                size.x / size.y.max(1.0)
            })
            .unwrap_or(16.0 / 9.0)
    }
}

#[derive(Default, Clone, Copy)]
struct PreviewOverlay {
    blank: AudienceBlank,
    playing: bool,
    badge: Option<&'static str>,
}

fn show_preview(
    ui: &mut egui::Ui,
    preview: &CuePreview,
    size: egui::Vec2,
    empty_message: &str,
    overlay: PreviewOverlay,
    sense: egui::Sense,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size.max(egui::vec2(1.0, 1.0)), sense);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 12.0, palette::PREVIEW);
    let image_rect = match &preview.texture {
        Some(texture) => {
            let texture_size = texture.size_vec2();
            let scale = (rect.width() / texture_size.x).min(rect.height() / texture_size.y);
            let image_rect = egui::Rect::from_center_size(rect.center(), texture_size * scale);
            egui::Image::new(texture)
                .corner_radius(8.0)
                .paint_at(ui, image_rect);
            image_rect
        }
        None => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                empty_message,
                egui::FontId::proportional(15.0),
                palette::FAINT,
            );
            rect
        }
    };
    let hover_stroke = if response.hovered() && sense.senses_click() {
        egui::Stroke::new(1.5, palette::ACCENT)
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(16))
    };
    painter.rect_stroke(rect, 12.0, hover_stroke, egui::StrokeKind::Inside);

    let badge_fill = egui::Color32::from_rgba_premultiplied(6, 7, 10, 225);
    if overlay.playing {
        paint_badge(
            &painter,
            rect.left_top() + egui::vec2(12.0, 12.0),
            false,
            "Playing to the next step",
            badge_fill,
            palette::TEXT,
            Some(palette::LIVE),
        );
    }
    if let Some(badge) = overlay.badge {
        paint_badge(
            &painter,
            rect.right_top() + egui::vec2(-12.0, 12.0),
            true,
            badge,
            palette::ACCENT_FILL,
            egui::Color32::WHITE,
            None,
        );
    } else if preview.stale {
        paint_badge(
            &painter,
            rect.right_top() + egui::vec2(-12.0, 12.0),
            true,
            "Updating preview…",
            badge_fill,
            palette::MUTED,
            None,
        );
    }

    let blank = match overlay.blank {
        AudienceBlank::Black => Some((
            egui::Color32::from_black_alpha(238),
            egui::Color32::WHITE,
            "Audience screen is black",
            "Press B to show the slide again",
        )),
        AudienceBlank::White => Some((
            egui::Color32::from_white_alpha(238),
            egui::Color32::BLACK,
            "Audience screen is white",
            "Press W to show the slide again",
        )),
        AudienceBlank::None => None,
    };
    if let Some((fill, color, title, hint)) = blank {
        painter.rect_filled(image_rect, 8.0, fill);
        painter.text(
            image_rect.center() - egui::vec2(0.0, 12.0),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(22.0),
            color,
        );
        painter.text(
            image_rect.center() + egui::vec2(0.0, 16.0),
            egui::Align2::CENTER_CENTER,
            hint,
            egui::FontId::proportional(14.0),
            color.gamma_multiply(0.75),
        );
    }
    response
}

const SHORTCUTS: &[(&str, &str)] = &[
    ("→  Enter", "Advance to the next step"),
    ("Space", "Play / pause (never skips a step)"),
    ("←  Backspace", "Back to the previous step"),
    ("Home  End", "First / last step"),
    ("O", "Open or close the overview"),
    ("B  W", "Black or white audience screen"),
    ("Click", "Advance (on the audience screen)"),
    ("P", "Reopen Presenter View"),
    ("Esc", "Close overview, clear blank, then exit"),
];

fn show_shortcuts(ui: &mut egui::Ui) {
    ui.set_min_width(380.0);
    section_label(ui, "KEYBOARD SHORTCUTS");
    ui.add_space(6.0);
    egui::Grid::new("presenter-shortcuts")
        .num_columns(2)
        .spacing([18.0, 8.0])
        .show(ui, |ui| {
            for (keys, action) in SHORTCUTS {
                egui::Frame::new()
                    .fill(egui::Color32::from_white_alpha(12))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(*keys)
                                .monospace()
                                .size(13.0)
                                .color(palette::TEXT),
                        );
                    });
                ui.label(
                    egui::RichText::new(*action)
                        .size(14.0)
                        .color(palette::MUTED),
                );
                ui.end_row();
            }
        });
}

// ---------------------------------------------------------------------------
// Audience dock.
// ---------------------------------------------------------------------------

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
    let slide = SlideView::at(&timeline, current);
    let status = PlaybackStatus::of(&timeline);
    let title = slide
        .as_ref()
        .map(|slide| slide.segment.name.clone())
        .unwrap_or_else(|| "Presentation".to_string());
    let position = slide
        .as_ref()
        .map(|slide| format!("{} / {}", slide.index + 1, timeline.segments.len()));
    let time = format!(
        "{} / {}",
        format_timeline_time(current),
        format_timeline_time(total)
    );
    let mut actions = Vec::new();

    let response = egui::Area::new("audience-playback-controls".into())
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(kit_palette::PANEL)
                .corner_radius(14.0)
                .inner_margin(egui::Margin::symmetric(16, 12))
                .stroke(egui::Stroke::new(1.0, kit_palette::PANEL_STROKE))
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 28,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(110),
                })
                .show(ui, |ui| {
                    // Must match `cursor_in_audience_dock_zone`.
                    let width = (ctx.viewport_rect().width() - 48.0).clamp(280.0, 920.0);
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 10.0);
                    egui::Sides::new()
                        .height(PRIMARY_SIZE)
                        .spacing(12.0)
                        .shrink_left()
                        .truncate()
                        .show(
                            ui,
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if icon_button(ui, Icon::SkipStart, ButtonTone::Ghost, true)
                                    .on_hover_text("First step · Home")
                                    .clicked()
                                {
                                    actions.push(PresentationAction::Home);
                                }
                                if icon_button(ui, Icon::PrevScene, ButtonTone::Ghost, true)
                                    .on_hover_text("Previous step · ← or Backspace")
                                    .clicked()
                                {
                                    actions.push(PresentationAction::Previous);
                                }
                                ui.add_space(2.0);
                                let play_icon = if status == PlaybackStatus::Playing {
                                    Icon::Pause
                                } else {
                                    Icon::Play
                                };
                                if icon_button(ui, play_icon, ButtonTone::Primary, true)
                                    .on_hover_text("Play / pause · Space")
                                    .clicked()
                                {
                                    actions.push(PresentationAction::TogglePlayback);
                                }
                                ui.add_space(2.0);
                                if icon_button(ui, Icon::NextScene, ButtonTone::Ghost, true)
                                    .on_hover_text("Next step · → or Enter")
                                    .clicked()
                                {
                                    actions.push(PresentationAction::Advance);
                                }
                                if icon_button(ui, Icon::SkipEnd, ButtonTone::Ghost, true)
                                    .on_hover_text("Last step · End")
                                    .clicked()
                                {
                                    actions.push(PresentationAction::End);
                                }
                                ui.add_space(10.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&title)
                                            .size(14.0)
                                            .color(kit_palette::TEXT),
                                    )
                                    .selectable(false)
                                    .truncate(),
                                );
                            },
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                ui.label(
                                    egui::RichText::new(&time)
                                        .monospace()
                                        .size(12.5)
                                        .color(kit_palette::TEXT_FAINT),
                                );
                                if let Some(position) = &position {
                                    ui.label(
                                        egui::RichText::new(position)
                                            .monospace()
                                            .size(12.5)
                                            .color(kit_palette::TEXT_MUTED),
                                    );
                                }
                            },
                        );
                    paint_slide_progress(ui, &timeline, current, 5.0, false);
                });
        });
    audience_controls.pointer_over = response.response.contains_pointer();

    for action in actions {
        apply_presentation_action(action, &mut timeline, &mut audience_blank, &mut overview);
    }
}

// ---------------------------------------------------------------------------
// Presenter View.
// ---------------------------------------------------------------------------

/// Everything the cockpit shows for one frame, derived before any widgets run
/// so panels never disagree about the current position.
struct PresenterFrame {
    status: PlaybackStatus,
    slide: Option<SlideView>,
    next: Option<NextCue>,
    slide_count: usize,
    current_preview: CuePreview,
    next_preview: CuePreview,
    blank: AudienceBlank,
    preview_status: PreviewStatus,
    omits_native_3d: bool,
    overview_open: bool,
    elapsed: String,
    clock: String,
    current_time: f64,
    total_time: f64,
}

impl PresenterFrame {
    fn preview_message(&self) -> &'static str {
        match self.preview_status {
            PreviewStatus::Failed(_) => "Preview unavailable",
            _ => "Rendering preview…",
        }
    }
}

fn show_header(
    ui: &mut egui::Ui,
    frame: &PresenterFrame,
    timeline: &Timeline,
    timer: &mut PresentationTimer,
    compact: bool,
) -> Option<f64> {
    let slide_position = |ui: &mut egui::Ui| {
        status_pill(ui, frame.status);
        match &frame.slide {
            Some(slide) => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "Slide {} of {}",
                            slide.index + 1,
                            frame.slide_count
                        ))
                        .strong()
                        .size(19.0)
                        .color(palette::TEXT),
                    )
                    .extend(),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new("No slides")
                        .size(17.0)
                        .color(palette::MUTED),
                );
            }
        }
    };
    // Right-to-left: the clock sits at the far right, elapsed time before it.
    let timers = |ui: &mut egui::Ui, timer: &mut PresentationTimer| {
        ui.spacing_mut().item_spacing.x = 6.0;
        readout_rtl(ui, "CLOCK", &frame.clock, palette::TEXT);
        ui.add_space(14.0);
        if icon_button(ui, Icon::Loop, ButtonTone::Ghost, true)
            .on_hover_text("Restart the session timer")
            .clicked()
        {
            timer.reset();
        }
        readout_rtl(ui, "ELAPSED", &frame.elapsed, palette::ACCENT);
    };

    if compact {
        ui.horizontal(slide_position);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                timers(ui, timer)
            });
        });
    } else {
        ui.horizontal(|ui| {
            slide_position(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                timers(ui, timer)
            });
        });
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let time = format!(
            "{} / {}",
            format_timeline_time(frame.current_time),
            format_timeline_time(frame.total_time)
        );
        let bar_width = (ui.available_width() - 110.0).max(60.0);
        let seek = ui
            .allocate_ui_with_layout(
                egui::vec2(bar_width, 12.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| paint_slide_progress(ui, timeline, frame.current_time, 8.0, true),
            )
            .inner;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(time)
                    .monospace()
                    .size(13.0)
                    .color(palette::FAINT),
            );
        });
        seek
    })
    .inner
}

/// Height reserved below the current preview for the step strip.
const STEP_STRIP_HEIGHT: f32 = 58.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepChipState {
    Current,
    Passed,
    Upcoming,
}

fn step_chip(ui: &mut egui::Ui, label: &str, state: StepChipState) -> egui::Response {
    let font = egui::FontId::proportional(14.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), palette::TEXT);
    let check_w = if state == StepChipState::Passed {
        20.0
    } else {
        0.0
    };
    let size = egui::vec2(galley.size().x + check_w + 28.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter();
    let hovered = response.hovered();
    let (fill, color) = match state {
        StepChipState::Current => (palette::ACCENT.gamma_multiply(0.24), palette::ACCENT),
        StepChipState::Passed => (
            egui::Color32::from_white_alpha(if hovered { 22 } else { 8 }),
            palette::MUTED,
        ),
        StepChipState::Upcoming => (
            egui::Color32::from_white_alpha(if hovered { 30 } else { 16 }),
            palette::TEXT,
        ),
    };
    painter.rect_filled(rect, 16.0, fill);
    let mut x = rect.min.x + 14.0;
    if state == StepChipState::Passed {
        paint_icon(
            painter,
            egui::Rect::from_center_size(
                egui::pos2(x + 7.0, rect.center().y),
                egui::vec2(13.0, 13.0),
            ),
            Icon::Check,
            palette::LIVE,
        );
        x += check_w;
    }
    painter.text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
    response
}

fn show_step_strip(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    slide: &SlideView,
    requested_seek: &mut Option<f64>,
) {
    let segment = &slide.segment;
    egui::ScrollArea::horizontal()
        .id_salt("presenter-steps")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let start_state = if slide.stop_index.is_none() {
                    StepChipState::Current
                } else {
                    StepChipState::Passed
                };
                if step_chip(ui, "Slide start", start_state)
                    .on_hover_text("Jump to the beginning of this slide")
                    .clicked()
                {
                    *requested_seek = Some(segment_entry_time(timeline, segment));
                }
                for (index, stop) in segment.stops.iter().enumerate() {
                    let state = match slide.stop_index {
                        Some(current) if index == current => StepChipState::Current,
                        Some(current) if index < current => StepChipState::Passed,
                        _ => StepChipState::Upcoming,
                    };
                    let name = step_name(segment, index);
                    let label = match authored_step_name(segment, index) {
                        Some(name) => {
                            format!("{}  {}", index + 1, truncate_with_ellipsis(name, 28))
                        }
                        None => name.clone(),
                    };
                    if step_chip(ui, &label, state)
                        .on_hover_text(format!("Jump to step {} · {name}", index + 1))
                        .clicked()
                    {
                        *requested_seek = Some(stop.time);
                    }
                }
            });
        });
}

fn show_now_panel(
    ui: &mut egui::Ui,
    frame: &PresenterFrame,
    slide: &SlideView,
    timeline: &Timeline,
    preview_height: Option<f32>,
    requested_seek: &mut Option<f64>,
) {
    section_label(ui, "NOW ON SCREEN");
    ui.add(
        egui::Label::new(
            egui::RichText::new(&slide.segment.name)
                .strong()
                .size(28.0)
                .color(palette::TEXT),
        )
        .truncate(),
    );
    ui.label(
        egui::RichText::new(step_caption(&slide.segment, slide.stop_index))
            .size(17.0)
            .color(if slide.stop_index.is_some() {
                palette::ACCENT
            } else {
                palette::MUTED
            }),
    );
    ui.add_space(6.0);
    let height = preview_height
        .unwrap_or_else(|| ui.available_height() - STEP_STRIP_HEIGHT)
        .max(140.0);
    show_preview(
        ui,
        &frame.current_preview,
        egui::vec2(ui.available_width(), height),
        frame.preview_message(),
        PreviewOverlay {
            blank: frame.blank,
            playing: frame.status == PlaybackStatus::Playing,
            badge: None,
        },
        egui::Sense::hover(),
    );
    ui.add_space(10.0);
    show_step_strip(ui, timeline, slide, requested_seek);
}

fn show_speaker_column(
    ui: &mut egui::Ui,
    frame: &PresenterFrame,
    preferences: &mut PresenterPreferences,
    notes_min_height: f32,
) {
    section_label(ui, "UP NEXT");
    let (title, detail) = match &frame.next {
        Some(next) => (next.title.as_str(), next.detail.as_str()),
        None => ("End of presentation", "Nothing left to advance"),
    };
    ui.add(
        egui::Label::new(
            egui::RichText::new(title)
                .strong()
                .size(20.0)
                .color(palette::TEXT),
        )
        .truncate(),
    );
    ui.add(
        egui::Label::new(egui::RichText::new(detail).size(14.0).color(palette::MUTED)).truncate(),
    );
    ui.add_space(4.0);
    let width = ui.available_width();
    let height = (width / frame.next_preview.aspect()).min((ui.available_height() * 0.4).max(90.0));
    let empty = if frame.next.is_some() {
        frame.preview_message()
    } else {
        "Presentation complete"
    };
    show_preview(
        ui,
        &frame.next_preview,
        egui::vec2(width, height),
        empty,
        PreviewOverlay::default(),
        egui::Sense::hover(),
    );

    ui.add_space(14.0);
    ui.horizontal(|ui| {
        section_label(ui, "SPEAKER NOTES");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let size = preferences.notes_size;
            if small_button(ui, "A+", size < NOTES_SIZE_MAX)
                .on_hover_text("Larger notes")
                .clicked()
            {
                preferences.adjust_notes_size(2.0);
            }
            if small_button(ui, "A−", size > NOTES_SIZE_MIN)
                .on_hover_text("Smaller notes")
                .clicked()
            {
                preferences.adjust_notes_size(-2.0);
            }
        });
    });
    let notes = frame
        .slide
        .as_ref()
        .and_then(|slide| slide.segment.notes.as_deref())
        .map(str::trim)
        .filter(|notes| !notes.is_empty());
    egui::Frame::new()
        .fill(palette::SURFACE)
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            let height = ui.available_height().max(notes_min_height);
            egui::ScrollArea::vertical()
                .id_salt("presenter-notes")
                .auto_shrink([false, false])
                .max_height(height)
                .show(ui, |ui| match notes {
                    Some(notes) => {
                        let size = preferences.notes_size;
                        ui.label(
                            egui::RichText::new(notes)
                                .size(size)
                                .color(palette::TEXT)
                                .line_height(Some(size * 1.4)),
                        );
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("No speaker notes for this slide.")
                                .size(16.0)
                                .italics()
                                .color(palette::FAINT),
                        );
                    }
                });
        });
}

/// Icon plus short text, for status readouts inside a right-to-left row.
fn inline_status_rtl(
    ui: &mut egui::Ui,
    icon: Icon,
    text: &str,
    color: egui::Color32,
) -> egui::Response {
    let label = ui.label(egui::RichText::new(text).size(13.0).color(color));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    paint_icon(ui.painter(), rect, icon, color);
    label
}

fn show_preview_status(ui: &mut egui::Ui, frame: &PresenterFrame, retry: &mut bool) {
    match &frame.preview_status {
        PreviewStatus::Rendering { done, total } => {
            ui.label(
                egui::RichText::new(format!("Rendering previews {done}/{total}"))
                    .size(13.0)
                    .color(palette::MUTED),
            );
            ui.spinner();
        }
        PreviewStatus::Failed(error) => {
            if small_button(ui, "Retry", true)
                .on_hover_text("Render the cue previews again")
                .clicked()
            {
                *retry = true;
            }
            inline_status_rtl(ui, Icon::Warning, "Previews failed", palette::DANGER)
                .on_hover_text(error.as_str());
        }
        PreviewStatus::Waiting if frame.slide.is_some() => {
            ui.spinner();
        }
        PreviewStatus::Waiting | PreviewStatus::Ready => {}
    }
    if frame.omits_native_3d {
        inline_status_rtl(ui, Icon::Warning, "3D not in previews", palette::WARN).on_hover_text(
            "Cue previews draw the 2D layers only. Native 3D objects still appear on the audience screen.",
        );
    }
}

/// Presenter View dock button sizes: large targets for a live talk.
const DOCK_BUTTON: f32 = 40.0;
const DOCK_PRIMARY: f32 = 48.0;

fn dock_button(ui: &mut egui::Ui, icon: Icon, hint: &str) -> bool {
    icon_button_sized(ui, icon, ButtonTone::Ghost, true, DOCK_BUTTON)
        .on_hover_text(hint)
        .clicked()
}

fn show_dock(
    ui: &mut egui::Ui,
    frame: &PresenterFrame,
    compact: bool,
    actions: &mut Vec<PresentationAction>,
    retry: &mut bool,
) {
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if dock_button(ui, Icon::SkipStart, "First step · Home") {
            actions.push(PresentationAction::Home);
        }
        if dock_button(ui, Icon::PrevScene, "Previous step · ← or Backspace") {
            actions.push(PresentationAction::Previous);
        }
        let play_icon = if frame.status == PlaybackStatus::Playing {
            Icon::Pause
        } else {
            Icon::Play
        };
        ui.add_space(4.0);
        if icon_button_sized(ui, play_icon, ButtonTone::Primary, true, DOCK_PRIMARY)
            .on_hover_text("Play / pause · Space")
            .clicked()
        {
            actions.push(PresentationAction::TogglePlayback);
        }
        ui.add_space(4.0);
        if dock_button(ui, Icon::NextScene, "Next step · → or Enter") {
            actions.push(PresentationAction::Advance);
        }
        if dock_button(ui, Icon::SkipEnd, "Last step · End") {
            actions.push(PresentationAction::End);
        }

        ui.add_space(8.0);
        divider(ui);
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.x = 8.0;
        let label = |text: &'static str| (!compact).then_some(text);
        if pill_toggle(
            ui,
            Icon::Grid,
            label("Overview"),
            frame.overview_open,
            palette::ACCENT,
            36.0,
        )
        .on_hover_text("Find and jump to any slide · O")
        .clicked()
        {
            actions.push(PresentationAction::ToggleOverview);
        }
        if pill_toggle(
            ui,
            Icon::BlackScreen,
            label("Black"),
            frame.blank == AudienceBlank::Black,
            palette::WARN,
            36.0,
        )
        .on_hover_text("Black out the audience screen · B")
        .clicked()
        {
            actions.push(PresentationAction::ToggleBlack);
        }
        if pill_toggle(
            ui,
            Icon::WhiteScreen,
            label("White"),
            frame.blank == AudienceBlank::White,
            palette::WARN,
            36.0,
        )
        .on_hover_text("White out the audience screen · W")
        .clicked()
        {
            actions.push(PresentationAction::ToggleWhite);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let keyboard =
                icon_button_sized(ui, Icon::Keyboard, ButtonTone::Ghost, true, DOCK_BUTTON)
                    .on_hover_text("Keyboard shortcuts");
            egui::Popup::menu(&keyboard).show(show_shortcuts);
            show_preview_status(ui, frame, retry);
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn show_overview(
    ctx: &egui::Context,
    overview: &mut PresenterOverviewState,
    frame: &PresenterFrame,
    timeline: &Timeline,
    thumbnails: &PresenterThumbnailCache,
    revision: u64,
    actions: &mut Vec<PresentationAction>,
    requested_seek: &mut Option<f64>,
) {
    let rect = ctx.viewport_rect().shrink(12.0);
    let current_segment = frame.slide.as_ref().map(|slide| slide.segment.id);
    egui::Window::new("Presentation overview")
        .title_bar(false)
        .resizable(false)
        .fixed_rect(rect)
        .frame(
            egui::Frame::new()
                .fill(palette::PANEL)
                .stroke(egui::Stroke::new(1.0, palette::BORDER))
                .corner_radius(16.0)
                .inner_margin(egui::Margin::same(20))
                .shadow(egui::Shadow {
                    offset: [0, 12],
                    blur: 40,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(140),
                }),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Overview")
                        .strong()
                        .size(24.0)
                        .color(palette::TEXT),
                );
                if let Some(slide) = &frame.slide {
                    ui.label(
                        egui::RichText::new(format!(
                            "Now on slide {} of {}",
                            slide.index + 1,
                            frame.slide_count
                        ))
                        .color(palette::MUTED),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                        .on_hover_text("Close · Esc")
                        .clicked()
                    {
                        actions.push(PresentationAction::ToggleOverview);
                    }
                });
            });
            ui.add_space(8.0);
            let search = ui.add(
                egui::TextEdit::singleline(&mut overview.query)
                    .hint_text("Search slides, steps or notes · Enter jumps to the first match")
                    .desired_width(f32::INFINITY)
                    .font(egui::FontId::proportional(16.0))
                    .text_color(palette::TEXT)
                    .frame(
                        egui::Frame::new()
                            .fill(egui::Color32::from_white_alpha(10))
                            .corner_radius(10.0)
                            .inner_margin(egui::Margin::symmetric(14, 10))
                            .stroke(egui::Stroke::new(1.0, palette::BORDER)),
                    )
                    .margin(egui::Margin::ZERO),
            );
            if overview.focus_search {
                search.request_focus();
                overview.focus_search = false;
            }
            let query = overview.query.trim().to_lowercase();
            let matches = timeline
                .segments
                .iter()
                .enumerate()
                .filter(|(_, segment)| segment_matches(segment, &query))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if search.lost_focus()
                && ui.input(|input| input.key_pressed(egui::Key::Enter))
                && let Some(&first) = matches.first()
            {
                *requested_seek = Some(segment_entry_time(timeline, &timeline.segments[first]));
            }
            ui.add_space(12.0);
            if matches.is_empty() {
                ui.label(
                    egui::RichText::new("No slides match this search.")
                        .italics()
                        .color(palette::FAINT),
                );
                return;
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let gap = 14.0;
                    let available = ui.available_width();
                    let columns = ((available + gap) / (250.0 + gap)).floor().max(1.0);
                    let card_width = ((available - gap * (columns - 1.0)) / columns).floor();
                    ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                    // Explicit rows: a wrapped layout cannot know a card's
                    // size before placing it, so it would never break lines.
                    for row in matches.chunks(columns as usize) {
                        ui.horizontal_top(|ui| {
                            for &index in row {
                                let segment = &timeline.segments[index];
                                let preview = CuePreview::lookup(
                                    thumbnails,
                                    (segment.id, ThumbnailMoment::Complete),
                                    revision,
                                );
                                show_overview_card(
                                    ui,
                                    timeline,
                                    index,
                                    segment,
                                    current_segment == Some(segment.id),
                                    &preview,
                                    frame.preview_message(),
                                    card_width,
                                    requested_seek,
                                );
                            }
                        });
                    }
                });
        });
}

#[allow(clippy::too_many_arguments)]
fn show_overview_card(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    index: usize,
    segment: &SegmentMetadata,
    is_current: bool,
    preview: &CuePreview,
    preview_message: &str,
    card_width: f32,
    requested_seek: &mut Option<f64>,
) {
    let stroke = if is_current {
        egui::Stroke::new(2.0, palette::ACCENT)
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(12))
    };
    let inner_width = (card_width - 24.0).max(80.0);
    egui::Frame::new()
        .fill(palette::SURFACE)
        .stroke(stroke)
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(inner_width);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                let height = inner_width / preview.aspect();
                let response = show_preview(
                    ui,
                    preview,
                    egui::vec2(inner_width, height),
                    preview_message,
                    PreviewOverlay {
                        badge: is_current.then_some("NOW"),
                        ..default()
                    },
                    egui::Sense::click(),
                );
                if response
                    .on_hover_text("Jump to the start of this slide")
                    .clicked()
                {
                    *requested_seek = Some(segment_entry_time(timeline, segment));
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{:02}", index + 1))
                            .monospace()
                            .size(14.0)
                            .color(if is_current {
                                palette::ACCENT
                            } else {
                                palette::FAINT
                            }),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&segment.name)
                                .strong()
                                .size(15.0)
                                .color(palette::TEXT),
                        )
                        .truncate(),
                    );
                });
                if !segment.stops.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        for (stop_index, stop) in segment.stops.iter().enumerate() {
                            let name = step_name(segment, stop_index);
                            let label = format!(
                                "{}  {}",
                                stop_index + 1,
                                truncate_with_ellipsis(&name, 16)
                            );
                            if chip(ui, &label, false)
                                .on_hover_text(format!("Jump to step {} · {name}", stop_index + 1))
                                .clicked()
                            {
                                *requested_seek = Some(stop.time);
                            }
                        }
                    });
                }
            });
        });
}

/// Focus-first speaker cockpit for live presentations.
///
/// Layout, in reading order: a slim header (status, slide position, clock,
/// elapsed time and a slide-by-slide progress bar), the slide on screen now
/// with its steps, the next resting point and the speaker notes, and a dock
/// with the navigation controls.
#[allow(clippy::too_many_arguments)]
pub(crate) fn presenter_view_system(
    mut contexts: Query<(Entity, &mut EguiContext), With<PresenterCamera>>,
    mut timeline: ResMut<Timeline>,
    mut audience_blank: ResMut<AudienceBlank>,
    replay_stash: Res<StashedReplay>,
    mut thumbnails: ResMut<PresenterThumbnailCache>,
    mut overview: ResMut<PresenterOverviewState>,
    mut presentation_timer: ResMut<PresentationTimer>,
    mut preferences: ResMut<PresenterPreferences>,
) {
    let Ok((camera_entity, mut context)) = contexts.single_mut() else {
        return;
    };
    let ctx = context.get_mut();
    apply_presenter_style(ctx);
    let revision = replay_stash.revision;
    let viewport = ctx.viewport_rect();
    let compact = viewport.width() < 900.0;

    let current_time = timeline.current_time;
    let slide = SlideView::at(&timeline, current_time);
    let next = next_cue(
        &timeline,
        current_time,
        slide.as_ref().map(|slide| slide.segment.id),
    );
    let priority = slide
        .as_ref()
        .map(SlideView::thumbnail_key)
        .into_iter()
        .chain(next.as_ref().map(|next| next.key))
        .collect::<Vec<_>>();
    thumbnails.update(
        &replay_stash,
        &timeline,
        desired_thumbnail_edge(viewport.width(), ctx.pixels_per_point()),
        &priority,
        Instant::now(),
    );
    thumbnails.sync_textures(ctx, camera_entity, &priority);

    let frame = PresenterFrame {
        status: PlaybackStatus::of(&timeline),
        current_preview: slide
            .as_ref()
            .map(|slide| CuePreview::lookup(&thumbnails, slide.thumbnail_key(), revision))
            .unwrap_or_default(),
        next_preview: next
            .as_ref()
            .map(|next| CuePreview::lookup(&thumbnails, next.key, revision))
            .unwrap_or_default(),
        slide,
        next,
        slide_count: timeline.segments.len(),
        blank: *audience_blank,
        preview_status: thumbnails.status(revision),
        omits_native_3d: thumbnails.omits_native_3d(revision),
        overview_open: overview.open,
        elapsed: format_stopwatch(presentation_timer.elapsed()),
        clock: chrono::Local::now().format("%H:%M").to_string(),
        current_time,
        total_time: timeline.cached_duration,
    };
    let mut actions = Vec::new();
    let mut requested_seek = None;
    let mut retry = false;

    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        "presenter-viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(viewport),
    );

    egui::Panel::top("presenter-header")
        .exact_size(if compact { 118.0 } else { 90.0 })
        .frame(
            egui::Frame::new()
                .fill(palette::PANEL)
                .inner_margin(egui::Margin::symmetric(20, 12)),
        )
        .show(&mut viewport_ui, |ui| {
            if let Some(time) = show_header(ui, &frame, &timeline, &mut presentation_timer, compact)
            {
                requested_seek = Some(time);
            }
        });

    egui::Panel::bottom("presenter-dock")
        .exact_size(72.0)
        .frame(
            egui::Frame::new()
                .fill(palette::PANEL)
                .inner_margin(egui::Margin::symmetric(16, 12)),
        )
        .show(&mut viewport_ui, |ui| {
            show_dock(ui, &frame, compact, &mut actions, &mut retry);
        });

    if !compact && frame.slide.is_some() {
        egui::Panel::right("presenter-speaker")
            .exact_size((viewport.width() * 0.36).clamp(320.0, 540.0))
            .frame(
                egui::Frame::new()
                    .fill(palette::BACKGROUND)
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(&mut viewport_ui, |ui| {
                show_speaker_column(ui, &frame, &mut preferences, 120.0);
            });
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(palette::BACKGROUND)
                .inner_margin(egui::Margin::same(20)),
        )
        .show(&mut viewport_ui, |ui| match &frame.slide {
            Some(slide) if compact => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let height = ui.available_width() / frame.current_preview.aspect();
                    show_now_panel(
                        ui,
                        &frame,
                        slide,
                        &timeline,
                        Some(height),
                        &mut requested_seek,
                    );
                    ui.add_space(18.0);
                    show_speaker_column(ui, &frame, &mut preferences, 220.0);
                });
            }
            Some(slide) => {
                show_now_panel(ui, &frame, slide, &timeline, None, &mut requested_seek);
            }
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.35);
                    ui.label(
                        egui::RichText::new("No slides to present")
                            .strong()
                            .size(26.0)
                            .color(palette::TEXT),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Add scene.segment(...) and scene.stop(...) to the script to build slides.",
                        )
                        .color(palette::MUTED),
                    );
                });
            }
        });

    if overview.open {
        show_overview(
            ctx,
            &mut overview,
            &frame,
            &timeline,
            &thumbnails,
            revision,
            &mut actions,
            &mut requested_seek,
        );
    }

    if retry {
        thumbnails.retry();
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
        AudienceControlsState, NOTES_SIZE_MAX, NOTES_SIZE_MIN, PlaybackStatus, PresentationAction,
        PresentationTimer, PresenterCamera, PresenterOverviewState, PresenterPreferences,
        PresenterWindow, SlideView, ThumbnailMoment, apply_presentation_action,
        audience_controls_visible, cleanup_presenter_before_window_close_system,
        cursor_in_audience_dock_zone, format_stopwatch, next_cue, presentation_input_system,
        segment_entry_time, segment_matches, step_caption, sync_presentation_timer_system,
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

    fn stop(name: Option<&str>, time: f64) -> SegmentStop {
        SegmentStop {
            name: name.map(str::to_owned),
            time,
        }
    }

    /// Two slides, each ending on a terminal stop, like `presentation_demo.py`.
    fn deck() -> Timeline {
        let mut timeline = Timeline::new();
        timeline.cached_duration = 4.0;
        timeline.set_segments(vec![
            SegmentMetadata {
                id: 1,
                name: "Reveal in steps".to_string(),
                notes: Some("Advance once per benefit.".to_string()),
                start_time: 0.0,
                end_time: 2.0,
                stops: vec![stop(Some("named-segments"), 1.0), stop(None, 2.0)],
            },
            SegmentMetadata {
                id: 2,
                name: "Thank you".to_string(),
                notes: None,
                start_time: 2.0,
                end_time: 4.0,
                stops: vec![stop(Some("questions"), 4.0)],
            },
        ]);
        timeline
    }

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
            stops: vec![stop(Some("cue"), 1.0)],
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
    fn step_captions_count_only_resting_points() {
        let timeline = deck();
        let segment = &timeline.segments[0];

        assert_eq!(step_caption(segment, None), "Slide start · 2 steps");
        assert_eq!(
            step_caption(segment, Some(0)),
            "Step 1 of 2 · named-segments"
        );
        assert_eq!(step_caption(segment, Some(1)), "Step 2 of 2");
        let mut plain = segment.clone();
        plain.stops.clear();
        assert_eq!(
            step_caption(&plain, None),
            "No steps · plays straight through"
        );
    }

    #[test]
    fn up_next_names_the_step_or_the_following_slide() {
        let timeline = deck();

        let within = next_cue(&timeline, 0.5, Some(1)).unwrap();
        assert_eq!(within.title, "named-segments");
        assert_eq!(within.detail, "Same slide · step 1 of 2");
        assert_eq!(within.key, (1, ThumbnailMoment::Stop(0)));

        let across = next_cue(&timeline, 2.0, Some(1)).unwrap();
        assert_eq!(across.title, "Thank you");
        assert_eq!(across.detail, "Slide 2 of 2 · Step 1 of 1 · questions");
        assert_eq!(across.key, (2, ThumbnailMoment::Stop(0)));

        assert!(next_cue(&timeline, 4.0, Some(2)).is_none());
    }

    #[test]
    fn terminal_stop_keeps_the_finished_slide_on_screen() {
        let timeline = deck();

        let slide = SlideView::at(&timeline, 2.0).unwrap();

        assert_eq!(slide.index, 0);
        assert_eq!(slide.stop_index, Some(1));
        assert_eq!(slide.thumbnail_key(), (1, ThumbnailMoment::Stop(1)));
    }

    #[test]
    fn jumping_to_a_slide_after_a_terminal_stop_shows_that_slide() {
        let timeline = deck();

        let first = segment_entry_time(&timeline, &timeline.segments[0]);
        let second = segment_entry_time(&timeline, &timeline.segments[1]);

        assert_eq!(first, 0.0);
        assert!(second > 2.0 && second < 4.0);
        let landed = SlideView::at(&timeline, second).unwrap();
        assert_eq!(landed.segment.id, 2);
        assert_eq!(landed.stop_index, None);
    }

    #[test]
    fn overview_search_matches_names_steps_and_notes() {
        let timeline = deck();
        let segment = &timeline.segments[0];

        assert!(segment_matches(segment, ""));
        assert!(segment_matches(segment, "reveal"));
        assert!(segment_matches(segment, "named-seg"));
        assert!(segment_matches(segment, "benefit"));
        assert!(!segment_matches(segment, "questions"));
    }

    #[test]
    fn playback_status_distinguishes_pause_from_the_end() {
        let mut timeline = deck();
        assert_eq!(PlaybackStatus::of(&timeline), PlaybackStatus::Paused);
        timeline.is_playing = true;
        assert_eq!(PlaybackStatus::of(&timeline), PlaybackStatus::Playing);
        timeline.is_playing = false;
        timeline.current_time = 4.0;
        assert_eq!(PlaybackStatus::of(&timeline), PlaybackStatus::Finished);
    }

    #[test]
    fn notes_size_stays_readable() {
        let mut preferences = PresenterPreferences::default();
        for _ in 0..40 {
            preferences.adjust_notes_size(2.0);
        }
        assert_eq!(preferences.notes_size, NOTES_SIZE_MAX);
        for _ in 0..40 {
            preferences.adjust_notes_size(-2.0);
        }
        assert_eq!(preferences.notes_size, NOTES_SIZE_MIN);
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

    fn input_app() -> App {
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
        app
    }

    #[test]
    fn focused_audience_window_receives_advance_shortcuts() {
        let mut app = input_app();
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
        let mut app = input_app();
        app.world_mut().remove_resource::<Timeline>();
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
        let mut app = input_app();
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
            .press(KeyCode::Enter);

        app.update();

        assert!(app.world().resource::<Timeline>().is_playing);
    }

    #[test]
    fn space_pauses_the_presentation_without_skipping_to_the_next_step() {
        let mut app = input_app();
        let mut timeline = deck();
        timeline.current_time = 0.4;
        timeline.is_playing = true;
        app.insert_resource(timeline);
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);

        app.update();

        let timeline = app.world().resource::<Timeline>();
        assert!(!timeline.is_playing);
        assert_eq!(
            timeline.seek_request, None,
            "Advance would have jumped to the stop at 1.0"
        );
    }

    #[test]
    fn audience_click_advances_when_the_audience_window_is_focused() {
        let mut app = input_app();
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
        let mut app = input_app();
        app.insert_resource(AudienceControlsState { pointer_over: true });
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
        let mut app = input_app();
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
}
