//! The talk's plan as the narration panel and the recorder read it: its
//! length, where that comes from, and the share of it each slide has.
//!
//! A slide's own `target=` is its budget. The rest of the talk's length is
//! shared among the slides without one, by how much their script says, so
//! a practice run can tell at every slide whether it is ahead or behind.

use std::time::Duration;

use gaanim_timeline::notes::split_notes;
use gaanim_timeline::timeline::{SegmentMetadata, Timeline};

use crate::presenter::pace::{LengthSource, TalkDuration};
use crate::presenter::prompter::IMPLICIT_SEGMENT;

/// A slide with less script than this still gets this many words' share of
/// the talk: a title slide is shown for a moment too.
const MIN_SLIDE_WORDS: usize = 15;

/// Where the talk's planned length comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanSource {
    /// Set in the narration panel or Presenter View, given with
    /// `--duration`, or written as `scene.slides.target`.
    Talk(LengthSource),
    /// Every slide has a `target=`, and they add up to it.
    Slides,
}

impl PlanSource {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::Talk(LengthSource::Chosen) => "set here",
            Self::Talk(LengthSource::Given) => "from --duration",
            Self::Talk(LengthSource::Script) => "from scene.slides.target",
            Self::Talk(LengthSource::Remembered) => "set last time",
            Self::Slides => "the slides' targets added up",
        }
    }
}

/// The talk's planned length, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TalkPlan {
    pub(crate) seconds: f64,
    pub(crate) source: PlanSource,
}

/// The talk's planned length: the one set for it (see [`TalkDuration`]),
/// else every slide's target added up when each has one.
pub(crate) fn talk_plan(timeline: &Timeline, talk: Option<&TalkDuration>) -> Option<TalkPlan> {
    let script = timeline
        .talk_target
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .map(Duration::from_secs_f64);
    let set = match talk {
        Some(talk) => talk.current(script),
        None => script.map(|duration| (duration, LengthSource::Script)),
    };
    if let Some((duration, source)) = set {
        return Some(TalkPlan {
            seconds: duration.as_secs_f64(),
            source: PlanSource::Talk(source),
        });
    }
    let slides = slides(timeline);
    if slides.is_empty() {
        return None;
    }
    let seconds: f64 = slides
        .iter()
        .map(|slide| slide.target)
        .sum::<Option<f64>>()?;
    Some(TalkPlan {
        seconds,
        source: PlanSource::Slides,
    })
}

/// The authored slides, in order.
pub(crate) fn slides(timeline: &Timeline) -> Vec<&SegmentMetadata> {
    timeline
        .segments
        .iter()
        .filter(|segment| segment.name != IMPLICIT_SEGMENT)
        .collect()
}

/// The words a slide's script says, without its stop marks.
pub(crate) fn slide_words(slide: &SegmentMetadata) -> usize {
    slide.notes.as_deref().map_or(0, |notes| {
        split_notes(notes)
            .iter()
            .map(|part| gaanim_media::narration::text_words(&part.text).len())
            .sum()
    })
}

/// Seconds `words` take at `words_per_minute`.
pub(crate) fn reading_seconds(words: usize, words_per_minute: f64) -> f64 {
    words as f64 * 60.0 / words_per_minute.max(1.0)
}

/// Each slide's share of a talk planned to last `planned` seconds: its own
/// target, else what the targets leave, shared by how much each says.
/// Without a plan, only the slides with a target have one.
pub(crate) fn budgets(slides: &[&SegmentMetadata], planned: Option<f64>) -> Vec<Option<f64>> {
    let Some(planned) = planned else {
        return slides.iter().map(|slide| slide.target).collect();
    };
    let authored: f64 = slides.iter().filter_map(|slide| slide.target).sum();
    let left = (planned - authored).max(0.0);
    let weight = |slide: &SegmentMetadata| slide_words(slide).max(MIN_SLIDE_WORDS) as f64;
    let shared: f64 = slides
        .iter()
        .filter(|slide| slide.target.is_none())
        .map(|slide| weight(slide))
        .sum();
    slides
        .iter()
        .map(|slide| {
            Some(match slide.target {
                Some(target) => target,
                None if shared > 0.0 => left * weight(slide) / shared,
                None => 0.0,
            })
        })
        .collect()
}

/// Seconds behind (positive) or ahead of (negative) the budgets, on the
/// slide at `current` after `spent` seconds there, for a run that started
/// at the slide at `first` and has taken `elapsed` seconds: the slides
/// passed count whole, the current one only once it runs over.
pub(crate) fn drift(
    budgets: &[Option<f64>],
    first: usize,
    current: usize,
    spent: f64,
    elapsed: f64,
) -> Option<f64> {
    let mut expected = 0.0;
    for budget in budgets.get(first..current)? {
        expected += (*budget)?;
    }
    let budget = (*budgets.get(current)?)?;
    Some(elapsed - expected - spent.min(budget))
}

/// A length typed for the talk, such as `15:00`, `15m` or `1h30m`.
pub(crate) fn parse_length(text: &str) -> Result<Duration, &'static str> {
    crate::presenter::pace::parse_talk_duration(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slide(name: &str, notes: &str, target: Option<f64>) -> SegmentMetadata {
        SegmentMetadata {
            id: 0,
            name: name.to_owned(),
            notes: (!notes.is_empty()).then(|| notes.to_owned()),
            target,
            start_time: 0.0,
            end_time: 0.0,
            stops: Vec::new(),
        }
    }

    #[test]
    fn the_plan_is_shared_by_how_much_each_slide_says() {
        let long = "word ".repeat(60);
        let short = "word ".repeat(20);
        let slides = [
            slide("Intro", "", Some(30.0)),
            slide("Long", &long, None),
            slide("Short", &format!("{short}\n[stop]\n"), None),
        ];
        let refs: Vec<&SegmentMetadata> = slides.iter().collect();
        let budgets = budgets(&refs, Some(150.0));
        assert_eq!(budgets[0], Some(30.0));
        // 120 seconds left, shared 60:20.
        assert!((budgets[1].unwrap() - 90.0).abs() < 1e-9);
        assert!((budgets[2].unwrap() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn a_slide_without_script_still_gets_a_share() {
        let slides = [
            slide("Title", "", None),
            slide("Body", &"w ".repeat(45), None),
        ];
        let refs: Vec<&SegmentMetadata> = slides.iter().collect();
        let budgets = budgets(&refs, Some(60.0));
        assert!((budgets[0].unwrap() - 15.0).abs() < 1e-9);
        assert!((budgets[1].unwrap() - 45.0).abs() < 1e-9);
        assert_eq!(super::budgets(&refs, None), vec![None, None]);
    }

    #[test]
    fn drift_counts_passed_slides_whole_and_the_current_once_over() {
        let budgets = [Some(30.0), Some(60.0), Some(20.0)];
        // 40 s on the first slide, 10 s into the second: 10 s behind.
        assert_eq!(drift(&budgets, 0, 1, 10.0, 50.0), Some(10.0));
        // 25 s on the first, 70 s on the second (10 over): 5 s behind.
        assert_eq!(drift(&budgets, 0, 1, 70.0, 95.0), Some(5.0));
        // Starting at the second slide, 50 s in: on time.
        assert_eq!(drift(&budgets, 1, 1, 50.0, 50.0), Some(0.0));
        assert_eq!(drift(&[Some(30.0), None], 0, 1, 1.0, 31.0), None);
    }

    #[test]
    fn the_slides_targets_plan_the_talk_when_nothing_else_does() {
        let mut timeline = Timeline::default();
        timeline.segments = vec![slide("A", "", Some(30.0)), slide("B", "", Some(45.0))];
        let plan = talk_plan(&timeline, None).unwrap();
        assert_eq!(plan.seconds, 75.0);
        assert_eq!(plan.source, PlanSource::Slides);
        let talk = TalkDuration::given(Some(Duration::from_secs(600)));
        let plan = talk_plan(&timeline, Some(&talk)).unwrap();
        assert_eq!(plan.seconds, 600.0);
        timeline.segments[1].target = None;
        assert_eq!(talk_plan(&timeline, None), None);
    }
}
