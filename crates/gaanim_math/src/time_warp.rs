//! Time warps of a whole composition: speed ramps and time remapping.
//!
//! A warp is a monotone map between authored seconds (the composition as
//! written) and played seconds (as it runs). Clips keep their authored
//! timing; the timeline places them at their played times and wraps their
//! easing, so a seek still evaluates a pure function of time.

use crate::RateFunc;

/// Samples that check a remapping easing never goes back.
const MONOTONE_SAMPLES: usize = 2048;

/// How a composition's time is warped, before its span is known.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TimeWarpSpec {
    /// Playback speed at fractions of the authored span, joined linearly
    /// (`1` is normal speed, `0.25` a quarter).
    SpeedRamp(Vec<(f64, f64)>),
    /// Authored progress as an easing of played progress; the span keeps
    /// its length.
    Remap(RateFunc),
}

impl TimeWarpSpec {
    /// Speeds keyed by fractions in `[0, 1]`, each positive. Before the
    /// first key and after the last one the speed holds.
    pub fn speed_ramp(mut keys: Vec<(f64, f64)>) -> Result<Self, String> {
        if keys.is_empty() {
            return Err("speed_ramp() needs at least one speed".into());
        }
        if keys
            .iter()
            .any(|(at, _)| !(at.is_finite() && (0.0..=1.0).contains(at)))
        {
            return Err("speed_ramp() keys are fractions of the composition, from 0 to 1".into());
        }
        if keys
            .iter()
            .any(|(_, speed)| !(speed.is_finite() && *speed > 0.0))
        {
            return Err("speed_ramp() speeds must be positive".into());
        }
        keys.sort_by(|a, b| a.0.total_cmp(&b.0));
        if keys.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("speed_ramp() has two speeds at the same instant".into());
        }
        Ok(Self::SpeedRamp(keys))
    }

    /// Played progress mapped through `easing` to authored progress. The
    /// easing must start at 0, end at 1 and never decrease.
    pub fn remap(easing: RateFunc) -> Result<Self, String> {
        let error = || {
            "time_remap() needs an easing that goes from 0 to 1 without going back; \
             Back, Elastic and springs overshoot"
                .to_string()
        };
        let mut previous = easing.evaluate(0.0);
        if previous.abs() > 1e-6 || (easing.evaluate(1.0) - 1.0).abs() > 1e-6 {
            return Err(error());
        }
        for index in 1..=MONOTONE_SAMPLES {
            let value = easing.evaluate(index as f64 / MONOTONE_SAMPLES as f64);
            if !value.is_finite()
                || value < previous - 1e-9
                || !(-1e-9..=1.0 + 1e-9).contains(&value)
            {
                return Err(error());
            }
            previous = value;
        }
        Ok(Self::Remap(easing))
    }

    /// The warp of a composition that lasts `span` authored seconds.
    pub fn build(&self, span: f64) -> TimeWarp {
        let span = span.max(0.0);
        match self {
            Self::SpeedRamp(keys) => {
                let mut stops = keys.clone();
                if stops[0].0 > 0.0 {
                    stops.insert(0, (0.0, stops[0].1));
                }
                let last = stops[stops.len() - 1];
                if last.0 < 1.0 {
                    stops.push((1.0, last.1));
                }
                if stops.len() == 1 {
                    stops.push((1.0, stops[0].1));
                }
                let mut played = vec![0.0];
                for pair in stops.windows(2) {
                    let elapsed =
                        played[played.len() - 1] + ramp_played(span, pair[0], pair[1], pair[1].0);
                    played.push(elapsed);
                }
                TimeWarp::Ramp {
                    span,
                    stops,
                    played,
                }
            }
            Self::Remap(easing) => TimeWarp::Remap {
                span,
                easing: easing.clone(),
            },
        }
    }
}

/// Played seconds from the start of the ramp segment `from`–`to` to the
/// authored fraction `at` within it, for a composition of `span` seconds.
fn ramp_played(span: f64, from: (f64, f64), to: (f64, f64), at: f64) -> f64 {
    let (u0, s0) = from;
    let (u1, s1) = to;
    let length = u1 - u0;
    if length <= 0.0 {
        return 0.0;
    }
    let du = at - u0;
    if (s1 - s0).abs() < 1e-12 {
        return span * du / s0;
    }
    // ∫ du / s(u) with s linear in u.
    let speed = s0 + (s1 - s0) * du / length;
    span * length / (s1 - s0) * (speed / s0).ln()
}

/// A warp of a composition with a known authored span.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TimeWarp {
    Ramp {
        span: f64,
        /// Authored fraction and speed, from 0 to 1.
        stops: Vec<(f64, f64)>,
        /// Played seconds at each stop.
        played: Vec<f64>,
    },
    Remap {
        span: f64,
        easing: RateFunc,
    },
}

impl TimeWarp {
    pub fn authored_span(&self) -> f64 {
        match self {
            Self::Ramp { span, .. } | Self::Remap { span, .. } => *span,
        }
    }

    /// Played length of the whole composition.
    pub fn played_span(&self) -> f64 {
        match self {
            Self::Ramp { played, .. } => played[played.len() - 1],
            Self::Remap { span, .. } => *span,
        }
    }

    /// Played seconds at `authored` seconds. Outside the span time runs at
    /// the speed of its nearest end.
    pub fn played(&self, authored: f64) -> f64 {
        let span = self.authored_span();
        if span <= 0.0 {
            return authored;
        }
        match self {
            Self::Ramp { stops, played, .. } => {
                if authored <= 0.0 {
                    return authored / stops[0].1;
                }
                if authored >= span {
                    return played[played.len() - 1] + (authored - span) / stops[stops.len() - 1].1;
                }
                let at = authored / span;
                let index = stops[1..stops.len() - 1].partition_point(|stop| stop.0 <= at);
                played[index] + ramp_played(span, stops[index], stops[index + 1], at)
            }
            Self::Remap { easing, .. } => {
                if authored <= 0.0 || authored >= span {
                    return authored;
                }
                // The easing gives authored from played: invert by bisection,
                // keeping the earliest instant across flat stretches.
                let target = authored / span;
                let (mut low, mut high) = (0.0_f64, 1.0_f64);
                for _ in 0..60 {
                    let mid = 0.5 * (low + high);
                    if easing.evaluate(mid) < target {
                        low = mid;
                    } else {
                        high = mid;
                    }
                }
                high * span
            }
        }
    }

    /// Authored seconds at `played` seconds; the inverse of [`Self::played`].
    pub fn authored(&self, played_at: f64) -> f64 {
        let span = self.authored_span();
        if span <= 0.0 {
            return played_at;
        }
        match self {
            Self::Ramp { stops, played, .. } => {
                let total = played[played.len() - 1];
                if played_at <= 0.0 {
                    return played_at * stops[0].1;
                }
                if played_at >= total {
                    return span + (played_at - total) * stops[stops.len() - 1].1;
                }
                let index = played[1..played.len() - 1].partition_point(|time| *time <= played_at);
                let (u0, s0) = stops[index];
                let (u1, s1) = stops[index + 1];
                let elapsed = played_at - played[index];
                let length = u1 - u0;
                let du = if (s1 - s0).abs() < 1e-12 {
                    elapsed * s0 / span
                } else {
                    let speed = s0 * (elapsed * (s1 - s0) / (span * length)).exp();
                    (speed - s0) * length / (s1 - s0)
                };
                ((u0 + du.clamp(0.0, length)) * span).clamp(0.0, span)
            }
            Self::Remap { easing, .. } => {
                if played_at <= 0.0 || played_at >= span {
                    return played_at;
                }
                easing.evaluate(played_at / span) * span
            }
        }
    }
}

/// One step of a [`TimeMap`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TimeStage {
    Shift(f64),
    Scale(f64),
    Warp(TimeWarp),
}

/// Where a clip's authored seconds land when played: offsets, stretches
/// and the warps of every composition around it, innermost first.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TimeMap(pub Vec<TimeStage>);

impl TimeMap {
    pub fn push(&mut self, stage: TimeStage) {
        self.0.push(stage);
    }

    pub fn played(&self, authored: f64) -> f64 {
        self.0.iter().fold(authored, |time, stage| match stage {
            TimeStage::Shift(offset) => time + offset,
            TimeStage::Scale(factor) => time * factor,
            TimeStage::Warp(warp) => warp.played(time),
        })
    }

    pub fn authored(&self, played: f64) -> f64 {
        self.0.iter().rev().fold(played, |time, stage| match stage {
            TimeStage::Shift(offset) => time - offset,
            TimeStage::Scale(factor) if *factor != 0.0 => time / factor,
            TimeStage::Scale(_) => 0.0,
            TimeStage::Warp(warp) => warp.authored(time),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_constant_ramp_scales_time() {
        let warp = TimeWarpSpec::speed_ramp(vec![(0.0, 0.5)])
            .unwrap()
            .build(2.0);
        assert!(close(warp.played_span(), 4.0));
        assert!(close(warp.played(1.0), 2.0));
        assert!(close(warp.authored(3.0), 1.5));
    }

    #[test]
    fn a_slow_motion_ramp_lengthens_the_middle_and_inverts_exactly() {
        let keys = vec![(0.0, 1.0), (0.4, 0.15), (0.6, 0.15), (1.0, 1.0)];
        let warp = TimeWarpSpec::speed_ramp(keys).unwrap().build(2.0);
        // Hold segment: 0.4 s authored at 0.15x is 0.4 / 0.15 played.
        let hold = warp.played(1.2) - warp.played(0.8);
        assert!(close(hold, 0.4 / 0.15));
        // Ramp segments: span * Δu / (s1 - s0) * ln(s1 / s0).
        let ramp = 2.0 * 0.4 / (0.15 - 1.0) * (0.15_f64).ln();
        assert!(close(warp.played(0.8), ramp));
        assert!(close(warp.played_span(), 2.0 * ramp + 0.4 / 0.15));
        for step in 0..=200 {
            let authored = 2.0 * step as f64 / 200.0;
            assert!(close(warp.authored(warp.played(authored)), authored));
        }
        // Monotone.
        let mut previous = -1.0;
        for step in 0..=200 {
            let played = warp.played(2.0 * step as f64 / 200.0);
            assert!(played > previous);
            previous = played;
        }
    }

    #[test]
    fn a_remap_keeps_the_span_and_follows_its_easing() {
        let warp = TimeWarpSpec::remap(RateFunc::EaseInOut(crate::EasingCurve::Cubic))
            .unwrap()
            .build(3.0);
        assert!(close(warp.played_span(), 3.0));
        // Ease in: little authored time passes at first.
        assert!(warp.authored(0.3) < 0.3);
        for step in 1..100 {
            let played = 3.0 * step as f64 / 100.0;
            assert!((warp.played(warp.authored(played)) - played).abs() < 1e-9);
        }
    }

    #[test]
    fn remaps_must_not_go_back_and_ramps_must_move() {
        assert!(
            TimeWarpSpec::remap(RateFunc::Back {
                overshoot: 1.7,
                mode: crate::EaseMode::Out
            })
            .is_err()
        );
        assert!(TimeWarpSpec::remap(RateFunc::ThereAndBack).is_err());
        assert!(TimeWarpSpec::remap(RateFunc::Steps(4)).is_ok());
        assert!(TimeWarpSpec::speed_ramp(vec![(0.5, 0.0)]).is_err());
        assert!(TimeWarpSpec::speed_ramp(vec![(1.5, 1.0)]).is_err());
        assert!(TimeWarpSpec::speed_ramp(vec![(0.5, 1.0), (0.5, 2.0)]).is_err());
        assert!(TimeWarpSpec::speed_ramp(Vec::new()).is_err());
    }

    #[test]
    fn maps_compose_shifts_scales_and_warps() {
        let warp = TimeWarpSpec::speed_ramp(vec![(0.0, 2.0)])
            .unwrap()
            .build(4.0);
        let map = TimeMap(vec![
            TimeStage::Shift(1.0),
            TimeStage::Warp(warp),
            TimeStage::Scale(3.0),
            TimeStage::Shift(0.5),
        ]);
        // (1 + 1) / 2 * 3 + 0.5
        assert!(close(map.played(1.0), 3.5));
        assert!(close(map.authored(3.5), 1.0));
    }

    #[test]
    fn a_zero_span_warp_is_the_identity() {
        let warp = TimeWarpSpec::speed_ramp(vec![(0.0, 0.25)])
            .unwrap()
            .build(0.0);
        assert_eq!(warp.played(0.0), 0.0);
        assert_eq!(warp.authored(0.0), 0.0);
    }
}
