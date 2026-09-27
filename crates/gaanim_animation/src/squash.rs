//! Squash and stretch driven by a drawable's own velocity.

use bevy::prelude::{Component, Entity};
use gaanim_core::glam::DVec2;
use gaanim_core::kurbo::Affine;

/// Half the time step, in seconds, of the centred difference that measures
/// a squashing drawable's velocity.
pub const SQUASH_STEP: f64 = 1.0 / 60.0;

/// Stretch a drawable along its velocity and squash it across, keeping its
/// area: the stretch is `1 + amount * speed` (speed in scene units per
/// second), capped at `max_ratio`, and the squash its reciprocal.
///
/// The timeline measures the velocity from the drawable's own animations at
/// `t ± SQUASH_STEP`, a pure function of time, and writes the result as a
/// [`gaanim_scene::ShapeDeform`], so a drawable at rest is never deformed.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct SquashStretch {
    pub amount: f64,
    pub max_ratio: f64,
    /// Scratch entity the timeline evaluates the drawable into.
    pub probe: Option<Entity>,
}

impl SquashStretch {
    pub fn new(amount: f64, max_ratio: f64) -> Self {
        Self {
            amount,
            max_ratio,
            probe: None,
        }
    }

    /// Area-preserving stretch along `velocity`, about the origin.
    pub fn deform(&self, velocity: DVec2) -> Affine {
        let speed = velocity.length();
        let ratio = (1.0 + self.amount * speed).clamp(1.0, self.max_ratio.max(1.0));
        if !speed.is_finite() || speed <= f64::EPSILON || ratio <= 1.0 + 1e-12 {
            return Affine::IDENTITY;
        }
        let angle = velocity.y.atan2(velocity.x);
        Affine::rotate(angle)
            * Affine::scale_non_uniform(ratio, 1.0 / ratio)
            * Affine::rotate(-angle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stretch_follows_velocity_and_keeps_area() {
        let squash = SquashStretch::new(0.1, 1.6);
        assert_eq!(squash.deform(DVec2::ZERO), Affine::IDENTITY);
        let horizontal = squash.deform(DVec2::new(4.0, 0.0));
        let [a, b, c, d, e, f] = horizontal.as_coeffs();
        assert!((a - 1.4).abs() < 1e-12 && (d - 1.0 / 1.4).abs() < 1e-12);
        assert!(b.abs() < 1e-12 && c.abs() < 1e-12 && e == 0.0 && f == 0.0);
        // Capped, and area-preserving in any direction.
        let diagonal = squash.deform(DVec2::new(30.0, 30.0));
        assert!((diagonal.determinant() - 1.0).abs() < 1e-12);
        let along = (diagonal * gaanim_core::kurbo::Point::new(1.0, 1.0)).to_vec2();
        assert!((along.length() / 2f64.sqrt() - 1.6).abs() < 1e-9);
    }
}
