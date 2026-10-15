//! The time a talk plans and the time it takes: what Presenter View counts
//! down, how far ahead or behind the plan the speaker is, and the rehearsals
//! it keeps to compare with.
//!
//! A plan comes from `--present --duration` (the whole talk) and from the
//! segments' targets (`scene.segment(..., target=90)`). Every session that
//! presents for a while is kept as a rehearsal: the time spent on each
//! segment, which the next session shows next to the current one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::prelude::Resource;
use gaanim_timeline::timeline::SegmentMetadata;
use serde::{Deserialize, Serialize};

/// The length planned for the whole talk. From the strongest: the one set
/// in Presenter View during this run, `--present --duration`, the script's
/// (`scene.slides.target`), and the one last set in Presenter View for the
/// presented file.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct TalkDuration {
    /// `--present --duration`.
    pub given: Option<Duration>,
    /// Set (or, as `Some(None)`, cleared) in Presenter View during this run.
    pub(crate) chosen: Option<Option<Duration>>,
    /// Last set in Presenter View for the presented file.
    pub(crate) remembered: Option<Duration>,
}

/// Where the planned length comes from, for Presenter View to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LengthSource {
    Chosen,
    Given,
    Script,
    Remembered,
}

impl LengthSource {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::Chosen => "Set here",
            Self::Given => "From --duration",
            Self::Script => "From the script (scene.slides.target)",
            Self::Remembered => "Set here last time",
        }
    }
}

impl TalkDuration {
    /// The length given with `--present --duration`, if any.
    pub fn given(duration: Option<Duration>) -> Self {
        Self {
            given: duration,
            ..Self::default()
        }
    }

    /// The length that applies, with the script's `script`, and where it
    /// comes from.
    pub(crate) fn current(&self, script: Option<Duration>) -> Option<(Duration, LengthSource)> {
        match self.chosen {
            Some(chosen) => chosen.map(|duration| (duration, LengthSource::Chosen)),
            None => self
                .given
                .map(|duration| (duration, LengthSource::Given))
                .or(script.map(|duration| (duration, LengthSource::Script)))
                .or(self
                    .remembered
                    .map(|duration| (duration, LengthSource::Remembered))),
        }
    }
}

/// What a talk's length is written as, for messages.
const EXPECTED_LENGTH: &str = "a length such as 20m, 1h30m, 90s or 20:00";

/// A talk's length: hours, minutes and seconds with their units (`20m`,
/// `90s`, `1h30m`), a clock (`20:00`, `1:15:00`) or a number of minutes.
/// The error says what was expected.
pub fn parse_talk_duration(text: &str) -> Result<Duration, &'static str> {
    let value = text.trim().to_lowercase();
    let number = |text: &str| text.parse::<f64>().map_err(|_| EXPECTED_LENGTH);
    let seconds = if value.contains(':') {
        let parts = value
            .split(':')
            .map(number)
            .collect::<Result<Vec<_>, _>>()?;
        if parts.len() > 3 {
            return Err(EXPECTED_LENGTH);
        }
        parts.iter().fold(0.0, |total, part| total * 60.0 + part)
    } else if value.ends_with(['h', 'm', 's']) {
        let mut total = 0.0;
        let mut digits = String::new();
        for c in value.chars().filter(|c| !c.is_whitespace()) {
            let unit = match c {
                'h' => 3600.0,
                'm' => 60.0,
                's' => 1.0,
                _ => {
                    digits.push(c);
                    continue;
                }
            };
            total += number(&digits)? * unit;
            digits.clear();
        }
        total
    } else {
        number(&value)? * 60.0
    };
    if !(seconds.is_finite() && seconds > 0.0) {
        return Err(EXPECTED_LENGTH);
    }
    Ok(Duration::from_secs_f64(seconds))
}

/// Seconds the talk is planned to last: `duration` when given, otherwise
/// the sum of the segments' targets when every segment has one.
pub(crate) fn planned_total(
    segments: &[SegmentMetadata],
    duration: Option<Duration>,
) -> Option<f64> {
    if let Some(duration) = duration {
        return Some(duration.as_secs_f64());
    }
    if segments.is_empty() {
        return None;
    }
    segments.iter().map(|segment| segment.target).sum()
}

/// Seconds behind (positive) or ahead of (negative) the plan, over the
/// segments with a target up to the one on screen, `current`: a segment
/// already left counts what it took against its target, the current one
/// only once it runs over. `None` when no segment has a target.
pub(crate) fn drift(
    segments: &[SegmentMetadata],
    spent: &HashMap<u32, Duration>,
    current: Option<usize>,
) -> Option<f64> {
    if segments.iter().all(|segment| segment.target.is_none()) {
        return None;
    }
    let Some(current) = current else {
        return Some(0.0);
    };
    let took =
        |segment: &SegmentMetadata| spent.get(&segment.id).map_or(0.0, Duration::as_secs_f64);
    Some(
        segments
            .iter()
            .take(current + 1)
            .enumerate()
            .filter_map(|(index, segment)| {
                let target = segment.target?;
                let over = took(segment) - target;
                Some(if index < current { over } else { over.max(0.0) })
            })
            .sum(),
    )
}

/// How the plan is going, for its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaceTone {
    /// On time or ahead.
    OnTime,
    /// Behind, by up to a tenth of the plan (at least 30 seconds).
    Behind,
    /// Further behind, or past the planned end.
    Late,
}

/// The tone of `behind` seconds of drift in a plan of `planned` seconds.
pub(crate) fn tone(behind: f64, planned: f64) -> PaceTone {
    let tolerance = (planned * 0.1).max(30.0);
    if behind <= 0.0 {
        PaceTone::OnTime
    } else if behind <= tolerance {
        PaceTone::Behind
    } else {
        PaceTone::Late
    }
}

/// The tone of a countdown with `left` of `planned` seconds.
pub(crate) fn countdown_tone(left: f64, planned: f64) -> PaceTone {
    if left < 0.0 {
        PaceTone::Late
    } else if left <= planned * 0.1 {
        PaceTone::Behind
    } else {
        PaceTone::OnTime
    }
}

/// `seconds` as `m:ss` (or `h:mm:ss`), signed when `signed`: `+1:05`
/// behind, `-0:20` ahead.
pub(crate) fn format_span(seconds: f64, signed: bool) -> String {
    let whole = seconds.abs().round() as u64;
    let sign = match (signed, seconds.round() as i64) {
        (true, value) if value > 0 => "+",
        (true, value) if value < 0 => "-",
        (false, value) if value < 0 => "-",
        _ => "",
    };
    if whole >= 3600 {
        format!(
            "{sign}{}:{:02}:{:02}",
            whole / 3600,
            (whole / 60) % 60,
            whole % 60
        )
    } else {
        format!("{sign}{}:{:02}", whole / 60, whole % 60)
    }
}

/// Sessions shorter than this are not kept as rehearsals.
const SHORTEST_REHEARSAL: Duration = Duration::from_secs(30);
/// Rehearsals kept per presented file.
const KEPT_REHEARSALS: usize = 20;

/// One session presenting a file: when it started and the seconds spent on
/// each segment, by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Rehearsal {
    /// Seconds since the Unix epoch.
    pub started: u64,
    pub total: f64,
    pub segments: Vec<(String, f64)>,
}

impl Rehearsal {
    /// Seconds this rehearsal spent on the segment named `name`.
    pub(crate) fn spent_on(&self, name: &str) -> Option<f64> {
        self.segments
            .iter()
            .find(|(segment, _)| segment == name)
            .map(|(_, seconds)| *seconds)
    }
}

/// The time each slide took when the speaker last recorded it in the
/// editor, which they record again slide by slide, or stop by stop, to fit
/// the plan.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct TalkTiming {
    #[serde(default)]
    pub slides: Vec<SlideTiming>,
}

/// The time one slide took when it was last recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SlideTiming {
    pub name: String,
    /// Seconds from entering the slide to leaving it.
    pub seconds: f64,
    /// Seconds each part of its script took: the one before each stop,
    /// then the one after the last.
    pub parts: Vec<f64>,
    /// Seconds since the Unix epoch.
    pub recorded: u64,
}

impl TalkTiming {
    pub(crate) fn slide(&self, name: &str) -> Option<&SlideTiming> {
        self.slides.iter().find(|slide| slide.name == name)
    }

    /// Keep `slide`, replacing the time recorded for it before.
    pub(crate) fn record(&mut self, slide: SlideTiming) {
        match self.slides.iter_mut().find(|kept| kept.name == slide.name) {
            Some(kept) => *kept = slide,
            None => self.slides.push(slide),
        }
    }

    /// Replace the time part `part` of `slide` took, which changes the
    /// slide's. A slide without a time yet keeps none.
    pub(crate) fn record_part(&mut self, slide: &str, part: usize, seconds: f64, recorded: u64) {
        let Some(kept) = self.slides.iter_mut().find(|kept| kept.name == slide) else {
            return;
        };
        if kept.parts.len() <= part {
            kept.parts.resize(part + 1, 0.0);
        }
        kept.parts[part] = seconds;
        kept.seconds = kept.parts.iter().sum();
        kept.recorded = recorded;
    }
}

/// A slide a recording went through from entering it to leaving it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TimedSlide {
    pub name: String,
    pub seconds: f64,
    pub parts: Vec<f64>,
}

/// Where a recording was in the script: the recording second at which the
/// part being said changed, with the slide (by name) and part from then on,
/// or `None` outside any slide.
pub(crate) type ScriptTrack = [(f64, Option<(String, usize)>)];

/// The slides a recording that ended at `end` went through whole, with
/// the time each took and each of its parts. The first slide counts when
/// the recording `entered_first` from its start or came to it from outside
/// any slide; the last one when the speaker `finished_last` before ending
/// or left it.
pub(crate) fn timed_slides(
    track: &ScriptTrack,
    end: f64,
    entered_first: bool,
    finished_last: bool,
) -> Vec<TimedSlide> {
    let outside = |entry: Option<&(f64, Option<(String, usize)>)>| {
        entry.is_some_and(|(_, position)| position.is_none())
    };
    let entered_first = entered_first || outside(track.first());
    let finished_last = finished_last || outside(track.last());
    // Consecutive changes within one slide, each with how long it lasted.
    let mut runs: Vec<(String, Vec<(usize, f64)>)> = Vec::new();
    let mut breaks = 0;
    for (index, (time, position)) in track.iter().enumerate() {
        let until = track.get(index + 1).map_or(end, |(next, _)| *next);
        let lasted = (until - time).max(0.0);
        match position {
            Some((slide, part)) => match runs.last_mut() {
                Some((name, parts)) if name == slide && breaks == 0 => parts.push((*part, lasted)),
                _ => {
                    runs.push((slide.clone(), vec![(*part, lasted)]));
                    breaks = 0;
                }
            },
            None => breaks += 1,
        }
    }
    let count = runs.len();
    runs.into_iter()
        .enumerate()
        .filter(|(index, _)| (*index > 0 || entered_first) && (*index + 1 < count || finished_last))
        .map(|(_, (name, changes))| {
            let mut parts = Vec::new();
            for (part, lasted) in changes {
                if parts.len() <= part {
                    parts.resize(part + 1, 0.0);
                }
                parts[part] += lasted;
            }
            TimedSlide {
                seconds: parts.iter().sum(),
                name,
                parts,
            }
        })
        .collect()
}

/// The recording seconds a recording that ended at `end` spent on part
/// `part` of `slide`, once it went through it whole: from reaching it to
/// leaving it, or to `end` when the speaker `finished` it there.
pub(crate) fn timed_part(
    track: &ScriptTrack,
    slide: &str,
    part: usize,
    end: f64,
    finished: bool,
) -> Option<(f64, f64)> {
    let here = |entry: &(f64, Option<(String, usize)>)| {
        entry
            .1
            .as_ref()
            .is_some_and(|(name, index)| name == slide && *index == part)
    };
    let reached = track.iter().position(here)?;
    let until = match track[reached..].iter().position(|entry| !here(entry)) {
        Some(left) => track[reached + left].0,
        None if finished => end,
        None => return None,
    };
    Some((track[reached].0, until.max(track[reached].0)))
}

/// Where each part of the slides named in `slides` begins and ends in a
/// recording that followed `track` and ended at `end`, in recording
/// seconds: `(slide, part, start, end)`.
pub(crate) fn part_spans(
    track: &ScriptTrack,
    end: f64,
    slides: &[&str],
) -> Vec<(String, usize, f64, f64)> {
    track
        .iter()
        .enumerate()
        .filter_map(|(index, (start, place))| {
            let (slide, part) = place.as_ref()?;
            slides.contains(&slide.as_str()).then(|| {
                let until = track.get(index + 1).map_or(end, |(next, _)| *next);
                (slide.clone(), *part, *start, until.max(*start))
            })
        })
        .collect()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RehearsalLog {
    source: PathBuf,
    /// Seconds planned for the whole talk, set in Presenter View or with
    /// `--duration`, which the next session starts with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration: Option<f64>,
    rehearsals: Vec<Rehearsal>,
    /// The talk as last recorded in the editor, slide by slide.
    #[serde(default, skip_serializing_if = "is_untimed")]
    timing: TalkTiming,
}

fn is_untimed(timing: &TalkTiming) -> bool {
    timing.slides.is_empty()
}

/// The rehearsals of the file at `source`, oldest first.
pub(crate) struct Rehearsals {
    path: Option<PathBuf>,
    log: RehearsalLog,
}

impl Rehearsals {
    /// The rehearsals kept for `source` in Gaanim's data folder.
    pub(crate) fn load(source: &Path) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let path = gaanim_project::user_data_dir()
            .map(|dir| dir.join("rehearsals").join(log_name(source)));
        // The web has no data folder: its rehearsals last the session.
        #[cfg(target_arch = "wasm32")]
        let path: Option<PathBuf> = None;
        let log = path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| RehearsalLog {
                source: source.to_path_buf(),
                ..RehearsalLog::default()
            });
        Self { path, log }
    }

    /// Where the audio of each part of the talk at `source` is kept, as
    /// last recorded: `<slide>-<digest>-<part>.wav` in a folder named after
    /// its log. `None` without a data folder.
    pub(crate) fn clips_dir(source: &Path) -> Option<PathBuf> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let log = log_name(source);
            let folder = log.strip_suffix(".json").unwrap_or(&log).to_owned();
            gaanim_project::user_data_dir().map(|dir| dir.join("rehearsals").join(folder))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = source;
            None
        }
    }

    /// The practice runs kept, oldest first.
    pub(crate) fn all(&self) -> &[Rehearsal] {
        &self.log.rehearsals
    }

    /// The talk as last recorded in the editor.
    pub(crate) fn timing(&self) -> &TalkTiming {
        &self.log.timing
    }

    /// Change the recorded timing of the file at `source`, read afresh so
    /// a session presenting meanwhile keeps its rehearsals, and return it.
    pub(crate) fn update_timing(source: &Path, change: impl FnOnce(&mut TalkTiming)) -> TalkTiming {
        let mut rehearsals = Self::load(source);
        change(&mut rehearsals.log.timing);
        rehearsals.save();
        rehearsals.log.timing
    }

    /// The talk's length planned for this file last time.
    pub(crate) fn duration(&self) -> Option<Duration> {
        self.log
            .duration
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
            .map(Duration::from_secs_f64)
    }

    /// Remember `duration` as this file's planned length.
    pub(crate) fn plan(&mut self, duration: Option<Duration>) {
        let seconds = duration.map(|duration| duration.as_secs_f64());
        if self.log.duration != seconds {
            self.log.duration = seconds;
            self.save();
        }
    }

    /// The latest rehearsal before the session that started at `started`.
    pub(crate) fn previous(&self, started: u64) -> Option<&Rehearsal> {
        self.log
            .rehearsals
            .iter()
            .rev()
            .find(|rehearsal| rehearsal.started != started)
    }

    /// Keep `rehearsal`, replacing the one of its session, once it lasted
    /// long enough to count.
    pub(crate) fn record(&mut self, rehearsal: Rehearsal) {
        if Duration::from_secs_f64(rehearsal.total.max(0.0)) < SHORTEST_REHEARSAL {
            return;
        }
        let rehearsals = &mut self.log.rehearsals;
        match rehearsals
            .iter_mut()
            .find(|kept| kept.started == rehearsal.started)
        {
            Some(kept) => *kept = rehearsal,
            None => rehearsals.push(rehearsal),
        }
        let extra = rehearsals.len().saturating_sub(KEPT_REHEARSALS);
        rehearsals.drain(..extra);
        self.save();
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Ok(text) = serde_json::to_string_pretty(&self.log) {
            let _ = std::fs::create_dir_all(path.parent().unwrap_or(path));
            if let Err(error) = gaanim_media::narration::write_atomically(path, text.as_bytes()) {
                gaanim_core::console::warn(
                    "rehearsal",
                    format!("could not save the rehearsal: {error}"),
                );
            }
        }
    }
}

/// The log file of `source`: its name and a digest of its full path, so
/// two talks with the same name keep apart.
#[cfg(not(target_arch = "wasm32"))]
fn log_name(source: &Path) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(source.to_string_lossy().as_bytes());
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "talk".to_owned())
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let hex: String = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{stem}-{hex}.json")
}

/// The prefix of the audio files of the slide named `slide`: its name made
/// safe for a file name and a digest of it (FNV-1a, the same in every
/// version), so two slides keep apart.
pub(crate) fn clip_prefix(slide: &str) -> String {
    let digest = slide.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    let name: String = slide
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .take(40)
        .collect();
    format!("{name}-{:06x}-", digest & 0xff_ffff)
}

/// The audio file of part `part` of the slide named `slide`, in `dir`.
pub(crate) fn clip_path(dir: &Path, slide: &str, part: usize) -> PathBuf {
    dir.join(format!("{}{part:02}.wav", clip_prefix(slide)))
}

/// The seconds spent on each segment, by name, for a rehearsal.
pub(crate) fn spent_by_name(
    segments: &[SegmentMetadata],
    spent: &HashMap<u32, Duration>,
) -> Vec<(String, f64)> {
    segments
        .iter()
        .filter_map(|segment| Some((segment.name.clone(), spent.get(&segment.id)?.as_secs_f64())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(id: u32, target: Option<f64>) -> SegmentMetadata {
        SegmentMetadata {
            id,
            name: format!("s{id}"),
            notes: None,
            target,
            start_time: 0.0,
            end_time: 1.0,
            stops: Vec::new(),
        }
    }

    fn spent(pairs: &[(u32, f64)]) -> HashMap<u32, Duration> {
        pairs
            .iter()
            .map(|(id, seconds)| (*id, Duration::from_secs_f64(*seconds)))
            .collect()
    }

    #[test]
    fn the_plan_is_the_duration_or_every_target() {
        let all = [segment(0, Some(60.0)), segment(1, Some(90.0))];
        let some = [segment(0, Some(60.0)), segment(1, None)];
        assert_eq!(planned_total(&all, None), Some(150.0));
        assert_eq!(planned_total(&some, None), None);
        assert_eq!(
            planned_total(&some, Some(Duration::from_secs(1200))),
            Some(1200.0)
        );
        assert_eq!(planned_total(&[], None), None);
    }

    #[test]
    fn drift_counts_left_segments_and_the_current_overrun() {
        let segments = [
            segment(0, Some(60.0)),
            segment(1, None),
            segment(2, Some(30.0)),
        ];
        assert_eq!(
            drift(&[segment(0, None)], &spent(&[(0, 10.0)]), Some(0)),
            None
        );
        // On the first slide, within its target: on time.
        assert_eq!(drift(&segments, &spent(&[(0, 20.0)]), Some(0)), Some(0.0));
        // Over it: behind by the overrun.
        assert_eq!(drift(&segments, &spent(&[(0, 75.0)]), Some(0)), Some(15.0));
        // Left early: ahead; a slide without a target adds nothing.
        assert_eq!(
            drift(&segments, &spent(&[(0, 40.0), (1, 300.0)]), Some(1)),
            Some(-20.0)
        );
        // The current slide only counts once it runs over.
        assert_eq!(
            drift(&segments, &spent(&[(0, 40.0), (2, 10.0)]), Some(2)),
            Some(-20.0)
        );
        assert_eq!(
            drift(&segments, &spent(&[(0, 40.0), (2, 70.0)]), Some(2)),
            Some(20.0)
        );
        // A skipped slide counts as ahead by its target.
        assert_eq!(drift(&segments, &spent(&[]), Some(2)), Some(-60.0));
    }

    #[test]
    fn tones_follow_how_far_behind() {
        assert_eq!(tone(-5.0, 600.0), PaceTone::OnTime);
        assert_eq!(tone(0.0, 600.0), PaceTone::OnTime);
        assert_eq!(tone(45.0, 600.0), PaceTone::Behind);
        assert_eq!(tone(61.0, 600.0), PaceTone::Late);
        // Short plans tolerate 30 seconds.
        assert_eq!(tone(25.0, 120.0), PaceTone::Behind);
        assert_eq!(countdown_tone(300.0, 600.0), PaceTone::OnTime);
        assert_eq!(countdown_tone(50.0, 600.0), PaceTone::Behind);
        assert_eq!(countdown_tone(-1.0, 600.0), PaceTone::Late);
    }

    #[test]
    fn talk_lengths_read_as_units_clocks_or_minutes() {
        let minutes =
            |text: &str| parse_talk_duration(text).map(|duration| duration.as_secs_f64() / 60.0);
        assert_eq!(minutes("20m"), Ok(20.0));
        assert_eq!(minutes("1h30m"), Ok(90.0));
        assert_eq!(minutes("1h 30m"), Ok(90.0));
        assert_eq!(minutes("90s"), Ok(1.5));
        assert_eq!(minutes(" 20:00 "), Ok(20.0));
        assert_eq!(minutes("1:15:00"), Ok(75.0));
        assert_eq!(minutes("45"), Ok(45.0));
        for invalid in ["", "0m", "-5m", "20x", "m", "1:2:3:4", "abc"] {
            assert_eq!(
                parse_talk_duration(invalid),
                Err(EXPECTED_LENGTH),
                "{invalid}"
            );
        }
    }

    #[test]
    fn the_length_set_here_wins_and_the_remembered_one_comes_last() {
        let minutes = |minutes: u64| Duration::from_secs(minutes * 60);
        let mut talk = TalkDuration::given(None);
        assert_eq!(talk.current(None), None);
        talk.remembered = Some(minutes(25));
        assert_eq!(
            talk.current(None),
            Some((minutes(25), LengthSource::Remembered))
        );
        // The script's comes before the remembered one.
        assert_eq!(
            talk.current(Some(minutes(20))),
            Some((minutes(20), LengthSource::Script))
        );
        talk.given = Some(minutes(30));
        assert_eq!(
            talk.current(Some(minutes(20))),
            Some((minutes(30), LengthSource::Given))
        );
        talk.chosen = Some(Some(minutes(15)));
        assert_eq!(
            talk.current(Some(minutes(20))),
            Some((minutes(15), LengthSource::Chosen))
        );
        // Cleared here: no length, whatever the others say.
        talk.chosen = Some(None);
        assert_eq!(talk.current(Some(minutes(20))), None);
    }

    #[test]
    fn a_file_remembers_its_planned_length() {
        let mut rehearsals = Rehearsals {
            path: None,
            log: RehearsalLog::default(),
        };
        assert_eq!(rehearsals.duration(), None);
        rehearsals.plan(Some(Duration::from_secs(1200)));
        assert_eq!(rehearsals.duration(), Some(Duration::from_secs(1200)));
        rehearsals.plan(None);
        assert_eq!(rehearsals.duration(), None);
    }

    fn at(time: f64, slide: Option<(&str, usize)>) -> (f64, Option<(String, usize)>) {
        (time, slide.map(|(name, part)| (name.to_owned(), part)))
    }

    #[test]
    fn a_recording_times_the_slides_it_went_through_whole() {
        let track = [
            at(0.0, Some(("Portada", 0))),
            at(10.0, Some(("Datos", 0))),
            at(14.0, Some(("Datos", 1))),
            at(30.0, Some(("Datos", 2))),
            at(32.0, Some(("Cierre", 0))),
        ];
        let names = |slides: &[TimedSlide]| -> Vec<String> {
            slides.iter().map(|slide| slide.name.clone()).collect()
        };
        let slides = timed_slides(&track, 40.0, true, false);
        // Cierre was still being said when the recording ended.
        assert_eq!(names(&slides), ["Portada", "Datos"]);
        assert_eq!(slides[0].seconds, 10.0);
        assert_eq!(slides[1].parts, [4.0, 16.0, 2.0]);
        assert_eq!(slides[1].seconds, 22.0);
        // Finished at its last stop, it counts too.
        let slides = timed_slides(&track, 40.0, true, true);
        assert_eq!(names(&slides), ["Portada", "Datos", "Cierre"]);
        assert_eq!(slides[2].seconds, 8.0);
        // Started halfway through its first slide, that one does not count.
        assert_eq!(names(&timed_slides(&track, 40.0, false, false)), ["Datos"]);
        // Leaving the slides counts as leaving the last one, and coming
        // from outside them as entering the first from its start.
        let around = [at(0.0, None), at(1.0, Some(("Portada", 0))), at(5.0, None)];
        let slides = timed_slides(&around, 6.0, false, false);
        assert_eq!(names(&slides), ["Portada"]);
        assert_eq!(slides[0].seconds, 4.0);
    }

    #[test]
    fn a_part_recorded_again_changes_its_slides_time() {
        let track = [at(0.0, Some(("Datos", 1))), at(6.0, Some(("Datos", 2)))];
        // Reached and left: from one change to the next.
        assert_eq!(timed_part(&track, "Datos", 1, 9.0, false), Some((0.0, 6.0)));
        // Still on it at the end: only once finished there.
        assert_eq!(timed_part(&track, "Datos", 2, 9.0, false), None);
        assert_eq!(timed_part(&track, "Datos", 2, 9.0, true), Some((6.0, 9.0)));
        assert_eq!(timed_part(&track, "Datos", 0, 9.0, true), None);
        // Each part of the slides asked for, where it begins and ends.
        assert_eq!(
            part_spans(&track, 9.0, &["Datos"]),
            [
                ("Datos".to_owned(), 1, 0.0, 6.0),
                ("Datos".to_owned(), 2, 6.0, 9.0)
            ]
        );
        assert!(part_spans(&track, 9.0, &["Cierre"]).is_empty());
        let dir = Path::new("clips");
        assert_eq!(
            clip_path(dir, "Datos", 1)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            format!("{}01.wav", clip_prefix("Datos"))
        );
        assert!(clip_prefix("¿Qué tal?").starts_with("-Qué-tal--"));
        assert_ne!(clip_prefix("A b"), clip_prefix("A-b"));
        let mut timing = TalkTiming::default();
        timing.record(SlideTiming {
            name: "Datos".into(),
            seconds: 20.0,
            parts: vec![4.0, 16.0],
            recorded: 1,
        });
        timing.record(SlideTiming {
            name: "Datos".into(),
            seconds: 22.0,
            parts: vec![4.0, 16.0, 2.0],
            recorded: 2,
        });
        assert_eq!(timing.slides.len(), 1);
        timing.record_part("Datos", 1, 6.0, 3);
        let datos = timing.slide("Datos").unwrap();
        assert_eq!(
            (datos.parts.clone(), datos.seconds),
            (vec![4.0, 6.0, 2.0], 12.0)
        );
        // A slide without a time does not get one from a part.
        timing.record_part("Cierre", 0, 5.0, 3);
        assert!(timing.slide("Cierre").is_none());
    }

    #[test]
    fn spans_read_as_minutes_and_signs() {
        assert_eq!(format_span(65.0, true), "+1:05");
        assert_eq!(format_span(-20.4, true), "-0:20");
        assert_eq!(format_span(0.2, true), "0:00");
        assert_eq!(format_span(3725.0, false), "1:02:05");
        assert_eq!(format_span(-75.0, false), "-1:15");
    }

    #[test]
    fn rehearsals_keep_the_last_sessions_and_find_the_previous_one() {
        let mut rehearsals = Rehearsals {
            path: None,
            log: RehearsalLog::default(),
        };
        let take = |started, total| Rehearsal {
            started,
            total,
            segments: vec![("intro".into(), total / 2.0)],
        };
        rehearsals.record(take(1, 10.0));
        assert!(rehearsals.previous(2).is_none(), "too short to keep");
        rehearsals.record(take(1, 100.0));
        rehearsals.record(take(1, 120.0));
        rehearsals.record(take(2, 200.0));
        assert_eq!(rehearsals.log.rehearsals.len(), 2);
        assert_eq!(rehearsals.previous(2).map(|kept| kept.total), Some(120.0));
        assert_eq!(
            rehearsals
                .previous(3)
                .and_then(|kept| kept.spent_on("intro")),
            Some(100.0)
        );
        for started in 3..40 {
            rehearsals.record(take(started, 60.0));
        }
        assert_eq!(rehearsals.log.rehearsals.len(), KEPT_REHEARSALS);
        assert_eq!(
            rehearsals.log.rehearsals.last().map(|kept| kept.started),
            Some(39)
        );
    }

    #[test]
    fn logs_are_named_after_the_file_and_its_path() {
        let one = log_name(Path::new("C:/talks/charla.py"));
        let other = log_name(Path::new("D:/old/charla.py"));
        assert!(one.starts_with("charla-") && one.ends_with(".json"));
        assert_ne!(one, other);
        assert_eq!(
            log_name(Path::new("mi charla.gaanim")).split('-').next(),
            Some("mi")
        );
    }
}
