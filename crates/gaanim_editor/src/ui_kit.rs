//! Visual language shared by the editor overlays (playback bar, export
//! dialogs): palette, vector icons, buttons and form controls.
//!
//! Icons are painted as shapes instead of font glyphs so they stay crisp,
//! share one stroke weight, and do not depend on emoji coverage of the
//! bundled fonts.

use bevy_egui::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2,
};

/// Colors shared by the editor overlays.
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
    pub const SURFACE: Color32 = Color32::from_rgb(22, 23, 30);
    pub const FIELD: Color32 = Color32::from_rgba_premultiplied(10, 10, 11, 10);
    pub const SELECTED: Color32 = Color32::from_rgba_premultiplied(30, 31, 34, 32);
    pub const DANGER: Color32 = Color32::from_rgb(255, 118, 118);
    pub const INK: Color32 = Color32::from_rgb(15, 16, 21);
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
    Close,
    Check,
    Warning,
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
        Icon::Close => {
            painter.line_segment([p(-0.30, -0.30), p(0.30, 0.30)], stroke);
            painter.line_segment([p(0.30, -0.30), p(-0.30, 0.30)], stroke);
        }
        Icon::Check => {
            painter.line(vec![p(-0.34, 0.02), p(-0.10, 0.26), p(0.36, -0.24)], stroke);
        }
        Icon::Warning => {
            painter.line_segment([p(0.0, -0.36), p(0.0, 0.10)], stroke);
            painter.circle_filled(p(0.0, 0.34), s * 0.07, color);
        }
        Icon::More => {
            for x in [-0.30, 0.0, 0.30] {
                painter.circle_filled(p(x, 0.0), s * 0.075, color);
            }
        }
    }
}

/// Floating card used by modal dialogs.
pub(crate) fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(palette::SURFACE)
        .corner_radius(16.0)
        .inner_margin(egui::Margin::same(22))
        .stroke(Stroke::new(1.0, palette::PANEL_STROKE))
        .shadow(egui::Shadow {
            offset: [0, 12],
            blur: 40,
            spread: 0,
            color: Color32::from_black_alpha(140),
        })
}

/// Rounded input surface for text edits and numeric fields.
pub(crate) fn field_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(palette::FIELD)
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .stroke(Stroke::new(1.0, palette::PANEL_STROKE))
}

/// Caption above a group of controls.
pub(crate) fn section_label(ui: &mut Ui, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .size(11.5)
                .color(palette::TEXT_MUTED),
        )
        .selectable(false),
    );
}

/// A segmented control. Each option is `(value, title, detail)`; an empty
/// detail keeps the control single-line. Returns `true` when it changed.
pub(crate) fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    current: &mut T,
    options: &[(T, &str, &str)],
) -> bool {
    let two_lines = options.iter().any(|(_, _, detail)| !detail.is_empty());
    let height = if two_lines { 48.0 } else { 32.0 };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 10.0, palette::FIELD);
    let count = options.len().max(1) as f32;
    let segment_w = (rect.width() - 4.0) / count;
    let mut changed = false;
    for (index, (value, title, detail)) in options.iter().enumerate() {
        let segment = Rect::from_min_size(
            pos2(
                rect.min.x + 2.0 + segment_w * index as f32,
                rect.min.y + 2.0,
            ),
            vec2(segment_w, height - 4.0),
        );
        let response = ui.interact(segment, ui.id().with(id).with(index), Sense::click());
        let selected = *current == *value;
        if selected {
            painter.rect_filled(segment, 8.0, palette::SELECTED);
            painter.rect_stroke(
                segment,
                8.0,
                Stroke::new(1.0, Color32::from_white_alpha(20)),
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() {
            painter.rect_filled(segment, 8.0, palette::HOVER);
        }
        let title_color = if selected || response.hovered() {
            palette::TEXT
        } else {
            palette::TEXT_MUTED
        };
        if two_lines {
            painter.text(
                segment.center() - vec2(0.0, 8.0),
                Align2::CENTER_CENTER,
                *title,
                FontId::proportional(13.0),
                title_color,
            );
            painter.text(
                segment.center() + vec2(0.0, 9.0),
                Align2::CENTER_CENTER,
                *detail,
                FontId::proportional(11.0),
                if selected {
                    palette::TEXT_MUTED
                } else {
                    palette::TEXT_FAINT
                },
            );
        } else {
            painter.text(
                segment.center(),
                Align2::CENTER_CENTER,
                *title,
                FontId::proportional(12.5),
                title_color,
            );
        }
        if response.clicked() && !selected {
            *current = *value;
            changed = true;
        }
    }
    changed
}

/// Compact pill used for presets.
pub(crate) fn chip(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let font = FontId::proportional(12.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), palette::TEXT);
    let size = vec2(galley.size().x + 20.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let fill = if selected {
        palette::ACCENT.gamma_multiply(0.22)
    } else if response.hovered() {
        palette::HOVER
    } else {
        palette::FIELD
    };
    painter.rect_filled(rect, 13.0, fill);
    let color = if selected {
        palette::ACCENT
    } else if response.hovered() {
        palette::TEXT
    } else {
        palette::TEXT_MUTED
    };
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, color);
    response
}

/// Filled call-to-action button with an optional leading icon.
pub(crate) fn primary_button(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    enabled: bool,
) -> Response {
    text_button(ui, label, icon, enabled, true)
}

/// Quiet button for secondary actions.
pub(crate) fn secondary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    text_button(ui, label, None, enabled, false)
}

fn text_button(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    enabled: bool,
    primary: bool,
) -> Response {
    let font = FontId::proportional(13.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), palette::TEXT);
    let icon_w = if icon.is_some() { 20.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 32.0, 34.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();
    let painter = ui.painter();
    let (fill, text) = match (primary, enabled) {
        (true, true) => (
            if pressed {
                palette::ACCENT.gamma_multiply(0.85)
            } else if hovered {
                Color32::from_rgb(140, 188, 255)
            } else {
                palette::ACCENT
            },
            palette::INK,
        ),
        (true, false) => (palette::ACCENT.gamma_multiply(0.3), palette::INK),
        (false, _) => (
            if pressed {
                palette::PRESSED
            } else if hovered {
                palette::HOVER
            } else {
                palette::FIELD
            },
            if enabled {
                palette::TEXT
            } else {
                palette::TEXT_FAINT
            },
        ),
    };
    painter.rect_filled(rect, 9.0, fill);
    let content_w = galley.size().x + icon_w;
    let start_x = rect.center().x - content_w / 2.0;
    if let Some(icon) = icon {
        paint_icon(
            painter,
            Rect::from_center_size(pos2(start_x + 7.0, rect.center().y), Vec2::splat(14.0)),
            icon,
            text,
        );
    }
    painter.text(
        pos2(start_x + icon_w, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        text,
    );
    response
}

/// Slim rounded progress track.
pub(crate) fn progress_track(ui: &mut Ui, fraction: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, Color32::from_white_alpha(22));
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction > 0.0 {
        let fill = Rect::from_min_max(
            rect.min,
            pos2(rect.min.x + rect.width() * fraction, rect.max.y),
        );
        painter.rect_filled(fill, 3.0, color);
    }
}

/// Round badge with an icon, used as the headline of result dialogs.
pub(crate) fn status_badge(ui: &mut Ui, icon: Icon, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
    let painter = ui.painter();
    painter.circle_filled(rect.center(), 20.0, color.gamma_multiply(0.18));
    paint_icon(
        painter,
        Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
        icon,
        color,
    );
}
