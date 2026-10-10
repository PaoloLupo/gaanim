//! The console: the status lines, warnings, script output and errors of the
//! scene, which otherwise only reach the terminal. `J` shows or hides it; a
//! badge counts the warnings and errors of the last run while it is hidden.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui, input::EguiWantsInput};
use gaanim_core::console::{self, Level, LogEntry};

use crate::PresentationMode;
use crate::ui_kit::{self, ButtonTone, Icon, caption, icon_button_sized, palette};

/// The console panel and the log entries it shows.
#[derive(Resource, Debug, Clone, Default)]
pub struct ConsolePanel {
    pub open: bool,
    /// Entries read from the log so far, oldest first.
    entries: Vec<LogEntry>,
    /// The last entry read from the log.
    last_seq: u64,
    /// Entries up to this one were cleared.
    cleared_through: u64,
    filter: ConsoleFilter,
    /// The entry whose whole message and detail show under the list.
    selected: Option<u64>,
}

/// Which entries the console lists.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConsoleFilter {
    errors: bool,
    warnings: bool,
    messages: bool,
    /// Text that the label, message or file must contain, ignoring case.
    query: String,
    /// Keep the entries of earlier runs of the script.
    history: bool,
}

impl Default for ConsoleFilter {
    fn default() -> Self {
        Self {
            errors: true,
            warnings: true,
            messages: true,
            query: String::new(),
            history: false,
        }
    }
}

/// The three groups the console filters by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Error,
    Warning,
    Message,
}

fn severity(level: Level) -> Severity {
    match level {
        Level::Error => Severity::Error,
        Level::Warn => Severity::Warning,
        Level::Debug | Level::Info | Level::Success => Severity::Message,
    }
}

impl ConsoleFilter {
    fn shows_severity(&self, severity: Severity) -> bool {
        match severity {
            Severity::Error => self.errors,
            Severity::Warning => self.warnings,
            Severity::Message => self.messages,
        }
    }

    fn matches_query(&self, entry: &LogEntry) -> bool {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        entry.message.to_lowercase().contains(&query)
            || entry.label.to_lowercase().contains(&query)
            || entry
                .location
                .as_ref()
                .is_some_and(|location| location.to_string().to_lowercase().contains(&query))
    }
}

/// Errors, warnings and other messages counted by [`ConsolePanel::counts`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Counts {
    errors: usize,
    warnings: usize,
    messages: usize,
}

impl ConsolePanel {
    /// Read the entries logged since the last call.
    fn sync(&mut self) {
        let new = console::entries_since(self.last_seq);
        let Some(last) = new.last() else {
            return;
        };
        self.last_seq = last.seq;
        self.entries.extend(new);
        let excess = self.entries.len().saturating_sub(console::LOG_CAPACITY);
        self.entries.drain(..excess);
    }

    /// Entries not cleared and, without history, of the current run.
    fn in_scope(&self, run: u64) -> impl Iterator<Item = &LogEntry> {
        let history = self.filter.history;
        let cleared = self.cleared_through;
        self.entries
            .iter()
            .filter(move |entry| entry.seq > cleared && (history || entry.run == run))
    }

    fn counts(&self, run: u64) -> Counts {
        let mut counts = Counts::default();
        for entry in self.in_scope(run) {
            match severity(entry.level) {
                Severity::Error => counts.errors += 1,
                Severity::Warning => counts.warnings += 1,
                Severity::Message => counts.messages += 1,
            }
        }
        counts
    }

    /// Positions in `entries` of the entries the list shows.
    fn visible_indices(&self, run: u64) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.seq > self.cleared_through && (self.filter.history || entry.run == run)
            })
            .filter(|(_, entry)| self.filter.shows_severity(severity(entry.level)))
            .filter(|(_, entry)| self.filter.matches_query(entry))
            .map(|(index, _)| index)
            .collect()
    }

    #[cfg(test)]
    fn visible(&self, run: u64) -> Vec<&LogEntry> {
        self.visible_indices(run)
            .into_iter()
            .map(|index| &self.entries[index])
            .collect()
    }
}

/// Read new log entries every frame, so counts stay current while the
/// panel is hidden.
pub fn console_sync_system(mut panel: ResMut<ConsolePanel>) {
    panel.sync();
}

/// `J` shows or hides the console.
pub fn console_keys_system(
    egui_wants: Res<EguiWantsInput>,
    keys: Res<ButtonInput<KeyCode>>,
    presentation: Res<PresentationMode>,
    mut panel: ResMut<ConsolePanel>,
) {
    if presentation.active || egui_wants.wants_keyboard_input() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        panel.open = !panel.open;
    }
}

/// The console window, or the badge that counts problems while it is hidden.
pub fn console_panel_system(
    mut contexts: EguiContexts,
    presentation: Res<PresentationMode>,
    mut panel: ResMut<ConsolePanel>,
    animations: Res<crate::animation_timeline::AnimationTimeline>,
) {
    if presentation.active {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let run = console::current_run();
    let counts = panel.counts(run);
    if !panel.open {
        if counts.errors + counts.warnings > 0 && problems_badge(ctx, counts) {
            panel.open = true;
        }
        return;
    }
    let panel = &mut *panel;
    let mut close = false;
    let max_width = (ctx.viewport_rect().width() - 24.0).max(320.0);
    egui::Window::new("Consola")
        .id(egui::Id::new("editor_console"))
        .title_bar(false)
        // Above the animation timeline when it is open.
        .anchor(
            egui::Align2::LEFT_BOTTOM,
            egui::vec2(
                12.0,
                -96.0
                    - if animations.height > 0.0 {
                        animations.height + 8.0
                    } else {
                        0.0
                    },
            ),
        )
        .order(egui::Order::Foreground)
        .resizable(false)
        .collapsible(false)
        .frame(ui_kit::card_frame().inner_margin(egui::Margin::same(12)))
        .show(ctx, |ui| {
            let width = 620.0_f32.min(max_width);
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(caption("CONSOLA"));
                ui.add_space(6.0);
                let filter = &mut panel.filter;
                if ui_kit::chip(ui, &format!("Errores {}", counts.errors), filter.errors).clicked()
                {
                    filter.errors = !filter.errors;
                }
                if ui_kit::chip(ui, &format!("Avisos {}", counts.warnings), filter.warnings)
                    .clicked()
                {
                    filter.warnings = !filter.warnings;
                }
                if ui_kit::chip(
                    ui,
                    &format!("Mensajes {}", counts.messages),
                    filter.messages,
                )
                .clicked()
                {
                    filter.messages = !filter.messages;
                }
                if ui_kit::chip(ui, "Historial", filter.history)
                    .on_hover_text("Conservar los mensajes de las recargas anteriores")
                    .clicked()
                {
                    filter.history = !filter.history;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button_sized(ui, Icon::Close, ButtonTone::Ghost, true, 24.0)
                        .on_hover_text("Ocultar la consola · J")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
            let visible = panel.visible_indices(run);
            let mut clear = false;
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut panel.filter.query)
                        .hint_text("Buscar")
                        .desired_width(width - 150.0),
                );
                if ui_kit::small_button(ui, "Copiar", !visible.is_empty())
                    .on_hover_text("Copiar los mensajes listados")
                    .clicked()
                {
                    let listed: Vec<&LogEntry> =
                        visible.iter().map(|&index| &panel.entries[index]).collect();
                    ui.ctx().copy_text(plain_text(&listed));
                }
                if ui_kit::small_button(ui, "Limpiar", true).clicked() {
                    clear = true;
                }
            });
            let mut toggled = None;
            // Opaque, so the scene behind never shows through the text.
            ui_kit::field_frame().fill(palette::INK).show(ui, |ui| {
                if visible.is_empty() {
                    ui.set_min_height(LIST_HEIGHT);
                    ui.label(
                        egui::RichText::new(if panel.entries.is_empty() {
                            "Sin mensajes todavía."
                        } else {
                            "Ningún mensaje coincide con el filtro."
                        })
                        .size(12.0)
                        .color(palette::TEXT_FAINT),
                    );
                    return;
                }
                // Only the rows in view are laid out, however long the log.
                // Rows touch: show_rows sizes the list with this spacing.
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::ScrollArea::vertical()
                    .max_height(LIST_HEIGHT)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show_rows(ui, ROW_HEIGHT, visible.len(), |ui, rows| {
                        for &index in &visible[rows] {
                            let entry = &panel.entries[index];
                            let selected = panel.selected == Some(entry.seq);
                            if entry_row(ui, entry, selected, entry.run != run) {
                                toggled = Some(entry.seq);
                            }
                        }
                    });
            });
            if let Some(seq) = toggled {
                panel.selected = (panel.selected != Some(seq)).then_some(seq);
            }
            if let Some(entry) = panel.selected.and_then(|seq| {
                visible
                    .iter()
                    .map(|&index| &panel.entries[index])
                    .find(|entry| entry.seq == seq)
            }) {
                entry_detail(ui, entry);
            }
            if clear {
                panel.cleared_through = panel.entries.last().map_or(0, |entry| entry.seq);
                panel.selected = None;
            }
        });
    if close {
        panel.open = false;
    }
}

/// The hidden console's badge; returns whether it was clicked.
fn problems_badge(ctx: &egui::Context, counts: Counts) -> bool {
    let color = if counts.errors > 0 {
        palette::DANGER
    } else {
        palette::STOP
    };
    let text = badge_text(counts);
    let mut clicked = false;
    egui::Area::new("console_badge".into())
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 12.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let response = egui::Frame::new()
                .fill(palette::PANEL)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.5)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui_kit::paint_icon(ui.painter(), rect, Icon::Warning, color);
                        ui.label(egui::RichText::new(text).size(12.5).color(palette::TEXT));
                    });
                })
                .response
                .interact(egui::Sense::click())
                .on_hover_text("Abrir la consola · J");
            clicked = response.clicked();
        });
    clicked
}

/// `1 error · 2 avisos`.
fn badge_text(counts: Counts) -> String {
    let mut parts = Vec::new();
    match counts.errors {
        0 => {}
        1 => parts.push("1 error".to_owned()),
        errors => parts.push(format!("{errors} errores")),
    }
    match counts.warnings {
        0 => {}
        1 => parts.push("1 aviso".to_owned()),
        warnings => parts.push(format!("{warnings} avisos")),
    }
    parts.join(" · ")
}

fn level_color(level: Level) -> egui::Color32 {
    match level {
        Level::Error => palette::DANGER,
        Level::Warn => palette::STOP,
        Level::Success => palette::LOOP,
        Level::Info => palette::ACCENT,
        Level::Debug => palette::TEXT_FAINT,
    }
}

/// Height of one row of the list.
const ROW_HEIGHT: f32 = 20.0;
/// Height of the list.
const LIST_HEIGHT: f32 = 220.0;

/// One line of the list: label, first line of the message and script line.
/// Returns whether the row was clicked, which shows or hides its detail.
fn entry_row(ui: &mut egui::Ui, entry: &LogEntry, selected: bool, earlier_run: bool) -> bool {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::click());
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, palette::SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, palette::HOVER);
    }
    let dim = |color: egui::Color32| {
        if earlier_run {
            color.gamma_multiply(0.55)
        } else {
            color
        }
    };
    let mono = egui::FontId::monospace(11.5);
    let center = rect.center().y;
    painter
        .with_clip_rect(egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.min.x + LABEL_WIDTH - 6.0, rect.max.y),
        ))
        .text(
            egui::pos2(rect.min.x + 6.0, center),
            egui::Align2::LEFT_CENTER,
            &entry.label,
            mono.clone(),
            dim(level_color(entry.level)),
        );

    let mut message_end = rect.max.x - 6.0;
    let mut location_clicked = false;
    if let Some(location) = &entry.location {
        let text = format!("{}:{}", location.file_name(), location.line);
        let galley = painter.layout_no_wrap(text, mono.clone(), dim(palette::ACCENT));
        let link = egui::Rect::from_min_size(
            egui::pos2(
                rect.max.x - 6.0 - galley.size().x,
                center - galley.size().y / 2.0,
            ),
            galley.size(),
        );
        message_end = link.min.x - 10.0;
        let link_response = ui
            .interact(
                link,
                ui.id().with(("console_link", entry.seq)),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(if crate::source_link::AVAILABLE {
                format!(
                    "{location}\nClic: abrir en el editor de código\n{}",
                    crate::source_link::hint()
                )
            } else {
                format!("{location}\nClic: copiar")
            });
        painter.galley(link.min, galley, palette::ACCENT);
        if link_response.clicked() {
            location_clicked = true;
            if crate::source_link::AVAILABLE {
                crate::source_link::open_or_report(location);
            } else {
                ui.ctx().copy_text(location.to_string());
            }
        }
    }
    let first_line = entry.message.lines().next().unwrap_or("");
    let more = entry.detail.is_some() || first_line.len() < entry.message.len();
    if more {
        // What the row leaves out shows when it is clicked.
        let hint = painter.layout_no_wrap(
            "detalle".to_owned(),
            egui::FontId::proportional(11.0),
            palette::TEXT_FAINT,
        );
        let hint_x = message_end - hint.size().x;
        let hint_y = center - hint.size().y / 2.0;
        painter.galley(egui::pos2(hint_x, hint_y), hint, palette::TEXT_FAINT);
        message_end = hint_x - 10.0;
    }
    painter
        .with_clip_rect(egui::Rect::from_min_max(
            egui::pos2(rect.min.x + LABEL_WIDTH, rect.min.y),
            egui::pos2(message_end, rect.max.y),
        ))
        .text(
            egui::pos2(rect.min.x + LABEL_WIDTH, center),
            egui::Align2::LEFT_CENTER,
            display_message(first_line),
            egui::FontId::proportional(12.5),
            dim(if entry.level == Level::Debug {
                palette::TEXT_MUTED
            } else {
                palette::TEXT
            }),
        );
    let response = response.on_hover_text(if more {
        "Clic: ver el mensaje completo"
    } else {
        "Clic: ver el mensaje"
    });
    response.clicked() && !location_clicked
}

/// Width of the label column.
const LABEL_WIDTH: f32 = 104.0;

/// The whole message and detail of the selected entry, under the list.
fn entry_detail(ui: &mut egui::Ui, entry: &LogEntry) {
    ui_kit::field_frame().fill(palette::INK).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::ScrollArea::vertical()
            .id_salt("console_entry_detail")
            .max_height(160.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(display_message(&entry.message))
                            .size(12.5)
                            .color(palette::TEXT),
                    )
                    .wrap(),
                );
                if let Some(detail) = &entry.detail {
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(detail)
                                .monospace()
                                .size(11.0)
                                .color(palette::TEXT_MUTED),
                        )
                        .wrap(),
                    );
                }
            });
        ui.horizontal(|ui| {
            if ui_kit::small_button(ui, "Copiar", true)
                .on_hover_text("Copiar este mensaje")
                .clicked()
            {
                ui.ctx().copy_text(plain_text(&[entry]));
            }
            if let Some(location) = &entry.location
                && crate::source_link::AVAILABLE
                && ui_kit::small_button(ui, "Abrir", true)
                    .on_hover_text(format!("{location}\n{}", crate::source_link::hint()))
                    .clicked()
            {
                crate::source_link::open_or_report(location);
            }
        });
    });
}

/// Most characters of a message the list shows; copying keeps them all.
const MAX_SHOWN_CHARS: usize = 500;

/// `message`, cut after [`MAX_SHOWN_CHARS`] with how much was left out.
fn display_message(message: &str) -> std::borrow::Cow<'_, str> {
    match message.char_indices().nth(MAX_SHOWN_CHARS) {
        None => message.into(),
        Some((end, _)) => {
            let hidden = message[end..].chars().count();
            format!("{}… ({hidden} caracteres más)", &message[..end]).into()
        }
    }
}

/// The entries as the terminal would show them, without colour.
fn plain_text(entries: &[&LogEntry]) -> String {
    let mut text = String::new();
    for entry in entries {
        text.push_str(&console::format_line(
            entry.level,
            &entry.label,
            &entry.message,
            false,
        ));
        if let Some(location) = &entry.location {
            text.push_str(&format!(" ({location})"));
        }
        text.push('\n');
        if let Some(detail) = &entry.detail {
            for line in detail.lines() {
                text.push_str("    ");
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::console::ScriptLocation;

    fn entry(seq: u64, run: u64, level: Level, message: &str) -> LogEntry {
        LogEntry {
            seq,
            run,
            ..LogEntry::new(level, "test", message)
        }
    }

    fn panel(entries: Vec<LogEntry>) -> ConsolePanel {
        ConsolePanel {
            last_seq: entries.last().map_or(0, |entry| entry.seq),
            entries,
            ..ConsolePanel::default()
        }
    }

    #[test]
    fn only_the_current_run_counts_unless_history_is_kept() {
        let mut panel = panel(vec![
            entry(1, 1, Level::Error, "old failure"),
            entry(2, 2, Level::Warn, "still here"),
            entry(3, 2, Level::Info, "Scene ready"),
        ]);
        assert_eq!(
            panel.counts(2),
            Counts {
                errors: 0,
                warnings: 1,
                messages: 1
            }
        );
        panel.filter.history = true;
        assert_eq!(panel.counts(2).errors, 1);
        assert_eq!(panel.visible(2).len(), 3);
    }

    #[test]
    fn filters_hide_severities_and_match_text_and_files() {
        let mut panel = panel(vec![
            entry(1, 1, Level::Error, "ZeroDivisionError"),
            LogEntry {
                location: Some(ScriptLocation::new("/work/intro.py", 4)),
                ..entry(2, 1, Level::Warn, "zero-length path")
            },
            entry(3, 1, Level::Info, "Scene ready"),
        ]);
        panel.filter.messages = false;
        assert_eq!(panel.visible(1).len(), 2);
        panel.filter.query = "INTRO.py".to_owned();
        let visible = panel.visible(1);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].message, "zero-length path");
    }

    #[test]
    fn clearing_hides_what_was_logged_before() {
        let mut panel = panel(vec![
            entry(1, 1, Level::Warn, "before"),
            entry(2, 1, Level::Warn, "also before"),
        ]);
        panel.cleared_through = 2;
        assert!(panel.visible(1).is_empty());
        panel.entries.push(entry(3, 1, Level::Warn, "after"));
        assert_eq!(panel.counts(1).warnings, 1);
    }

    #[test]
    fn the_badge_names_errors_before_warnings() {
        assert_eq!(
            badge_text(Counts {
                errors: 1,
                warnings: 2,
                messages: 9
            }),
            "1 error · 2 avisos"
        );
        assert_eq!(
            badge_text(Counts {
                errors: 0,
                warnings: 1,
                messages: 0
            }),
            "1 aviso"
        );
    }

    #[test]
    fn long_messages_are_cut_in_the_list() {
        assert_eq!(display_message("corto"), "corto");
        let long = "x".repeat(MAX_SHOWN_CHARS + 25);
        let shown = display_message(&long);
        assert!(shown.ends_with("… (25 caracteres más)"));
        assert_eq!(
            shown.chars().filter(|&ch| ch == 'x').count(),
            MAX_SHOWN_CHARS
        );
    }

    #[test]
    fn copied_text_reads_like_the_terminal() {
        let warning = LogEntry {
            location: Some(ScriptLocation::new("/work/intro.py", 4)),
            detail: Some("line one\nline two".to_owned()),
            ..entry(1, 1, Level::Warn, "zero-length path")
        };
        let text = plain_text(&[&warning]);
        assert!(text.starts_with("  ! test      zero-length path ("));
        assert!(text.contains("intro.py:4)\n    line one\n    line two\n"));
    }
}
