//! Pointing and drawing over a presentation: a laser pointer (`L`), a pen
//! (`D`, `C` clears) and a zoom around the cursor (`Z`).
//!
//! None of it touches the scene. The laser and the strokes are an egui layer
//! over the audience window and over Presenter View's "Now on screen"; the
//! zoom scales the audience camera's view, so the slide stays sharp. Points
//! are kept in the slide's own coordinates, 0 to 1 across its frame before
//! the zoom, so they stay on what they mark when the window or the zoom
//! changes. Moving to another step clears the strokes. The web player's two
//! pages share the [`InkState`] through [`crate::presenter_link`].

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui;
use gaanim_timeline::timeline::Timeline;
use serde::{Deserialize, Serialize};

use super::{AudienceControlsState, PresenterWindow};
use crate::{PresentationMode, ViewportFrame};

/// Seconds `L` must be held for the laser to go out on release; a shorter
/// press leaves it on until the next.
const LASER_HOLD: f64 = 0.35;
/// Shortest move, across the slide's height, that adds a point to a stroke.
const STROKE_STEP: f32 = 0.002;
/// Zoom `Z` starts at, and its limits.
const ZOOM_START: f32 = 2.0;
const ZOOM_MIN: f32 = 1.25;
const ZOOM_MAX: f32 = 6.0;
/// Pen width and laser radius, across the slide's height.
const PEN_WIDTH: f32 = 0.006;
const LASER_RADIUS: f32 = 0.011;
const PEN_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 59, 48);
const LASER_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 40, 40);

/// The zoom around a point of the slide.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Zoom {
    /// The point that stays put, in slide coordinates.
    pub focus: [f32; 2],
    pub factor: f32,
}

impl Zoom {
    /// Where slide point `point` shows within the frame once zoomed.
    pub(crate) fn project(&self, point: [f32; 2]) -> [f32; 2] {
        let [fx, fy] = self.focus;
        [
            fx + (point[0] - fx) * self.factor,
            fy + (point[1] - fy) * self.factor,
        ]
    }

    /// The slide point shown at `shown` within the zoomed frame.
    pub(crate) fn unproject(&self, shown: [f32; 2]) -> [f32; 2] {
        let [fx, fy] = self.focus;
        [
            fx + (shown[0] - fx) / self.factor,
            fy + (shown[1] - fy) / self.factor,
        ]
    }
}

/// What both screens show over the slide.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct InkState {
    /// The laser's point, while it is on and over the slide.
    pub laser: Option<[f32; 2]>,
    /// The pen's strokes, each a line through its points.
    pub strokes: Vec<Vec<[f32; 2]>>,
    pub zoom: Option<Zoom>,
}

impl InkState {
    /// Whether there is nothing to draw or zoom.
    pub(crate) fn is_empty(&self) -> bool {
        self.laser.is_none() && self.strokes.is_empty() && self.zoom.is_none()
    }

    fn shown(&self, point: [f32; 2]) -> [f32; 2] {
        self.zoom.map_or(point, |zoom| zoom.project(point))
    }
}

/// The pointer over the slide this frame: where, before the zoom (0 to 1
/// across the frame), and its button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pointer {
    pub shown: [f32; 2],
    pub pressed: bool,
    pub down: bool,
}

/// The tools and what they drew, for this presentation.
#[derive(Resource, Debug)]
pub(crate) struct PresentationInk {
    pub state: InkState,
    pub laser: bool,
    pub pen: bool,
    /// When `L` went down, while it is held.
    laser_pressed: Option<f64>,
    zoom_factor: f32,
    drawing: bool,
    /// The step the strokes belong to.
    step: Option<(u32, Option<usize>)>,
    /// The slide's frame within what the audience canvas shows (and the
    /// mirror copies), 0 to 1 across it.
    pub frame_in_view: egui::Rect,
}

impl Default for PresentationInk {
    fn default() -> Self {
        Self {
            state: InkState::default(),
            laser: false,
            pen: false,
            laser_pressed: None,
            zoom_factor: ZOOM_START,
            drawing: false,
            step: None,
            frame_in_view: egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        }
    }
}

impl PresentationInk {
    /// Whether a tool is on.
    pub(crate) fn active(&self) -> bool {
        self.laser || self.pen || self.state.zoom.is_some()
    }

    /// `L` went down at `now` seconds: the laser comes on, or goes out if
    /// it was left on.
    pub(crate) fn press_laser(&mut self, now: f64) {
        if self.laser {
            self.laser = false;
            self.state.laser = None;
        } else {
            self.laser = true;
            self.pen = false;
            self.drawing = false;
            self.laser_pressed = Some(now);
        }
    }

    /// `L` came up at `now`: after a hold the laser goes out.
    pub(crate) fn release_laser(&mut self, now: f64) {
        if let Some(pressed) = self.laser_pressed.take()
            && now - pressed >= LASER_HOLD
        {
            self.laser = false;
            self.state.laser = None;
        }
    }

    /// `D`: the pen on or off.
    pub(crate) fn toggle_pen(&mut self) {
        self.pen = !self.pen;
        self.drawing = false;
        if self.pen {
            self.laser = false;
            self.state.laser = None;
        }
    }

    /// `C`: clear the strokes.
    pub(crate) fn clear(&mut self) {
        self.state.strokes.clear();
        self.drawing = false;
    }

    /// `Z`: zoom around `focus` (slide coordinates before the zoom), or
    /// stop zooming.
    pub(crate) fn toggle_zoom(&mut self, focus: [f32; 2]) {
        self.state.zoom = match self.state.zoom {
            Some(_) => None,
            None => Some(Zoom {
                focus: clamp_unit(focus),
                factor: self.zoom_factor,
            }),
        };
    }

    /// Scroll `steps` notches while zooming: closer up, or back.
    pub(crate) fn scroll_zoom(&mut self, steps: f32) {
        if let Some(zoom) = &mut self.state.zoom {
            zoom.factor = (zoom.factor * 1.15_f32.powf(steps)).clamp(ZOOM_MIN, ZOOM_MAX);
            self.zoom_factor = zoom.factor;
        }
    }

    /// Turn every tool off and drop what they drew.
    pub(crate) fn reset(&mut self) {
        *self = Self {
            zoom_factor: self.zoom_factor,
            frame_in_view: self.frame_in_view,
            step: self.step,
            ..Self::default()
        };
    }

    /// Follow the pointer: the zoom keeps the point under it, the laser
    /// shows there and the pen draws while its button is down.
    pub(crate) fn pointer(&mut self, pointer: Option<Pointer>) {
        let Some(pointer) = pointer else {
            self.state.laser = None;
            self.drawing = false;
            return;
        };
        // The zoom follows the pointer, but holds still while the pen draws,
        // so the stroke lands where it shows.
        if let Some(zoom) = &mut self.state.zoom
            && !(self.pen && pointer.down)
        {
            zoom.focus = clamp_unit(pointer.shown);
        }
        let point = self
            .state
            .zoom
            .map_or(pointer.shown, |zoom| zoom.unproject(pointer.shown));
        self.state.laser = self.laser.then_some(point);
        if !self.pen {
            self.drawing = false;
            return;
        }
        if pointer.pressed {
            self.state.strokes.push(vec![point]);
            self.drawing = true;
        } else if self.drawing && pointer.down {
            if let Some(stroke) = self.state.strokes.last_mut()
                && stroke
                    .last()
                    .is_none_or(|last| distance(*last, point) >= STROKE_STEP)
            {
                stroke.push(point);
            }
        } else {
            self.drawing = false;
        }
    }

    /// The presentation is at `step` (segment, stop): a new one clears the
    /// strokes.
    pub(crate) fn note_step(&mut self, step: Option<(u32, Option<usize>)>) {
        if self.step != step {
            if self.step.is_some() {
                self.clear();
            }
            self.step = step;
        }
    }

    /// The other page's state (web player).
    pub(crate) fn apply_remote(&mut self, state: InkState) {
        self.state = state;
    }
}

fn clamp_unit([x, y]: [f32; 2]) -> [f32; 2] {
    [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)]
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Draw `state` over a slide shown in `frame` (before the zoom).
pub(crate) fn paint(painter: &egui::Painter, frame: egui::Rect, state: &InkState) {
    if frame.width() <= 0.0 || frame.height() <= 0.0 {
        return;
    }
    let painter = painter.with_clip_rect(frame.intersect(painter.clip_rect()));
    let to_screen = |point: [f32; 2]| {
        let [x, y] = state.shown(point);
        frame.min + egui::vec2(x * frame.width(), y * frame.height())
    };
    let factor = state.zoom.map_or(1.0, |zoom| zoom.factor);
    let width = (PEN_WIDTH * frame.height() * factor).max(1.5);
    for stroke in &state.strokes {
        let points: Vec<egui::Pos2> = stroke.iter().copied().map(to_screen).collect();
        match points.as_slice() {
            [] => {}
            [point] => {
                painter.circle_filled(*point, width * 0.5, PEN_COLOR);
            }
            _ => {
                painter.add(egui::Shape::line(
                    points,
                    egui::Stroke::new(width, PEN_COLOR),
                ));
            }
        }
    }
    if let Some(point) = state.laser {
        let center = to_screen(point);
        let radius = (LASER_RADIUS * frame.height()).max(4.0);
        for (scale, alpha) in [(2.6, 30), (1.8, 60), (1.25, 110)] {
            painter.circle_filled(
                center,
                radius * scale,
                egui::Color32::from_rgba_unmultiplied(255, 40, 40, alpha),
            );
        }
        painter.circle_filled(center, radius, LASER_COLOR);
        painter.circle_filled(
            center,
            radius * 0.45,
            egui::Color32::from_rgb(255, 210, 210),
        );
    }
}

/// The audience window's pointer and wheel drive the tools.
#[allow(clippy::too_many_arguments)]
pub(crate) fn audience_ink_input_system(
    presentation: Res<PresentationMode>,
    windows: Query<(&Window, Has<PresenterWindow>), With<PrimaryWindow>>,
    frame: Res<ViewportFrame>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    controls: Res<AudienceControlsState>,
    timeline: Option<Res<Timeline>>,
    mut ink: ResMut<PresentationInk>,
) {
    let steps: f32 = wheel
        .read()
        .map(|event| match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / 40.0,
        })
        .sum();
    if !presentation.active {
        if ink.active() || !ink.state.is_empty() {
            ink.reset();
        }
        return;
    }
    if let Some(timeline) = timeline {
        ink.note_step(
            timeline
                .segment_position
                .map(|position| (position.segment_id, position.stop_index)),
        );
    }
    let Ok((window, presenter_page)) = windows.single() else {
        return;
    };
    let width = window.width().max(1.0);
    let height = window.height().max(1.0);
    ink.frame_in_view = if presenter_page {
        // The web player's Presenter View page draws the slide alone.
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0))
    } else {
        egui::Rect::from_min_size(
            egui::pos2(
                frame.origin.x as f32 / width,
                frame.origin.y as f32 / height,
            ),
            egui::vec2(frame.size.x as f32 / width, frame.size.y as f32 / height),
        )
    };
    // Presenter View's preview feeds the tools there; this window feeds
    // them while the pointer is over it.
    if presenter_page || !window.focused || controls.pointer_over {
        return;
    }
    if steps != 0.0 {
        ink.scroll_zoom(steps);
    }
    if !ink.active() {
        return;
    }
    let pointer = window.cursor_position().and_then(|cursor| {
        let shown = [
            (cursor.x - frame.origin.x as f32) / (frame.size.x as f32).max(1.0),
            (cursor.y - frame.origin.y as f32) / (frame.size.y as f32).max(1.0),
        ];
        (0.0..=1.0).contains(&shown[0]).then_some(())?;
        (0.0..=1.0).contains(&shown[1]).then_some(())?;
        Some(Pointer {
            shown,
            pressed: mouse.just_pressed(MouseButton::Left),
            down: mouse.pressed(MouseButton::Left),
        })
    });
    ink.pointer(pointer);
}

/// Draw the laser and the strokes over the audience window.
pub(crate) fn audience_ink_overlay_system(
    mut contexts: bevy_egui::EguiContexts,
    presentation: Res<PresentationMode>,
    frame: Res<ViewportFrame>,
    page: Query<(), (With<PrimaryWindow>, With<PresenterWindow>)>,
    ink: Res<PresentationInk>,
) {
    if !presentation.active || !page.is_empty() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ink.pen && ctx.is_pointer_over_egui() {
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if ink.state.laser.is_none() && ink.state.strokes.is_empty() {
        return;
    }
    let rect = egui::Rect::from_min_size(
        egui::pos2(frame.origin.x as f32, frame.origin.y as f32),
        egui::vec2(frame.size.x as f32, frame.size.y as f32),
    );
    // Beneath the blank screen and the playback dock, which are
    // foreground layers.
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("gaanim-presentation-ink"),
    ));
    paint(&painter, rect, &ink.state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(shown: [f32; 2], pressed: bool, down: bool) -> Option<Pointer> {
        Some(Pointer {
            shown,
            pressed,
            down,
        })
    }

    #[test]
    fn a_short_press_leaves_the_laser_on_and_a_hold_does_not() {
        let mut ink = PresentationInk::default();
        ink.press_laser(1.0);
        ink.release_laser(1.1);
        assert!(ink.laser);
        ink.pointer(at([0.5, 0.25], false, false));
        assert_eq!(ink.state.laser, Some([0.5, 0.25]));
        // Pressed again, it goes out.
        ink.press_laser(2.0);
        assert!(!ink.laser);
        assert_eq!(ink.state.laser, None);
        // Held, it lasts as long as the key.
        ink.press_laser(3.0);
        ink.release_laser(3.5);
        assert!(!ink.laser);
    }

    #[test]
    fn the_pen_draws_while_the_button_is_down() {
        let mut ink = PresentationInk::default();
        ink.toggle_pen();
        ink.pointer(at([0.1, 0.1], false, false));
        assert!(ink.state.strokes.is_empty());
        ink.pointer(at([0.1, 0.1], true, true));
        ink.pointer(at([0.1001, 0.1], false, true)); // too close to add
        ink.pointer(at([0.2, 0.1], false, true));
        ink.pointer(at([0.3, 0.1], false, false));
        ink.pointer(at([0.4, 0.1], false, false));
        assert_eq!(ink.state.strokes, [vec![[0.1, 0.1], [0.2, 0.1]]]);
        ink.pointer(at([0.5, 0.5], true, true));
        assert_eq!(ink.state.strokes.len(), 2);
        ink.clear();
        assert!(ink.state.strokes.is_empty());
    }

    #[test]
    fn a_new_step_clears_the_strokes() {
        let mut ink = PresentationInk::default();
        ink.note_step(Some((1, None)));
        ink.toggle_pen();
        ink.pointer(at([0.1, 0.1], true, true));
        ink.note_step(Some((1, None)));
        assert_eq!(ink.state.strokes.len(), 1);
        ink.note_step(Some((1, Some(0))));
        assert!(ink.state.strokes.is_empty());
    }

    #[test]
    fn the_zoom_keeps_the_point_under_the_pointer() {
        let zoom = Zoom {
            focus: [0.25, 0.5],
            factor: 2.0,
        };
        assert_eq!(zoom.project([0.25, 0.5]), [0.25, 0.5]);
        assert_eq!(zoom.project([0.5, 0.5]), [0.75, 0.5]);
        assert_eq!(zoom.unproject(zoom.project([0.4, 0.3])), [0.4, 0.3]);

        let mut ink = PresentationInk::default();
        ink.toggle_zoom([0.5, 0.5]);
        ink.pointer(at([0.75, 0.5], false, false));
        assert_eq!(ink.state.zoom.unwrap().focus, [0.75, 0.5]);
        // Strokes drawn while zoomed land on the slide where they show.
        ink.toggle_pen();
        ink.pointer(at([0.75, 0.5], true, true));
        ink.pointer(at([0.85, 0.5], false, true));
        let stroke = &ink.state.strokes[0];
        assert!((stroke[1][0] - 0.8).abs() < 1e-6, "{stroke:?}");
        ink.scroll_zoom(100.0);
        assert_eq!(ink.state.zoom.unwrap().factor, ZOOM_MAX);
        ink.toggle_zoom([0.0, 0.0]);
        assert_eq!(ink.state.zoom, None);
    }

    #[test]
    fn tools_take_turns_and_reset_keeps_the_zoom_level() {
        let mut ink = PresentationInk::default();
        ink.toggle_pen();
        ink.press_laser(0.0);
        assert!(ink.laser && !ink.pen);
        ink.toggle_pen();
        assert!(ink.pen && !ink.laser);
        ink.toggle_zoom([0.5, 0.5]);
        ink.scroll_zoom(-1.0);
        let factor = ink.state.zoom.unwrap().factor;
        ink.reset();
        assert!(!ink.active() && ink.state.is_empty());
        ink.toggle_zoom([0.5, 0.5]);
        assert_eq!(ink.state.zoom.unwrap().factor, factor);
    }
}
