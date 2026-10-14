//! The speaker's script as a teleprompter: each slide's notes cut at their
//! stop marks (see `gaanim_timeline::notes`), the part being said
//! highlighted, and a cue wherever the speaker advances. Presenter View
//! shows it, as the current slide's notes or as the whole talk, and so does
//! the teleprompter of a live take.

use bevy_egui::egui;
use gaanim_timeline::notes::{NotesPart, split_notes};
use gaanim_timeline::timeline::{SegmentMetadata, Timeline};

use crate::ui_kit::palette;

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

/// Seconds `text` takes to read aloud at the narration's estimated pace.
fn reading_seconds(text: &str) -> Option<f64> {
    let words = gaanim_media::narration::text_words(text).len();
    (words > 0).then(|| words as f64 / gaanim_media::narration::ESTIMATED_WORDS_PER_SECOND)
}

/// The words the teleprompter writes, in the language of the screen it is on.
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
    pub(crate) const SPANISH: Self = Self {
        advance: "AVANZA",
        silent: "Sin guion para esta diapositiva",
        unmatched: "no hay ese stop",
    };
}

/// Draw `lines` with their text at `size` points. With `follow`, scroll so
/// the part being said sits in the top third, as on a teleprompter.
pub(crate) fn show_script(
    ui: &mut egui::Ui,
    lines: &[ScriptLine],
    size: f32,
    follow: bool,
    words: &PrompterWords,
) {
    let small = (size * 0.6).max(12.0);
    let mut now: Option<egui::Rect> = None;
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
                reading,
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
                    if let Some(seconds) = reading
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
        if now.is_none()
            && line.state() == PartState::Now
            && !matches!(line, ScriptLine::Slide { .. })
        {
            now = Some(rect);
        }
    }
    if follow && let Some(rect) = now {
        let lead = ui.clip_rect().height() * 0.3;
        let target = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top() - lead),
            egui::pos2(rect.right(), rect.top() - lead + 1.0),
        );
        ui.scroll_to_rect(target, Some(egui::Align::TOP));
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
    fn reading_time_follows_the_narration_pace() {
        assert_eq!(reading_seconds(""), None);
        assert_eq!(reading_seconds("uno dos tres cuatro cinco"), Some(2.0));
        assert_eq!(reading_span(72.4), "1:12");
    }
}
