use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use gaanim_timeline::clip::{ClipId, ClipPayload, PropertyLensSpec, TrackId};
use gaanim_timeline::timeline::Timeline;
use std::collections::HashMap;

// ── Color palette ──────────────────────────────────────────────────────
const BG: Color32 = Color32::from_rgb(26, 26, 26);
const TRACK_EVEN: Color32 = Color32::from_rgb(37, 37, 37);
const TRACK_ODD: Color32 = Color32::from_rgb(42, 42, 42);
const RULER_BG: Color32 = Color32::from_rgb(45, 45, 45);
const HEADER_BG: Color32 = Color32::from_rgb(35, 35, 35);
const GRID_MAJOR: Color32 = Color32::from_rgb(55, 55, 55);
const GRID_MINOR: Color32 = Color32::from_rgb(45, 45, 45);
const PLAYHEAD: Color32 = Color32::from_rgb(255, 60, 60);
const SELECTION: Color32 = Color32::from_rgb(68, 160, 255);
const TEXT: Color32 = Color32::from_rgb(200, 200, 200);

const TEXT_LABEL: Color32 = Color32::from_rgb(240, 240, 240);

const CLR_ANIM: Color32 = Color32::from_rgb(70, 130, 220);
const CLR_AUDIO: Color32 = Color32::from_rgb(70, 180, 90);
const CLR_WAIT: Color32 = Color32::from_rgb(100, 100, 100);
const CLR_MARKER: Color32 = Color32::from_rgb(220, 200, 60);
const CLR_BREAKPOINT: Color32 = Color32::from_rgb(220, 80, 80);
const CLR_SEGMENT: Color32 = Color32::from_rgb(180, 100, 200);
const CLR_KEYFRAME: Color32 = Color32::from_rgb(255, 200, 80);

// ── Drag state ─────────────────────────────────────────────────────────
enum ZoomBarKind {
    Body,
    LeftEdge,
    RightEdge,
}

enum DragState {
    None,
    Playhead,
    ClipBody(ClipId),
    ClipLeftEdge(ClipId),
    ClipRightEdge(ClipId),
    ZoomBar {
        kind: ZoomBarKind,
        initial_start: f64,
        initial_end: f64,
    },
}

// ── Widget ─────────────────────────────────────────────────────────────
pub struct TimelineWidget {
    pub pixels_per_second: f64,
    pub scroll_offset: f32,
    pub header_height: f32,
    pub ruler_height: f32,
    pub track_height: f32,
    pub zoom_bar_height: f32,
    pub selected_clip: Option<ClipId>,
    pub snap_enabled: bool,
    pub snap_threshold_pixels: f32,

    drag_state: DragState,
    drag_mouse_start_x: f32,
    drag_orig_start: f64,
    drag_orig_duration: f64,

    hovered_clip_info: Option<String>,
    last_canvas_width: f32,
}

impl Default for TimelineWidget {
    fn default() -> Self {
        Self {
            pixels_per_second: 80.0,
            scroll_offset: 0.0,
            header_height: 28.0,
            ruler_height: 22.0,
            track_height: 28.0,
            zoom_bar_height: 16.0,
            selected_clip: None,
            snap_enabled: true,
            snap_threshold_pixels: 8.0,
            drag_state: DragState::None,
            drag_mouse_start_x: 0.0,
            drag_orig_start: 0.0,
            drag_orig_duration: 0.0,
            hovered_clip_info: None,
            last_canvas_width: 800.0,
        }
    }
}

impl TimelineWidget {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Main entry point ────────────────────────────────────────────────
    pub fn show(&mut self, ui: &mut egui::Ui, timeline: &mut Timeline) {
        let track_indices: HashMap<TrackId, usize> = timeline
            .tracks
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (id, i))
            .collect();

        let num_tracks = timeline.tracks.len().max(1) as f32;
        let tracks_height = self.track_height * num_tracks;
        let canvas_height = self.zoom_bar_height + self.ruler_height + tracks_height;

        // ── Header ──────────────────────────────────────────────────────
        let header_size = Vec2::new(ui.available_width(), self.header_height);
        let (header_rect, _) = ui.allocate_exact_size(header_size, Sense::hover());
        let p = ui.painter_at(header_rect);
        p.rect_filled(header_rect, 0u8, HEADER_BG);

        {
            let inner = header_rect.shrink2(Vec2::new(6.0, 0.0));
            let mut hu = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            self.paint_header(&mut hu, timeline);
        }

        // ── Canvas (zoom bar + ruler + tracks) ─────────────────────────
        let canvas_size = Vec2::new(ui.available_width(), canvas_height.max(100.0));
        let (canvas_rect, response) =
            ui.allocate_exact_size(canvas_size, Sense::click_and_drag());

        let zoom_bar_rect = Rect::from_min_size(
            canvas_rect.min,
            Vec2::new(canvas_rect.width(), self.zoom_bar_height),
        );
        let ruler_rect = Rect::from_min_size(
            Pos2::new(canvas_rect.min.x, zoom_bar_rect.max.y),
            Vec2::new(canvas_rect.width(), self.ruler_height),
        );
        let tracks_rect = Rect::from_min_size(
            Pos2::new(canvas_rect.min.x, ruler_rect.max.y),
            Vec2::new(canvas_rect.width(), tracks_height),
        );

        self.last_canvas_width = canvas_rect.width();

        // ── Hover state ──────────────────────────────────────────────────
        let hover_pos = response.hover_pos();
        let hovered_clip = hover_pos.and_then(|pos| {
            if !tracks_rect.contains(pos) { return None; }
            let on_edge = self.hit_clip_edge(pos, tracks_rect, timeline, &track_indices);
            if on_edge.is_some() { return None; }
            self.hit_clip_body(pos, tracks_rect, timeline, &track_indices)
        });
        self.hovered_clip_info = hovered_clip.and_then(|cid| {
            timeline.clips.get(cid).map(|c| {
                let label = clip_label(&c.payload);
                format!("{} | {:.2}s\u{2192}{:.2}s", label, c.start, c.end())
            })
        });

        let on_edge = hover_pos.map_or(false, |pos| {
            self.hit_clip_edge(pos, tracks_rect, timeline, &track_indices).is_some()
        });

        // Input first so dragging feels responsive
        self.handle_input(
            ui, &response, zoom_bar_rect, ruler_rect, tracks_rect, timeline,
            &track_indices, on_edge, hover_pos,
        );

        // Paint
        let p = ui.painter_at(canvas_rect);
        self.paint_zoom_bar(&p, zoom_bar_rect, tracks_rect, timeline);
        self.paint_tracks(&p, tracks_rect, timeline, &track_indices);
        self.paint_ruler(&p, ruler_rect);
        self.paint_grid(&p, tracks_rect, timeline);
        self.paint_clips(&p, tracks_rect, timeline, &track_indices, hover_pos);
        self.paint_keyframes(&p, ruler_rect, timeline);
        self.paint_playhead(&p, canvas_rect, timeline);

        // ── Auto-scroll during playback ─────────────────────────────────
        if timeline.is_playing {
            let px = self.time_to_x(timeline.current_time, tracks_rect);
            let margin = 80.0;
            if px > tracks_rect.max.x - margin {
                self.scroll_offset += px - (tracks_rect.max.x - margin);
                self.clamp_scroll(tracks_rect, timeline);
            } else if px < tracks_rect.min.x + 20.0 {
                self.scroll_offset -= (tracks_rect.min.x + 20.0 - px).min(self.scroll_offset);
            }
        }
    }

    // ── Header ──────────────────────────────────────────────────────────
    fn paint_header(&mut self, ui: &mut egui::Ui, timeline: &mut Timeline) {
        let play_text = if timeline.is_playing { "⏸" } else { "▶" };
        if ui.button(play_text).clicked() {
            timeline.is_playing = !timeline.is_playing;
        }
        if ui.button("⏮").clicked() {
            timeline.is_playing = false;
            timeline.seek_request = Some(0.0);
        }
        if ui.button("⏭").clicked() {
            timeline.is_playing = false;
            timeline.seek_request = Some(timeline.cached_duration);
        }

        ui.separator();

        // Show clip info on hover, otherwise time label
        if let Some(info) = &self.hovered_clip_info {
            ui.label(info);
        } else {
            ui.label(format!(
                "{:.2}s / {:.2}s",
                timeline.current_time, timeline.cached_duration
            ));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Fit").clicked() {
                let dur = timeline.cached_duration.max(1.0);
                let w = self.last_canvas_width.max(100.0);
                self.pixels_per_second = (w as f64 / dur).clamp(20.0, 500.0);
                self.scroll_offset = 0.0;
            }
        });
    }

    // ── Ruler ───────────────────────────────────────────────────────────
    fn paint_ruler(&self, p: &egui::Painter, rect: Rect) {
        p.rect_filled(rect, 0u8, RULER_BG);

        let (major, minor) = self.tick_intervals(rect);
        let t0 = self.x_to_time(rect.min.x, rect);
        let t1 = self.x_to_time(rect.max.x, rect);

        let mut t = (t0 / minor).ceil() * minor;
        while t <= t1 {
            let x = self.time_to_x(t, rect);
            let is_major = (t / major).fract().abs() < 1e-9
                || ((t / major) - (t / major).round()).abs() < 1e-9;

            let h = if is_major {
                rect.height()
            } else {
                rect.height() * 0.45
            };
            let c = if is_major {
                Color32::from_rgb(200, 200, 200)
            } else {
                Color32::from_rgb(120, 120, 120)
            };
            p.line_segment(
                [Pos2::new(x, rect.max.y - h), Pos2::new(x, rect.max.y)],
                Stroke::new(1.0, c),
            );

            if is_major {
                p.text(
                    Pos2::new(x + 3.0, rect.min.y + 1.0),
                    Align2::LEFT_TOP,
                    format!("{t:.1}s"),
                    FontId::proportional(11.0),
                    Color32::from_rgb(200, 200, 200),
                );
            }

            t += minor;
        }
    }

    // ── Track backgrounds ───────────────────────────────────────────────
    fn paint_tracks(
        &self,
        p: &egui::Painter,
        rect: Rect,
        timeline: &Timeline,
        track_indices: &HashMap<TrackId, usize>,
    ) {
        p.rect_filled(rect, 0u8, BG);

        for (track, idx) in track_indices {
            let y = rect.min.y + *idx as f32 * self.track_height;
            let tr = Rect::from_min_size(
                Pos2::new(rect.min.x, y),
                Vec2::new(rect.width(), self.track_height),
            );
            let bg = if idx % 2 == 0 { TRACK_EVEN } else { TRACK_ODD };
            p.rect_filled(tr, 0u8, bg);

            if let Some(t) = timeline.tracks.get(*track) {
                p.text(
                    Pos2::new(rect.min.x + 4.0, y + self.track_height / 2.0),
                    Align2::LEFT_CENTER,
                    &t.name,
                    FontId::proportional(11.0),
                    TEXT,
                );
            }
        }
    }

    // ── Grid ────────────────────────────────────────────────────────────
    fn paint_grid(&self, p: &egui::Painter, rect: Rect, _timeline: &Timeline) {
        let (major, minor) = self.tick_intervals(rect);
        let t0 = self.x_to_time(rect.min.x, rect);
        let t1 = self.x_to_time(rect.max.x, rect);

        let mut t = (t0 / minor).ceil() * minor;
        while t <= t1 {
            let x = self.time_to_x(t, rect);
            let is_major =
                (t / major).fract().abs() < 1e-9 || ((t / major) - (t / major).round()).abs() < 1e-9;
            p.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(if is_major { 1.0 } else { 0.5 }, if is_major { GRID_MAJOR } else { GRID_MINOR }),
            );
            t += minor;
        }
    }

    // ── Clips ───────────────────────────────────────────────────────────
    fn paint_clips(
        &self,
        p: &egui::Painter,
        rect: Rect,
        timeline: &Timeline,
        track_indices: &HashMap<TrackId, usize>,
        hover_pos: Option<Pos2>,
    ) {
        for clip in timeline.clips.values() {
            let Some(&track_idx) = track_indices.get(&clip.track) else {
                continue;
            };

            let x1 = self.time_to_x(clip.start, rect);
            let x2 = self.time_to_x(clip.start + clip.duration, rect);
            let y = rect.min.y + track_idx as f32 * self.track_height + 2.0;
            let ch = self.track_height - 4.0;

            let color = clip_color(&clip.payload);
            let cr = Rect::from_min_size(
                Pos2::new(x1, y),
                Vec2::new((x2 - x1).max(2.0), ch),
            );

            let is_hovered = hover_pos.map_or(false, |pos| cr.contains(pos));
            let fill = if is_hovered { lighten(color) } else { color };
            p.rect_filled(cr, 3u8, fill);

            let label = clip_label(&clip.payload);
            p.text(
                Pos2::new(cr.min.x + 4.0, cr.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(10.0),
                TEXT_LABEL,
            );

            if self.selected_clip == Some(clip.id) {
                p.rect_stroke(cr, 3u8, Stroke::new(2.0, SELECTION), StrokeKind::Inside);
            }
        }
    }

    // ── Zoom bar ────────────────────────────────────────────────────────
    fn paint_zoom_bar(&self, p: &egui::Painter, rect: Rect, tracks_rect: Rect, timeline: &Timeline) {
        p.rect_filled(rect, 0u8, Color32::from_rgb(25, 25, 25));

        let total = timeline.cached_duration.max(0.01);
        let bar_w = rect.width();

        let vw = tracks_rect.width() as f64;
        let v_start = (self.scroll_offset as f64 / self.pixels_per_second).clamp(0.0, total);
        let v_end = ((self.scroll_offset + vw as f32) as f64 / self.pixels_per_second).clamp(0.0, total);
        let v_dur = (v_end - v_start).max(0.01);

        let l_ratio = v_start / total;
        let r_ratio = v_end / total;

        let win_l = rect.min.x + (l_ratio as f32 * bar_w);
        let win_r = rect.min.x + (r_ratio as f32 * bar_w);

        // Window rect
        let win_rect = Rect::from_min_size(
            Pos2::new(win_l, rect.min.y + 2.0),
            Vec2::new((win_r - win_l).max(12.0), rect.height() - 4.0),
        );
        p.rect_filled(win_rect, 3u8, Color32::from_rgb(120, 120, 120));
        p.rect_stroke(win_rect, 3u8, Stroke::new(1.0, Color32::from_rgb(160, 160, 160)), StrokeKind::Inside);

        // Label showing visible range / total
        let label = if v_dur >= 1.0 {
            format!("{:.0}s / {:.0}s", v_dur, total)
        } else {
            format!("{:.1}s / {:.0}s", v_dur, total)
        };
        let font = FontId::proportional(9.0);
        let text_color = Color32::from_rgb(200, 200, 200);
        // Center text in the window if wide enough, otherwise to the right
        if win_rect.width() > 60.0 {
            p.text(win_rect.center(), Align2::CENTER_CENTER, label, font, text_color);
        } else {
            p.text(Pos2::new(win_r + 4.0, win_rect.center().y), Align2::LEFT_CENTER, label, font, text_color);
        }
    }
    fn paint_keyframes(&self, p: &egui::Painter, rect: Rect, timeline: &Timeline) {
        let d = 5.0;
        for &kf_time in timeline.keyframes.keys() {
            let x = self.time_to_x(kf_time.0, rect);
            if x < rect.min.x - d || x > rect.max.x + d {
                continue;
            }
            let cy = rect.center().y;
            let diamond = [
                Pos2::new(x, cy - d),
                Pos2::new(x + d, cy),
                Pos2::new(x, cy + d),
                Pos2::new(x - d, cy),
            ];
            p.add(egui::Shape::convex_polygon(
                diamond.to_vec(),
                CLR_KEYFRAME,
                Stroke::new(1.0, Color32::from_rgb(200, 160, 50)),
            ));
        }
    }

    // ── Playhead ────────────────────────────────────────────────────────
    fn paint_playhead(&self, p: &egui::Painter, rect: Rect, timeline: &Timeline) {
        let x = self.time_to_x(timeline.current_time, rect);
        if x < rect.min.x || x > rect.max.x {
            return;
        }

        p.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            Stroke::new(2.0, PLAYHEAD),
        );

        // Triangle handle in the ruler section
        let ty = rect.min.y + self.header_height;
        let ts = 6.0;
        let tri = [
            Pos2::new(x, ty),
            Pos2::new(x - ts, ty + ts),
            Pos2::new(x + ts, ty + ts),
        ];
        p.add(egui::Shape::convex_polygon(tri.to_vec(), PLAYHEAD, Stroke::NONE));
    }

    // ── Input ───────────────────────────────────────────────────────────
    fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        zoom_bar_rect: Rect,
        ruler_rect: Rect,
        tracks_rect: Rect,
        timeline: &mut Timeline,
        track_indices: &HashMap<TrackId, usize>,
        on_edge: bool,
        hover_pos: Option<Pos2>,
    ) {
        // ── Keyboard shortcuts ──────────────────────────────────────────
        let pressed = |key| ui.input(|i| i.key_pressed(key));
        // Space = play/pause (global, no focus needed)
        if pressed(egui::Key::Space) {
            timeline.is_playing = !timeline.is_playing;
        }
        // Delete/Backspace only when canvas has focus
        if response.has_focus() {
            if pressed(egui::Key::Delete) || pressed(egui::Key::Backspace) {
                if let Some(cid) = self.selected_clip {
                    timeline.remove_clip(cid);
                    self.selected_clip = None;
                    self.drag_state = DragState::None;
                }
            }
        }
        if response.clicked() {
            response.request_focus();
        }

        // ── Zoom bar double-click → Fit ─────────────────────────────────
        if response.double_clicked() {
            if let Some(pos) = hover_pos {
                if zoom_bar_rect.contains(pos) {
                    let dur = timeline.cached_duration.max(1.0);
                    let w = tracks_rect.width().max(100.0);
                    self.pixels_per_second = (w as f64 / dur).clamp(20.0, 500.0);
                    self.scroll_offset = 0.0;
                }
            }
        }

        // ── Zoom bar helpers (used below) ────────────────────────────────
        let total = timeline.cached_duration.max(0.01);
        let bar_w = zoom_bar_rect.width();
        let view_w = tracks_rect.width();
        let min_v_dur = view_w as f64 / 500.0; // max pps
        let zoom_bar_info = |pos_x: f32| -> (f64, f64) {
            let ratio = ((pos_x - zoom_bar_rect.min.x) / bar_w).clamp(0.0, 1.0) as f64;
            let ct = ratio * total;
            (ct, ratio)
        };

        // ── Hover cursor ────────────────────────────────────────────────
        if hover_pos.is_some() && matches!(self.drag_state, DragState::None) {
            if let Some(pos) = hover_pos {
                if zoom_bar_rect.contains(pos) {
                    let v_start = (self.scroll_offset as f64 / self.pixels_per_second).max(0.0);
                    let v_end = ((self.scroll_offset + view_w) as f64 / self.pixels_per_second)
                        .min(total);
                    let l_ratio = v_start / total;
                    let r_ratio = v_end / total;
                    let win_l = zoom_bar_rect.min.x + l_ratio as f32 * bar_w;
                    let win_r = zoom_bar_rect.min.x + r_ratio as f32 * bar_w;
                    let edge = 6.0;
                    if (pos.x - win_l).abs() < edge || (pos.x - win_r).abs() < edge {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                    } else if pos.x >= win_l && pos.x <= win_r {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                } else if on_edge {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                } else if tracks_rect.contains(pos)
                    && self.hit_clip_body(pos, tracks_rect, timeline, track_indices).is_some()
                {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
        }

        // Scroll → zoom (ctrl) or pan.
        // When hovering over the ruler or zoom bar, scroll always zooms.
        let scroll = ui.input(|i| i.smooth_scroll_delta);
        if scroll != Vec2::ZERO {
            let over_zoom_area = hover_pos.map_or(false, |p| ruler_rect.contains(p) || zoom_bar_rect.contains(p));
            let ctrl = ui.input(|i| i.modifiers.ctrl);
            if ctrl || over_zoom_area {
                let anchor = response
                    .hover_pos()
                    .unwrap_or(Pos2::new(tracks_rect.center().x, 0.0));
                let anchor_time = self.x_to_time(anchor.x, tracks_rect);
                let old_pps = self.pixels_per_second;
                self.pixels_per_second *= 1.0 + scroll.y as f64 * -0.002;
                self.pixels_per_second = self.pixels_per_second.clamp(20.0, 500.0);
                self.scroll_offset =
                    (anchor.x - tracks_rect.min.x) + (anchor_time * self.pixels_per_second) as f32
                        - (anchor.x - tracks_rect.min.x + self.scroll_offset)
                            * (self.pixels_per_second / old_pps) as f32;
                self.clamp_scroll(tracks_rect, timeline);
            } else {
                self.scroll_offset = (self.scroll_offset - scroll.x).max(0.0);
                self.clamp_scroll(tracks_rect, timeline);
            }
            ui.ctx().request_repaint();
        }

        let mouse = response.interact_pointer_pos();

        match &self.drag_state {
            DragState::None => {
                if response.drag_started() {
                    if let Some(pos) = mouse {
                        // Zoom bar interaction (highest priority)
                        if zoom_bar_rect.contains(pos) {
                            let v_start = (self.scroll_offset as f64 / self.pixels_per_second).max(0.0);
                            let v_end = ((self.scroll_offset + view_w) as f64 / self.pixels_per_second)
                                .min(total);
                            let l_ratio = v_start / total;
                            let r_ratio = v_end / total;
                            let win_l = zoom_bar_rect.min.x + l_ratio as f32 * bar_w;
                            let win_r = zoom_bar_rect.min.x + r_ratio as f32 * bar_w;
                            let edge = 6.0;

                            let kind = if (pos.x - win_l).abs() < edge {
                                Some(ZoomBarKind::LeftEdge)
                            } else if (pos.x - win_r).abs() < edge {
                                Some(ZoomBarKind::RightEdge)
                            } else if pos.x >= win_l && pos.x <= win_r {
                                Some(ZoomBarKind::Body)
                            } else {
                                None
                            };

                            if let Some(k) = kind {
                                self.drag_state = DragState::ZoomBar {
                                    kind: k,
                                    initial_start: v_start,
                                    initial_end: v_end,
                                };
                            } else {
                                // Click on zoom bar track → jump center
                                let (ct, _) = zoom_bar_info(pos.x);
                                let v_dur = v_end - v_start;
                                let new_start = (ct - v_dur / 2.0).clamp(0.0, total - v_dur);
                                let new_end = new_start + v_dur;
                                self.scroll_offset = (new_start * self.pixels_per_second) as f32;
                                self.clamp_scroll(tracks_rect, timeline);
                                self.drag_state = DragState::ZoomBar {
                                    kind: ZoomBarKind::Body,
                                    initial_start: new_start,
                                    initial_end: new_end,
                                };
                            }
                        } else if ruler_rect.contains(pos)
                            || (pos.x - self.time_to_x(timeline.current_time, tracks_rect)).abs()
                                < 10.0
                        {
                            timeline.is_playing = false;
                            self.drag_state = DragState::Playhead;
                            self.drag_mouse_start_x = pos.x;
                            let t = self.x_to_time(pos.x, tracks_rect).clamp(0.0, timeline.cached_duration);
                            timeline.seek_request = Some(t);
                        } else if let Some((cid, edge)) =
                            self.hit_clip_edge(pos, tracks_rect, timeline, track_indices)
                        {
                            match edge {
                                ClipEdge::Left => {
                                    self.drag_state = DragState::ClipLeftEdge(cid);
                                }
                                ClipEdge::Right => {
                                    self.drag_state = DragState::ClipRightEdge(cid);
                                }
                            }
                            self.drag_mouse_start_x = pos.x;
                            if let Some(c) = timeline.clips.get(cid) {
                                self.drag_orig_start = c.start;
                                self.drag_orig_duration = c.duration;
                            }
                        } else if let Some(cid) =
                            self.hit_clip_body(pos, tracks_rect, timeline, track_indices)
                        {
                            self.drag_state = DragState::ClipBody(cid);
                            self.drag_mouse_start_x = pos.x;
                            if let Some(c) = timeline.clips.get(cid) {
                                self.drag_orig_start = c.start;
                                self.drag_orig_duration = c.duration;
                            }
                        } else if ruler_rect.contains(pos) {
                            let t =
                                self.x_to_time(pos.x, tracks_rect).clamp(0.0, timeline.cached_duration);
                            timeline.seek_request = Some(t);
                        }
                    }
                }
                if response.clicked() {
                    if let Some(pos) = mouse {
                        if tracks_rect.contains(pos) {
                            self.selected_clip =
                                self.hit_clip_body(pos, tracks_rect, timeline, track_indices);
                        }
                    }
                }
            }
            _ => {
                if response.dragged() {
                    if let Some(pos) = mouse {
                        match &self.drag_state {
                            DragState::Playhead => {
                                let t = self.x_to_time(pos.x, tracks_rect)
                                    .clamp(0.0, timeline.cached_duration);
                                timeline.seek_request = Some(t);
                            }
                            DragState::ZoomBar { kind, initial_start, initial_end, .. } => {
                                let ratio = ((pos.x - zoom_bar_rect.min.x) / bar_w).clamp(0.0, 1.0) as f64;
                                let mouse_time = ratio * total;
                                let vw_f64 = view_w as f64;

                                match kind {
                                    ZoomBarKind::Body => {
                                        let v_dur = initial_end - initial_start;
                                        let ns = (mouse_time - v_dur / 2.0).clamp(0.0, total - v_dur);
                                        self.scroll_offset = (ns * self.pixels_per_second) as f32;
                                        self.clamp_scroll(tracks_rect, timeline);
                                    }
                                    ZoomBarKind::LeftEdge => {
                                        let ns = mouse_time.clamp(0.0, initial_end - min_v_dur);
                                        let nd = (initial_end - ns).max(min_v_dur);
                                        self.pixels_per_second = (vw_f64 / nd).clamp(20.0, 500.0);
                                        self.scroll_offset = (ns * self.pixels_per_second) as f32;
                                        self.clamp_scroll(tracks_rect, timeline);
                                    }
                                    ZoomBarKind::RightEdge => {
                                        let ne = mouse_time.clamp(initial_start + min_v_dur, total);
                                        let nd = (ne - initial_start).max(min_v_dur);
                                        self.pixels_per_second = (vw_f64 / nd).clamp(20.0, 500.0);
                                        self.scroll_offset = (initial_start * self.pixels_per_second) as f32;
                                        self.clamp_scroll(tracks_rect, timeline);
                                    }
                                }
                            }
                            DragState::ClipBody(cid) => {
                                let dx = pos.x - self.drag_mouse_start_x;
                                let dt = dx as f64 / self.pixels_per_second;
                                let ns = (self.drag_orig_start + dt).max(0.0);
                                let snapped = if self.snap_enabled {
                                    self.snap_time(ns, tracks_rect, timeline)
                                } else {
                                    ns
                                };
                                if let Some(clip) = timeline.clips.get_mut(*cid) {
                                    clip.start = snapped;
                                }
                            }
                            DragState::ClipLeftEdge(cid) => {
                                let dx = pos.x - self.drag_mouse_start_x;
                                let dt = dx as f64 / self.pixels_per_second;
                                let ns = (self.drag_orig_start + dt).max(0.0);
                                let snapped = if self.snap_enabled {
                                    self.snap_time(ns, tracks_rect, timeline)
                                } else {
                                    ns
                                };
                                let ds = snapped - self.drag_orig_start;
                                let nd = (self.drag_orig_duration - ds).max(0.1);
                                if let Some(clip) = timeline.clips.get_mut(*cid) {
                                    clip.start = snapped;
                                    clip.duration = nd;
                                }
                            }
                            DragState::ClipRightEdge(cid) => {
                                let dx = pos.x - self.drag_mouse_start_x;
                                let dt = dx as f64 / self.pixels_per_second;
                                let ne = self.drag_orig_start + self.drag_orig_duration + dt;
                                let snapped_end = if self.snap_enabled {
                                    self.snap_time(ne, tracks_rect, timeline)
                                } else {
                                    ne
                                };
                                if let Some(clip) = timeline.clips.get_mut(*cid) {
                                    clip.duration = (snapped_end - clip.start).max(0.1);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if response.drag_stopped() {
                    let needs_rebuild = !matches!(self.drag_state, DragState::Playhead | DragState::ZoomBar { .. });
                    if needs_rebuild {
                        timeline.rebuild_clip_index();
                        timeline.recompute_bounds();
                    }
                    self.drag_state = DragState::None;
                }
            }
        }
    }

    // ── Hit helpers ─────────────────────────────────────────────────────
    fn hit_clip_edge(
        &self,
        pos: Pos2,
        rect: Rect,
        timeline: &Timeline,
        track_indices: &HashMap<TrackId, usize>,
    ) -> Option<(ClipId, ClipEdge)> {
        const EDGE_PX: f32 = 6.0;
        for clip in timeline.clips.values() {
            let Some(&ti) = track_indices.get(&clip.track) else {
                continue;
            };
            let x1 = self.time_to_x(clip.start, rect);
            let x2 = self.time_to_x(clip.start + clip.duration, rect);
            let y = rect.min.y + ti as f32 * self.track_height + 2.0;
            let cr = Rect::from_min_size(
                Pos2::new(x1, y),
                Vec2::new((x2 - x1).max(2.0), self.track_height - 4.0),
            );
            // Only hit edges when the vertical is within the clip's track
            if pos.y >= cr.min.y && pos.y <= cr.max.y {
                if (pos.x - x1).abs() < EDGE_PX {
                    return Some((clip.id, ClipEdge::Left));
                }
                if (pos.x - x2).abs() < EDGE_PX {
                    return Some((clip.id, ClipEdge::Right));
                }
            }
        }
        None
    }

    fn hit_clip_body(
        &self,
        pos: Pos2,
        rect: Rect,
        timeline: &Timeline,
        track_indices: &HashMap<TrackId, usize>,
    ) -> Option<ClipId> {
        // Iterate in reverse so topmost (last-drawn) clip wins
        let clips: Vec<_> = timeline.clips.values().collect();
        for clip in clips.into_iter().rev() {
            let Some(&ti) = track_indices.get(&clip.track) else {
                continue;
            };
            let x1 = self.time_to_x(clip.start, rect);
            let x2 = self.time_to_x(clip.start + clip.duration, rect);
            let y = rect.min.y + ti as f32 * self.track_height + 2.0;
            let cr = Rect::from_min_size(
                Pos2::new(x1, y),
                Vec2::new((x2 - x1).max(2.0), self.track_height - 4.0),
            );
            // Exclude edges (handled by hit_clip_edge)
            if cr.contains(pos) && (pos.x - x1).abs() > 6.0 && (pos.x - x2).abs() > 6.0 {
                return Some(clip.id);
            }
        }
        None
    }

    // ── Snap ────────────────────────────────────────────────────────────
    fn snap_time(&self, time: f64, _rect: Rect, timeline: &Timeline) -> f64 {
        let thresh = self.snap_threshold_pixels as f64 / self.pixels_per_second;

        // Playhead
        if (time - timeline.current_time).abs() < thresh {
            return timeline.current_time;
        }

        // Clip edges
        for clip in timeline.clips.values() {
            if (time - clip.start).abs() < thresh {
                return clip.start;
            }
            if (time - clip.end()).abs() < thresh {
                return clip.end();
            }
        }

        // Keyframes
        for &kf in timeline.keyframes.keys() {
            if (time - kf.0).abs() < thresh {
                return kf.0;
            }
        }

        // Rounded seconds
        let r = time.round();
        if (time - r).abs() < thresh {
            return r;
        }

        time
    }

    // ── Coordinate helpers ──────────────────────────────────────────────
    fn time_to_x(&self, time: f64, rect: Rect) -> f32 {
        rect.min.x + (time * self.pixels_per_second) as f32 - self.scroll_offset
    }

    fn x_to_time(&self, x: f32, rect: Rect) -> f64 {
        ((x - rect.min.x + self.scroll_offset) as f64) / self.pixels_per_second
    }

    fn tick_intervals(&self, rect: Rect) -> (f64, f64) {
        let visible = rect.width() as f64 / self.pixels_per_second;
        let raw_major = (visible / 6.0).max(0.01);
        let major = nice_round(raw_major);
        let minor = (major / 5.0).max(0.01);
        (major, minor)
    }

    fn clamp_scroll(&mut self, rect: Rect, timeline: &Timeline) {
        let content_w = (timeline.cached_duration * self.pixels_per_second) as f32;
        let view_w = rect.width();
        if content_w > view_w {
            let max_off = content_w - view_w;
            self.scroll_offset = self.scroll_offset.min(max_off);
        } else {
            self.scroll_offset = 0.0;
        }
        self.scroll_offset = self.scroll_offset.max(0.0);
    }
}

// ── Small helpers ──────────────────────────────────────────────────────
enum ClipEdge {
    Left,
    Right,
}

fn nice_round(raw: f64) -> f64 {
    let exp = raw.log10().floor();
    let frac = raw / 10.0_f64.powf(exp);
    let nice = if frac < 1.5 {
        1.0
    } else if frac < 3.5 {
        2.0
    } else if frac < 7.5 {
        5.0
    } else {
        10.0
    };
    nice * 10.0_f64.powf(exp)
}

fn lighten(c: Color32) -> Color32 {
    Color32::from_rgb(
        (c.r() as u16 + 50).min(255) as u8,
        (c.g() as u16 + 50).min(255) as u8,
        (c.b() as u16 + 50).min(255) as u8,
    )
}

fn clip_color(payload: &ClipPayload) -> Color32 {
    match payload {
        ClipPayload::Animation(_) => CLR_ANIM,
        ClipPayload::Audio { .. } => CLR_AUDIO,
        ClipPayload::Wait => CLR_WAIT,
        ClipPayload::Marker(_) => CLR_MARKER,
        ClipPayload::Breakpoint => CLR_BREAKPOINT,
        ClipPayload::SegmentStart(_) => CLR_SEGMENT,
    }
}

fn clip_label(payload: &ClipPayload) -> String {
    match payload {
        ClipPayload::Animation(a) => {
            let n = match &a.lens {
                PropertyLensSpec::Translation { .. } => "Move",
                PropertyLensSpec::Rotation { .. } => "Rotate",
                PropertyLensSpec::Scale { .. } => "Scale",
                PropertyLensSpec::Opacity { .. } => "Fade",
                PropertyLensSpec::FillColor { .. } => "Fill",
                PropertyLensSpec::StrokeColor { .. } => "Stroke",
                PropertyLensSpec::StrokeWidth { .. } => "StW",
                PropertyLensSpec::PathCompletion { .. } => "Draw",
                PropertyLensSpec::FillDrawProgress { .. } => "FillIn",
                PropertyLensSpec::CameraPosition { .. } => "CamPos",
                PropertyLensSpec::CameraRotation { .. } => "CamRot",
                PropertyLensSpec::CameraZoom { .. } => "Zoom",
                PropertyLensSpec::PathFollow { .. } => "Follow",
                PropertyLensSpec::Custom { type_name, .. } => type_name.as_str(),
            };
            format!("Anim:{}", n)
        }
        ClipPayload::Audio { source, .. } => {
            if source.len() > 18 {
                format!("Audio:{}…", &source[..15])
            } else {
                format!("Audio:{}", source)
            }
        }
        ClipPayload::Wait => "Wait".into(),
        ClipPayload::Marker(s) => format!("Marker:{}", s),
        ClipPayload::Breakpoint => "BP".into(),
        ClipPayload::SegmentStart(s) => format!("Seg:{}", s),
    }
}
