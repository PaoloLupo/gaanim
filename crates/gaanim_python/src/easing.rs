use gaanim_math::{EasingCurve, RateFunc};
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
    #[pyo3(signature = (stiffness=90.0, damping=12.0))]
    fn spring(stiffness: f64, damping: f64) -> PyResult<Self> {
        Self::validate_finite("stiffness", stiffness)?;
        Self::validate_finite("damping", damping)?;
        if stiffness <= 0.0 {
            return Err(PyValueError::new_err("stiffness must be positive"));
        }
        if damping < 0.0 {
            return Err(PyValueError::new_err("damping must be non-negative"));
        }
        Ok(Self::new(
            RateFunc::Spring { stiffness, damping },
            format!("Easing.spring(stiffness={stiffness}, damping={damping})"),
        ))
    }

    #[staticmethod]
    fn steps(count: i64) -> PyResult<Self> {
        if count < 1 || count > u32::MAX as i64 {
            return Err(PyValueError::new_err("count must be at least 1"));
        }
        let count = count as u32;
        Ok(Self::new(
            RateFunc::Steps(count),
            format!("Easing.steps({count})"),
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
        let spring = PyEasing::spring(90.0, 12.0).unwrap();
        assert_same(
            &spring.inner,
            &RateFunc::Spring {
                stiffness: 90.0,
                damping: 12.0,
            },
        );
        assert!((1..100).any(|step| spring.inner.evaluate(step as f64 / 100.0) > 1.0));

        assert_same(&PyEasing::steps(5).unwrap().inner, &RateFunc::Steps(5));
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
    fn factories_reject_invalid_numeric_parameters() {
        assert!(PyEasing::spring(0.0, 12.0).is_err());
        assert!(PyEasing::spring(90.0, -1.0).is_err());
        assert!(PyEasing::spring(f64::NAN, 12.0).is_err());
        assert!(PyEasing::steps(0).is_err());
        assert!(PyEasing::there_and_back(-0.1).is_err());
        assert!(PyEasing::there_and_back(1.1).is_err());
        assert!(PyEasing::there_and_back(f64::INFINITY).is_err());
        assert!(PyEasing::cubic_bezier(-0.1, 0.0, 0.5, 1.0).is_err());
        assert!(PyEasing::cubic_bezier(0.1, 0.0, 1.1, 1.0).is_err());
        assert!(PyEasing::cubic_bezier(0.1, f64::NAN, 0.9, 1.0).is_err());
    }
}
