//! Script feedback drawn over the preview: the reload badge and the error
//! panel, in the visual language of [`crate::ui_kit`].

use bevy_egui::egui;
use gaanim_core::console::ScriptLocation;

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
    /// The innermost script frame of the traceback.
    location: Option<ScriptLocation>,
}

fn summarize_error(message: &str) -> ErrorSummary<'_> {
    let (headline, location) = gaanim_core::console::traceback_summary(message);
    ErrorSummary { headline, location }
}

/// `file.py · línea N`, how the editor names a script line.
pub(crate) fn location_label(location: &ScriptLocation) -> String {
    format!("{} · línea {}", location.file_name(), location.line)
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
                            egui::RichText::new(location_label(location))
                                .size(12.0)
                                .color(palette::TEXT_MUTED),
                        )
                        .on_hover_text(location.to_string());
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
                    if let Some(location) = &summary.location
                        && crate::source_link::AVAILABLE
                        && ui_kit::secondary_button(ui, "Abrir en el editor", true)
                            .on_hover_text(format!("{location}\n{}", crate::source_link::hint()))
                            .clicked()
                    {
                        crate::source_link::open_or_report(location);
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
        let summary = summarize_error(traceback);
        assert_eq!(summary.headline, "ZeroDivisionError: division by zero");
        assert_eq!(
            summary.location.as_ref().map(location_label).as_deref(),
            Some("lib.py · línea 12")
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
