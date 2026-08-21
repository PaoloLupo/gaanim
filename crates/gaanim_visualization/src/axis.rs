use gaanim_core::peniko::Color;
use gaanim_expr::Expr;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AxisError {
    #[error("axis domain must contain finite values with minimum < maximum")]
    InvalidDomain,
    #[error("logarithmic domains must be strictly positive")]
    InvalidLogDomain,
    #[error("logarithm base must be finite, positive, and different from one")]
    InvalidLogBase,
    #[error("power exponent must be finite and non-zero")]
    InvalidExponent,
    #[error("symlog threshold must be finite and positive")]
    InvalidThreshold,
    #[error("categorical axes require at least one unique non-empty category")]
    InvalidCategories,
    #[error("tick step must be finite and positive")]
    InvalidTickStep,
    #[error("value cannot be represented on this scale")]
    OutOfDomain,
    #[error("categorical scales cannot map reactive scalar expressions")]
    ReactiveCategory,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Scale {
    Linear,
    Log { base: f64 },
    SymLog { base: f64, threshold: f64 },
    Power { exponent: f64 },
    Time,
    Category { values: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Crossing {
    Auto,
    Zero,
    Minimum,
    Maximum,
    Value(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumberFormat {
    Auto,
    Fixed(usize),
    Scientific(usize),
    Percent(usize),
    Fraction { denominator: u32 },
    Pi { denominator: u32 },
    DateTime { pattern: String },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisStyle {
    pub color: Color,
    pub tick_color: Color,
    pub width: f64,
    pub tick_length: f64,
    pub tick_width: f64,
    pub number_color: Color,
    pub label_color: Color,
}

/// Optional authored overrides layered over theme-provided axis defaults.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AxisStylePatch {
    pub color: Option<Color>,
    pub tick_color: Option<Color>,
    pub width: Option<f64>,
    pub tick_length: Option<f64>,
    pub tick_width: Option<f64>,
    pub number_color: Option<Color>,
    pub label_color: Option<Color>,
}

impl Default for AxisStyle {
    fn default() -> Self {
        Self {
            color: Color::from_rgb8(0x20, 0x20, 0x20),
            tick_color: Color::from_rgb8(0x20, 0x20, 0x20),
            width: 3.0,
            tick_length: 8.0,
            tick_width: 2.0,
            number_color: Color::from_rgb8(0x20, 0x20, 0x20),
            label_color: Color::from_rgb8(0x20, 0x20, 0x20),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub value: f64,
    pub label: String,
    pub major: bool,
}

/// Reusable immutable-style axis specification. Builder methods consume and
/// return `Self`, so a cloned base spec can safely configure multiple spaces.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    scale: Scale,
    min: f64,
    max: f64,
    major_step: Option<f64>,
    minor_subdivisions: usize,
    format: NumberFormat,
    label: Option<String>,
    crossing: Crossing,
    style: AxisStyle,
    style_patch: AxisStylePatch,
}

impl Axis {
    pub fn linear(min: f64, max: f64) -> Result<Self, AxisError> {
        Self::numeric(Scale::Linear, min, max)
    }

    pub fn log(min: f64, max: f64, base: f64) -> Result<Self, AxisError> {
        if !base.is_finite() || base <= 0.0 || (base - 1.0).abs() <= f64::EPSILON {
            return Err(AxisError::InvalidLogBase);
        }
        if min <= 0.0 || max <= 0.0 {
            return Err(AxisError::InvalidLogDomain);
        }
        Self::numeric(Scale::Log { base }, min, max)
    }

    pub fn symlog(min: f64, max: f64, base: f64, threshold: f64) -> Result<Self, AxisError> {
        if !base.is_finite() || base <= 0.0 || (base - 1.0).abs() <= f64::EPSILON {
            return Err(AxisError::InvalidLogBase);
        }
        if !threshold.is_finite() || threshold <= 0.0 {
            return Err(AxisError::InvalidThreshold);
        }
        Self::numeric(Scale::SymLog { base, threshold }, min, max)
    }

    pub fn power(min: f64, max: f64, exponent: f64) -> Result<Self, AxisError> {
        if !exponent.is_finite() || exponent.abs() <= f64::EPSILON {
            return Err(AxisError::InvalidExponent);
        }
        Self::numeric(Scale::Power { exponent }, min, max)
    }

    pub fn time(min_timestamp: f64, max_timestamp: f64) -> Result<Self, AxisError> {
        Self::numeric(Scale::Time, min_timestamp, max_timestamp)
    }

    pub fn category(values: impl IntoIterator<Item = String>) -> Result<Self, AxisError> {
        let values: Vec<String> = values.into_iter().collect();
        if values.is_empty()
            || values.iter().any(|value| value.trim().is_empty())
            || values
                .iter()
                .enumerate()
                .any(|(index, value)| values[..index].contains(value))
        {
            return Err(AxisError::InvalidCategories);
        }
        Ok(Self {
            // Categories occupy unit-wide bands rather than points at the
            // frame edges. This centres category `i` at `i` and leaves a
            // half-band on each side, so a width <= 1 bar is wholly visible.
            min: -0.5,
            max: values.len() as f64 - 0.5,
            scale: Scale::Category { values },
            major_step: Some(1.0),
            minor_subdivisions: 0,
            format: NumberFormat::Auto,
            label: None,
            // A categorical axis has no meaningful zero crossing. Keep the
            // perpendicular axis on the outer band edge instead of through
            // the first category.
            crossing: Crossing::Minimum,
            style: AxisStyle::default(),
            style_patch: AxisStylePatch::default(),
        })
    }

    fn numeric(scale: Scale, min: f64, max: f64) -> Result<Self, AxisError> {
        if !min.is_finite() || !max.is_finite() || min >= max {
            return Err(AxisError::InvalidDomain);
        }
        Ok(Self {
            scale,
            min,
            max,
            major_step: None,
            minor_subdivisions: 0,
            format: NumberFormat::Auto,
            label: None,
            crossing: Crossing::Auto,
            style: AxisStyle::default(),
            style_patch: AxisStylePatch::default(),
        })
    }

    pub fn ticks(mut self, step: f64) -> Result<Self, AxisError> {
        if !step.is_finite() || step <= 0.0 {
            return Err(AxisError::InvalidTickStep);
        }
        self.major_step = Some(step);
        Ok(self)
    }

    pub fn auto_ticks(mut self) -> Self {
        self.major_step = None;
        self
    }

    pub fn minor_ticks(mut self, subdivisions: usize) -> Self {
        self.minor_subdivisions = subdivisions;
        self
    }

    pub fn numbers(mut self, format: NumberFormat) -> Self {
        self.format = format;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn crossing(mut self, crossing: Crossing) -> Self {
        self.crossing = crossing;
        self
    }

    pub fn style(mut self, style: AxisStyle) -> Self {
        self.style = style;
        self.style_patch = AxisStylePatch {
            color: Some(style.color),
            tick_color: Some(style.tick_color),
            width: Some(style.width),
            tick_length: Some(style.tick_length),
            tick_width: Some(style.tick_width),
            number_color: Some(style.number_color),
            label_color: Some(style.label_color),
        };
        self
    }

    pub fn style_patch(mut self, patch: AxisStylePatch) -> Self {
        if let Some(value) = patch.color {
            self.style.color = value;
            self.style_patch.color = Some(value);
        }
        if let Some(value) = patch.tick_color {
            self.style.tick_color = value;
            self.style_patch.tick_color = Some(value);
        }
        if let Some(value) = patch.width {
            self.style.width = value;
            self.style_patch.width = Some(value);
        }
        if let Some(value) = patch.tick_length {
            self.style.tick_length = value;
            self.style_patch.tick_length = Some(value);
        }
        if let Some(value) = patch.tick_width {
            self.style.tick_width = value;
            self.style_patch.tick_width = Some(value);
        }
        if let Some(value) = patch.number_color {
            self.style.number_color = value;
            self.style_patch.number_color = Some(value);
        }
        if let Some(value) = patch.label_color {
            self.style.label_color = value;
            self.style_patch.label_color = Some(value);
        }
        self
    }

    /// Fill non-authored style properties from a resolved theme style.
    pub fn with_theme_style(mut self, theme: AxisStyle) -> Self {
        if self.style_patch.color.is_none() {
            self.style.color = theme.color;
        }
        if self.style_patch.tick_color.is_none() {
            self.style.tick_color = theme.tick_color;
        }
        if self.style_patch.width.is_none() {
            self.style.width = theme.width;
        }
        if self.style_patch.tick_length.is_none() {
            self.style.tick_length = theme.tick_length;
        }
        if self.style_patch.tick_width.is_none() {
            self.style.tick_width = theme.tick_width;
        }
        if self.style_patch.number_color.is_none() {
            self.style.number_color = theme.number_color;
        }
        if self.style_patch.label_color.is_none() {
            self.style.label_color = theme.label_color;
        }
        self
    }

    pub fn style_overrides(&self) -> AxisStylePatch {
        self.style_patch
    }

    pub fn scale(&self) -> &Scale {
        &self.scale
    }

    pub fn domain(&self) -> (f64, f64) {
        (self.min, self.max)
    }

    pub fn label_text(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn style_value(&self) -> AxisStyle {
        self.style
    }

    pub fn crossing_value(&self) -> f64 {
        match self.crossing {
            Crossing::Auto => {
                if self.min <= 0.0 && self.max >= 0.0 {
                    0.0
                } else if self.min > 0.0 {
                    self.min
                } else {
                    self.max
                }
            }
            Crossing::Zero => 0.0,
            Crossing::Minimum => self.min,
            Crossing::Maximum => self.max,
            Crossing::Value(value) => value.clamp(self.min, self.max),
        }
    }

    pub fn normalize(&self, value: f64) -> Result<f64, AxisError> {
        if !value.is_finite() {
            return Err(AxisError::OutOfDomain);
        }
        let transform = |value: f64| -> Result<f64, AxisError> {
            match &self.scale {
                Scale::Linear | Scale::Time | Scale::Category { .. } => Ok(value),
                Scale::Log { base } => {
                    if value <= 0.0 {
                        Err(AxisError::OutOfDomain)
                    } else {
                        Ok(value.log(*base))
                    }
                }
                Scale::SymLog { base, threshold } => {
                    Ok(value.signum() * (1.0 + value.abs() / threshold).log(*base))
                }
                Scale::Power { exponent } => Ok(value.signum() * value.abs().powf(*exponent)),
            }
        };
        let min = transform(self.min)?;
        let max = transform(self.max)?;
        Ok((transform(value)? - min) / (max - min))
    }

    /// Build the native expression that maps a data value into `[0, 1]`.
    ///
    /// This is the reactive counterpart of [`Axis::normalize`]. Categorical
    /// axes reject scalar expressions because their mapping is defined by
    /// discrete string identity rather than a continuous value.
    pub fn normalize_expr(&self, value: Expr) -> Result<Expr, AxisError> {
        let transform = |value: Expr| -> Result<Expr, AxisError> {
            match &self.scale {
                Scale::Linear | Scale::Time => Ok(value),
                Scale::Category { .. } => Err(AxisError::ReactiveCategory),
                Scale::Log { base } => Ok(value.ln() / base.ln()),
                Scale::SymLog { base, threshold } => {
                    let sign = value
                        .clone()
                        .if_positive(1.0, (-value.clone()).if_positive(-1.0, 0.0));
                    Ok(sign * (1.0 + value.abs() / *threshold).ln() / base.ln())
                }
                Scale::Power { exponent } => {
                    let sign = value
                        .clone()
                        .if_positive(1.0, (-value.clone()).if_positive(-1.0, 0.0));
                    Ok(sign * value.abs().pow(*exponent))
                }
            }
        };
        let min = self.transformed_bound(self.min)?;
        let max = self.transformed_bound(self.max)?;
        Ok((transform(value)? - min) / (max - min))
    }

    fn transformed_bound(&self, value: f64) -> Result<f64, AxisError> {
        match &self.scale {
            Scale::Linear | Scale::Time | Scale::Category { .. } => Ok(value),
            Scale::Log { base } => {
                if value <= 0.0 {
                    Err(AxisError::OutOfDomain)
                } else {
                    Ok(value.log(*base))
                }
            }
            Scale::SymLog { base, threshold } => {
                Ok(value.signum() * (1.0 + value.abs() / threshold).log(*base))
            }
            Scale::Power { exponent } => Ok(value.signum() * value.abs().powf(*exponent)),
        }
    }

    pub fn denormalize(&self, normalized: f64) -> Result<f64, AxisError> {
        if !normalized.is_finite() {
            return Err(AxisError::OutOfDomain);
        }
        let transformed_bounds = match &self.scale {
            Scale::Linear | Scale::Time | Scale::Category { .. } => (self.min, self.max),
            Scale::Log { base } => (self.min.log(*base), self.max.log(*base)),
            Scale::SymLog { base, threshold } => {
                let f = |value: f64| value.signum() * (1.0 + value.abs() / threshold).log(*base);
                (f(self.min), f(self.max))
            }
            Scale::Power { exponent } => {
                let f = |value: f64| value.signum() * value.abs().powf(*exponent);
                (f(self.min), f(self.max))
            }
        };
        let transformed =
            transformed_bounds.0 + (transformed_bounds.1 - transformed_bounds.0) * normalized;
        let value = match &self.scale {
            Scale::Linear | Scale::Time | Scale::Category { .. } => transformed,
            Scale::Log { base } => base.powf(transformed),
            Scale::SymLog { base, threshold } => {
                transformed.signum() * threshold * (base.powf(transformed.abs()) - 1.0)
            }
            Scale::Power { exponent } => {
                transformed.signum() * transformed.abs().powf(1.0 / exponent)
            }
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(AxisError::OutOfDomain)
        }
    }

    pub fn ticks_values(&self, target_count: usize) -> Result<Vec<Tick>, AxisError> {
        if let Scale::Category { values } = &self.scale {
            return Ok(values
                .iter()
                .enumerate()
                .map(|(index, label)| Tick {
                    value: index as f64,
                    label: label.clone(),
                    major: true,
                })
                .collect());
        }
        if let Scale::Log { base } = self.scale {
            let start = self.min.log(base).ceil() as i32;
            let end = self.max.log(base).floor() as i32;
            return Ok((start..=end)
                .map(|exponent| {
                    let value = base.powi(exponent);
                    Tick {
                        value,
                        label: self.format_value(value),
                        major: true,
                    }
                })
                .collect());
        }
        let step = self
            .major_step
            .unwrap_or_else(|| nice_step(self.max - self.min, target_count.max(2)));
        if !step.is_finite() || step <= 0.0 {
            return Err(AxisError::InvalidTickStep);
        }
        let first = (self.min / step).ceil() * step;
        let mut ticks = Vec::new();
        let mut value = first;
        let mut guard = 0usize;
        while value <= self.max + step * 1e-9 && guard < 10_000 {
            ticks.push(Tick {
                value,
                label: self.format_value(value),
                major: true,
            });
            if self.minor_subdivisions > 1 {
                let minor_step = step / self.minor_subdivisions as f64;
                for index in 1..self.minor_subdivisions {
                    let minor = value + index as f64 * minor_step;
                    if minor < self.max - step * 1e-9 {
                        ticks.push(Tick {
                            value: minor,
                            label: String::new(),
                            major: false,
                        });
                    }
                }
            }
            value += step;
            guard += 1;
        }
        ticks.sort_by(|left, right| left.value.total_cmp(&right.value));
        Ok(ticks)
    }

    pub fn format_value(&self, value: f64) -> String {
        match &self.format {
            NumberFormat::Auto => {
                if value.abs() >= 1e6 || (value != 0.0 && value.abs() < 1e-4) {
                    format!("{value:.3e}")
                } else if (value - value.round()).abs() < 1e-9 {
                    format!("{value:.0}")
                } else {
                    let formatted = format!("{value:.6}");
                    formatted
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_owned()
                }
            }
            NumberFormat::Fixed(places) => format!("{value:.places$}"),
            NumberFormat::Scientific(places) => format!("{value:.places$e}"),
            NumberFormat::Percent(places) => format!("{:.places$}%", value * 100.0),
            NumberFormat::Fraction { denominator } => {
                let numerator = (value * *denominator as f64).round() as i64;
                if *denominator == 1 {
                    numerator.to_string()
                } else {
                    format!("{numerator}/{denominator}")
                }
            }
            NumberFormat::Pi { denominator } => {
                let numerator = (value / std::f64::consts::PI * *denominator as f64).round() as i64;
                match (numerator, denominator) {
                    (0, _) => "0".to_owned(),
                    (1, 1) => "π".to_owned(),
                    (-1, 1) => "-π".to_owned(),
                    (_, 1) => format!("{numerator}π"),
                    (1, _) => format!("π/{denominator}"),
                    (-1, _) => format!("-π/{denominator}"),
                    _ => format!("{numerator}π/{denominator}"),
                }
            }
            // Formatting timestamps is handled in Python for now; the native
            // fallback is explicit and stable for exports.
            NumberFormat::DateTime { pattern } => format!("{value:.0} {pattern}"),
        }
    }
}

fn nice_step(span: f64, target_count: usize) -> f64 {
    let rough = span.abs() / target_count.max(1) as f64;
    if rough <= f64::EPSILON {
        return 1.0;
    }
    let magnitude = 10.0_f64.powf(rough.log10().floor());
    let normalized = rough / magnitude;
    let nice = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * magnitude
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonlinear_scales_round_trip() {
        for axis in [
            Axis::log(0.1, 100.0, 10.0).unwrap(),
            Axis::symlog(-100.0, 100.0, 10.0, 1.0).unwrap(),
            Axis::power(-8.0, 8.0, 3.0).unwrap(),
        ] {
            for normalized in [0.0, 0.2, 0.5, 0.9, 1.0] {
                let value = axis.denormalize(normalized).unwrap();
                assert!((axis.normalize(value).unwrap() - normalized).abs() < 1e-10);
            }
        }
    }

    #[test]
    fn categorical_ticks_preserve_labels() {
        let axis = Axis::category(["A", "B", "C"].map(str::to_owned)).unwrap();
        assert_eq!(axis.domain(), (-0.5, 2.5));
        assert_eq!(axis.crossing_value(), -0.5);
        assert!((axis.normalize(0.0).unwrap() - 1.0 / 6.0).abs() < 1e-12);
        assert!((axis.normalize(2.0).unwrap() - 5.0 / 6.0).abs() < 1e-12);
        assert_eq!(
            axis.ticks_values(7)
                .unwrap()
                .iter()
                .map(|tick| tick.label.as_str())
                .collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
    }

    #[test]
    fn a_single_category_is_centered_in_its_band() {
        let axis = Axis::category(["only".to_owned()]).unwrap();
        assert_eq!(axis.domain(), (-0.5, 0.5));
        assert_eq!(axis.crossing_value(), -0.5);
        assert_eq!(axis.normalize(0.0).unwrap(), 0.5);
        assert_eq!(axis.denormalize(0.5).unwrap(), 0.0);
    }

    #[test]
    fn reactive_normalization_matches_scalar_scales() {
        use gaanim_expr::EvalContext;

        let axes_and_values = [
            (Axis::linear(-2.0, 6.0).unwrap(), -0.75),
            (
                Axis::time(1_700_000_000.0, 1_800_000_000.0).unwrap(),
                1_750_000_000.0,
            ),
            (Axis::log(0.1, 1_000.0, 10.0).unwrap(), 12.5),
            (Axis::symlog(-100.0, 100.0, 10.0, 1.0).unwrap(), -12.5),
            (Axis::power(-8.0, 8.0, 3.0).unwrap(), -2.5),
        ];

        for (axis, value) in axes_and_values {
            let expression = axis.normalize_expr(Expr::variable("value")).unwrap();
            let evaluated = expression
                .eval(&EvalContext::new().with_variable("value", value))
                .unwrap();
            assert!((evaluated - axis.normalize(value).unwrap()).abs() < 1e-10);
        }
    }

    #[test]
    fn categorical_axes_reject_reactive_scalars() {
        let axis = Axis::category(["A", "B"].map(str::to_owned)).unwrap();
        assert_eq!(
            axis.normalize_expr(Expr::variable("value")).unwrap_err(),
            AxisError::ReactiveCategory
        );
    }

    #[test]
    fn pi_ticks_cover_number_line_domain_exactly() {
        let ticks = Axis::linear(0.0, 3.0 * std::f64::consts::PI)
            .unwrap()
            .ticks(std::f64::consts::PI)
            .unwrap()
            .numbers(NumberFormat::Pi { denominator: 1 })
            .ticks_values(9)
            .unwrap();
        assert_eq!(
            ticks
                .iter()
                .map(|tick| tick.label.as_str())
                .collect::<Vec<_>>(),
            ["0", "\u{03c0}", "2\u{03c0}", "3\u{03c0}"]
        );
    }

    #[test]
    fn pi_formatter_is_readable() {
        let axis = Axis::linear(-3.2, 3.2)
            .unwrap()
            .numbers(NumberFormat::Pi { denominator: 2 });
        assert_eq!(axis.format_value(std::f64::consts::FRAC_PI_2), "π/2");
    }
}
