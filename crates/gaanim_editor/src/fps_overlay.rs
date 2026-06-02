use bevy::prelude::*;
use bevy_egui::egui::{self, Color32};

const HISTORY_LEN: usize = 120;

#[derive(Resource)]
pub struct FpsOverlay {
    pub visible: bool,
    history: [f32; HISTORY_LEN],
    idx: usize,
    count: usize,
    pub current_fps: f32,
    pub current_ms: f32,
    min_fps: f32,
    max_fps: f32,
}

impl Default for FpsOverlay {
    fn default() -> Self {
        Self {
            visible: false,
            history: [0.0; HISTORY_LEN],
            idx: 0,
            count: 0,
            current_fps: 0.0,
            current_ms: 0.0,
            min_fps: f32::MAX,
            max_fps: 0.0,
        }
    }
}

impl FpsOverlay {
    pub fn push(&mut self, dt_secs: f64) {
        if dt_secs <= 0.0 {
            return;
        }
        let fps = 1.0 / dt_secs as f32;
        self.current_ms = (dt_secs * 1000.0) as f32;
        self.current_fps = fps;
        self.min_fps = self.min_fps.min(fps);
        self.max_fps = self.max_fps.max(fps);
        self.history[self.idx] = fps;
        self.idx = (self.idx + 1) % HISTORY_LEN;
        self.count = (self.count + 1).min(HISTORY_LEN);
    }

    pub fn render(&self, ctx: &egui::Context) {
        if !self.visible {
            return;
        }

        let area_w = 260.0;
        let area_h = 120.0;
        let graph_h = 70.0;
        let bar_w = area_w / HISTORY_LEN as f32;

        egui::Area::new(egui::Id::new("fps_overlay"))
            .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::splat(-6.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                // Fix the area size
                ui.set_min_size(egui::Vec2::new(area_w, area_h));

                let rect = ui.min_rect();
                let p = ui.painter_at(rect);

                // Background
                p.rect_filled(rect, 4u8, Color32::from_rgba_premultiplied(0, 0, 0, 180));

                // Text stats
                let avg = if self.count > 0 {
                    self.history.iter().take(self.count).sum::<f32>() / self.count as f32
                } else {
                    0.0
                };
                let min = if self.min_fps < f32::MAX { self.min_fps } else { 0.0 };
                let stats = format!(
                    "{:.0} FPS  |  {:.1} ms  |  Ø {:.0}  |  ↓ {:.0}  ↑ {:.0}",
                    self.current_fps, self.current_ms, avg, min, self.max_fps,
                );
                p.text(
                    egui::Pos2::new(rect.min.x + 6.0, rect.min.y + 10.0),
                    egui::Align2::LEFT_TOP,
                    stats,
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(180, 255, 180),
                );

                // Bar graph area
                let graph_rect = egui::Rect::from_min_max(
                    egui::Pos2::new(rect.min.x + 4.0, rect.min.y + 24.0),
                    egui::Pos2::new(rect.max.x - 4.0, rect.min.y + 24.0 + graph_h),
                );
                let p = ui.painter_at(graph_rect);

                // Background for graph
                p.rect_filled(graph_rect, 2u8, Color32::from_rgba_premultiplied(0, 0, 0, 100));

                let baseline = 30.0_f32; // 0 fps baseline in graph space
                let scale = (graph_h - 4.0) / baseline; // pixels per fps

                let count = self.count.min(HISTORY_LEN);
                for i in 0..count {
                    let hist_idx = if i <= self.idx {
                        self.idx - i
                    } else {
                        self.idx + HISTORY_LEN - i
                    };
                    let hist_idx = hist_idx % HISTORY_LEN;
                    let fps = self.history[hist_idx];
                    let x = graph_rect.max.x - (i as f32 + 0.5) * bar_w;
                    let h = (fps * scale).clamp(0.0, graph_h - 2.0);
                    let bar_rect = egui::Rect::from_min_max(
                        egui::Pos2::new(x - bar_w * 0.4, graph_rect.max.y - 2.0 - h),
                        egui::Pos2::new(x + bar_w * 0.4, graph_rect.max.y - 2.0),
                    );
                    let color = if fps >= 55.0 {
                        Color32::from_rgb(80, 220, 80)
                    } else if fps >= 30.0 {
                        Color32::from_rgb(220, 200, 60)
                    } else {
                        Color32::from_rgb(220, 60, 60)
                    };
                    p.rect_filled(bar_rect, 0u8, color);
                }

                // 60 fps reference line
                let y60 = graph_rect.max.y - 2.0 - 60.0 * scale;
                if y60 > graph_rect.min.y {
                    p.line_segment(
                        [
                            egui::Pos2::new(graph_rect.min.x, y60),
                            egui::Pos2::new(graph_rect.max.x, y60),
                        ],
                        egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(100, 255, 100, 60)),
                    );
                }
            });
    }
}

/// System: updates FpsOverlay with frame delta time each frame.
pub fn fps_overlay_system(
    time: Res<Time>,
    mut overlay: ResMut<FpsOverlay>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if keyboard.just_pressed(KeyCode::F12) {
        overlay.visible = !overlay.visible;
        if overlay.visible {
            overlay.min_fps = f32::MAX;
            overlay.max_fps = 0.0;
            overlay.count = 0;
        }
    }
    overlay.push(time.delta_secs_f64());
}
