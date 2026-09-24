//! Visual language of the editor playback bar: palette, vector icons and
//! icon buttons.
//!
//! Icons are painted as shapes instead of font glyphs so they stay crisp,
//! share one stroke weight, and do not depend on emoji coverage of the
//! bundled fonts.

use bevy_egui::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

/// Colors shared by the playback bar.
pub(crate) mod palette {
    use super::Color32;

    pub const PANEL: Color32 = Color32::from_rgba_premultiplied(15, 16, 21, 238);
    pub const PANEL_STROKE: Color32 = Color32::from_rgba_premultiplied(20, 21, 26, 24);
    pub const TEXT: Color32 = Color32::from_rgb(236, 238, 243);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(148, 153, 166);
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(92, 97, 110);
    pub const ACCENT: Color32 = Color32::from_rgb(112, 170, 255);
    pub const LOOP: Color32 = Color32::from_rgb(96, 206, 152);
    pub const STOP: Color32 = Color32::from_rgb(245, 192, 86);
    pub const HOVER: Color32 = Color32::from_rgba_premultiplied(22, 22, 24, 22);
    pub const PRESSED: Color32 = Color32::from_rgba_premultiplied(34, 34, 36, 34);
    pub const DIVIDER: Color32 = Color32::from_rgba_premultiplied(24, 24, 26, 26);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    Play,
    Pause,
    SkipStart,
    SkipEnd,
    PrevScene,
    NextScene,
    Loop,
    Continuous,
    Fullscreen,
    Present,
    Export,
    Pin,
    More,
}

/// How an icon button presents its state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ButtonTone {
    #[default]
    Ghost,
    /// A toggle that is currently on; tinted with the given color.
    On(ToggleColor),
    /// The main action of the bar (play / pause).
    Primary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToggleColor {
    Accent,
    Loop,
    Stop,
}

impl ToggleColor {
    fn color(self) -> Color32 {
        match self {
            Self::Accent => palette::ACCENT,
            Self::Loop => palette::LOOP,
            Self::Stop => palette::STOP,
        }
    }
}

pub(crate) const BUTTON_SIZE: f32 = 30.0;
pub(crate) const PRIMARY_SIZE: f32 = 36.0;

/// A square icon button with hover, press, toggle and disabled states.
pub(crate) fn icon_button(ui: &mut Ui, icon: Icon, tone: ButtonTone, enabled: bool) -> Response {
    let side = if tone == ButtonTone::Primary {
        PRIMARY_SIZE
    } else {
        BUTTON_SIZE
    };
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), sense);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();

    let icon_color = match tone {
        ButtonTone::Primary => {
            let fill = if pressed {
                Color32::from_rgb(200, 204, 214)
            } else if hovered {
                Color32::WHITE
            } else {
                palette::TEXT
            };
            let radius = if hovered {
                side / 2.0
            } else {
                side / 2.0 - 1.0
            };
            painter.circle_filled(
                rect.center() + vec2(0.0, 1.0),
                radius,
                Color32::from_black_alpha(70),
            );
            painter.circle_filled(rect.center(), radius, fill);
            Color32::from_rgb(15, 16, 21)
        }
        ButtonTone::On(toggle) => {
            let color = toggle.color();
            let alpha = if hovered { 58 } else { 40 };
            painter.rect_filled(rect.shrink(1.0), 8.0, color.gamma_multiply_u8(alpha));
            color
        }
        ButtonTone::Ghost => {
            if pressed {
                painter.rect_filled(rect.shrink(1.0), 8.0, palette::PRESSED);
            } else if hovered {
                painter.rect_filled(rect.shrink(1.0), 8.0, palette::HOVER);
            }
            if !enabled {
                palette::TEXT_FAINT
            } else if hovered {
                palette::TEXT
            } else {
                palette::TEXT_MUTED
            }
        }
    };

    let icon_side = if tone == ButtonTone::Primary {
        15.0
    } else {
        16.0
    };
    paint_icon(
        painter,
        Rect::from_center_size(rect.center(), Vec2::splat(icon_side)),
        icon,
        icon_color,
    );
    response
}

/// Thin vertical rule used between control groups.
pub(crate) fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 18.0), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, palette::DIVIDER),
    );
}

/// Paint `icon` centered in `rect` with a uniform stroke weight.
pub(crate) fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke = Stroke::new((s * 0.105).max(1.4), color);
    let p = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    let fill_poly = |points: Vec<Pos2>| {
        painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
    };
    let bar = |x: f32, half_w: f32, half_h: f32| {
        painter.rect_filled(
            Rect::from_min_max(p(x - half_w, -half_h), p(x + half_w, half_h)),
            s * 0.05,
            color,
        );
    };

    match icon {
        Icon::Play => fill_poly(vec![p(-0.30, -0.42), p(0.44, 0.0), p(-0.30, 0.42)]),
        Icon::Pause => {
            bar(-0.19, 0.105, 0.40);
            bar(0.19, 0.105, 0.40);
        }
        Icon::SkipStart => {
            bar(-0.36, 0.07, 0.36);
            fill_poly(vec![p(0.36, -0.36), p(0.36, 0.36), p(-0.22, 0.0)]);
        }
        Icon::SkipEnd => {
            bar(0.36, 0.07, 0.36);
            fill_poly(vec![p(-0.36, -0.36), p(0.22, 0.0), p(-0.36, 0.36)]);
        }
        Icon::PrevScene => {
            painter.line(vec![p(0.12, -0.34), p(-0.18, 0.0), p(0.12, 0.34)], stroke);
        }
        Icon::NextScene => {
            painter.line(vec![p(-0.12, -0.34), p(0.18, 0.0), p(-0.12, 0.34)], stroke);
        }
        Icon::Loop => {
            // Open circle with an arrowhead at the end of the arc.
            let radius = 0.34;
            let start = -60.0_f32.to_radians();
            let sweep = 290.0_f32.to_radians();
            let points: Vec<Pos2> = (0..=28)
                .map(|i| {
                    let a = start + sweep * i as f32 / 28.0;
                    p(radius * a.cos(), radius * a.sin())
                })
                .collect();
            let tip = *points.last().expect("arc has points");
            painter.line(points, stroke);
            let end = start + sweep;
            let tangent = vec2(-end.sin(), end.cos());
            let normal = vec2(end.cos(), end.sin());
            let head = s * 0.2;
            fill_poly(vec![
                tip + tangent * head * 0.9,
                tip - normal * head * 0.75,
                tip + normal * head * 0.75,
            ]);
        }
        Icon::Continuous => {
            // Lemniscate of Bernoulli: "play through every stop".
            let a = 0.46;
            let points: Vec<Pos2> = (0..=48)
                .map(|i| {
                    let t = std::f32::consts::TAU * i as f32 / 48.0;
                    let d = 1.0 + t.sin() * t.sin();
                    p(a * t.cos() / d, a * t.sin() * t.cos() / d)
                })
                .collect();
            painter.line(points, stroke);
        }
        Icon::Fullscreen => {
            let (o, l) = (0.38, 0.16);
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                painter.line(
                    vec![
                        p(sx * o, sy * (o - l)),
                        p(sx * o, sy * o),
                        p(sx * (o - l), sy * o),
                    ],
                    stroke,
                );
            }
        }
        Icon::Present => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.44, -0.32), p(0.44, 0.22)),
                s * 0.06,
                stroke,
                egui::StrokeKind::Middle,
            );
            fill_poly(vec![p(-0.09, -0.17), p(0.16, -0.05), p(-0.09, 0.07)]);
            painter.line_segment([p(-0.16, 0.42), p(0.16, 0.42)], stroke);
        }
        Icon::Export => {
            painter.line_segment([p(0.0, -0.40), p(0.0, 0.12)], stroke);
            painter.line(vec![p(-0.20, -0.07), p(0.0, 0.13), p(0.20, -0.07)], stroke);
            painter.line(
                vec![p(-0.40, 0.14), p(-0.40, 0.38), p(0.40, 0.38), p(0.40, 0.14)],
                stroke,
            );
        }
        Icon::Pin => {
            painter.line(vec![p(-0.18, -0.40), p(0.18, -0.40)], stroke);
            painter.line(
                vec![
                    p(-0.12, -0.40),
                    p(-0.12, -0.08),
                    p(-0.30, 0.10),
                    p(0.30, 0.10),
                    p(0.12, -0.08),
                    p(0.12, -0.40),
                ],
                stroke,
            );
            painter.line_segment([p(0.0, 0.10), p(0.0, 0.44)], stroke);
        }
        Icon::More => {
            for x in [-0.30, 0.0, 0.30] {
                painter.circle_filled(p(x, 0.0), s * 0.075, color);
            }
        }
    }
}
