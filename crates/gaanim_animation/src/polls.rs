//! Audience poll data inside a scene (`scene.poll`, `scene.quiz`,
//! `scene.leaderboard`).
//!
//! A poll exposes its data as values the scene reads: parameters
//! ([`PollValue`]) that any reactive drawable can follow, bars ([`PollBar`])
//! whose length follows an answer or a player's score, and live text
//! ([`LiveText`]) for the leaderboard's nicknames. While a presentation
//! collects votes the host fills [`PollResults`] and marks it live;
//! everywhere else each value shows its authored preview, so previews,
//! exports and snapshots are deterministic.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::{Component, DetectChangesMut, Query, Res, Resource};
use gaanim_core::kurbo::{Affine, BezPath, Rect, RoundedRect, Shape};
use gaanim_math::Bounds3D;
use gaanim_scene::{LocalBounds, Path2D, PathSource};

use crate::updaters::{SampledProperty, SampledSeriesDrivers};

/// What a live presentation reported. Outside one it is not live, and every
/// poll value shows its preview.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct PollResults {
    /// Whether a presentation is collecting votes.
    pub live: bool,
    /// Vote counts by poll id.
    pub counts: HashMap<Arc<str>, Vec<u32>>,
    /// Seconds left to answer each quiz the relay has opened.
    pub remaining: HashMap<Arc<str>, f64>,
    /// Players' nicknames and scores, best first.
    pub leaderboard: Vec<(Arc<str>, u64)>,
    /// Everyone who joined the game.
    pub players: u32,
}

impl PollResults {
    /// The counts to show for `poll` while live: the relay's, or zeros for a
    /// poll it has not opened yet.
    pub fn live_counts(&self, poll: &str, answers: usize) -> Vec<u32> {
        self.counts
            .get(poll)
            .filter(|counts| counts.len() == answers)
            .cloned()
            .unwrap_or_else(|| vec![0; answers])
    }

    /// The live value of `source`, or `None` outside a live presentation.
    pub fn value(&self, source: &PollSource) -> Option<f64> {
        if !self.live {
            return None;
        }
        Some(match source {
            PollSource::Poll {
                poll,
                measure: PollMeasure::Remaining { time },
                ..
            } => self.remaining.get(poll.as_ref()).copied().unwrap_or(*time),
            PollSource::Poll {
                poll,
                answers,
                measure,
            } => measure.value(&self.live_counts(poll, *answers)),
            PollSource::LeaderScore { rank } => self
                .leaderboard
                .get(*rank)
                .map_or(0.0, |(_, score)| *score as f64),
            PollSource::Players => f64::from(self.players),
        })
    }

    /// The live nickname at `rank`, empty past the last player.
    pub fn leader_name(&self, rank: usize) -> Option<Arc<str>> {
        if !self.live {
            return None;
        }
        Some(
            self.leaderboard
                .get(rank)
                .map_or_else(|| Arc::from(""), |(name, _)| name.clone()),
        )
    }
}

/// A number a poll reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PollMeasure {
    /// Votes for one answer.
    Votes(usize),
    /// One answer's fraction of all votes, from 0 to 1.
    Share(usize),
    /// One answer's share of all votes, from 0 to 100.
    Percent(usize),
    /// Votes for every answer.
    Total,
    /// Seconds left to answer a quiz of `time` seconds.
    Remaining { time: f64 },
}

impl PollMeasure {
    pub fn value(self, counts: &[u32]) -> f64 {
        let total: u32 = counts.iter().sum();
        let share = |answer: usize| {
            if total > 0 {
                f64::from(counts.get(answer).copied().unwrap_or(0)) / f64::from(total)
            } else {
                0.0
            }
        };
        match self {
            Self::Votes(answer) => f64::from(counts.get(answer).copied().unwrap_or(0)),
            Self::Share(answer) => share(answer),
            Self::Percent(answer) => 100.0 * share(answer),
            Self::Total => f64::from(total),
            Self::Remaining { time } => time,
        }
    }
}

/// Where a live value comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum PollSource {
    Poll {
        poll: Arc<str>,
        answers: usize,
        measure: PollMeasure,
    },
    /// The score of the player at `rank` (0 for the leader).
    LeaderScore { rank: usize },
    /// How many players joined.
    Players,
}

/// Makes a parameter report a live value: its sampled driver holds the
/// value, so readouts, computed values and bindings follow it natively.
/// Outside a live presentation the driver keeps its authored `preview`
/// samples.
#[derive(Component, Debug, Clone)]
pub struct PollValue {
    pub source: PollSource,
    pub preview: Arc<[f64]>,
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

/// What a bar's length follows.
#[derive(Debug, Clone, PartialEq)]
pub enum BarSource {
    /// One answer of a poll.
    Answer {
        poll: Arc<str>,
        answer: usize,
        answers: usize,
    },
    /// The score of the player at `rank`, against the leader's.
    Leader { rank: usize },
}

impl BarSource {
    /// The live fraction of the bar, or `None` outside a live presentation.
    pub fn live_fraction(&self, spec: &BarSpec, results: &PollResults) -> Option<f64> {
        if !results.live {
            return None;
        }
        Some(match self {
            Self::Answer {
                poll,
                answer,
                answers,
            } => spec.fraction(*answer, &results.live_counts(poll, *answers)),
            Self::Leader { rank } => {
                leader_fraction(results.leaderboard.iter().map(|(_, score)| *score), *rank)
            }
        })
    }
}

/// The score at `rank` against the leader's, from 0 to 1.
pub fn leader_fraction(scores: impl IntoIterator<Item = u64>, rank: usize) -> f64 {
    let scores: Vec<u64> = scores.into_iter().collect();
    match (scores.first(), scores.get(rank)) {
        (Some(&leader), Some(&score)) if leader > 0 => {
            (score as f64 / leader as f64).clamp(0.0, 1.0)
        }
        _ => 0.0,
    }
}

/// A bar whose length follows a poll answer or a player's score.
#[derive(Component, Debug, Clone)]
pub struct PollBar {
    pub source: BarSource,
    pub spec: BarSpec,
    /// Fraction shown outside a live presentation.
    pub preview: f64,
    /// The fraction last drawn and its outline.
    pub last: Option<(f64, Arc<BezPath>)>,
}

/// Put each live value into its parameter's driver, or restore its preview.
/// Runs before the sampled series system, which writes the signal from it.
pub fn poll_value_system(
    results: Option<Res<PollResults>>,
    mut values: Query<(&PollValue, &mut SampledSeriesDrivers)>,
) {
    let empty = PollResults::default();
    let results = results.as_deref().unwrap_or(&empty);
    for (value, mut drivers) in &mut values {
        let live = results.value(&value.source);
        let wanted = |len: usize| -> Arc<[f64]> {
            match live {
                Some(current) => Arc::from(vec![current; len]),
                None => value.preview.clone(),
            }
        };
        let stale = drivers.0.iter().any(|driver| {
            driver.property == SampledProperty::Signal
                && *driver.values != *wanted(driver.values.len())
        });
        if !stale {
            continue;
        }
        for driver in drivers
            .0
            .iter_mut()
            .filter(|driver| driver.property == SampledProperty::Signal)
        {
            driver.values = wanted(driver.values.len());
        }
    }
}

/// Draw each poll bar at its current fraction, honoring a create
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
            .source
            .live_fraction(&bar.spec, results)
            .unwrap_or(bar.preview);
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

// ---------------------------------------------------------------------------
// Live text
// ---------------------------------------------------------------------------

/// How live text sits on its position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Left, Self::Center, Self::Right]
            .into_iter()
            .find(|align| align.name() == name)
    }
}

/// Outlines and advances of single characters, so text lays out the same
/// wherever it is drawn: in the scene, or replayed from a bundle that stored
/// the atlas.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlyphAtlas {
    pub glyphs: HashMap<char, (Arc<BezPath>, f64)>,
}

impl GlyphAtlas {
    /// `text` laid out glyph by glyph from the atlas: its outline, advance
    /// and the vertical offset that centers capitals on the baseline row.
    /// Characters missing from the atlas show as `?`, or nothing.
    pub fn layout(&self, text: &str) -> (BezPath, f64) {
        let mut path = BezPath::new();
        let mut pen = 0.0;
        for ch in text.chars() {
            let Some((outline, advance)) = self.glyphs.get(&ch).or_else(|| self.glyphs.get(&'?'))
            else {
                continue;
            };
            let mut glyph = (**outline).clone();
            glyph.apply_affine(Affine::translate((pen, 0.0)));
            path.extend(glyph);
            pen += advance;
        }
        (path, pen)
    }

    /// Vertical offset that puts the middle of a capital `H` on y = 0, so
    /// names with and without descenders share one row.
    pub fn center_offset(&self) -> f64 {
        self.glyphs
            .get(&'H')
            .map(|(outline, _)| {
                let rect = outline.bounding_box();
                -(rect.y0 + rect.y1) / 2.0
            })
            .unwrap_or(0.0)
    }

    /// Place `text` for `align`: its outline around the origin and bounds.
    pub fn place(&self, text: &str, align: TextAlign) -> (BezPath, Bounds3D) {
        let (mut path, width) = self.layout(text);
        let x = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => -width / 2.0,
            TextAlign::Right => -width,
        };
        path.apply_affine(Affine::translate((x, self.center_offset())));
        let rect = path.bounding_box();
        let bounds = if path.elements().is_empty() {
            Bounds3D::default()
        } else {
            Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1)
        };
        (path, bounds)
    }
}

/// Shape `ch` alone: its outline and advance.
pub fn shape_glyph(
    registry: &gaanim_text::font::FontRegistry,
    ch: char,
    font_family: &str,
    weight: Option<u16>,
    font_size: f64,
) -> Option<(Arc<BezPath>, f64)> {
    let mut buffer = [0; 4];
    let run = gaanim_text::typst_compiler::shape_typst_text_run(
        registry,
        ch.encode_utf8(&mut buffer),
        font_family,
        weight,
        font_size,
    )
    .ok()?;
    Some((Arc::new(run.path), run.advance))
}

/// Characters a bundle stores for live text: printable ASCII and Latin-1,
/// which covers nicknames in Spanish, Portuguese, French and English.
pub fn atlas_characters() -> impl Iterator<Item = char> {
    (' '..='~').chain('\u{a1}'..='\u{ff}')
}

/// What live text shows.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveTextSource {
    /// The nickname of the player at `rank` (0 for the leader).
    LeaderName { rank: usize },
}

/// Text that follows live data, drawn from a glyph atlas.
#[derive(Component, Debug, Clone)]
pub struct LiveText {
    pub source: LiveTextSource,
    /// Text shown outside a live presentation.
    pub preview: Arc<str>,
    pub font_family: String,
    pub font_weight: Option<u16>,
    pub font_size: f64,
    pub align: TextAlign,
    /// Glyphs shaped so far; a bundle stores them.
    pub atlas: GlyphAtlas,
    /// The text last drawn and its outline.
    pub last: Option<(Arc<str>, Arc<BezPath>, Bounds3D)>,
}

impl LiveText {
    fn text(&self, results: &PollResults) -> Arc<str> {
        match self.source {
            LiveTextSource::LeaderName { rank } => results.leader_name(rank),
        }
        .unwrap_or_else(|| self.preview.clone())
    }

    /// Shape every character of `text` the atlas lacks, and the `H` and `?`
    /// the layout relies on.
    pub fn learn(&mut self, registry: &gaanim_text::font::FontRegistry, text: &str) {
        self.learn_with(registry, text, &mut GlyphCache::default());
    }

    /// [`Self::learn`], reusing glyphs other live texts in the same font
    /// already shaped.
    pub fn learn_with(
        &mut self,
        registry: &gaanim_text::font::FontRegistry,
        text: &str,
        cache: &mut GlyphCache,
    ) {
        for ch in text.chars().chain(['H', '?']) {
            if self.atlas.glyphs.contains_key(&ch) {
                continue;
            }
            let key = (
                self.font_family.clone(),
                self.font_weight,
                self.font_size.to_bits(),
                ch,
            );
            let glyph = cache.0.entry(key).or_insert_with(|| {
                shape_glyph(
                    registry,
                    ch,
                    &self.font_family,
                    self.font_weight,
                    self.font_size,
                )
            });
            if let Some(glyph) = glyph {
                self.atlas.glyphs.insert(ch, glyph.clone());
            }
        }
    }
}

/// Glyphs shaped for live text, by font family, weight, size and
/// character, shared by every live text so each glyph is shaped once.
#[derive(Default)]
#[allow(clippy::type_complexity)]
pub struct GlyphCache(HashMap<(String, Option<u16>, u64, char), Option<(Arc<BezPath>, f64)>>);

/// Draw each live text at its current value.
pub fn live_text_system(
    registry: Option<Res<gaanim_text::font::FontRegistry>>,
    results: Option<Res<PollResults>>,
    mut cache: bevy::prelude::Local<GlyphCache>,
    mut texts: Query<(
        &mut LiveText,
        &mut Path2D,
        Option<&mut PathSource>,
        &mut LocalBounds,
        Option<&crate::writing::PathReveal>,
    )>,
) {
    let empty = PollResults::default();
    let results = results.as_deref().unwrap_or(&empty);
    for (mut live, mut path, source, mut bounds, reveal) in &mut texts {
        let text = live.text(results);
        let (outline, placed) = match &live.last {
            Some((last, outline, placed)) if *last == text => (outline.clone(), *placed),
            _ => {
                if let Some(registry) = registry.as_deref() {
                    live.learn_with(registry, &text, &mut cache);
                }
                let (outline, placed) = live.atlas.place(&text, live.align);
                let outline = Arc::new(outline);
                live.last = Some((text, outline.clone(), placed));
                (outline, placed)
            }
        };
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
        bounds.set_if_neq(LocalBounds(placed));
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

    fn live(counts: &[(&str, Vec<u32>)]) -> PollResults {
        PollResults {
            live: true,
            counts: counts
                .iter()
                .map(|(poll, counts)| (Arc::from(*poll), counts.clone()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn measures_read_votes_shares_percents_and_totals() {
        let counts = [3, 1, 0];
        assert_eq!(PollMeasure::Votes(0).value(&counts), 3.0);
        assert_eq!(PollMeasure::Share(1).value(&counts), 0.25);
        assert_eq!(PollMeasure::Percent(0).value(&counts), 75.0);
        assert_eq!(PollMeasure::Total.value(&counts), 4.0);
        assert_eq!(PollMeasure::Share(0).value(&[0, 0]), 0.0);
        assert_eq!(PollMeasure::Votes(7).value(&counts), 0.0);
        assert_eq!(PollMeasure::Remaining { time: 20.0 }.value(&counts), 20.0);
    }

    #[test]
    fn live_values_replace_previews_only_while_live() {
        let votes = PollSource::Poll {
            poll: "p0".into(),
            answers: 2,
            measure: PollMeasure::Votes(0),
        };
        assert_eq!(PollResults::default().value(&votes), None);
        assert_eq!(live(&[("p0", vec![5, 0])]).value(&votes), Some(5.0));
        // A poll the relay has not opened counts nothing yet.
        assert_eq!(live(&[]).value(&votes), Some(0.0));
        assert_eq!(live(&[("p0", vec![5, 0, 1])]).value(&votes), Some(0.0));

        let remaining = PollSource::Poll {
            poll: "q1".into(),
            answers: 2,
            measure: PollMeasure::Remaining { time: 20.0 },
        };
        let mut results = live(&[]);
        assert_eq!(results.value(&remaining), Some(20.0));
        results.remaining.insert("q1".into(), 7.5);
        assert_eq!(results.value(&remaining), Some(7.5));

        results.leaderboard = vec![("Ana".into(), 900), ("Beto".into(), 300)];
        results.players = 5;
        assert_eq!(
            results.value(&PollSource::LeaderScore { rank: 1 }),
            Some(300.0)
        );
        assert_eq!(
            results.value(&PollSource::LeaderScore { rank: 4 }),
            Some(0.0)
        );
        assert_eq!(results.value(&PollSource::Players), Some(5.0));
        assert_eq!(results.leader_name(0).as_deref(), Some("Ana"));
        assert_eq!(results.leader_name(3).as_deref(), Some(""));
    }

    #[test]
    fn bars_follow_answers_or_the_leader() {
        let counts = [2, 6, 0];
        let total = bar(BarDirection::Right, BarScale::Total);
        assert_eq!(total.fraction(0, &counts), 0.25);
        let leader = bar(BarDirection::Right, BarScale::Leader);
        assert_eq!(leader.fraction(1, &counts), 1.0);
        assert!((leader.fraction(0, &counts) - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(leader.fraction(0, &[0, 0, 0]), 0.0);

        assert_eq!(leader_fraction([800, 200], 1), 0.25);
        assert_eq!(leader_fraction([0, 0], 1), 0.0);
        assert_eq!(leader_fraction([800], 3), 0.0);
        let mut results = live(&[]);
        results.leaderboard = vec![("Ana".into(), 1000), ("Beto".into(), 500)];
        let source = BarSource::Leader { rank: 1 };
        assert_eq!(source.live_fraction(&total, &results), Some(0.5));
        assert_eq!(source.live_fraction(&total, &PollResults::default()), None);
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

    #[test]
    fn atlas_text_lays_out_glyph_by_glyph_and_aligns() {
        let square = |size: f64| Arc::new(Rect::new(0.0, 0.0, size, size).to_path(1e-3));
        let mut atlas = GlyphAtlas::default();
        atlas.glyphs.insert('H', (square(1.0), 1.5));
        atlas.glyphs.insert('i', (square(0.5), 0.75));
        atlas.glyphs.insert('?', (square(0.25), 1.0));
        let (_, width) = atlas.layout("Hi");
        assert_eq!(width, 2.25);
        // Unknown characters fall back to `?`.
        assert_eq!(atlas.layout("Hé").1, 2.5);
        let (_, bounds) = atlas.place("Hi", TextAlign::Right);
        assert!((bounds.max.x - (-0.75 + 0.5)).abs() < 1e-9);
        let (_, bounds) = atlas.place("H", TextAlign::Center);
        assert!((bounds.min.x + 0.75).abs() < 1e-9);
        // Capitals are centered on y = 0.
        assert!((bounds.min.y + 0.5).abs() < 1e-9 && (bounds.max.y - 0.5).abs() < 1e-9);
        assert_eq!(atlas.place("", TextAlign::Left).1, Bounds3D::default());
    }

    /// Per-frame cost of the poll systems in a busy game scene. Run with
    /// `cargo test -p gaanim_animation poll_systems_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing, not a check"]
    fn poll_systems_cost_per_frame() {
        use bevy::prelude::{Schedule, World};
        use std::time::Instant;

        let mut world = World::new();
        let registry = gaanim_text::font::FontRegistry::without_system_fonts();
        let body = gaanim_text::prelude::TextConfig::default().roles
            [&gaanim_text::prelude::TextRole::Body]
            .clone();
        world.insert_resource(registry);
        let mut results = live(&[("p0", vec![5, 3, 2, 1])]);
        results.leaderboard = (0..100)
            .map(|index| {
                (
                    Arc::from(format!("Jugadora {index} ñandú")),
                    3000 - index as u64 * 20,
                )
            })
            .collect();
        results.players = 100;
        results.remaining.insert("p0".into(), 20.0);
        world.insert_resource(results);
        for index in 0..40 {
            let driver = crate::SampledSeriesDriver::new(
                vec![0.0],
                vec![0.0],
                SampledProperty::Signal,
                crate::SampledInterpolation::Step,
                1.0,
                0.0,
            )
            .unwrap();
            world.spawn((
                PollValue {
                    source: PollSource::Poll {
                        poll: "p0".into(),
                        answers: 4,
                        measure: if index % 5 == 4 {
                            PollMeasure::Remaining { time: 20.0 }
                        } else {
                            PollMeasure::Votes(index % 4)
                        },
                    },
                    preview: Arc::from([0.0]),
                },
                SampledSeriesDrivers::from(driver),
            ));
        }
        for rank in 0..10 {
            world.spawn((
                PollBar {
                    source: BarSource::Leader { rank },
                    spec: bar(BarDirection::Right, BarScale::Leader),
                    preview: 0.5,
                    last: None,
                },
                Path2D::default(),
                PathSource::default(),
                LocalBounds::default(),
            ));
            world.spawn((
                LiveText {
                    source: LiveTextSource::LeaderName { rank },
                    preview: "".into(),
                    font_family: body.font_family.clone(),
                    font_weight: None,
                    font_size: body.size,
                    align: TextAlign::Left,
                    atlas: GlyphAtlas::default(),
                    last: None,
                },
                Path2D::default(),
                PathSource::default(),
                LocalBounds::default(),
            ));
        }
        let mut schedule = Schedule::default();
        schedule.add_systems((poll_value_system, poll_bar_system, live_text_system));

        // A scene has shaped text before its nicknames arrive: warm Typst up.
        let start = Instant::now();
        shape_glyph(
            world.resource::<gaanim_text::font::FontRegistry>(),
            'x',
            &body.font_family,
            None,
            body.size,
        );
        let warm_up = start.elapsed();
        let start = Instant::now();
        schedule.run(&mut world);
        let first = start.elapsed();

        // A quiz's clock changes every frame; new counts and standings
        // arrive once a second (every 60 frames).
        let frames = 6000;
        let start = Instant::now();
        for frame in 0..frames {
            let mut results = world.resource_mut::<PollResults>();
            *results.remaining.get_mut("p0").unwrap() = 20.0 - frame as f64 / 60.0;
            if frame % 60 == 0 {
                results.counts.get_mut("p0").unwrap()[0] += 1;
                results.leaderboard.rotate_left(1);
            }
            schedule.run(&mut world);
        }
        let per_frame = start.elapsed() / frames;
        println!("Typst warm-up: {warm_up:?}; first frame: {first:?}; per frame: {per_frame:?}");
    }
}
