//! Procedural motion layered over authored animation.
//!
//! A [`ProceduralMotion`] adds jitter or periodic motion that is a pure
//! function of timeline time. It is applied to the local transform and
//! opacity right before hierarchy propagation and removed right after, so
//! only the propagated world state carries it. Timeline animations, snapshots,
//! and seeks keep seeing the authored values, `move_to` and similar clips
//! combine with it additively, and any frame is reproducible from its time.

use bevy::prelude::*;
use gaanim_core::glam::{DQuat, DVec3};
use gaanim_math::{Noise, SpatialTransform};
use gaanim_scene::Opacity;

use crate::updaters::PlaybackState;

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

#[derive(Debug, Clone, PartialEq)]
pub enum ProceduralLayer {
    /// Organic jitter: seeded fBm noise on position, rotation, and scale.
    Wiggle {
        noise: Noise,
        position: f64,
        rotation: f64,
        scale: f64,
    },
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

    /// Combined offset of the layers active at `time`.
    pub fn offset_at(&self, time: f64) -> ProceduralOffset {
        let mut offset = ProceduralOffset::default();
        for scheduled in &self.layers {
            if time < scheduled.start || scheduled.end.is_some_and(|end| time >= end) {
                continue;
            }
            let local = time - scheduled.start;
            match &scheduled.layer {
                ProceduralLayer::Wiggle {
                    noise,
                    position,
                    rotation,
                    scale,
                } => {
                    // Subtracting the value at the layer's start avoids a jump.
                    let channel = |index| noise.at_time(local, index) - noise.at_time(0.0, index);
                    offset.translation.x += position * channel(0);
                    offset.translation.y += position * channel(1);
                    offset.rotation += rotation * channel(2);
                    offset.scale *= 1.0 + scale * channel(3);
                }
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
) {
    let time = playback.map_or(0.0, |state| state.current_time);
    for (mut motion, mut transform, opacity) in &mut query {
        let offset = motion.offset_at(time);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wiggle() -> ProceduralLayer {
        ProceduralLayer::Wiggle {
            noise: Noise::new(1, 2.0, 1.0, 2),
            position: 0.1,
            rotation: 0.05,
            scale: 0.0,
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
}
