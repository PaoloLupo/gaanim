//! Audience poll data inside a scene (`scene.poll`).
//!
//! A poll exposes its votes as values the scene reads: parameters
//! ([`PollValue`]) that any reactive drawable can follow, and bars
//! ([`PollBar`]) whose length follows an answer. While a presentation
//! collects votes, the host fills [`PollResults`]; everywhere else it stays
//! empty and each poll shows its authored preview counts, so previews,
//! exports and snapshots are deterministic.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::{Component, DetectChangesMut, Query, Res, Resource};
use gaanim_core::kurbo::{BezPath, Rect, RoundedRect, Shape};
use gaanim_math::Bounds3D;
use gaanim_scene::{LocalBounds, Path2D, PathSource};

use crate::updaters::{SampledProperty, SampledSeriesDrivers};

/// Vote counts reported during a live presentation, by poll id. Empty
/// outside one, so every poll shows its preview counts.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct PollResults(pub HashMap<Arc<str>, Vec<u32>>);

impl PollResults {
    /// The counts to show for `poll`: live ones when the presentation has
    /// them for the same answers, the preview otherwise.
    pub fn counts<'a>(&'a self, poll: &str, preview: &'a [u32]) -> &'a [u32] {
        self.0
            .get(poll)
            .filter(|counts| counts.len() == preview.len())
            .map_or(preview, Vec::as_slice)
    }
}

/// A number a poll reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollMeasure {
    /// Votes for one answer.
    Votes(usize),
    /// One answer's fraction of all votes, from 0 to 1.
    Share(usize),
    /// Votes for every answer.
    Total,
}

impl PollMeasure {
    pub fn value(self, counts: &[u32]) -> f64 {
        let total: u32 = counts.iter().sum();
        match self {
            Self::Votes(answer) => f64::from(counts.get(answer).copied().unwrap_or(0)),
            Self::Share(answer) if total > 0 => {
                f64::from(counts.get(answer).copied().unwrap_or(0)) / f64::from(total)
            }
            Self::Share(_) => 0.0,
            Self::Total => f64::from(total),
        }
    }
}

/// Makes a parameter report a poll's live value: its sampled driver holds
/// the value, so readouts, computed values and bindings follow it natively.
#[derive(Component, Debug, Clone)]
pub struct PollValue {
    pub poll: Arc<str>,
    pub measure: PollMeasure,
    pub preview: Arc<[u32]>,
}

/// Which way a bar grows from its start edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarDirection {
    Right,
    Left,
    Up,
    Down,
}

impl BarDirection {
    pub fn name(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Right, Self::Left, Self::Up, Self::Down]
            .into_iter()
            .find(|direction| direction.name() == name)
    }
}

/// What a full-length bar stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarScale {
    /// All the votes: a bar's length is its answer's share.
    Total,
    /// The leading answer's votes, so the leader always fills its bar.
    Leader,
}

impl BarScale {
    pub fn name(self) -> &'static str {
        match self {
            Self::Total => "total",
            Self::Leader => "leader",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Total, Self::Leader]
            .into_iter()
            .find(|scale| scale.name() == name)
    }
}

/// Geometry of a poll bar. The drawable's bounds are always the full-length
/// box, centered on the origin, so layouts do not move as votes arrive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarSpec {
    pub length: f64,
    pub thickness: f64,
    pub radius: f64,
    pub direction: BarDirection,
    pub scale: BarScale,
}

impl BarSpec {
    /// How much of the bar answer `answer` fills, from 0 to 1.
    pub fn fraction(&self, answer: usize, counts: &[u32]) -> f64 {
        let votes = f64::from(counts.get(answer).copied().unwrap_or(0));
        let full = match self.scale {
            BarScale::Total => f64::from(counts.iter().sum::<u32>()),
            BarScale::Leader => f64::from(counts.iter().copied().max().unwrap_or(0)),
        };
        if full > 0.0 {
            (votes / full).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The full-length box, in the drawable's local coordinates.
    pub fn full(&self) -> Rect {
        let (width, height) = match self.direction {
            BarDirection::Right | BarDirection::Left => (self.length, self.thickness),
            BarDirection::Up | BarDirection::Down => (self.thickness, self.length),
        };
        Rect::new(-width / 2.0, -height / 2.0, width / 2.0, height / 2.0)
    }

    pub fn bounds(&self) -> Bounds3D {
        let full = self.full();
        Bounds3D::new_2d(full.x0, full.y0, full.x1, full.y1)
    }

    /// The filled part of the bar at `fraction`, grown from its start edge.
    pub fn path(&self, fraction: f64) -> BezPath {
        let filled = self.length * fraction.clamp(0.0, 1.0);
        if filled <= 1e-9 {
            return BezPath::new();
        }
        let full = self.full();
        let rect = match self.direction {
            BarDirection::Right => Rect::new(full.x0, full.y0, full.x0 + filled, full.y1),
            BarDirection::Left => Rect::new(full.x1 - filled, full.y0, full.x1, full.y1),
            BarDirection::Up => Rect::new(full.x0, full.y0, full.x1, full.y0 + filled),
            BarDirection::Down => Rect::new(full.x0, full.y1 - filled, full.x1, full.y1),
        };
        let radius = self
            .radius
            .min(rect.width() / 2.0)
            .min(rect.height() / 2.0)
            .max(0.0);
        if radius > 0.0 {
            RoundedRect::from_rect(rect, radius).to_path(1e-3)
        } else {
            rect.to_path(1e-3)
        }
    }
}

/// A bar whose length follows one answer of a poll.
#[derive(Component, Debug, Clone)]
pub struct PollBar {
    pub poll: Arc<str>,
    pub answer: usize,
    pub preview: Arc<[u32]>,
    pub spec: BarSpec,
    /// The fraction last drawn and its outline.
    pub last: Option<(f64, Arc<BezPath>)>,
}

/// Put each poll value into its parameter's driver. Runs before the sampled
/// series system, which writes the parameter's signal from it.
pub fn poll_value_system(
    results: Option<Res<PollResults>>,
    mut values: Query<(&PollValue, &mut SampledSeriesDrivers)>,
) {
    let empty = PollResults::default();
    let results = results.as_deref().unwrap_or(&empty);
    for (value, mut drivers) in &mut values {
        let current = value
            .measure
            .value(results.counts(&value.poll, &value.preview));
        let stale = drivers.0.iter().any(|driver| {
            driver.property == SampledProperty::Signal
                && driver.values.iter().any(|value| *value != current)
        });
        if !stale {
            continue;
        }
        for driver in drivers
            .0
            .iter_mut()
            .filter(|driver| driver.property == SampledProperty::Signal)
        {
            driver.values = Arc::from(vec![current; driver.times.len()]);
        }
    }
}

/// Draw each poll bar at its answer's current fraction, honoring a create
/// animation's reveal.
pub fn poll_bar_system(
    results: Option<Res<PollResults>>,
    mut bars: Query<(
        &mut PollBar,
        &mut Path2D,
        Option<&mut PathSource>,
        &mut LocalBounds,
        Option<&crate::writing::PathReveal>,
    )>,
) {
    let empty = PollResults::default();
    let results = results.as_deref().unwrap_or(&empty);
    for (mut bar, mut path, source, mut bounds, reveal) in &mut bars {
        let fraction = bar
            .spec
            .fraction(bar.answer, results.counts(&bar.poll, &bar.preview));
        let outline = match &bar.last {
            Some((last, outline)) if (*last - fraction).abs() < 1e-12 => outline.clone(),
            _ => {
                let outline = Arc::new(bar.spec.path(fraction));
                bar.last = Some((fraction, outline.clone()));
                outline
            }
        };
        // Seeks restore Path2D from a snapshot: write whenever it differs.
        if let Some(mut source) = source
            && !Arc::ptr_eq(&source.0, &outline)
        {
            source.0 = outline.clone();
        }
        let reveal = reveal.map_or(1.0, |reveal| reveal.0);
        let visible = crate::writing::path_at_reveal(&outline, reveal);
        if !Arc::ptr_eq(&path.0, &visible) {
            path.0 = visible;
        }
        bounds.set_if_neq(LocalBounds(bar.spec.bounds()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(direction: BarDirection, scale: BarScale) -> BarSpec {
        BarSpec {
            length: 4.0,
            thickness: 1.0,
            radius: 0.0,
            direction,
            scale,
        }
    }

    #[test]
    fn measures_read_votes_shares_and_totals() {
        let counts = [3, 1, 0];
        assert_eq!(PollMeasure::Votes(0).value(&counts), 3.0);
        assert_eq!(PollMeasure::Share(1).value(&counts), 0.25);
        assert_eq!(PollMeasure::Total.value(&counts), 4.0);
        assert_eq!(PollMeasure::Share(0).value(&[0, 0]), 0.0);
        assert_eq!(PollMeasure::Votes(7).value(&counts), 0.0);
    }

    #[test]
    fn live_counts_replace_the_preview_only_for_the_same_answers() {
        let mut results = PollResults::default();
        let preview = [1, 2];
        assert_eq!(results.counts("p0", &preview), [1, 2]);
        results.0.insert("p0".into(), vec![5, 0]);
        assert_eq!(results.counts("p0", &preview), [5, 0]);
        results.0.insert("p1".into(), vec![5, 0, 1]);
        assert_eq!(results.counts("p1", &preview), [1, 2]);
    }

    #[test]
    fn bars_scale_by_total_or_by_the_leader() {
        let counts = [2, 6, 0];
        let total = bar(BarDirection::Right, BarScale::Total);
        assert_eq!(total.fraction(0, &counts), 0.25);
        let leader = bar(BarDirection::Right, BarScale::Leader);
        assert_eq!(leader.fraction(1, &counts), 1.0);
        assert!((leader.fraction(0, &counts) - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(leader.fraction(0, &[0, 0, 0]), 0.0);
    }

    #[test]
    fn bars_grow_from_their_start_edge_inside_fixed_bounds() {
        let right = bar(BarDirection::Right, BarScale::Total);
        assert_eq!(
            right.path(0.5).bounding_box(),
            Rect::new(-2.0, -0.5, 0.0, 0.5)
        );
        let left = bar(BarDirection::Left, BarScale::Total);
        assert_eq!(
            left.path(0.25).bounding_box(),
            Rect::new(1.0, -0.5, 2.0, 0.5)
        );
        let up = bar(BarDirection::Up, BarScale::Total);
        assert_eq!(up.path(1.0).bounding_box(), Rect::new(-0.5, -2.0, 0.5, 2.0));
        assert_eq!(up.bounds(), Bounds3D::new_2d(-0.5, -2.0, 0.5, 2.0));
        let down = bar(BarDirection::Down, BarScale::Total);
        assert_eq!(
            down.path(0.5).bounding_box(),
            Rect::new(-0.5, 0.0, 0.5, 2.0)
        );
        assert!(right.path(0.0).elements().is_empty());
    }
}
