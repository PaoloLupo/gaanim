use std::sync::Arc;

/// Extensible easing curve shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EasingCurve {
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

/// Extensible time-interpolation rate functions (easing functions).
///
/// Fully cloneable and supports conditional serialization via `serde`
/// (custom closures are skipped in serialization).
#[derive(Clone)]
pub enum RateFunc {
    /// Constant rate.
    Linear,
    /// Hermite interpolation (3t^2 - 2t^3).
    Smooth,
    /// Smooth interpolation applied twice.
    DoubleSmooth,
    /// Easing curve starting slow and ending fast.
    EaseIn(EasingCurve),
    /// Easing curve starting fast and ending slow.
    EaseOut(EasingCurve),
    /// Easing curve starting slow, accelerating, and ending slow.
    EaseInOut(EasingCurve),
    /// A physically-modeled spring solver (analytical solution for constant evaluation).
    Spring {
        /// Stiffness coefficient (k). Controls acceleration towards target.
        stiffness: f64,
        /// Damping coefficient (c). Controls oscillation reduction.
        damping: f64,
    },
    /// Split interpolation into discrete steps.
    Steps(u32),
    /// Mirror an underlying rate function symmetrically.
    Mirror(Box<RateFunc>),
    /// Interpolates from 0.0 to 1.0 (at t=0.5) and back to 0.0.
    ThereAndBack,
    /// Interpolates from 0.0 to 1.0, pauses at 1.0, then goes back to 0.0.
    /// Parameter represents the fraction of time spent paused at peak.
    ThereAndBackWithPause(f64),
    /// Easing that lingers longer at the peak/end.
    Lingering,
    /// Backs up slightly before rushing forward.
    RunningStart,
    /// CSS-style Cubic Bezier curve using normalized timing points (x1, y1, x2, y2).
    CubicBezier(f64, f64, f64, f64),
    /// Exponential decay curve: starts fast, decelerates asymptotically towards 1.0.
    ExponentialDecay,
    /// Never quite reaches 1.0 — plateaus at 0.95 then settles.
    NotQuiteThere,
    /// Custom mathematical closure.
    Custom(Arc<dyn Fn(f64) -> f64 + Send + Sync>),
}

// Implement standard Debug since closures can't be debugged easily
impl std::fmt::Debug for RateFunc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Linear => write!(f, "Linear"),
            Self::Smooth => write!(f, "Smooth"),
            Self::DoubleSmooth => write!(f, "DoubleSmooth"),
            Self::EaseIn(c) => write!(f, "EaseIn({:?})", c),
            Self::EaseOut(c) => write!(f, "EaseOut({:?})", c),
            Self::EaseInOut(c) => write!(f, "EaseInOut({:?})", c),
            Self::Spring { stiffness, damping } => {
                write!(
                    f,
                    "Spring {{ stiffness: {}, damping: {} }}",
                    stiffness, damping
                )
            }
            Self::Steps(steps) => write!(f, "Steps({})", steps),
            Self::Mirror(inner) => write!(f, "Mirror({:?})", inner),
            Self::ThereAndBack => write!(f, "ThereAndBack"),
            Self::ThereAndBackWithPause(p) => write!(f, "ThereAndBackWithPause({})", p),
            Self::Lingering => write!(f, "Lingering"),
            Self::RunningStart => write!(f, "RunningStart"),
            Self::CubicBezier(x1, y1, x2, y2) => {
                write!(f, "CubicBezier({}, {}, {}, {})", x1, y1, x2, y2)
            }
            Self::ExponentialDecay => write!(f, "ExponentialDecay"),
            Self::NotQuiteThere => write!(f, "NotQuiteThere"),
            Self::Custom(_) => write!(f, "Custom(<closure>)"),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for RateFunc {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        match self {
            Self::Linear => serializer.serialize_unit_variant("RateFunc", 0, "Linear"),
            Self::Smooth => serializer.serialize_unit_variant("RateFunc", 1, "Smooth"),
            Self::DoubleSmooth => serializer.serialize_unit_variant("RateFunc", 2, "DoubleSmooth"),
            Self::EaseIn(c) => serializer.serialize_newtype_variant("RateFunc", 3, "EaseIn", c),
            Self::EaseOut(c) => serializer.serialize_newtype_variant("RateFunc", 4, "EaseOut", c),
            Self::EaseInOut(c) => {
                serializer.serialize_newtype_variant("RateFunc", 5, "EaseInOut", c)
            }
            Self::Spring { stiffness, damping } => {
                let mut state = serializer.serialize_struct("Spring", 2)?;
                state.serialize_field("stiffness", stiffness)?;
                state.serialize_field("damping", damping)?;
                state.end()
            }
            Self::Steps(s) => serializer.serialize_newtype_variant("RateFunc", 6, "Steps", s),
            Self::Mirror(inner) => {
                serializer.serialize_newtype_variant("RateFunc", 7, "Mirror", inner)
            }
            Self::ThereAndBack => serializer.serialize_unit_variant("RateFunc", 8, "ThereAndBack"),
            Self::ThereAndBackWithPause(p) => {
                serializer.serialize_newtype_variant("RateFunc", 9, "ThereAndBackWithPause", p)
            }
            Self::Lingering => serializer.serialize_unit_variant("RateFunc", 10, "Lingering"),
            Self::RunningStart => serializer.serialize_unit_variant("RateFunc", 11, "RunningStart"),
            Self::CubicBezier(x1, y1, x2, y2) => {
                let mut state = serializer.serialize_struct("CubicBezier", 4)?;
                state.serialize_field("x1", x1)?;
                state.serialize_field("y1", y1)?;
                state.serialize_field("x2", x2)?;
                state.serialize_field("y2", y2)?;
                state.end()
            }
            Self::ExponentialDecay => {
                serializer.serialize_unit_variant("RateFunc", 13, "ExponentialDecay")
            }
            Self::NotQuiteThere => {
                serializer.serialize_unit_variant("RateFunc", 14, "NotQuiteThere")
            }
            Self::Custom(_) => serializer.serialize_unit_variant("RateFunc", 12, "Custom"),
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for RateFunc {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Simple manual deserializer support or falling back to Linear if custom
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum RawRateFunc {
            Simple(String),
            Ease(String, EasingCurve),
            Spring { stiffness: f64, damping: f64 },
            Steps(u32),
            Mirror(Box<RateFunc>),
            ThereAndBackWithPause(f64),
            CubicBezier { x1: f64, y1: f64, x2: f64, y2: f64 },
        }

        match RawRateFunc::deserialize(deserializer)? {
            RawRateFunc::Simple(name) => match name.as_str() {
                "Linear" => Ok(Self::Linear),
                "Smooth" => Ok(Self::Smooth),
                "DoubleSmooth" => Ok(Self::DoubleSmooth),
                "ThereAndBack" => Ok(Self::ThereAndBack),
                "Lingering" => Ok(Self::Lingering),
                "RunningStart" => Ok(Self::RunningStart),
                "ExponentialDecay" => Ok(Self::ExponentialDecay),
                "NotQuiteThere" => Ok(Self::NotQuiteThere),
                _ => Ok(Self::Linear),
            },
            RawRateFunc::Ease(mode, curve) => match mode.as_str() {
                "EaseIn" => Ok(Self::EaseIn(curve)),
                "EaseOut" => Ok(Self::EaseOut(curve)),
                _ => Ok(Self::EaseInOut(curve)),
            },
            RawRateFunc::Spring { stiffness, damping } => Ok(Self::Spring { stiffness, damping }),
            RawRateFunc::Steps(s) => Ok(Self::Steps(s)),
            RawRateFunc::Mirror(inner) => Ok(Self::Mirror(inner)),
            RawRateFunc::ThereAndBackWithPause(p) => Ok(Self::ThereAndBackWithPause(p)),
            RawRateFunc::CubicBezier { x1, y1, x2, y2 } => Ok(Self::CubicBezier(x1, y1, x2, y2)),
        }
    }
}

impl RateFunc {
    /// Evaluates the rate function at a given normalized time `t` in `[0.0, 1.0]`.
    ///
    /// Outputs the interpolated progress factor, typically in `[0.0, 1.0]` (though spring
    /// physical oscillations and back-easing curves can overshoot or undershoot slightly).
    pub fn evaluate(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Smooth => t * t * (3.0 - 2.0 * t),
            Self::DoubleSmooth => {
                let s = t * t * (3.0 - 2.0 * t);
                s * s * (3.0 - 2.0 * s)
            }
            Self::EaseIn(c) => Self::eval_curve(*c, t),
            Self::EaseOut(c) => 1.0 - Self::eval_curve(*c, 1.0 - t),
            Self::EaseInOut(c) => {
                if t < 0.5 {
                    Self::eval_curve(*c, t * 2.0) * 0.5
                } else {
                    1.0 - Self::eval_curve(*c, (1.0 - t) * 2.0) * 0.5
                }
            }
            Self::Spring { stiffness, damping } => {
                // Analytical solver for: x''(t) + c*x'(t) + k*(x(t) - 1) = 0
                // Starting with x(0) = 0, x'(0) = 0, target position = 1.0.
                let k = *stiffness;
                let c = *damping;
                let mass = 1.0;

                let omega_n = (k / mass).sqrt();
                let zeta = c / (2.0 * (mass * k).sqrt());

                // Scale evaluation time. Usually spring animations look best if normalized time t [0..1]
                // maps to physical time. Let's map t to ~5.0 seconds of physical time.
                let t_phys = t * 5.0;

                if zeta < 1.0 {
                    // Underdamped oscillation
                    let omega_d = omega_n * (1.0 - zeta * zeta).sqrt();
                    let exponent = (-zeta * omega_n * t_phys).exp();
                    1.0 - exponent
                        * ((zeta * omega_n / omega_d) * (omega_d * t_phys).sin()
                            + (omega_d * t_phys).cos())
                } else if (zeta - 1.0).abs() < 1e-5 {
                    // Critically damped
                    let exponent = (-omega_n * t_phys).exp();
                    1.0 - exponent * (1.0 + omega_n * t_phys)
                } else {
                    // Overdamped
                    let r1 = -omega_n * (zeta - (zeta * zeta - 1.0).sqrt());
                    let r2 = -omega_n * (zeta + (zeta * zeta - 1.0).sqrt());
                    let c1 = r2 / (r2 - r1);
                    let c2 = -r1 / (r2 - r1);
                    1.0 + c1 * (r1 * t_phys).exp() + c2 * (r2 * t_phys).exp()
                }
            }
            Self::Steps(steps) => {
                if *steps == 0 {
                    t
                } else {
                    (t * (*steps as f64)).floor() / (*steps as f64)
                }
            }
            Self::Mirror(inner) => {
                if t < 0.5 {
                    inner.evaluate(t * 2.0) * 0.5
                } else {
                    1.0 - inner.evaluate((1.0 - t) * 2.0) * 0.5
                }
            }
            Self::ThereAndBack => {
                if t < 0.5 {
                    t * 2.0
                } else {
                    (1.0 - t) * 2.0
                }
            }
            Self::ThereAndBackWithPause(pause_ratio) => {
                let pause = pause_ratio.clamp(0.0, 0.9);
                let side_duration = (1.0 - pause) * 0.5;
                if t < side_duration {
                    t / side_duration
                } else if t < side_duration + pause {
                    1.0
                } else {
                    (1.0 - t) / side_duration
                }
            }
            Self::Lingering => {
                // Easing function that lingers around 0.5, slowing down then speeding up
                t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
            }
            Self::RunningStart => {
                // Antic/back-in kind of feel
                let s = 1.70158;
                t * t * ((s + 1.0) * t - s)
            }
            Self::CubicBezier(x1, y1, x2, y2) => Self::solve_cubic_bezier(*x1, *y1, *x2, *y2, t),
            Self::ExponentialDecay => {
                if t >= 1.0 {
                    1.0
                } else {
                    1.0 - (-5.0 * t).exp()
                }
            }
            Self::NotQuiteThere => {
                let p = 1.0 - (-5.0 * (t / 0.95).clamp(0.0, 1.0)).exp();
                (p * 0.95).min(0.95)
            }
            Self::Custom(f) => f(t),
        }
    }

    fn eval_curve(curve: EasingCurve, t: f64) -> f64 {
        match curve {
            EasingCurve::Quadratic => t * t,
            EasingCurve::Cubic => t * t * t,
            EasingCurve::Quartic => t.powi(4),
            EasingCurve::Quintic => t.powi(5),
            EasingCurve::Exponential => {
                if t == 0.0 {
                    0.0
                } else {
                    2.0f64.powf(10.0 * (t - 1.0))
                }
            }
            EasingCurve::Sine => 1.0 - (t * std::f64::consts::FRAC_PI_2).cos(),
            EasingCurve::Circular => 1.0 - (1.0 - t * t).sqrt(),
            EasingCurve::Back => {
                let s = 1.70158;
                t * t * ((s + 1.0) * t - s)
            }
            EasingCurve::Elastic => {
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    let p = 0.3;
                    let s = p / 4.0;
                    let t_scaled = t - 1.0;
                    -(2.0f64.powf(10.0 * t_scaled)
                        * ((t_scaled - s) * (2.0 * std::f64::consts::PI) / p).sin())
                }
            }
            EasingCurve::Bounce => 1.0 - Self::eval_bounce_out(1.0 - t),
        }
    }

    fn eval_bounce_out(t: f64) -> f64 {
        let n1 = 7.5625;
        let d1 = 2.75;

        if t < 1.0 / d1 {
            n1 * t * t
        } else if t < 2.0 / d1 {
            let t_sub = t - 1.5 / d1;
            n1 * t_sub * t_sub + 0.75
        } else if t < 2.5 / d1 {
            let t_sub = t - 2.25 / d1;
            n1 * t_sub * t_sub + 0.9375
        } else {
            let t_sub = t - 2.625 / d1;
            n1 * t_sub * t_sub + 0.984375
        }
    }

    /// Numerical cubic bezier root solver for CSS transitions
    fn solve_cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, t: f64) -> f64 {
        // De Casteljau bezier parameter solver for X(p) = t, then evaluate Y(p)
        // Check edge cases
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }

        let mut p = t;
        // Run a few Newton-Raphson iterations to solve for x(p) = t
        for _ in 0..8 {
            let x = Self::bezier_coord(x1, x2, p);
            let slope = Self::bezier_slope(x1, x2, p);
            if slope.abs() < 1e-6 {
                break;
            }
            let diff = x - t;
            p -= diff / slope;
            p = p.clamp(0.0, 1.0);
        }

        Self::bezier_coord(y1, y2, p)
    }

    fn bezier_coord(c1: f64, c2: f64, t: f64) -> f64 {
        // B(t) = 3 * (1-t)^2 * t * c1 + 3 * (1-t) * t^2 * c2 + t^3
        let mt = 1.0 - t;
        3.0 * mt * mt * t * c1 + 3.0 * mt * t * t * c2 + t * t * t
    }

    fn bezier_slope(c1: f64, c2: f64, t: f64) -> f64 {
        // B'(t) = 9 * (1-t)^2 * c1 - 6 * (1-t) * t * c1 + 6 * (1-t) * t * c2 - 3 * t^2 * c2 + 3 * t^2
        // Simplifies to: 3*(1-t)^2 * c1 + 6*(1-t)*t*(c2-c1) + 3*t^2*(1-c2) (with standard 1 at end)
        let mt = 1.0 - t;
        3.0 * mt * mt * c1 + 6.0 * mt * t * (c2 - c1) + 3.0 * t * t * (1.0 - c2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_func_linear() {
        let rf = RateFunc::Linear;
        assert!((rf.evaluate(0.0) - 0.0).abs() < 1e-9);
        assert!((rf.evaluate(0.5) - 0.5).abs() < 1e-9);
        assert!((rf.evaluate(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rate_func_smooth() {
        let rf = RateFunc::Smooth;
        assert!((rf.evaluate(0.0) - 0.0).abs() < 1e-9);
        assert!((rf.evaluate(0.5) - 0.5).abs() < 1e-9);
        assert!((rf.evaluate(1.0) - 1.0).abs() < 1e-9);
        // Smooth S-curve: starts slow (below linear in first half, above in second)
        assert!(rf.evaluate(0.25) < 0.25);
        assert!(rf.evaluate(0.75) > 0.75);
    }

    #[test]
    fn rate_func_ease_in_quad() {
        let rf = RateFunc::EaseIn(EasingCurve::Quadratic);
        assert!((rf.evaluate(0.0) - 0.0).abs() < 1e-9);
        assert!((rf.evaluate(1.0) - 1.0).abs() < 1e-9);
        // EaseIn Quad at t=0.5 should be 0.25
        assert!((rf.evaluate(0.5) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn rate_func_ease_out_quad() {
        let rf = RateFunc::EaseOut(EasingCurve::Quadratic);
        assert!((rf.evaluate(0.0) - 0.0).abs() < 1e-9);
        assert!((rf.evaluate(1.0) - 1.0).abs() < 1e-9);
        // EaseOut Quad at t=0.5 should be 0.75
        assert!((rf.evaluate(0.5) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn rate_func_there_and_back() {
        let rf = RateFunc::ThereAndBack;
        assert!((rf.evaluate(0.0) - 0.0).abs() < 1e-9);
        assert!((rf.evaluate(0.5) - 1.0).abs() < 1e-9);
        assert!((rf.evaluate(1.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn rate_func_mirror() {
        let rf = RateFunc::Mirror(Box::new(RateFunc::Linear));
        assert!((rf.evaluate(0.0) - 0.0).abs() < 1e-9);
        assert!((rf.evaluate(0.5) - 0.5).abs() < 1e-9);
        // Mirror of linear is linear (symmetric around (0.5, 0.5))
        assert!((rf.evaluate(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rate_func_steps() {
        let rf = RateFunc::Steps(2);
        assert_eq!(rf.evaluate(0.0), 0.0);
        assert_eq!(rf.evaluate(0.24), 0.0);
        // Steps(2): change point is at t=0.5, not 0.25
        assert_eq!(rf.evaluate(0.25), 0.0);
        assert_eq!(rf.evaluate(0.5), 0.5);
        assert_eq!(rf.evaluate(0.74), 0.5);
        assert_eq!(rf.evaluate(1.0), 1.0);
    }

    #[test]
    fn rate_func_spring_approaches_one() {
        let rf = RateFunc::Spring {
            stiffness: 90.0,
            damping: 12.0,
        };
        assert!((rf.evaluate(0.0) - 0.0).abs() < 1e-9);
        // At t=1.0 spring is close to but may slightly overshoot 1.0 due to FP math
        let v = rf.evaluate(1.0);
        assert!(
            (v - 1.0).abs() < 1e-6,
            "spring at t=1 should be close to 1, got {}",
            v
        );
    }

    #[test]
    fn rate_func_cubic_bezier_linear() {
        // Linear equivalent bezier (0.25, 0.25, 0.75, 0.75) approximates linear
        let rf = RateFunc::CubicBezier(0.25, 0.25, 0.75, 0.75);
        assert!((rf.evaluate(0.0) - 0.0).abs() < 1e-3);
        assert!((rf.evaluate(0.5) - 0.5).abs() < 1e-2);
        assert!((rf.evaluate(1.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn rate_func_custom() {
        let rf = RateFunc::Custom(Arc::new(|t| t * t));
        assert!((rf.evaluate(0.0) - 0.0).abs() < 1e-9);
        assert!((rf.evaluate(0.5) - 0.25).abs() < 1e-9);
        assert!((rf.evaluate(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rate_func_clamps_out_of_range() {
        let rf = RateFunc::Linear;
        assert_eq!(rf.evaluate(-0.5), 0.0);
        assert_eq!(rf.evaluate(1.5), 1.0);
    }
}
