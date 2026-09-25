//! Updater PyClass — preset updaters for per-frame reactive behavior.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use gaanim_animation::{OscillatedChannel, ProceduralLayer, Waveform};
use gaanim_api::canvas::UpdaterPreset;

/// Preset updater that can be attached to a DrawableHandle via `add_updater()`.
///
/// Use the static factory methods to create instances:
/// - `Updater.orbit(cx, cy, radius, speed)`
/// - `Updater.advance_x(speed)`
/// - `Updater.bob(amplitude, frequency)`
/// - `Updater.rotate(speed)`
/// - `Updater.pulse(min_scale, max_scale, frequency)`
#[pyclass(name = "Updater", module = "gaanim_core", from_py_object)]
#[derive(Clone, Debug)]
pub struct PyUpdater(pub UpdaterPreset);

#[pymethods]
impl PyUpdater {
    /// Orbit around (cx, cy) at given radius and angular speed.
    #[staticmethod]
    fn orbit(cx: f64, cy: f64, radius: f64, speed: f64) -> Self {
        Self(UpdaterPreset::Orbit {
            cx,
            cy,
            radius,
            speed,
        })
    }

    /// Move X by `speed * dt` each frame.
    #[staticmethod]
    fn advance_x(speed: f64) -> Self {
        Self(UpdaterPreset::AdvanceX { speed })
    }

    /// Sinusoidal Y oscillation.
    #[staticmethod]
    fn bob(amplitude: f64, frequency: f64) -> Self {
        Self(UpdaterPreset::Bob {
            amplitude,
            frequency,
        })
    }

    /// Continuous Z-axis rotation.
    #[staticmethod]
    fn rotate(speed: f64) -> Self {
        Self(UpdaterPreset::Rotate { speed })
    }

    /// Organic jitter layered over the drawable's animation.
    #[staticmethod]
    #[pyo3(signature = (*, position=0.08, rotation=0.0, scale=0.0, frequency=2.0, octaves=2, seed=0))]
    fn wiggle(
        position: f64,
        rotation: f64,
        scale: f64,
        frequency: f64,
        octaves: u32,
        seed: u64,
    ) -> PyResult<Self> {
        for (name, value) in [
            ("position", position),
            ("rotation", rotation),
            ("scale", scale),
            ("frequency", frequency),
        ] {
            if !value.is_finite() {
                return Err(PyValueError::new_err(format!("{name} must be finite")));
            }
        }
        if position < 0.0 || rotation < 0.0 || scale < 0.0 {
            return Err(PyValueError::new_err(
                "position, rotation and scale amplitudes must be non-negative",
            ));
        }
        if frequency <= 0.0 {
            return Err(PyValueError::new_err("frequency must be positive"));
        }
        if !(1..=8).contains(&octaves) {
            return Err(PyValueError::new_err("octaves must be between 1 and 8"));
        }
        Ok(Self(UpdaterPreset::Procedural(ProceduralLayer::Wiggle {
            noise: gaanim_math::Noise::new(seed, frequency, 1.0, octaves),
            position,
            rotation,
            scale,
        })))
    }

    /// Periodic value on one channel, layered over the drawable's animation.
    #[staticmethod]
    #[pyo3(signature = (channel, *, waveform="sine", frequency=1.0, low=0.0, high=1.0, phase=0.0))]
    fn oscillate(
        channel: &str,
        waveform: &str,
        frequency: f64,
        low: f64,
        high: f64,
        phase: f64,
    ) -> PyResult<Self> {
        let channel = match channel {
            "x" => OscillatedChannel::X,
            "y" => OscillatedChannel::Y,
            "rotation" => OscillatedChannel::Rotation,
            "scale" => OscillatedChannel::Scale,
            "opacity" => OscillatedChannel::Opacity,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown channel {other:?}; expected \"x\", \"y\", \"rotation\", \"scale\" or \"opacity\""
                )))
            }
        };
        let waveform = match waveform {
            "sine" => Waveform::Sine,
            "square" => Waveform::Square,
            "triangle" => Waveform::Triangle,
            "saw" => Waveform::Saw,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown waveform {other:?}; expected \"sine\", \"square\", \"triangle\" or \"saw\""
                )))
            }
        };
        for (name, value) in [
            ("frequency", frequency),
            ("low", low),
            ("high", high),
            ("phase", phase),
        ] {
            if !value.is_finite() {
                return Err(PyValueError::new_err(format!("{name} must be finite")));
            }
        }
        if frequency <= 0.0 {
            return Err(PyValueError::new_err("frequency must be positive"));
        }
        if matches!(channel, OscillatedChannel::Opacity) && !(0.0..=1.0).contains(&low.min(high)) {
            return Err(PyValueError::new_err(
                "opacity factors must be within [0, 1]",
            ));
        }
        if matches!(channel, OscillatedChannel::Opacity) && high.max(low) > 1.0 {
            return Err(PyValueError::new_err(
                "opacity factors must be within [0, 1]",
            ));
        }
        Ok(Self(UpdaterPreset::Procedural(
            ProceduralLayer::Oscillate {
                channel,
                waveform,
                frequency,
                low,
                high,
                phase,
            },
        )))
    }

    /// Scale oscillation between min and max.
    #[staticmethod]
    fn pulse(min_scale: f64, max_scale: f64, frequency: f64) -> Self {
        Self(UpdaterPreset::Pulse {
            min_scale,
            max_scale,
            frequency,
        })
    }
}
