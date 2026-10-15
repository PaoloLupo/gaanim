//! Script & narration, docked on the right of the editor: the talk's plan
//! and how the recorded takes and the practice runs fit it, a map of the
//! talk, the slides to record or practise again, the script of the slide
//! on screen, and the controls that record or practise.
//!
//! The plan works both ways: the target typed here is the one Presenter
//! View plans with (and the reverse), every practice run here or in
//! Presenter View lands in the same rehearsal log, and the slides' share
//! of the target ([`plan::budgets`]) is what the prompter times a practice
//! run against.

use std::path::Path;
use std::time::Duration;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_timeline::notes::split_notes;
use gaanim_timeline::timeline::{SegmentMetadata, Timeline};

use super::plan::{self, TalkPlan};
use super::{
    Input, InputLevel, NarrationPanel, NarrationSession, NoticeKind, ProjectSpeech, PrompterPace,
    Rehearsals, Request, TalkAudio, empty_state, level_meter, live_card, manifest, notice_color,
    part_label, script_path, script_slides, section_heading, slide_parts, slides_hint,
    timing_color, voiceover_card,
};
use crate::export::{ProjectPaths, StashedReplay};
use crate::presenter::pace::{self, Rehearsal, TalkDuration};
use crate::presenter::prompter::PrompterSettings;
use crate::ui_kit::{
    ButtonSize, ButtonTone, Icon, icon_button, paint_icon, palette, primary_button_sized,
    secondary_button_sized, small_button_sized,
};
use crate::{PresentationMode, ViewportInset};

/// Width of the docked panel on a wide window.
const DOCK_WIDTH: f32 = 420.0;
/// Narrower windows keep at least this much of their width for the scene.
const SCENE_SHARE: f32 = 0.55;
/// Height of a slide's row in the list.
const ROW_HEIGHT: f32 = 26.0;
/// The rehearsal log is read again this often while the panel shows.
const REHEARSALS_REFRESH: Duration = Duration::from_secs(2);
/// Practice runs the history shows.
const HISTORY_RUNS: usize = 12;
/// Width of a row's time bar, its time, and its two buttons.
const BAR_WIDTH: f32 = 64.0;
const TIME_WIDTH: f32 = 40.0;
const ACTIONS_WIDTH: f32 = 2.0 * crate::ui_kit::BUTTON_SIZE + 6.0;
/// What a row's fixed parts take: the fold arrow, the number, the bar, the
/// time, the buttons, and the gaps between them.
const ROW_FIXED: f32 = 16.0 + 20.0 + BAR_WIDTH + TIME_WIDTH + ACTIONS_WIDTH + 5.0 * 6.0 + 6.0;
/// How much of the slide's script the panel shows.
const SCRIPT_PREVIEW_CHARS: usize = 320;

/// Which times the slide list and the map show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ListView {
    /// The time each slide took when it was last recorded.
    #[default]
    Takes,
    /// The time each slide took in the last practice run.
    Practice,
}

/// Which slides the list shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SlideFilter {
    #[default]
    All,
    /// Without a time yet.
    Missing,
    /// Over their share of the plan.
    Over,
}

/// What the controls at the bottom start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum DockMode {
    #[default]
    Record,
    Practice,
}

/// What the panel shows, kept while the editor runs.
#[derive(Debug, Default)]
pub(super) struct PanelView {
    list: ListView,
    filter: SlideFilter,
    query: String,
    /// The target as it is being typed.
    target_text: Option<String>,
    dock: DockMode,
}

/// The panel's width on a window `screen_width` wide.
fn dock_width(screen_width: f32) -> f32 {
    DOCK_WIDTH
        .min(screen_width * (1.0 - SCENE_SHARE))
        .max(280.0)
}

/// One slide, with what the panel tells about it.
struct SlideRow<'a> {
    index: usize,
    segment: &'a SegmentMetadata,
    budget: Option<f64>,
    take: Option<f64>,
    /// The recorded time of each part, when the slide has one.
    parts: Vec<f64>,
    practice: Option<f64>,
}

impl SlideRow<'_> {
    fn shown(&self, view: ListView) -> Option<f64> {
        match view {
            ListView::Takes => self.take,
            ListView::Practice => self.practice,
        }
    }

    fn over(&self, view: ListView) -> bool {
        match (self.shown(view), self.budget) {
            (Some(seconds), Some(budget)) => seconds > budget + 0.5,
            _ => false,
        }
    }
}

/// What the panel asks for besides a recording request.
#[derive(Default)]
struct Changes {
    close: bool,
    open_settings: bool,
    /// A new target for the talk; `Some(None)` clears the one set here.
    target: Option<Option<Duration>>,
    target_error: Option<String>,
    toggle_prompter: bool,
    mic_test: Option<bool>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn narration_panel_system(
    mut contexts: bevy_egui::EguiContexts,
    mut panel: ResMut<NarrationPanel>,
    session: Res<NarrationSession>,
    (stash, paths, timeline): (Res<StashedReplay>, Option<Res<ProjectPaths>>, Res<Timeline>),
    presentation: Res<PresentationMode>,
    hub: Option<Res<crate::project_hub::ProjectHubState>>,
    pace: Res<PrompterPace>,
    mut talk: Option<ResMut<TalkDuration>>,
    mut clip_playback: Option<ResMut<gaanim_media::ClipPlayback>>,
    mut settings_window: ResMut<crate::settings::SettingsWindow>,
    mut prompter: ResMut<PrompterSettings>,
    mut inset: ResMut<ViewportInset>,
) {
    let hidden = !panel.open
        || session.capturing()
        || presentation.active
        || hub.is_some_and(|hub| hub.active);
    if hidden {
        inset.right = 0.0;
        // The recorder opens the microphone itself.
        panel.mic_test = None;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let screen = ctx.viewport_rect();
    let width = dock_width(screen.width());
    // The scene's fit is in window pixels; egui's points are zoomed.
    inset.right = width * ctx.zoom_factor();

    let manifest = manifest(&stash);
    let script = script_path(&stash, &manifest);
    let script_slides = script_slides(&stash);
    if panel
        .speech
        .as_ref()
        .is_none_or(|(revision, _)| *revision != stash.revision)
    {
        panel.speech = Some((stash.revision, ProjectSpeech::read(paths.as_deref())));
    }
    let source = paths.as_ref().map(|paths| paths.script_path.clone());
    let runs = panel.practice_runs(source.as_deref(), talk.as_deref_mut());

    // The plan and each slide's share of it.
    let plan = plan::talk_plan(&timeline, talk.as_deref());
    let slides = plan::slides(&timeline);
    let budgets = plan::budgets(&slides, plan.map(|plan| plan.seconds));
    let last_run = runs.last();
    let rows: Vec<SlideRow> = slides
        .iter()
        .enumerate()
        .map(|(index, segment)| SlideRow {
            index,
            segment,
            budget: budgets[index],
            take: pace.timing.slide(&segment.name).map(|slide| slide.seconds),
            parts: pace
                .timing
                .slide(&segment.name)
                .map(|slide| slide.parts.clone())
                .unwrap_or_default(),
            practice: last_run.and_then(|run| run.spent_on(&segment.name)),
        })
        .collect();
    let current = timeline
        .segment_position_at(timeline.current_time)
        .and_then(|position| {
            slides
                .iter()
                .position(|slide| slide.id == position.segment_id)
        });
    let wpm = pace
        .fixed
        .or(pace.measured)
        .unwrap_or(crate::presenter::prompter::DEFAULT_WORDS_PER_MINUTE);

    let mut changes = Changes::default();
    let mut request = None;
    let can_dictate = panel.token_available();
    let dictating = panel.dictating() && can_dictate;
    let mut dictate = dictating;
    let keeping = panel.settings().keep_talk_audio;
    let mut keep_audio = keeping;
    let folded_before = panel.settings().folded_sections.clone();
    let mut folded = folded_before.clone();
    let mut expanded = std::mem::take(&mut panel.expanded);
    let mut view = std::mem::take(&mut panel.view);
    let clips_dir = source.as_deref().and_then(Rehearsals::clips_dir);
    let clip_files = panel.clip_files(clips_dir.as_deref());
    let now = Instant::now();
    if let Some(input) = &mut panel.mic_test {
        input.update(now);
        // The test only shows the level: nothing it hears is kept.
        input.recorder.restart();
        ctx.request_repaint();
    }
    let mic = panel.mic_test.as_ref().map(|input| {
        (
            input.recorder.device().to_owned(),
            input.level,
            input.warning(now),
        )
    });
    let live = manifest.live.is_some();
    let notice = panel.notice.clone();
    let busy_live = manifest
        .live
        .as_ref()
        .is_some_and(|live| panel.busy(&live.key));
    let voice_jobs: Vec<(bool, bool)> = manifest
        .voiceovers
        .iter()
        .map(|voice| (panel.transcribing(&voice.key), panel.leveling(&voice.key)))
        .collect();

    let rect = egui::Rect::from_min_max(egui::pos2(screen.max.x - width, screen.min.y), screen.max);
    egui::Area::new(egui::Id::new("narration_panel"))
        .fixed_pos(rect.min)
        .constrain(false)
        .order(egui::Order::Middle)
        .show(ctx, |area| {
            // The whole strip is the panel's: the scene under it takes no
            // clicks or wheel.
            area.allocate_rect(rect, egui::Sense::hover());
            area.painter().rect_filled(rect, 0.0, palette::GRAPHITE);
            area.painter().line_segment(
                [rect.left_top(), rect.left_bottom()],
                egui::Stroke::new(1.0, palette::DIVIDER),
            );
            // The controls stay at the bottom, as tall as they were last
            // frame; the rest scrolls above them.
            let dock_id = egui::Id::new("narration-dock-height");
            let dock_height = area
                .ctx()
                .data(|data| data.get_temp::<f32>(dock_id))
                .unwrap_or(200.0)
                .min(rect.height() * 0.6);
            let split = rect.max.y - dock_height;
            let region = |area: &mut egui::Ui, region: egui::Rect| {
                let mut child = area.new_child(
                    egui::UiBuilder::new()
                        .max_rect(region)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                child.set_clip_rect(region);
                child.spacing_mut().item_spacing.y = 8.0;
                child
            };
            let mut bottom = region(
                area,
                egui::Rect::from_min_max(egui::pos2(rect.min.x, split), rect.max),
            );
            let mut top = region(
                area,
                egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, split)),
            );
            {
                dock(
                    &mut bottom,
                    &mut view,
                    rows.get(current.unwrap_or(0)).map(|row| row.segment),
                    plan,
                    mic.as_ref(),
                    live,
                    (&mut dictate, can_dictate, &mut keep_audio),
                    notice.as_ref(),
                    &mut request,
                    &mut changes,
                );
                let used = bottom.min_rect().height();
                if (used - dock_height).abs() > 0.5 {
                    area.ctx().data_mut(|data| data.insert_temp(dock_id, used));
                    area.ctx().request_repaint();
                }
                {
                    let ui = &mut top;
                    header(
                        ui,
                        &manifest,
                        &script_slides,
                        script.as_deref(),
                        &mut request,
                        &mut changes,
                    );
                    egui::ScrollArea::vertical()
                        .max_height(ui.available_height())
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            padded(ui, |ui| {
                                if slides.is_empty() {
                                    if script_slides.is_empty() && manifest.is_empty() {
                                        empty_state(ui);
                                    } else {
                                        slides_hint(ui);
                                    }
                                } else {
                                    plan_section(ui, &mut view, plan, &rows, &runs, &mut changes);
                                    talk_map(
                                        ui,
                                        view.list,
                                        &rows,
                                        current,
                                        &timeline,
                                        &mut request,
                                    );
                                    if let Some(row) = current.and_then(|index| rows.get(index)) {
                                        script_card(
                                            ui,
                                            row,
                                            wpm,
                                            prompter.editor_open,
                                            &mut request,
                                            &mut changes,
                                        );
                                    }
                                    list_toolbar(ui, &mut view, &rows);
                                    let mut audio = TalkAudio {
                                        dir: clips_dir.as_deref(),
                                        files: &clip_files,
                                        playback: clip_playback.as_deref_mut(),
                                    };
                                    slide_list(
                                        ui,
                                        &view,
                                        &rows,
                                        current,
                                        live,
                                        &timeline,
                                        &mut expanded,
                                        &mut audio,
                                        dictate && can_dictate,
                                        &mut request,
                                    );
                                }
                                if let Some(live) = &manifest.live
                                    && section_heading(ui, "LIVE TAKE", &mut folded)
                                {
                                    live_card(ui, live, busy_live, &mut dictate, &mut request);
                                }
                                if !manifest.voiceovers.is_empty()
                                    && section_heading(ui, "VOICEOVER", &mut folded)
                                {
                                    for (voice, (transcribing, leveling)) in
                                        manifest.voiceovers.iter().zip(&voice_jobs)
                                    {
                                        voiceover_card(
                                            ui,
                                            voice,
                                            *transcribing,
                                            *leveling,
                                            &mut request,
                                        );
                                    }
                                }
                            });
                        });
                }
            }
        });

    // What the panel asked for.
    panel.expanded = expanded;
    panel.view = view;
    if dictate != dictating {
        panel.dictate = Some(dictate);
    }
    let mut settings_changed = false;
    if keep_audio != keeping {
        panel.settings().keep_talk_audio = keep_audio;
        settings_changed = true;
    }
    if folded != folded_before {
        panel.settings().folded_sections = folded;
        settings_changed = true;
    }
    if settings_changed && let Err(error) = panel.settings().save() {
        panel.notify(
            NoticeKind::Error,
            format!("the settings were not saved: {error}"),
        );
    }
    if let Some(error) = changes.target_error {
        panel.notify(NoticeKind::Error, format!("Target: {error}"));
    }
    if let Some(target) = changes.target {
        set_target(&mut panel, talk.as_deref_mut(), source.as_deref(), target);
    }
    if changes.toggle_prompter {
        prompter.editor_open = !prompter.editor_open;
    }
    match changes.mic_test {
        Some(true) => match super::MicRecorder::start() {
            Ok(recorder) => {
                panel.mic_test = Some(Input {
                    recorder,
                    level: InputLevel::new(Instant::now()),
                })
            }
            Err(error) => panel.notify(
                NoticeKind::Error,
                format!("could not open the microphone: {error}"),
            ),
        },
        Some(false) => panel.mic_test = None,
        None => {}
    }
    if changes.close {
        panel.open = false;
        panel.mic_test = None;
        inset.right = 0.0;
    }
    if changes.open_settings {
        settings_window.show(crate::settings::Section::Recording);
    }
    if let Some(request) = request {
        // The button keeps no keyboard focus: the Enter that finishes the
        // recording would press it again.
        ctx.memory_mut(|memory| {
            if let Some(focused) = memory.focused() {
                memory.surrender_focus(focused);
            }
        });
        // The recorder opens the microphone itself.
        panel.mic_test = None;
        panel.request = Some(request);
    }
}

impl NarrationPanel {
    /// The practice runs of the talk at `source`, oldest first, read again
    /// every few seconds: Presenter View adds to them too. Also tells
    /// `talk` the length planned for the file last time.
    fn practice_runs(
        &mut self,
        source: Option<&Path>,
        talk: Option<&mut TalkDuration>,
    ) -> Vec<Rehearsal> {
        let stale = self
            .rehearsals
            .as_ref()
            .is_none_or(|(read, _)| read.elapsed() > REHEARSALS_REFRESH);
        if stale {
            let log = source.map(Rehearsals::load);
            if let (Some(talk), Some(log)) = (talk, &log) {
                talk.remembered = log.duration();
            }
            let runs = log.map(|log| log.all().to_vec()).unwrap_or_default();
            self.rehearsals = Some((Instant::now(), runs));
        }
        self.rehearsals
            .as_ref()
            .map(|(_, runs)| runs.clone())
            .unwrap_or_default()
    }
}

/// Plan the talk to last `target` (or clear the length set here), for this
/// run and, as Presenter View does, for the next time the file opens.
fn set_target(
    panel: &mut NarrationPanel,
    talk: Option<&mut TalkDuration>,
    source: Option<&Path>,
    target: Option<Duration>,
) {
    if let Some(talk) = talk {
        talk.chosen = target.map(Some);
        if target.is_none() {
            talk.remembered = None;
        }
    }
    if let Some(source) = source {
        Rehearsals::load(source).plan(target);
    }
    panel.rehearsals = None;
    let message = match target {
        Some(target) => format!(
            "The talk is planned for {}; Presenter View plans with it too",
            pace::format_span(target.as_secs_f64(), false)
        ),
        None => "The target set here was cleared".to_owned(),
    };
    panel.notify(NoticeKind::Info, message);
}

fn padded(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn header(
    ui: &mut egui::Ui,
    manifest: &gaanim_api::canvas::NarrationManifest,
    slides: &[super::ScriptSlide],
    script: Option<&Path>,
    request: &mut Option<Request>,
    changes: &mut Changes,
) {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 14,
            right: 10,
            top: 10,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (badge, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(22.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(badge, 0.0, palette::DANGER.gamma_multiply(0.16));
                paint_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(badge.center(), egui::Vec2::splat(13.0)),
                    Icon::Mic,
                    palette::DANGER,
                );
                ui.label(
                    egui::RichText::new("Script & narration")
                        .size(16.0)
                        .strong()
                        .color(palette::TEXT),
                );
                let sections = manifest
                    .script
                    .as_ref()
                    .map_or(&[][..], |script| &script.sections[..]);
                let missing = super::script_gaps(sections, manifest, slides);
                let chip = match &manifest.script {
                    Some(script) if missing > 0 => {
                        format!("{} · {} to write", super::file_name(&script.path), missing)
                    }
                    Some(script) => format!(
                        "{} · {}",
                        super::file_name(&script.path),
                        script.sections.len()
                    ),
                    None => "no script yet".to_owned(),
                };
                ui.label(
                    egui::RichText::new(chip)
                        .monospace()
                        .size(11.5)
                        .color(palette::TEXT_MUTED),
                )
                .on_hover_text(script.map_or_else(String::new, |path| path.display().to_string()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                        .on_hover_text("Close")
                        .clicked()
                    {
                        changes.close = true;
                    }
                    if icon_button(ui, Icon::Settings, ButtonTone::Ghost, true)
                        .on_hover_text("Recording, transcription and dictation settings · Ctrl+,")
                        .clicked()
                    {
                        changes.open_settings = true;
                    }
                    let label = if manifest.script.is_some() {
                        "Open the script: Gaanim adds the slides and stops it lacks"
                    } else {
                        "Create the script: one \"## name\" section per slide, [stop] where \
                         you advance"
                    };
                    if icon_button(ui, Icon::Book, ButtonTone::Ghost, script.is_some())
                        .on_hover_text(label)
                        .clicked()
                    {
                        *request = Some(Request::OpenScript);
                    }
                });
            });
        });
    rule(ui);
}

fn rule(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette::DIVIDER);
}

fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(11.0)
            .strong()
            .color(palette::TEXT_FAINT),
    );
}

/// The target, how the takes and the last practice fit it, and the
/// practice runs so far.
fn plan_section(
    ui: &mut egui::Ui,
    view: &mut PanelView,
    plan: Option<TalkPlan>,
    rows: &[SlideRow],
    runs: &[Rehearsal],
    changes: &mut Changes,
) {
    ui.horizontal(|ui| {
        caption(ui, "TARGET");
        let shown = plan.map_or_else(String::new, |plan| pace::format_span(plan.seconds, false));
        let text = view.target_text.get_or_insert_with(|| shown.clone());
        let response = ui.add(
            egui::TextEdit::singleline(text)
                .desired_width(70.0)
                .font(egui::FontId::monospace(15.0))
                .hint_text("15:00"),
        );
        let commit = response.lost_focus();
        if !response.has_focus() && !commit && *text != shown {
            // Changed elsewhere (Presenter View, the script): show it.
            *text = shown.clone();
        }
        if commit {
            let typed = text.trim().to_owned();
            if typed.is_empty() {
                if plan.is_some_and(|plan| {
                    matches!(
                        plan.source,
                        plan::PlanSource::Talk(pace::LengthSource::Chosen)
                    )
                }) {
                    changes.target = Some(None);
                }
            } else if typed != shown {
                match plan::parse_length(&typed) {
                    Ok(duration) => changes.target = Some(Some(duration)),
                    Err(error) => changes.target_error = Some(error.to_owned()),
                }
            }
            view.target_text = None;
        }
        response.on_hover_text(
            "How long the talk should last: 15:00, 15m or 1h30m. Practice runs and \
             Presenter View time the talk against it; empty it to clear the one set here",
        );
        let source = plan.map_or("no target yet", |plan| plan.source.describe());
        ui.label(
            egui::RichText::new(source)
                .size(12.0)
                .color(palette::TEXT_FAINT),
        );
    });

    let recorded: f64 = rows.iter().filter_map(|row| row.take).sum();
    let timed = rows.iter().filter(|row| row.take.is_some()).count();
    let last = runs.last();
    ui.columns(2, |columns| {
        stat(
            &mut columns[0],
            "TAKES",
            (timed > 0).then_some(recorded),
            plan.map(|plan| plan.seconds),
            &format!("{timed} of {} slides", rows.len()),
        );
        let detail = match last {
            Some(run) => format!("{} · {} run(s)", ago(run.started), runs.len()),
            None => "Practise to compare runs".to_owned(),
        };
        stat(
            &mut columns[1],
            "LAST PRACTICE",
            last.map(|run| run.total),
            plan.map(|plan| plan.seconds),
            &detail,
        );
    });
    if let Some(plan) = plan {
        progress(ui, recorded, last.map(|run| run.total), plan.seconds);
    }
    if runs.len() > 1 {
        history(ui, runs, plan.map(|plan| plan.seconds));
    }
}

/// A big time against the plan, and a line under it.
fn stat(ui: &mut egui::Ui, title: &str, seconds: Option<f64>, planned: Option<f64>, detail: &str) {
    caption(ui, title);
    match seconds {
        Some(seconds) => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(pace::format_span(seconds, false))
                        .monospace()
                        .size(20.0)
                        .color(palette::TEXT),
                );
                if let Some(planned) = planned {
                    let left = planned - seconds;
                    let (text, color) = if left < -0.5 {
                        (
                            format!("{} over", pace::format_span(-left, false)),
                            timing_color(seconds, Some(planned)),
                        )
                    } else {
                        (
                            format!("{} left", pace::format_span(left.max(0.0), false)),
                            palette::TEXT_MUTED,
                        )
                    };
                    ui.label(egui::RichText::new(text).size(12.0).color(color));
                }
            });
        }
        None => {
            ui.label(
                egui::RichText::new("—")
                    .monospace()
                    .size(20.0)
                    .color(palette::TEXT_FAINT),
            );
        }
    }
    ui.label(
        egui::RichText::new(detail)
            .size(12.0)
            .color(palette::TEXT_MUTED),
    );
}

/// When a rehearsal that started at `started` (Unix seconds) was, roughly.
fn ago(started: u64) -> String {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let seconds = now.saturating_sub(started);
    match seconds {
        0..120 => "just now".to_owned(),
        120..7200 => format!("{} min ago", seconds / 60),
        7200..172_800 => format!("{} h ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86_400),
    }
}

/// The takes' total along the plan, the last practice as a tick, and the
/// plan's end in gold.
fn progress(ui: &mut egui::Ui, recorded: f64, practice: Option<f64>, planned: f64) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 12.0), egui::Sense::hover());
    let painter = ui.painter();
    let span = planned.max(recorded).max(practice.unwrap_or(0.0)).max(1.0) * 1.04;
    let x = |seconds: f64| rect.min.x + rect.width() * (seconds / span).clamp(0.0, 1.0) as f32;
    let bar = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, rect.center().y - 3.0),
        egui::pos2(rect.max.x, rect.center().y + 3.0),
    );
    painter.rect_filled(bar, 0.0, palette::FIELD);
    painter.rect_filled(
        egui::Rect::from_min_max(bar.min, egui::pos2(x(recorded), bar.max.y)),
        0.0,
        timing_color(recorded, Some(planned)).gamma_multiply(0.85),
    );
    if let Some(practice) = practice {
        let at = x(practice);
        painter.line_segment(
            [egui::pos2(at, rect.min.y), egui::pos2(at, rect.max.y)],
            egui::Stroke::new(2.0, palette::TEXT),
        );
    }
    let end = x(planned);
    painter.line_segment(
        [
            egui::pos2(end, rect.min.y - 2.0),
            egui::pos2(end, rect.max.y + 2.0),
        ],
        egui::Stroke::new(2.0, palette::STOP),
    );
    response.on_hover_text(format!(
        "Bar: the takes add up to {}. White tick: the last practice{}. Gold: the target, {}",
        pace::format_span(recorded, false),
        practice.map_or_else(String::new, |practice| format!(
            ", {}",
            pace::format_span(practice, false)
        )),
        pace::format_span(planned, false)
    ));
}

/// The last practice runs' totals as bars, against the target.
fn history(ui: &mut egui::Ui, runs: &[Rehearsal], planned: Option<f64>) {
    caption(ui, "PRACTICE RUNS");
    let shown = &runs[runs.len().saturating_sub(HISTORY_RUNS)..];
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::hover());
    let top = shown
        .iter()
        .map(|run| run.total)
        .fold(planned.unwrap_or(0.0), f64::max)
        .max(1.0)
        * 1.1;
    let slot = rect.width() / HISTORY_RUNS as f32;
    for (index, run) in shown.iter().enumerate() {
        let height = (rect.height() * (run.total / top) as f32).max(2.0);
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + slot * index as f32 + 2.0, rect.max.y - height),
            egui::pos2(rect.min.x + slot * (index + 1) as f32 - 2.0, rect.max.y),
        );
        let latest = index + 1 == shown.len();
        let color = timing_color(run.total, planned);
        ui.painter().rect_filled(
            bar,
            0.0,
            if latest {
                color
            } else {
                color.gamma_multiply(0.45)
            },
        );
        let response = ui.interact(
            bar,
            ui.id().with(("practice-run", index)),
            egui::Sense::hover(),
        );
        response.on_hover_text(format!(
            "{} · {}",
            ago(run.started),
            pace::format_span(run.total, false)
        ));
    }
    if let Some(planned) = planned {
        let y = rect.max.y - rect.height() * (planned / top) as f32;
        ui.painter().line_segment(
            [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
            egui::Stroke::new(1.0, palette::STOP.gamma_multiply(0.8)),
        );
    }
}

/// A bar per slide, as wide as its share of the plan, filled by the time
/// it took against that share; a click goes to the slide.
fn talk_map(
    ui: &mut egui::Ui,
    view: ListView,
    rows: &[SlideRow],
    current: Option<usize>,
    timeline: &Timeline,
    request: &mut Option<Request>,
) {
    caption(ui, "TALK MAP");
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::hover());
    let weights: Vec<f64> = rows
        .iter()
        .map(|row| row.budget.unwrap_or(0.0).max(1.0))
        .collect();
    let total: f64 = weights.iter().sum::<f64>().max(1.0);
    let gap = if rows.len() > 60 { 0.0 } else { 1.0 };
    let mut x = rect.min.x;
    for (row, weight) in rows.iter().zip(&weights) {
        let width = rect.width() * (*weight / total) as f32;
        let cell = egui::Rect::from_min_max(
            egui::pos2(x, rect.min.y),
            egui::pos2((x + width - gap).max(x + 1.0), rect.max.y),
        );
        x += width;
        let shown = row.shown(view);
        let painter = ui.painter();
        painter.rect_filled(cell, 0.0, palette::FIELD);
        if let Some(seconds) = shown {
            let fill = match row.budget {
                Some(budget) if budget > 0.0 => (seconds / budget).min(1.0) as f32,
                _ => 1.0,
            };
            let bar = egui::Rect::from_min_max(
                egui::pos2(cell.min.x, cell.max.y - cell.height() * fill.max(0.15)),
                cell.max,
            );
            painter.rect_filled(bar, 0.0, timing_color(seconds, row.budget));
        }
        if Some(row.index) == current {
            painter.rect_stroke(
                cell.expand(1.0),
                0.0,
                egui::Stroke::new(1.5, palette::TEXT),
                egui::StrokeKind::Outside,
            );
        }
        let response = ui
            .interact(
                cell,
                ui.id().with(("talk-map", row.index)),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            *request = Some(Request::Seek(crate::presenter::segment_entry_time(
                timeline,
                row.segment,
            )));
        }
        response.on_hover_text(row_summary(row, view));
    }
}

/// A slide's name and times, for tooltips.
fn row_summary(row: &SlideRow, view: ListView) -> String {
    let mut text = format!("{:02} {}", row.index + 1, row.segment.name);
    let span = |seconds: Option<f64>| {
        seconds.map_or_else(
            || "—".to_owned(),
            |seconds| pace::format_span(seconds, false),
        )
    };
    text.push_str(&format!(
        "\n{}: {}",
        match view {
            ListView::Takes => "Take",
            ListView::Practice => "Last practice",
        },
        span(row.shown(view))
    ));
    if let Some(budget) = row.budget {
        let kind = if row.segment.target.is_some() {
            "target"
        } else {
            "share of the plan"
        };
        text.push_str(&format!(" · {kind} {}", span(Some(budget))));
    }
    text
}

/// Takes or practice, which slides, and a search.
fn list_toolbar(ui: &mut egui::Ui, view: &mut PanelView, rows: &[SlideRow]) {
    crate::ui_kit::segmented(
        ui,
        "narration-list-view",
        &mut view.list,
        &[
            (ListView::Takes, "Takes", ""),
            (ListView::Practice, "Last practice", ""),
        ],
    );
    let missing = rows
        .iter()
        .filter(|row| row.shown(view.list).is_none())
        .count();
    let over = rows.iter().filter(|row| row.over(view.list)).count();
    ui.horizontal(|ui| {
        for (filter, label) in [
            (SlideFilter::All, format!("All {}", rows.len())),
            (SlideFilter::Missing, format!("No time {missing}")),
            (SlideFilter::Over, format!("Over {over}")),
        ] {
            if crate::ui_kit::chip(ui, &label, view.filter == filter).clicked() {
                view.filter = filter;
            }
        }
        ui.add(
            egui::TextEdit::singleline(&mut view.query)
                .hint_text("Filter slides")
                .desired_width(ui.available_width()),
        );
    });
}

/// The slides the toolbar asks for, each with its time against its share
/// and what to do with it.
#[allow(clippy::too_many_arguments)]
fn slide_list(
    ui: &mut egui::Ui,
    view: &PanelView,
    rows: &[SlideRow],
    current: Option<usize>,
    live: bool,
    timeline: &Timeline,
    expanded: &mut std::collections::BTreeSet<String>,
    audio: &mut TalkAudio,
    dictate: bool,
    request: &mut Option<Request>,
) {
    let query = view.query.trim().to_lowercase();
    // Measured once: a row that overflowed would widen the next ones.
    let name_width = (ui.available_width() - ROW_FIXED).max(60.0);
    let shown: Vec<&SlideRow> = rows
        .iter()
        .filter(|row| match view.filter {
            SlideFilter::All => true,
            SlideFilter::Missing => row.shown(view.list).is_none(),
            SlideFilter::Over => row.over(view.list),
        })
        .filter(|row| query.is_empty() || row.segment.name.to_lowercase().contains(&query))
        .collect();
    if shown.is_empty() {
        ui.label(
            egui::RichText::new("No slide matches")
                .size(13.0)
                .color(palette::TEXT_FAINT),
        );
        return;
    }
    let live_hint = "A live take is recorded whole: record it again from its card";
    ui.spacing_mut().item_spacing.y = 2.0;
    for row in shown {
        let parts = slide_parts(row.segment);
        let open = expanded.contains(&row.segment.name);
        let is_current = Some(row.index) == current;
        let response = ui
            .horizontal(|ui| {
                ui.set_height(ROW_HEIGHT);
                ui.spacing_mut().item_spacing.x = 6.0;
                if view.list == ListView::Takes && parts > 1 {
                    let toggle = super::disclosure(ui, open).on_hover_text(if open {
                        "Hide its stops"
                    } else {
                        "Show its stops, to record or hear one again"
                    });
                    if toggle.clicked() {
                        if open {
                            expanded.remove(&row.segment.name);
                        } else {
                            expanded.insert(row.segment.name.clone());
                        }
                    }
                } else {
                    // The fold arrow takes its width and a gap.
                    ui.add_space(16.0 + ui.spacing().item_spacing.x);
                }
                ui.add_sized(
                    egui::vec2(20.0, ROW_HEIGHT),
                    egui::Label::new(
                        egui::RichText::new(format!("{:02}", row.index + 1))
                            .monospace()
                            .size(11.0)
                            .color(palette::TEXT_FAINT),
                    ),
                );
                let name_color = if is_current {
                    palette::TEXT
                } else {
                    palette::TEXT_MUTED
                };
                ui.allocate_ui_with_layout(
                    egui::vec2(name_width, ROW_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(name_width);
                        if super::name_link(ui, &row.segment.name, 13.5, name_color)
                            .on_hover_text(row_summary(row, view.list))
                            .clicked()
                        {
                            *request = Some(Request::Seek(crate::presenter::segment_entry_time(
                                timeline,
                                row.segment,
                            )));
                        }
                    },
                );
                time_bar(ui, row.shown(view.list), row.budget, BAR_WIDTH);
                let seconds = row.shown(view.list);
                ui.add_sized(
                    egui::vec2(TIME_WIDTH, ROW_HEIGHT),
                    egui::Label::new(
                        egui::RichText::new(seconds.map_or_else(
                            || "—".to_owned(),
                            |seconds| pace::format_span(seconds, false),
                        ))
                        .monospace()
                        .size(12.0)
                        .color(
                            seconds.map_or(palette::TEXT_FAINT, |seconds| {
                                timing_color(seconds, row.budget)
                            }),
                        ),
                    ),
                );
                match view.list {
                    ListView::Takes => {
                        let clips = audio.clips(&row.segment.name, None);
                        audio.listen(ui, row.segment.name.clone(), clips);
                        let (hint, only) = if row.take.is_some() {
                            (
                                "Record this slide again: it ends once you leave it, and its \
                                 time, script and audio are replaced",
                                true,
                            )
                        } else {
                            (
                                "Record from this slide on: each slide you go through keeps \
                                 its time; Enter finishes",
                                false,
                            )
                        };
                        if icon_button(ui, Icon::Record, ButtonTone::Ghost, !live)
                            .on_hover_text(if live { live_hint } else { hint })
                            .clicked()
                        {
                            *request = Some(Request::RecordTalk {
                                from: Some(row.segment.name.clone()),
                                part: None,
                                only,
                                dictate,
                            });
                        }
                    }
                    ListView::Practice => {
                        if icon_button(ui, Icon::Present, ButtonTone::Ghost, true)
                            .on_hover_text("Practise from this slide on")
                            .clicked()
                        {
                            *request = Some(Request::Practice {
                                from: Some(row.segment.name.clone()),
                            });
                        }
                    }
                }
            })
            .response;
        if is_current {
            let rect = response.rect;
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.min.x - 8.0, rect.min.y),
                    egui::pos2(rect.min.x - 6.0, rect.max.y),
                ),
                0.0,
                palette::STOP,
            );
        }
        if !open || view.list != ListView::Takes {
            continue;
        }
        for part in 0..parts {
            let label = part_label(row.segment, part);
            ui.horizontal(|ui| {
                ui.set_height(ROW_HEIGHT - 2.0);
                ui.add_space(46.0);
                let time = match row.segment.stops.get(part) {
                    Some(stop) => stop.time,
                    // Past the last stop.
                    None => row
                        .segment
                        .stops
                        .last()
                        .map_or(row.segment.start_time, |stop| stop.time + 1e-3),
                };
                let part_width = name_width + BAR_WIDTH + 20.0 - 46.0 + 12.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(part_width, ROW_HEIGHT - 2.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(part_width);
                        if super::name_link(ui, &label, 12.5, palette::TEXT_MUTED)
                            .on_hover_text("Go to this stop")
                            .clicked()
                        {
                            *request = Some(Request::Seek(time));
                        }
                    },
                );
                let seconds = row.parts.get(part).copied();
                ui.add_sized(
                    egui::vec2(TIME_WIDTH, ROW_HEIGHT - 2.0),
                    egui::Label::new(
                        egui::RichText::new(seconds.map_or_else(
                            || "—".to_owned(),
                            |seconds| pace::format_span(seconds, false),
                        ))
                        .monospace()
                        .size(11.5)
                        .color(palette::TEXT_MUTED),
                    ),
                );
                let clips = audio.clips(&row.segment.name, Some(part));
                audio.listen(ui, format!("{} · {label}", row.segment.name), clips);
                if icon_button(ui, Icon::Record, ButtonTone::Ghost, !live)
                    .on_hover_text(if live {
                        live_hint
                    } else {
                        "Record only this stop again: from the stop before it until you \
                         advance at this one"
                    })
                    .clicked()
                {
                    *request = Some(Request::RecordTalk {
                        from: Some(row.segment.name.clone()),
                        part: Some(part),
                        only: true,
                        dictate,
                    });
                }
            });
        }
    }
    ui.spacing_mut().item_spacing.y = 8.0;
}

/// A small bar: `seconds` against `budget`, with the budget marked.
fn time_bar(ui: &mut egui::Ui, seconds: Option<f64>, budget: Option<f64>, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::hover());
    let bar = egui::Rect::from_center_size(rect.center(), egui::vec2(width, 5.0));
    let painter = ui.painter();
    painter.rect_filled(bar, 0.0, palette::FIELD);
    // The budget sits at 70%, so going over shows.
    let scale = match budget {
        Some(budget) if budget > 0.0 => 0.7 / budget,
        _ => 0.0,
    };
    if let Some(seconds) = seconds {
        let fraction = if scale > 0.0 {
            (seconds * scale).min(1.0) as f32
        } else {
            1.0
        };
        painter.rect_filled(
            egui::Rect::from_min_max(
                bar.min,
                egui::pos2(bar.min.x + bar.width() * fraction, bar.max.y),
            ),
            0.0,
            timing_color(seconds, budget),
        );
    }
    if scale > 0.0 {
        let x = bar.min.x + bar.width() * 0.7;
        painter.line_segment(
            [
                egui::pos2(x, bar.min.y - 3.0),
                egui::pos2(x, bar.max.y + 3.0),
            ],
            egui::Stroke::new(1.0, palette::TEXT_FAINT),
        );
    }
}

/// The script of the slide on screen: how much it says, how long that
/// takes, and how the takes and the practice went.
fn script_card(
    ui: &mut egui::Ui,
    row: &SlideRow,
    wpm: f64,
    prompter_open: bool,
    request: &mut Option<Request>,
    changes: &mut Changes,
) {
    ui.add_space(4.0);
    crate::ui_kit::field_frame()
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                caption(
                    ui,
                    &format!(
                        "SCRIPT · {:02} {}",
                        row.index + 1,
                        row.segment.name.to_uppercase()
                    ),
                );
            });
            let words = plan::slide_words(row.segment);
            let reading = plan::reading_seconds(words, wpm);
            ui.label(
                egui::RichText::new(if words == 0 {
                    "No script for this slide yet".to_owned()
                } else {
                    format!(
                        "{words} words · ≈{} at {wpm:.0} wpm",
                        pace::format_span(reading, false)
                    )
                })
                .monospace()
                .size(12.0)
                .color(palette::TEXT_MUTED),
            );
            ui.horizontal_wrapped(|ui| {
                let span = |seconds: f64| pace::format_span(seconds, false);
                if let Some(budget) = row.budget {
                    figure(ui, "Share", &span(budget), palette::TEXT_MUTED);
                }
                if let Some(take) = row.take {
                    figure(ui, "Take", &span(take), timing_color(take, row.budget));
                }
                if let Some(practice) = row.practice {
                    figure(
                        ui,
                        "Practice",
                        &span(practice),
                        timing_color(practice, row.budget),
                    );
                }
                if let Some(budget) = row.budget
                    && words > 0
                    && reading > budget + 0.5
                {
                    figure(
                        ui,
                        "Script",
                        &format!("{} too long", span(reading - budget)),
                        palette::STOP,
                    );
                }
            });
            if let Some(notes) = row
                .segment
                .notes
                .as_deref()
                .filter(|notes| !notes.trim().is_empty())
            {
                ui.label(
                    egui::RichText::new(preview(notes))
                        .size(13.0)
                        .color(palette::TEXT_MUTED),
                );
            }
            ui.horizontal(|ui| {
                let label = if prompter_open {
                    "Hide the prompter"
                } else {
                    "Rehearse with the prompter"
                };
                if small_button_sized(ui, label, true, 13.0)
                    .on_hover_text(
                        "The prompter over the preview lights the script word by word at \
                         your pace (Shift+T)",
                    )
                    .clicked()
                {
                    changes.toggle_prompter = true;
                }
                if small_button_sized(ui, "Open in the editor", true, 13.0)
                    .on_hover_text("Opens the script at this slide's section")
                    .clicked()
                {
                    *request = Some(Request::OpenScriptAt(row.segment.name.clone()));
                }
            });
        });
}

/// A label and its value, side by side.
fn figure(ui: &mut egui::Ui, label: &str, value: &str, color: egui::Color32) {
    ui.label(
        egui::RichText::new(label)
            .size(12.0)
            .color(palette::TEXT_FAINT),
    );
    ui.label(
        egui::RichText::new(value)
            .monospace()
            .size(12.5)
            .color(color),
    );
    ui.add_space(6.0);
}

/// The start of a slide's script, its stops shown as ▸.
fn preview(notes: &str) -> String {
    let text = split_notes(notes)
        .iter()
        .map(|part| part.text.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("  ▸  ");
    if text.chars().count() > SCRIPT_PREVIEW_CHARS {
        let cut: String = text.chars().take(SCRIPT_PREVIEW_CHARS).collect();
        format!("{}…", cut.trim_end())
    } else {
        text
    }
}

/// The controls at the bottom: record or practise, from here or the start.
#[allow(clippy::too_many_arguments)]
fn dock(
    ui: &mut egui::Ui,
    view: &mut PanelView,
    here: Option<&SegmentMetadata>,
    plan: Option<TalkPlan>,
    mic: Option<&(String, InputLevel, Option<super::LevelWarning>)>,
    live: bool,
    (dictate, can_dictate, keep_audio): (&mut bool, bool, &mut bool),
    notice: Option<&(String, NoticeKind)>,
    request: &mut Option<Request>,
    changes: &mut Changes,
) {
    rule(ui);
    egui::Frame::new()
        .fill(palette::SURFACE)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                crate::ui_kit::segmented(
                    ui,
                    "narration-dock-mode",
                    &mut view.dock,
                    &[
                        (DockMode::Record, "Record", ""),
                        (DockMode::Practice, "Practice", ""),
                    ],
                );
                let here_name = here.map(|slide| slide.name.clone());
                match view.dock {
                    DockMode::Record => {
                        ui.horizontal(|ui| {
                            let (icon, _) = ui
                                .allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                            paint_icon(ui.painter(), icon, Icon::Mic, palette::TEXT_MUTED);
                            match mic {
                                Some((device, level, _)) => {
                                    ui.label(
                                        egui::RichText::new(device)
                                            .size(12.0)
                                            .color(palette::TEXT_MUTED),
                                    );
                                    level_meter(
                                        ui,
                                        level,
                                        (ui.available_width() - 110.0).max(60.0),
                                    );
                                    if small_button_sized(ui, "Stop", true, 12.5).clicked() {
                                        changes.mic_test = Some(false);
                                    }
                                }
                                None => {
                                    ui.label(
                                        egui::RichText::new("Microphone")
                                            .size(12.0)
                                            .color(palette::TEXT_MUTED),
                                    );
                                    if small_button_sized(ui, "Test level", true, 12.5)
                                        .on_hover_text(
                                            "Opens the microphone to show its level, without \
                                             recording",
                                        )
                                        .clicked()
                                    {
                                        changes.mic_test = Some(true);
                                    }
                                }
                            }
                        });
                        if let Some((_, _, Some(warning))) = mic {
                            ui.label(
                                egui::RichText::new(warning.message())
                                    .size(12.0)
                                    .color(warning.color()),
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            let from_here = !live && here_name.is_some();
                            if primary_button_sized(
                                ui,
                                "Record from here",
                                Some(Icon::Record),
                                from_here,
                                ButtonSize::REGULAR,
                            )
                            .on_hover_text(
                                "Present from the slide on screen while speaking: each slide \
                                 you go through keeps its time and audio. → or Space advances, \
                                 Enter finishes, Esc discards",
                            )
                            .clicked()
                            {
                                *request = Some(Request::RecordTalk {
                                    from: here_name.clone(),
                                    part: None,
                                    only: false,
                                    dictate: *dictate && can_dictate,
                                });
                            }
                            if secondary_button_sized(
                                ui,
                                "This slide",
                                from_here,
                                ButtonSize::REGULAR,
                            )
                            .on_hover_text("Record only the slide on screen again")
                            .clicked()
                            {
                                *request = Some(Request::RecordTalk {
                                    from: here_name.clone(),
                                    part: None,
                                    only: true,
                                    dictate: *dictate && can_dictate,
                                });
                            }
                            if secondary_button_sized(
                                ui,
                                "From the start",
                                true,
                                ButtonSize::REGULAR,
                            )
                            .clicked()
                            {
                                *request = Some(Request::RecordTalk {
                                    from: None,
                                    part: None,
                                    only: false,
                                    dictate: *dictate && can_dictate,
                                });
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.add_enabled(
                                can_dictate,
                                egui::Checkbox::new(dictate, "Dictate into the script"),
                            )
                            .on_hover_text(
                                "Zoom Scribe writes each phrase into its slide and stop; the \
                                 earlier text stays as a <!-- previous: … --> comment",
                            )
                            .on_disabled_hover_text(
                                "Save a Scribe token in Settings → Dictation to dictate",
                            );
                            crate::ui_kit::checkbox(ui, keep_audio, "Keep the audio")
                                .on_hover_text(
                                    "Keeps each slide's audio in Gaanim's data folder to listen \
                                 back; never in the project",
                                );
                        });
                    }
                    DockMode::Practice => {
                        let text = match plan {
                            Some(plan) => format!(
                                "Times a run against the {} target, slide by slide; nothing \
                                 is recorded or changed. Presenter View runs count too.",
                                pace::format_span(plan.seconds, false)
                            ),
                            None => "Set a target above to see, at every slide, whether \
                                     you are ahead or behind. Nothing is recorded."
                                .to_owned(),
                        };
                        ui.label(
                            egui::RichText::new(text)
                                .size(12.5)
                                .color(palette::TEXT_MUTED),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if primary_button_sized(
                                ui,
                                "Practise from the start",
                                Some(Icon::Present),
                                true,
                                ButtonSize::REGULAR,
                            )
                            .on_hover_text("→ or Space advances, Enter finishes, Esc discards")
                            .clicked()
                            {
                                *request = Some(Request::Practice { from: None });
                            }
                            if secondary_button_sized(
                                ui,
                                "From here",
                                here_name.is_some(),
                                ButtonSize::REGULAR,
                            )
                            .clicked()
                            {
                                *request = Some(Request::Practice {
                                    from: here_name.clone(),
                                });
                            }
                        });
                    }
                }
                if let Some((message, kind)) = notice {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(message)
                                .size(12.5)
                                .color(notice_color(*kind)),
                        )
                        .wrap(),
                    );
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dock_leaves_the_scene_most_of_a_narrow_window() {
        assert_eq!(dock_width(1920.0), DOCK_WIDTH);
        assert!((dock_width(700.0) - 315.0).abs() < 1e-3);
        assert_eq!(dock_width(400.0), 280.0);
    }

    #[test]
    fn the_preview_marks_stops_and_stops_short() {
        assert_eq!(preview("One two.\n[stop]\nThree."), "One two.  ▸  Three.");
        let long = "word ".repeat(200);
        let shown = preview(&long);
        assert!(shown.ends_with('…'));
        assert!(shown.chars().count() <= SCRIPT_PREVIEW_CHARS + 1);
    }
}
