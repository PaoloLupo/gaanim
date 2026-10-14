//! The speaker's script as a teleprompter: each slide's notes cut at their
//! stop marks (see `gaanim_timeline::notes`), the part being said lit word
//! by word at the speaker's reading pace and kept in the top third, and a
//! cue wherever the speaker advances. Presenter View shows it, as the
//! current slide's notes or as the whole talk, and the editor shows the same
//! prompter in a window of its own, which live takes and dictations open.

use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_timeline::notes::{NotesPart, split_notes};
use gaanim_timeline::timeline::{SegmentMetadata, Timeline};

use crate::ui_kit::{palette, small_button};

/// Text size and reading pace of the prompter, the same in Presenter View
/// and in the editor, and whether the editor shows its prompter window.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub(crate) struct PrompterSettings {
    pub(crate) size: f32,
    /// Words per minute the part being said is lit at.
    pub(crate) words_per_minute: f32,
    pub(crate) editor_open: bool,
}

impl Default for PrompterSettings {
    fn default() -> Self {
        Self {
            size: 30.0,
            words_per_minute: 150.0,
            editor_open: false,
        }
    }
}

const SIZE_RANGE: (f32, f32) = (16.0, 56.0);
const PACE_RANGE: (f32, f32) = (90.0, 220.0);

impl PrompterSettings {
    fn adjust_size(&mut self, delta: f32) {
        self.size = (self.size + delta).clamp(SIZE_RANGE.0, SIZE_RANGE.1);
    }

    fn adjust_pace(&mut self, delta: f32) {
        self.words_per_minute = (self.words_per_minute + delta).clamp(PACE_RANGE.0, PACE_RANGE.1);
    }
}

/// The prompter's size and pace buttons.
pub(crate) fn prompter_controls(
    ui: &mut egui::Ui,
    settings: &mut PrompterSettings,
    words: &PrompterWords,
) {
    ui.spacing_mut().item_spacing.x = 4.0;
    if small_button(ui, "A+", settings.size < SIZE_RANGE.1)
        .on_hover_text(words.larger)
        .clicked()
    {
        settings.adjust_size(2.0);
    }
    if small_button(ui, "A−", settings.size > SIZE_RANGE.0)
        .on_hover_text(words.smaller)
        .clicked()
    {
        settings.adjust_size(-2.0);
    }
    ui.add_space(6.0);
    if small_button(ui, "+", settings.words_per_minute < PACE_RANGE.1)
        .on_hover_text(words.faster)
        .clicked()
    {
        settings.adjust_pace(10.0);
    }
    ui.label(
        egui::RichText::new(format!("{:.0} {}", settings.words_per_minute, words.pace))
            .size(12.0)
            .monospace()
            .color(palette::TEXT_MUTED),
    );
    if small_button(ui, "−", settings.words_per_minute > PACE_RANGE.0)
        .on_hover_text(words.slower)
        .clicked()
    {
        settings.adjust_pace(-10.0);
    }
}

/// Words of the part being said that the speaker has read by now, at
/// `words_per_minute` from the moment the part became the current one. The
/// clock lives in `ui`'s memory under `id`, one per prompter.
pub(crate) fn words_read(
    ui: &egui::Ui,
    id: egui::Id,
    position: Option<ScriptPosition>,
    words_per_minute: f32,
) -> f64 {
    let now = ui.input(|input| input.time);
    let key = id.with("prompter-reading");
    let anchor = position.map(|position| (position.slide, position.part));
    let since = match ui.data(|data| data.get_temp::<(Option<(usize, usize)>, f64)>(key)) {
        Some((kept, since)) if kept == anchor => since,
        _ => now,
    };
    ui.data_mut(|data| data.insert_temp(key, (anchor, since)));
    (now - since).max(0.0) * f64::from(words_per_minute) / 60.0
}

/// The byte and character index where `text` splits after its first
/// `words` words: at the space that follows them.
fn split_after_words(text: &str, words: usize) -> (usize, usize) {
    if words == 0 {
        return (0, 0);
    }
    let mut seen = 0;
    let mut in_word = false;
    for (characters, (byte, character)) in text.char_indices().enumerate() {
        if character.is_whitespace() {
            if in_word {
                in_word = false;
                if seen == words {
                    return (byte, characters);
                }
            }
        } else if !in_word {
            in_word = true;
            seen += 1;
        }
    }
    (text.len(), text.chars().count())
}

/// The segment a scene has before its first `scene.segment(...)`.
pub(crate) const IMPLICIT_SEGMENT: &str = "_default";

/// Where the speaker is in the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScriptPosition {
    /// The slide, by index in the timeline's segments.
    pub(crate) slide: usize,
    /// The part being said: the text before the mark of stop `part`, or
    /// the text after the last mark once every stop is behind.
    pub(crate) part: usize,
    /// The slide rests at the stop of `part`: the speaker advances once
    /// the part is said.
    pub(crate) resting: bool,
}

impl ScriptPosition {
    /// Where the presentation at `timeline`'s playhead is in the script.
    pub(crate) fn of(timeline: &Timeline) -> Option<Self> {
        let position = timeline.segment_position_at(timeline.current_time)?;
        let slide = timeline
            .segments
            .iter()
            .position(|segment| segment.id == position.segment_id)?;
        let Some(stop) = position.stop_index else {
            return Some(Self {
                slide,
                part: 0,
                resting: false,
            });
        };
        let resting = timeline.resting_stop().is_some_and(|time| {
            timeline.segments[slide]
                .stops
                .get(stop)
                .is_some_and(|reached| (reached.time - time).abs() <= 1e-5)
        });
        // Once the speaker advances past a stop, the next part begins.
        Some(Self {
            slide,
            part: if resting { stop } else { stop + 1 },
            resting,
        })
    }
}

/// How far the speaker is through a part of the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartState {
    Said,
    Now,
    Ahead,
}

/// A line of the teleprompter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ScriptLine {
    /// A slide's heading, in the whole talk's script.
    Slide {
        number: usize,
        name: String,
        state: PartState,
    },
    /// What the speaker says.
    Text { text: String, state: PartState },
    /// Where the speaker advances.
    Advance {
        /// The stop's name, the mark's, or its number.
        label: String,
        state: PartState,
        /// The slide waits here now.
        resting: bool,
        /// The notes mark a stop the slide does not have.
        unmatched: bool,
        /// Seconds the part before takes to read aloud.
        reading: Option<f64>,
    },
    /// A slide without notes.
    Silent { state: PartState },
}

impl ScriptLine {
    fn state(&self) -> PartState {
        match self {
            Self::Slide { state, .. }
            | Self::Text { state, .. }
            | Self::Advance { state, .. }
            | Self::Silent { state } => *state,
        }
    }
}

/// The lines of the slides `slides` of `segments`, with `position` marking
/// what is being said; `headings` names each slide.
pub(crate) fn script_lines(
    segments: &[SegmentMetadata],
    slides: std::ops::Range<usize>,
    position: Option<ScriptPosition>,
    headings: bool,
) -> Vec<ScriptLine> {
    let mut lines = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        if !slides.contains(&index) {
            continue;
        }
        let notes = segment
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|notes| !notes.is_empty());
        if segment.name == IMPLICIT_SEGMENT && notes.is_none() && segment.stops.is_empty() {
            continue;
        }
        let state = |part: usize| match position {
            Some(position) if position.slide == index => match part.cmp(&position.part) {
                std::cmp::Ordering::Less => PartState::Said,
                std::cmp::Ordering::Equal => PartState::Now,
                std::cmp::Ordering::Greater => PartState::Ahead,
            },
            Some(position) if index < position.slide => PartState::Said,
            _ => PartState::Ahead,
        };
        let slide_state = match position {
            Some(position) if position.slide == index => PartState::Now,
            Some(position) if index < position.slide => PartState::Said,
            _ => PartState::Ahead,
        };
        let resting = |part: usize| {
            position.is_some_and(|position| {
                position.slide == index && position.part == part && position.resting
            })
        };
        let label = |part: usize, mark: Option<&str>| {
            segment
                .stops
                .get(part)
                .and_then(|stop| stop.name.as_deref())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .or(mark)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Step {}", part + 1))
        };
        if headings {
            lines.push(ScriptLine::Slide {
                number: index + 1,
                name: segment.name.clone(),
                state: slide_state,
            });
        }
        let parts = notes.map(split_notes).unwrap_or_else(|| {
            vec![NotesPart {
                text: String::new(),
                mark: None,
            }]
        });
        if notes.is_none() {
            lines.push(ScriptLine::Silent { state: slide_state });
        }
        let marked = parts.len() - 1;
        for (part, notes_part) in parts.into_iter().enumerate() {
            let reading = reading_seconds(&notes_part.text);
            if !notes_part.text.is_empty() {
                lines.push(ScriptLine::Text {
                    text: notes_part.text,
                    state: state(part),
                });
            }
            if let Some(mark) = notes_part.mark {
                lines.push(ScriptLine::Advance {
                    label: label(part, mark.name.as_deref()),
                    state: state(part),
                    resting: resting(part),
                    unmatched: part >= segment.stops.len(),
                    reading,
                });
            }
        }
        // Stops the notes do not mark follow them.
        for part in marked..segment.stops.len() {
            lines.push(ScriptLine::Advance {
                label: label(part, None),
                state: state(part),
                resting: resting(part),
                unmatched: false,
                reading: None,
            });
        }
    }
    lines
}

/// How much of a slide's script is written, or dictated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScriptCoverage {
    /// No part has text yet.
    Pending,
    /// Some parts have text.
    Partial,
    /// Every part the speaker says has text: one per stop, or the whole
    /// notes of a slide without stops.
    Complete,
}

/// How much of `segment`'s script is written.
pub(crate) fn script_coverage(segment: &SegmentMetadata) -> ScriptCoverage {
    let parts = segment
        .notes
        .as_deref()
        .map(split_notes)
        .unwrap_or_default();
    let needed = segment.stops.len().max(1);
    let written = (0..needed)
        .filter(|part| {
            parts
                .get(*part)
                .is_some_and(|notes_part| !notes_part.text.trim().is_empty())
        })
        .count();
    match written {
        0 => ScriptCoverage::Pending,
        written if written < needed => ScriptCoverage::Partial,
        _ => ScriptCoverage::Complete,
    }
}

/// Seconds `text` takes to read aloud at the narration's estimated pace.
fn reading_seconds(text: &str) -> Option<f64> {
    let words = gaanim_media::narration::text_words(text).len();
    (words > 0).then(|| words as f64 / gaanim_media::narration::ESTIMATED_WORDS_PER_SECOND)
}

/// The words the teleprompter writes.
pub(crate) struct PrompterWords {
    pub(crate) advance: &'static str,
    pub(crate) silent: &'static str,
    pub(crate) unmatched: &'static str,
    pub(crate) larger: &'static str,
    pub(crate) smaller: &'static str,
    pub(crate) faster: &'static str,
    pub(crate) slower: &'static str,
    /// The unit of the reading pace.
    pub(crate) pace: &'static str,
}

impl PrompterWords {
    pub(crate) const ENGLISH: Self = Self {
        advance: "ADVANCE",
        silent: "No notes for this slide",
        unmatched: "no such stop",
        larger: "Larger text",
        smaller: "Smaller text",
        faster: "Read faster",
        slower: "Read slower",
        pace: "wpm",
    };
}

/// Draw `lines` with their text at `size` points. The part being said
/// lights up word by word as `read` words of it go by; with `follow`, the
/// view scrolls smoothly to keep that point in the top third, as on a
/// teleprompter.
pub(crate) fn show_script(
    ui: &mut egui::Ui,
    lines: &[ScriptLine],
    size: f32,
    follow: bool,
    read: Option<f64>,
    words: &PrompterWords,
) {
    let small = (size * 0.6).max(12.0);
    let content_top = ui.min_rect().top();
    // Where the reading point is: inside the part being said, or its line.
    let mut reading: Option<egui::Rect> = None;
    let mut still_reading = false;
    let pulse =
        (ui.input(|input| input.time) * std::f64::consts::TAU * 0.8).sin() as f32 * 0.5 + 0.5;
    if lines
        .iter()
        .any(|line| matches!(line, ScriptLine::Advance { resting: true, .. }))
    {
        ui.ctx().request_repaint();
    }
    for (index, line) in lines.iter().enumerate() {
        let rect = match line {
            ScriptLine::Slide {
                number,
                name,
                state,
            } => {
                if index > 0 {
                    ui.add_space(size * 0.9);
                }
                let color = match state {
                    PartState::Now => palette::ACCENT,
                    PartState::Said => palette::TEXT_FAINT,
                    PartState::Ahead => palette::TEXT_MUTED,
                };
                ui.label(
                    egui::RichText::new(format!("{number} · {}", name.to_uppercase()))
                        .size(small)
                        .strong()
                        .color(color),
                )
                .rect
            }
            ScriptLine::Text { text, state } => {
                let response = egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 14,
                        right: 0,
                        top: 2,
                        bottom: 2,
                    })
                    .show(ui, |ui| match (state, read) {
                        (PartState::Now, Some(read)) => {
                            let (byte, character) =
                                split_after_words(text, read.max(0.0).floor() as usize);
                            still_reading |= byte < text.len();
                            let format = |color| egui::TextFormat {
                                font_id: egui::FontId::proportional(size),
                                color,
                                line_height: Some(size * 1.4),
                                ..Default::default()
                            };
                            let mut job = egui::text::LayoutJob::default();
                            job.append(&text[..byte], 0.0, format(palette::TEXT));
                            job.append(&text[byte..], 0.0, format(UNREAD));
                            job.wrap.max_width = ui.available_width();
                            let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
                            let label =
                                ui.add(egui::Label::new(egui::WidgetText::Galley(galley.clone())));
                            let point = galley
                                .pos_from_cursor(egui::text::CCursor::new(character))
                                .translate(label.rect.min.to_vec2());
                            reading.get_or_insert(point);
                        }
                        _ => {
                            let color = match state {
                                PartState::Now => palette::TEXT,
                                PartState::Said => palette::TEXT_FAINT,
                                PartState::Ahead => palette::TEXT_MUTED,
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(text)
                                        .size(size)
                                        .color(color)
                                        .line_height(Some(size * 1.4)),
                                )
                                .wrap(),
                            );
                        }
                    })
                    .response;
                if *state == PartState::Now {
                    let bar = egui::Rect::from_min_max(
                        response.rect.left_top(),
                        egui::pos2(response.rect.left() + 3.0, response.rect.bottom()),
                    );
                    ui.painter().rect_filled(bar, 0.0, palette::ACCENT);
                }
                response.rect
            }
            ScriptLine::Advance {
                label,
                state,
                resting,
                unmatched,
                reading: seconds,
            } => {
                let color = match (state, resting, unmatched) {
                    (_, _, true) => palette::DANGER,
                    (PartState::Now, true, _) => palette::STOP.gamma_multiply(0.55 + 0.45 * pulse),
                    (PartState::Now, false, _) => palette::STOP.gamma_multiply(0.7),
                    (PartState::Said, ..) => palette::TEXT_FAINT,
                    (PartState::Ahead, ..) => palette::STOP.gamma_multiply(0.45),
                };
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    let mut text = format!("▶ {} · {label}", words.advance);
                    if *unmatched {
                        text.push_str(&format!(" ({})", words.unmatched));
                    }
                    ui.label(egui::RichText::new(text).size(small).strong().color(color));
                    if let Some(seconds) = seconds
                        && *state != PartState::Said
                    {
                        ui.label(
                            egui::RichText::new(format!("≈{}", reading_span(*seconds)))
                                .size(small * 0.85)
                                .color(palette::TEXT_FAINT),
                        );
                    }
                })
                .response
                .rect
            }
            ScriptLine::Silent { .. } => {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new(words.silent)
                            .size(small)
                            .italics()
                            .color(palette::TEXT_FAINT),
                    );
                })
                .response
                .rect
            }
        };
        if reading.is_none()
            && line.state() == PartState::Now
            && !matches!(line, ScriptLine::Slide { .. })
        {
            reading = Some(rect);
        }
    }
    if still_reading {
        // The words light up as time goes by.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(60));
    }
    if follow && let Some(point) = reading {
        follow_reading(ui, point, content_top);
    }
}

/// The part being said, before it is read.
const UNREAD: egui::Color32 = egui::Color32::from_rgb(178, 182, 194);

/// Scroll so `point` sits in the top third, smoothly, whenever it moves
/// within the content (whose top is `content_top` on screen).
fn follow_reading(ui: &mut egui::Ui, point: egui::Rect, content_top: f32) {
    let key = ui.id().with("prompter-follow");
    let offset = point.top() - content_top;
    let moved = ui
        .data(|data| data.get_temp::<f32>(key))
        .is_none_or(|kept| (kept - offset).abs() > 2.0);
    if !moved {
        return;
    }
    ui.data_mut(|data| data.insert_temp(key, offset));
    let lead = ui.clip_rect().height() * 0.33;
    let target = egui::Rect::from_min_max(
        egui::pos2(point.left(), point.top() - lead),
        egui::pos2(point.right(), point.top() - lead + 1.0),
    );
    ui.scroll_to_rect_animation(
        target,
        Some(egui::Align::TOP),
        egui::style::ScrollAnimation::duration(0.35),
    );
}

/// The editor's prompter: the same as Presenter View's, in a window over
/// the preview. It shows while the speaker opens it (`Shift+T`) or presents
/// to record a live take or dictate the script.
pub(crate) fn editor_prompter_system(
    mut contexts: bevy_egui::EguiContexts,
    timeline: Option<Res<Timeline>>,
    mut settings: ResMut<PrompterSettings>,
    presentation: Res<crate::PresentationMode>,
    session: Res<crate::narration::NarrationSession>,
    hub: Option<Res<crate::project_hub::ProjectHubState>>,
) {
    let presenting = session.presenting();
    if !(settings.editor_open || presenting)
        || presentation.active
        || hub.is_some_and(|hub| hub.active)
    {
        return;
    }
    let Some(timeline) = timeline else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let screen = ctx.viewport_rect();
    let mut open = true;
    let mut window = egui::Window::new("Prompter")
        .id(egui::Id::new("editor_prompter"))
        .collapsible(false)
        .resizable(true)
        .default_size([520.0, 380.0])
        .default_pos(egui::pos2(screen.max.x - 560.0, screen.min.y + 60.0))
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(16, 17, 22))
                .stroke(egui::Stroke::new(1.0, palette::PANEL_STROKE))
                .inner_margin(egui::Margin::same(14)),
        );
    // While presenting for a capture, it stays open.
    if !presenting {
        window = window.open(&mut open);
    }
    window.show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("PROMPTER")
                    .size(11.5)
                    .strong()
                    .color(palette::TEXT_MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                prompter_controls(ui, &mut settings, &PrompterWords::ENGLISH);
            });
        });
        ui.add_space(6.0);
        let position = ScriptPosition::of(&timeline);
        let lines = script_lines(
            &timeline.segments,
            0..timeline.segments.len(),
            position,
            true,
        );
        let read = words_read(
            ui,
            egui::Id::new("editor_prompter"),
            position,
            settings.words_per_minute,
        );
        egui::ScrollArea::vertical()
            .id_salt("editor_prompter_script")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                show_script(
                    ui,
                    &lines,
                    settings.size,
                    true,
                    Some(read),
                    &PrompterWords::ENGLISH,
                );
            });
    });
    if !open {
        settings.editor_open = false;
    }
}

/// `seconds` as `0:12`.
fn reading_span(seconds: f64) -> String {
    let seconds = seconds.round() as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_timeline::timeline::SegmentStop;

    fn slide(
        id: u32,
        name: &str,
        notes: Option<&str>,
        stops: &[(f64, Option<&str>)],
    ) -> SegmentMetadata {
        SegmentMetadata {
            id,
            name: name.to_owned(),
            notes: notes.map(str::to_owned),
            target: None,
            start_time: stops.first().map_or(0.0, |(time, _)| time - 1.0),
            end_time: stops.last().map_or(1.0, |(time, _)| time + 1.0),
            stops: stops
                .iter()
                .map(|(time, name)| SegmentStop {
                    name: name.map(str::to_owned),
                    time: *time,
                    ambient: None,
                })
                .collect(),
        }
    }

    fn timeline(segments: Vec<SegmentMetadata>, time: f64, playing: bool) -> Timeline {
        let mut timeline = Timeline::new();
        timeline.cached_duration = segments.last().map_or(0.0, |segment| segment.end_time);
        timeline.set_segments(segments);
        timeline.current_time = time;
        timeline.is_playing = playing;
        timeline
    }

    #[test]
    fn the_part_being_said_follows_the_stops() {
        let deck = || {
            vec![
                slide(1, "Portada", Some("Hola"), &[(1.0, None)]),
                slide(2, "Datos", None, &[(4.0, Some("grafico")), (6.0, None)]),
            ]
        };
        let at = |time: f64, playing: bool| ScriptPosition::of(&timeline(deck(), time, playing));
        // Entering a slide, before its first stop.
        assert_eq!(
            at(3.5, true),
            Some(ScriptPosition {
                slide: 1,
                part: 0,
                resting: false
            })
        );
        // Waiting at its first stop: say part 0, then advance.
        assert_eq!(
            at(4.0, false),
            Some(ScriptPosition {
                slide: 1,
                part: 0,
                resting: true
            })
        );
        // Advanced past it: part 1 begins while the slide plays on.
        assert_eq!(
            at(5.0, true),
            Some(ScriptPosition {
                slide: 1,
                part: 1,
                resting: false
            })
        );
        // Paused by hand between stops is not resting at one.
        assert_eq!(
            at(5.0, false),
            Some(ScriptPosition {
                slide: 1,
                part: 1,
                resting: false
            })
        );
    }

    #[test]
    fn lines_cut_the_notes_at_their_marks_and_add_unmarked_stops() {
        let segments = vec![
            slide(
                1,
                "Datos",
                Some("Miren el gráfico.\n\n[stop]\n\nCrece.\n\n[stop extra]"),
                &[(4.0, Some("grafico"))],
            ),
            slide(2, "Cierre", None, &[(8.0, None), (9.0, Some("fin"))]),
        ];
        let position = Some(ScriptPosition {
            slide: 0,
            part: 1,
            resting: false,
        });
        let lines = script_lines(&segments, 0..2, position, true);
        let advances: Vec<(&str, PartState, bool)> = lines
            .iter()
            .filter_map(|line| match line {
                ScriptLine::Advance {
                    label,
                    state,
                    unmatched,
                    ..
                } => Some((label.as_str(), *state, *unmatched)),
                _ => None,
            })
            .collect();
        assert_eq!(
            advances,
            [
                // The stop's own name wins over a bare mark.
                ("grafico", PartState::Said, false),
                // A mark past the slide's stops is flagged.
                ("extra", PartState::Now, true),
                ("Step 1", PartState::Ahead, false),
                ("fin", PartState::Ahead, false),
            ]
        );
        assert!(lines.contains(&ScriptLine::Text {
            text: "Crece.".into(),
            state: PartState::Now,
        }));
        assert!(lines.contains(&ScriptLine::Silent {
            state: PartState::Ahead
        }));
        assert!(matches!(
            lines[0],
            ScriptLine::Slide {
                number: 1,
                state: PartState::Now,
                ..
            }
        ));
        // Only the current slide, without headings, for its notes.
        let notes = script_lines(&segments, 1..2, position, false);
        assert!(
            !notes
                .iter()
                .any(|line| matches!(line, ScriptLine::Slide { .. }))
        );
        assert_eq!(notes.len(), 3);
    }

    #[test]
    fn coverage_counts_the_parts_said_before_each_stop() {
        let stops = [(1.0, None), (2.0, None)];
        assert_eq!(
            script_coverage(&slide(1, "a", None, &stops)),
            ScriptCoverage::Pending
        );
        assert_eq!(
            script_coverage(&slide(1, "a", Some("Uno.\n[stop]"), &stops)),
            ScriptCoverage::Partial
        );
        assert_eq!(
            script_coverage(&slide(1, "a", Some("[stop]\nDos.\n[stop]"), &stops)),
            ScriptCoverage::Partial
        );
        assert_eq!(
            script_coverage(&slide(1, "a", Some("Uno.\n[stop]\nDos.\n[stop]"), &stops)),
            ScriptCoverage::Complete
        );
        // Without stops, any text covers the slide.
        assert_eq!(
            script_coverage(&slide(1, "a", Some("Hola."), &[])),
            ScriptCoverage::Complete
        );
        assert_eq!(
            script_coverage(&slide(1, "a", Some("  "), &[])),
            ScriptCoverage::Pending
        );
    }

    #[test]
    fn the_text_splits_after_the_words_already_read() {
        let text = "Miren  el gráfico,\nde ventas.";
        assert_eq!(split_after_words(text, 0), (0, 0));
        assert_eq!(split_after_words(text, 1), (5, 5));
        // Byte and character indices part ways after the accent.
        assert_eq!(split_after_words(text, 3), (19, 18));
        assert_eq!(split_after_words(text, 4), (22, 21));
        assert_eq!(
            split_after_words(text, 9),
            (text.len(), text.chars().count())
        );
        let mut settings = PrompterSettings::default();
        settings.adjust_pace(500.0);
        settings.adjust_size(-100.0);
        assert_eq!(
            (settings.words_per_minute, settings.size),
            (PACE_RANGE.1, SIZE_RANGE.0)
        );
    }

    #[test]
    fn reading_time_follows_the_narration_pace() {
        assert_eq!(reading_seconds(""), None);
        assert_eq!(reading_seconds("uno dos tres cuatro cinco"), Some(2.0));
        assert_eq!(reading_span(72.4), "1:12");
    }
}
