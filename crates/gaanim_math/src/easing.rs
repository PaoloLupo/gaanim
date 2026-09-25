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

/// Which end of a curve family carries its character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EaseMode {
    In,
    #[default]
    Out,
    InOut,
}

impl EaseMode {
    /// Applies this mode to an ease-in curve `f` with `f(0) = 0`, `f(1) = 1`.
    fn apply(self, t: f64, f: impl Fn(f64) -> f64) -> f64 {
        match self {
            Self::In => f(t),
            Self::Out => 1.0 - f(1.0 - t),
            Self::InOut => {
                if t < 0.5 {
                    f(t * 2.0) * 0.5
                } else {
                    1.0 - f((1.0 - t) * 2.0) * 0.5
                }
            }
        }
    }
}

/// CSS `steps()` jump positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StepJump {
    /// The first jump happens at `t = 0`.
    Start,
    /// The last jump happens at `t = 1` (the classic `Steps`).
    #[default]
    End,
    /// No jump at either end; holds 0 first and 1 last.
    None,
    /// Jumps at both ends.
    Both,
}

/// How [`RateFunc::Repeat`] chains its cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RepeatMode {
    /// Restart each cycle from the beginning.
    #[default]
    Cycle,
    /// Alternate forward and backward cycles (yoyo).
    PingPong,
    /// Continue each cycle from where the previous one ended. Scene builders
    /// expand this into sequential copies so relative animations accumulate;
    /// evaluated directly it extrapolates past 1.
    Offset,
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
    /// Damped harmonic oscillator in normalized time, solved in closed form.
    ///
    /// `omega` is the natural angular frequency per unit of normalized time,
    /// `zeta` the damping ratio, and `velocity` the initial velocity in
    /// target distances per unit of normalized time. The residual at `t = 1`
    /// is removed linearly so the curve ends exactly at 1.
    DampedSpring {
        omega: f64,
        zeta: f64,
        velocity: f64,
    },
    /// Back easing with an adjustable overshoot (1.70158 is the classic).
    Back { overshoot: f64, mode: EaseMode },
    /// Elastic easing with `amplitude >= 1` and `period` in normalized time.
    Elastic {
        amplitude: f64,
        period: f64,
        mode: EaseMode,
    },
    /// Bounce easing; `strength` 0 is a cubic ease and 1 the classic bounce.
    Bounce { strength: f64, mode: EaseMode },
    /// Fast, slow linear middle, fast (GSAP `SlowMo`).
    SlowMo { linear_ratio: f64, power: f64 },
    /// Runs `inner` only between `start` and `end`, holding its ends outside.
    Squish {
        inner: Box<RateFunc>,
        start: f64,
        end: f64,
    },
    /// Discrete steps with CSS jump positions.
    SteppedJump { count: u32, jump: StepJump },
    /// Repeats `inner` `count` times, holding `gap` (a fraction of one cycle)
    /// between cycles. The clip's duration covers all cycles and gaps.
    Repeat {
        inner: Box<RateFunc>,
        count: u32,
        gap: f64,
        mode: RepeatMode,
    },
    /// Lookup table of progress values at evenly spaced times from 0 to 1,
    /// linearly interpolated. Built once from an authored curve, so
    /// evaluation needs no callback and serializes.
    Sampled(Arc<[f64]>),
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
            Self::Custom(function) if gaanim_core::fingerprint::identity_debug() => {
                write!(
                    f,
                    "Custom({:?})",
                    gaanim_core::fingerprint::identity(&**function)
                )
            }
            Self::Custom(_) => write!(f, "Custom(<closure>)"),
            Self::DampedSpring {
                omega,
                zeta,
                velocity,
            } => write!(
                f,
                "DampedSpring {{ omega: {omega}, zeta: {zeta}, velocity: {velocity} }}"
            ),
            Self::Back { overshoot, mode } => write!(f, "Back({overshoot}, {mode:?})"),
            Self::Elastic {
                amplitude,
                period,
                mode,
            } => write!(f, "Elastic({amplitude}, {period}, {mode:?})"),
            Self::Bounce { strength, mode } => write!(f, "Bounce({strength}, {mode:?})"),
            Self::SlowMo {
                linear_ratio,
                power,
            } => write!(f, "SlowMo({linear_ratio}, {power})"),
            Self::Squish { inner, start, end } => write!(f, "Squish({inner:?}, {start}, {end})"),
            Self::SteppedJump { count, jump } => write!(f, "SteppedJump({count}, {jump:?})"),
            Self::Repeat {
                inner,
                count,
                gap,
                mode,
            } => write!(f, "Repeat({inner:?}, {count}, {gap}, {mode:?})"),
            Self::Sampled(samples) => {
                // Hash the table so equal-length curves stay distinguishable
                // in debug fingerprints.
                let hash = samples
                    .iter()
                    .fold(0xcbf2_9ce4_8422_2325_u64, |hash, value| {
                        (hash ^ value.to_bits()).wrapping_mul(0x0100_0000_01b3)
                    });
                write!(f, "Sampled({} samples, {hash:016x})", samples.len())
            }
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
            Self::DampedSpring {
                omega,
                zeta,
                velocity,
            } => {
                let mut state = serializer.serialize_struct("DampedSpring", 3)?;
                state.serialize_field("omega", omega)?;
                state.serialize_field("zeta", zeta)?;
                state.serialize_field("velocity", velocity)?;
                state.end()
            }
            Self::Back { overshoot, mode } => {
                let mut state = serializer.serialize_struct("Back", 2)?;
                state.serialize_field("overshoot", overshoot)?;
                state.serialize_field("mode", mode)?;
                state.end()
            }
            Self::Elastic {
                amplitude,
                period,
                mode,
            } => {
                let mut state = serializer.serialize_struct("Elastic", 3)?;
                state.serialize_field("amplitude", amplitude)?;
                state.serialize_field("period", period)?;
                state.serialize_field("mode", mode)?;
                state.end()
            }
            Self::Bounce { strength, mode } => {
                let mut state = serializer.serialize_struct("Bounce", 2)?;
                state.serialize_field("strength", strength)?;
                state.serialize_field("mode", mode)?;
                state.end()
            }
            Self::SlowMo {
                linear_ratio,
                power,
            } => {
                let mut state = serializer.serialize_struct("SlowMo", 2)?;
                state.serialize_field("linear_ratio", linear_ratio)?;
                state.serialize_field("power", power)?;
                state.end()
            }
            Self::Squish { inner, start, end } => {
                let mut state = serializer.serialize_struct("Squish", 3)?;
                state.serialize_field("inner", inner)?;
                state.serialize_field("start", start)?;
                state.serialize_field("end", end)?;
                state.end()
            }
            Self::Repeat {
                inner,
                count,
                gap,
                mode,
            } => {
                let mut state = serializer.serialize_struct("Repeat", 4)?;
                state.serialize_field("repeat_inner", inner)?;
                state.serialize_field("count", count)?;
                state.serialize_field("gap", gap)?;
                state.serialize_field("mode", mode)?;
                state.end()
            }
            Self::SteppedJump { count, jump } => {
                let mut state = serializer.serialize_struct("SteppedJump", 2)?;
                state.serialize_field("count", count)?;
                state.serialize_field("jump", jump)?;
                state.end()
            }
            Self::Sampled(samples) => {
                let mut state = serializer.serialize_struct("Sampled", 1)?;
                state.serialize_field("samples", &samples[..])?;
                state.end()
            }
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
            Repeat {
                repeat_inner: Box<RateFunc>,
                count: u32,
                gap: f64,
                mode: RepeatMode,
            },
            DampedSpring {
                omega: f64,
                zeta: f64,
                velocity: f64,
            },
            Elastic {
                amplitude: f64,
                period: f64,
                mode: EaseMode,
            },
            Squish {
                inner: Box<RateFunc>,
                start: f64,
                end: f64,
            },
            Back {
                overshoot: f64,
                mode: EaseMode,
            },
            Bounce {
                strength: f64,
                mode: EaseMode,
            },
            SlowMo {
                linear_ratio: f64,
                power: f64,
            },
            SteppedJump {
                count: u32,
                jump: StepJump,
            },
            Simple(String),
            Ease(String, EasingCurve),
            Spring {
                stiffness: f64,
                damping: f64,
            },
            Steps(u32),
            Mirror(Box<RateFunc>),
            ThereAndBackWithPause(f64),
            CubicBezier {
                x1: f64,
                y1: f64,
                x2: f64,
                y2: f64,
            },
            Sampled {
                samples: Vec<f64>,
            },
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
            RawRateFunc::Sampled { samples } => Ok(Self::Sampled(samples.into())),
            RawRateFunc::DampedSpring {
                omega,
                zeta,
                velocity,
            } => Ok(Self::DampedSpring {
                omega,
                zeta,
                velocity,
            }),
            RawRateFunc::Elastic {
                amplitude,
                period,
                mode,
            } => Ok(Self::Elastic {
                amplitude,
                period,
                mode,
            }),
            RawRateFunc::Squish { inner, start, end } => Ok(Self::Squish { inner, start, end }),
            RawRateFunc::Back { overshoot, mode } => Ok(Self::Back { overshoot, mode }),
            RawRateFunc::Bounce { strength, mode } => Ok(Self::Bounce { strength, mode }),
            RawRateFunc::SlowMo {
                linear_ratio,
                power,
            } => Ok(Self::SlowMo {
                linear_ratio,
                power,
            }),
            RawRateFunc::SteppedJump { count, jump } => Ok(Self::SteppedJump { count, jump }),
            RawRateFunc::Repeat {
                repeat_inner,
                count,
                gap,
                mode,
            } => Ok(Self::Repeat {
                inner: repeat_inner,
                count,
                gap,
                mode,
            }),
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
            Self::DampedSpring {
                omega,
                zeta,
                velocity,
            } => {
                let displacement = |tau: f64| Self::damped_spring(*omega, *zeta, *velocity, tau);
                // Remove the tiny residual linearly so the clip ends on target.
                displacement(t) + (1.0 - displacement(1.0)) * t
            }
            Self::Back { overshoot, mode } => {
                let s = *overshoot;
                mode.apply(t, |u| u * u * ((s + 1.0) * u - s))
            }
            Self::Elastic {
                amplitude,
                period,
                mode,
            } => {
                let a = amplitude.max(1.0);
                let p = period.max(1e-3);
                let s = p / std::f64::consts::TAU * (1.0 / a).asin();
                mode.apply(t, |u| {
                    if u <= 0.0 {
                        0.0
                    } else if u >= 1.0 {
                        1.0
                    } else {
                        let v = u - 1.0;
                        -(a * 2.0f64.powf(10.0 * v) * ((v - s) * std::f64::consts::TAU / p).sin())
                    }
                })
            }
            Self::Bounce { strength, mode } => {
                let k = strength.clamp(0.0, 1.0);
                mode.apply(t, |u| {
                    (1.0 - k) * u * u * u + k * (1.0 - Self::eval_bounce_out(1.0 - u))
                })
            }
            Self::SlowMo {
                linear_ratio,
                power,
            } => {
                let ratio = linear_ratio.clamp(0.0, 1.0);
                let p1 = (1.0 - ratio) / 2.0;
                let p3 = p1 + ratio;
                let r = t + (0.5 - t) * power;
                if p1 > 0.0 && t < p1 {
                    let q = 1.0 - t / p1;
                    r - q * q * q * q * r
                } else if p1 > 0.0 && t > p3 {
                    let q = (t - p3) / p1;
                    r + (t - r) * q * q * q * q
                } else {
                    r
                }
            }
            Self::Squish { inner, start, end } => {
                if end <= start {
                    return if t < *start {
                        inner.evaluate(0.0)
                    } else {
                        inner.evaluate(1.0)
                    };
                }
                inner.evaluate(((t - start) / (end - start)).clamp(0.0, 1.0))
            }
            Self::SteppedJump { count, jump } => {
                let n = (*count).max(1) as f64;
                match jump {
                    StepJump::End => (t * n).floor().min(n) / n,
                    StepJump::Start => (t * n).ceil() / n,
                    StepJump::None if n >= 2.0 => ((t * n).floor() / (n - 1.0)).min(1.0),
                    StepJump::None => t,
                    StepJump::Both => (((t * n).floor() + 1.0) / (n + 1.0)).min(1.0),
                }
            }
            Self::Repeat {
                inner,
                count,
                gap,
                mode,
            } => {
                let (cycle, progress) = Self::repeat_position(*count, *gap, t);
                match mode {
                    RepeatMode::Cycle => inner.evaluate(progress),
                    RepeatMode::PingPong if cycle % 2 == 1 => inner.evaluate(1.0 - progress),
                    RepeatMode::PingPong => inner.evaluate(progress),
                    RepeatMode::Offset => cycle as f64 + inner.evaluate(progress),
                }
            }
            Self::Sampled(samples) => match samples.len() {
                0 => t,
                1 => samples[0],
                len => {
                    let position = t * (len - 1) as f64;
                    let index = (position.floor() as usize).min(len - 2);
                    let fraction = position - index as f64;
                    samples[index] + (samples[index + 1] - samples[index]) * fraction
                }
            },
        }
    }

    /// Cycle index and progress within it for [`RateFunc::Repeat`] at `t`.
    /// During a gap the progress holds at 1.
    fn repeat_position(count: u32, gap: f64, t: f64) -> (u32, f64) {
        let count = count.max(1);
        let gap = gap.max(0.0);
        let total = count as f64 + (count - 1) as f64 * gap;
        let elapsed = t * total;
        let period = 1.0 + gap;
        let cycle = ((elapsed / period).floor() as u32).min(count - 1);
        let progress = ((elapsed - cycle as f64 * period).min(1.0)).max(0.0);
        (cycle, progress)
    }

    /// Whether this rate function finishes where it started (an even number
    /// of ping-pong cycles), so the animated property returns to its start.
    pub fn ends_at_start(&self) -> bool {
        matches!(
            self,
            Self::Repeat {
                count,
                mode: RepeatMode::PingPong,
                ..
            } if count % 2 == 0
        )
    }

    /// Position of a unit step response `x(0) = 0`, `x'(0) = velocity`.
    fn damped_spring(omega: f64, zeta: f64, velocity: f64, tau: f64) -> f64 {
        let omega = omega.max(1e-6);
        let zeta = zeta.max(0.0);
        // y = x - 1 with y(0) = -1 and y'(0) = velocity.
        let offset = if zeta < 1.0 - 1e-6 {
            let omega_d = omega * (1.0 - zeta * zeta).sqrt();
            let b = (velocity - zeta * omega) / omega_d;
            (-zeta * omega * tau).exp() * (-(omega_d * tau).cos() + b * (omega_d * tau).sin())
        } else if zeta <= 1.0 + 1e-6 {
            (-omega * tau).exp() * (-1.0 + (velocity - omega) * tau)
        } else {
            let root = (zeta * zeta - 1.0).sqrt();
            let r1 = -omega * (zeta - root);
            let r2 = -omega * (zeta + root);
            let c1 = (velocity + r2) / (r1 - r2);
            let c2 = -1.0 - c1;
            c1 * (r1 * tau).exp() + c2 * (r2 * tau).exp()
        };
        1.0 + offset
    }

    /// Perceptual spring: the peak overshoot is `bounce` (0 is critically
    /// damped) and the motion settles within the clip. `speed` scales how
    /// early it settles; 1 settles to about 0.1% at the end.
    pub fn spring_bounce(bounce: f64, velocity: f64, speed: f64) -> Self {
        let bounce = bounce.clamp(0.0, 0.95);
        let zeta = if bounce <= 0.0 {
            1.0
        } else {
            let log = bounce.ln();
            -log / (std::f64::consts::PI * std::f64::consts::PI + log * log).sqrt()
        };
        // The envelope e^(-zeta * omega * t) reaches 1e-3 at t = 1 / speed.
        let omega = 1000f64.ln() / zeta * speed.max(1e-3);
        Self::DampedSpring {
            omega,
            zeta,
            velocity,
        }
    }

    /// Deterministic jittered ramp through `points` interior knots
    /// (GSAP `RoughEase`). Ends stay at 0 and 1.
    pub fn rough(strength: f64, points: u32, seed: u64) -> Self {
        let points = points.max(1);
        let mut rng = crate::random::SeededRng::new(seed);
        let knots = points as usize + 1;
        let table: Vec<f64> = (0..=knots)
            .map(|index| {
                let base = index as f64 / knots as f64;
                if index == 0 || index == knots {
                    base
                } else {
                    (base + rng.uniform(-0.5, 0.5) * strength * 0.4).clamp(-1.0, 2.0)
                }
            })
            .collect();
        Self::Sampled(table.into())
    }

    /// Samples an SVG path drawn in the unit square (x = time, y = progress)
    /// into a lookup table. The path must start at x = 0, end at x = 1, and
    /// never move backwards in x.
    pub fn from_svg_path(data: &str, samples: usize) -> Result<Self, String> {
        let path =
            kurbo::BezPath::from_svg(data).map_err(|error| format!("invalid SVG path: {error}"))?;
        let mut polyline: Vec<(f64, f64)> = Vec::new();
        kurbo::flatten(&path, 1e-4, |element| match element {
            kurbo::PathEl::MoveTo(point) | kurbo::PathEl::LineTo(point) => {
                polyline.push((point.x, point.y))
            }
            _ => {}
        });
        let (Some(first), Some(last)) = (polyline.first(), polyline.last()) else {
            return Err("the SVG path is empty".into());
        };
        if first.0.abs() > 1e-6 || (last.0 - 1.0).abs() > 1e-6 {
            return Err("the SVG path must run from x = 0 to x = 1".into());
        }
        if polyline.windows(2).any(|pair| pair[1].0 < pair[0].0 - 1e-9) {
            return Err("the SVG path must not move backwards in x".into());
        }
        let samples = samples.max(2);
        let mut segment = 0;
        let table: Vec<f64> = (0..samples)
            .map(|index| {
                let x = index as f64 / (samples - 1) as f64;
                while segment + 2 < polyline.len() && polyline[segment + 1].0 < x {
                    segment += 1;
                }
                let (a, b) = (
                    polyline[segment],
                    polyline[(segment + 1).min(polyline.len() - 1)],
                );
                if b.0 - a.0 <= 1e-12 {
                    b.1
                } else {
                    a.1 + (b.1 - a.1) * ((x - a.0) / (b.0 - a.0)).clamp(0.0, 1.0)
                }
            })
            .collect();
        if table
            .iter()
            .any(|value| !value.is_finite() || !(-1.0..=2.0).contains(value))
        {
            return Err("the SVG path must stay within y in [-1, 2]".into());
        }
        Ok(Self::Sampled(table.into()))
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
    fn repeat_cycles_ping_pongs_and_holds_gaps() {
        let repeat = |count, gap, mode| RateFunc::Repeat {
            inner: Box::new(RateFunc::Linear),
            count,
            gap,
            mode,
        };
        let cycle = repeat(3, 0.0, RepeatMode::Cycle);
        assert_eq!(cycle.evaluate(0.0), 0.0);
        assert!((cycle.evaluate(1.0 / 6.0) - 0.5).abs() < 1e-12);
        assert!((cycle.evaluate(0.5) - 0.5).abs() < 1e-12);
        assert_eq!(cycle.evaluate(1.0), 1.0);

        let yoyo = repeat(2, 0.0, RepeatMode::PingPong);
        assert!((yoyo.evaluate(0.25) - 0.5).abs() < 1e-12);
        assert!((yoyo.evaluate(0.5) - 1.0).abs() < 1e-12);
        assert!((yoyo.evaluate(0.75) - 0.5).abs() < 1e-12);
        assert_eq!(yoyo.evaluate(1.0), 0.0);
        assert!(yoyo.ends_at_start());
        assert!(!repeat(3, 0.0, RepeatMode::PingPong).ends_at_start());

        // Two cycles with a half-cycle gap span 2.5 cycle lengths.
        let gapped = repeat(2, 0.5, RepeatMode::Cycle);
        assert_eq!(gapped.evaluate(1.2 / 2.5), 1.0);
        assert!((gapped.evaluate(2.0 / 2.5) - 0.5).abs() < 1e-12);

        let offset = repeat(3, 0.0, RepeatMode::Offset);
        assert!((offset.evaluate(1.0) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn perceptual_spring_overshoots_by_bounce_and_ends_on_target() {
        for bounce in [0.0, 0.1, 0.35, 0.6] {
            let spring = RateFunc::spring_bounce(bounce, 0.0, 1.0);
            assert_eq!(spring.evaluate(0.0), 0.0);
            assert!((spring.evaluate(1.0) - 1.0).abs() < 1e-12);
            let peak = (0..=4000)
                .map(|step| spring.evaluate(step as f64 / 4000.0))
                .fold(f64::MIN, f64::max);
            assert!(
                (peak - 1.0 - bounce).abs() < 0.01,
                "bounce {bounce}: peak {peak}"
            );
            // Continuity: no jump right after start or right before the end.
            assert!(spring.evaluate(1e-4) < 0.01);
            assert!((spring.evaluate(1.0 - 1e-4) - 1.0).abs() < 0.01);
        }
        let launched = RateFunc::DampedSpring {
            omega: 12.0,
            zeta: 0.5,
            velocity: 6.0,
        };
        let slope = launched.evaluate(1e-5) / 1e-5;
        assert!((slope - 6.0).abs() < 0.1, "initial slope {slope}");
        for (omega, zeta) in [(8.0, 1.0), (8.0, 2.5)] {
            let spring = RateFunc::DampedSpring {
                omega,
                zeta,
                velocity: 0.0,
            };
            assert!(spring.evaluate(0.0).abs() < 1e-12);
            assert!((spring.evaluate(1.0) - 1.0).abs() < 1e-12);
            assert!((0..=100).all(|step| spring.evaluate(step as f64 / 100.0) <= 1.0 + 1e-9));
        }
    }

    #[test]
    fn parametric_easings_hit_their_ends_and_control_points() {
        let modes = [EaseMode::In, EaseMode::Out, EaseMode::InOut];
        for mode in modes {
            let curves = [
                RateFunc::Back {
                    overshoot: 2.5,
                    mode,
                },
                RateFunc::Elastic {
                    amplitude: 1.4,
                    period: 0.4,
                    mode,
                },
                RateFunc::Bounce {
                    strength: 0.6,
                    mode,
                },
            ];
            for curve in curves {
                assert!(curve.evaluate(0.0).abs() < 1e-9, "{curve:?} at 0");
                assert!((curve.evaluate(1.0) - 1.0).abs() < 1e-9, "{curve:?} at 1");
            }
        }
        let classic_back = RateFunc::Back {
            overshoot: 1.70158,
            mode: EaseMode::In,
        };
        let legacy = RateFunc::EaseIn(EasingCurve::Back);
        let bigger = RateFunc::Back {
            overshoot: 3.0,
            mode: EaseMode::In,
        };
        assert!((classic_back.evaluate(0.3) - legacy.evaluate(0.3)).abs() < 1e-12);
        assert!(bigger.evaluate(0.3) < classic_back.evaluate(0.3));
        let classic_elastic = RateFunc::Elastic {
            amplitude: 1.0,
            period: 0.3,
            mode: EaseMode::In,
        };
        let legacy = RateFunc::EaseIn(EasingCurve::Elastic);
        assert!((classic_elastic.evaluate(0.7) - legacy.evaluate(0.7)).abs() < 1e-12);
        let full_bounce = RateFunc::Bounce {
            strength: 1.0,
            mode: EaseMode::Out,
        };
        assert!(
            (full_bounce.evaluate(0.5) - RateFunc::EaseOut(EasingCurve::Bounce).evaluate(0.5))
                .abs()
                < 1e-12
        );

        let slow_mo = RateFunc::SlowMo {
            linear_ratio: 0.7,
            power: 0.7,
        };
        assert_eq!(slow_mo.evaluate(0.0), 0.0);
        assert!((slow_mo.evaluate(1.0) - 1.0).abs() < 1e-12);
        // The linear middle advances slower than time.
        let middle = slow_mo.evaluate(0.6) - slow_mo.evaluate(0.4);
        assert!(middle < 0.2 * 0.5, "middle slope {middle}");

        let squish = RateFunc::Squish {
            inner: Box::new(RateFunc::Linear),
            start: 0.2,
            end: 0.8,
        };
        assert_eq!(squish.evaluate(0.1), 0.0);
        assert!((squish.evaluate(0.5) - 0.5).abs() < 1e-12);
        assert_eq!(squish.evaluate(0.9), 1.0);

        let step = |jump, t| RateFunc::SteppedJump { count: 4, jump }.evaluate(t);
        assert_eq!(step(StepJump::End, 0.3), 0.25);
        assert_eq!(step(StepJump::End, 1.0), 1.0);
        assert_eq!(step(StepJump::Start, 0.1), 0.25);
        assert_eq!(step(StepJump::None, 0.1), 0.0);
        assert_eq!(step(StepJump::None, 0.99), 1.0);
        assert_eq!(step(StepJump::Both, 0.0), 0.2);
        assert_eq!(step(StepJump::Both, 0.99), 0.8);
    }

    #[test]
    fn rough_and_svg_easings_sample_to_tables() {
        let rough = RateFunc::rough(1.0, 20, 4);
        assert_eq!(rough.evaluate(0.0), 0.0);
        assert_eq!(rough.evaluate(1.0), 1.0);
        assert_eq!(
            format!("{rough:?}"),
            format!("{:?}", RateFunc::rough(1.0, 20, 4))
        );
        assert_ne!(
            format!("{rough:?}"),
            format!("{:?}", RateFunc::rough(1.0, 20, 5))
        );
        let jitter = (1..100)
            .map(|step| (rough.evaluate(step as f64 / 100.0) - step as f64 / 100.0).abs())
            .fold(0.0, f64::max);
        assert!(jitter > 0.02);

        let svg = RateFunc::from_svg_path("M0,0 C0.3,0 0.2,1.2 1,1", 256).unwrap();
        assert!(svg.evaluate(0.0).abs() < 1e-9);
        assert!((svg.evaluate(1.0) - 1.0).abs() < 1e-9);
        let css = RateFunc::CubicBezier(0.3, 0.0, 0.2, 1.2);
        for t in [0.25, 0.5, 0.75] {
            assert!((svg.evaluate(t) - css.evaluate(t)).abs() < 0.01, "t={t}");
        }
        assert!(RateFunc::from_svg_path("M0,0 L0.5,1", 64).is_err());
        assert!(RateFunc::from_svg_path("M0,0 L0.6,0.5 L0.4,0.6 L1,1", 64).is_err());
        assert!(RateFunc::from_svg_path("M0,0 L0.5,3 L1,1", 64).is_err());
        assert!(RateFunc::from_svg_path("nonsense", 64).is_err());
    }

    #[test]
    fn sampled_rate_func_interpolates_its_table() {
        let quartic_out: Arc<[f64]> = (0..=256)
            .map(|i| 1.0 - (1.0 - i as f64 / 256.0).powi(4))
            .collect();
        let sampled = RateFunc::Sampled(quartic_out);
        assert_eq!(sampled.evaluate(0.0), 0.0);
        assert_eq!(sampled.evaluate(1.0), 1.0);
        assert_eq!(sampled.evaluate(-0.5), 0.0);
        assert_eq!(sampled.evaluate(1.5), 1.0);
        let mut previous = f64::NEG_INFINITY;
        for step in 0..=1000 {
            let t = step as f64 / 1000.0;
            let value = sampled.evaluate(t);
            assert!(value >= previous, "not monotone at t={t}");
            assert!((value - (1.0 - (1.0 - t).powi(4))).abs() < 1e-4);
            previous = value;
        }

        let bump = RateFunc::Sampled(Arc::from([0.0, 1.0, 0.0]));
        assert_eq!(bump.evaluate(0.25), 0.5);
        assert_eq!(bump.evaluate(0.5), 1.0);
        assert_eq!(RateFunc::Sampled(Arc::from([0.3])).evaluate(0.7), 0.3);
        assert_ne!(
            format!("{bump:?}"),
            format!("{:?}", RateFunc::Sampled(Arc::from([0.0, 0.5, 0.0])))
        );
    }

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
