//! The speaker's script as a teleprompter: each slide's notes cut at their
//! stop marks (see `gaanim_timeline::notes`), and the part being said shown
//! as a caption on an opaque band, lit word by word as the speaker should
//! be saying it, with a cue where they advance. The reading follows the
//! timeline: the part the playhead crossed, plus the time spent resting at
//! the stop that ends it, read at the pace the part took when it was
//! recorded, or the speaker's measured pace. Presenter View shows the
//! caption in its speaker column, and the editor over the preview, where
//! recordings and dictations show their status on it too.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_timeline::notes::{NotesPart, split_notes};
use gaanim_timeline::timeline::{SegmentMetadata, Timeline};

use super::pace::TalkTiming;
use crate::ui_kit::{palette, small_button};

/// Text size of the prompter, the same in Presenter View and in the
/// editor, and whether the editor shows its caption.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub(crate) struct PrompterSettings {
    pub(crate) size: f32,
    pub(crate) editor_open: bool,
}

impl Default for PrompterSettings {
    fn default() -> Self {
        Self {
            size: 28.0,
            editor_open: false,
        }
    }
}

const SIZE_RANGE: (f32, f32) = (16.0, 56.0);

impl PrompterSettings {
    pub(crate) fn adjust_size(&mut self, delta: f32) {
        self.size = (self.size + delta).clamp(SIZE_RANGE.0, SIZE_RANGE.1);
    }

    pub(crate) fn can_grow(&self) -> bool {
        self.size < SIZE_RANGE.1
    }

    pub(crate) fn can_shrink(&self) -> bool {
        self.size > SIZE_RANGE.0
    }
}

/// The prompter's text size buttons.
pub(crate) fn size_controls(ui: &mut egui::Ui, settings: &mut PrompterSettings) {
    ui.spacing_mut().item_spacing.x = 4.0;
    if small_button(ui, "A+", settings.can_grow())
        .on_hover_text("Larger text")
        .clicked()
    {
        settings.adjust_size(2.0);
    }
    if small_button(ui, "A−", settings.can_shrink())
        .on_hover_text("Smaller text")
        .clicked()
    {
        settings.adjust_size(-2.0);
    }
}

/// What the prompter reads at: the talk as last recorded in the editor and
/// the speaker's pace, measured from their dictations.
#[derive(Resource, Debug, Default)]
pub(crate) struct PrompterPace {
    /// The file `timing` belongs to.
    pub(crate) source: Option<PathBuf>,
    pub(crate) timing: TalkTiming,
    /// Words per minute measured from earlier dictations.
    pub(crate) measured: Option<f64>,
    /// Words per minute of the dictation running now, once it can tell.
    pub(crate) live: Option<f64>,
    /// `measured` was read from the speaker's settings.
    pub(crate) loaded: bool,
}

/// The pace of reading aloud when nothing measured it.
pub(crate) const DEFAULT_WORDS_PER_MINUTE: f64 = 150.0;
/// Paces outside this range, in words per minute, are not reading.
pub(crate) const PACE_RANGE: (f64, f64) = (80.0, 240.0);

/// Where a reading pace comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaceSource {
    /// The time the part took when the talk was recorded.
    Recorded,
    /// The dictation running now.
    Live,
    /// The speaker's earlier dictations.
    Measured,
    Default,
}

/// How fast a part is read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pace {
    pub(crate) words_per_minute: f64,
    pub(crate) source: PaceSource,
}

impl Pace {
    pub(crate) fn describe(&self) -> String {
        let source = match self.source {
            PaceSource::Recorded => "the time you recorded for this part",
            PaceSource::Live => "what you are dictating now",
            PaceSource::Measured => "your earlier dictations",
            PaceSource::Default => "the usual pace; dictate to measure yours",
        };
        format!("Reading at {:.0} wpm, from {source}", self.words_per_minute)
    }
}

/// The pace of a part of `words` words: from the `recorded` seconds it
/// took, else the speaker's pace measured `live` or from earlier
/// dictations, else the usual one.
pub(crate) fn pace_for(
    words: usize,
    recorded: Option<f64>,
    live: Option<f64>,
    measured: Option<f64>,
) -> Pace {
    let clamp = |wpm: f64| wpm.clamp(PACE_RANGE.0, PACE_RANGE.1);
    if let Some(seconds) = recorded.filter(|seconds| *seconds > 0.5)
        && words > 0
    {
        return Pace {
            words_per_minute: clamp(words as f64 * 60.0 / seconds),
            source: PaceSource::Recorded,
        };
    }
    match (live, measured) {
        (Some(live), _) => Pace {
            words_per_minute: clamp(live),
            source: PaceSource::Live,
        },
        (None, Some(measured)) => Pace {
            words_per_minute: clamp(measured),
            source: PaceSource::Measured,
        },
        (None, None) => Pace {
            words_per_minute: DEFAULT_WORDS_PER_MINUTE,
            source: PaceSource::Default,
        },
    }
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

    /// Whether every part of the slide is said or being said last: the
    /// slide rests at its last stop, or its stops are behind.
    pub(crate) fn finishes_slide(&self, segments: &[SegmentMetadata]) -> bool {
        let stops = segments
            .get(self.slide)
            .map_or(0, |segment| segment.stops.len());
        self.part >= stops || (self.resting && self.part + 1 == stops)
    }
}

/// The span of the timeline the part at `position` covers: from where it
/// begins to the stop that ends it, or the end of its slide.
pub(crate) fn part_span(timeline: &Timeline, position: ScriptPosition) -> (f64, f64) {
    let Some(segment) = timeline.segments.get(position.slide) else {
        return (timeline.current_time, timeline.current_time);
    };
    let start = match position.part.checked_sub(1) {
        Some(previous) => segment
            .stops
            .get(previous)
            .map_or(segment.end_time, |stop| stop.time),
        None => super::segment_entry_time(timeline, segment),
    };
    let end = segment
        .stops
        .get(position.part)
        .map_or(segment.end_time, |stop| stop.time);
    (start, end.max(start))
}

/// Seconds the speaker has spent on a part spanning `span`: the stretch of
/// it the playhead at `time` crossed, plus `rested` seconds resting at the
/// stop that ends it.
pub(crate) fn part_elapsed(span: (f64, f64), time: f64, rested: f64) -> f64 {
    (time - span.0).clamp(0.0, span.1 - span.0) + rested.max(0.0)
}

/// Seconds the presentation has rested at the stop of `position`, on a
/// clock kept in `ctx`'s memory under `id`.
fn rested(ctx: &egui::Context, id: egui::Id, position: ScriptPosition) -> f64 {
    let now = ctx.input(|input| input.time);
    let key = id.with("prompter-rest");
    let resting = position.resting.then_some((position.slide, position.part));
    let since = match ctx.data(|data| data.get_temp::<(Option<(usize, usize)>, f64)>(key)) {
        Some((kept, since)) if kept == resting => since,
        _ => now,
    };
    ctx.data_mut(|data| data.insert_temp(key, (resting, since)));
    if resting.is_some() { now - since } else { 0.0 }
}

/// The part the speaker is saying.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PartView {
    pub(crate) slide_name: String,
    /// What the speaker says, without its mark.
    pub(crate) text: String,
    /// Where they advance once it is said: the stop's name, else the next
    /// slide's.
    pub(crate) cue: String,
}

impl PartView {
    pub(crate) fn words(&self) -> usize {
        gaanim_media::narration::text_words(&self.text).len()
    }
}

/// The part of `segments`' script at `position`.
pub(crate) fn part_view(
    segments: &[SegmentMetadata],
    position: ScriptPosition,
) -> Option<PartView> {
    let segment = segments.get(position.slide)?;
    let parts = segment
        .notes
        .as_deref()
        .map(split_notes)
        .unwrap_or_default();
    let notes_part = parts.get(position.part);
    let cue = match segment.stops.get(position.part) {
        Some(stop) => stop
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .or(notes_part.and_then(|part| part.mark.as_ref()?.name.as_deref()))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Step {}", position.part + 1)),
        None => segments[position.slide + 1..]
            .iter()
            .find(|next| next.name != IMPLICIT_SEGMENT)
            .map_or_else(|| "End".to_owned(), |next| next.name.clone()),
    };
    Some(PartView {
        slide_name: segment.name.clone(),
        text: notes_part.map(|part| part.text.clone()).unwrap_or_default(),
        cue,
    })
}

/// How far through the part at `position` the speaker should be, in words
/// of `part`, and the pace that says so.
pub(crate) fn reading(
    ctx: &egui::Context,
    id: egui::Id,
    timeline: &Timeline,
    position: ScriptPosition,
    part: &PartView,
    pace: &PrompterPace,
) -> (f64, Pace) {
    let recorded = pace
        .timing
        .slide(&part.slide_name)
        .and_then(|slide| slide.parts.get(position.part).copied());
    let pace = pace_for(part.words(), recorded, pace.live, pace.measured);
    let elapsed = part_elapsed(
        part_span(timeline, position),
        timeline.current_time,
        rested(ctx, id, position),
    );
    (elapsed * pace.words_per_minute / 60.0, pace)
}

/// What a caption shows.
#[derive(Debug, Clone)]
pub(crate) enum CaptionText {
    /// The part of the script being said, lit up to `read` words.
    Script { text: String, read: f64 },
    /// What has been transcribed of the part so far; `listening` while
    /// the speaker talks.
    Live { text: String, listening: bool },
    /// Nothing to read, and why.
    Hint(String),
    /// Text already laid out, such as a voiceover's with its markers.
    Job(egui::text::LayoutJob),
}

/// Where the speaker advances, on the right of the caption's status line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cue {
    pub(crate) label: String,
    /// The slide waits there now.
    pub(crate) resting: bool,
    /// The part is said: it is time to advance.
    pub(crate) due: bool,
}

/// Words of the part already said, dimmed so the next ones stand out.
const SAID: egui::Color32 = egui::Color32::from_rgb(118, 122, 134);
/// The caption's band, opaque so the text reads over any slide.
pub(crate) const BAND: egui::Color32 = egui::Color32::from_rgb(7, 7, 9);

/// The caption's text at `size` points, `lines` rows at most: the rows
/// around the reading point, sliding up as it moves on. Its scroll lives in
/// `ui`'s memory under `id`, one per part.
pub(crate) fn caption_text(
    ui: &mut egui::Ui,
    id: egui::Id,
    text: &CaptionText,
    size: f32,
    lines: usize,
) {
    let width = ui.available_width();
    let line_height = (size * 1.3).round();
    let format = |color, size: f32, italics: bool| egui::TextFormat {
        font_id: egui::FontId::proportional(size),
        color,
        italics,
        line_height: Some(line_height),
        ..Default::default()
    };
    // The character the reading point is at, or the end for live text.
    let (mut job, focus) = match text {
        CaptionText::Script { text, read } => {
            let (byte, character) = split_after_words(text, read.max(0.0).floor() as usize);
            let mut job = egui::text::LayoutJob::default();
            job.append(&text[..byte], 0.0, format(SAID, size, false));
            job.append(&text[byte..], 0.0, format(palette::TEXT, size, false));
            (job, Some(character))
        }
        CaptionText::Live { text, listening } => {
            let mut job = egui::text::LayoutJob::default();
            job.append(text, 0.0, format(palette::TEXT, size, false));
            if *listening {
                let dots = if text.is_empty() { "…" } else { " …" };
                job.append(dots, 0.0, format(palette::TEXT_MUTED, size, false));
            }
            (job, Some(usize::MAX))
        }
        CaptionText::Hint(hint) => {
            let mut job = egui::text::LayoutJob::default();
            job.append(hint, 0.0, format(palette::TEXT_MUTED, size * 0.75, true));
            (job, None)
        }
        CaptionText::Job(job) => (job.clone(), None),
    };
    job.wrap.max_width = width;
    job.halign = egui::Align::Center;
    let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
    let rows = galley.rows.len().max(1);
    let shown = rows.min(lines.max(1));
    let row_of = |y: f32| {
        galley
            .rows
            .iter()
            .position(|row| y < row.rect().bottom())
            .unwrap_or(rows - 1)
    };
    let focus_row = match focus {
        Some(usize::MAX) => rows - 1,
        Some(character) => row_of(
            galley
                .pos_from_cursor(egui::text::CCursor::new(character))
                .center()
                .y,
        ),
        None => 0,
    };
    let first = focus_row.min(rows - shown);
    let target = galley.rows.get(first).map_or(0.0, |row| row.rect().top());
    let offset = ui.ctx().animate_value_with_time(id, target, 0.25);
    let height = galley
        .rows
        .get(first + shown - 1)
        .map_or(line_height, |row| row.rect().bottom() - target)
        .max(line_height);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter().with_clip_rect(rect).galley(
        egui::pos2(rect.center().x, rect.top() - offset),
        galley,
        palette::TEXT,
    );
}

/// The editor's caption: an opaque band centered over the preview, its
/// bottom `gap` points above the window's, outlined in `outline` when
/// something needs the speaker's attention. Right-clicking it opens `menu`.
pub(crate) fn show_band(
    ctx: &egui::Context,
    gap: f32,
    outline: Option<egui::Color32>,
    contents: impl FnOnce(&mut egui::Ui),
    menu: impl FnOnce(&mut egui::Ui),
) {
    let screen = ctx.viewport_rect();
    let margin = egui::Margin::symmetric(24, 12);
    let width = (screen.width() - 48.0).clamp(280.0, 980.0) - margin.sum().x;
    egui::Area::new(egui::Id::new("editor_caption"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -gap))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let band = egui::Frame::new()
                .fill(BAND)
                .inner_margin(margin)
                .stroke(outline.map_or(egui::Stroke::NONE, |color| egui::Stroke::new(2.0, color)))
                .show(ui, |ui| {
                    ui.set_width(width);
                    contents(ui);
                });
            band.response
                .interact(egui::Sense::click())
                .context_menu(menu);
        });
}

/// The cue to advance, which pulses once the part is said and the slide
/// waits for the speaker.
pub(crate) fn cue_label(ui: &mut egui::Ui, cue: &Cue, size: f32) {
    let pulse =
        (ui.input(|input| input.time) * std::f64::consts::TAU * 0.8).sin() as f32 * 0.5 + 0.5;
    let color = match (cue.resting, cue.due) {
        (true, true) => {
            ui.ctx().request_repaint();
            palette::STOP.gamma_multiply(0.55 + 0.45 * pulse)
        }
        (true, false) => palette::STOP.gamma_multiply(0.7),
        (false, _) => palette::TEXT_FAINT,
    };
    // Laid out right to left: the label, then the triangle before it.
    ui.label(
        egui::RichText::new(&cue.label)
            .size(size)
            .strong()
            .color(color),
    );
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(size * 0.8), egui::Sense::hover());
    crate::ui_kit::paint_icon(ui.painter(), rect, crate::ui_kit::Icon::Play, color);
}

/// The script's caption at the playhead.
pub(crate) struct ScriptCaption {
    pub(crate) text: CaptionText,
    pub(crate) cue: Cue,
    pub(crate) part: PartView,
    pub(crate) position: ScriptPosition,
    pub(crate) pace: Pace,
    /// The caption's own memory: one per part.
    pub(crate) id: egui::Id,
}

/// The caption for the script at `timeline`'s playhead, or `None` outside
/// the slides.
pub(crate) fn script_caption(
    ctx: &egui::Context,
    id: egui::Id,
    timeline: &Timeline,
    pace: &PrompterPace,
) -> Option<ScriptCaption> {
    let position = ScriptPosition::of(timeline)?;
    let part = part_view(&timeline.segments, position)?;
    let (read, pace) = reading(ctx, id, timeline, position, &part, pace);
    let words = part.words();
    let cue = Cue {
        label: part.cue.clone(),
        resting: position.resting,
        due: read >= words as f64,
    };
    let text = if part.text.trim().is_empty() {
        CaptionText::Hint(format!("No script for this part of {}", part.slide_name))
    } else {
        if position.resting && read < words as f64 {
            // Resting at the stop, the words light up as time goes by;
            // playing, the playhead moves them.
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
        CaptionText::Script {
            text: part.text.clone(),
            read,
        }
    };
    Some(ScriptCaption {
        text,
        cue,
        part,
        position,
        pace,
        id: id.with((position.slide, position.part)),
    })
}

/// How far the speaker is through a part of the script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartState {
    Said,
    Now,
    Ahead,
}

/// A line of a slide's notes.
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

/// The words a slide's notes write.
pub(crate) struct PrompterWords {
    pub(crate) advance: &'static str,
    pub(crate) silent: &'static str,
    pub(crate) unmatched: &'static str,
}

impl PrompterWords {
    pub(crate) const ENGLISH: Self = Self {
        advance: "ADVANCE",
        silent: "No notes for this slide",
        unmatched: "no such stop",
    };
}

/// Draw `lines` with their text at `size` points, the part being said
/// marked; with `follow`, the view scrolls smoothly to keep it in the top
/// third.
pub(crate) fn show_script(
    ui: &mut egui::Ui,
    lines: &[ScriptLine],
    size: f32,
    follow: bool,
    words: &PrompterWords,
) {
    let small = (size * 0.6).max(12.0);
    let content_top = ui.min_rect().top();
    let mut reading: Option<egui::Rect> = None;
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
                let color = match state {
                    PartState::Now => palette::TEXT,
                    PartState::Said => palette::TEXT_FAINT,
                    PartState::Ahead => palette::TEXT_MUTED,
                };
                let response = egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 14,
                        right: 0,
                        top: 2,
                        bottom: 2,
                    })
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text)
                                    .size(size)
                                    .color(color)
                                    .line_height(Some(size * 1.4)),
                            )
                            .wrap(),
                        );
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
    if follow && let Some(point) = reading {
        follow_reading(ui, point, content_top);
    }
}

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
        // Resting at the last stop finishes the slide; before it, not yet.
        let segments = deck();
        assert!(at(6.0, false).unwrap().finishes_slide(&segments));
        assert!(!at(5.0, true).unwrap().finishes_slide(&segments));
        assert!(at(6.5, true).unwrap().finishes_slide(&segments));
    }

    #[test]
    fn the_reading_follows_the_playhead_and_the_rest_at_the_stop() {
        let deck = timeline(
            vec![
                slide(1, "Portada", None, &[(1.0, None)]),
                slide(2, "Datos", None, &[(4.0, None), (6.0, None)]),
            ],
            5.0,
            true,
        );
        let at = |part, resting| ScriptPosition {
            slide: 1,
            part,
            resting,
        };
        // The first part runs from where the slide is entered to its stop.
        assert_eq!(part_span(&deck, at(0, false)), (3.0, 4.0));
        assert_eq!(part_span(&deck, at(1, false)), (4.0, 6.0));
        // After the last stop, to the end of the slide.
        assert_eq!(part_span(&deck, at(2, false)), (6.0, 7.0));
        // Halfway through its animation, half its span has gone by.
        assert_eq!(part_elapsed((4.0, 6.0), 5.0, 0.0), 1.0);
        // Seeking back goes back with it.
        assert_eq!(part_elapsed((4.0, 6.0), 3.0, 0.0), 0.0);
        // Resting at its stop, the rest adds up.
        assert_eq!(part_elapsed((4.0, 6.0), 6.0, 3.5), 5.5);
    }

    #[test]
    fn the_pace_comes_from_the_recording_then_the_speaker_then_the_usual_one() {
        // 20 words in the 10 seconds the part took when recorded.
        let recorded = pace_for(20, Some(10.0), Some(130.0), Some(140.0));
        assert_eq!(
            (recorded.words_per_minute, recorded.source),
            (120.0, PaceSource::Recorded)
        );
        // Too fast to be reading: clamped.
        assert_eq!(
            pace_for(100, Some(10.0), None, None).words_per_minute,
            PACE_RANGE.1
        );
        // Without a recording, the dictation running now, then earlier ones.
        assert_eq!(
            pace_for(20, None, Some(130.0), Some(140.0)).source,
            PaceSource::Live
        );
        assert_eq!(
            pace_for(20, Some(0.1), None, Some(140.0)).source,
            PaceSource::Measured
        );
        assert_eq!(
            pace_for(0, Some(10.0), None, None),
            Pace {
                words_per_minute: DEFAULT_WORDS_PER_MINUTE,
                source: PaceSource::Default
            }
        );
    }

    #[test]
    fn a_part_cues_its_stop_or_the_next_slide() {
        let segments = vec![
            slide(
                1,
                "Datos",
                Some("Miren el gráfico.\n\n[stop barras]\n\nCrece."),
                &[(4.0, None), (6.0, Some("fin"))],
            ),
            slide(2, IMPLICIT_SEGMENT, None, &[]),
            slide(3, "Cierre", None, &[]),
        ];
        let view = |slide, part| {
            part_view(
                &segments,
                ScriptPosition {
                    slide,
                    part,
                    resting: false,
                },
            )
            .unwrap()
        };
        // A bare stop takes the mark's name; a named stop keeps its own.
        assert_eq!(view(0, 0).cue, "barras");
        assert_eq!(view(0, 0).text, "Miren el gráfico.");
        assert_eq!(view(0, 0).words(), 3);
        assert_eq!(view(0, 1).cue, "fin");
        assert_eq!(view(0, 1).text, "Crece.");
        // Past the last stop, the next named slide.
        assert_eq!(view(0, 2).cue, "Cierre");
        assert_eq!(view(0, 2).text, "");
        assert_eq!(view(2, 0).cue, "End");
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
        settings.adjust_size(-100.0);
        assert_eq!(settings.size, SIZE_RANGE.0);
        assert!(!settings.can_shrink() && settings.can_grow());
    }

    #[test]
    fn reading_time_follows_the_narration_pace() {
        assert_eq!(reading_seconds(""), None);
        assert_eq!(reading_seconds("uno dos tres cuatro cinco"), Some(2.0));
        assert_eq!(reading_span(72.4), "1:12");
    }
}
