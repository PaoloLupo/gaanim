//! Python adaptation for pure, exact-time custom property animations.

use std::cell::Cell;

use gaanim_animation::{CustomAnimation, CustomChannel, CustomValues};
use gaanim_core::glam::DVec3;
use pyo3::{
    exceptions::{PyRuntimeError, PyTypeError, PyValueError},
    prelude::*,
    types::PyDict,
};

use crate::brush::PyPaint;

thread_local! { static PURE_CALLBACK_DEPTH: Cell<usize> = const { Cell::new(0) }; }

/// All authoring mutation entrypoints use this check before touching state.
pub(crate) fn ensure_authoring_allowed() -> PyResult<()> {
    if PURE_CALLBACK_DEPTH.with(|depth| depth.get() != 0) {
        Err(PyRuntimeError::new_err("animation and reactive callbacks must be pure; scene, drawable, parameter and timeline access is not allowed"))
    } else {
        Ok(())
    }
}

struct PureCallbackGuard;

/// Apply the same purity boundary to computed/property callbacks.
pub(crate) fn with_pure_callback<T>(callback: impl FnOnce() -> T) -> T {
    let _guard = PureCallbackGuard::enter();
    callback()
}
impl PureCallbackGuard {
    fn enter() -> Self {
        PURE_CALLBACK_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self
    }
}
impl Drop for PureCallbackGuard {
    fn drop(&mut self) {
        PURE_CALLBACK_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

fn parse_channel(value: &str) -> PyResult<CustomChannel> {
    Ok(match value {
        "position" => CustomChannel::Position,
        "rotation" => CustomChannel::Rotation,
        "scale" => CustomChannel::Scale,
        "opacity" => CustomChannel::Opacity,
        "fill" => CustomChannel::Fill,
        "stroke" => CustomChannel::Stroke,
        "stroke_width" => CustomChannel::StrokeWidth,
        _ => return Err(PyValueError::new_err("custom channels must be position, rotation, scale, opacity, fill, stroke, or stroke_width")),
    })
}

pub(crate) fn animation(callback: Py<PyAny>, channels: Vec<String>) -> PyResult<CustomAnimation> {
    ensure_authoring_allowed()?;
    Python::attach(|py| {
        crate::visualization::validate_callback(py, callback.bind(py), 1)?;
        let inspect = py.import("inspect")?;
        for candidate in [
            Some(callback.bind(py).clone()),
            callback.bind(py).getattr("__call__").ok(),
        ]
        .into_iter()
        .flatten()
        {
            for predicate in [
                "iscoroutinefunction",
                "isasyncgenfunction",
                "isgeneratorfunction",
            ] {
                if inspect
                    .getattr(predicate)?
                    .call1((&candidate,))?
                    .extract::<bool>()?
                {
                    return Err(PyTypeError::new_err(
                        "custom callback must be a synchronous function returning a dict",
                    ));
                }
            }
        }
        Ok(())
    })?;
    let channels = channels
        .iter()
        .map(|channel| parse_channel(channel))
        .collect::<PyResult<Vec<_>>>()?;
    let expected_channels = channels.clone();
    CustomAnimation::new(channels, move |alpha| {
        let _guard = PureCallbackGuard::enter();
        Python::attach(|py| -> PyResult<CustomValues> {
            let result = callback.bind(py).call1((alpha,))?;
            let mapping = result.cast::<PyDict>().map_err(|_| {
                PyTypeError::new_err("custom callback must return a dict of its declared channels")
            })?;
            if mapping.len() != expected_channels.len() {
                return Err(PyValueError::new_err(
                    "custom callback must return exactly its declared channels",
                ));
            }
            let mut values = CustomValues::default();
            for channel in &expected_channels {
                let value = mapping.get_item(channel.name())?.ok_or_else(|| {
                    PyValueError::new_err(format!("custom callback omitted '{}'", channel.name()))
                })?;
                match channel {
                    CustomChannel::Position => {
                        values.position = Some(if let Ok((x, y)) = value.extract::<(f64, f64)>() {
                            DVec3::new(x, y, 0.0)
                        } else {
                            let (x, y, z) = value.extract::<(f64, f64, f64)>()?;
                            DVec3::new(x, y, z)
                        });
                    }
                    CustomChannel::Rotation => values.rotation = Some(value.extract()?),
                    CustomChannel::Scale => {
                        values.scale = Some(if let Ok(uniform) = value.extract::<f64>() {
                            DVec3::splat(uniform)
                        } else {
                            let (x, y, z) = value.extract::<(f64, f64, f64)>()?;
                            DVec3::new(x, y, z)
                        });
                    }
                    CustomChannel::Opacity => values.opacity = Some(value.extract()?),
                    CustomChannel::Fill => values.fill = Some(value.extract::<PyPaint>()?.0),
                    CustomChannel::Stroke => values.stroke = Some(value.extract::<PyPaint>()?.0),
                    CustomChannel::StrokeWidth => values.stroke_width = Some(value.extract()?),
                }
            }
            Ok(values)
        })
        .map_err(|error| error.to_string())
    })
    .map_err(PyValueError::new_err)
}
