//! Updater PyClass — preset updaters for per-frame reactive behavior.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use gaanim_animation::{LayerParam, OscillatedChannel, ProceduralLayer, Waveform};
use gaanim_api::canvas::UpdaterPreset;

use crate::visualization::PyParameter;

/// A number of an updater: a float, or a `Parameter` whose animation
/// changes the updater while it runs. `check` validates a float.
fn layer_param(
    name: &str,
    value: &Bound<'_, PyAny>,
    check: impl Fn(f64) -> Result<(), String>,
) -> PyResult<LayerParam> {
    if let Ok(parameter) = value.extract::<PyRef<'_, PyParameter>>() {
        return Ok(LayerParam::signal(parameter.inner.drawable().id));
    }
    if let Ok(computed) = value.extract::<PyRef<'_, crate::visualization::PyComputed>>() {
        let source = computed.time_only_source().ok_or_else(|| {
            PyValueError::new_err(format!(
                "{name} must be a Computed of the time alone (an audio signal, scene.noise, \
                 scene.time); pass a Parameter directly instead of computing from it"
            ))
        })?;
        return Ok(LayerParam::Source(std::sync::Arc::new(
            gaanim_animation::SourceParam::new(source),
        )));
    }
    let value: f64 = value.extract().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(format!(
            "{name} must be a number, a Parameter or a time Computed"
        ))
    })?;
    if !value.is_finite() {
        return Err(PyValueError::new_err(format!("{name} must be finite")));
    }
    check(value).map_err(PyValueError::new_err)?;
    Ok(LayerParam::Fixed(value))
}

/// Preset updater that can be attached to a DrawableHandle via `add_updater()`.
///
/// Use the static factory methods to create instances:
/// - `Updater.orbit(cx, cy, radius, speed)`
/// - `Updater.advance_x(speed)`
/// - `Updater.bob(amplitude, frequency)`
/// - `Updater.rotate(speed)`
/// - `Updater.pulse(min_scale, max_scale, frequency)`
/// - `Updater.dash_flow(speed)`
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

    /// Continuous Z-axis rotation; a `Parameter` speed can be animated.
    #[staticmethod]
    fn rotate(speed: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(match layer_param("speed", speed, |_| Ok(()))? {
            LayerParam::Fixed(speed) => UpdaterPreset::Rotate { speed },
            speed => UpdaterPreset::Procedural(ProceduralLayer::Spin { speed }),
        }))
    }

    /// Organic jitter layered over the drawable's animation.
    #[staticmethod]
    #[pyo3(signature = (*, position=None, rotation=None, scale=None, frequency=None, octaves=2, seed=0))]
    fn wiggle(
        position: Option<&Bound<'_, PyAny>>,
        rotation: Option<&Bound<'_, PyAny>>,
        scale: Option<&Bound<'_, PyAny>>,
        frequency: Option<&Bound<'_, PyAny>>,
        octaves: u32,
        seed: u64,
    ) -> PyResult<Self> {
        let amplitude = |name: &str, value: Option<&Bound<'_, PyAny>>, default: f64| {
            value.map_or(Ok(LayerParam::Fixed(default)), |value| {
                layer_param(name, value, |value| {
                    if value < 0.0 {
                        Err("position, rotation and scale amplitudes must be non-negative".into())
                    } else {
                        Ok(())
                    }
                })
            })
        };
        let position = amplitude("position", position, 0.08)?;
        let rotation = amplitude("rotation", rotation, 0.0)?;
        let scale = amplitude("scale", scale, 0.0)?;
        let frequency = frequency.map_or(Ok(LayerParam::Fixed(2.0)), |value| {
            layer_param("frequency", value, |value| {
                if value <= 0.0 {
                    Err("frequency must be positive".into())
                } else {
                    Ok(())
                }
            })
        })?;
        if !(1..=8).contains(&octaves) {
            return Err(PyValueError::new_err("octaves must be between 1 and 8"));
        }
        // A fixed frequency lives in the noise; a parameter one advances a
        // unit-frequency noise by its integral.
        let noise_frequency = frequency.fixed().unwrap_or(1.0);
        Ok(Self(UpdaterPreset::Procedural(ProceduralLayer::Wiggle {
            noise: gaanim_math::Noise::new(seed, noise_frequency, 1.0, octaves),
            position,
            rotation,
            scale,
            frequency,
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
                )));
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
                )));
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

    /// Dashes flowing along every stroke at `speed` scene units per second.
    #[staticmethod]
    #[pyo3(signature = (speed=0.5))]
    fn dash_flow(speed: f64) -> PyResult<Self> {
        if !speed.is_finite() {
            return Err(PyValueError::new_err("speed must be finite"));
        }
        Ok(Self(UpdaterPreset::DashFlow { speed }))
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
