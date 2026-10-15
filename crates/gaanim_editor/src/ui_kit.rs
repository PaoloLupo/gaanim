//! Visual language shared by the editor overlays (playback bar, export
//! dialogs): palette, vector icons, buttons and form controls.
//!
//! Icons are painted as shapes instead of font glyphs so they stay crisp,
//! share one stroke weight, and do not depend on emoji coverage of the
//! bundled fonts.
//!
//! Corners are square everywhere, like the pixel mark: controls, cards,
//! tracks and knobs are rectangles, and [`square_corners`] does the same for
//! what egui draws on its own.

use bevy_egui::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2,
};

/// Colors shared by the editor overlays: Gaanim's brand palette
/// (`docs/assets/brand/README.md`) on graphite without a blue tint.
pub(crate) mod palette {
    use super::Color32;

    /// Graphite `#121212` at 93 %: floating bars and panels.
    pub const PANEL: Color32 = Color32::from_rgba_premultiplied(17, 17, 17, 238);
    pub const PANEL_STROKE: Color32 = Color32::from_rgba_premultiplied(22, 22, 22, 24);
    /// Paper `#F7F6FF`.
    pub const TEXT: Color32 = Color32::from_rgb(247, 246, 255);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(160, 158, 170);
    pub const TEXT_FAINT: Color32 = Color32::from_rgb(104, 102, 114);
    /// Light violet `#A99FFF`: links, selection and the primary action.
    pub const ACCENT: Color32 = Color32::from_rgb(169, 159, 255);
    /// Haze `#CBC6FF`: the primary action under the pointer.
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(203, 198, 255);
    /// Violet `#7C6CFF`: loops, progress and what is ready.
    pub const LOOP: Color32 = Color32::from_rgb(124, 108, 255);
    /// Gold `#FFC933`: the current frame (stops, the playhead).
    pub const STOP: Color32 = Color32::from_rgb(255, 201, 51);
    pub const HOVER: Color32 = Color32::from_rgba_premultiplied(22, 22, 22, 22);
    pub const PRESSED: Color32 = Color32::from_rgba_premultiplied(34, 34, 34, 34);
    pub const DIVIDER: Color32 = Color32::from_rgba_premultiplied(26, 26, 26, 26);
    /// Graphite `#1A1A1A`: cards and dialogs.
    pub const SURFACE: Color32 = Color32::from_rgb(26, 26, 26);
    pub const FIELD: Color32 = Color32::from_rgba_premultiplied(10, 10, 10, 10);
    pub const SELECTED: Color32 = Color32::from_rgba_premultiplied(32, 32, 32, 32);
    pub const DANGER: Color32 = Color32::from_rgb(255, 118, 118);
    /// Graphite `#121212`: the window behind everything.
    pub const GRAPHITE: Color32 = Color32::from_rgb(18, 18, 18);
    /// Ink `#14112E`: text on violet.
    pub const INK: Color32 = Color32::from_rgb(20, 17, 46);
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
    Grid,
    BlackScreen,
    WhiteScreen,
    Keyboard,
    Plus,
    Folder,
    Code,
    Book,
    ExternalLink,
    Package,
    Mic,
    Record,
    /// Four-way arrows: move the view.
    Move,
    /// Ring with ticks: a point and its coordinates.
    Crosshair,
    /// Counter-clockwise arrow: back to the starting view.
    Reset,
    /// Frame split in thirds: composition guides.
    Thirds,
    /// A box holding two boxes: layout inspector.
    Layout,
    /// Two chain links: copy a link.
    Link,
    /// A speaker with sound waves.
    Volume,
    /// A speaker with a cross: muted.
    VolumeMuted,
    /// A magnifying glass: inspect the selected drawable.
    Inspect,
    /// A prompt and a cursor: the console.
    Console,
    /// Viewfinder corners around a dot: save the frame on screen.
    Capture,
    /// Two columns of dots: drag to move a panel.
    Grip,
    /// Staggered bars on tracks: the animation timeline.
    Timeline,
    /// Three sliders with square knobs: settings.
    Settings,
    /// A house: back to Home.
    Home,
    /// Three rows, each with a square: a list.
    List,
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
    icon_button_sized(ui, icon, tone, enabled, side)
}

/// [`icon_button`] at an explicit size, for surfaces read from a distance.
pub(crate) fn icon_button_sized(
    ui: &mut Ui,
    icon: Icon,
    tone: ButtonTone,
    enabled: bool,
    side: f32,
) -> Response {
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
                palette::LOOP
            } else if hovered {
                palette::ACCENT_HOVER
            } else {
                palette::ACCENT
            };
            let body = Rect::from_center_size(
                rect.center(),
                Vec2::splat(if hovered { side } else { side - 2.0 }),
            );
            painter.rect_filled(
                body.translate(vec2(0.0, 1.0)),
                0.0,
                Color32::from_black_alpha(70),
            );
            painter.rect_filled(body, 0.0, fill);
            palette::INK
        }
        ButtonTone::On(toggle) => {
            let color = toggle.color();
            let alpha = if hovered { 58 } else { 40 };
            painter.rect_filled(rect.shrink(1.0), 0.0, color.gamma_multiply_u8(alpha));
            color
        }
        ButtonTone::Ghost => {
            if pressed {
                painter.rect_filled(rect.shrink(1.0), 0.0, palette::PRESSED);
            } else if hovered {
                painter.rect_filled(rect.shrink(1.0), 0.0, palette::HOVER);
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
        side * 0.42
    } else {
        (side * 0.53).min(20.0)
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
/// The preview's volume: a speaker that mutes and unmutes, and a slider
/// beside it when `slider`.
pub(crate) fn volume_control(ui: &mut Ui, volume: &mut gaanim_media::PreviewVolume, slider: bool) {
    let silent = volume.muted || volume.level <= 0.0;
    let icon = if silent {
        Icon::VolumeMuted
    } else {
        Icon::Volume
    };
    // Right to left in the bar: the slider sits left of the speaker.
    if icon_button(ui, icon, ButtonTone::Ghost, true)
        .on_hover_text(if silent { "Unmute" } else { "Mute" })
        .clicked()
    {
        if volume.muted || volume.level > 0.0 {
            volume.muted = !volume.muted;
        } else {
            // Muted by the slider: sound comes back at a usable level.
            volume.level = 0.5;
            volume.muted = false;
        }
    }
    if slider {
        volume_slider(ui, volume, 72.0);
    }
}

/// The volume as a slider `width` points wide; moving it unmutes.
pub(crate) fn volume_slider(ui: &mut Ui, volume: &mut gaanim_media::PreviewVolume, width: f32) {
    let mut level = if volume.muted { 0.0 } else { volume.level };
    ui.spacing_mut().slider_width = width;
    let response = ui
        .add(egui::Slider::new(&mut level, 0.0..=1.0).show_value(false))
        .on_hover_text(format!("Volumen {:.0} %", level * 100.0));
    if response.changed() {
        volume.level = level;
        volume.muted = false;
    }
}

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
            0.0,
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
                0.0,
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
            painter.rect_filled(
                Rect::from_center_size(p(0.0, 0.34), Vec2::splat(s * 0.14)),
                0.0,
                color,
            );
        }
        Icon::Grid => {
            for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                painter.rect_stroke(
                    Rect::from_center_size(p(x * 0.2, y * 0.2), Vec2::splat(s * 0.3)),
                    0.0,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
        }
        Icon::BlackScreen => {
            painter.rect_filled(
                Rect::from_min_max(p(-0.42, -0.30), p(0.42, 0.30)),
                0.0,
                color,
            );
        }
        Icon::WhiteScreen => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.42, -0.30), p(0.42, 0.30)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Keyboard => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.46, -0.28), p(0.46, 0.28)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            for row in [-0.1, 0.06] {
                for column in [-0.24, -0.08, 0.08, 0.24] {
                    painter.rect_filled(
                        Rect::from_center_size(p(column, row), Vec2::splat(s * 0.08)),
                        0.0,
                        color,
                    );
                }
            }
            painter.line_segment(
                [p(-0.16, 0.18), p(0.16, 0.18)],
                Stroke::new(stroke.width * 0.8, color),
            );
        }
        Icon::More => {
            for x in [-0.30, 0.0, 0.30] {
                painter.rect_filled(
                    Rect::from_center_size(p(x, 0.0), Vec2::splat(s * 0.15)),
                    0.0,
                    color,
                );
            }
        }
        Icon::Plus => {
            painter.line_segment([p(0.0, -0.36), p(0.0, 0.36)], stroke);
            painter.line_segment([p(-0.36, 0.0), p(0.36, 0.0)], stroke);
        }
        Icon::Folder => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(-0.44, -0.30),
                    p(-0.10, -0.30),
                    p(0.02, -0.18),
                    p(0.44, -0.18),
                    p(0.44, 0.34),
                    p(-0.44, 0.34),
                ],
                stroke,
            ));
            painter.line_segment([p(-0.44, -0.06), p(0.44, -0.06)], stroke);
        }
        Icon::Code => {
            painter.line(vec![p(-0.16, -0.28), p(-0.42, 0.0), p(-0.16, 0.28)], stroke);
            painter.line(vec![p(0.16, -0.28), p(0.42, 0.0), p(0.16, 0.28)], stroke);
            painter.line_segment([p(0.07, -0.36), p(-0.07, 0.36)], stroke);
        }
        Icon::Book => {
            for side in [-1.0, 1.0] {
                painter.add(egui::Shape::closed_line(
                    vec![
                        p(0.0, -0.26),
                        p(side * 0.44, -0.36),
                        p(side * 0.44, 0.28),
                        p(0.0, 0.38),
                    ],
                    stroke,
                ));
            }
        }
        Icon::ExternalLink => {
            painter.line(
                vec![
                    p(-0.06, -0.40),
                    p(-0.40, -0.40),
                    p(-0.40, 0.40),
                    p(0.40, 0.40),
                    p(0.40, 0.06),
                ],
                stroke,
            );
            painter.line(vec![p(0.08, -0.40), p(0.40, -0.40), p(0.40, -0.08)], stroke);
            painter.line_segment([p(0.40, -0.40), p(-0.02, 0.02)], stroke);
        }
        Icon::Package => {
            let top = [p(0.0, -0.42), p(0.40, -0.21), p(0.0, 0.0), p(-0.40, -0.21)];
            painter.add(egui::Shape::closed_line(top.to_vec(), stroke));
            painter.line(
                vec![
                    p(-0.40, -0.21),
                    p(-0.40, 0.21),
                    p(0.0, 0.42),
                    p(0.40, 0.21),
                    p(0.40, -0.21),
                ],
                stroke,
            );
            painter.line_segment([p(0.0, 0.0), p(0.0, 0.42)], stroke);
        }
        Icon::Mic => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.14, -0.44), p(0.14, 0.10)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line(
                vec![
                    p(-0.30, -0.08),
                    p(-0.30, 0.10),
                    p(-0.16, 0.25),
                    p(0.16, 0.25),
                    p(0.30, 0.10),
                    p(0.30, -0.08),
                ],
                stroke,
            );
            painter.line_segment([p(0.0, 0.25), p(0.0, 0.42)], stroke);
            painter.line_segment([p(-0.16, 0.42), p(0.16, 0.42)], stroke);
        }
        Icon::Record => {
            painter.circle_filled(c, s * 0.3, color);
        }
        Icon::Move => {
            painter.line_segment([p(-0.42, 0.0), p(0.42, 0.0)], stroke);
            painter.line_segment([p(0.0, -0.42), p(0.0, 0.42)], stroke);
            let head = 0.13;
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let tip = (dx * 0.42, dy * 0.42);
                let back = (tip.0 - dx * head, tip.1 - dy * head);
                painter.line(
                    vec![
                        p(back.0 - dy * head, back.1 - dx * head),
                        p(tip.0, tip.1),
                        p(back.0 + dy * head, back.1 + dx * head),
                    ],
                    stroke,
                );
            }
        }
        Icon::Reset => {
            // Open circle whose arrowhead turns counter-clockwise.
            let radius = 0.34;
            let start = 240.0_f32.to_radians();
            let sweep = -290.0_f32.to_radians();
            let points: Vec<Pos2> = (0..=28)
                .map(|i| {
                    let a = start + sweep * i as f32 / 28.0;
                    p(radius * a.cos(), radius * a.sin())
                })
                .collect();
            let tip = *points.last().expect("arc has points");
            painter.line(points, stroke);
            let end = start + sweep;
            let tangent = vec2(end.sin(), -end.cos());
            let normal = vec2(end.cos(), end.sin());
            let head = s * 0.2;
            fill_poly(vec![
                tip + tangent * head * 0.9,
                tip - normal * head * 0.75,
                tip + normal * head * 0.75,
            ]);
        }
        Icon::Volume | Icon::VolumeMuted => {
            // The speaker: a box and its cone, each convex.
            fill_poly(vec![
                p(-0.40, -0.13),
                p(-0.20, -0.13),
                p(-0.20, 0.13),
                p(-0.40, 0.13),
            ]);
            fill_poly(vec![
                p(-0.20, -0.13),
                p(0.02, -0.36),
                p(0.02, 0.36),
                p(-0.20, 0.13),
            ]);
            if icon == Icon::VolumeMuted {
                painter.line_segment([p(0.16, -0.14), p(0.42, 0.14)], stroke);
                painter.line_segment([p(0.16, 0.14), p(0.42, -0.14)], stroke);
            } else {
                for radius in [0.18_f32, 0.34] {
                    let points: Vec<Pos2> = (0..=12)
                        .map(|step| {
                            let angle = (-50.0 + 100.0 * step as f32 / 12.0).to_radians();
                            p(0.02 + radius * angle.cos(), radius * angle.sin())
                        })
                        .collect();
                    painter.line(points, stroke);
                }
            }
        }
        Icon::Link => {
            // Two rounded links along the diagonal, overlapping in the middle.
            let k = std::f32::consts::FRAC_1_SQRT_2;
            let (half_length, radius) = (0.13, 0.14);
            for center in [-0.15_f32, 0.15] {
                let mut points = Vec::with_capacity(35);
                for (end, start) in [(half_length, -90.0_f32), (-half_length, 90.0)] {
                    for step in 0..=16 {
                        let angle = (start + 180.0 * step as f32 / 16.0).to_radians();
                        let (u, v) = (center + end + radius * angle.cos(), radius * angle.sin());
                        points.push(p((u - v) * k, -(u + v) * k));
                    }
                }
                points.push(points[0]);
                painter.line(points, stroke);
            }
        }
        Icon::Layout => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.42, -0.34), p(0.42, 0.34)),
                s * 0.06,
                stroke,
                egui::StrokeKind::Middle,
            );
            let thin = Stroke::new(stroke.width * 0.8, color);
            for (left, right) in [(-0.28, -0.04), (0.04, 0.28)] {
                painter.rect_stroke(
                    Rect::from_min_max(p(left, -0.2), p(right, 0.2)),
                    0.0,
                    thin,
                    egui::StrokeKind::Middle,
                );
            }
        }
        Icon::Thirds => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.42, -0.32), p(0.42, 0.32)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            let thin = Stroke::new(stroke.width * 0.7, color);
            for x in [-0.14, 0.14] {
                painter.line_segment([p(x, -0.32), p(x, 0.32)], thin);
            }
            for y in [-0.107, 0.107] {
                painter.line_segment([p(-0.42, y), p(0.42, y)], thin);
            }
        }
        Icon::Crosshair => {
            painter.circle_stroke(c, s * 0.24, stroke);
            for (from, to) in [
                ((-0.44, 0.0), (-0.24, 0.0)),
                ((0.24, 0.0), (0.44, 0.0)),
                ((0.0, -0.44), (0.0, -0.24)),
                ((0.0, 0.24), (0.0, 0.44)),
            ] {
                painter.line_segment([p(from.0, from.1), p(to.0, to.1)], stroke);
            }
            painter.circle_filled(c, s * 0.06, color);
        }
        Icon::Inspect => {
            painter.circle_stroke(p(-0.08, -0.08), s * 0.26, stroke);
            painter.line_segment([p(0.12, 0.12), p(0.40, 0.40)], stroke);
        }
        Icon::Settings => {
            let thin = Stroke::new(stroke.width * 0.8, color);
            for (y, knob) in [(-0.28, 0.16), (0.0, -0.18), (0.28, 0.06)] {
                painter.line_segment([p(-0.42, y), p(0.42, y)], thin);
                painter.rect_filled(
                    Rect::from_center_size(p(knob, y), Vec2::splat(s * 0.2)),
                    0.0,
                    color,
                );
            }
        }
        Icon::List => {
            for y in [-0.28, 0.0, 0.28] {
                painter.rect_filled(
                    Rect::from_center_size(p(-0.30, y), Vec2::splat(s * 0.14)),
                    0.0,
                    color,
                );
                painter.line_segment([p(-0.12, y), p(0.42, y)], stroke);
            }
        }
        Icon::Home => {
            painter.line(vec![p(-0.42, -0.02), p(0.0, -0.40), p(0.42, -0.02)], stroke);
            painter.line(
                vec![
                    p(-0.28, -0.14),
                    p(-0.28, 0.38),
                    p(0.28, 0.38),
                    p(0.28, -0.14),
                ],
                stroke,
            );
            painter.rect_filled(
                Rect::from_min_max(p(-0.08, 0.12), p(0.08, 0.38)),
                0.0,
                color,
            );
        }
        Icon::Grip => {
            for x in [-0.14, 0.14] {
                for y in [-0.28, 0.0, 0.28] {
                    painter.circle_filled(p(x, y), s * 0.075, color);
                }
            }
        }
        Icon::Timeline => {
            let thin = Stroke::new(stroke.width * 0.8, color);
            for (y, from, to) in [(-0.26, -0.42, 0.05), (0.0, -0.15, 0.3), (0.26, 0.1, 0.42)] {
                painter.line_segment(
                    [p(from, y), p(to, y)],
                    Stroke::new(stroke.width * 1.6, color),
                );
            }
            painter.line_segment([p(-0.05, -0.44), p(-0.05, 0.44)], thin);
        }
        Icon::Capture => {
            for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                painter.line(
                    vec![
                        p(x * 0.42, y * 0.14),
                        p(x * 0.42, y * 0.34),
                        p(x * 0.22, y * 0.34),
                    ],
                    stroke,
                );
            }
            painter.circle_filled(c, s * 0.11, color);
        }
        Icon::Console => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.44, -0.34), p(0.44, 0.34)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            let thin = Stroke::new(stroke.width * 0.85, color);
            painter.line(vec![p(-0.26, -0.12), p(-0.10, 0.0), p(-0.26, 0.12)], thin);
            painter.line_segment([p(0.0, 0.14), p(0.24, 0.14)], thin);
        }
    }
}

/// Square every corner egui draws on its own (windows, menus, tooltips,
/// buttons, scroll bars, slider handles) to match the painted controls.
pub(crate) fn square_corners(style: &mut egui::Style) {
    let visuals = &mut style.visuals;
    // egui's own widgets in the brand's colours: violet selection and links,
    // graphite fields without egui's blue tint.
    visuals.selection.bg_fill = palette::LOOP.gamma_multiply(0.55);
    visuals.selection.stroke = Stroke::new(1.0, palette::ACCENT);
    visuals.hyperlink_color = palette::ACCENT;
    visuals.extreme_bg_color = Color32::from_rgb(12, 12, 12);
    visuals.text_edit_bg_color = Some(Color32::from_rgb(12, 12, 12));
    visuals.faint_bg_color = Color32::from_rgb(24, 24, 24);
    visuals.window_fill = palette::SURFACE;
    visuals.panel_fill = palette::GRAPHITE;
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(40, 40, 42);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(34, 34, 36);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(52, 52, 56);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(46, 46, 50);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette::ACCENT.gamma_multiply(0.6));
    visuals.widgets.active.bg_fill = Color32::from_rgb(62, 60, 72);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(56, 54, 66);
    visuals.widgets.open.weak_bg_fill = Color32::from_rgb(40, 40, 42);
    // Fields, sliders and drop-downs: a faint frame at rest, violet under
    // the pointer and while open, paper text when touched.
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(52, 52, 56));
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette::TEXT_MUTED);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, palette::TEXT);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette::ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, palette::TEXT);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, palette::ACCENT);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, palette::TEXT);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette::DIVIDER);
    // A slider fills its rail in violet up to the knob, like a seek track.
    visuals.slider_trailing_fill = true;
    style.spacing.slider_rail_height = 4.0;
    // Drop-down lists and menus: a graphite card with a hairline.
    visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(44, 44, 46));
    visuals.popup_shadow = egui::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(130),
    };
    visuals.window_corner_radius = egui::CornerRadius::ZERO;
    visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    visuals.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::ZERO;
    }
}

/// Floating card used by modal dialogs.
pub(crate) fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(palette::SURFACE)
        .inner_margin(egui::Margin::same(22))
        .stroke(Stroke::new(1.0, palette::PANEL_STROKE))
        .shadow(egui::Shadow {
            offset: [0, 12],
            blur: 40,
            spread: 0,
            color: Color32::from_black_alpha(140),
        })
}

/// Input surface for text edits and numeric fields.
pub(crate) fn field_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(palette::FIELD)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .stroke(Stroke::new(1.0, palette::PANEL_STROKE))
}

/// Caption above a group of controls.
pub(crate) fn section_label(ui: &mut Ui, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .monospace()
                .size(11.5)
                .color(palette::TEXT_MUTED),
        )
        .selectable(false),
    );
}

/// A check box in the brand's style: a square that fills violet with an
/// ink check when on, framed violet under the pointer.
pub(crate) fn checkbox(
    ui: &mut Ui,
    checked: &mut bool,
    text: impl Into<egui::WidgetText>,
) -> Response {
    const SIDE: f32 = 16.0;
    const GAP: f32 = 8.0;
    let galley = text.into().into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        (ui.available_width() - SIDE - GAP).max(40.0),
        egui::TextStyle::Body,
    );
    let size = vec2(SIDE + GAP + galley.size().x, SIDE.max(galley.size().y));
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, *checked, galley.text())
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let hovered = enabled && response.hovered();
    let dim = |color: Color32| {
        if enabled {
            color
        } else {
            color.gamma_multiply(0.45)
        }
    };
    let square = Rect::from_min_size(
        pos2(
            rect.min.x,
            rect.min.y + (galley.size().y.max(SIDE) - SIDE) / 2.0,
        ),
        Vec2::splat(SIDE),
    );
    let painter = ui.painter();
    let (fill, stroke) = match (*checked, hovered) {
        (true, true) => (palette::ACCENT, palette::ACCENT),
        (true, false) => (palette::LOOP, palette::LOOP),
        (false, true) => (Color32::from_rgb(12, 12, 12), palette::ACCENT),
        (false, false) => (Color32::from_rgb(12, 12, 12), Color32::from_rgb(78, 76, 88)),
    };
    painter.rect_filled(square, 0.0, dim(fill));
    painter.rect_stroke(
        square,
        0.0,
        Stroke::new(1.0, dim(stroke)),
        egui::StrokeKind::Inside,
    );
    if *checked {
        let at = |x: f32, y: f32| square.min + vec2(x, y) * SIDE;
        painter.line(
            vec![at(0.24, 0.52), at(0.43, 0.70), at(0.78, 0.31)],
            Stroke::new(2.0, dim(palette::INK)),
        );
    }
    let text_color = if enabled {
        palette::TEXT
    } else {
        palette::TEXT_FAINT
    };
    painter.galley(
        pos2(square.max.x + GAP, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    response.on_hover_cursor(if enabled {
        egui::CursorIcon::PointingHand
    } else {
        egui::CursorIcon::Default
    })
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
    painter.rect_filled(rect, 0.0, palette::FIELD);
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
            painter.rect_filled(segment, 0.0, palette::SELECTED);
            painter.rect_stroke(
                segment,
                0.0,
                Stroke::new(1.0, Color32::from_white_alpha(20)),
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() {
            painter.rect_filled(segment, 0.0, palette::HOVER);
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

/// Compact tag used for presets.
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
    painter.rect_filled(rect, 0.0, fill);
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

/// The size of a text button: its font and its height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ButtonSize {
    pub font: f32,
    pub height: f32,
}

impl ButtonSize {
    pub(crate) const REGULAR: Self = Self {
        font: 13.0,
        height: 34.0,
    };
    /// For panels read while speaking, such as Script & narration.
    pub(crate) const LARGE: Self = Self {
        font: 15.0,
        height: 38.0,
    };
}

/// Filled call-to-action button with an optional leading icon.
pub(crate) fn primary_button(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    enabled: bool,
) -> Response {
    text_button(ui, label, icon, enabled, true, ButtonSize::REGULAR)
}

/// [`primary_button`] at `size`.
pub(crate) fn primary_button_sized(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    enabled: bool,
    size: ButtonSize,
) -> Response {
    text_button(ui, label, icon, enabled, true, size)
}

/// Quiet button for secondary actions.
pub(crate) fn secondary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    text_button(ui, label, None, enabled, false, ButtonSize::REGULAR)
}

/// [`secondary_button`] at `size`.
pub(crate) fn secondary_button_sized(
    ui: &mut Ui,
    label: &str,
    enabled: bool,
    size: ButtonSize,
) -> Response {
    text_button(ui, label, None, enabled, false, size)
}

/// The brand's hover (`--frames` on the docs site): under the pointer the
/// element steps 3 px to the right in three frames over 0.15 s, leaving two
/// violet ghosts of its edge behind, as onion-skin frames of a tween. Returns
/// where to draw the element now.
pub(crate) fn frame_hover(ui: &Ui, id: egui::Id, rect: Rect, hovered: bool) -> Rect {
    let t = ui
        .ctx()
        .animate_bool_with_time(id.with("frame-hover"), hovered, 0.15);
    // steps(3): whole pixels, never in between.
    let shift = (t * 3.0).floor().min(3.0);
    if shift >= 1.0 {
        let strip = |from: f32, to: f32| {
            Rect::from_min_max(
                pos2(rect.min.x + from, rect.min.y),
                pos2(rect.min.x + to, rect.max.y),
            )
        };
        let painter = ui.painter();
        painter.rect_filled(
            strip(shift - 6.0, shift - 3.0),
            0.0,
            palette::ACCENT.gamma_multiply(0.2),
        );
        painter.rect_filled(
            strip(shift - 3.0, shift),
            0.0,
            palette::ACCENT.gamma_multiply(0.45),
        );
    }
    rect.translate(vec2(shift, 0.0))
}

fn text_button(
    ui: &mut Ui,
    label: &str,
    icon: Option<Icon>,
    enabled: bool,
    primary: bool,
    button: ButtonSize,
) -> Response {
    let font = FontId::proportional(button.font);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), palette::TEXT);
    let icon_side = (button.font * 14.0 / 13.0).round();
    let icon_w = if icon.is_some() { icon_side + 6.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 32.0, button.height);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();
    let rect = frame_hover(ui, response.id, rect, hovered);
    let painter = ui.painter();
    let (fill, text) = match (primary, enabled) {
        (true, true) => (
            if pressed {
                palette::ACCENT.gamma_multiply(0.85)
            } else if hovered {
                palette::ACCENT_HOVER
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
    painter.rect_filled(rect, 0.0, fill);
    let content_w = galley.size().x + icon_w;
    let start_x = rect.center().x - content_w / 2.0;
    if let Some(icon) = icon {
        paint_icon(
            painter,
            Rect::from_center_size(
                pos2(start_x + icon_side / 2.0, rect.center().y),
                Vec2::splat(icon_side),
            ),
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

/// Square badge with an icon, used as the headline of result dialogs.
pub(crate) fn status_badge(ui: &mut Ui, icon: Icon, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, color.gamma_multiply(0.18));
    paint_icon(
        painter,
        Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
        icon,
        color,
    );
}

/// Mode toggle with an icon and an optional label. `color` tints it
/// while active (e.g. amber for modes that hide the audience screen).
pub(crate) fn pill_toggle(
    ui: &mut Ui,
    icon: Icon,
    label: Option<&str>,
    active: bool,
    color: Color32,
    height: f32,
) -> Response {
    let font = FontId::proportional(14.0);
    let galley = label.map(|label| {
        ui.painter()
            .layout_no_wrap(label.to_owned(), font.clone(), palette::TEXT)
    });
    let icon_side = 16.0;
    let text_w = galley.as_ref().map_or(0.0, |galley| galley.size().x + 8.0);
    let width = (icon_side + text_w + 28.0).max(height);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let painter = ui.painter();
    let hovered = response.hovered();
    let fill = if active {
        color.gamma_multiply(if hovered { 0.3 } else { 0.22 })
    } else if hovered {
        palette::HOVER
    } else {
        palette::FIELD
    };
    painter.rect_filled(rect, 0.0, fill);
    let tint = if active {
        color
    } else if hovered {
        palette::TEXT
    } else {
        palette::TEXT_MUTED
    };
    let start = rect.center().x - (icon_side + text_w) / 2.0;
    paint_icon(
        painter,
        Rect::from_center_size(
            pos2(start + icon_side / 2.0, rect.center().y),
            Vec2::splat(icon_side),
        ),
        icon,
        tint,
    );
    if let Some(label) = label {
        painter.text(
            pos2(start + icon_side + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            font,
            tint,
        );
    }
    response
}

/// Compact text button for secondary, in-panel actions (e.g. `A+`).
pub(crate) fn small_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    small_button_sized(ui, label, enabled, 13.0)
}

/// [`small_button`] with its text at `font` points.
pub(crate) fn small_button_sized(ui: &mut Ui, label: &str, enabled: bool, font: f32) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).size(font).color(if enabled {
            palette::TEXT
        } else {
            palette::TEXT_FAINT
        }))
        .fill(palette::FIELD)
        .stroke(Stroke::NONE)
        .corner_radius(0.0)
        .min_size(vec2(font * 34.0 / 13.0, font * 2.0)),
    )
}

/// Letter-spaced caption used as a section heading.
pub(crate) fn caption(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .monospace()
        .size(11.0)
        .strong()
        .extra_letter_spacing(1.2)
        .color(palette::TEXT_FAINT)
}

/// What happened this frame on the header that drags a panel's window.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct HeaderDrag {
    /// How far the header was dragged.
    pub delta: Vec2,
    /// A double click: dock the window again.
    pub dock: bool,
    /// Under the pointer or being dragged.
    pub active: bool,
}

/// Make the next `height` points of `ui`, across its width, the handle that
/// drags its window. Call it before drawing the header: the controls drawn
/// after it stay clickable on top of it.
pub(crate) fn drag_handle(ui: &mut Ui, height: f32) -> HeaderDrag {
    let row = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), height));
    let handle = ui
        .interact(row, ui.id().with("drag_handle"), Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::Grab)
        .on_hover_text("Drag to move · double click: dock it again");
    let dragged = handle.dragged();
    if dragged {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    HeaderDrag {
        delta: if dragged {
            handle.drag_delta()
        } else {
            Vec2::ZERO
        },
        dock: handle.double_clicked(),
        active: dragged || handle.hovered(),
    }
}

/// The dots that show a header drags its window.
pub(crate) fn grip(ui: &mut Ui, drag: &HeaderDrag) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 14.0), Sense::hover());
    let color = if drag.active {
        palette::TEXT_MUTED
    } else {
        palette::TEXT_FAINT
    };
    paint_icon(ui.painter(), rect, Icon::Grip, color);
}

/// `window` at `position`, where its header dragged it, or docked at
/// `align` and `offset` while `position` is `None`.
pub(crate) fn placed<'a>(
    window: egui::Window<'a>,
    position: Option<Pos2>,
    align: Align2,
    offset: Vec2,
) -> egui::Window<'a> {
    match position {
        // Only the header moves it, so text inside stays selectable.
        Some(position) => window.movable(false).current_pos(position),
        None => window.anchor(align, offset),
    }
}

/// Where a draggable window goes after `drag`: docked again (`None`) on a
/// double click, or moved from where it was drawn (`rect`), kept inside
/// `screen`, which may have shrunk since.
pub(crate) fn moved(
    position: Option<Pos2>,
    rect: Option<Rect>,
    drag: &HeaderDrag,
    screen: Rect,
) -> Option<Pos2> {
    if drag.dock {
        return None;
    }
    let rect = rect?;
    let position = match position {
        None if drag.delta == Vec2::ZERO => return None,
        None => rect.min,
        Some(position) => position,
    } + drag.delta;
    let max = (screen.max - rect.size()).max(screen.min);
    Some(position.clamp(screen.min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_drags_its_window_and_a_double_click_docks_it() {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0));
        let docked = Rect::from_min_size(pos2(988.0, 62.0), vec2(280.0, 400.0));
        let drag = |x: f32, y: f32| HeaderDrag {
            delta: vec2(x, y),
            ..HeaderDrag::default()
        };
        let still = HeaderDrag::default();
        // Docked until dragged; the first drag starts where it was drawn.
        assert_eq!(moved(None, Some(docked), &still, screen), None);
        let position = moved(None, Some(docked), &drag(-300.0, 20.0), screen);
        assert_eq!(position, Some(pos2(688.0, 82.0)));
        // Later drags add up, and it stays where it was left.
        let rect = Rect::from_min_size(position.unwrap(), docked.size());
        assert_eq!(
            moved(position, Some(rect), &drag(-10.0, 0.0), screen),
            Some(pos2(678.0, 82.0))
        );
        assert_eq!(moved(position, Some(rect), &still, screen), position);
        // It never leaves the screen, also when the screen shrinks.
        assert_eq!(
            moved(position, Some(rect), &drag(-5000.0, 5000.0), screen),
            Some(pos2(0.0, 320.0))
        );
        let small = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 300.0));
        assert_eq!(
            moved(position, Some(rect), &still, small),
            Some(pos2(520.0, 0.0))
        );
        // A double click docks it again.
        let dock = HeaderDrag {
            dock: true,
            ..HeaderDrag::default()
        };
        assert_eq!(moved(position, Some(rect), &dock, screen), None);
    }
}
