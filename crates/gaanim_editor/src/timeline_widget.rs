use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use gaanim_core::id::ObjectId;
use gaanim_timeline::clip::{ClipId, ClipPayload, PropertyLensSpec, TrackId};
use gaanim_timeline::timeline::Timeline;
use std::collections::{HashMap, HashSet};

// ── Color palette (Premiere-inspired) ─────────────────────────────────
const BG: Color32 = Color32::from_rgb(22, 22, 24);
const TRACK_EVEN: Color32 = Color32::from_rgb(32, 32, 36);
const TRACK_ODD: Color32 = Color32::from_rgb(28, 28, 32);
const TRACK_HOVER: Color32 = Color32::from_rgb(42, 42, 48);
const TRACK_SELECTED: Color32 = Color32::from_rgb(38, 52, 78);
const RULER_BG: Color32 = Color32::from_rgb(36, 36, 40);
const HEADER_BG: Color32 = Color32::from_rgb(30, 30, 34);
const GRID_MAJOR: Color32 = Color32::from_rgb(60, 60, 66);
const GRID_MINOR: Color32 = Color32::from_rgb(44, 44, 50);
const PLAYHEAD: Color32 = Color32::from_rgb(255, 70, 70);
const SELECTION: Color32 = Color32::from_rgb(75, 140, 255);
const SELECTION_ACCENT: Color32 = Color32::from_rgb(75, 140, 255);
const TEXT: Color32 = Color32::from_rgb(210, 210, 215);
const TEXT_MUTED: Color32 = Color32::from_rgb(150, 150, 160);

const TEXT_LABEL: Color32 = Color32::from_rgb(240, 240, 240);

const CLR_AUDIO: Color32 = Color32::from_rgb(70, 180, 90);
const CLR_WAIT: Color32 = Color32::from_rgb(100, 100, 100);
const CLR_MARKER: Color32 = Color32::from_rgb(220, 200, 60);
const CLR_BREAKPOINT: Color32 = Color32::from_rgb(220, 80, 80);
const CLR_SEGMENT: Color32 = Color32::from_rgb(180, 100, 200);
const CLR_KEYFRAME: Color32 = Color32::from_rgb(255, 200, 80);
const CLR_UNGROUP: Color32 = Color32::from_rgb(230, 100, 50);

const PROP_ROW_BG: Color32 = Color32::from_rgb(32, 32, 32);
const PROP_LABEL: Color32 = Color32::from_rgb(160, 160, 160);
const PROP_VALUE: Color32 = Color32::from_rgb(220, 220, 220);
const PROP_SEPARATOR: Color32 = Color32::from_rgb(50, 50, 50);

const CLR_SIGNAL_ACCENT: Color32 = Color32::from_rgb(180, 180, 50);
const CLR_UPDATER_BADGE: Color32 = Color32::from_rgb(100, 200, 255);
const CLR_DECIMAL_ACCENT: Color32 = Color32::from_rgb(255, 160, 80);
const BASE_PROP_COUNT: usize = 6;

pub struct PropertyValues {
    pub pos_x: f64,
    pub pos_y: f64,
    pub pos_z: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub scale_z: f64,
    pub rotation_deg: f64,
    pub fill_label: String,
    pub stroke_label: String,
    pub stroke_width: f64,
    pub opacity: f32,
}

// ── Track row layout (computed dynamically) ────────────────────────────
struct TrackLayout {
    header_y: f32,
    prop_count: usize,
    depth: u8,
    has_signal: bool,
    has_updater: bool,
    has_decimal: bool,
}

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
    Divider {
        initial_mouse_x: f32,
        initial_width: f32,
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
    pub label_width: f32,
    pub selected_clip: Option<ClipId>,
    pub selected_track: Option<TrackId>,
    pub snap_enabled: bool,
    pub snap_threshold_pixels: f32,
    pub show_scene_header: bool,
    pub scene_expanded: bool,
    pub expanded_tracks: HashSet<TrackId>,
    pub property_row_height: f32,
    pub scroll_y: f32,
    known_tracks: HashSet<TrackId>,

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
            label_width: 150.0,
            selected_clip: None,
            selected_track: None,
            snap_enabled: true,
            snap_threshold_pixels: 8.0,
            show_scene_header: true,
            scene_expanded: true,
            expanded_tracks: HashSet::new(),
            property_row_height: 20.0,
            scroll_y: 0.0,
            known_tracks: HashSet::new(),
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
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        timeline: &mut Timeline,
        property_values: &HashMap<ObjectId, PropertyValues>,
        group_children: &HashMap<TrackId, Vec<TrackId>>,
        signal_values: &HashMap<ObjectId, f64>,
        updater_entities: &HashSet<ObjectId>,
        decimal_values: &HashMap<ObjectId, f64>,
    ) {
        let header_rows = if self.show_scene_header { 1 } else { 0 };
        let mobject_tracks: Vec<TrackId> = timeline
            .tracks
            .iter()
            .filter(|(_, t)| t.name != "Main Graphics")
            .map(|(id, _)| id)
            .collect();
        // Track known tracks (collapsed by default — user expands manually)
        for &tid in &mobject_tracks {
            self.known_tracks.insert(tid);
        }
        let tracks_visible = !self.show_scene_header || self.scene_expanded;

        let (track_layouts, content_height) = self.compute_track_layouts(
            &mobject_tracks,
            tracks_visible,
            header_rows,
            group_children,
            timeline,
            signal_values,
            updater_entities,
            decimal_values,
        );

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

        // ── Canvas (zoom bar + ruler + tracks) — fills remaining height ──
        let canvas_height = (ui.available_height()).max(100.0);
        let canvas_size = Vec2::new(ui.available_width(), canvas_height);
        let (canvas_rect, response) = ui.allocate_exact_size(canvas_size, Sense::click_and_drag());

        let lw = self.label_width;
        let clips_left = (canvas_rect.min.x + lw).min(canvas_rect.max.x - 50.0);
        let clips_w = (canvas_rect.max.x - clips_left).max(50.0);

        let zoom_bar_rect = Rect::from_min_size(
            Pos2::new(clips_left, canvas_rect.min.y),
            Vec2::new(clips_w, self.zoom_bar_height),
        );
        let ruler_rect = Rect::from_min_size(
            Pos2::new(clips_left, zoom_bar_rect.max.y),
            Vec2::new(clips_w, self.ruler_height),
        );
        // Tracks viewport: visible area (fixed), content extends beyond via scroll_y
        let tracks_visible_h = (canvas_rect.max.y - ruler_rect.max.y).max(0.0);
        let tracks_rect = Rect::from_min_size(
            Pos2::new(canvas_rect.min.x, ruler_rect.max.y),
            Vec2::new(canvas_rect.width(), tracks_visible_h),
        );
        let clips_rect = Rect::from_min_size(
            Pos2::new(clips_left, tracks_rect.min.y),
            Vec2::new(clips_w, tracks_visible_h),
        );
        let label_rect = Rect::from_min_size(tracks_rect.min, Vec2::new(lw, tracks_visible_h));
        // Clamp vertical scroll
        let max_scroll_y = (content_height - tracks_visible_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll_y);

        self.last_canvas_width = canvas_rect.width();

        // ── Hover state ──────────────────────────────────────────────────
        let hover_pos = response.hover_pos();
        let hovered_clip = hover_pos.and_then(|pos| {
            if !clips_rect.contains(pos) {
                return None;
            }
            let on_edge = self.hit_clip_edge(pos, clips_rect, timeline, &track_layouts);
            if on_edge.is_some() {
                return None;
            }
            self.hit_clip_body(pos, clips_rect, timeline, &track_layouts)
        });
        self.hovered_clip_info = hovered_clip.and_then(|cid| {
            timeline.clips.get(cid).map(|c| {
                let label = clip_label(&c.payload);
                format!("{} | {:.2}s\u{2192}{:.2}s", label, c.start, c.end())
            })
        });

        let on_edge = hover_pos.is_some_and(|pos| {
            self.hit_clip_edge(pos, clips_rect, timeline, &track_layouts)
                .is_some()
        });

        // Hovered track (for highlight)
        let hovered_track = hover_pos.and_then(|pos| {
            if !tracks_rect.contains(pos) {
                return None;
            }
            for (tid, layout) in &track_layouts {
                let y0 = tracks_rect.min.y + layout.header_y - self.scroll_y;
                let h = self.track_height + layout.prop_count as f32 * self.property_row_height;
                if pos.y >= y0 && pos.y < y0 + h {
                    return Some(*tid);
                }
            }
            None
        });

        // Input first so dragging feels responsive
        self.handle_input(
            ui,
            &response,
            zoom_bar_rect,
            ruler_rect,
            clips_rect,
            tracks_rect,
            label_rect,
            timeline,
            &track_layouts,
            on_edge,
            hover_pos,
            group_children,
        );

        // Paint
        let mut p = ui.painter_at(canvas_rect);
        let cp = ui.painter_at(clips_rect); // clips-region painter for clean clipping
        self.paint_zoom_bar(&p, zoom_bar_rect, clips_rect, timeline);
        self.paint_ruler(&p, ruler_rect);
        self.paint_track_bg(
            &p,
            tracks_rect,
            label_rect,
            timeline,
            &track_layouts,
            hovered_track,
            self.selected_track,
        );
        self.paint_grid(&cp, clips_rect, timeline);
        self.paint_clips(
            &cp,
            clips_rect,
            timeline,
            &track_layouts,
            hover_pos,
            group_children,
        );
        // Scene header: clip to its own row so it doesn't overlap track labels
        let header_clip = Rect::from_min_size(
            tracks_rect.min,
            Vec2::new(tracks_rect.width(), self.track_height),
        );
        p.set_clip_rect(header_clip);
        self.paint_scene_header(&p, tracks_rect, timeline);
        p.set_clip_rect(canvas_rect);
        {
            let mut lp = ui.painter_at(label_rect);
            lp.set_clip_rect(label_rect);
            self.paint_track_labels(
                &lp,
                label_rect,
                timeline,
                &track_layouts,
                group_children,
                hovered_track,
                self.selected_track,
            );
        }
        self.paint_divider(&p, label_rect, tracks_rect);
        self.paint_vertical_scrollbar(&p, tracks_rect, content_height);
        // Empty state placeholder (Premiere-like)
        if timeline.clips.is_empty() && self.scene_expanded {
            let msg_rect = Rect::from_center_size(clips_rect.center(), Vec2::new(280.0, 40.0));
            p.rect_filled(
                msg_rect,
                6u8,
                Color32::from_rgba_premultiplied(40, 40, 46, 220),
            );
            p.rect_stroke(
                msg_rect,
                6u8,
                Stroke::new(1.0, Color32::from_rgb(70, 70, 80)),
                StrokeKind::Inside,
            );
            p.text(
                msg_rect.center(),
                Align2::CENTER_CENTER,
                "No clips — add animations in Python",
                FontId::proportional(11.0),
                TEXT_MUTED,
            );
        }
        self.paint_keyframes(&p, ruler_rect, timeline);
        self.paint_playhead(&cp, clips_rect, ruler_rect.min.y, timeline);
        self.paint_track_properties(
            &p,
            clips_rect,
            label_rect,
            timeline,
            property_values,
            &track_layouts,
            signal_values,
            decimal_values,
        );

        // ── Auto-scroll during playback ─────────────────────────────────
        if timeline.is_playing {
            let px = self.time_to_x(timeline.current_time, clips_rect);
            let margin = 80.0;
            if px > clips_rect.max.x - margin {
                self.scroll_offset += px - (clips_rect.max.x - margin);
                self.clamp_scroll(clips_rect, timeline);
            } else if px < clips_rect.min.x + 20.0 {
                self.scroll_offset -= (clips_rect.min.x + 20.0 - px).min(self.scroll_offset);
            }
        }
    }

    fn compute_track_layouts(
        &self,
        tracks: &[TrackId],
        visible: bool,
        header_rows: usize,
        group_children: &HashMap<TrackId, Vec<TrackId>>,
        timeline: &Timeline,
        signal_values: &HashMap<ObjectId, f64>,
        updater_entities: &HashSet<ObjectId>,
        decimal_values: &HashMap<ObjectId, f64>,
    ) -> (HashMap<TrackId, TrackLayout>, f32) {
        let mut layouts = HashMap::new();
        let mut total = header_rows as f32 * self.track_height;
        if !visible {
            return (layouts, total.max(self.track_height));
        }

        // Collect all children tracks so we skip them in the main loop
        let child_set: HashSet<TrackId> = group_children
            .values()
            .flat_map(|v| v.iter().copied())
            .collect();

        let mut add_track = |tid: &TrackId, total: &mut f32, depth: u8| {
            let has_object = timeline
                .tracks
                .get(*tid)
                .and_then(|t| t.object_id)
                .is_some();
            let (has_signal, has_updater, has_decimal) =
                if let Some(obj_id) = timeline.tracks.get(*tid).and_then(|t| t.object_id) {
                    (
                        signal_values.contains_key(&obj_id),
                        updater_entities.contains(&obj_id),
                        decimal_values.contains_key(&obj_id),
                    )
                } else {
                    (false, false, false)
                };
            let extra = (has_signal as usize) + (has_updater as usize) + (has_decimal as usize);
            let prop_count = if self.expanded_tracks.contains(tid) && has_object {
                BASE_PROP_COUNT + extra
            } else {
                0
            };
            layouts.insert(
                *tid,
                TrackLayout {
                    header_y: *total,
                    prop_count,
                    depth,
                    has_signal,
                    has_updater,
                    has_decimal,
                },
            );
            *total += self.track_height + prop_count as f32 * self.property_row_height;
        };

        for track_id in tracks {
            if child_set.contains(track_id) {
                continue; // will be rendered under its group parent
            }
            add_track(track_id, &mut total, 0);

            // If this is a group track and it's expanded, insert children
            if let Some(children) = group_children.get(track_id)
                && self.expanded_tracks.contains(track_id)
            {
                for child_id in children {
                    add_track(child_id, &mut total, 1);
                }
            }
        }
        (layouts, total.max(self.track_height))
    }

    // ── Header (Premiere-style) ───────────────────────────────────────
    fn paint_header(&mut self, ui: &mut egui::Ui, timeline: &mut Timeline) {
        // Style helpers
        let btn_fill = |active: bool| {
            if active {
                Color32::from_rgb(55, 90, 140)
            } else {
                Color32::from_rgb(45, 45, 48)
            }
        };
        let btn_stroke = Color32::from_rgb(70, 70, 75);
        let mk_btn = |label: &str, active: bool| {
            egui::Button::new(egui::RichText::new(label).size(12.0).color(TEXT_LABEL))
                .fill(btn_fill(active))
                .stroke(egui::Stroke::new(1.0, btn_stroke))
                .corner_radius(4.0)
                .min_size(Vec2::new(28.0, 22.0))
        };

        // ── Left: transport ───────────────────────────────────────────────
        let play_active = timeline.is_playing;
        if ui
            .add(mk_btn(if play_active { "⏸" } else { "▶" }, play_active))
            .on_hover_text("Play / Pause (Space)")
            .clicked()
        {
            timeline.is_playing = !timeline.is_playing;
        }
        if ui
            .add(mk_btn("⏮", false))
            .on_hover_text("Go to start (Home)")
            .clicked()
        {
            timeline.is_playing = false;
            timeline.seek_request = Some(0.0);
        }
        if ui
            .add(mk_btn("⏭", false))
            .on_hover_text("Go to end (End)")
            .clicked()
        {
            timeline.is_playing = false;
            timeline.seek_request = Some(timeline.cached_duration);
        }
        ui.add_space(6.0);
        let loop_active = timeline.loop_range.is_some();
        if ui
            .add(mk_btn(if loop_active { "🔁" } else { "↻" }, loop_active))
            .on_hover_text(if loop_active { "Loop: ON" } else { "Loop: OFF" })
            .clicked()
        {
            if loop_active {
                timeline.loop_range = None;
            } else {
                let end = timeline.cached_duration.max(0.01);
                timeline.loop_range = Some((0.0, end));
            }
        }

        ui.separator();

        // ── Center: timecode + hover info ──────────────────────────────────
        let time_text = if let Some(info) = &self.hovered_clip_info {
            egui::RichText::new(info.clone())
                .size(11.0)
                .color(Color32::from_rgb(180, 220, 255))
        } else {
            egui::RichText::new(format!(
                "{:.2}s / {:.2}s  •  {} clips",
                timeline.current_time,
                timeline.cached_duration,
                timeline.clips.len()
            ))
            .size(11.0)
            .color(TEXT)
        };
        ui.label(time_text);

        // ── Right: view controls ───────────────────────────────────────────
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Fit
            if ui
                .add(mk_btn("Fit", false))
                .on_hover_text("Fit timeline to view (double-click zoom bar)")
                .clicked()
            {
                let dur = timeline.cached_duration.max(1.0);
                let w = self.last_canvas_width.max(100.0);
                self.pixels_per_second = (w as f64 / dur).clamp(20.0, 500.0);
                self.scroll_offset = 0.0;
            }
            ui.add_space(8.0);
            // Zoom slider (premiere-like)
            let zoom_resp = ui.add(
                egui::Slider::new(&mut self.pixels_per_second, 20.0..=500.0)
                    .logarithmic(true)
                    .show_value(false)
                    .handle_shape(egui::style::HandleShape::Circle)
                    .text(""),
            );
            if zoom_resp.on_hover_text("Zoom (Ctrl+Scroll)").hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            // Snap toggle
            let snap_label = if self.snap_enabled { "🧲" } else { "○" };
            if ui
                .add(mk_btn(snap_label, self.snap_enabled))
                .on_hover_text(if self.snap_enabled {
                    "Snap: ON (magnet to playhead/clips)"
                } else {
                    "Snap: OFF"
                })
                .clicked()
            {
                self.snap_enabled = !self.snap_enabled;
            }
        });
    }

    // ── Ruler (Premiere-like: timecode + ticks) ───────────────────────
    fn paint_ruler(&self, p: &egui::Painter, rect: Rect) {
        // Background with bottom border
        p.rect_filled(rect, 0u8, RULER_BG);
        p.line_segment(
            [
                Pos2::new(rect.min.x, rect.max.y),
                Pos2::new(rect.max.x, rect.max.y),
            ],
            Stroke::new(1.0, Color32::from_rgb(55, 55, 60)),
        );

        let (major, minor) = self.tick_intervals(rect);
        let t0 = self.x_to_time(rect.min.x, rect);
        let t1 = self.x_to_time(rect.max.x, rect);

        let mut t = (t0 / minor).ceil() * minor;
        while t <= t1 {
            let x = self.time_to_x(t, rect);
            let is_major = (t / major).fract().abs() < 1e-9
                || ((t / major) - (t / major).round()).abs() < 1e-9;

            let h = if is_major {
                rect.height() - 4.0
            } else {
                rect.height() * 0.38
            };
            let c = if is_major {
                Color32::from_rgb(210, 210, 215)
            } else {
                Color32::from_rgb(110, 110, 120)
            };
            p.line_segment(
                [Pos2::new(x, rect.max.y - h), Pos2::new(x, rect.max.y)],
                Stroke::new(if is_major { 1.0 } else { 0.7 }, c),
            );

            if is_major {
                // Timecode: mm:ss.d
                let secs = t.max(0.0);
                let mins = (secs / 60.0).floor() as i32;
                let secs_rem = secs % 60.0;
                let label = if mins > 0 {
                    format!("{}:{:04.1}", mins, secs_rem)
                } else {
                    format!("{:.1}s", secs)
                };
                p.text(
                    Pos2::new(x + 4.0, rect.min.y + 2.0),
                    Align2::LEFT_TOP,
                    label,
                    FontId::proportional(10.0),
                    Color32::from_rgb(190, 190, 200),
                );
            }

            t += minor;
        }
        // Top highlight
        p.line_segment(
            [
                Pos2::new(rect.min.x, rect.min.y),
                Pos2::new(rect.max.x, rect.min.y),
            ],
            Stroke::new(1.0, Color32::from_white_alpha(8)),
        );
    }

    // ── Track backgrounds (Premiere-like alternating + hover/selection) ──
    fn paint_track_bg(
        &self,
        p: &egui::Painter,
        rect: Rect,
        label_rect: Rect,
        _timeline: &Timeline,
        track_layouts: &HashMap<TrackId, TrackLayout>,
        hovered: Option<TrackId>,
        selected: Option<TrackId>,
    ) {
        p.rect_filled(rect, 0u8, BG);
        p.rect_filled(label_rect, 0u8, Color32::from_rgb(28, 28, 30));

        if self.show_scene_header {
            let hdr = Rect::from_min_size(
                Pos2::new(rect.min.x, rect.min.y),
                Vec2::new(rect.width(), self.track_height),
            );
            // Subtle header with bottom border
            p.rect_filled(hdr, 0u8, Color32::from_rgb(32, 32, 36));
            p.line_segment(
                [
                    Pos2::new(hdr.min.x, hdr.max.y),
                    Pos2::new(hdr.max.x, hdr.max.y),
                ],
                Stroke::new(1.0, Color32::from_rgb(50, 50, 56)),
            );
        }

        if !self.scene_expanded {
            return;
        }

        // Sort for stable alternating
        let mut sorted: Vec<(&TrackId, &TrackLayout)> = track_layouts.iter().collect();
        sorted.sort_by_key(|(_, l)| l.header_y as i32);

        for (idx, (tid, layout)) in sorted.iter().enumerate() {
            let is_hovered = hovered == Some(**tid);
            let is_selected = selected == Some(**tid);
            let base = if idx % 2 == 0 { TRACK_EVEN } else { TRACK_ODD };
            let bg = if is_selected {
                TRACK_SELECTED
            } else if is_hovered {
                TRACK_HOVER
            } else {
                base
            };
            let y = rect.min.y + layout.header_y - self.scroll_y;

            // Left label bg with accent border when selected
            let lb = Rect::from_min_size(
                label_rect.min + Vec2::new(0.0, layout.header_y - self.scroll_y),
                Vec2::new(label_rect.width(), self.track_height),
            );
            p.rect_filled(lb, 2u8, bg);
            if is_selected {
                p.rect_stroke(
                    lb,
                    2u8,
                    Stroke::new(2.0, SELECTION_ACCENT),
                    StrokeKind::Inside,
                );
                // Left accent bar
                p.rect_filled(
                    Rect::from_min_size(lb.min, Vec2::new(3.0, lb.height())),
                    0u8,
                    SELECTION_ACCENT,
                );
            } else if is_hovered {
                p.rect_stroke(
                    lb,
                    2u8,
                    Stroke::new(1.0, Color32::from_rgb(60, 60, 70)),
                    StrokeKind::Inside,
                );
            }

            let cb = Rect::from_min_size(
                Pos2::new(label_rect.max.x, y),
                Vec2::new(rect.max.x - label_rect.max.x, self.track_height),
            );
            p.rect_filled(cb, 2u8, bg);
            if is_selected {
                p.rect_stroke(
                    cb,
                    2u8,
                    Stroke::new(1.0, SELECTION_ACCENT.linear_multiply(0.6)),
                    StrokeKind::Inside,
                );
            }

            for i in 0..layout.prop_count {
                let py = y + self.track_height + i as f32 * self.property_row_height;
                let plb = Rect::from_min_size(
                    Pos2::new(label_rect.min.x, py),
                    Vec2::new(label_rect.width(), self.property_row_height),
                );
                let pcb = Rect::from_min_size(
                    Pos2::new(label_rect.max.x, py),
                    Vec2::new(rect.max.x - label_rect.max.x, self.property_row_height),
                );
                // Property rows slightly darker
                let prop_bg = if is_hovered {
                    Color32::from_rgb(38, 38, 44)
                } else {
                    PROP_ROW_BG
                };
                p.rect_filled(plb, 0u8, prop_bg);
                p.rect_filled(pcb, 0u8, prop_bg);
                p.line_segment(
                    [Pos2::new(plb.min.x, py), Pos2::new(pcb.max.x, py)],
                    Stroke::new(0.5, PROP_SEPARATOR),
                );
            }
            // Bottom hairline
            let bottom_y =
                y + self.track_height + layout.prop_count as f32 * self.property_row_height;
            p.line_segment(
                [
                    Pos2::new(rect.min.x, bottom_y),
                    Pos2::new(rect.max.x, bottom_y),
                ],
                Stroke::new(0.5, Color32::from_rgb(45, 45, 50)),
            );
        }
    }

    // ── Scene header text (drawn AFTER clips) ─────────────────────────
    fn paint_scene_header(&self, p: &egui::Painter, rect: Rect, timeline: &Timeline) {
        if !self.show_scene_header {
            return;
        }
        // Scene header stays fixed at the top of the tracks viewport
        let arrow = if self.scene_expanded { "▾" } else { "▸" };
        let count = timeline.tracks.len();
        p.text(
            Pos2::new(rect.min.x + 6.0, rect.min.y + self.track_height / 2.0),
            Align2::LEFT_CENTER,
            format!("{} Scene  ({})", arrow, count),
            FontId::proportional(12.0),
            Color32::from_rgb(180, 180, 180),
        );
    }

    // ── Track labels (Premiere-like: accent bar + type icon + badges) ──
    fn paint_track_labels(
        &self,
        p: &egui::Painter,
        label_rect: Rect,
        timeline: &Timeline,
        track_layouts: &HashMap<TrackId, TrackLayout>,
        group_children: &HashMap<TrackId, Vec<TrackId>>,
        hovered: Option<TrackId>,
        selected: Option<TrackId>,
    ) {
        if !self.scene_expanded {
            return;
        }
        for (track_id, layout) in track_layouts {
            let y = label_rect.min.y + layout.header_y - self.scroll_y;
            // Cull offscreen
            if y + self.track_height < label_rect.min.y - 10.0 || y > label_rect.max.y + 10.0 {
                continue;
            }
            let Some(t) = timeline.tracks.get(*track_id) else {
                continue;
            };
            let is_group = group_children.contains_key(track_id);
            let has_props = timeline
                .tracks
                .get(*track_id)
                .and_then(|t| t.object_id)
                .is_some();
            let is_hovered = hovered == Some(*track_id);
            let is_selected = selected == Some(*track_id);
            let indent = layout.depth as f32 * 16.0;

            // Accent color from first clip (track type)
            let accent = timeline
                .clips
                .values()
                .find(|c| c.track == *track_id)
                .map(|c| clip_color(&c.payload))
                .unwrap_or(Color32::from_rgb(80, 80, 90));
            // Left accent bar
            let bar_rect = Rect::from_min_size(
                Pos2::new(label_rect.min.x, y + 2.0),
                Vec2::new(3.0, self.track_height - 4.0),
            );
            p.rect_filled(bar_rect, 1u8, accent);

            let label = if is_group || has_props {
                let expanded = self.expanded_tracks.contains(track_id);
                let arrow = if expanded { "▾" } else { "▸" };
                format!("{} {}", arrow, t.name)
            } else {
                t.name.clone()
            };
            let text_color = if is_selected {
                Color32::WHITE
            } else if is_hovered {
                TEXT_LABEL
            } else {
                TEXT
            };
            let text_x = label_rect.min.x + 10.0 + indent;
            p.text(
                Pos2::new(text_x, y + self.track_height / 2.0),
                Align2::LEFT_CENTER,
                &label,
                FontId::proportional(11.0),
                text_color,
            );

            // Right side: badges + visibility/lock placeholders (Premiere-like)
            let mut badge_x = label_rect.max.x - 14.0;
            // Decimal
            if layout.has_decimal {
                p.text(
                    Pos2::new(badge_x, y + self.track_height / 2.0),
                    Align2::RIGHT_CENTER,
                    "№",
                    FontId::proportional(9.0),
                    if is_selected {
                        Color32::WHITE
                    } else {
                        CLR_DECIMAL_ACCENT
                    },
                );
                badge_x -= 16.0;
            }
            if layout.has_updater {
                p.text(
                    Pos2::new(badge_x, y + self.track_height / 2.0),
                    Align2::RIGHT_CENTER,
                    "◷",
                    FontId::proportional(10.0),
                    if is_selected {
                        Color32::WHITE
                    } else {
                        CLR_UPDATER_BADGE
                    },
                );
                badge_x -= 16.0;
            }
            if layout.has_signal {
                p.text(
                    Pos2::new(badge_x, y + self.track_height / 2.0),
                    Align2::RIGHT_CENTER,
                    "◆",
                    FontId::proportional(8.0),
                    if is_selected {
                        Color32::WHITE
                    } else {
                        CLR_SIGNAL_ACCENT
                    },
                );
                badge_x -= 16.0;
            }
            // Visibility eye (always show faint)
            p.text(
                Pos2::new(badge_x, y + self.track_height / 2.0),
                Align2::RIGHT_CENTER,
                "👁",
                FontId::proportional(8.0),
                if is_hovered || is_selected {
                    TEXT_MUTED
                } else {
                    Color32::from_rgb(70, 70, 75)
                },
            );
        }
    }

    fn paint_track_properties(
        &self,
        p: &egui::Painter,
        clips_rect: Rect,
        label_rect: Rect,
        timeline: &Timeline,
        property_values: &HashMap<ObjectId, PropertyValues>,
        track_layouts: &HashMap<TrackId, TrackLayout>,
        signal_values: &HashMap<ObjectId, f64>,
        decimal_values: &HashMap<ObjectId, f64>,
    ) {
        if !self.scene_expanded {
            return;
        }
        let font = FontId::proportional(10.0);
        let indent = 20.0;

        for (track_id, layout) in track_layouts {
            if layout.prop_count == 0 {
                continue;
            }
            let track = match timeline.tracks.get(*track_id) {
                Some(t) => t,
                None => continue,
            };
            let obj_id = match track.object_id {
                Some(id) => id,
                None => continue,
            };
            let vals = match property_values.get(&obj_id) {
                Some(v) => v,
                None => continue,
            };

            let base_y = clips_rect.min.y + layout.header_y - self.scroll_y + self.track_height;

            let base_rows: &[(&str, String, Color32)] = &[
                (
                    "Position",
                    format!("({:.1}, {:.1}, {:.1})", vals.pos_x, vals.pos_y, vals.pos_z),
                    PROP_VALUE,
                ),
                (
                    "Scale",
                    format!(
                        "({:.2}, {:.2}, {:.2})",
                        vals.scale_x, vals.scale_y, vals.scale_z
                    ),
                    PROP_VALUE,
                ),
                (
                    "Rotation",
                    format!("{:.2}\u{b0}", vals.rotation_deg),
                    PROP_VALUE,
                ),
                ("Fill", vals.fill_label.clone(), PROP_VALUE),
                (
                    "Stroke",
                    format!("{}  w:{:.1}", vals.stroke_label, vals.stroke_width),
                    PROP_VALUE,
                ),
                ("Opacity", format!("{:.2}", vals.opacity), PROP_VALUE),
            ];

            let extra_rows: Vec<(&str, String, Color32)> = {
                let mut r = Vec::new();
                if layout.has_signal {
                    if let Some(&val) = signal_values.get(&obj_id) {
                        r.push(("Signal", format!("{:.4}", val), CLR_SIGNAL_ACCENT));
                    }
                }
                if layout.has_updater {
                    r.push(("Updater", "\u{1F504} running".into(), CLR_UPDATER_BADGE));
                }
                if layout.has_decimal {
                    if let Some(&val) = decimal_values.get(&obj_id) {
                        r.push(("Display", format!("{:.4}", val), CLR_DECIMAL_ACCENT));
                    }
                }
                r
            };

            let row_count = base_rows.len() + extra_rows.len();

            for (i, (label, value, color)) in base_rows.iter().chain(extra_rows.iter()).enumerate()
            {
                let y = base_y + i as f32 * self.property_row_height;
                let mid_y = y + self.property_row_height / 2.0;

                p.line_segment(
                    [
                        Pos2::new(label_rect.min.x, y),
                        Pos2::new(clips_rect.max.x, y),
                    ],
                    Stroke::new(0.5, PROP_SEPARATOR),
                );

                p.text(
                    Pos2::new(label_rect.min.x + indent, mid_y),
                    Align2::LEFT_CENTER,
                    *label,
                    font.clone(),
                    PROP_LABEL,
                );

                p.text(
                    Pos2::new(clips_rect.min.x + 6.0, mid_y),
                    Align2::LEFT_CENTER,
                    value.as_str(),
                    font.clone(),
                    *color,
                );
            }

            // Fill remaining rows with separators to maintain consistent look
            for i in row_count..layout.prop_count {
                let y = base_y + i as f32 * self.property_row_height;
                p.line_segment(
                    [
                        Pos2::new(label_rect.min.x, y),
                        Pos2::new(clips_rect.max.x, y),
                    ],
                    Stroke::new(0.5, PROP_SEPARATOR),
                );
            }
        }
    }

    // ── Divider line between labels and clips ─────────────────────────
    fn paint_divider(&self, p: &egui::Painter, label_rect: Rect, tracks_rect: Rect) {
        let div_x = label_rect.max.x;
        if div_x < tracks_rect.max.x {
            p.line_segment(
                [
                    Pos2::new(div_x, tracks_rect.min.y),
                    Pos2::new(div_x, tracks_rect.max.y),
                ],
                Stroke::new(1.0, Color32::from_rgb(80, 80, 80)),
            );
            // Grip dots on divider
            let mid_y = (tracks_rect.min.y + tracks_rect.max.y) / 2.0;
            for dy in [-12.0, -6.0, 0.0, 6.0, 12.0] {
                p.circle_filled(
                    Pos2::new(div_x, mid_y + dy),
                    1.2,
                    Color32::from_rgb(100, 100, 110),
                );
            }
        }
    }

    // ── Vertical scrollbar (when content overflows) ─────────────────────
    fn paint_vertical_scrollbar(&self, p: &egui::Painter, rect: Rect, content_h: f32) {
        let visible_h = rect.height();
        if content_h <= visible_h + 1.0 {
            return;
        }
        let bar_w = 6.0;
        let bar_rect = Rect::from_min_size(
            Pos2::new(rect.max.x - bar_w - 2.0, rect.min.y),
            Vec2::new(bar_w, visible_h),
        );
        // Track
        p.rect_filled(bar_rect, 3u8, Color32::from_rgb(35, 35, 40));
        // Thumb
        let thumb_h = (visible_h * visible_h / content_h).clamp(20.0, visible_h * 0.8);
        let max_scroll = (content_h - visible_h).max(1.0);
        let thumb_y = bar_rect.min.y + (self.scroll_y / max_scroll) * (visible_h - thumb_h);
        let thumb_rect = Rect::from_min_size(
            Pos2::new(bar_rect.min.x + 1.0, thumb_y),
            Vec2::new(bar_w - 2.0, thumb_h),
        );
        p.rect_filled(thumb_rect, 3u8, Color32::from_rgb(90, 90, 100));
        p.rect_stroke(
            thumb_rect,
            3u8,
            Stroke::new(1.0, Color32::from_rgb(120, 120, 130)),
            StrokeKind::Inside,
        );
    }

    // ── Grid ────────────────────────────────────────────────────────────
    fn paint_grid(&self, p: &egui::Painter, rect: Rect, _timeline: &Timeline) {
        if !self.scene_expanded {
            return;
        }
        let (major, minor) = self.tick_intervals(rect);
        let t0 = self.x_to_time(rect.min.x, rect);
        let t1 = self.x_to_time(rect.max.x, rect);

        let mut t = (t0 / minor).ceil() * minor;
        while t <= t1 {
            let x = self.time_to_x(t, rect);
            let is_major = (t / major).fract().abs() < 1e-9
                || ((t / major) - (t / major).round()).abs() < 1e-9;
            p.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(
                    if is_major { 1.0 } else { 0.5 },
                    if is_major { GRID_MAJOR } else { GRID_MINOR },
                ),
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
        track_layouts: &HashMap<TrackId, TrackLayout>,
        hover_pos: Option<Pos2>,
        group_children: &HashMap<TrackId, Vec<TrackId>>,
    ) {
        // Skip all clips when scene is collapsed
        if !self.scene_expanded {
            return;
        }
        for clip in timeline.clips.values() {
            let Some(layout) = track_layouts.get(&clip.track) else {
                continue;
            };

            let x1 = self.time_to_x(clip.start, rect);
            let x2 = self.time_to_x(clip.start + clip.duration, rect);
            let y = rect.min.y + layout.header_y - self.scroll_y + 2.0;
            let ch = self.track_height - 4.0;

            let base_color = clip_color(&clip.payload);
            let cr = Rect::from_min_size(Pos2::new(x1, y), Vec2::new((x2 - x1).max(2.0), ch));
            let is_hovered = hover_pos.is_some_and(|pos| cr.contains(pos));
            let is_selected = self.selected_clip == Some(clip.id);
            // Shadow
            let shadow_rect = cr.translate(Vec2::new(1.0, 1.0));
            p.rect_filled(shadow_rect, 3u8, Color32::from_black_alpha(40));
            // Fill with hover brightening
            let fill = if is_selected {
                lighten(base_color)
            } else if is_hovered {
                lighten(base_color)
            } else {
                base_color
            };
            p.rect_filled(cr, 4u8, fill);
            // Top accent strip (darker)
            let accent = Color32::from_rgba_premultiplied(
                (base_color.r() as f32 * 0.7) as u8,
                (base_color.g() as f32 * 0.7) as u8,
                (base_color.b() as f32 * 0.7) as u8,
                255,
            );
            let top_strip = Rect::from_min_size(cr.min, Vec2::new(cr.width(), 3.0));
            p.rect_filled(top_strip, 2u8, accent);
            // Inner highlight
            p.rect_stroke(
                cr,
                4u8,
                Stroke::new(1.0, Color32::from_white_alpha(22)),
                StrokeKind::Inside,
            );

            // ── Value ramp indicator for SignalFloat clips ──────────
            if let ClipPayload::Animation(anim) = &clip.payload {
                if let PropertyLensSpec::SignalFloat { from, to } = &anim.lens {
                    if cr.width() > 20.0 {
                        let ramp_h = 4.0;
                        let ramp_y = cr.max.y - ramp_h - 2.0;
                        let from_color = if *from < *to {
                            Color32::from_rgb(80, 80, 160)
                        } else {
                            Color32::from_rgb(160, 80, 80)
                        };
                        let to_color = if *from < *to {
                            Color32::from_rgb(255, 200, 80)
                        } else {
                            Color32::from_rgb(200, 100, 100)
                        };
                        let segments = 8;
                        let seg_w = (cr.width() / segments as f32).max(1.0);
                        for s in 0..segments {
                            let t = s as f32 / segments as f32;
                            let t2 = (s + 1) as f32 / segments as f32;
                            let c = lerp_color(from_color, to_color, (t + t2) / 2.0);
                            let sx = cr.min.x + s as f32 * seg_w;
                            p.rect_filled(
                                Rect::from_min_size(
                                    Pos2::new(sx, ramp_y),
                                    Vec2::new(seg_w, ramp_h),
                                ),
                                1u8,
                                c,
                            );
                        }
                    }
                    if is_hovered {
                        let val_label = format!("{:.2}→{:.2}", from, to);
                        p.text(
                            Pos2::new(cr.min.x + 4.0, cr.max.y - 8.0),
                            Align2::LEFT_BOTTOM,
                            val_label,
                            FontId::proportional(8.0),
                            Color32::from_rgb(255, 220, 140),
                        );
                    }
                }
            }

            // Icon + label
            let icon = clip_icon(&clip.payload);
            let label = clip_label(&clip.payload);
            let label_x = cr.min.x + 6.0;
            // Clip text to width
            let available = cr.width() - 12.0;
            if available > 20.0 {
                p.text(
                    Pos2::new(label_x, cr.center().y),
                    Align2::LEFT_CENTER,
                    format!("{} {}", icon, label),
                    FontId::proportional(10.0),
                    TEXT_LABEL,
                );
            }
            // Duration at bottom-right when wide enough
            if cr.width() > 60.0 {
                let dur_label = format!("{:.1}s", clip.duration);
                p.text(
                    Pos2::new(cr.max.x - 4.0, cr.max.y - 2.0),
                    Align2::RIGHT_BOTTOM,
                    dur_label,
                    FontId::proportional(7.0),
                    Color32::from_white_alpha(140),
                );
            }

            // Handles
            if is_hovered || is_selected {
                let handle_w = 4.0;
                let handle_h = cr.height() * 0.55;
                let handle_y = cr.center().y - handle_h / 2.0;
                // Left handle
                p.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(cr.min.x - 1.0, handle_y),
                        Vec2::new(handle_w, handle_h),
                    ),
                    2u8,
                    Color32::from_white_alpha(210),
                );
                // Right handle
                p.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(cr.max.x - handle_w + 1.0, handle_y),
                        Vec2::new(handle_w, handle_h),
                    ),
                    2u8,
                    Color32::from_white_alpha(210),
                );
            }

            if is_selected {
                // Outer glow + stroke
                p.rect_stroke(
                    cr.expand(1.0),
                    5u8,
                    Stroke::new(2.0, SELECTION),
                    StrokeKind::Inside,
                );
                p.rect_stroke(
                    cr,
                    4u8,
                    Stroke::new(1.0, Color32::WHITE),
                    StrokeKind::Inside,
                );
            } else if is_hovered {
                p.rect_stroke(
                    cr,
                    4u8,
                    Stroke::new(1.0, Color32::from_white_alpha(90)),
                    StrokeKind::Inside,
                );
            }
        }

        // ── Ungroup markers on group tracks ──────────────────────────
        // Build reverse map: group ObjectId → group TrackId
        let group_oid_to_tid: HashMap<ObjectId, TrackId> = group_children
            .keys()
            .filter_map(|&tid| {
                timeline
                    .tracks
                    .get(tid)
                    .and_then(|t| t.object_id)
                    .map(|oid| (oid, tid))
            })
            .collect();
        for clip in timeline.clips.values() {
            if let ClipPayload::Ungroup { group, .. } = &clip.payload {
                let Some(&group_tid) = group_oid_to_tid.get(group) else {
                    continue;
                };
                let Some(layout) = track_layouts.get(&group_tid) else {
                    continue;
                };
                let x = self.time_to_x(clip.start, rect);
                if x < rect.min.x - 20.0 || x > rect.max.x + 20.0 {
                    continue;
                }
                let y = rect.min.y + layout.header_y - self.scroll_y;
                // Dashed vertical line spanning the group track
                let mut dash_y = y + 2.0;
                let dash_end = y + self.track_height - 2.0;
                while dash_y < dash_end {
                    let seg_end = (dash_y + 4.0).min(dash_end);
                    p.line_segment(
                        [Pos2::new(x, dash_y), Pos2::new(x, seg_end)],
                        Stroke::new(2.0, CLR_UNGROUP),
                    );
                    dash_y += 8.0;
                }
                // Small label
                p.text(
                    Pos2::new(x + 4.0, y + self.track_height / 2.0),
                    Align2::LEFT_CENTER,
                    "ungroup",
                    FontId::proportional(9.0),
                    CLR_UNGROUP,
                );
            }
        }
    }

    // ── Zoom bar (Premiere-like scrollbar) ────────────────────────────
    fn paint_zoom_bar(&self, p: &egui::Painter, rect: Rect, clips_rect: Rect, timeline: &Timeline) {
        // Track background
        p.rect_filled(rect, 2u8, Color32::from_rgb(28, 28, 32));
        p.rect_stroke(
            rect,
            2u8,
            Stroke::new(1.0, Color32::from_rgb(45, 45, 50)),
            StrokeKind::Inside,
        );
        // Subtle ticks in background
        let total = timeline.cached_duration.max(0.01);
        let bar_w = rect.width();
        let vw = clips_rect.width() as f64;
        let v_start = (self.scroll_offset as f64 / self.pixels_per_second).clamp(0.0, total);
        let v_end =
            ((self.scroll_offset + vw as f32) as f64 / self.pixels_per_second).clamp(0.0, total);
        let v_dur = (v_end - v_start).max(0.01);
        let l_ratio = v_start / total;
        let r_ratio = v_end / total;
        let win_l = rect.min.x + (l_ratio as f32 * bar_w);
        let win_r = rect.min.x + (r_ratio as f32 * bar_w);
        // Window (visible range)
        let win_rect = Rect::from_min_size(
            Pos2::new(win_l, rect.min.y + 2.0),
            Vec2::new((win_r - win_l).max(16.0), rect.height() - 4.0),
        );
        // Window gradient-like fill
        p.rect_filled(win_rect, 3u8, Color32::from_rgb(75, 75, 85));
        p.rect_filled(
            Rect::from_min_size(
                win_rect.min,
                Vec2::new(win_rect.width(), win_rect.height() * 0.5),
            ),
            3u8,
            Color32::from_rgb(95, 95, 105),
        );
        p.rect_stroke(
            win_rect,
            3u8,
            Stroke::new(1.0, Color32::from_rgb(130, 130, 140)),
            StrokeKind::Inside,
        );
        // Grip dots in center
        if win_rect.width() > 24.0 {
            let cx = win_rect.center().x;
            let cy = win_rect.center().y;
            for dx in [-6.0, 0.0, 6.0] {
                p.circle_filled(
                    Pos2::new(cx + dx as f32, cy),
                    1.5,
                    Color32::from_rgb(200, 200, 210),
                );
            }
        }
        // Handles
        let handle_w = 6.0;
        p.rect_filled(
            Rect::from_min_size(
                Pos2::new(win_rect.min.x - 1.0, win_rect.min.y),
                Vec2::new(handle_w, win_rect.height()),
            ),
            2u8,
            Color32::from_rgb(180, 180, 190),
        );
        p.rect_filled(
            Rect::from_min_size(
                Pos2::new(win_rect.max.x - handle_w + 1.0, win_rect.min.y),
                Vec2::new(handle_w, win_rect.height()),
            ),
            2u8,
            Color32::from_rgb(180, 180, 190),
        );
        // Label
        let label = if v_dur >= 1.0 {
            format!("{:.0}s / {:.0}s", v_dur, total)
        } else {
            format!("{:.1}s / {:.0}s", v_dur, total)
        };
        let font = FontId::proportional(9.0);
        let text_color = Color32::from_rgb(220, 220, 230);
        if win_rect.width() > 70.0 {
            p.text(
                win_rect.center(),
                Align2::CENTER_CENTER,
                label,
                font,
                text_color,
            );
        } else if win_r + 4.0 < rect.max.x - 20.0 {
            p.text(
                Pos2::new(win_r + 6.0, win_rect.center().y),
                Align2::LEFT_CENTER,
                label,
                font,
                text_color,
            );
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

    // ── Playhead (Premiere-like) ──────────────────────────────────────
    fn paint_playhead(&self, p: &egui::Painter, rect: Rect, ruler_top: f32, timeline: &Timeline) {
        let x = self.time_to_x(timeline.current_time, rect);
        if x < rect.min.x - 1.0 || x > rect.max.x + 1.0 {
            return;
        }
        // Glow behind line
        p.line_segment(
            [Pos2::new(x, ruler_top), Pos2::new(x, rect.max.y)],
            Stroke::new(4.0, Color32::from_rgba_premultiplied(255, 60, 60, 30)),
        );
        // Core line
        p.line_segment(
            [Pos2::new(x, ruler_top), Pos2::new(x, rect.max.y)],
            Stroke::new(1.8, PLAYHEAD),
        );
        // Handle cap at ruler
        let cap_w = 14.0;
        let cap_h = 12.0;
        let cap_rect = Rect::from_min_size(
            Pos2::new(x - cap_w / 2.0, ruler_top - 1.0),
            Vec2::new(cap_w, cap_h),
        );
        p.rect_filled(cap_rect, 2u8, PLAYHEAD);
        p.rect_stroke(
            cap_rect,
            2u8,
            Stroke::new(1.0, Color32::WHITE),
            StrokeKind::Inside,
        );
        // Inner dot
        p.circle_filled(Pos2::new(x, ruler_top + cap_h / 2.0), 2.2, Color32::WHITE);
        // Time label just below handle when not overlapping ruler text
        let time_label = format!("{:.2}s", timeline.current_time);
        p.text(
            Pos2::new(x, ruler_top + cap_h + 4.0),
            Align2::CENTER_TOP,
            time_label,
            FontId::proportional(9.0),
            Color32::from_rgb(255, 200, 200),
        );
    }

    // ── Input ───────────────────────────────────────────────────────────
    fn handle_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        zoom_bar_rect: Rect,
        ruler_rect: Rect,
        clips_rect: Rect,
        tracks_rect: Rect,
        label_rect: Rect,
        timeline: &mut Timeline,
        track_layouts: &HashMap<TrackId, TrackLayout>,
        on_edge: bool,
        hover_pos: Option<Pos2>,
        group_children: &HashMap<TrackId, Vec<TrackId>>,
    ) {
        // ── Keyboard shortcuts ──────────────────────────────────────────
        let pressed = |key| ui.input(|i| i.key_pressed(key));
        // Space = play/pause (global, no focus needed)
        if pressed(egui::Key::Space) {
            timeline.is_playing = !timeline.is_playing;
        }
        // Delete/Backspace only when canvas has focus
        if response.has_focus() {
            if (pressed(egui::Key::Delete) || pressed(egui::Key::Backspace))
                && let Some(cid) = self.selected_clip
            {
                timeline.remove_clip(cid);
                self.selected_clip = None;
                self.drag_state = DragState::None;
            }
        }
        if response.clicked() {
            response.request_focus();
        }

        // ── Zoom bar double-click → Fit ─────────────────────────────────
        if response.double_clicked()
            && let Some(pos) = hover_pos
            && zoom_bar_rect.contains(pos)
        {
            let dur = timeline.cached_duration.max(1.0);
            let w = clips_rect.width().max(100.0);
            self.pixels_per_second = (w as f64 / dur).clamp(20.0, 500.0);
            self.scroll_offset = 0.0;
        }

        // ── Zoom bar helpers (used below) ────────────────────────────────
        let total = timeline.cached_duration.max(0.01);
        let bar_w = zoom_bar_rect.width();
        let view_w = clips_rect.width();
        let min_v_dur = view_w as f64 / 500.0; // max pps
        let zoom_bar_info = |pos_x: f32| -> (f64, f64) {
            let ratio = ((pos_x - zoom_bar_rect.min.x) / bar_w).clamp(0.0, 1.0) as f64;
            let ct = ratio * total;
            (ct, ratio)
        };

        // ── Hover cursor ────────────────────────────────────────────────
        if hover_pos.is_some()
            && matches!(self.drag_state, DragState::None)
            && let Some(pos) = hover_pos
        {
            if tracks_rect.contains(pos)
                && (pos.x - (tracks_rect.min.x + self.label_width)).abs() < 5.0
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
            } else if zoom_bar_rect.contains(pos) {
                let v_start = (self.scroll_offset as f64 / self.pixels_per_second).max(0.0);
                let v_end =
                    ((self.scroll_offset + view_w) as f64 / self.pixels_per_second).min(total);
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
            } else if clips_rect.contains(pos)
                && self
                    .hit_clip_body(pos, clips_rect, timeline, track_layouts)
                    .is_some()
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        }

        // ── Context menu (right-click, Premiere-like) ──────────────────────
        if response.secondary_clicked() {
            if let Some(pos) = hover_pos {
                if let Some(cid) = self.hit_clip_body(pos, clips_rect, timeline, track_layouts) {
                    self.selected_clip = Some(cid);
                } else if let Some((cid, _)) =
                    self.hit_clip_edge(pos, clips_rect, timeline, track_layouts)
                {
                    self.selected_clip = Some(cid);
                }
            }
        }
        response.context_menu(|ui| {
            ui.set_min_width(180.0);
            ui.label(
                egui::RichText::new("CLIP")
                    .weak()
                    .size(9.0)
                    .color(TEXT_MUTED),
            );
            let has_sel = self.selected_clip.is_some();
            if ui
                .add_enabled(has_sel, egui::Button::new("🗑  Delete  (Del)"))
                .on_hover_text("Remove selected clip")
                .clicked()
            {
                if let Some(cid) = self.selected_clip {
                    timeline.remove_clip(cid);
                    self.selected_clip = None;
                }
                ui.close();
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("⎘  Duplicate"))
                .on_hover_text("Duplicate clip (TODO)")
                .clicked()
            {
                ui.close();
            }
            ui.separator();
            ui.label(
                egui::RichText::new("TIMELINE")
                    .weak()
                    .size(9.0)
                    .color(TEXT_MUTED),
            );
            if ui.button("◧  Split at playhead").clicked() {
                ui.close();
            }
            if ui.button(" Fit to view ").clicked() {
                let dur = timeline.cached_duration.max(1.0);
                let w = clips_rect.width().max(100.0);
                self.pixels_per_second = (w as f64 / dur).clamp(20.0, 500.0);
                self.scroll_offset = 0.0;
                ui.close();
            }
        });

        // Scroll → zoom (ctrl), vertical pan (shift or over tracks), or horizontal pan.
        let scroll = ui.input(|i| i.smooth_scroll_delta);
        if scroll != Vec2::ZERO {
            let over_zoom_area =
                hover_pos.is_some_and(|p| ruler_rect.contains(p) || zoom_bar_rect.contains(p));
            let ctrl = ui.input(|i| i.modifiers.ctrl);
            let shift = ui.input(|i| i.modifiers.shift);
            let over_tracks = hover_pos.is_some_and(|p| tracks_rect.contains(p));
            if ctrl || over_zoom_area {
                // Horizontal zoom
                let anchor = response
                    .hover_pos()
                    .unwrap_or(Pos2::new(clips_rect.center().x, 0.0));
                let anchor_time = self.x_to_time(anchor.x, clips_rect);
                let old_pps = self.pixels_per_second;
                self.pixels_per_second *= 1.0 + scroll.y as f64 * -0.002;
                self.pixels_per_second = self.pixels_per_second.clamp(20.0, 500.0);
                self.scroll_offset = (anchor.x - clips_rect.min.x)
                    + (anchor_time * self.pixels_per_second) as f32
                    - (anchor.x - clips_rect.min.x + self.scroll_offset)
                        * (self.pixels_per_second / old_pps) as f32;
                self.clamp_scroll(clips_rect, timeline);
            } else if shift || over_tracks {
                // Vertical scroll (shift+scroll or scroll directly over tracks area)
                self.scroll_y -= scroll.y;
                // clamp is done at the top of next frame via max_scroll_y
                self.scroll_y = self.scroll_y.max(0.0);
            } else {
                self.scroll_offset = (self.scroll_offset - scroll.x).max(0.0);
                self.clamp_scroll(clips_rect, timeline);
            }
            ui.ctx().request_repaint();
        }

        let mouse = response.interact_pointer_pos();

        match &self.drag_state {
            DragState::None => {
                if response.drag_started()
                    && let Some(pos) = mouse
                {
                    // Divider drag (highest priority)
                    if tracks_rect.contains(pos)
                        && (pos.x - (tracks_rect.min.x + self.label_width)).abs() < 6.0
                    {
                        self.drag_state = DragState::Divider {
                            initial_mouse_x: pos.x,
                            initial_width: self.label_width,
                        };
                    } else if zoom_bar_rect.contains(pos) {
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
                            self.clamp_scroll(clips_rect, timeline);
                            self.drag_state = DragState::ZoomBar {
                                kind: ZoomBarKind::Body,
                                initial_start: new_start,
                                initial_end: new_end,
                            };
                        }
                    } else if ruler_rect.contains(pos)
                        || (pos.x - self.time_to_x(timeline.current_time, clips_rect)).abs() < 10.0
                    {
                        timeline.is_playing = false;
                        self.drag_state = DragState::Playhead;
                        self.drag_mouse_start_x = pos.x;
                        let t = self
                            .x_to_time(pos.x, clips_rect)
                            .clamp(0.0, timeline.cached_duration);
                        timeline.seek_request = Some(t);
                    } else if let Some((cid, edge)) =
                        self.hit_clip_edge(pos, clips_rect, timeline, track_layouts)
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
                        self.hit_clip_body(pos, clips_rect, timeline, track_layouts)
                    {
                        self.drag_state = DragState::ClipBody(cid);
                        self.drag_mouse_start_x = pos.x;
                        if let Some(c) = timeline.clips.get(cid) {
                            self.drag_orig_start = c.start;
                            self.drag_orig_duration = c.duration;
                        }
                    } else if ruler_rect.contains(pos) {
                        let t = self
                            .x_to_time(pos.x, clips_rect)
                            .clamp(0.0, timeline.cached_duration);
                        timeline.seek_request = Some(t);
                    }
                }
                if response.clicked()
                    && let Some(pos) = mouse
                {
                    // Scene header toggle (uses full tracks_rect including label sidebar)
                    if self.show_scene_header
                        && tracks_rect.contains(pos)
                        && pos.y < tracks_rect.min.y + self.track_height
                    {
                        self.scene_expanded = !self.scene_expanded;
                    } else if label_rect.contains(pos) && self.scene_expanded {
                        let rel_y = pos.y - label_rect.min.y + self.scroll_y;
                        for (track_id, layout) in track_layouts {
                            if rel_y >= layout.header_y
                                && rel_y < layout.header_y + self.track_height
                            {
                                let is_group = group_children.contains_key(track_id);
                                let has_props = timeline
                                    .tracks
                                    .get(*track_id)
                                    .and_then(|t| t.object_id)
                                    .is_some();
                                let indent = layout.depth as f32 * 16.0;
                                let arrow_x = label_rect.min.x + 6.0 + indent;
                                let arrow_w = 16.0;
                                if (is_group || has_props)
                                    && pos.x >= arrow_x
                                    && pos.x < arrow_x + arrow_w
                                {
                                    if self.expanded_tracks.contains(track_id) {
                                        self.expanded_tracks.remove(track_id);
                                    } else {
                                        self.expanded_tracks.insert(*track_id);
                                    }
                                } else {
                                    self.selected_clip = None;
                                    self.selected_track = Some(*track_id);
                                }
                                break;
                            }
                        }
                    } else if clips_rect.contains(pos) {
                        self.selected_clip =
                            self.hit_clip_body(pos, clips_rect, timeline, track_layouts);
                    }
                }
            }
            _ => {
                if response.dragged()
                    && let Some(pos) = mouse
                {
                    match &self.drag_state {
                        DragState::Playhead => {
                            let t = self
                                .x_to_time(pos.x, clips_rect)
                                .clamp(0.0, timeline.cached_duration);
                            timeline.seek_request = Some(t);
                        }
                        DragState::ZoomBar {
                            kind,
                            initial_start,
                            initial_end,
                            ..
                        } => {
                            let ratio =
                                ((pos.x - zoom_bar_rect.min.x) / bar_w).clamp(0.0, 1.0) as f64;
                            let mouse_time = ratio * total;
                            let vw_f64 = view_w as f64;

                            match kind {
                                ZoomBarKind::Body => {
                                    let v_dur = initial_end - initial_start;
                                    let ns = (mouse_time - v_dur / 2.0).clamp(0.0, total - v_dur);
                                    self.scroll_offset = (ns * self.pixels_per_second) as f32;
                                    self.clamp_scroll(clips_rect, timeline);
                                }
                                ZoomBarKind::LeftEdge => {
                                    let ns = mouse_time.clamp(0.0, initial_end - min_v_dur);
                                    let nd = (initial_end - ns).max(min_v_dur);
                                    self.pixels_per_second = (vw_f64 / nd).clamp(20.0, 500.0);
                                    self.scroll_offset = (ns * self.pixels_per_second) as f32;
                                    self.clamp_scroll(clips_rect, timeline);
                                }
                                ZoomBarKind::RightEdge => {
                                    let ne = mouse_time.clamp(initial_start + min_v_dur, total);
                                    let nd = (ne - initial_start).max(min_v_dur);
                                    self.pixels_per_second = (vw_f64 / nd).clamp(20.0, 500.0);
                                    self.scroll_offset =
                                        (initial_start * self.pixels_per_second) as f32;
                                    self.clamp_scroll(clips_rect, timeline);
                                }
                            }
                        }
                        DragState::ClipBody(cid) => {
                            let dx = pos.x - self.drag_mouse_start_x;
                            let dt = dx as f64 / self.pixels_per_second;
                            let ns = (self.drag_orig_start + dt).max(0.0);
                            let snapped = if self.snap_enabled {
                                self.snap_time(ns, clips_rect, timeline)
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
                                self.snap_time(ns, clips_rect, timeline)
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
                                self.snap_time(ne, clips_rect, timeline)
                            } else {
                                ne
                            };
                            if let Some(clip) = timeline.clips.get_mut(*cid) {
                                clip.duration = (snapped_end - clip.start).max(0.1);
                            }
                        }
                        DragState::Divider {
                            initial_mouse_x,
                            initial_width,
                        } => {
                            let delta = pos.x - initial_mouse_x;
                            let new_w = (initial_width + delta).clamp(80.0, 400.0);
                            self.label_width = new_w;
                        }
                        _ => {}
                    }
                }
                if response.drag_stopped() {
                    let needs_rebuild = !matches!(
                        self.drag_state,
                        DragState::Playhead | DragState::ZoomBar { .. } | DragState::Divider { .. }
                    );
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
        track_layouts: &HashMap<TrackId, TrackLayout>,
    ) -> Option<(ClipId, ClipEdge)> {
        const EDGE_PX: f32 = 6.0;
        for clip in timeline.clips.values() {
            let Some(layout) = track_layouts.get(&clip.track) else {
                continue;
            };
            let x1 = self.time_to_x(clip.start, rect);
            let x2 = self.time_to_x(clip.start + clip.duration, rect);
            let y = rect.min.y + layout.header_y - self.scroll_y + 2.0;
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
        track_layouts: &HashMap<TrackId, TrackLayout>,
    ) -> Option<ClipId> {
        let clips: Vec<_> = timeline.clips.values().collect();
        for clip in clips.into_iter().rev() {
            let Some(layout) = track_layouts.get(&clip.track) else {
                continue;
            };
            let x1 = self.time_to_x(clip.start, rect);
            let x2 = self.time_to_x(clip.start + clip.duration, rect);
            let y = rect.min.y + layout.header_y - self.scroll_y + 2.0;
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

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
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
        ClipPayload::Animation(anim) => lens_color(&anim.lens),
        ClipPayload::GltfAnimation(_) => Color32::from_rgb(90, 170, 220),
        ClipPayload::Audio { .. } => CLR_AUDIO,
        ClipPayload::Wait => CLR_WAIT,
        ClipPayload::Marker(_) => CLR_MARKER,
        ClipPayload::Breakpoint => CLR_BREAKPOINT,
        ClipPayload::SegmentStart(_) => CLR_SEGMENT,
        ClipPayload::Ungroup { .. } => Color32::from_rgb(230, 100, 50),
        ClipPayload::SceneStart(_) => Color32::from_rgb(50, 180, 100),
        ClipPayload::SceneEnd(_) => Color32::from_rgb(180, 50, 50),
        ClipPayload::RemoveUpdater { .. } => Color32::from_rgb(120, 120, 220),
        ClipPayload::SetSceneMember { .. } => Color32::from_rgb(80, 160, 180),
        ClipPayload::Transition { .. } => Color32::from_rgb(200, 200, 50),
    }
}

fn lens_color(lens: &PropertyLensSpec) -> Color32 {
    match lens {
        PropertyLensSpec::Translation { .. } => Color32::from_rgb(74, 144, 217),
        PropertyLensSpec::Rotation { .. } => Color32::from_rgb(80, 200, 120),
        PropertyLensSpec::Scale { .. } => Color32::from_rgb(255, 185, 15),
        PropertyLensSpec::Opacity { .. } => Color32::from_rgb(255, 140, 0),
        PropertyLensSpec::FillColor { .. } => Color32::from_rgb(220, 80, 80),
        PropertyLensSpec::StrokeColor { .. } => Color32::from_rgb(200, 80, 200),
        PropertyLensSpec::StrokeWidth { .. } => Color32::from_rgb(0, 200, 200),
        PropertyLensSpec::PathCompletion { .. } => Color32::from_rgb(155, 89, 182),
        PropertyLensSpec::FillDrawProgress { .. } => Color32::from_rgb(180, 130, 210),
        PropertyLensSpec::CameraPosition { .. }
        | PropertyLensSpec::CameraRotation { .. }
        | PropertyLensSpec::CameraZoom { .. }
        | PropertyLensSpec::CameraFollow { .. }
        | PropertyLensSpec::CameraShake { .. }
        | PropertyLensSpec::CameraTarget { .. }
        | PropertyLensSpec::CameraPerspective { .. } => Color32::from_rgb(100, 100, 100),
        PropertyLensSpec::PathFollow { .. } => Color32::from_rgb(26, 188, 156),
        PropertyLensSpec::SignalFloat { .. } => Color32::from_rgb(180, 180, 50),
        PropertyLensSpec::PathRange { .. } => Color32::from_rgb(255, 105, 180),
        PropertyLensSpec::PathMorph { .. } => Color32::from_rgb(100, 149, 237),
        PropertyLensSpec::Custom { .. } => Color32::from_rgb(200, 200, 200),
    }
}

fn clip_icon(payload: &ClipPayload) -> &'static str {
    match payload {
        ClipPayload::Animation(a) => match &a.lens {
            PropertyLensSpec::Translation { .. } => "↔",
            PropertyLensSpec::Rotation { .. } => "⟲",
            PropertyLensSpec::Scale { .. } => "⤢",
            PropertyLensSpec::Opacity { .. } => "◐",
            PropertyLensSpec::FillColor { .. } => "●",
            PropertyLensSpec::StrokeColor { .. } => "◯",
            PropertyLensSpec::StrokeWidth { .. } => "━",
            PropertyLensSpec::PathCompletion { .. } => "✎",
            PropertyLensSpec::FillDrawProgress { .. } => "⬔",
            PropertyLensSpec::CameraPosition { .. } => "🎥",
            PropertyLensSpec::CameraRotation { .. } => "🎞",
            PropertyLensSpec::CameraZoom { .. } => "🔍",
            PropertyLensSpec::CameraFollow { .. } => "🎯",
            PropertyLensSpec::CameraShake { .. } => "〰",
            PropertyLensSpec::CameraTarget { .. } => "🎯",
            PropertyLensSpec::CameraPerspective { .. } => "🔭",
            PropertyLensSpec::PathFollow { .. } => "➰",
            PropertyLensSpec::SignalFloat { .. } => "📈",
            PropertyLensSpec::PathRange { .. } => "⚡",
            PropertyLensSpec::PathMorph { .. } => "⬡",
            PropertyLensSpec::Custom { .. } => "★",
        },
        ClipPayload::GltfAnimation(_) => "3D",
        ClipPayload::Audio { .. } => "♪",
        ClipPayload::Wait => "⏳",
        ClipPayload::Marker(_) => "⚑",
        ClipPayload::Breakpoint => "🔴",
        ClipPayload::SegmentStart(_) => "▶",
        ClipPayload::Ungroup { .. } => "⧉",
        ClipPayload::SceneStart(_) => "🎬",
        ClipPayload::SceneEnd(_) => "■",
        ClipPayload::RemoveUpdater { .. } => "✕",
        ClipPayload::SetSceneMember { .. } => "👁",
        ClipPayload::Transition { .. } => "⇄",
    }
}

fn clip_label(payload: &ClipPayload) -> String {
    match payload {
        ClipPayload::Animation(a) => {
            // Use the high-level animation label if available
            if let Some(label) = &a.label {
                return label.clone();
            }
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
                PropertyLensSpec::CameraFollow { .. } => "CamFollow",
                PropertyLensSpec::CameraShake { .. } => "CamShake",
                PropertyLensSpec::CameraTarget { .. } => "CamTarget",
                PropertyLensSpec::CameraPerspective { .. } => "CamPersp",
                PropertyLensSpec::PathFollow { .. } => "Follow",
                PropertyLensSpec::SignalFloat { .. } => "Signal",
                PropertyLensSpec::PathRange { .. } => "Flash",
                PropertyLensSpec::PathMorph { .. } => "Morph",
                PropertyLensSpec::Custom { type_name, .. } => type_name.as_str(),
            };
            n.to_string()
        }
        ClipPayload::GltfAnimation(action) => format!("Action:{}", action.animation_index),
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
        ClipPayload::Ungroup { .. } => "Ungroup".into(),
        ClipPayload::SceneStart(_) => "Scene▶".into(),
        ClipPayload::SceneEnd(_) => "Scene◼".into(),
        ClipPayload::RemoveUpdater { .. } => "StopUpdater".into(),
        ClipPayload::SetSceneMember { .. } => "SceneMember".into(),
        ClipPayload::Transition { .. } => "Transition".into(),
    }
}
