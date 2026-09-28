//! Touch controls for phones and tablets, in the manner of mobile video
//! players. With a mouse nothing here runs; the first touch turns them on.
//!
//! - A tap on the scene shows or hides the controls; they hide again after a
//!   few seconds.
//! - While they show, large previous / play-pause / next buttons sit in the
//!   middle of the scene above the same playback bar as on the desktop,
//!   which keeps its size: in CSS pixels it matches a phone's.
//! - A double tap on the right or left side goes to the next or previous
//!   stop (or 10 s forward or back without stops); further quick taps keep
//!   going. A horizontal swipe does the same.

use std::collections::HashMap;

use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use gaanim_timeline::timeline::Timeline;

use crate::ui_kit::{Icon, paint_icon, palette};
use crate::{EditorState, PreviewInteractive, ViewportFrame};

/// Seconds the controls stay after a tap.
const REVEAL_SECS: f64 = 3.5;
/// Longest gap between the taps of a double tap.
const DOUBLE_TAP_SECS: f64 = 0.3;
/// Longest press that still counts as a tap.
const TAP_SECS: f64 = 0.5;
/// Farthest a finger may move, in logical pixels, and still tap.
const TAP_SLOP: f32 = 18.0;
/// Shortest horizontal travel of a swipe, in logical pixels.
const SWIPE_MIN: f32 = 60.0;
/// Width of the left and right double-tap zones, as a fraction of the window.
const SIDE_ZONE: f32 = 0.35;
/// Seek of a double tap on a timeline without stops.
const SEEK_STEP: f64 = 10.0;
/// Seconds the double-tap ripple stays.
const FEEDBACK_SECS: f64 = 0.6;
/// Height of the row of center buttons, the play button's diameter.
const CENTER_ROW_HEIGHT: f32 = 64.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Back,
    Forward,
}

struct Press {
    started: f64,
    over_controls: bool,
    multi: bool,
}

#[derive(Clone, Copy)]
struct Tap {
    at: f64,
    side: Option<Direction>,
    revealed_before: bool,
}

/// Resource: gesture state and what the touch overlay draws.
#[derive(Resource, Default)]
pub(crate) struct TouchControls {
    /// A touch has been seen; the touch controls are on from then on.
    pub active: bool,
    now: f64,
    reveal_until: f64,
    presses: HashMap<u64, Press>,
    last_tap: Option<Tap>,
    feedback: Option<(Direction, f64)>,
    /// Center buttons drawn last frame, in egui points.
    center_rect: Option<egui::Rect>,
    /// egui zoom last frame, to map touch positions to points.
    zoom: f32,
}

impl TouchControls {
    fn revealed(&self) -> bool {
        self.now < self.reveal_until
    }

    fn reveal(&mut self) {
        self.reveal_until = self.now + REVEAL_SECS;
    }

    fn over_controls(&self, position: Vec2, bar_rect: Option<egui::Rect>) -> bool {
        let zoom = if self.zoom > 0.0 { self.zoom } else { 1.0 };
        let point = egui::pos2(position.x / zoom, position.y / zoom);
        bar_rect.is_some_and(|rect| rect.expand(8.0).contains(point))
            || (self.revealed()
                && self
                    .center_rect
                    .is_some_and(|rect| rect.expand(8.0).contains(point)))
    }
}

/// Go to the next or previous stop, or seek without stops.
fn step(timeline: &mut Timeline, direction: Direction) {
    let has_stops = timeline
        .segments
        .iter()
        .any(|segment| !segment.stops.is_empty());
    match (has_stops, direction) {
        (true, Direction::Forward) => timeline.advance(),
        (true, Direction::Back) => timeline.go_back(),
        (false, direction) => {
            let offset = if direction == Direction::Forward {
                SEEK_STEP
            } else {
                -SEEK_STEP
            };
            let end = timeline.cached_duration.max(0.0);
            timeline.seek_request = Some((timeline.current_time + offset).clamp(0.0, end));
        }
    }
}

/// System: turns taps, double taps and swipes into playback actions.
pub(crate) fn touch_gesture_system(
    touches: Res<Touches>,
    time: Res<Time<Real>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    interactive: Option<Res<PreviewInteractive>>,
    mut state: ResMut<EditorState>,
    mut touch: ResMut<TouchControls>,
    mut timeline: ResMut<Timeline>,
) {
    let now = time.elapsed_secs_f64();
    touch.now = now;
    // Dragging the camera in interactive inspection owns the touches.
    let inspecting = interactive.is_some_and(|interactive| interactive.enabled);
    let width = windows
        .single()
        .map_or(1.0, |window| window.width().max(1.0));
    let fingers = touches.iter().count();

    for pressed in touches.iter_just_pressed() {
        touch.active = true;
        let over_controls = touch.over_controls(pressed.position(), state.bar_rect);
        touch.presses.insert(
            pressed.id(),
            Press {
                started: now,
                over_controls,
                multi: fingers > 1,
            },
        );
    }
    if fingers > 1 {
        for press in touch.presses.values_mut() {
            press.multi = true;
        }
    }
    // Using the controls keeps them on screen.
    if touch.presses.values().any(|press| press.over_controls) {
        touch.reveal();
    }

    for released in touches.iter_just_released() {
        let Some(press) = touch.presses.remove(&released.id()) else {
            continue;
        };
        if press.over_controls {
            touch.reveal();
            continue;
        }
        if press.multi || inspecting {
            continue;
        }
        let delta = released.position() - released.start_position();
        if delta.x.abs() > SWIPE_MIN && delta.x.abs() > 1.5 * delta.y.abs() {
            let direction = if delta.x < 0.0 {
                Direction::Forward
            } else {
                Direction::Back
            };
            step(&mut timeline, direction);
            touch.feedback = Some((direction, now + FEEDBACK_SECS));
            touch.last_tap = None;
            continue;
        }
        if delta.length() > TAP_SLOP || now - press.started > TAP_SECS {
            continue;
        }
        let x = released.position().x / width;
        let side = if x < SIDE_ZONE {
            Some(Direction::Back)
        } else if x > 1.0 - SIDE_ZONE {
            Some(Direction::Forward)
        } else {
            None
        };
        if let Some(last) = touch.last_tap
            && let Some(direction) = side
            && last.side == side
            && now - last.at < DOUBLE_TAP_SECS
        {
            // The first tap of the pair toggled the controls; undo that.
            step(&mut timeline, direction);
            touch.feedback = Some((direction, now + FEEDBACK_SECS));
            touch.reveal_until = if last.revealed_before {
                now + REVEAL_SECS
            } else {
                0.0
            };
            touch.last_tap = Some(Tap { at: now, ..last });
            continue;
        }
        let revealed = touch.revealed();
        touch.last_tap = Some(Tap {
            at: now,
            side,
            revealed_before: revealed,
        });
        if revealed {
            touch.reveal_until = 0.0;
        } else {
            touch.reveal();
        }
    }
    for canceled in touches.iter_just_canceled() {
        touch.presses.remove(&canceled.id());
    }
    state.touch_reveal = touch.revealed();
}

/// System: draws the center buttons and the double-tap ripple.
pub(crate) fn touch_overlay_system(
    mut contexts: EguiContexts,
    state: Res<EditorState>,
    frame: Option<Res<ViewportFrame>>,
    mut touch: ResMut<TouchControls>,
    mut timeline: ResMut<Timeline>,
) {
    if !touch.active {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    touch.zoom = ctx.zoom_factor();
    let screen = ctx.content_rect();
    let now = touch.now;

    if let Some((direction, until)) = touch.feedback {
        if now < until {
            paint_ripple(
                ctx,
                screen,
                direction,
                ((until - now) / FEEDBACK_SECS) as f32,
            );
            ctx.request_repaint();
        } else {
            touch.feedback = None;
        }
    }

    if !touch.revealed() || timeline.cached_duration <= 0.0 {
        touch.center_rect = None;
        return;
    }
    ctx.request_repaint();
    // Over the scene, as a video player does, but in the part of it above
    // the playback bar, so they never cover the bar on a short screen.
    let free = match state.bar_rect {
        Some(bar) if bar.top() > screen.top() => {
            egui::Rect::from_min_max(screen.min, egui::pos2(screen.right(), bar.top()))
        }
        _ => screen,
    };
    let scene = frame
        .filter(|frame| frame.size.x > 0.0 && frame.size.y > 0.0)
        .map(|frame| {
            let zoom = touch.zoom.max(f32::EPSILON);
            egui::Rect::from_min_size(
                egui::pos2(frame.origin.x as f32, frame.origin.y as f32) / zoom,
                egui::vec2(frame.size.x as f32, frame.size.y as f32) / zoom,
            )
        });
    let region = scene
        .map(|scene| scene.intersect(free))
        .filter(|region| region.height() >= CENTER_ROW_HEIGHT + 16.0)
        .unwrap_or(free);
    let mut action = None;
    let area = egui::Area::new("touch_center_controls".into())
        .anchor(
            egui::Align2::CENTER_CENTER,
            region.center() - screen.center(),
        )
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.x = 28.0;
            ui.horizontal(|ui| {
                if round_button(ui, Icon::PrevScene, 48.0).clicked() {
                    action = Some(CenterAction::Step(Direction::Back));
                }
                let play_icon = if timeline.is_playing {
                    Icon::Pause
                } else {
                    Icon::Play
                };
                if round_button(ui, play_icon, 64.0).clicked() {
                    action = Some(CenterAction::PlayPause);
                }
                if round_button(ui, Icon::NextScene, 48.0).clicked() {
                    action = Some(CenterAction::Step(Direction::Forward));
                }
            });
        });
    touch.center_rect = Some(area.response.rect);
    match action {
        Some(CenterAction::Step(direction)) => {
            step(&mut timeline, direction);
            touch.reveal();
        }
        Some(CenterAction::PlayPause) => {
            if !timeline.is_playing && timeline.current_time >= timeline.playback_end() - 1e-6 {
                timeline.seek_request = Some(timeline.playback_start());
            }
            timeline.is_playing = !timeline.is_playing;
            touch.reveal();
        }
        None => {}
    }
}

enum CenterAction {
    Step(Direction),
    PlayPause,
}

/// A round, translucent button with an icon, like a mobile player's.
fn round_button(ui: &mut egui::Ui, icon: Icon, diameter: f32) -> egui::Response {
    // Every button takes the row's full height, so they line up centered.
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(diameter, CENTER_ROW_HEIGHT),
        egui::Sense::click(),
    );
    let painter = ui.painter();
    let fill = if response.is_pointer_button_down_on() {
        egui::Color32::from_black_alpha(190)
    } else {
        egui::Color32::from_black_alpha(140)
    };
    painter.circle_filled(rect.center(), diameter / 2.0, fill);
    paint_icon(
        painter,
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(diameter * 0.42)),
        icon,
        palette::TEXT,
    );
    response
}

/// The double-tap confirmation: a fading half disc on that side.
fn paint_ripple(ctx: &egui::Context, screen: egui::Rect, direction: Direction, strength: f32) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("touch_ripple"),
    ));
    let (center_x, icon) = match direction {
        Direction::Back => (screen.left(), Icon::PrevScene),
        Direction::Forward => (screen.right(), Icon::NextScene),
    };
    let radius = screen.height() * 0.45;
    let center = egui::pos2(center_x, screen.center().y);
    painter.with_clip_rect(screen).circle_filled(
        center,
        radius,
        egui::Color32::from_white_alpha((40.0 * strength) as u8),
    );
    let icon_x = match direction {
        Direction::Back => screen.left() + radius * 0.45,
        Direction::Forward => screen.right() - radius * 0.45,
    };
    paint_icon(
        &painter,
        egui::Rect::from_center_size(egui::pos2(icon_x, center.y), egui::Vec2::splat(36.0)),
        icon,
        palette::TEXT.gamma_multiply(strength.clamp(0.0, 1.0)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::InputPlugin;
    use bevy::input::touch::{TouchInput, TouchPhase};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn deck() -> Timeline {
        let mut timeline = Timeline::default();
        timeline.cached_duration = 60.0;
        timeline
    }

    #[test]
    fn stepping_without_stops_seeks_ten_seconds_within_the_timeline() {
        let mut timeline = deck();
        timeline.current_time = 55.0;
        step(&mut timeline, Direction::Forward);
        assert_eq!(timeline.seek_request, Some(60.0));
        timeline.current_time = 4.0;
        step(&mut timeline, Direction::Back);
        assert_eq!(timeline.seek_request, Some(0.0));
    }

    /// An app with real touch input, a 1280 px window and 100 ms frames.
    fn touch_app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )))
            .init_resource::<EditorState>()
            .init_resource::<TouchControls>()
            .insert_resource({
                let mut timeline = deck();
                timeline.current_time = 20.0;
                timeline
            })
            .add_systems(Update, touch_gesture_system);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();
        (app, window)
    }

    fn touch(app: &mut App, window: Entity, id: u64, phase: TouchPhase, x: f32, y: f32) {
        app.world_mut().write_message(TouchInput {
            phase,
            position: Vec2::new(x, y),
            window,
            force: None,
            id,
        });
        app.update();
    }

    fn tap(app: &mut App, window: Entity, id: u64, x: f32) {
        touch(app, window, id, TouchPhase::Started, x, 300.0);
        touch(app, window, id, TouchPhase::Ended, x, 300.0);
    }

    fn revealed(app: &App) -> bool {
        app.world().resource::<EditorState>().touch_reveal
    }

    fn take_seek(app: &mut App) -> Option<f64> {
        app.world_mut()
            .resource_mut::<Timeline>()
            .seek_request
            .take()
    }

    #[test]
    fn a_tap_toggles_the_controls_and_they_hide_on_their_own() {
        let (mut app, window) = touch_app();
        assert!(!revealed(&app));
        tap(&mut app, window, 1, 640.0);
        assert!(revealed(&app));
        assert!(app.world().resource::<TouchControls>().active);
        // A tap in the middle never seeks.
        assert_eq!(take_seek(&mut app), None);
        for _ in 0..40 {
            app.update();
        }
        assert!(!revealed(&app), "controls hide after a few seconds");
        tap(&mut app, window, 2, 640.0);
        assert!(revealed(&app));
        for _ in 0..5 {
            app.update();
        }
        tap(&mut app, window, 3, 640.0);
        assert!(!revealed(&app), "a second tap hides them");
    }

    #[test]
    fn double_taps_on_a_side_step_and_keep_the_controls_as_they_were() {
        let (mut app, window) = touch_app();
        tap(&mut app, window, 1, 1150.0);
        assert!(revealed(&app), "the first tap alone shows the controls");
        assert_eq!(take_seek(&mut app), None);
        tap(&mut app, window, 2, 1150.0);
        assert_eq!(take_seek(&mut app), Some(30.0));
        assert!(
            !revealed(&app),
            "the double tap undoes the first tap's toggle"
        );
        // A third quick tap keeps going.
        tap(&mut app, window, 3, 1150.0);
        assert_eq!(take_seek(&mut app), Some(30.0));

        for _ in 0..10 {
            app.update();
        }
        tap(&mut app, window, 4, 100.0);
        tap(&mut app, window, 5, 100.0);
        assert_eq!(take_seek(&mut app), Some(10.0));
    }

    #[test]
    fn slow_taps_and_taps_on_different_sides_do_not_step() {
        let (mut app, window) = touch_app();
        tap(&mut app, window, 1, 1150.0);
        for _ in 0..5 {
            app.update();
        }
        tap(&mut app, window, 2, 1150.0);
        assert_eq!(take_seek(&mut app), None);
        tap(&mut app, window, 3, 100.0);
        tap(&mut app, window, 4, 1150.0);
        assert_eq!(take_seek(&mut app), None);
    }

    #[test]
    fn a_horizontal_swipe_steps_and_a_vertical_one_does_not() {
        let (mut app, window) = touch_app();
        touch(&mut app, window, 1, TouchPhase::Started, 900.0, 300.0);
        touch(&mut app, window, 1, TouchPhase::Moved, 700.0, 305.0);
        touch(&mut app, window, 1, TouchPhase::Ended, 600.0, 305.0);
        assert_eq!(take_seek(&mut app), Some(30.0), "swiping left goes forward");
        // A finger always moves before it lifts; the release keeps the last
        // position it moved to.
        touch(&mut app, window, 2, TouchPhase::Started, 600.0, 300.0);
        touch(&mut app, window, 2, TouchPhase::Moved, 800.0, 310.0);
        touch(&mut app, window, 2, TouchPhase::Ended, 800.0, 310.0);
        assert_eq!(take_seek(&mut app), Some(10.0), "swiping right goes back");
        touch(&mut app, window, 3, TouchPhase::Started, 600.0, 100.0);
        touch(&mut app, window, 3, TouchPhase::Moved, 640.0, 400.0);
        touch(&mut app, window, 3, TouchPhase::Ended, 640.0, 400.0);
        assert_eq!(take_seek(&mut app), None);
    }

    #[test]
    fn touches_that_start_on_the_bar_leave_the_scene_alone() {
        let (mut app, window) = touch_app();
        app.world_mut().resource_mut::<EditorState>().bar_rect = Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 600.0),
            egui::pos2(1280.0, 720.0),
        ));
        touch(&mut app, window, 1, TouchPhase::Started, 1150.0, 650.0);
        touch(&mut app, window, 1, TouchPhase::Ended, 1150.0, 650.0);
        touch(&mut app, window, 2, TouchPhase::Started, 1150.0, 650.0);
        touch(&mut app, window, 2, TouchPhase::Ended, 1150.0, 650.0);
        assert_eq!(take_seek(&mut app), None);
        assert!(revealed(&app), "using the bar keeps it on screen");
    }

    #[test]
    fn touches_over_the_bar_count_as_using_the_controls() {
        let mut touch = TouchControls {
            zoom: 2.0,
            ..default()
        };
        let bar = Some(egui::Rect::from_min_max(
            egui::pos2(10.0, 300.0),
            egui::pos2(200.0, 340.0),
        ));
        // Logical pixels are points times the zoom.
        assert!(touch.over_controls(Vec2::new(100.0, 640.0), bar));
        assert!(!touch.over_controls(Vec2::new(100.0, 200.0), bar));
        // The center buttons only catch touches while they show.
        touch.center_rect = Some(egui::Rect::from_center_size(
            egui::pos2(100.0, 100.0),
            egui::Vec2::splat(40.0),
        ));
        assert!(!touch.over_controls(Vec2::new(200.0, 200.0), None));
        touch.reveal();
        assert!(touch.over_controls(Vec2::new(200.0, 200.0), None));
    }
}
