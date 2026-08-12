use std::collections::{BTreeMap, BTreeSet};

use gaanim_core::peniko::Color;

use crate::{Axis, Column, Crossing, DataError, DataTable, Scale};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    X,
    Y,
    Z,
    Color,
    Size,
    Opacity,
    Label,
}

impl Channel {
    pub fn parse(value: &str) -> Result<Self, ChartError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "x" => Ok(Self::X),
            "y" => Ok(Self::Y),
            "z" => Ok(Self::Z),
            "color" | "colour" => Ok(Self::Color),
            "size" => Ok(Self::Size),
            "opacity" => Ok(Self::Opacity),
            "label" => Ok(Self::Label),
            _ => Err(ChartError::UnknownChannel(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Point,
    Line,
    Step,
    Area,
    Bar,
    Histogram,
    Box,
    Violin,
    ErrorBar,
    Heatmap,
    Surface,
}

impl MarkKind {
    pub fn parse(value: &str) -> Result<Self, ChartError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "point" | "scatter" => Ok(Self::Point),
            "line" => Ok(Self::Line),
            "step" => Ok(Self::Step),
            "area" => Ok(Self::Area),
            "bar" | "bars" => Ok(Self::Bar),
            "histogram" | "hist" => Ok(Self::Histogram),
            "box" | "box_plot" => Ok(Self::Box),
            "violin" => Ok(Self::Violin),
            "error_bar" | "error_bars" => Ok(Self::ErrorBar),
            "heatmap" => Ok(Self::Heatmap),
            "surface" => Ok(Self::Surface),
            _ => Err(ChartError::UnknownMark(value.to_owned())),
        }
    }

    pub fn is_three_dimensional(self, encodings: &BTreeMap<Channel, Encoding>) -> bool {
        encodings.contains_key(&Channel::Z) || self == Self::Surface
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstantValue {
    Number(f64),
    Text(String),
    Color(Color),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScaleKind {
    Linear,
    Log { base: f64 },
    SymLog { base: f64, threshold: f64 },
    Power { exponent: f64 },
    Time,
    Category,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScaleSpec {
    pub kind: ScaleKind,
    pub domain: Option<(f64, f64)>,
    pub categories: Vec<String>,
    pub colors: Vec<Color>,
    pub clamp: bool,
}

impl ScaleSpec {
    pub fn linear(domain: Option<(f64, f64)>) -> Result<Self, ChartError> {
        Self::numeric(ScaleKind::Linear, domain)
    }

    pub fn log(domain: Option<(f64, f64)>, base: f64) -> Result<Self, ChartError> {
        if !base.is_finite() || base <= 0.0 || (base - 1.0).abs() <= f64::EPSILON {
            return Err(ChartError::InvalidScale);
        }
        if domain.is_some_and(|(min, _)| min <= 0.0) {
            return Err(ChartError::InvalidScale);
        }
        Self::numeric(ScaleKind::Log { base }, domain)
    }

    pub fn symlog(
        domain: Option<(f64, f64)>,
        base: f64,
        threshold: f64,
    ) -> Result<Self, ChartError> {
        if !base.is_finite()
            || base <= 0.0
            || (base - 1.0).abs() <= f64::EPSILON
            || !threshold.is_finite()
            || threshold <= 0.0
        {
            return Err(ChartError::InvalidScale);
        }
        Self::numeric(ScaleKind::SymLog { base, threshold }, domain)
    }

    pub fn power(domain: Option<(f64, f64)>, exponent: f64) -> Result<Self, ChartError> {
        if !exponent.is_finite() || exponent.abs() <= f64::EPSILON {
            return Err(ChartError::InvalidScale);
        }
        Self::numeric(ScaleKind::Power { exponent }, domain)
    }

    pub fn time(domain: Option<(f64, f64)>) -> Result<Self, ChartError> {
        Self::numeric(ScaleKind::Time, domain)
    }

    pub fn category(values: impl IntoIterator<Item = String>) -> Result<Self, ChartError> {
        let categories: Vec<String> = values.into_iter().collect();
        if categories.iter().any(|value| value.trim().is_empty())
            || categories
                .iter()
                .enumerate()
                .any(|(index, value)| categories[..index].contains(value))
        {
            return Err(ChartError::InvalidScale);
        }
        Ok(Self {
            kind: ScaleKind::Category,
            domain: None,
            categories,
            colors: Vec::new(),
            clamp: false,
        })
    }

    fn numeric(kind: ScaleKind, domain: Option<(f64, f64)>) -> Result<Self, ChartError> {
        if domain.is_some_and(|(min, max)| !min.is_finite() || !max.is_finite() || min >= max) {
            return Err(ChartError::InvalidScale);
        }
        Ok(Self {
            kind,
            domain,
            categories: Vec::new(),
            colors: Vec::new(),
            clamp: false,
        })
    }

    pub fn colors(mut self, colors: impl IntoIterator<Item = Color>) -> Self {
        self.colors = colors.into_iter().collect();
        self
    }

    pub fn clamp(mut self, clamp: bool) -> Self {
        self.clamp = clamp;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Encoding {
    Field {
        column: String,
        scale: Option<ScaleSpec>,
    },
    Value(ConstantValue),
}

impl Encoding {
    pub fn field(column: impl Into<String>) -> Self {
        Self::Field {
            column: column.into(),
            scale: None,
        }
    }

    pub fn scaled_field(column: impl Into<String>, scale: ScaleSpec) -> Self {
        Self::Field {
            column: column.into(),
            scale: Some(scale),
        }
    }

    pub fn column(&self) -> Option<&str> {
        match self {
            Self::Field { column, .. } => Some(column),
            Self::Value(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkSpec {
    pub kind: MarkKind,
    pub options: BTreeMap<String, ConstantValue>,
}

impl Default for MarkSpec {
    fn default() -> Self {
        Self {
            kind: MarkKind::Point,
            options: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GuideSpec {
    None,
    Legend { title: Option<String> },
    ColorBar { title: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartSpec {
    data: DataTable,
    key: Option<String>,
    mark: MarkSpec,
    encodings: BTreeMap<Channel, Encoding>,
    axes: BTreeMap<Channel, Axis>,
    guides: BTreeMap<Channel, GuideSpec>,
}

impl ChartSpec {
    pub fn new(data: DataTable, key: Option<String>) -> Result<Self, ChartError> {
        let spec = Self {
            data,
            key,
            mark: MarkSpec::default(),
            encodings: BTreeMap::new(),
            axes: BTreeMap::new(),
            guides: BTreeMap::new(),
        };
        spec.validate_key()?;
        Ok(spec)
    }

    pub fn data(&self) -> &DataTable {
        &self.data
    }

    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    pub fn mark_spec(&self) -> &MarkSpec {
        &self.mark
    }

    pub fn encodings(&self) -> &BTreeMap<Channel, Encoding> {
        &self.encodings
    }

    pub fn axes_specs(&self) -> &BTreeMap<Channel, Axis> {
        &self.axes
    }

    pub fn guides_specs(&self) -> &BTreeMap<Channel, GuideSpec> {
        &self.guides
    }

    pub fn mark(mut self, kind: MarkKind, options: BTreeMap<String, ConstantValue>) -> Self {
        self.mark = MarkSpec { kind, options };
        self
    }

    pub fn encode(mut self, channel: Channel, encoding: Encoding) -> Result<Self, ChartError> {
        self.validate_encoding(channel, &encoding)?;
        self.encodings.insert(channel, encoding);
        Ok(self)
    }

    pub fn axis(mut self, channel: Channel, axis: Axis) -> Result<Self, ChartError> {
        if !matches!(channel, Channel::X | Channel::Y | Channel::Z) {
            return Err(ChartError::AxisOnNonPositionalChannel);
        }
        self.axes.insert(channel, axis);
        Ok(self)
    }

    pub fn guide(mut self, channel: Channel, guide: GuideSpec) -> Self {
        self.guides.insert(channel, guide);
        self
    }

    pub fn validate(&self) -> Result<(), ChartError> {
        self.validate_key()?;
        for (channel, encoding) in &self.encodings {
            self.validate_encoding(*channel, encoding)?;
        }
        let has = |channel| self.encodings.contains_key(&channel);
        let valid = match self.mark.kind {
            MarkKind::Point | MarkKind::Line | MarkKind::Step | MarkKind::Area | MarkKind::Bar => {
                has(Channel::X) && has(Channel::Y)
            }
            MarkKind::Histogram | MarkKind::Box | MarkKind::Violin => {
                has(Channel::X) || has(Channel::Y)
            }
            MarkKind::ErrorBar => {
                has(Channel::X)
                    && has(Channel::Y)
                    && self.mark.options.contains_key("low")
                    && self.mark.options.contains_key("high")
            }
            MarkKind::Heatmap => has(Channel::X) && has(Channel::Y) && has(Channel::Color),
            MarkKind::Surface => has(Channel::X) && has(Channel::Y) && has(Channel::Z),
        };
        if valid {
            Ok(())
        } else {
            Err(ChartError::MissingRequiredEncoding(self.mark.kind))
        }
    }

    pub fn batch(&self) -> Result<MarkBatch, ChartError> {
        self.validate()?;
        let axes = self.resolved_axes()?;
        let mut data = Vec::with_capacity(self.data.len());
        for row in 0..self.data.len() {
            let key = self.row_key(row)?;
            let position = [
                self.position_value(row, Channel::X, axes.get(&Channel::X))?,
                self.position_value(row, Channel::Y, axes.get(&Channel::Y))?,
                self.position_value(row, Channel::Z, axes.get(&Channel::Z))?,
            ];
            let size = self.numeric_value(row, Channel::Size)?.unwrap_or(8.0);
            let opacity = self
                .numeric_value(row, Channel::Opacity)?
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let color = self.color_value(row)?;
            let label = self.label_value(row)?;
            data.push(BatchDatum {
                key,
                source_row: row,
                position,
                size,
                opacity,
                color,
                label,
            });
        }
        Ok(MarkBatch {
            mark: self.mark.kind,
            dimensions: if self.mark.kind.is_three_dimensional(&self.encodings) {
                3
            } else {
                2
            },
            data,
        })
    }

    pub fn transition_to(
        &self,
        target: &Self,
        matching: MatchPolicy,
        fallback: TransitionFallback,
    ) -> Result<MarkTransition, ChartError> {
        let source = self.batch()?;
        let target_batch = target.batch()?;
        let native = native_transition(
            self.mark.kind,
            target.mark.kind,
            source.dimensions,
            target_batch.dimensions,
        );
        if !native && fallback != TransitionFallback::Crossfade {
            return Err(ChartError::IncompatibleTransition {
                source_mark: self.mark.kind,
                target_mark: target.mark.kind,
            });
        }
        if !native {
            return Ok(MarkTransition {
                kind: TransitionKind::Crossfade,
                source,
                target: target_batch,
                pairs: Vec::new(),
            });
        }
        let pairs = match matching {
            MatchPolicy::Key => match_by_key(&source, &target_batch)?,
            MatchPolicy::Index => match_by_index(&source, &target_batch),
        };
        Ok(MarkTransition {
            kind: TransitionKind::Morph,
            source,
            target: target_batch,
            pairs,
        })
    }

    fn validate_encoding(&self, channel: Channel, encoding: &Encoding) -> Result<(), ChartError> {
        match encoding {
            Encoding::Field { column, .. } => {
                self.data.column(column)?;
            }
            Encoding::Value(ConstantValue::Number(value))
                if !value.is_finite()
                    || (channel == Channel::Size && *value < 0.0)
                    || (channel == Channel::Opacity && !(0.0..=1.0).contains(value)) =>
            {
                return Err(ChartError::InvalidConstant);
            }
            Encoding::Value(_) => {}
        }
        Ok(())
    }

    fn validate_key(&self) -> Result<(), ChartError> {
        let Some(key) = self.key.as_deref() else {
            return Ok(());
        };
        self.data.column(key)?;
        let mut seen = BTreeSet::new();
        for row in 0..self.data.len() {
            let value = self.row_key(row)?.ok_or(ChartError::MissingKey { row })?;
            if !seen.insert(value.clone()) {
                return Err(ChartError::DuplicateKey { row });
            }
        }
        Ok(())
    }

    fn row_key(&self, row: usize) -> Result<Option<DatumKey>, ChartError> {
        let Some(column) = self.key.as_deref() else {
            return Ok(None);
        };
        match self.data.value(row, column)? {
            Some(crate::DataValue::Number(value)) => Ok(Some(DatumKey::Number(value.to_bits()))),
            Some(crate::DataValue::Text(value)) => Ok(Some(DatumKey::Text(value))),
            Some(crate::DataValue::Missing) | None => Ok(None),
        }
    }

    pub fn resolved_axes(&self) -> Result<BTreeMap<Channel, Axis>, ChartError> {
        let mut result = self.axes.clone();
        for channel in [Channel::X, Channel::Y, Channel::Z] {
            if result.contains_key(&channel) || !self.encodings.contains_key(&channel) {
                continue;
            }
            let encoding = &self.encodings[&channel];
            let axis = infer_axis(&self.data, encoding)?;
            result.insert(channel, axis);
        }
        if self.mark.kind == MarkKind::Bar && !self.encodings.contains_key(&Channel::Z) {
            self.pad_inferred_bar_axes(&mut result)?;
        }
        Ok(result)
    }

    fn pad_inferred_bar_axes(&self, axes: &mut BTreeMap<Channel, Axis>) -> Result<(), ChartError> {
        let has_authored_domain = |channel| {
            matches!(
                self.encodings.get(&channel),
                Some(Encoding::Field {
                    scale: Some(ScaleSpec {
                        domain: Some(_),
                        ..
                    }),
                    ..
                })
            )
        };
        if !self.axes.contains_key(&Channel::X) && !has_authored_domain(Channel::X) {
            let width = match self.mark.options.get("width") {
                Some(ConstantValue::Number(width)) if width.is_finite() && *width > 0.0 => *width,
                _ => 0.8,
            };
            if let Some(axis) = axes.get(&Channel::X) {
                let (min, max) = axis.domain();
                let margin = width * 0.15;
                let padded = match axis.scale() {
                    Scale::Linear => Some(Axis::linear(
                        min - width * 0.5 - margin,
                        max + width * 0.5 + margin,
                    )?),
                    Scale::Time => Some(Axis::time(
                        min - width * 0.5 - margin,
                        max + width * 0.5 + margin,
                    )?),
                    _ => None,
                };
                if let Some(axis) = padded {
                    axes.insert(Channel::X, axis.crossing(Crossing::Minimum));
                }
            }
        }
        if !self.axes.contains_key(&Channel::Y) && !has_authored_domain(Channel::Y) {
            let baseline = match self.mark.options.get("baseline") {
                Some(ConstantValue::Number(baseline)) if baseline.is_finite() => *baseline,
                _ => 0.0,
            };
            if let Some(axis) = axes.get(&Channel::Y)
                && matches!(axis.scale(), Scale::Linear | Scale::Time)
            {
                let (min, max) = axis.domain();
                let min = min.min(baseline);
                let max = max.max(baseline);
                if min < max {
                    let axis = if matches!(axis.scale(), Scale::Time) {
                        Axis::time(min, max)?
                    } else {
                        Axis::linear(min, max)?
                    };
                    axes.insert(Channel::Y, axis);
                }
            }
        }
        Ok(())
    }

    fn position_value(
        &self,
        row: usize,
        channel: Channel,
        axis: Option<&Axis>,
    ) -> Result<f64, ChartError> {
        let Some(encoding) = self.encodings.get(&channel) else {
            return Ok(0.0);
        };
        let axis = axis.ok_or(ChartError::MissingAxis(channel))?;
        match encoding {
            Encoding::Value(ConstantValue::Number(value)) => {
                axis.normalize(*value).map_err(Into::into)
            }
            Encoding::Field { column, .. } => match self.data.value(row, column)? {
                Some(crate::DataValue::Number(value)) => axis.normalize(value).map_err(Into::into),
                Some(crate::DataValue::Text(value)) => category_position(axis, &value),
                Some(crate::DataValue::Missing) | None => Ok(f64::NAN),
            },
            Encoding::Value(_) => Err(ChartError::NonNumericPosition(channel)),
        }
    }

    fn numeric_value(&self, row: usize, channel: Channel) -> Result<Option<f64>, ChartError> {
        let Some(encoding) = self.encodings.get(&channel) else {
            return Ok(None);
        };
        match encoding {
            Encoding::Value(ConstantValue::Number(value)) => Ok(Some(*value)),
            Encoding::Field { column, .. } => match self.data.value(row, column)? {
                Some(crate::DataValue::Number(value)) => Ok(Some(value)),
                Some(crate::DataValue::Missing) | None => Ok(None),
                Some(crate::DataValue::Text(_)) => Err(ChartError::NonNumericChannel(channel)),
            },
            Encoding::Value(_) => Err(ChartError::NonNumericChannel(channel)),
        }
    }

    fn color_value(&self, row: usize) -> Result<Option<Color>, ChartError> {
        let Some(encoding) = self.encodings.get(&Channel::Color) else {
            return Ok(None);
        };
        match encoding {
            Encoding::Value(ConstantValue::Color(color)) => Ok(Some(*color)),
            Encoding::Value(_) => Err(ChartError::InvalidConstant),
            Encoding::Field { column, scale } => match self.data.value(row, column)? {
                Some(crate::DataValue::Number(value)) => {
                    let axis = infer_axis(&self.data, encoding)?;
                    let t = axis.normalize(value)?.clamp(0.0, 1.0);
                    Ok(Some(interpolate_palette(scale.as_ref(), t)))
                }
                Some(crate::DataValue::Text(value)) => {
                    let categories = categorical_values(&self.data, column, scale.as_ref())?;
                    let index = categories
                        .iter()
                        .position(|candidate| candidate == &value)
                        .ok_or_else(|| ChartError::UnknownCategory(value.clone()))?;
                    let colors = color_palette(scale.as_ref());
                    Ok(Some(colors[index % colors.len()]))
                }
                Some(crate::DataValue::Missing) | None => Ok(None),
            },
        }
    }

    fn label_value(&self, row: usize) -> Result<Option<String>, ChartError> {
        let Some(encoding) = self.encodings.get(&Channel::Label) else {
            return Ok(None);
        };
        Ok(match encoding {
            Encoding::Value(ConstantValue::Text(value)) => Some(value.clone()),
            Encoding::Value(ConstantValue::Number(value)) => Some(value.to_string()),
            Encoding::Value(ConstantValue::Color(_)) => return Err(ChartError::InvalidConstant),
            Encoding::Field { column, .. } => match self.data.value(row, column)? {
                Some(crate::DataValue::Number(value)) => Some(value.to_string()),
                Some(crate::DataValue::Text(value)) => Some(value),
                Some(crate::DataValue::Missing) | None => None,
            },
        })
    }
}

fn categorical_values(
    data: &DataTable,
    column: &str,
    scale: Option<&ScaleSpec>,
) -> Result<Vec<String>, ChartError> {
    if let Some(ScaleSpec {
        kind: ScaleKind::Category,
        categories,
        ..
    }) = scale
        && !categories.is_empty()
    {
        return Ok(categories.clone());
    }
    let Column::Text(values) = data.column(column)? else {
        return Err(ChartError::NonNumericChannel(Channel::Color));
    };
    let mut categories = Vec::new();
    for value in values.iter().flatten() {
        if !categories.contains(value) {
            categories.push(value.clone());
        }
    }
    Ok(categories)
}

fn color_palette(scale: Option<&ScaleSpec>) -> Vec<Color> {
    let configured = scale
        .map(|scale| scale.colors.as_slice())
        .unwrap_or_default();
    if configured.is_empty() {
        vec![
            Color::from_rgb8(0x35, 0x8F, 0xD3),
            Color::from_rgb8(0x63, 0xD1, 0xC5),
            Color::from_rgb8(0xFF, 0xC8, 0x57),
            Color::from_rgb8(0xE8, 0x5D, 0x75),
        ]
    } else {
        configured.to_vec()
    }
}

fn interpolate_palette(scale: Option<&ScaleSpec>, t: f64) -> Color {
    let colors = color_palette(scale);
    if colors.len() == 1 {
        return colors[0];
    }
    let scaled = t * (colors.len() - 1) as f64;
    let index = scaled.floor() as usize;
    let next = (index + 1).min(colors.len() - 1);
    let local = scaled - index as f64;
    let left = colors[index].to_rgba8();
    let right = colors[next].to_rgba8();
    let mix = |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * local) as u8;
    Color::from_rgba8(
        mix(left.r, right.r),
        mix(left.g, right.g),
        mix(left.b, right.b),
        mix(left.a, right.a),
    )
}

fn infer_axis(data: &DataTable, encoding: &Encoding) -> Result<Axis, ChartError> {
    match encoding {
        Encoding::Value(ConstantValue::Number(value)) => {
            let delta = value.abs().max(1.0) * 0.5;
            Ok(Axis::linear(value - delta, value + delta)?)
        }
        Encoding::Field { column, scale } => match data.column(column)? {
            Column::Numeric(values) => {
                let mut min = f64::INFINITY;
                let mut max = f64::NEG_INFINITY;
                for value in values.iter().flatten().filter(|value| value.is_finite()) {
                    min = min.min(*value);
                    max = max.max(*value);
                }
                if !min.is_finite() || !max.is_finite() {
                    return Err(ChartError::EmptyDomain(column.clone()));
                }
                if (max - min).abs() <= f64::EPSILON {
                    let delta = min.abs().max(1.0) * 0.5;
                    min -= delta;
                    max += delta;
                }
                axis_from_scale(scale.as_ref(), min, max)
            }
            Column::Text(values) => {
                let mut categories = Vec::new();
                for value in values.iter().flatten() {
                    if !categories.contains(value) {
                        categories.push(value.clone());
                    }
                }
                if let Some(ScaleSpec {
                    kind: ScaleKind::Category,
                    categories: configured,
                    ..
                }) = scale
                {
                    if !configured.is_empty() {
                        categories = configured.clone();
                    }
                }
                Ok(Axis::category(categories)?)
            }
        },
        Encoding::Value(_) => Err(ChartError::InvalidConstant),
    }
}

fn axis_from_scale(scale: Option<&ScaleSpec>, min: f64, max: f64) -> Result<Axis, ChartError> {
    let domain = scale.and_then(|scale| scale.domain).unwrap_or((min, max));
    Ok(
        match scale.map(|scale| scale.kind).unwrap_or(ScaleKind::Linear) {
            ScaleKind::Linear => Axis::linear(domain.0, domain.1)?,
            ScaleKind::Log { base } => Axis::log(domain.0, domain.1, base)?,
            ScaleKind::SymLog { base, threshold } => {
                Axis::symlog(domain.0, domain.1, base, threshold)?
            }
            ScaleKind::Power { exponent } => Axis::power(domain.0, domain.1, exponent)?,
            ScaleKind::Time => Axis::time(domain.0, domain.1)?,
            ScaleKind::Category => return Err(ChartError::InvalidScale),
        },
    )
}

fn category_position(axis: &Axis, value: &str) -> Result<f64, ChartError> {
    let crate::Scale::Category { values } = axis.scale() else {
        return Err(ChartError::InvalidScale);
    };
    let index = values
        .iter()
        .position(|candidate| candidate == value)
        .ok_or_else(|| ChartError::UnknownCategory(value.to_owned()))?;
    axis.normalize(index as f64).map_err(Into::into)
}

fn native_transition(source: MarkKind, target: MarkKind, source_dim: u8, target_dim: u8) -> bool {
    if source == target {
        return true;
    }
    if matches!(source, MarkKind::Point | MarkKind::Line | MarkKind::Bar)
        && matches!(target, MarkKind::Point | MarkKind::Line | MarkKind::Bar)
    {
        return true;
    }
    matches!(
        (source, target),
        (MarkKind::Heatmap, MarkKind::Surface) | (MarkKind::Surface, MarkKind::Heatmap)
    ) && source_dim != target_dim
}

fn match_by_key(source: &MarkBatch, target: &MarkBatch) -> Result<Vec<DatumMatch>, ChartError> {
    if source.data.iter().any(|datum| datum.key.is_none())
        || target.data.iter().any(|datum| datum.key.is_none())
    {
        return Err(ChartError::KeyRequired);
    }
    let target_by_key: BTreeMap<_, _> = target
        .data
        .iter()
        .enumerate()
        .map(|(index, datum)| (datum.key.clone().expect("checked key"), index))
        .collect();
    let source_by_key: BTreeMap<_, _> = source
        .data
        .iter()
        .enumerate()
        .map(|(index, datum)| (datum.key.clone().expect("checked key"), index))
        .collect();
    let keys: BTreeSet<_> = source_by_key
        .keys()
        .chain(target_by_key.keys())
        .cloned()
        .collect();
    Ok(keys
        .into_iter()
        .map(|key| DatumMatch {
            source: source_by_key.get(&key).copied(),
            target: target_by_key.get(&key).copied(),
        })
        .collect())
}

fn match_by_index(source: &MarkBatch, target: &MarkBatch) -> Vec<DatumMatch> {
    (0..source.data.len().max(target.data.len()))
        .map(|index| DatumMatch {
            source: (index < source.data.len()).then_some(index),
            target: (index < target.data.len()).then_some(index),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DatumKey {
    Number(u64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchDatum {
    pub key: Option<DatumKey>,
    pub source_row: usize,
    pub position: [f64; 3],
    pub size: f64,
    pub opacity: f64,
    pub color: Option<Color>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkBatch {
    pub mark: MarkKind,
    pub dimensions: u8,
    pub data: Vec<BatchDatum>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchPolicy {
    Key,
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionFallback {
    Error,
    Crossfade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    Morph,
    Crossfade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatumMatch {
    pub source: Option<usize>,
    pub target: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarkTransition {
    pub kind: TransitionKind,
    pub source: MarkBatch,
    pub target: MarkBatch,
    pub pairs: Vec<DatumMatch>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ChartError {
    #[error(transparent)]
    Data(#[from] DataError),
    #[error(transparent)]
    Axis(#[from] crate::AxisError),
    #[error("unknown chart channel '{0}'")]
    UnknownChannel(String),
    #[error("unknown chart mark '{0}'")]
    UnknownMark(String),
    #[error("chart scale configuration is invalid")]
    InvalidScale,
    #[error("constant encoding is invalid for its channel")]
    InvalidConstant,
    #[error("axis guides can only be attached to x, y, or z")]
    AxisOnNonPositionalChannel,
    #[error("mark {0:?} is missing a required encoding")]
    MissingRequiredEncoding(MarkKind),
    #[error("key at row {row} is missing")]
    MissingKey { row: usize },
    #[error("key at row {row} duplicates an earlier key")]
    DuplicateKey { row: usize },
    #[error("semantic matching requires a stable key on both chart specs")]
    KeyRequired,
    #[error("channel {0:?} has no coordinate axis")]
    MissingAxis(Channel),
    #[error("channel {0:?} requires numeric data")]
    NonNumericChannel(Channel),
    #[error("positional channel {0:?} requires numeric or categorical data")]
    NonNumericPosition(Channel),
    #[error("numeric column '{0}' has no finite domain")]
    EmptyDomain(String),
    #[error("category '{0}' is outside the configured scale")]
    UnknownCategory(String),
    #[error("transition from {source_mark:?} to {target_mark:?} requires fallback='crossfade'")]
    IncompatibleTransition {
        source_mark: MarkKind,
        target_mark: MarkKind,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(ids: &[&str], z: bool) -> DataTable {
        let mut columns = vec![
            (
                "id".to_owned(),
                Column::Text(ids.iter().map(|value| Some((*value).to_owned())).collect()),
            ),
            (
                "x".to_owned(),
                Column::Numeric((0..ids.len()).map(|value| Some(value as f64)).collect()),
            ),
            (
                "y".to_owned(),
                Column::Numeric(
                    (0..ids.len())
                        .map(|value| Some((value * 2) as f64))
                        .collect(),
                ),
            ),
        ];
        if z {
            columns.push((
                "z".to_owned(),
                Column::Numeric(
                    (0..ids.len())
                        .map(|value| Some((value * 3) as f64))
                        .collect(),
                ),
            ));
        }
        DataTable::new(columns).unwrap()
    }

    fn points(ids: &[&str], z: bool) -> ChartSpec {
        let mut spec = ChartSpec::new(table(ids, z), Some("id".into()))
            .unwrap()
            .mark(MarkKind::Point, BTreeMap::new())
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap();
        if z {
            spec = spec.encode(Channel::Z, Encoding::field("z")).unwrap();
        }
        spec
    }

    #[test]
    fn chart_specs_snapshot_and_normalize_without_renderer_types() {
        let batch = points(&["a", "b", "c"], true).batch().unwrap();
        assert_eq!(batch.dimensions, 3);
        assert_eq!(batch.data.len(), 3);
        assert_eq!(batch.data[0].position, [0.0, 0.0, 0.0]);
        assert_eq!(batch.data[2].position, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn visual_channels_are_resolved_into_the_canonical_batch() {
        let table = DataTable::new([
            ("x".to_owned(), Column::Numeric(vec![Some(0.0), Some(1.0)])),
            ("y".to_owned(), Column::Numeric(vec![Some(1.0), Some(2.0)])),
            (
                "group".to_owned(),
                Column::Text(vec![Some("control".into()), Some("pilot".into())]),
            ),
        ])
        .unwrap();
        let spec = ChartSpec::new(table, None)
            .unwrap()
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap()
            .encode(
                Channel::Color,
                Encoding::scaled_field(
                    "group",
                    ScaleSpec::category(["control".into(), "pilot".into()]).unwrap(),
                ),
            )
            .unwrap()
            .encode(Channel::Size, Encoding::Value(ConstantValue::Number(12.0)))
            .unwrap()
            .encode(
                Channel::Opacity,
                Encoding::Value(ConstantValue::Number(0.4)),
            )
            .unwrap()
            .encode(Channel::Label, Encoding::field("group"))
            .unwrap();

        let batch = spec.batch().unwrap();
        assert_ne!(batch.data[0].color, batch.data[1].color);
        assert_eq!(batch.data[0].size, 12.0);
        assert_eq!(batch.data[0].opacity, 0.4);
        assert_eq!(batch.data[1].label.as_deref(), Some("pilot"));
    }

    #[test]
    fn inferred_bar_axes_leave_space_before_the_first_bar() {
        let data = DataTable::numeric([
            ("x".to_owned(), vec![0.0, 1.0, 2.0]),
            ("y".to_owned(), vec![2.0, 3.0, 1.0]),
        ])
        .unwrap();
        let spec = ChartSpec::new(data, None)
            .unwrap()
            .mark(MarkKind::Bar, BTreeMap::new())
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap();

        let axes = spec.resolved_axes().unwrap();
        let x = &axes[&Channel::X];
        let y = &axes[&Channel::Y];
        let (x_min, x_max) = x.domain();
        assert!(x_min < -0.4, "the first bar edge must not touch the Y axis");
        assert!(x_max > 2.4, "the last bar needs symmetric outer padding");
        assert_eq!(x.crossing_value(), x_min);
        assert_eq!(y.domain(), (0.0, 3.0));
    }

    #[test]
    fn authored_bar_axis_domains_are_not_padded() {
        let data = DataTable::numeric([
            ("x".to_owned(), vec![0.0, 1.0]),
            ("y".to_owned(), vec![1.0, 2.0]),
        ])
        .unwrap();
        let authored = Axis::linear(-1.0, 4.0).unwrap();
        let spec = ChartSpec::new(data, None)
            .unwrap()
            .mark(MarkKind::Bar, BTreeMap::new())
            .encode(Channel::X, Encoding::field("x"))
            .unwrap()
            .encode(Channel::Y, Encoding::field("y"))
            .unwrap()
            .axis(Channel::X, authored.clone())
            .unwrap();

        assert_eq!(spec.resolved_axes().unwrap()[&Channel::X], authored);
    }

    #[test]
    fn duplicate_and_missing_keys_are_rejected_eagerly() {
        let duplicate = ChartSpec::new(table(&["a", "a"], false), Some("id".into()));
        assert!(matches!(
            duplicate,
            Err(ChartError::DuplicateKey { row: 1 })
        ));
        let missing = DataTable::new([
            ("id".to_owned(), Column::Text(vec![Some("a".into()), None])),
            ("x".to_owned(), Column::Numeric(vec![Some(0.0), Some(1.0)])),
        ])
        .unwrap();
        assert!(matches!(
            ChartSpec::new(missing, Some("id".into())),
            Err(ChartError::MissingKey { row: 1 })
        ));
    }

    #[test]
    fn keyed_2d_to_3d_transition_tracks_enters_and_exits() {
        let source = points(&["a", "b"], false);
        let target = points(&["b", "c"], true);
        let transition = source
            .transition_to(&target, MatchPolicy::Key, TransitionFallback::Error)
            .unwrap();
        assert_eq!(transition.kind, TransitionKind::Morph);
        assert_eq!(transition.pairs.len(), 3);
        assert_eq!(
            transition.pairs[0],
            DatumMatch {
                source: Some(0),
                target: None
            }
        );
        assert_eq!(
            transition.pairs[1],
            DatumMatch {
                source: Some(1),
                target: Some(0)
            }
        );
        assert_eq!(
            transition.pairs[2],
            DatumMatch {
                source: None,
                target: Some(1)
            }
        );
    }

    #[test]
    fn incompatible_marks_require_explicit_crossfade() {
        let source = points(&["a"], false);
        let target = points(&["a"], false).mark(MarkKind::Violin, BTreeMap::new());
        assert!(matches!(
            source.transition_to(&target, MatchPolicy::Key, TransitionFallback::Error),
            Err(ChartError::MissingRequiredEncoding(MarkKind::Violin))
                | Err(ChartError::IncompatibleTransition { .. })
        ));
    }
}
