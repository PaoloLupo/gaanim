//! Perceptual camera motion: exponential zoom and trauma-driven shake.
//!
//! Both helpers are pure functions of their inputs, so preview, exact seek,
//! snapshots, and export evaluate identical camera poses.

use glam::DVec2;

use crate::random::Noise;

/// How an animated orthographic zoom travels between two values.
///
/// Linear interpolation of a scale factor feels like it accelerates on large
/// zooms. `Exponential` evaluates `s0 * (s1 / s0)^p`, which changes the visible
/// area by the same ratio every frame and therefore reads as constant speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ZoomInterpolation {
    /// `s0 + (s1 - s0) * p`.
    Linear,
    /// `s0 * (s1 / s0)^p` (GSAP `ExpoScaleEase`).
    #[default]
    Exponential,
}

impl ZoomInterpolation {
    /// Parse the Python-facing name (`"linear"` or `"exponential"`).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "linear" => Some(Self::Linear),
            "exponential" => Some(Self::Exponential),
            _ => None,
        }
    }

    /// Zoom at eased progress `t`. Non-positive endpoints fall back to linear.
    pub fn zoom(self, from: f64, to: f64, t: f64) -> f64 {
        match self {
            Self::Exponential if from > 0.0 && to > 0.0 && from.is_finite() && to.is_finite() => {
                from * (to / from).powf(t)
            }
            _ => from + (to - from) * t,
        }
    }

    /// Fraction of a pan that accompanies this zoom at eased progress `t`.
    ///
    /// `Linear` pans linearly. `Exponential` pans in proportion to the change
    /// of visible width, so a combined pan+zoom scales the view about a fixed
    /// point: content moves along straight lines at a perceptually constant
    /// rate instead of drifting away before snapping into frame.
    pub fn pan_weight(self, from: f64, to: f64, t: f64) -> f64 {
        if self == Self::Linear || !(from > 0.0 && to > 0.0) {
            return t;
        }
        let (start_width, end_width) = (from.recip(), to.recip());
        let span = end_width - start_width;
        if !span.is_finite() || span.abs() <= 1e-9 * start_width.max(end_width) {
            return t;
        }
        (self.zoom(from, to, t).recip() - start_width) / span
    }
}

/// Camera shake driven by trauma (Squirrel Eiserloh, "Juicing Your Cameras
/// With Math", GDC 2016).
///
/// Trauma starts at `trauma` and decays linearly by `decay` per second. The
/// shake magnitude is `trauma²`, so light hits barely move the camera while
/// heavy hits shake hard. Translation and roll come from seeded coherent noise
/// sampled at `frequency` Hz, and the clip always releases to rest at its end.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TraumaShake {
    /// Initial trauma in `0..=1`.
    pub trauma: f64,
    /// Trauma lost per second.
    pub decay: f64,
    /// Noise frequency in Hz.
    pub frequency: f64,
    /// Maximum translation, in scene units, at trauma `1`.
    pub amplitude: f64,
    /// Maximum roll, in radians, at trauma `1`.
    pub rotation: f64,
    /// Noise seed; equal seeds replay the same shake.
    pub seed: u64,
}

impl Default for TraumaShake {
    fn default() -> Self {
        Self {
            trauma: 0.8,
            decay: 1.5,
            frequency: 12.0,
            amplitude: 0.4,
            rotation: 0.02,
            seed: 0,
        }
    }
}

impl TraumaShake {
    /// Longest final window, in seconds, used to release a clip that ends
    /// before its trauma has fully decayed.
    pub const RELEASE: f64 = 0.15;

    /// Time for trauma to decay to zero, or one second for a sustained shake.
    pub fn natural_duration(&self) -> f64 {
        if self.decay > 0.0 {
            self.trauma.clamp(0.0, 1.0) / self.decay
        } else {
            1.0
        }
    }

    /// Trauma `elapsed` seconds into a clip lasting `duration` seconds.
    pub fn trauma_at(&self, elapsed: f64, duration: f64) -> f64 {
        if !(elapsed >= 0.0 && elapsed < duration) {
            return 0.0;
        }
        let initial = self.trauma.clamp(0.0, 1.0);
        let decayed = (initial - self.decay.max(0.0) * elapsed).max(0.0);
        let window = (duration * 0.25).min(Self::RELEASE);
        let release = ((duration - elapsed) / window).clamp(0.0, 1.0);
        decayed.min(initial * release)
    }

    /// Translation offset and roll angle `elapsed` seconds into the clip.
    pub fn sample(&self, elapsed: f64, duration: f64) -> (DVec2, f64) {
        let shake = self.trauma_at(elapsed, duration).powi(2);
        if shake <= 0.0 {
            return (DVec2::ZERO, 0.0);
        }
        let noise = Noise::new(self.seed, self.frequency.max(0.0), 1.0, 2);
        let offset = DVec2::new(noise.at_time(elapsed, 0), noise.at_time(elapsed, 1));
        (
            offset * self.amplitude * shake,
            noise.at_time(elapsed, 2) * self.rotation * shake,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_zoom_has_constant_ratio_and_exact_endpoints() {
        let mode = ZoomInterpolation::Exponential;
        assert_eq!(mode.zoom(1.0, 8.0, 0.0), 1.0);
        assert!((mode.zoom(1.0, 8.0, 1.0) - 8.0).abs() < 1e-12);
        assert!((mode.zoom(1.0, 8.0, 0.5) - 8f64.sqrt()).abs() < 1e-12);
        let step = 0.1;
        let first = mode.zoom(1.0, 8.0, step) / mode.zoom(1.0, 8.0, 0.0);
        for index in 1..10 {
            let t = index as f64 * step;
            let ratio = mode.zoom(1.0, 8.0, t + step) / mode.zoom(1.0, 8.0, t);
            assert!((ratio - first).abs() < 1e-12);
        }
        assert_eq!(ZoomInterpolation::Linear.zoom(1.0, 8.0, 0.5), 4.5);
        // Invalid endpoints degrade to linear instead of producing NaN.
        assert_eq!(mode.zoom(0.0, 2.0, 0.5), 1.0);
        assert_eq!(ZoomInterpolation::default(), mode);
        assert_eq!(
            ZoomInterpolation::parse("linear"),
            Some(ZoomInterpolation::Linear)
        );
        assert_eq!(ZoomInterpolation::parse("exponential"), Some(mode));
        assert_eq!(ZoomInterpolation::parse("expo"), None);
    }

    #[test]
    fn exponential_pan_scales_the_view_about_a_fixed_point() {
        let mode = ZoomInterpolation::Exponential;
        let (z0, z1) = (1.0, 8.0);
        let (c0, c1) = (DVec2::new(0.0, 0.0), DVec2::new(3.0, -1.0));
        assert_eq!(mode.pan_weight(z0, z1, 0.0), 0.0);
        assert!((mode.pan_weight(z0, z1, 1.0) - 1.0).abs() < 1e-12);
        assert_eq!(mode.pan_weight(2.0, 2.0, 0.3), 0.3);
        assert_eq!(ZoomInterpolation::Linear.pan_weight(z0, z1, 0.3), 0.3);
        // The world point that stays still on screen during the move.
        let k = z0 / z1;
        let fixed = (c1 - c0 * k) / (1.0 - k);
        let screen = |t: f64| {
            let center = c0 + (c1 - c0) * mode.pan_weight(z0, z1, t);
            (fixed - center) * mode.zoom(z0, z1, t)
        };
        let start = screen(0.0);
        for index in 1..=20 {
            let drift = screen(index as f64 / 20.0) - start;
            assert!(drift.length() < 1e-9, "fixed point drifted by {drift:?}");
        }
    }

    #[test]
    fn trauma_shake_is_quadratic_decays_and_rests_at_the_end() {
        let shake = TraumaShake::default();
        let duration = shake.natural_duration();
        assert!((duration - 0.8 / 1.5).abs() < 1e-12);
        assert!((shake.trauma_at(0.0, duration) - 0.8).abs() < 1e-12);
        assert!((shake.trauma_at(0.2, duration) - 0.5).abs() < 1e-12);
        assert_eq!(shake.trauma_at(duration, duration), 0.0);
        assert_eq!(shake.sample(duration, duration), (DVec2::ZERO, 0.0));
        assert_eq!(shake.sample(-0.1, duration), (DVec2::ZERO, 0.0));

        // Magnitude is proportional to trauma squared.
        let sustained = TraumaShake {
            decay: 0.0,
            ..shake
        };
        let light = TraumaShake {
            trauma: 0.4,
            ..sustained
        };
        let (heavy_offset, heavy_roll) = sustained.sample(0.3, 1.0);
        let (light_offset, light_roll) = light.sample(0.3, 1.0);
        assert!(heavy_offset.length() > 0.0);
        assert!((heavy_offset - light_offset * 4.0).length() < 1e-12);
        assert!((heavy_roll - light_roll * 4.0).abs() < 1e-12);

        // A clip shorter than the natural decay still releases to rest.
        // The release window is a quarter of the clip (0.075 s) here.
        let early = shake.trauma_at(0.29, 0.3);
        assert!(
            (early - 0.8 * 0.01 / 0.075).abs() < 1e-9,
            "release: {early}"
        );
        assert_eq!(shake.trauma_at(0.3, 0.3), 0.0);
    }

    #[test]
    fn trauma_shake_is_seeded_bounded_and_continuous() {
        let shake = TraumaShake {
            decay: 0.5,
            ..TraumaShake::default()
        };
        assert_eq!(shake.sample(0.25, 1.0), shake.sample(0.25, 1.0));
        let other = TraumaShake { seed: 9, ..shake };
        assert_ne!(shake.sample(0.25, 1.0), other.sample(0.25, 1.0));
        let mut previous = shake.sample(0.0, 1.0);
        for step in 1..1000 {
            let current = shake.sample(step as f64 * 0.001, 1.0);
            assert!(current.0.x.abs() <= shake.amplitude && current.0.y.abs() <= shake.amplitude);
            assert!(current.1.abs() <= shake.rotation);
            assert!(
                (current.0 - previous.0).length() < 0.02,
                "jump at step {step}"
            );
            assert!(
                (current.1 - previous.1).abs() < 0.002,
                "roll jump at step {step}"
            );
            previous = current;
        }
    }
}
