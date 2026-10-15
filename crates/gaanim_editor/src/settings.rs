//! Settings: one window, the same on Home and in the editor (`Ctrl+,`),
//! with every preference Gaanim keeps for this user. They are saved as soon
//! as they change, each area in its section of `settings.json` (see
//! [`gaanim_project::settings`]).
//!
//! Environment variables still win over what is saved: a setting one of them
//! fixes is shown locked, naming the variable.

use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_renderer::prelude::{PREVIEW_RESOLUTION_ENV, PreviewResolution};
use serde::{Deserialize, Serialize};

use crate::export::ProjectPaths;
use crate::narration::{NarrationPanel, NarrationSettingsSection};
use crate::presenter::prompter::{
    PaceMode, PrompterPace, PrompterPreferences, PrompterSettings, SIZE_RANGE,
};
use crate::ui_kit::{
    self, Icon, card_frame, field_frame, palette, secondary_button, section_label, segmented,
    small_button,
};

// ---- Preferences ----------------------------------------------------------------------------

/// The `preview` section: the editor preview's resolution, written like
/// [`PREVIEW_RESOLUTION_ENV`] (`auto`, `full` or a fraction such as `0.5`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PreviewPreferences {
    #[serde(default = "default_resolution")]
    pub(crate) resolution: String,
}

fn default_resolution() -> String {
    "auto".to_owned()
}

impl Default for PreviewPreferences {
    fn default() -> Self {
        Self {
            resolution: default_resolution(),
        }
    }
}

impl PreviewPreferences {
    const SECTION: &str = "preview";

    pub(crate) fn load() -> Self {
        gaanim_project::settings::load(Self::SECTION)
    }

    fn save(&self) -> Result<(), String> {
        gaanim_project::settings::save(Self::SECTION, self)
    }
}

/// Which problems the corner badge counts while the console is hidden.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProblemsBadge {
    /// Warnings and errors.
    #[default]
    All,
    Errors,
    /// None: the console (`J`) still lists them.
    Hidden,
}

/// The `notices` section: what the editor shows on its own after a reload.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct NoticePreferences {
    #[serde(default)]
    pub(crate) problems: ProblemsBadge,
    /// The "Scene ready" notice after each reload.
    #[serde(default = "enabled")]
    pub(crate) reload: bool,
}

impl Default for NoticePreferences {
    fn default() -> Self {
        Self {
            problems: ProblemsBadge::All,
            reload: true,
        }
    }
}

impl NoticePreferences {
    const SECTION: &str = "notices";

    fn load() -> Self {
        gaanim_project::settings::load(Self::SECTION)
    }

    fn save(&self) -> Result<(), String> {
        gaanim_project::settings::save(Self::SECTION, self)
    }
}

/// The notice preferences in force, read every frame without the file.
static PROBLEMS_BADGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static RELOAD_NOTICE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn apply_notices(preferences: NoticePreferences) {
    use std::sync::atomic::Ordering;
    PROBLEMS_BADGE.store(preferences.problems as u8, Ordering::Relaxed);
    RELOAD_NOTICE.store(preferences.reload, Ordering::Relaxed);
}

/// Which problems the console's corner badge counts.
pub(crate) fn problems_badge() -> ProblemsBadge {
    match PROBLEMS_BADGE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => ProblemsBadge::Errors,
        2 => ProblemsBadge::Hidden,
        _ => ProblemsBadge::All,
    }
}

/// Hide the console's corner badge from now on, as its close button asks.
pub(crate) fn hide_problems_badge() -> Result<(), String> {
    let mut preferences = NoticePreferences::load();
    preferences.problems = ProblemsBadge::Hidden;
    apply_notices(preferences);
    preferences.save()
}

/// Whether the "Scene ready" notice shows after a reload.
pub fn reload_notice_shown() -> bool {
    RELOAD_NOTICE.load(std::sync::atomic::Ordering::Relaxed)
}

/// The preview's resolution: [`PREVIEW_RESOLUTION_ENV`], else Settings.
pub(crate) fn preview_resolution() -> PreviewResolution {
    let variable = std::env::var(PREVIEW_RESOLUTION_ENV).ok();
    let saved = PreviewPreferences::load().resolution;
    PreviewResolution::from_setting(Some(variable.as_deref().unwrap_or(&saved)))
}

/// The `integrations` section: programs Gaanim hands work to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct IntegrationPreferences {
    /// The command that opens a line of the script, with `{file}` and
    /// `{line}`; empty looks for a known editor (see `source_link`).
    #[serde(default)]
    pub(crate) code_editor: String,
}

impl IntegrationPreferences {
    const SECTION: &str = "integrations";

    pub(crate) fn load() -> Self {
        gaanim_project::settings::load(Self::SECTION)
    }

    fn save(&self) -> Result<(), String> {
        gaanim_project::settings::save(Self::SECTION, self)
    }
}

/// The `updates` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct UpdatePreferences {
    /// Home asks GitHub once a day whether a newer version exists.
    #[serde(default = "enabled")]
    pub(crate) check: bool,
}

fn enabled() -> bool {
    true
}

impl Default for UpdatePreferences {
    fn default() -> Self {
        Self { check: true }
    }
}

impl UpdatePreferences {
    const SECTION: &str = "updates";

    pub(crate) fn load() -> Self {
        gaanim_project::settings::load(Self::SECTION)
    }

    // Settings, the only caller, never opens on the web.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn save(&self) -> Result<(), String> {
        gaanim_project::settings::save(Self::SECTION, self)
    }
}

// ---- Window ---------------------------------------------------------------------------------

/// The areas of Settings, in the order the window lists them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Section {
    #[default]
    Recording,
    Transcription,
    Dictation,
    Prompter,
    Preview,
    Integrations,
    Updates,
}

impl Section {
    pub(crate) const ALL: [Self; 7] = [
        Self::Recording,
        Self::Transcription,
        Self::Dictation,
        Self::Prompter,
        Self::Preview,
        Self::Integrations,
        Self::Updates,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Recording => "Recording",
            Self::Transcription => "Transcription",
            Self::Dictation => "Dictation",
            Self::Prompter => "Prompter",
            Self::Preview => "Preview",
            Self::Integrations => "Integrations",
            Self::Updates => "Updates",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Self::Recording => "How voice takes and recordings of the talk are made and kept.",
            Self::Transcription => "whisper.cpp, which finds markers by the words you say.",
            Self::Dictation => "Zoom Scribe, which writes the script while you present.",
            Self::Prompter => "The pace and size of the script in the prompter.",
            Self::Preview => "How the editor draws the preview while it plays.",
            Self::Integrations => "Your code editor and the relay of audience polls.",
            Self::Updates => "How Gaanim learns about new versions.",
        }
    }
}

/// Whether Settings is open, on which section, and what it is editing.
#[derive(Resource, Default)]
pub(crate) struct SettingsWindow {
    open: bool,
    section: Section,
    /// Read when the window opens.
    drafts: Option<Drafts>,
}

impl SettingsWindow {
    /// Open Settings on `section`.
    pub(crate) fn show(&mut self, section: Section) {
        self.open = true;
        self.section = section;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Open Settings on the section it showed last.
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    /// Open Settings, or close it when it is open.
    pub(crate) fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// What Settings edits that is not kept in another resource.
struct Drafts {
    prompter: PrompterPreferences,
    preview: PreviewPreferences,
    notices: NoticePreferences,
    integrations: IntegrationPreferences,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    updates: UpdatePreferences,
    /// The relay address as typed, saved with its button.
    relay: String,
    /// The last thing saving said, and whether it failed.
    notice: Option<(String, bool)>,
}

impl Drafts {
    fn load() -> Self {
        Self {
            prompter: PrompterPreferences::load(),
            preview: PreviewPreferences::load(),
            notices: NoticePreferences::load(),
            integrations: IntegrationPreferences::load(),
            updates: UpdatePreferences::load(),
            relay: gaanim_project::relay::saved().unwrap_or_default(),
            notice: None,
        }
    }

    fn saved(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            self.notice = Some((format!("Not saved: {error}"), true));
        }
    }
}

pub(crate) struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        if !crate::WEB {
            apply_notices(NoticePreferences::load());
        }
        app.init_resource::<SettingsWindow>()
            .add_systems(
                Update,
                crate::presenter::prompter::persist_prompter_size_system,
            )
            .add_systems(
                bevy_egui::EguiPrimaryContextPass,
                settings_window_system.after(crate::editor_ui_system),
            );
    }
}

const WINDOW_WIDTH: f32 = 760.0;
const NAV_WIDTH: f32 = 176.0;

#[allow(clippy::too_many_arguments)]
fn settings_window_system(
    mut contexts: bevy_egui::EguiContexts,
    mut window: ResMut<SettingsWindow>,
    mut narration: ResMut<NarrationPanel>,
    mut prompter: ResMut<PrompterSettings>,
    mut pace: ResMut<PrompterPace>,
    preview: Option<ResMut<PreviewResolution>>,
    mut hub: Option<ResMut<crate::project_hub::ProjectHubState>>,
    paths: Option<Res<ProjectPaths>>,
) {
    if crate::WEB {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let ctx = ctx.clone();
    let shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Comma);
    if ctx.input_mut(|input| input.consume_shortcut(&shortcut)) {
        window.toggle();
    }
    if !window.open {
        window.drafts = None;
        return;
    }
    let window = &mut *window;
    let drafts = window.drafts.get_or_insert_with(|| {
        narration.forget_settings_project();
        Drafts::load()
    });
    // On Home no project is open, whatever the editor last showed.
    let paths = paths
        .as_deref()
        .filter(|_| !hub.as_ref().is_some_and(|hub| hub.active));

    let mut close = false;
    let screen = ctx.viewport_rect();
    let body_height = (screen.height() - 200.0).clamp(240.0, 520.0);
    let modal = egui::Modal::new(egui::Id::new("gaanim-settings"))
        .frame(card_frame().inner_margin(egui::Margin::same(0)))
        .backdrop_color(egui::Color32::from_black_alpha(150))
        .show(&ctx, |ui| {
            let width = WINDOW_WIDTH.min(screen.width() - 32.0);
            // Fixed widths: fields that fill the room they get would
            // otherwise widen the window a little more every frame.
            let body_width = (width - NAV_WIDTH - 1.0).max(240.0);
            ui.set_width(width);
            header(ui, &mut close);
            rule(ui);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                nav(ui, &mut window.section, body_height);
                ui.add_space(1.0);
                ui.vertical(|ui| {
                    ui.set_width(body_width);
                    egui::ScrollArea::vertical()
                        .id_salt(("settings-body", window.section as u8))
                        .max_height(body_height)
                        .min_scrolled_height(body_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin {
                                    left: 24,
                                    right: 24,
                                    top: 18,
                                    bottom: 22,
                                })
                                .show(ui, |ui| {
                                    ui.set_width(body_width - 48.0);
                                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                                    section_heading(ui, window.section);
                                    match window.section {
                                        Section::Recording => narration.settings_ui(
                                            ui,
                                            NarrationSettingsSection::Recording,
                                            paths,
                                        ),
                                        Section::Transcription => narration.settings_ui(
                                            ui,
                                            NarrationSettingsSection::Transcription,
                                            paths,
                                        ),
                                        Section::Dictation => narration.settings_ui(
                                            ui,
                                            NarrationSettingsSection::Dictation,
                                            paths,
                                        ),
                                        Section::Prompter => prompter_section(
                                            ui,
                                            drafts,
                                            &mut prompter,
                                            &mut pace,
                                            &mut narration,
                                        ),
                                        Section::Preview => preview_section(ui, drafts, preview),
                                        Section::Integrations => integrations_section(ui, drafts),
                                        Section::Updates => {
                                            updates_section(ui, drafts, hub.as_deref_mut())
                                        }
                                    }
                                    if let Some((message, failed)) = &drafts.notice {
                                        ui.add_space(4.0);
                                        ui.add(
                                            egui::Label::new(egui::RichText::new(message).color(
                                                if *failed {
                                                    palette::DANGER
                                                } else {
                                                    palette::ACCENT
                                                },
                                            ))
                                            .wrap(),
                                        );
                                    }
                                });
                        });
                });
            });
            rule(ui);
            footer(ui);
        });
    if close || modal.should_close() {
        window.open = false;
    }
}

fn header(ui: &mut egui::Ui, close: &mut bool) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(20, 14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::hover());
                ui_kit::paint_icon(ui.painter(), rect, Icon::Settings, palette::ACCENT);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Settings")
                        .size(18.0)
                        .strong()
                        .color(palette::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui_kit::icon_button(ui, Icon::Close, ui_kit::ButtonTone::Ghost, true)
                        .on_hover_text("Close · Esc")
                        .clicked()
                    {
                        *close = true;
                    }
                    ui.label(
                        egui::RichText::new("Ctrl+,")
                            .monospace()
                            .size(11.5)
                            .color(palette::TEXT_FAINT),
                    );
                });
            });
        });
}

/// A full-width line between the window's header, body and footer.
fn rule(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette::DIVIDER);
}

fn nav(ui: &mut egui::Ui, section: &mut Section, height: f32) {
    let (area, _) = ui.allocate_exact_size(egui::vec2(NAV_WIDTH, height), egui::Sense::hover());
    ui.painter().rect_filled(area, 0.0, palette::FIELD);
    ui.painter().line_segment(
        [area.right_top(), area.right_bottom()],
        egui::Stroke::new(1.0, palette::DIVIDER),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(area.shrink2(egui::vec2(8.0, 12.0)))
            .layout(egui::Layout::top_down_justified(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
    for candidate in Section::ALL {
        if nav_row(&mut child, candidate.title(), *section == candidate).clicked() {
            *section = candidate;
        }
    }
}

fn nav_row(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 0.0, palette::SELECTED);
        // The current frame: gold, like the playhead.
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.min.x, rect.center().y - 7.0),
                egui::vec2(3.0, 14.0),
            ),
            0.0,
            palette::STOP,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, palette::HOVER);
    }
    painter.text(
        egui::pos2(rect.min.x + 14.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        if selected || response.hovered() {
            palette::TEXT
        } else {
            palette::TEXT_MUTED
        },
    );
    response
}

fn section_heading(ui: &mut egui::Ui, section: Section) {
    ui.label(
        egui::RichText::new(section.title())
            .size(17.0)
            .strong()
            .color(palette::TEXT),
    );
    ui.label(
        egui::RichText::new(section.summary())
            .size(13.5)
            .color(palette::TEXT_MUTED),
    );
    ui.add_space(6.0);
}

fn footer(ui: &mut egui::Ui) {
    let Some(path) = gaanim_project::settings::path() else {
        return;
    };
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(20, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Saved in {}", path.display()))
                        .monospace()
                        .size(11.0)
                        .color(palette::TEXT_FAINT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if small_button(ui, "Show folder", true).clicked()
                        && let Some(folder) = path.parent()
                    {
                        let _ = crate::platform::open_detached(folder);
                    }
                });
            });
        });
}

/// A faint explanation under a control.
fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text.into())
                .size(13.0)
                .color(palette::TEXT_FAINT),
        )
        .wrap(),
    );
}

/// Says that `variable` fixes a setting, and returns whether it does.
fn locked_by(ui: &mut egui::Ui, variable: &str, value: Option<String>) -> bool {
    let Some(value) = value else {
        return false;
    };
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!(
                "Set by {variable} = \"{value}\". Remove the variable to choose here."
            ))
            .size(13.0)
            .color(palette::STOP),
        )
        .wrap(),
    );
    true
}

fn prompter_section(
    ui: &mut egui::Ui,
    drafts: &mut Drafts,
    prompter: &mut PrompterSettings,
    pace: &mut PrompterPace,
    narration: &mut NarrationPanel,
) {
    section_label(ui, "READING PACE");
    let before = drafts.prompter;
    segmented(
        ui,
        "settings-pace",
        &mut drafts.prompter.pace,
        &[
            (
                PaceMode::Auto,
                "Automatic",
                "From your recordings and dictations",
            ),
            (PaceMode::Fixed, "Fixed", "The same for every part"),
        ],
    );
    match drafts.prompter.pace {
        PaceMode::Fixed => {
            ui.add(
                egui::Slider::new(&mut drafts.prompter.wpm, 80.0..=240.0)
                    .step_by(5.0)
                    .suffix(" wpm"),
            );
            note(
                ui,
                "Every part lights at this pace, even the ones you recorded. Reading aloud \
                 usually goes at 130 to 160 words per minute.",
            );
        }
        PaceMode::Auto => {
            let measured = narration.measured_pace();
            note(
                ui,
                format!(
                    "Each part lights at the pace you took when you recorded it; if you did \
                     not, at your pace measured while dictating ({}); and if you never \
                     dictated, at 150 words per minute.",
                    measured.map_or("not measured yet".to_owned(), |wpm| format!("{wpm:.0} wpm"))
                ),
            );
            if measured.is_some()
                && small_button(ui, "Forget the measured pace", true)
                    .on_hover_text("The next dictation measures it again")
                    .clicked()
            {
                let result = narration.forget_measured_pace();
                if result.is_ok() {
                    pace.measured = None;
                }
                drafts.saved(result);
            }
        }
    }
    if drafts.prompter != before {
        pace.fixed = drafts.prompter.fixed();
        let result = drafts.prompter.save();
        drafts.saved(result);
    }

    ui.add_space(10.0);
    section_label(ui, "TEXT SIZE");
    let mut size = prompter.size;
    ui.add(
        egui::Slider::new(&mut size, SIZE_RANGE.0..=SIZE_RANGE.1)
            .step_by(2.0)
            .fixed_decimals(0)
            .suffix(" pt"),
    );
    if size != prompter.size {
        prompter.size = size;
    }
    note(
        ui,
        "The same in Presenter View and in the editor's caption, where A+ and A− change it too.",
    );
}

/// How the preview's resolution is chosen, in Settings.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ResolutionMode {
    Auto,
    Full,
    Fixed,
}

fn preview_section(
    ui: &mut egui::Ui,
    drafts: &mut Drafts,
    preview: Option<ResMut<PreviewResolution>>,
) {
    let variable = std::env::var(PREVIEW_RESOLUTION_ENV).ok();
    let locked = locked_by(ui, PREVIEW_RESOLUTION_ENV, variable);
    let current = PreviewResolution::from_setting(Some(&drafts.preview.resolution));
    let mut mode = match (current.adaptive, current.scale) {
        (true, _) => ResolutionMode::Auto,
        (false, scale) if scale >= 1.0 => ResolutionMode::Full,
        _ => ResolutionMode::Fixed,
    };
    let mut scale = if mode == ResolutionMode::Fixed {
        current.scale
    } else {
        0.5
    };
    let before = drafts.preview.clone();
    ui.add_enabled_ui(!locked, |ui| {
        section_label(ui, "RESOLUTION");
        segmented(
            ui,
            "settings-resolution",
            &mut mode,
            &[
                (
                    ResolutionMode::Auto,
                    "Automatic",
                    "Lowers while playback runs slow",
                ),
                (ResolutionMode::Full, "Full", "Every pixel, always"),
                (ResolutionMode::Fixed, "Fixed", "A share of the pixels"),
            ],
        );
        if mode == ResolutionMode::Fixed {
            let mut percent = (scale * 100.0).round();
            ui.add(
                egui::Slider::new(
                    &mut percent,
                    (gaanim_renderer::canvas::MIN_PREVIEW_SCALE * 100.0)..=100.0,
                )
                .step_by(5.0)
                .suffix(" %"),
            );
            scale = percent / 100.0;
        }
    });
    drafts.preview.resolution = match mode {
        ResolutionMode::Auto => "auto".to_owned(),
        ResolutionMode::Full => "full".to_owned(),
        ResolutionMode::Fixed => format!("{scale:.2}"),
    };
    note(
        ui,
        "Automatic draws 75 % and then 50 % of the pixels when playback misses 60 fps because \
         drawing costs too much, and goes back to full resolution when it pauses. It never \
         changes exports.",
    );
    if drafts.preview != before && !locked {
        if let Some(mut preview) = preview {
            *preview = PreviewResolution::from_setting(Some(&drafts.preview.resolution));
        }
        let result = drafts.preview.save();
        drafts.saved(result);
    }

    ui.add_space(12.0);
    section_label(ui, "PROBLEMS BADGE");
    let before = drafts.notices;
    segmented(
        ui,
        "settings-problems",
        &mut drafts.notices.problems,
        &[
            (ProblemsBadge::All, "Warnings and errors", ""),
            (ProblemsBadge::Errors, "Errors only", ""),
            (ProblemsBadge::Hidden, "Hidden", ""),
        ],
    );
    note(
        ui,
        "The corner badge that counts the script's problems after each reload while the \
         console is closed. Hidden or not, the console (J) lists them, and an error that \
         stops the scene still opens its panel.",
    );
    crate::ui_kit::checkbox(
        ui,
        &mut drafts.notices.reload,
        "Show \"Scene ready\" after each reload",
    );
    if drafts.notices != before {
        apply_notices(drafts.notices);
        let result = drafts.notices.save();
        drafts.saved(result);
    }
}

fn integrations_section(ui: &mut egui::Ui, drafts: &mut Drafts) {
    section_label(ui, "CODE EDITOR");
    let variable = std::env::var(crate::source_link::EDITOR_VARIABLE)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let locked = locked_by(ui, crate::source_link::EDITOR_VARIABLE, variable);
    let before = drafts.integrations.clone();
    ui.add_enabled_ui(!locked, |ui| {
        field_frame().show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut drafts.integrations.code_editor)
                    .hint_text("code --goto {file}:{line}")
                    .desired_width(f32::INFINITY)
                    .frame(egui::Frame::NONE)
                    .font(egui::TextStyle::Monospace),
            );
        });
    });
    note(
        ui,
        "The inspector, the console, the animations and the error panel open the script at a \
         line with this command: {file} and {line} are the path and the line number, quotes \
         group words with spaces, and without {file} the path goes at the end. Empty, Gaanim \
         tries code, cursor, codium, zed and subl on the PATH, and else the default app.",
    );
    if drafts.integrations != before && !locked {
        let result = drafts.integrations.save();
        drafts.saved(result);
    }

    ui.add_space(12.0);
    section_label(ui, "AUDIENCE POLLS RELAY");
    let variable = std::env::var(gaanim_project::relay::RELAY_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let locked = locked_by(ui, gaanim_project::relay::RELAY_ENV, variable);
    let saved = gaanim_project::relay::saved().unwrap_or_default();
    ui.add_enabled_ui(!locked, |ui| {
        ui.horizontal(|ui| {
            field_frame().show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut drafts.relay)
                        .hint_text("https://gaanim-relay.example.workers.dev")
                        .desired_width((ui.available_width() - 150.0).max(160.0))
                        .frame(egui::Frame::NONE)
                        .font(egui::TextStyle::Monospace),
                );
            });
            let typed = drafts.relay.trim().trim_end_matches('/');
            if secondary_button(ui, "Save", typed != saved && !typed.is_empty()).clicked() {
                drafts.notice = Some(match gaanim_project::relay::save(Some(typed)) {
                    Ok(Some(url)) => {
                        drafts.relay = url.clone();
                        (format!("Audience polls use {url}"), false)
                    }
                    Ok(None) => ("Forgot the saved relay".to_owned(), false),
                    Err(error) => (error, true),
                });
            }
            if secondary_button(ui, "Forget", !saved.is_empty()).clicked() {
                drafts.notice = Some(match gaanim_project::relay::save(None) {
                    Ok(_) => {
                        drafts.relay.clear();
                        ("Forgot the saved relay".to_owned(), false)
                    }
                    Err(error) => (error, true),
                });
            }
        });
    });
    note(
        ui,
        "The relay phones vote through, deployed with gaanim relay init. A project's \
         [polls] relay in gaanim.toml wins over this one; gaanim relay use saves it here too.",
    );
}

/// The web player has no updates to check; Settings never opens there.
#[cfg(target_arch = "wasm32")]
fn updates_section(
    _ui: &mut egui::Ui,
    _drafts: &mut Drafts,
    _hub: Option<&mut crate::project_hub::ProjectHubState>,
) {
}

#[cfg(not(target_arch = "wasm32"))]
fn updates_section(
    ui: &mut egui::Ui,
    drafts: &mut Drafts,
    hub: Option<&mut crate::project_hub::ProjectHubState>,
) {
    if let Some(hub) = hub {
        section_label(ui, "NOW");
        crate::project_hub::update_settings_ui(ui, hub);
        note(
            ui,
            "Asks GitHub right away, even with the daily check off, and installs the new \
             version over this one as gaanim update does.",
        );
        ui.add_space(12.0);
    }
    section_label(ui, "DAILY CHECK");
    let variable = crate::update::disabled_by_variable()
        .then(|| std::env::var(crate::update::DISABLE_VARIABLE).unwrap_or_default());
    let locked = locked_by(ui, crate::update::DISABLE_VARIABLE, variable);
    let before = drafts.updates.clone();
    ui.add_enabled_ui(!locked, |ui| {
        crate::ui_kit::checkbox(
            ui,
            &mut drafts.updates.check,
            "Check for new versions once a day on Home",
        );
    });
    note(
        ui,
        "Home asks GitHub for the latest release and offers to install it; nothing else is \
         sent. gaanim update works either way.",
    );
    if drafts.updates != before && !locked {
        let result = drafts.updates.save();
        drafts.saved(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_sections_keep_the_defaults() {
        let preview: PreviewPreferences = serde_json::from_str("{}").unwrap();
        assert_eq!(preview.resolution, "auto");
        let updates: UpdatePreferences = serde_json::from_str("{}").unwrap();
        assert!(updates.check);
        let integrations: IntegrationPreferences = serde_json::from_str("{}").unwrap();
        assert!(integrations.code_editor.is_empty());
    }

    #[test]
    fn a_saved_resolution_reads_like_the_variable() {
        for (saved, adaptive, scale) in [
            ("auto", true, 1.0),
            ("full", false, 1.0),
            ("0.50", false, 0.5),
        ] {
            let resolution = PreviewResolution::from_setting(Some(saved));
            assert_eq!((resolution.adaptive, resolution.scale), (adaptive, scale));
        }
    }
}
