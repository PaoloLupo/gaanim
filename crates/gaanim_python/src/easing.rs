use gaanim_math::{EaseMode, EasingCurve, RateFunc, StepJump};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[pyclass(
    name = "EasingCurve",
    module = "gaanim_core",
    rename_all = "SCREAMING_SNAKE_CASE",
    frozen,
    eq,
    eq_int,
    from_py_object
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PyEasingCurve {
    Quadratic,
    Cubic,
    Quartic,
    Quintic,
    Exponential,
    Sine,
    Circular,
    Back,
    Elastic,
    Bounce,
}

impl From<PyEasingCurve> for EasingCurve {
    fn from(value: PyEasingCurve) -> Self {
        match value {
            PyEasingCurve::Quadratic => Self::Quadratic,
            PyEasingCurve::Cubic => Self::Cubic,
            PyEasingCurve::Quartic => Self::Quartic,
            PyEasingCurve::Quintic => Self::Quintic,
            PyEasingCurve::Exponential => Self::Exponential,
            PyEasingCurve::Sine => Self::Sine,
            PyEasingCurve::Circular => Self::Circular,
            PyEasingCurve::Back => Self::Back,
            PyEasingCurve::Elastic => Self::Elastic,
            PyEasingCurve::Bounce => Self::Bounce,
        }
    }
}

/// Upper bound for `Easing.custom` lookup tables.
const MAX_CUSTOM_SAMPLES: i64 = 65_536;

#[pyclass(name = "Easing", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone)]
pub struct PyEasing {
    pub(crate) inner: RateFunc,
    label: String,
}

impl PyEasing {
    fn new(inner: RateFunc, label: impl Into<String>) -> Self {
        Self {
            inner,
            label: label.into(),
        }
    }

    fn spring_preset(bounce: f64, speed: f64, label: &str) -> Self {
        Self::new(RateFunc::spring_bounce(bounce, 0.0, speed), label)
    }

    fn mode(mode: &str) -> PyResult<EaseMode> {
        match mode {
            "in" => Ok(EaseMode::In),
            "out" => Ok(EaseMode::Out),
            "in_out" => Ok(EaseMode::InOut),
            other => Err(PyValueError::new_err(format!(
                "unknown mode {other:?}; expected \"in\", \"out\" or \"in_out\""
            ))),
        }
    }

    fn validate_finite(name: &str, value: f64) -> PyResult<()> {
        if value.is_finite() {
            Ok(())
        } else {
            Err(PyValueError::new_err(format!("{name} must be finite")))
        }
    }
}

#[pymethods]
#[allow(non_snake_case)]
impl PyEasing {
    #[classattr]
    fn LINEAR() -> Self {
        Self::new(RateFunc::Linear, "Easing.LINEAR")
    }

    #[classattr]
    fn SMOOTH() -> Self {
        Self::new(RateFunc::Smooth, "Easing.SMOOTH")
    }

    #[classattr]
    fn DOUBLE_SMOOTH() -> Self {
        Self::new(RateFunc::DoubleSmooth, "Easing.DOUBLE_SMOOTH")
    }

    #[classattr]
    fn THERE_AND_BACK() -> Self {
        Self::new(RateFunc::ThereAndBack, "Easing.THERE_AND_BACK")
    }

    #[classattr]
    fn LINGERING() -> Self {
        Self::new(RateFunc::Lingering, "Easing.LINGERING")
    }

    #[classattr]
    fn RUNNING_START() -> Self {
        Self::new(RateFunc::RunningStart, "Easing.RUNNING_START")
    }

    #[classattr]
    fn EXPONENTIAL_DECAY() -> Self {
        Self::new(RateFunc::ExponentialDecay, "Easing.EXPONENTIAL_DECAY")
    }

    #[classattr]
    fn NOT_QUITE_THERE() -> Self {
        Self::new(RateFunc::NotQuiteThere, "Easing.NOT_QUITE_THERE")
    }

    #[classattr]
    fn SMOOTH_SPRING() -> Self {
        Self::spring_preset(0.0, 1.2, "Easing.SMOOTH_SPRING")
    }

    #[classattr]
    fn GENTLE() -> Self {
        Self::spring_preset(0.05, 0.5, "Easing.GENTLE")
    }

    #[classattr]
    fn QUICK() -> Self {
        Self::spring_preset(0.1, 1.4, "Easing.QUICK")
    }

    #[classattr]
    fn SNAPPY() -> Self {
        Self::spring_preset(0.2, 1.4, "Easing.SNAPPY")
    }

    #[classattr]
    fn BOUNCY() -> Self {
        Self::spring_preset(0.45, 0.6, "Easing.BOUNCY")
    }

    #[staticmethod]
    fn ease_in(curve: PyEasingCurve) -> Self {
        Self::new(
            RateFunc::EaseIn(curve.into()),
            format!("Easing.ease_in(EasingCurve.{curve:?})"),
        )
    }

    #[staticmethod]
    fn ease_out(curve: PyEasingCurve) -> Self {
        Self::new(
            RateFunc::EaseOut(curve.into()),
            format!("Easing.ease_out(EasingCurve.{curve:?})"),
        )
    }

    #[staticmethod]
    fn ease_in_out(curve: PyEasingCurve) -> Self {
        Self::new(
            RateFunc::EaseInOut(curve.into()),
            format!("Easing.ease_in_out(EasingCurve.{curve:?})"),
        )
    }

    #[staticmethod]
    #[pyo3(signature = (stiffness=None, damping=None, *, mass=1.0, velocity=0.0, bounce=None))]
    fn spring(
        stiffness: Option<f64>,
        damping: Option<f64>,
        mass: f64,
        velocity: f64,
        bounce: Option<f64>,
    ) -> PyResult<Self> {
        Self::validate_finite("mass", mass)?;
        Self::validate_finite("velocity", velocity)?;
        if let Some(bounce) = bounce {
            if stiffness.is_some() || damping.is_some() || mass != 1.0 {
                return Err(PyValueError::new_err(
                    "bounce describes the spring perceptually; do not combine it with stiffness, damping or mass",
                ));
            }
            Self::validate_finite("bounce", bounce)?;
            if !(0.0..1.0).contains(&bounce) {
                return Err(PyValueError::new_err("bounce must be in [0, 1)"));
            }
            return Ok(Self::new(
                RateFunc::spring_bounce(bounce, velocity, 1.0),
                format!("Easing.spring(bounce={bounce}, velocity={velocity})"),
            ));
        }
        let stiffness = stiffness.unwrap_or(90.0);
        let damping = damping.unwrap_or(12.0);
        Self::validate_finite("stiffness", stiffness)?;
        Self::validate_finite("damping", damping)?;
        if stiffness <= 0.0 {
            return Err(PyValueError::new_err("stiffness must be positive"));
        }
        if damping < 0.0 {
            return Err(PyValueError::new_err("damping must be non-negative"));
        }
        if mass <= 0.0 {
            return Err(PyValueError::new_err("mass must be positive"));
        }
        if mass == 1.0 && velocity == 0.0 {
            return Ok(Self::new(
                RateFunc::Spring { stiffness, damping },
                format!("Easing.spring(stiffness={stiffness}, damping={damping})"),
            ));
        }
        // Same clock as `RateFunc::Spring`: the clip spans 5 physical seconds.
        Ok(Self::new(
            RateFunc::DampedSpring {
                omega: (stiffness / mass).sqrt() * 5.0,
                zeta: damping / (2.0 * (stiffness * mass).sqrt()),
                velocity,
            },
            format!(
                "Easing.spring(stiffness={stiffness}, damping={damping}, mass={mass}, velocity={velocity})"
            ),
        ))
    }

    #[staticmethod]
    #[pyo3(signature = (overshoot=1.70158, *, mode="out"))]
    fn back(overshoot: f64, mode: &str) -> PyResult<Self> {
        Self::validate_finite("overshoot", overshoot)?;
        if overshoot < 0.0 {
            return Err(PyValueError::new_err("overshoot must be non-negative"));
        }
        Ok(Self::new(
            RateFunc::Back {
                overshoot,
                mode: Self::mode(mode)?,
            },
            format!("Easing.back({overshoot}, mode={mode:?})"),
        ))
    }

    #[staticmethod]
    #[pyo3(signature = (amplitude=1.0, period=0.3, *, mode="out"))]
    fn elastic(amplitude: f64, period: f64, mode: &str) -> PyResult<Self> {
        Self::validate_finite("amplitude", amplitude)?;
        Self::validate_finite("period", period)?;
        if amplitude < 1.0 {
            return Err(PyValueError::new_err("amplitude must be at least 1"));
        }
        if period <= 0.0 {
            return Err(PyValueError::new_err("period must be positive"));
        }
        Ok(Self::new(
            RateFunc::Elastic {
                amplitude,
                period,
                mode: Self::mode(mode)?,
            },
            format!("Easing.elastic({amplitude}, {period}, mode={mode:?})"),
        ))
    }

    #[staticmethod]
    #[pyo3(signature = (strength=1.0, *, mode="out"))]
    fn bounce(strength: f64, mode: &str) -> PyResult<Self> {
        Self::validate_finite("strength", strength)?;
        if !(0.0..=1.0).contains(&strength) {
            return Err(PyValueError::new_err("strength must be between 0 and 1"));
        }
        Ok(Self::new(
            RateFunc::Bounce {
                strength,
                mode: Self::mode(mode)?,
            },
            format!("Easing.bounce({strength}, mode={mode:?})"),
        ))
    }

    #[staticmethod]
    #[pyo3(signature = (linear_ratio=0.7, power=0.7))]
    fn slow_mo(linear_ratio: f64, power: f64) -> PyResult<Self> {
        Self::validate_finite("linear_ratio", linear_ratio)?;
        Self::validate_finite("power", power)?;
        if !(0.0..=1.0).contains(&linear_ratio) || !(0.0..=1.0).contains(&power) {
            return Err(PyValueError::new_err(
                "linear_ratio and power must be between 0 and 1",
            ));
        }
        Ok(Self::new(
            RateFunc::SlowMo {
                linear_ratio,
                power,
            },
            format!("Easing.slow_mo({linear_ratio}, {power})"),
        ))
    }

    #[staticmethod]
    #[pyo3(signature = (strength=1.0, points=20, seed=0))]
    fn rough(strength: f64, points: i64, seed: u64) -> PyResult<Self> {
        Self::validate_finite("strength", strength)?;
        if !(0.0..=2.0).contains(&strength) {
            return Err(PyValueError::new_err("strength must be between 0 and 2"));
        }
        if !(1..=10_000).contains(&points) {
            return Err(PyValueError::new_err("points must be between 1 and 10000"));
        }
        Ok(Self::new(
            RateFunc::rough(strength, points as u32, seed),
            format!("Easing.rough({strength}, points={points}, seed={seed})"),
        ))
    }

    #[staticmethod]
    fn squish(easing: &PyEasing, start: f64, end: f64) -> PyResult<Self> {
        Self::validate_finite("start", start)?;
        Self::validate_finite("end", end)?;
        if !(0.0 <= start && start < end && end <= 1.0) {
            return Err(PyValueError::new_err(
                "squish requires 0 <= start < end <= 1",
            ));
        }
        Ok(Self::new(
            RateFunc::Squish {
                inner: Box::new(easing.inner.clone()),
                start,
                end,
            },
            format!("Easing.squish({}, {start}, {end})", easing.label),
        ))
    }

    #[staticmethod]
    #[pyo3(signature = (path, samples=256))]
    fn from_svg(path: &str, samples: i64) -> PyResult<Self> {
        if !(2..=MAX_CUSTOM_SAMPLES).contains(&samples) {
            return Err(PyValueError::new_err(format!(
                "samples must be between 2 and {MAX_CUSTOM_SAMPLES}"
            )));
        }
        RateFunc::from_svg_path(path, samples as usize)
            .map(|inner| Self::new(inner, format!("Easing.from_svg({path:?})")))
            .map_err(PyValueError::new_err)
    }

    #[staticmethod]
    #[pyo3(signature = (count, jump="end"))]
    fn steps(count: i64, jump: &str) -> PyResult<Self> {
        if count < 1 || count > u32::MAX as i64 {
            return Err(PyValueError::new_err("count must be at least 1"));
        }
        let count = count as u32;
        let jump = match jump {
            "end" => {
                return Ok(Self::new(
                    RateFunc::Steps(count),
                    format!("Easing.steps({count})"),
                ))
            }
            "start" => StepJump::Start,
            "none" if count >= 2 => StepJump::None,
            "none" => {
                return Err(PyValueError::new_err(
                    "jump=\"none\" needs at least 2 steps",
                ))
            }
            "both" => StepJump::Both,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown jump {other:?}; expected \"start\", \"end\", \"none\" or \"both\""
                )))
            }
        };
        Ok(Self::new(
            RateFunc::SteppedJump { count, jump },
            format!("Easing.steps({count}, jump={jump:?})"),
        ))
    }

    #[staticmethod]
    fn mirror(easing: &PyEasing) -> Self {
        Self::new(
            RateFunc::Mirror(Box::new(easing.inner.clone())),
            format!("Easing.mirror({})", easing.label),
        )
    }

    #[staticmethod]
    #[pyo3(signature = (pause=0.0))]
    fn there_and_back(pause: f64) -> PyResult<Self> {
        Self::validate_finite("pause", pause)?;
        if !(0.0..=1.0).contains(&pause) {
            return Err(PyValueError::new_err("pause must be between 0 and 1"));
        }
        let inner = if pause == 0.0 {
            RateFunc::ThereAndBack
        } else {
            RateFunc::ThereAndBackWithPause(pause)
        };
        Ok(Self::new(
            inner,
            format!("Easing.there_and_back(pause={pause})"),
        ))
    }

    #[staticmethod]
    fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> PyResult<Self> {
        for (name, value) in [("x1", x1), ("y1", y1), ("x2", x2), ("y2", y2)] {
            Self::validate_finite(name, value)?;
        }
        if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
            return Err(PyValueError::new_err("x1 and x2 must be between 0 and 1"));
        }
        Ok(Self::new(
            RateFunc::CubicBezier(x1, y1, x2, y2),
            format!("Easing.cubic_bezier({x1}, {y1}, {x2}, {y2})"),
        ))
    }

    /// Sample `function` at `samples` evenly spaced times once, at authoring.
    #[staticmethod]
    #[pyo3(signature = (function, samples=256))]
    fn custom(function: &Bound<'_, PyAny>, samples: i64) -> PyResult<Self> {
        if !function.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Easing.custom() expects a callable taking t in [0, 1]",
            ));
        }
        if !(2..=MAX_CUSTOM_SAMPLES).contains(&samples) {
            return Err(PyValueError::new_err(format!(
                "samples must be between 2 and {MAX_CUSTOM_SAMPLES}"
            )));
        }
        let last = (samples - 1) as f64;
        let table = (0..samples)
            .map(|index| {
                let t = index as f64 / last;
                let value: f64 = function.call1((t,))?.extract()?;
                if !value.is_finite() || !(-1.0..=2.0).contains(&value) {
                    return Err(PyValueError::new_err(format!(
                        "Easing.custom() function returned {value} at t={t}; values must be finite and within [-1, 2]"
                    )));
                }
                Ok(value)
            })
            .collect::<PyResult<Vec<f64>>>()?;
        Ok(Self::new(
            RateFunc::Sampled(table.into()),
            format!("Easing.custom(<function>, samples={samples})"),
        ))
    }

    fn __repr__(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_same(actual: &RateFunc, expected: &RateFunc) {
        for t in [0.0, 0.1, 0.35, 0.5, 0.8, 1.0] {
            let delta = (actual.evaluate(t) - expected.evaluate(t)).abs();
            assert!(delta < 1e-12, "mismatch at t={t}: delta={delta}");
        }
    }

    #[test]
    fn presets_delegate_to_rate_func_without_reimplementation() {
        for (actual, expected) in [
            (PyEasing::LINEAR().inner, RateFunc::Linear),
            (PyEasing::SMOOTH().inner, RateFunc::Smooth),
            (PyEasing::DOUBLE_SMOOTH().inner, RateFunc::DoubleSmooth),
            (PyEasing::THERE_AND_BACK().inner, RateFunc::ThereAndBack),
            (PyEasing::LINGERING().inner, RateFunc::Lingering),
            (PyEasing::RUNNING_START().inner, RateFunc::RunningStart),
            (
                PyEasing::EXPONENTIAL_DECAY().inner,
                RateFunc::ExponentialDecay,
            ),
            (PyEasing::NOT_QUITE_THERE().inner, RateFunc::NotQuiteThere),
        ] {
            assert_same(&actual, &expected);
        }
    }

    #[test]
    fn every_curve_family_maps_to_the_matching_rate_func() {
        for curve in [
            PyEasingCurve::Quadratic,
            PyEasingCurve::Cubic,
            PyEasingCurve::Quartic,
            PyEasingCurve::Quintic,
            PyEasingCurve::Exponential,
            PyEasingCurve::Sine,
            PyEasingCurve::Circular,
            PyEasingCurve::Back,
            PyEasingCurve::Elastic,
            PyEasingCurve::Bounce,
        ] {
            let native = EasingCurve::from(curve);
            assert_same(&PyEasing::ease_in(curve).inner, &RateFunc::EaseIn(native));
            assert_same(&PyEasing::ease_out(curve).inner, &RateFunc::EaseOut(native));
            assert_same(
                &PyEasing::ease_in_out(curve).inner,
                &RateFunc::EaseInOut(native),
            );
        }
        assert!(PyEasing::ease_in(PyEasingCurve::Back).inner.evaluate(0.5) < 0.0);
    }

    #[test]
    fn validated_factories_preserve_rate_func_behavior() {
        let spring = PyEasing::spring(Some(90.0), Some(12.0), 1.0, 0.0, None).unwrap();
        assert_same(
            &spring.inner,
            &RateFunc::Spring {
                stiffness: 90.0,
                damping: 12.0,
            },
        );
        assert!((1..100).any(|step| spring.inner.evaluate(step as f64 / 100.0) > 1.0));

        assert_same(
            &PyEasing::steps(5, "end").unwrap().inner,
            &RateFunc::Steps(5),
        );
        assert_same(
            &PyEasing::mirror(&PyEasing::SMOOTH()).inner,
            &RateFunc::Mirror(Box::new(RateFunc::Smooth)),
        );
        assert_same(
            &PyEasing::there_and_back(0.25).unwrap().inner,
            &RateFunc::ThereAndBackWithPause(0.25),
        );
        assert_same(
            &PyEasing::cubic_bezier(0.25, 0.1, 0.25, 1.0).unwrap().inner,
            &RateFunc::CubicBezier(0.25, 0.1, 0.25, 1.0),
        );
    }

    #[test]
    fn spring_presets_and_expressive_factories_build_native_curves() {
        let bouncy = PyEasing::spring(None, None, 1.0, 0.0, Some(0.35)).unwrap();
        assert_same(&bouncy.inner, &RateFunc::spring_bounce(0.35, 0.0, 1.0));
        for preset in [
            PyEasing::SMOOTH_SPRING(),
            PyEasing::GENTLE(),
            PyEasing::QUICK(),
            PyEasing::SNAPPY(),
            PyEasing::BOUNCY(),
        ] {
            assert_eq!(preset.inner.evaluate(0.0), 0.0);
            assert!((preset.inner.evaluate(1.0) - 1.0).abs() < 1e-12);
        }
        let heavy = PyEasing::spring(Some(90.0), Some(12.0), 2.0, 1.5, None).unwrap();
        assert!(matches!(heavy.inner, RateFunc::DampedSpring { velocity, .. } if velocity == 1.5));
        assert!(PyEasing::spring(Some(90.0), None, 1.0, 0.0, Some(0.2)).is_err());
        assert!(PyEasing::spring(None, None, 1.0, 0.0, Some(1.0)).is_err());
        assert!(PyEasing::spring(Some(90.0), Some(12.0), 0.0, 1.0, None).is_err());

        assert_same(
            &PyEasing::back(2.0, "in").unwrap().inner,
            &RateFunc::Back {
                overshoot: 2.0,
                mode: EaseMode::In,
            },
        );
        assert!(PyEasing::back(1.0, "sideways").is_err());
        assert!(PyEasing::elastic(0.5, 0.3, "out").is_err());
        assert!(PyEasing::elastic(1.0, 0.0, "out").is_err());
        assert!(PyEasing::bounce(1.5, "out").is_err());
        assert!(PyEasing::slow_mo(1.5, 0.7).is_err());
        assert!(PyEasing::rough(1.0, 0, 1).is_err());
        assert!(PyEasing::squish(&PyEasing::SMOOTH(), 0.8, 0.2).is_err());
        assert!(PyEasing::from_svg("M0,0 L0.5,1", 64).is_err());
        assert!(PyEasing::from_svg("M0,0 C0.3,0 0.2,1.2 1,1", 64).is_ok());
        assert_same(
            &PyEasing::steps(4, "both").unwrap().inner,
            &RateFunc::SteppedJump {
                count: 4,
                jump: StepJump::Both,
            },
        );
        assert!(PyEasing::steps(1, "none").is_err());
        assert!(PyEasing::steps(3, "middle").is_err());
    }

    #[test]
    fn factories_reject_invalid_numeric_parameters() {
        assert!(PyEasing::spring(Some(0.0), Some(12.0), 1.0, 0.0, None).is_err());
        assert!(PyEasing::spring(Some(90.0), Some(-1.0), 1.0, 0.0, None).is_err());
        assert!(PyEasing::spring(Some(f64::NAN), Some(12.0), 1.0, 0.0, None).is_err());
        assert!(PyEasing::steps(0, "end").is_err());
        assert!(PyEasing::there_and_back(-0.1).is_err());
        assert!(PyEasing::there_and_back(1.1).is_err());
        assert!(PyEasing::there_and_back(f64::INFINITY).is_err());
        assert!(PyEasing::cubic_bezier(-0.1, 0.0, 0.5, 1.0).is_err());
        assert!(PyEasing::cubic_bezier(0.1, 0.0, 1.1, 1.0).is_err());
        assert!(PyEasing::cubic_bezier(0.1, f64::NAN, 0.9, 1.0).is_err());
    }

    #[test]
    fn custom_samples_the_callable_once_into_a_table() {
        Python::initialize();
        Python::attach(|py| {
            let eval = |source: &str| {
                py.eval(&std::ffi::CString::new(source).unwrap(), None, None)
                    .unwrap()
            };
            let quartic = PyEasing::custom(&eval("lambda t: 1 - (1 - t) ** 4"), 256).unwrap();
            let RateFunc::Sampled(table) = &quartic.inner else {
                panic!("Easing.custom() should build a sampled rate function");
            };
            assert_eq!(table.len(), 256);
            assert_eq!(quartic.inner.evaluate(0.0), 0.0);
            assert_eq!(quartic.inner.evaluate(1.0), 1.0);
            assert!((quartic.inner.evaluate(0.5) - 0.9375).abs() < 1e-4);

            let is_value_error = |result: PyResult<PyEasing>| {
                result.is_err_and(|error| error.is_instance_of::<PyValueError>(py))
            };
            assert!(is_value_error(PyEasing::custom(
                &eval("lambda t: float('nan')"),
                8
            )));
            assert!(is_value_error(PyEasing::custom(
                &eval("lambda t: 3 * t"),
                8
            )));
            assert!(is_value_error(PyEasing::custom(&eval("lambda t: t"), 1)));
            assert!(PyEasing::custom(&eval("1.0"), 8)
                .is_err_and(|error| { error.is_instance_of::<pyo3::exceptions::PyTypeError>(py) }));
            assert!(PyEasing::custom(&eval("lambda t: 1 / 0"), 8).is_err());
        });
    }
}
