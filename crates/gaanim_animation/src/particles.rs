//! Deterministic particle emitters as ECS data.
//!
//! The compiler inserts a [`ParticleEmitter`] on the group of an emitter and
//! marks its layer entities with [`ParticleLayer`]; the timeline samples the
//! anchor trail and the renderer rebuilds the layer paths every frame.

use bevy::prelude::{Component, Entity};

/// Opacity levels a fading particle emitter draws in: a particle with fade
/// opacity `alpha` falls in the layer of level `ceil(alpha * levels)`.
pub const PARTICLE_FADE_LEVELS: usize = 5;

/// Where an anchored emitter was, sampled by the timeline on a fixed grid of
/// timeline times so every particle leaves from where its anchor was at its
/// birth, at any seek.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnchorTrail {
    /// Seconds between samples; 0 for a single constant sample.
    pub step: f64,
    /// Grid index of the first sample: its time is `first * step`.
    pub first: i64,
    pub points: Vec<gaanim_core::glam::DVec2>,
}

impl AnchorTrail {
    /// A trail that stays at `point`.
    pub fn constant(point: gaanim_core::glam::DVec2) -> Self {
        Self {
            step: 0.0,
            first: 0,
            points: vec![point],
        }
    }

    /// The anchor at `time`, interpolated between samples and held beyond
    /// them; the origin without samples.
    pub fn at(&self, time: f64) -> gaanim_core::glam::DVec2 {
        match self.points.as_slice() {
            [] => gaanim_core::glam::DVec2::ZERO,
            [point] => *point,
            points => {
                if self.step.is_nan() || self.step <= 0.0 {
                    return points[0];
                }
                let last = (points.len() - 1) as f64;
                let x = (time / self.step - self.first as f64).clamp(0.0, last);
                let index = (x.floor() as usize).min(points.len() - 2);
                let t = x - index as f64;
                points[index].lerp(points[index + 1], t)
            }
        }
    }
}

/// Deterministic particles drawn into the layer entities of their group,
/// rebuilt from the timeline time in `SceneSet::DerivedGeometry`.
///
/// Layer `color * levels + level` draws the particles of that color whose
/// fade opacity falls in `level` (see [`PARTICLE_FADE_LEVELS`]); its own
/// fill and opacity give them their look.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ParticleEmitter {
    pub system: gaanim_math::ParticleSystem,
    /// Emitter position in scene coordinates, relative to the anchor.
    pub position: gaanim_core::glam::DVec2,
    pub layers: Vec<Entity>,
    /// Fade levels per color; 1 without fade.
    pub levels: usize,
    /// Timeline id of the drawable the emitter follows.
    pub anchor: Option<gaanim_core::ObjectId>,
    /// Anchor positions written by the timeline.
    pub trail: AnchorTrail,
    /// Scratch entity the timeline evaluates the anchor into.
    pub probe: Option<Entity>,
}

impl ParticleEmitter {
    /// Where particles born at `birth` leave from.
    pub fn origin_at(&self, birth: f64) -> gaanim_core::glam::DVec2 {
        if self.anchor.is_some() {
            self.position + self.trail.at(birth)
        } else {
            self.position
        }
    }

    /// The layer a particle of `color` and fade opacity `alpha` draws in.
    pub fn layer_of(&self, color: usize, alpha: f64) -> usize {
        let levels = self.levels.max(1);
        let level = ((alpha * levels as f64).ceil() as usize).clamp(1, levels) - 1;
        color * levels + level
    }

    /// The outline of every layer at `time`, in scene coordinates.
    pub fn outlines(&self, time: f64) -> Vec<gaanim_core::kurbo::BezPath> {
        let mut paths = vec![gaanim_core::kurbo::BezPath::new(); self.layers.len()];
        self.system.for_each_live(
            time,
            |birth| self.origin_at(birth),
            |particle| {
                if let Some(path) = paths.get_mut(self.layer_of(particle.color, particle.alpha)) {
                    gaanim_math::particles::append_particle(path, self.system.shape, &particle);
                }
            },
        );
        paths
    }
}

/// Marks a layer entity drawn by a [`ParticleEmitter`].
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ParticleLayer;

#[cfg(test)]
mod particle_tests {
    use super::*;
    use gaanim_core::glam::DVec2;

    #[test]
    fn trails_interpolate_and_layers_split_by_fade() {
        let trail = AnchorTrail {
            step: 0.5,
            first: 2,
            points: vec![
                DVec2::new(0.0, 0.0),
                DVec2::new(1.0, 2.0),
                DVec2::new(3.0, 2.0),
            ],
        };
        assert_eq!(trail.at(0.0), DVec2::ZERO);
        assert_eq!(trail.at(1.25), DVec2::new(0.5, 1.0));
        assert_eq!(trail.at(1.75), DVec2::new(2.0, 2.0));
        assert_eq!(trail.at(9.0), DVec2::new(3.0, 2.0));
        assert_eq!(AnchorTrail::constant(DVec2::X).at(4.0), DVec2::X);

        let emitter = ParticleEmitter {
            system: gaanim_math::ParticleSystem {
                colors: 2,
                bursts: vec![(0.0, 40)],
                rate: 0.0,
                ..Default::default()
            },
            position: DVec2::new(1.0, 0.0),
            layers: vec![Entity::PLACEHOLDER; 2 * PARTICLE_FADE_LEVELS],
            levels: PARTICLE_FADE_LEVELS,
            anchor: None,
            trail: AnchorTrail::default(),
            probe: None,
        };
        assert_eq!(emitter.layer_of(0, 1.0), PARTICLE_FADE_LEVELS - 1);
        assert_eq!(emitter.layer_of(1, 0.01), PARTICLE_FADE_LEVELS);
        assert_eq!(emitter.origin_at(3.0), DVec2::new(1.0, 0.0));
        let outlines = emitter.outlines(0.1);
        assert_eq!(outlines.len(), 2 * PARTICLE_FADE_LEVELS);
        assert!(outlines.iter().any(|path| !path.elements().is_empty()));
        assert!(
            emitter
                .outlines(5.0)
                .iter()
                .all(|path| path.elements().is_empty())
        );
    }
}
