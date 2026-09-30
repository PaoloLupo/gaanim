//! Bar chart races: interpolated keyframes, smoothed ranks and slot layout.
//!
//! Everything here is a pure function of a fractional keyframe `position`:
//! `0.0` is the first keyframe, `1.5` lies halfway between the second and the
//! third. Values interpolate linearly between keyframes. A bar's rank is the
//! number of bars ahead of it. When two bars cross, their swap is eased over
//! `rank_smoothing` keyframes centered on the crossing, and shortened so it
//! never overlaps the previous or next crossing of the same two bars; away
//! from crossings every bar sits in its own whole slot. Nothing accumulates
//! between evaluations, so seeking to a position always gives the same state
//! as playing up to it.

use std::hash::{Hash, Hasher};

/// Invalid bar race data or options.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BarRaceError {
    #[error("a bar race needs at least one keyframe and one bar")]
    Empty,
    #[error("keyframe {frame} has {actual} values for {expected} bars")]
    Ragged {
        frame: usize,
        expected: usize,
        actual: usize,
    },
    #[error("bar race values must be finite")]
    NonFinite,
    #[error("bar race names must be unique; {0:?} repeats")]
    DuplicateName(String),
    #[error("top must be at least 1")]
    Top,
    #[error("rank_smoothing must be finite and between 0 and 2 keyframes")]
    Smoothing,
    #[error("invalid value format {0:?}: {1}")]
    Format(String, &'static str),
}

/// Longest swap between two bars, in keyframes. A longer swap would lag the
/// data it animates by more than two keyframe intervals.
pub const MAX_RANK_SMOOTHING: f64 = 2.0;

/// Offset used to read which bar leads on each side of a crossing or a tie.
const SIDE: f64 = 1e-7;

/// Values of every bar at one position, with their smoothed ranks.
#[derive(Debug, Clone, PartialEq)]
pub struct BarRaceState {
    /// Interpolated value of each bar, in bar order.
    pub values: Vec<f64>,
    /// Smoothed rank of each bar: `0.0` leads, `1.0` is second, and a bar
    /// halfway through overtaking the leader sits at `0.5`.
    pub ranks: Vec<f64>,
    /// Largest value, the length of a full bar; `1.0` when no value is positive.
    pub maximum: f64,
}

impl BarRaceState {
    /// Fraction of the full bar length for bar `index`, clamped to `[0, 1]`.
    pub fn fraction(&self, index: usize) -> f64 {
        (self.values[index] / self.maximum).clamp(0.0, 1.0)
    }
}

/// Keyframed data of a bar chart race; see the module documentation.
#[derive(Debug, Clone, PartialEq)]
pub struct BarRaceModel {
    names: Vec<String>,
    /// `values[frame][bar]`.
    values: Vec<Vec<f64>>,
    top: usize,
    rank_smoothing: f64,
    /// Sorted positions where the lead of each pair changes, at
    /// `behind * bar_count + ahead` for `behind < ahead`.
    crossings: Vec<Vec<f64>>,
}

impl BarRaceModel {
    /// `values[frame][bar]` holds the value of bar `names[bar]` at each keyframe.
    pub fn new(
        names: Vec<String>,
        values: Vec<Vec<f64>>,
        top: usize,
        rank_smoothing: f64,
    ) -> Result<Self, BarRaceError> {
        if names.is_empty() || values.is_empty() {
            return Err(BarRaceError::Empty);
        }
        for (index, name) in names.iter().enumerate() {
            if names[..index].contains(name) {
                return Err(BarRaceError::DuplicateName(name.clone()));
            }
        }
        for (frame, row) in values.iter().enumerate() {
            if row.len() != names.len() {
                return Err(BarRaceError::Ragged {
                    frame,
                    expected: names.len(),
                    actual: row.len(),
                });
            }
            if row.iter().any(|value| !value.is_finite()) {
                return Err(BarRaceError::NonFinite);
            }
        }
        if top == 0 {
            return Err(BarRaceError::Top);
        }
        if !rank_smoothing.is_finite() || !(0.0..=MAX_RANK_SMOOTHING).contains(&rank_smoothing) {
            return Err(BarRaceError::Smoothing);
        }
        let mut model = Self {
            names,
            values,
            top,
            rank_smoothing,
            crossings: Vec::new(),
        };
        let count = model.names.len();
        let mut crossings = vec![Vec::new(); count * count];
        for behind in 0..count {
            for ahead in (behind + 1)..count {
                crossings[behind * count + ahead] = model.pair_crossings(ahead, behind);
            }
        }
        model.crossings = crossings;
        Ok(model)
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn frame_count(&self) -> usize {
        self.values.len()
    }

    pub fn bar_count(&self) -> usize {
        self.names.len()
    }

    /// Number of ranks shown; bars ranked lower fade out below the last slot.
    pub fn top(&self) -> usize {
        self.top
    }

    pub fn rank_smoothing(&self) -> f64 {
        self.rank_smoothing
    }

    /// Position of the last keyframe.
    pub fn last_position(&self) -> f64 {
        (self.values.len() - 1) as f64
    }

    /// Value of every bar at one keyframe.
    pub fn keyframe(&self, frame: usize) -> &[f64] {
        &self.values[frame]
    }

    /// Linearly interpolated value of `bar`; positions outside the keyframes
    /// hold the first or the last value.
    pub fn value(&self, bar: usize, position: f64) -> f64 {
        let (frame, t) = self.segment(position);
        let start = self.values[frame][bar];
        if t == 0.0 {
            return start;
        }
        start + (self.values[frame + 1][bar] - start) * t
    }

    /// Keyframe index and fraction towards the next one.
    fn segment(&self, position: f64) -> (usize, f64) {
        let last = self.values.len() - 1;
        if last == 0 || position.is_nan() || position <= 0.0 {
            return (0, 0.0);
        }
        if position >= last as f64 {
            return (last, 0.0);
        }
        let frame = position.floor() as usize;
        (frame, position - frame as f64)
    }

    /// Whether `ahead` ranks above `behind` when their values are `a` and `b`:
    /// larger values lead, and ties go to the bar listed first.
    fn leads(ahead: usize, behind: usize, difference: f64) -> bool {
        difference > 0.0 || (difference == 0.0 && ahead < behind)
    }

    fn difference(&self, ahead: usize, behind: usize, position: f64) -> f64 {
        self.value(ahead, position) - self.value(behind, position)
    }

    /// Whether `ahead` leads `behind` at exactly `position`.
    fn leads_at(&self, ahead: usize, behind: usize, position: f64) -> bool {
        Self::leads(ahead, behind, self.difference(ahead, behind, position))
    }

    /// Positions strictly between the first and last keyframes where the
    /// lead of the pair changes. Values are linear between keyframes, so the
    /// lead can only change where the difference crosses zero inside a
    /// segment, or at a keyframe where it is zero.
    fn pair_crossings(&self, ahead: usize, behind: usize) -> Vec<f64> {
        let last = self.values.len() - 1;
        let at = |frame: usize| self.values[frame][ahead] - self.values[frame][behind];
        let mut candidates = Vec::new();
        for frame in 0..last {
            let (d0, d1) = (at(frame), at(frame + 1));
            if (d0 > 0.0 && d1 < 0.0) || (d0 < 0.0 && d1 > 0.0) {
                candidates.push(frame as f64 + d0 / (d0 - d1));
            }
            if d1 == 0.0 && frame + 1 < last {
                candidates.push((frame + 1) as f64);
            }
        }
        // A tie the pair touches and leaves on the same side is no crossing.
        candidates.retain(|&position| {
            self.leads_at(ahead, behind, position - SIDE)
                != self.leads_at(ahead, behind, position + SIDE)
        });
        candidates
    }

    /// Whether `ahead` leads `behind` at `position` when no swap is under
    /// way. At a tie this is the order the pair is heading into (the order it
    /// arrives in at the last keyframe), so a momentary tie never flips it.
    fn settled_lead(&self, ahead: usize, behind: usize, position: f64) -> bool {
        let difference = self.difference(ahead, behind, position);
        if difference != 0.0 {
            return difference > 0.0;
        }
        let side = if position + SIDE < self.last_position() {
            self.difference(ahead, behind, position + SIDE)
        } else {
            self.difference(ahead, behind, position - SIDE)
        };
        Self::leads(ahead, behind, side)
    }

    /// Eased share in `[0, 1]` of the slot swap by which `ahead` leads `behind`.
    fn lead_share(&self, ahead: usize, behind: usize, position: f64) -> f64 {
        let settled = if self.settled_lead(ahead, behind, position) {
            1.0
        } else {
            0.0
        };
        let crossings = &self.crossings[behind * self.names.len() + ahead];
        if self.rank_smoothing == 0.0 || crossings.is_empty() {
            return settled;
        }
        // The crossing nearest to the position.
        let next = crossings.partition_point(|&crossing| crossing < position);
        let index = match (next.checked_sub(1), crossings.get(next)) {
            (Some(previous), Some(&following))
                if position - crossings[previous] > following - position =>
            {
                next
            }
            (Some(previous), _) => previous,
            (None, _) => next,
        };
        let crossing = crossings[index];
        // Swaps of one pair never overlap: each ends before the next begins.
        let mut half = self.rank_smoothing * 0.5;
        if index > 0 {
            half = half.min((crossing - crossings[index - 1]) * 0.5);
        }
        if let Some(&following) = crossings.get(index + 1) {
            half = half.min((following - crossing) * 0.5);
        }
        let offset = position - crossing;
        if half <= 0.0 || offset.abs() >= half {
            return settled;
        }
        let t = offset / (2.0 * half) + 0.5;
        // Smoothstep is symmetric, so the two shares of a pair still add up to
        // one and the ranks of all bars remain a blend of permutations.
        let eased = t * t * (3.0 - 2.0 * t);
        if self.leads_at(ahead, behind, crossing + SIDE) {
            eased
        } else {
            1.0 - eased
        }
    }

    /// Values, smoothed ranks and full-bar length at `position`.
    pub fn state(&self, position: f64) -> BarRaceState {
        let count = self.names.len();
        let values = (0..count)
            .map(|bar| self.value(bar, position))
            .collect::<Vec<_>>();
        let mut ranks = vec![0.0; count];
        for behind in 0..count {
            for ahead in (behind + 1)..count {
                let share = self.lead_share(ahead, behind, position);
                ranks[behind] += share;
                ranks[ahead] += 1.0 - share;
            }
        }
        let maximum = values.iter().copied().fold(0.0, f64::max);
        BarRaceState {
            values,
            ranks,
            maximum: if maximum > 0.0 { maximum } else { 1.0 },
        }
    }

    /// Opacity of a bar at a smoothed `rank`: opaque in the shown ranks,
    /// fading out as it moves from the last slot to the first hidden one.
    pub fn visibility(&self, rank: f64) -> f64 {
        (self.top as f64 - rank).clamp(0.0, 1.0)
    }

    /// Stable content hash, used to describe reactive callbacks for hot reload.
    pub fn fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.names.hash(&mut hasher);
        for row in &self.values {
            for value in row {
                value.to_bits().hash(&mut hasher);
            }
        }
        self.top.hash(&mut hasher);
        self.rank_smoothing.to_bits().hash(&mut hasher);
        hasher.finish()
    }
}

/// Linear interpolation of numeric keyframe labels such as years.
pub fn interpolate_label(labels: &[f64], position: f64) -> f64 {
    let Some(last) = labels.len().checked_sub(1) else {
        return f64::NAN;
    };
    if last == 0 || position.is_nan() || position <= 0.0 {
        return labels[0];
    }
    if position >= last as f64 {
        return labels[last];
    }
    let frame = position.floor() as usize;
    let t = position - frame as f64;
    labels[frame] + (labels[frame + 1] - labels[frame]) * t
}

/// Opacity of the text label of keyframe `frame`: fully shown while it is
/// the nearest keyframe, crossfading with its neighbor over `fade` keyframes
/// around the midpoint.
pub fn label_opacity(frame: usize, position: f64, fade: f64) -> f64 {
    let distance = (position - frame as f64).abs();
    if fade <= 0.0 {
        return if distance < 0.5 { 1.0 } else { 0.0 };
    }
    ((0.5 - distance) / fade + 0.5).clamp(0.0, 1.0)
}

/// Slots of a bar race of `top` ranks inside a `height` tall area centered
/// on the origin, with the leader at the top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarRaceSlots {
    pub height: f64,
    pub top: usize,
    /// Fraction of each slot left empty between bars, in `[0, 1)`.
    pub gap: f64,
}

impl BarRaceSlots {
    pub fn slot_height(&self) -> f64 {
        self.height / self.top as f64
    }

    pub fn bar_thickness(&self) -> f64 {
        self.slot_height() * (1.0 - self.gap)
    }

    /// Vertical center of a bar at a (smoothed) `rank`.
    pub fn center_y(&self, rank: f64) -> f64 {
        self.height * 0.5 - (rank + 0.5) * self.slot_height()
    }
}

/// A Python-style number format such as `"{:,.0f}"` or `"${:,.1f} M"`,
/// reduced to what a rolling number can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueFormat {
    pub prefix: String,
    pub suffix: String,
    pub decimals: usize,
    /// Thousands separator: `,` or `_`, if any.
    pub group_separator: Option<char>,
}

impl Default for ValueFormat {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            suffix: String::new(),
            decimals: 0,
            group_separator: Some(','),
        }
    }
}

impl ValueFormat {
    /// Parse one replacement field `{}` or `{:[,|_][.N][f|d]}` with literal
    /// text around it. `N` is at most 6.
    pub fn parse(format: &str) -> Result<Self, BarRaceError> {
        let error = |reason| BarRaceError::Format(format.to_owned(), reason);
        let open = format.find('{').ok_or_else(|| error("missing {} field"))?;
        let close = format[open..]
            .find('}')
            .map(|index| open + index)
            .ok_or_else(|| error("unclosed {"))?;
        let (prefix, suffix) = (&format[..open], &format[close + 1..]);
        if prefix.contains(['{', '}']) || suffix.contains(['{', '}']) {
            return Err(error("use exactly one {} field"));
        }
        let field = &format[open + 1..close];
        let spec = match field.strip_prefix(':') {
            Some(spec) => spec,
            None if field.is_empty() => "",
            None => return Err(error("the field takes no name or index")),
        };
        let mut rest = spec;
        let group_separator = match rest.chars().next() {
            Some(separator @ (',' | '_')) => {
                rest = &rest[1..];
                Some(separator)
            }
            _ => None,
        };
        let mut decimals = 0;
        if let Some(precision) = rest.strip_prefix('.') {
            let digits = precision
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(precision.len());
            if digits == 0 {
                return Err(error("missing precision after ."));
            }
            decimals = precision[..digits]
                .parse()
                .map_err(|_| error("invalid precision"))?;
            rest = &precision[digits..];
        }
        match rest {
            "" | "f" | "F" => {}
            "d" if decimals == 0 => {}
            _ => return Err(error("supported fields are {}, {:,.Nf}, {:_.Nf} and {:,d}")),
        }
        if decimals > 6 {
            return Err(error("at most 6 decimals"));
        }
        Ok(Self {
            prefix: prefix.to_owned(),
            suffix: suffix.to_owned(),
            decimals,
            group_separator,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(values: Vec<Vec<f64>>, smoothing: f64) -> BarRaceModel {
        let names = (0..values[0].len()).map(|i| format!("bar {i}")).collect();
        BarRaceModel::new(names, values, 2, smoothing).unwrap()
    }

    #[test]
    fn rejects_invalid_data() {
        let names = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(
            BarRaceModel::new(names.clone(), vec![], 2, 0.3),
            Err(BarRaceError::Empty)
        );
        assert!(matches!(
            BarRaceModel::new(names.clone(), vec![vec![1.0]], 2, 0.3),
            Err(BarRaceError::Ragged { frame: 0, .. })
        ));
        assert_eq!(
            BarRaceModel::new(names.clone(), vec![vec![1.0, f64::NAN]], 2, 0.3),
            Err(BarRaceError::NonFinite)
        );
        assert_eq!(
            BarRaceModel::new(names.clone(), vec![vec![1.0, 2.0]], 0, 0.3),
            Err(BarRaceError::Top)
        );
        assert_eq!(
            BarRaceModel::new(names.clone(), vec![vec![1.0, 2.0]], 1, -0.1),
            Err(BarRaceError::Smoothing)
        );
        assert_eq!(
            BarRaceModel::new(names.clone(), vec![vec![1.0, 2.0]], 1, 2.5),
            Err(BarRaceError::Smoothing)
        );
        assert!(matches!(
            BarRaceModel::new(vec!["a".into(), "a".into()], vec![vec![1.0, 2.0]], 1, 0.0),
            Err(BarRaceError::DuplicateName(_))
        ));
    }

    #[test]
    fn values_interpolate_between_keyframes_and_hold_outside() {
        let race = model(vec![vec![0.0, 10.0], vec![4.0, 10.0], vec![8.0, 0.0]], 0.0);
        assert_eq!(race.value(0, -1.0), 0.0);
        assert_eq!(race.value(0, 0.5), 2.0);
        assert_eq!(race.value(0, 1.0), 4.0);
        assert_eq!(race.value(1, 1.5), 5.0);
        assert_eq!(race.value(0, 7.0), 8.0);
        assert_eq!(race.state(2.0).maximum, 8.0);
        assert_eq!(race.state(2.0).fraction(1), 0.0);
        let single = model(vec![vec![3.0, 1.0]], 0.3);
        assert_eq!(single.value(0, 0.7), 3.0);
        assert_eq!(single.state(0.7).ranks, vec![0.0, 1.0]);
    }

    #[test]
    fn ranks_follow_values_and_ties_keep_the_listed_order() {
        let race = model(vec![vec![1.0, 2.0, 2.0]], 0.0);
        assert_eq!(race.state(0.0).ranks, vec![2.0, 0.0, 1.0]);
        let zeros = model(vec![vec![0.0, 0.0]], 0.0);
        assert_eq!(zeros.state(0.0).maximum, 1.0);
    }

    #[test]
    fn overtakes_are_smoothed_symmetrically_around_the_crossing() {
        // Bar 1 overtakes bar 0 exactly at position 0.5.
        let race = model(vec![vec![2.0, 0.0], vec![0.0, 2.0]], 0.4);
        let before = race.state(0.2);
        assert_eq!(before.ranks, vec![0.0, 1.0]);
        let crossing = race.state(0.5);
        assert!((crossing.ranks[0] - 0.5).abs() < 1e-12);
        assert!((crossing.ranks[1] - 0.5).abs() < 1e-12);
        let during = race.state(0.6);
        assert!(during.ranks[0] > 0.5 && during.ranks[0] < 1.0);
        assert!((during.ranks[0] + during.ranks[1] - 1.0).abs() < 1e-12);
        assert_eq!(race.state(0.8).ranks, vec![1.0, 0.0]);
        // Without smoothing the swap is instant.
        let instant = model(vec![vec![2.0, 0.0], vec![0.0, 2.0]], 0.0);
        assert_eq!(instant.state(0.49).ranks, vec![0.0, 1.0]);
        assert_eq!(instant.state(0.51).ranks, vec![1.0, 0.0]);
    }

    #[test]
    fn swaps_centered_on_a_keyframe_span_both_segments() {
        // The crossing sits on keyframe 1; the swap covers two segments.
        let race = model(vec![vec![3.0, 0.0], vec![1.0, 1.0], vec![0.0, 3.0]], 1.0);
        let state = race.state(1.0);
        // Ties go to bar 0 only at a single instant, so the shares are even.
        assert!((state.ranks[0] - 0.5).abs() < 1e-12);
        assert!(race.state(0.75).ranks[0] < 0.5);
        assert!(race.state(1.25).ranks[0] > 0.5);
    }

    #[test]
    fn wide_smoothing_never_merges_bars_between_crossings() {
        // Four bars reverse their order at every midpoint, and the swaps of
        // each pair are shortened so they never overlap the next crossing.
        let forward = vec![40.0, 30.0, 20.0, 10.0];
        let backward = vec![10.0, 20.0, 30.0, 40.0];
        let race = model(
            vec![
                forward.clone(),
                backward.clone(),
                forward.clone(),
                backward,
                forward,
            ],
            MAX_RANK_SMOOTHING,
        );
        for keyframe in 0..5 {
            let mut ranks = race.state(keyframe as f64).ranks;
            ranks.sort_by(f64::total_cmp);
            assert_eq!(ranks, vec![0.0, 1.0, 2.0, 3.0], "keyframe {keyframe}");
        }
        for crossing in [0.5, 1.5, 2.5, 3.5] {
            for offset in [-0.45, -0.3, -0.185, -0.15, 0.15, 0.185, 0.3, 0.45] {
                let position = crossing + offset;
                let mut ranks = race.state(position).ranks;
                ranks.sort_by(f64::total_cmp);
                for pair in ranks.windows(2) {
                    assert!(
                        pair[1] - pair[0] > 0.3,
                        "bars share a slot at {position}: {ranks:?}"
                    );
                }
            }
        }
        let mut settled = race.state(1.685).ranks;
        settled.sort_by(f64::total_cmp);
        assert!(settled.windows(2).all(|pair| pair[1] - pair[0] > 0.5));
    }

    #[test]
    fn momentary_ties_do_not_flip_the_order() {
        // Bar 1 touches bar 0 at keyframe 1 and falls back: no crossing.
        let race = model(vec![vec![3.0, 1.0], vec![2.0, 2.0], vec![3.0, 1.0]], 1.0);
        for position in [0.0, 0.9, 1.0, 1.1, 2.0] {
            assert_eq!(race.state(position).ranks, vec![0.0, 1.0], "{position}");
        }
        // Bar 1 ties the leader from keyframe 1 on; bar 0, listed first,
        // stays ahead through the tie.
        let race = model(vec![vec![3.0, 1.0], vec![2.0, 2.0], vec![2.0, 2.0]], 0.0);
        assert_eq!(race.state(1.5).ranks, vec![0.0, 1.0]);
        assert_eq!(race.state(2.0).ranks, vec![0.0, 1.0]);
    }

    #[test]
    fn states_are_pure_functions_of_position() {
        let race = model(
            vec![
                vec![5.0, 1.0, 3.0],
                vec![1.0, 6.0, 2.0],
                vec![2.0, 2.0, 7.0],
            ],
            0.3,
        );
        let forward = (0..=40)
            .map(|step| race.state(step as f64 * 0.05))
            .collect::<Vec<_>>();
        // Evaluating in any order reproduces the same state bit for bit.
        for (step, expected) in forward.iter().enumerate().rev() {
            assert_eq!(&race.state(step as f64 * 0.05), expected);
        }
        for state in forward {
            let total: f64 = state.ranks.iter().sum();
            assert!((total - 3.0).abs() < 1e-9, "ranks stay a permutation blend");
        }
    }

    #[test]
    fn visibility_fades_bars_leaving_the_top() {
        let race = model(vec![vec![1.0, 2.0, 3.0]], 0.3);
        assert_eq!(race.visibility(0.0), 1.0);
        assert_eq!(race.visibility(1.0), 1.0);
        assert_eq!(race.visibility(1.5), 0.5);
        assert_eq!(race.visibility(2.0), 0.0);
        assert_eq!(race.visibility(3.0), 0.0);
    }

    #[test]
    fn slots_stack_from_the_top() {
        let slots = BarRaceSlots {
            height: 6.0,
            top: 3,
            gap: 0.25,
        };
        assert_eq!(slots.slot_height(), 2.0);
        assert_eq!(slots.bar_thickness(), 1.5);
        assert_eq!(slots.center_y(0.0), 2.0);
        assert_eq!(slots.center_y(2.0), -2.0);
        assert_eq!(slots.center_y(0.5), 1.0);
    }

    #[test]
    fn labels_interpolate_and_crossfade() {
        assert_eq!(interpolate_label(&[2000.0, 2001.0, 2003.0], 1.5), 2002.0);
        assert_eq!(interpolate_label(&[2000.0], 3.0), 2000.0);
        assert_eq!(interpolate_label(&[2000.0, 2001.0], 9.0), 2001.0);
        assert_eq!(label_opacity(1, 1.0, 0.1), 1.0);
        assert_eq!(label_opacity(1, 1.5, 0.1), 0.5);
        assert_eq!(label_opacity(2, 1.5, 0.1), 0.5);
        assert_eq!(label_opacity(1, 1.6, 0.1), 0.0);
        assert_eq!(label_opacity(0, 0.4, 0.0), 1.0);
    }

    #[test]
    fn value_formats_follow_python_format_fields() {
        assert_eq!(
            ValueFormat::parse("{:,.0f}").unwrap(),
            ValueFormat::default()
        );
        assert_eq!(
            ValueFormat::parse("${:_.2f} M").unwrap(),
            ValueFormat {
                prefix: "$".into(),
                suffix: " M".into(),
                decimals: 2,
                group_separator: Some('_'),
            }
        );
        assert_eq!(
            ValueFormat::parse("{}").unwrap(),
            ValueFormat {
                group_separator: None,
                ..ValueFormat::default()
            }
        );
        assert_eq!(ValueFormat::parse("{:,d}").unwrap(), ValueFormat::default());
        for invalid in [
            "", "{", "{0}", "{:.f}", "{:.7f}", "{:e}", "{:.0%}", "{}{}", "{:.2d}",
        ] {
            assert!(ValueFormat::parse(invalid).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn fingerprints_change_with_content() {
        let first = model(vec![vec![1.0, 2.0]], 0.3);
        let second = model(vec![vec![1.0, 2.5]], 0.3);
        assert_eq!(first.fingerprint(), first.clone().fingerprint());
        assert_ne!(first.fingerprint(), second.fingerprint());
    }
}
