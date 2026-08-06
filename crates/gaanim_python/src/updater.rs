//! Updater PyClass — preset updaters for per-frame reactive behavior.

use pyo3::prelude::*;

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
