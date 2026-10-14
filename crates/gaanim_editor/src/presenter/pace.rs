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

/// The length planned for the whole talk with `--present --duration`.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct TalkDuration(pub Option<Duration>);

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

#[derive(Debug, Default, Serialize, Deserialize)]
struct RehearsalLog {
    source: PathBuf,
    rehearsals: Vec<Rehearsal>,
}

/// The rehearsals of the file at `source`, oldest first.
pub(crate) struct Rehearsals {
    path: Option<PathBuf>,
    log: RehearsalLog,
}

impl Rehearsals {
    /// The rehearsals kept for `source` in Gaanim's data folder.
    pub(crate) fn load(source: &Path) -> Self {
        let path = gaanim_project::user_data_dir()
            .map(|dir| dir.join("rehearsals").join(log_name(source)));
        let log = path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| RehearsalLog {
                source: source.to_path_buf(),
                rehearsals: Vec::new(),
            });
        Self { path, log }
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
