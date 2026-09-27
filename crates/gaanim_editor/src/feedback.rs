//! Script feedback drawn over the preview: the reload badge and the error
//! panel, in the visual language of [`crate::ui_kit`].

use bevy_egui::egui;

use crate::ui_kit::{self, Icon, palette};

/// What the user did with the error panel this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPanelAction {
    None,
    Close,
}

/// Headline of a script error: the exception line of a Python traceback, or
/// the first line of any other message, plus where it was raised.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ErrorSummary<'a> {
    headline: &'a str,
    /// `file.py · línea N` of the innermost traceback frame.
    location: Option<String>,
}

fn summarize_error(message: &str) -> ErrorSummary<'_> {
    let mut lines = message.lines().filter(|line| !line.trim().is_empty());
    // The runner prefixes Python tracebacks with `<script> — traceback:`.
    if !message
        .lines()
        .any(|line| line.starts_with("Traceback (most recent call last)"))
    {
        return ErrorSummary {
            headline: lines.next().unwrap_or("").trim(),
            location: None,
        };
    }
    let headline = message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty() && !line.starts_with(char::is_whitespace))
        .unwrap_or("")
        .trim();
    let location = lines
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("File \"")?;
            let (path, rest) = rest.split_once('"')?;
            let number = rest.strip_prefix(", line ")?.split(',').next()?;
            number.parse::<u32>().ok()?;
            let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
            Some(format!("{file} · línea {number}"))
        })
        .next_back();
    ErrorSummary { headline, location }
}

/// Badge confirming a reload, faded by `opacity` while it disappears.
/// `top` moves it below other top-anchored bars.
pub fn reload_badge(ctx: &egui::Context, message: &str, opacity: f32, top: f32) {
    egui::Area::new("reload_status".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, top))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(opacity);
            egui::Frame::new()
                .fill(palette::PANEL)
                .stroke(egui::Stroke::new(1.0, palette::PANEL_STROKE))
                .inner_margin(egui::Margin::symmetric(10, 7))
                .shadow(egui::Shadow {
                    offset: [0, 6],
                    blur: 20,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(90),
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let (rect, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(14.0), egui::Sense::hover());
                        ui_kit::paint_icon(ui.painter(), rect, Icon::Check, palette::LOOP);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(message)
                                    .size(12.5)
                                    .color(palette::TEXT_MUTED),
                            )
                            .selectable(false),
                        );
                    });
                });
        });
}

/// Panel with the full script error. It stays open until the next successful
/// reload, `Esc`, or its close button.
pub fn script_error_panel(ctx: &egui::Context, message: &str) -> ErrorPanelAction {
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        return ErrorPanelAction::Close;
    }
    let summary = summarize_error(message);
    let mut action = ErrorPanelAction::None;
    let max_width = (ctx.viewport_rect().width() - 48.0).max(320.0);
    egui::Window::new("Error en el script")
        .id(egui::Id::new("script_error_window"))
        .title_bar(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .resizable(true)
        .collapsible(false)
        .default_width(720.0_f32.min(max_width))
        .max_width(max_width)
        .frame(
            ui_kit::card_frame()
                .inner_margin(egui::Margin::same(18))
                .stroke(egui::Stroke::new(1.0, palette::DANGER.gamma_multiply(0.45))),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui_kit::status_badge(ui, Icon::Warning, palette::DANGER);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.label(ui_kit::caption("ERROR EN EL SCRIPT"));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(summary.headline)
                                .size(15.0)
                                .strong()
                                .color(palette::TEXT),
                        )
                        .wrap(),
                    );
                    if let Some(location) = &summary.location {
                        ui.label(
                            egui::RichText::new(location)
                                .size(12.0)
                                .color(palette::TEXT_MUTED),
                        );
                    }
                });
            });
            ui_kit::field_frame().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height((ctx.viewport_rect().height() * 0.45).max(120.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(message)
                                    .monospace()
                                    .size(12.0)
                                    .color(palette::TEXT_MUTED),
                            )
                            .wrap(),
                        );
                    });
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Corrige el archivo y guárdalo: el editor lo vuelve a cargar solo.",
                        )
                        .size(12.0)
                        .color(palette::TEXT_FAINT),
                    )
                    .wrap(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui_kit::secondary_button(ui, "Cerrar · Esc", true).clicked() {
                        action = ErrorPanelAction::Close;
                    }
                    if ui_kit::secondary_button(ui, "Copiar", true)
                        .on_hover_text("Copiar el error completo")
                        .clicked()
                    {
                        ctx.copy_text(message.to_owned());
                    }
                });
            });
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_traceback_is_summarized_by_its_exception_and_innermost_frame() {
        let traceback = "/work/main.py — traceback:\nTraceback (most recent call last):\n  File \"/home/me/project/main.py\", line 4, in <module>\n    helper()\n  File \"C:\\\\scenes\\\\lib.py\", line 12, in helper\n    1 / 0\n    ~~^~~\nZeroDivisionError: division by zero\n";
        assert_eq!(
            summarize_error(traceback),
            ErrorSummary {
                headline: "ZeroDivisionError: division by zero",
                location: Some("lib.py · línea 12".to_owned()),
            }
        );
    }

    #[test]
    fn other_errors_use_their_first_line() {
        assert_eq!(
            summarize_error("\nCustom animation failed for 3 at alpha 0.5\nboom\n"),
            ErrorSummary {
                headline: "Custom animation failed for 3 at alpha 0.5",
                location: None,
            }
        );
        assert_eq!(summarize_error("").headline, "");
    }
}
