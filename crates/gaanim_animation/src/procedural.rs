//! Procedural motion layered over authored animation.
//!
//! A [`ProceduralMotion`] adds jitter or periodic motion that is a pure
//! function of timeline time. It is applied to the local transform and
//! opacity right before hierarchy propagation and removed right after, so
//! only the propagated world state carries it. Timeline animations, snapshots,
//! and seeks keep seeing the authored values, `move_to` and similar clips
//! combine with it additively, and any frame is reproducible from its time.

use std::sync::Arc;

use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec3};
use gaanim_core::peniko::{Brush, Color};
use gaanim_math::{Noise, RateFunc, SpatialTransform};
use gaanim_scene::{Opacity, StrokeBrush};

use crate::signals::FloatSignal;
use crate::updaters::PlaybackState;

/// One tween of a parameter signal, as the timeline evaluates it.
#[derive(Debug, Clone)]
pub struct SignalTween {
    pub start: f64,
    pub duration: f64,
    pub from: f64,
    pub to: f64,
    pub rate: RateFunc,
}

impl SignalTween {
    fn end(&self) -> f64 {
        self.start + self.duration
    }

    fn value_at(&self, time: f64) -> f64 {
        let t = if self.duration <= 0.0 || time >= self.end() {
            self.rate.evaluate(1.0)
        } else {
            self.rate
                .evaluate(((time - self.start) / self.duration).clamp(0.0, 1.0))
        };
        self.from + (self.to - self.from) * t
    }
}

/// The values a parameter signal takes from a layer's start: `base` until
/// its first tween, then each tween in start order, as a seek applies them.
#[derive(Debug, Clone)]
pub struct SignalTrack {
    pub base: f64,
    /// Tweens sorted by start time.
    pub tweens: Vec<SignalTween>,
}

impl SignalTrack {
    pub fn new(base: f64, mut tweens: Vec<SignalTween>) -> Self {
        tweens.sort_by(|a, b| a.start.total_cmp(&b.start));
        Self { base, tweens }
    }

    pub fn value_at(&self, time: f64) -> f64 {
        let mut value = self.base;
        for tween in &self.tweens {
            if tween.start > time {
                break;
            }
            value = tween.value_at(time);
        }
        value
    }

    /// `∫ value dt` over `[from, to]`: exact where the value holds still,
    /// Simpson's rule inside each tween. A pure function of its bounds, so a
    /// seek and a playback reach the same angle or phase.
    pub fn integral(&self, from: f64, to: f64) -> f64 {
        if to <= from {
            return 0.0;
        }
        let mut breaks = vec![from, to];
        for tween in &self.tweens {
            for time in [tween.start, tween.end()] {
                if time > from && time < to {
                    breaks.push(time);
                }
            }
        }
        breaks.sort_by(f64::total_cmp);
        breaks.dedup();
        let mut total = 0.0;
        for window in breaks.windows(2) {
            let (a, b) = (window[0], window[1]);
            let middle = 0.5 * (a + b);
            let moving = self
                .tweens
                .iter()
                .rev()
                .find(|tween| tween.start <= middle)
                .is_some_and(|tween| tween.duration > 0.0 && middle < tween.end());
            if !moving {
                total += self.value_at(middle) * (b - a);
                continue;
            }
            const STEPS: usize = 32;
            let step = (b - a) / STEPS as f64;
            let mut sum = self.value_at(a) + self.value_at(b);
            for index in 1..STEPS {
                let weight = if index % 2 == 1 { 4.0 } else { 2.0 };
                sum += weight * self.value_at(a + step * index as f64);
            }
            total += sum * step / 3.0;
        }
        total
    }
}

/// A number of a procedural layer: fixed, or a `Parameter` whose animation
/// changes the layer while it runs.
#[derive(Debug, Clone)]
pub enum LayerParam {
    Fixed(f64),
    Signal {
        /// Authoring id of the parameter until compiled, then its compiled id.
        id: ObjectId,
        /// The signal entity, read for the live value.
        entity: Option<Entity>,
        /// The signal's values from the layer's start, for integrals.
        track: Option<Arc<SignalTrack>>,
    },
}

impl PartialEq for LayerParam {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Fixed(a), Self::Fixed(b)) => a.to_bits() == b.to_bits(),
            (
                Self::Signal {
                    id: a,
                    entity: ea,
                    track: ta,
                },
                Self::Signal {
                    id: b,
                    entity: eb,
                    track: tb,
                },
            ) => {
                a == b
                    && ea == eb
                    && match (ta, tb) {
                        (Some(ta), Some(tb)) => Arc::ptr_eq(ta, tb),
                        (None, None) => true,
                        _ => false,
                    }
            }
            _ => false,
        }
    }
}

impl From<f64> for LayerParam {
    fn from(value: f64) -> Self {
        Self::Fixed(value)
    }
}

impl LayerParam {
    pub fn signal(id: ObjectId) -> Self {
        Self::Signal {
            id,
            entity: None,
            track: None,
        }
    }

    /// The fixed value, if it is one.
    pub fn fixed(&self) -> Option<f64> {
        match self {
            Self::Fixed(value) => Some(*value),
            Self::Signal { .. } => None,
        }
    }

    /// The value at `time`: the signal's live value when it can be read.
    fn value(&self, time: f64, live: &dyn Fn(Entity) -> Option<f64>) -> f64 {
        match self {
            Self::Fixed(value) => *value,
            Self::Signal { entity, track, .. } => entity
                .and_then(live)
                .or_else(|| track.as_ref().map(|track| track.value_at(time)))
                .unwrap_or(0.0),
        }
    }

    /// `∫ value dt` over `[from, to]`.
    fn integral(&self, from: f64, to: f64, live: &dyn Fn(Entity) -> Option<f64>) -> f64 {
        match self {
            Self::Fixed(value) => value * (to - from),
            Self::Signal {
                track: Some(track), ..
            } => track.integral(from, to),
            Self::Signal { .. } => self.value(to, live) * (to - from),
        }
    }
}

/// Periodic shape used by [`ProceduralLayer::Oscillate`], mapped to `[0, 1]`
/// and starting at 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Square,
    Triangle,
    Saw,
}

impl Waveform {
    pub fn sample(self, cycles: f64) -> f64 {
        let phase = cycles.rem_euclid(1.0);
        match self {
            Self::Sine => 0.5 - 0.5 * (phase * std::f64::consts::TAU).cos(),
            Self::Square => {
                if phase < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
            Self::Triangle => 1.0 - (2.0 * phase - 1.0).abs(),
            Self::Saw => phase,
        }
    }
}

/// Channel driven by [`ProceduralLayer::Oscillate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscillatedChannel {
    /// Added to the x translation.
    X,
    /// Added to the y translation.
    Y,
    /// Added to the z rotation, in radians.
    Rotation,
    /// Multiplies the scale.
    Scale,
    /// Multiplies the opacity.
    Opacity,
}

// Built once per scene or clip, not stored in bulk: boxing the large
// variant would only add indirection.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ProceduralLayer {
    /// Organic jitter: seeded fBm noise on position, rotation, and scale.
    /// A fixed `frequency` lives in `noise`; a signal one keeps the noise at
    /// frequency 1 and advances it by the integrated frequency, so changing
    /// it changes the pace without jumping.
    Wiggle {
        noise: Noise,
        position: LayerParam,
        rotation: LayerParam,
        scale: LayerParam,
        frequency: LayerParam,
    },
    /// A turn about z at `speed` radians per second; with a signal speed the
    /// angle is its integral, so it speeds up and slows down smoothly.
    Spin { speed: LayerParam },
    /// Periodic value between `low` and `high`.
    Oscillate {
        channel: OscillatedChannel,
        waveform: Waveform,
        frequency: f64,
        low: f64,
        high: f64,
        phase: f64,
    },
}

impl ProceduralLayer {
    /// The numbers of the layer that a parameter may drive.
    pub fn params_mut(&mut self) -> Vec<&mut LayerParam> {
        match self {
            Self::Wiggle {
                position,
                rotation,
                scale,
                frequency,
                ..
            } => vec![position, rotation, scale, frequency],
            Self::Spin { speed } => vec![speed],
            Self::Oscillate { .. } => Vec::new(),
        }
    }
}

/// A layer active from `start` until `end` in absolute timeline seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledLayer {
    pub layer: ProceduralLayer,
    pub start: f64,
    pub end: Option<f64>,
}

/// Additive offset produced by the active layers at one instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProceduralOffset {
    pub translation: DVec3,
    pub rotation: f64,
    pub scale: f64,
    pub opacity: f64,
}

impl Default for ProceduralOffset {
    fn default() -> Self {
        Self {
            translation: DVec3::ZERO,
            rotation: 0.0,
            scale: 1.0,
            opacity: 1.0,
        }
    }
}

#[derive(Component, Debug, Clone, Default)]
pub struct ProceduralMotion {
    pub layers: Vec<ScheduledLayer>,
    applied: Option<(SpatialTransform, Option<f32>)>,
}

impl ProceduralMotion {
    pub fn push(&mut self, layer: ProceduralLayer, start: f64) {
        self.layers.push(ScheduledLayer {
            layer,
            start,
            end: None,
        });
    }

    /// Ends every open layer at `time`.
    pub fn stop_at(&mut self, time: f64) {
        for layer in &mut self.layers {
            if layer.end.is_none() && layer.start <= time {
                layer.end = Some(time);
            }
        }
    }

    /// Combined offset of the layers active at `time`, with fixed values
    /// only; signal parameters read through [`Self::offset_with`].
    pub fn offset_at(&self, time: f64) -> ProceduralOffset {
        self.offset_with(time, &|_| None)
    }

    /// Combined offset of the layers active at `time`; `live` reads the
    /// current value of a parameter signal.
    pub fn offset_with(&self, time: f64, live: &dyn Fn(Entity) -> Option<f64>) -> ProceduralOffset {
        let mut offset = ProceduralOffset::default();
        for scheduled in &self.layers {
            if time < scheduled.start {
                continue;
            }
            // A removed spin keeps the angle it reached, like the `rotate`
            // updater; every other layer ends with its removal.
            if let ProceduralLayer::Spin { speed } = &scheduled.layer {
                let until = scheduled.end.map_or(time, |end| time.min(end));
                offset.rotation += speed.integral(scheduled.start, until, live);
                continue;
            }
            if scheduled.end.is_some_and(|end| time >= end) {
                continue;
            }
            let local = time - scheduled.start;
            match &scheduled.layer {
                ProceduralLayer::Wiggle {
                    noise,
                    position,
                    rotation,
                    scale,
                    frequency,
                } => {
                    let x = match frequency {
                        LayerParam::Fixed(_) => local,
                        LayerParam::Signal { .. } => {
                            frequency.integral(scheduled.start, time, live)
                        }
                    };
                    // Subtracting the value at the layer's start avoids a jump.
                    let channel = |index| noise.at_time(x, index) - noise.at_time(0.0, index);
                    let position = position.value(time, live);
                    offset.translation.x += position * channel(0);
                    offset.translation.y += position * channel(1);
                    offset.rotation += rotation.value(time, live) * channel(2);
                    offset.scale *= 1.0 + scale.value(time, live) * channel(3);
                }
                ProceduralLayer::Spin { .. } => {}
                ProceduralLayer::Oscillate {
                    channel,
                    waveform,
                    frequency,
                    low,
                    high,
                    phase,
                } => {
                    let value = low + (high - low) * waveform.sample(phase + local * frequency);
                    match channel {
                        OscillatedChannel::X => offset.translation.x += value,
                        OscillatedChannel::Y => offset.translation.y += value,
                        OscillatedChannel::Rotation => offset.rotation += value,
                        OscillatedChannel::Scale => offset.scale *= value,
                        OscillatedChannel::Opacity => offset.opacity *= value,
                    }
                }
            }
        }
        offset
    }
}

/// Adds the procedural offset to local state before propagation.
pub fn apply_procedural_motion_system(
    playback: Option<Res<PlaybackState>>,
    mut query: Query<(
        &mut ProceduralMotion,
        &mut SpatialTransform,
        Option<&mut Opacity>,
    )>,
    signals: Query<&FloatSignal>,
) {
    let time = playback.map_or(0.0, |state| state.current_time);
    let live = |entity: Entity| signals.get(entity).ok().map(|signal| signal.value);
    for (mut motion, mut transform, opacity) in &mut query {
        let offset = motion.offset_with(time, &live);
        let base = *transform;
        transform.translation += offset.translation;
        transform.rotation = base.rotation * DQuat::from_rotation_z(offset.rotation);
        transform.scale = base.scale * offset.scale;
        let base_opacity = opacity.map(|mut opacity| {
            let authored = opacity.0;
            opacity.0 = (authored as f64 * offset.opacity).clamp(0.0, 1.0) as f32;
            authored
        });
        motion.applied = Some((base, base_opacity));
    }
}

/// Restores the authored local state after propagation.
pub fn restore_procedural_motion_system(
    mut query: Query<(
        &mut ProceduralMotion,
        &mut SpatialTransform,
        Option<&mut Opacity>,
    )>,
) {
    for (mut motion, mut transform, opacity) in &mut query {
        let Some((base, base_opacity)) = motion.applied.take() else {
            continue;
        };
        *transform = base;
        if let (Some(mut opacity), Some(value)) = (opacity, base_opacity) {
            opacity.0 = value;
        }
    }
}

/// Dashes that flow along a stroke at a constant speed ("marching ants").
///
/// Each run advances the dash pattern by `speed` scene units per second
/// from its start until it stops, where the pattern stays. The shift is a
/// pure function of timeline time, added to the stroke's authored
/// `dash_offset` from bounds until the start of the next frame, so both the
/// live renderer and exports that extract after the frame see it.
#[derive(Component, Debug, Clone, Default)]
pub struct DashFlow {
    /// `(speed, start, end)` of every run, in timeline seconds.
    pub runs: Vec<(f64, f64, Option<f64>)>,
    /// Authored and shifted dash offsets while the shifted one is shown.
    applied: Option<(f64, f64)>,
}

impl DashFlow {
    pub fn push(&mut self, speed: f64, start: f64) {
        self.runs.push((speed, start, None));
    }

    /// Stops every open run at `time`.
    pub fn stop_at(&mut self, time: f64) {
        for (_, start, end) in &mut self.runs {
            if end.is_none() && *start <= time {
                *end = Some(time);
            }
        }
    }

    /// Distance the dashes have travelled along the path by `time`.
    pub fn travel_at(&self, time: f64) -> f64 {
        self.runs
            .iter()
            .map(|(speed, start, end)| {
                let until = end.map_or(time, |end| time.min(end));
                speed * (until - start).max(0.0)
            })
            .sum()
    }
}

/// Adds the travelled distance to the stroke right before extraction.
pub fn apply_dash_flow_system(
    playback: Option<Res<PlaybackState>>,
    mut query: Query<(&mut DashFlow, &mut StrokeBrush)>,
) {
    let time = playback.map_or(0.0, |state| state.current_time);
    for (mut flow, mut stroke) in &mut query {
        // A larger dash offset moves the pattern back toward the start, so
        // a positive speed subtracts to flow forward along the path.
        let shift = -flow.travel_at(time);
        if shift != 0.0 {
            let authored = stroke.style.dash_offset;
            let shifted = authored + shift;
            stroke.style.dash_offset = shifted;
            flow.applied = Some((authored, shifted));
        }
    }
}

/// Restores the authored dash offset at the start of the next frame, before
/// seeks and tweens read it.
///
/// The exact authored value is restored (subtracting the shift could drift),
/// and only when nothing rewrote the shifted one in between, such as a seek
/// issued outside the schedule.
pub fn restore_dash_flow_system(mut query: Query<(&mut DashFlow, &mut StrokeBrush)>) {
    for (mut flow, mut stroke) in &mut query {
        if let Some((authored, shifted)) = flow.applied.take()
            && stroke.style.dash_offset.to_bits() == shifted.to_bits()
        {
            stroke.style.dash_offset = authored;
        }
    }
}

/// A stroke color that cycles through `colors`, like an animated boundary.
///
/// The color is a pure function of timeline time: `rate` full turns through
/// the list per second from `start`, blending each color into the next and the
/// last into the first. It replaces the stroke color right before extraction
/// and is restored at the start of the next frame, like [`DashFlow`].
#[derive(Component, Debug, Clone)]
pub struct StrokeCycle {
    pub colors: Vec<Color>,
    /// Turns through the whole list per second.
    pub rate: f64,
    pub start: f64,
    /// Authored and cycled brushes while the cycled one is shown.
    applied: Option<(Option<Brush>, Brush)>,
}

impl StrokeCycle {
    pub fn new(colors: Vec<Color>, rate: f64, start: f64) -> Self {
        Self {
            colors,
            rate,
            start,
            applied: None,
        }
    }

    /// The color at `time`; the first one until `start`.
    pub fn color_at(&self, time: f64) -> Color {
        match self.colors.len() {
            0 => Color::TRANSPARENT,
            1 => self.colors[0],
            count => {
                let phase =
                    ((time - self.start).max(0.0) * self.rate).rem_euclid(1.0) * count as f64;
                let index = (phase.floor() as usize).min(count - 1);
                gaanim_core::interpolate_color(
                    self.colors[index],
                    self.colors[(index + 1) % count],
                    phase - index as f64,
                )
            }
        }
    }
}

/// Puts the cycled color on the stroke right before extraction.
pub fn apply_stroke_cycle_system(
    playback: Option<Res<PlaybackState>>,
    mut query: Query<(&mut StrokeCycle, &mut StrokeBrush)>,
) {
    let time = playback.map_or(0.0, |state| state.current_time);
    for (mut cycle, mut stroke) in &mut query {
        if cycle.colors.is_empty() {
            continue;
        }
        let authored = stroke.brush.clone();
        let cycled = Brush::Solid(cycle.color_at(time));
        stroke.brush = Some(cycled.clone());
        cycle.applied = Some((authored, cycled));
    }
}

/// Restores the authored stroke at the start of the next frame, and only when
/// nothing rewrote the cycled one in between, such as a seek.
pub fn restore_stroke_cycle_system(mut query: Query<(&mut StrokeCycle, &mut StrokeBrush)>) {
    for (mut cycle, mut stroke) in &mut query {
        if let Some((authored, cycled)) = cycle.applied.take()
            && stroke.brush.as_ref() == Some(&cycled)
        {
            stroke.brush = authored;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiggle() -> ProceduralLayer {
        ProceduralLayer::Wiggle {
            noise: Noise::new(1, 2.0, 1.0, 2),
            position: 0.1.into(),
            rotation: 0.05.into(),
            scale: 0.0.into(),
            frequency: 2.0.into(),
        }
    }

    #[test]
    fn offsets_are_pure_functions_of_time() {
        let mut motion = ProceduralMotion::default();
        motion.push(wiggle(), 1.0);
        motion.push(
            ProceduralLayer::Oscillate {
                channel: OscillatedChannel::Opacity,
                waveform: Waveform::Triangle,
                frequency: 0.5,
                low: 0.4,
                high: 1.0,
                phase: 0.0,
            },
            0.0,
        );
        assert_eq!(motion.offset_at(0.5).translation, DVec3::ZERO);
        assert_eq!(motion.offset_at(1.0).translation, DVec3::ZERO);
        let sample = motion.offset_at(2.3);
        assert_eq!(sample, motion.offset_at(2.3));
        assert!(sample.translation.length() > 0.0);
        assert!(sample.translation.x.abs() <= 0.2 && sample.rotation.abs() <= 0.1);
        // Triangle at 0.5 Hz: low at t = 0, high at t = 1.
        assert!((motion.offset_at(0.0).opacity - 0.4).abs() < 1e-12);
        assert!((motion.offset_at(1.0).opacity - 1.0).abs() < 1e-12);

        motion.stop_at(3.0);
        assert_eq!(motion.offset_at(3.5), ProceduralOffset::default());
    }

    fn ramp(start: f64, duration: f64, from: f64, to: f64) -> SignalTween {
        SignalTween {
            start,
            duration,
            from,
            to,
            rate: RateFunc::Linear,
        }
    }

    #[test]
    fn signal_tracks_integrate_holds_and_tweens_exactly() {
        // 1 rad/s until t = 2, a linear ramp to 3 rad/s over [2, 4], then a
        // cut to 0 at t = 5.
        let track = SignalTrack::new(
            1.0,
            vec![ramp(5.0, 0.0, 3.0, 0.0), ramp(2.0, 2.0, 1.0, 3.0)],
        );
        assert_eq!(track.value_at(1.0), 1.0);
        assert_eq!(track.value_at(3.0), 2.0);
        assert_eq!(track.value_at(4.5), 3.0);
        assert_eq!(track.value_at(6.0), 0.0);
        // 2 + 4 (ramp average 2 over 2 s) + 3 + 0.
        assert!((track.integral(0.0, 7.0) - 9.0).abs() < 1e-9);
        // Additive over any split, which keeps seeks consistent.
        let split = track.integral(0.0, 3.3) + track.integral(3.3, 7.0);
        assert!((split - track.integral(0.0, 7.0)).abs() < 1e-9);
        assert_eq!(track.integral(3.0, 3.0), 0.0);
    }

    #[test]
    fn spin_speed_and_wiggle_amplitude_follow_their_signals() {
        let track = Arc::new(SignalTrack::new(0.0, vec![ramp(1.0, 1.0, 0.0, 2.0)]));
        let speed = LayerParam::Signal {
            id: ObjectId::from_raw(7),
            entity: None,
            track: Some(track),
        };
        let mut motion = ProceduralMotion::default();
        motion.push(ProceduralLayer::Spin { speed }, 0.0);
        assert_eq!(motion.offset_at(1.0).rotation, 0.0);
        // The ramp averages 1 rad/s over [1, 2], then holds 2 rad/s.
        assert!((motion.offset_at(2.0).rotation - 1.0).abs() < 1e-9);
        assert!((motion.offset_at(3.0).rotation - 3.0).abs() < 1e-9);
        // A fixed speed keeps turning at its rate, and holds its angle once
        // removed.
        let mut fixed = ProceduralMotion::default();
        fixed.push(ProceduralLayer::Spin { speed: 0.5.into() }, 1.0);
        assert!((fixed.offset_at(3.0).rotation - 1.0).abs() < 1e-12);
        fixed.stop_at(3.0);
        assert!((fixed.offset_at(5.0).rotation - 1.0).abs() < 1e-12);

        // A live amplitude scales the jitter; at zero it is still.
        let entity = Entity::from_raw_u32(3).unwrap();
        let mut jitter = ProceduralMotion::default();
        jitter.push(
            ProceduralLayer::Wiggle {
                noise: Noise::new(1, 2.0, 1.0, 2),
                position: LayerParam::Signal {
                    id: ObjectId::from_raw(8),
                    entity: Some(entity),
                    track: None,
                },
                rotation: 0.0.into(),
                scale: 0.0.into(),
                frequency: 2.0.into(),
            },
            0.0,
        );
        let at = |amplitude: f64| jitter.offset_with(2.3, &|_| Some(amplitude)).translation;
        assert_eq!(at(0.0), DVec3::ZERO);
        assert!((at(0.2) - 2.0 * at(0.1)).length() < 1e-12);
        assert!(at(0.1).length() > 0.0);
    }

    #[test]
    fn waveforms_span_zero_to_one() {
        for waveform in [
            Waveform::Sine,
            Waveform::Square,
            Waveform::Triangle,
            Waveform::Saw,
        ] {
            assert_eq!(waveform.sample(0.0), 0.0);
            for step in 0..100 {
                let value = waveform.sample(step as f64 / 37.0);
                assert!((0.0..=1.0).contains(&value));
            }
        }
        assert!((Waveform::Sine.sample(0.5) - 1.0).abs() < 1e-12);
        assert_eq!(Waveform::Square.sample(0.75), 1.0);
    }

    #[test]
    fn layer_touches_only_propagated_state() {
        let mut world = World::new();
        world.insert_resource(PlaybackState {
            current_time: 2.0,
            ..Default::default()
        });
        let mut motion = ProceduralMotion::default();
        motion.push(
            ProceduralLayer::Oscillate {
                channel: OscillatedChannel::X,
                waveform: Waveform::Saw,
                frequency: 1.0,
                low: 0.0,
                high: 1.0,
                phase: 0.25,
            },
            0.0,
        );
        let entity = world
            .spawn((motion, SpatialTransform::new_2d(3.0, 0.0), Opacity(1.0)))
            .id();
        let mut apply = IntoSystem::into_system(apply_procedural_motion_system);
        apply.initialize(&mut world);
        apply.run((), &mut world).unwrap();
        let moved = world.get::<SpatialTransform>(entity).unwrap().translation.x;
        assert!((moved - 3.25).abs() < 1e-12);
        let mut restore = IntoSystem::into_system(restore_procedural_motion_system);
        restore.initialize(&mut world);
        restore.run((), &mut world).unwrap();
        assert_eq!(
            world.get::<SpatialTransform>(entity).unwrap().translation.x,
            3.0
        );
    }

    #[test]
    fn dash_flow_travels_with_time_and_touches_only_extracted_state() {
        let mut flow = DashFlow::default();
        flow.push(0.5, 1.0);
        assert_eq!(flow.travel_at(0.5), 0.0);
        assert!((flow.travel_at(3.0) - 1.0).abs() < 1e-12);
        flow.stop_at(2.0);
        // A stopped run keeps the dashes where they were.
        assert!((flow.travel_at(9.0) - 0.5).abs() < 1e-12);

        let mut world = World::new();
        world.insert_resource(PlaybackState {
            current_time: 9.0,
            ..Default::default()
        });
        let mut stroke = StrokeBrush::new(gaanim_core::peniko::Color::WHITE, 0.1);
        stroke.style = stroke.style.with_dashes(0.25, [0.2, 0.1]);
        let entity = world.spawn((flow, stroke)).id();
        let mut apply = IntoSystem::into_system(apply_dash_flow_system);
        apply.initialize(&mut world);
        apply.run((), &mut world).unwrap();
        let offset = world.get::<StrokeBrush>(entity).unwrap().style.dash_offset;
        assert!((offset + 0.25).abs() < 1e-12);
        let mut restore = IntoSystem::into_system(restore_dash_flow_system);
        restore.initialize(&mut world);
        restore.run((), &mut world).unwrap();
        assert_eq!(
            world.get::<StrokeBrush>(entity).unwrap().style.dash_offset,
            0.25
        );
    }

    #[test]
    fn dash_flow_restores_the_authored_offset_bit_for_bit() {
        let mut world = World::new();
        world.insert_resource(PlaybackState::default());
        let mut flow = DashFlow::default();
        flow.push(0.3, 0.0);
        flow.push(-0.7, 0.5);
        let mut stroke = StrokeBrush::new(gaanim_core::peniko::Color::WHITE, 0.1);
        stroke.style = stroke.style.with_dashes(0.1, [0.2, 0.1]);
        let entity = world.spawn((flow, stroke)).id();
        let mut apply = IntoSystem::into_system(apply_dash_flow_system);
        apply.initialize(&mut world);
        let mut restore = IntoSystem::into_system(restore_dash_flow_system);
        restore.initialize(&mut world);
        for frame in 0..10_000 {
            world.resource_mut::<PlaybackState>().current_time = frame as f64 / 60.0;
            apply.run((), &mut world).unwrap();
            restore.run((), &mut world).unwrap();
            let offset = world.get::<StrokeBrush>(entity).unwrap().style.dash_offset;
            assert_eq!(offset.to_bits(), 0.1f64.to_bits(), "frame {frame}");
        }
    }

    #[test]
    fn dash_flow_keeps_an_offset_rewritten_between_frames() {
        let mut world = World::new();
        world.insert_resource(PlaybackState {
            current_time: 2.0,
            ..Default::default()
        });
        let mut flow = DashFlow::default();
        flow.push(1.0, 0.0);
        let entity = world
            .spawn((
                flow,
                StrokeBrush::new(gaanim_core::peniko::Color::WHITE, 0.1),
            ))
            .id();
        let mut apply = IntoSystem::into_system(apply_dash_flow_system);
        apply.initialize(&mut world);
        let mut restore = IntoSystem::into_system(restore_dash_flow_system);
        restore.initialize(&mut world);
        apply.run((), &mut world).unwrap();
        // The shifted offset stays for extraction after the frame.
        assert_eq!(
            world.get::<StrokeBrush>(entity).unwrap().style.dash_offset,
            -2.0
        );
        // A seek outside the schedule restores another authored offset.
        world
            .get_mut::<StrokeBrush>(entity)
            .unwrap()
            .style
            .dash_offset = 0.75;
        restore.run((), &mut world).unwrap();
        assert_eq!(
            world.get::<StrokeBrush>(entity).unwrap().style.dash_offset,
            0.75
        );
    }

    #[test]
    fn a_stroke_cycle_blends_through_its_colors_and_wraps() {
        let red = Color::from_rgb8(255, 0, 0);
        let green = Color::from_rgb8(0, 255, 0);
        let blue = Color::from_rgb8(0, 0, 255);
        let cycle = StrokeCycle::new(vec![red, green, blue], 0.5, 1.0);
        assert_eq!(cycle.color_at(0.0), red, "the first color before the start");
        assert_eq!(cycle.color_at(1.0), red);
        // A turn takes 2 s: the second color is a third of the way through.
        let second = cycle.color_at(1.0 + 2.0 / 3.0).to_rgba8();
        assert!(second.g > 250 && second.r < 6 && second.b < 6, "{second:?}");
        assert_eq!(
            cycle.color_at(3.0).to_rgba8(),
            red.to_rgba8(),
            "a whole turn returns"
        );
        let wrapping = cycle.color_at(1.0 + 2.0 * 5.0 / 6.0).to_rgba8();
        assert!(
            wrapping.b > 100 && wrapping.r > 100,
            "blue blends back into red: {wrapping:?}"
        );
        assert_eq!(
            cycle.color_at(1.7),
            cycle.color_at(1.7),
            "a pure function of time"
        );
    }

    #[test]
    fn a_stroke_cycle_replaces_the_stroke_and_restores_it() {
        let mut world = World::new();
        world.insert_resource(PlaybackState {
            current_time: 1.0,
            ..Default::default()
        });
        let red = Color::from_rgb8(255, 0, 0);
        let blue = Color::from_rgb8(0, 0, 255);
        let white = Color::from_rgb8(255, 255, 255);
        let entity = world
            .spawn((
                StrokeCycle::new(vec![red, blue], 0.25, 0.0),
                StrokeBrush::new(white, 0.05),
            ))
            .id();
        let mut apply = Schedule::default();
        apply.add_systems(apply_stroke_cycle_system);
        apply.run(&mut world);
        assert_eq!(
            world.get::<StrokeBrush>(entity).unwrap().brush,
            Some(Brush::Solid(
                StrokeCycle::new(vec![red, blue], 0.25, 0.0).color_at(1.0)
            ))
        );
        let mut restore = Schedule::default();
        restore.add_systems(restore_stroke_cycle_system);
        restore.run(&mut world);
        assert_eq!(
            world.get::<StrokeBrush>(entity).unwrap().brush,
            Some(Brush::Solid(white))
        );
    }
}
