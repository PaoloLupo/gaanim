//! Python bindings for native coordinate spaces, expressions, and data marks.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use gaanim_api::canvas::{
    Canvas as ApiCanvas, ChartHandle, CoordinateRef, CoordinateSpace3DHandle,
    CoordinateSpaceHandle, NumberLineHandle, Parameter as NativeParameter, PolarSpaceHandle,
    DEFAULT_REACTIVE_TEXT_SIZE,
};
use gaanim_expr::{EvalContext, Expr as NativeExpr};
use gaanim_visualization::{
    Axis as NativeAxis, AxisLabelPosition, AxisStylePatch, Channel, ChartSpec as NativeChartSpec,
    Column, ConstantValue, Crossing, DataMarkKind, DataSource as NativeDataSource,
    DataTable as NativeDataTable, Encoding, GuideSpec, MarkKind, MatchPolicy, NonFinitePolicy,
    NumberFormat, Sampling, ScaleSpec as NativeScaleSpec, SpaceLayer, TransitionFallback,
};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::pyclass_init::PyClassInitializer;
use pyo3::types::{PyAny, PyDict};

use crate::color::PyColor;
use crate::pycanvas::{PyPointRef, PyScene};
use crate::pydrawable::{PyCanvasAnim, PyDrawable};

fn value_error(error: impl ToString) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn parse_non_finite_policy(value: &str) -> PyResult<NonFinitePolicy> {
    match value {
        "gap" => Ok(NonFinitePolicy::Gap),
        "drop" => Ok(NonFinitePolicy::Drop),
        "error" => Ok(NonFinitePolicy::Error),
        _ => Err(value_error(
            "policy must be 'gap', 'drop', or 'error' for non-finite samples",
        )),
    }
}

fn trace_readout_source(source: Bound<'_, PyAny>) -> PyResult<NativeExpr> {
    if source.is_callable() {
        let result = source.call0().map_err(|_| {
            PyTypeError::new_err(
                "readout lambda must be a no-argument scalar expression traced with gaanim.math",
            )
        })?;
        extract_expr(result).map_err(|_| {
            PyTypeError::new_err(
                "readout lambda must return a number, Parameter, Variable, or traced scalar",
            )
        })
    } else {
        extract_expr(source)
    }
}

fn trace_scalar_function(
    function: Bound<'_, PyAny>,
    variable: &str,
    owner: &str,
) -> PyResult<NativeExpr> {
    if let Ok(expr) = function.extract::<PyRef<'_, PyExpr>>() {
        return Ok(expr.0.clone());
    }
    if !function.is_callable() {
        return Err(PyTypeError::new_err("function must be callable"));
    }
    // Invoke Python once with a symbolic probe. Sampling and reactive updates
    // remain native after the expression tree has been captured.
    let probe = Py::new(function.py(), PyExpr(NativeExpr::variable(variable)))?;
    let result = function.call1((probe,)).map_err(|_| {
        PyTypeError::new_err(format!(
            "{owner} lambda must return a scalar traced with gaanim.math; Python math/control flow cannot be traced"
        ))
    })?;
    extract_expr(result).map_err(|_| {
        PyTypeError::new_err(format!(
            "{owner} lambda must return a scalar traced with gaanim.math"
        ))
    })
}

fn build_readout_parts(
    canvas: &mut ApiCanvas,
    expression: NativeExpr,
    label: Option<String>,
    format: String,
    prefix: String,
    suffix: String,
    unit: Option<String>,
    font_size: Option<f64>,
    color: Option<PyColor>,
    invalid: String,
) -> (
    gaanim_api::canvas::DrawableHandle,
    Option<PyDrawable>,
    Option<PyDrawable>,
    PyDrawable,
    Option<PyDrawable>,
) {
    let font_size = font_size.unwrap_or(DEFAULT_REACTIVE_TEXT_SIZE);
    let mut number =
        canvas.expression_readout(expression, format, prefix, suffix, invalid, Some(font_size));
    if let Some(color) = color.clone() {
        number = number.fill(color.0);
    }
    let number_part = PyDrawable(number.clone());
    let mut text_part = |value: &str| {
        let mut style = gaanim_text::prelude::TextStyle::default();
        style.size = Some(font_size);
        let spec = gaanim_text::prelude::TextSpec::new(
            vec![value.into()],
            None,
            style,
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("reactive readout text is validated by the public binding");
        let mut handle = canvas.text_spec(spec);
        if let Some(color) = color.clone() {
            handle = handle.fill(color.0);
        }
        PyDrawable(handle)
    };
    let equals_part = label
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|_| text_part("="));
    let label_part = label
        .filter(|value| !value.is_empty())
        .map(|value| text_part(&value));
    let unit_part = unit
        .filter(|value| !value.is_empty())
        .map(|value| text_part(&value));
    let group = canvas.reactive_readout_group(
        label_part.as_ref().map(|part| &part.0),
        equals_part.as_ref().map(|part| &part.0),
        &number_part.0,
        unit_part.as_ref().map(|part| &part.0),
        10.0,
    );
    (group, label_part, equals_part, number_part, unit_part)
}

fn sampling(samples: Option<usize>, tolerance: f64) -> PyResult<Sampling> {
    if let Some(samples) = samples {
        if samples < 2 {
            return Err(value_error("samples must be at least 2"));
        }
        Ok(Sampling::Fixed { samples })
    } else if tolerance.is_finite() && tolerance > 0.0 {
        Ok(Sampling::Adaptive {
            min_samples: 32,
            max_depth: 8,
            tolerance,
        })
    } else {
        Err(value_error("tolerance must be finite and positive"))
    }
}

/// Immutable reusable axis builder.
#[pyclass(name = "Axis", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyAxis(pub NativeAxis);

#[pymethods]
impl PyAxis {
    #[staticmethod]
    fn linear(minimum: f64, maximum: f64) -> PyResult<Self> {
        NativeAxis::linear(minimum, maximum)
            .map(Self)
            .map_err(value_error)
    }

    #[staticmethod]
    #[pyo3(signature = (minimum, maximum, base=10.0))]
    fn log(minimum: f64, maximum: f64, base: f64) -> PyResult<Self> {
        NativeAxis::log(minimum, maximum, base)
            .map(Self)
            .map_err(value_error)
    }

    #[staticmethod]
    #[pyo3(signature = (minimum, maximum, *, base=10.0, threshold=1.0))]
    fn symlog(minimum: f64, maximum: f64, base: f64, threshold: f64) -> PyResult<Self> {
        NativeAxis::symlog(minimum, maximum, base, threshold)
            .map(Self)
            .map_err(value_error)
    }

    #[staticmethod]
    fn power(minimum: f64, maximum: f64, exponent: f64) -> PyResult<Self> {
        NativeAxis::power(minimum, maximum, exponent)
            .map(Self)
            .map_err(value_error)
    }

    #[staticmethod]
    fn time(minimum_timestamp: f64, maximum_timestamp: f64) -> PyResult<Self> {
        NativeAxis::time(minimum_timestamp, maximum_timestamp)
            .map(Self)
            .map_err(value_error)
    }

    #[staticmethod]
    fn category(values: Vec<String>) -> PyResult<Self> {
        NativeAxis::category(values).map(Self).map_err(value_error)
    }

    fn ticks(&self, step: f64) -> PyResult<Self> {
        self.0.clone().ticks(step).map(Self).map_err(value_error)
    }

    fn auto_ticks(&self) -> Self {
        Self(self.0.clone().auto_ticks())
    }

    fn minor_ticks(&self, subdivisions: usize) -> Self {
        Self(self.0.clone().minor_ticks(subdivisions))
    }

    #[pyo3(signature = (format="auto", precision=2, denominator=4, pattern=None))]
    fn numbers(
        &self,
        format: &str,
        precision: usize,
        denominator: u32,
        pattern: Option<String>,
    ) -> PyResult<Self> {
        let format = match format {
            "auto" => NumberFormat::Auto,
            "fixed" => NumberFormat::Fixed(precision),
            "scientific" => NumberFormat::Scientific(precision),
            "percent" => NumberFormat::Percent(precision),
            "fraction" if denominator > 0 => NumberFormat::Fraction { denominator },
            "pi" if denominator > 0 => NumberFormat::Pi { denominator },
            "datetime" => NumberFormat::DateTime {
                pattern: pattern.unwrap_or_else(|| "%Y-%m-%d".to_owned()),
            },
            "fraction" | "pi" => return Err(value_error("denominator must be positive")),
            _ => {
                return Err(value_error(
                    "format must be auto, fixed, scientific, percent, fraction, pi, or datetime",
                ));
            }
        };
        Ok(Self(self.0.clone().numbers(format)))
    }

    #[pyo3(signature = (text, *, position="center"))]
    fn label(&self, text: String, position: &str) -> PyResult<Self> {
        let position = match position {
            "start" | "bottom" => AxisLabelPosition::Start,
            "center" | "middle" => AxisLabelPosition::Center,
            "end" | "top" => AxisLabelPosition::End,
            _ => {
                return Err(value_error(
                    "label position must be start, center, end, top, or bottom",
                ));
            }
        };
        Ok(Self(self.0.clone().label(text).label_position(position)))
    }

    fn crossing(&self, value: Bound<'_, PyAny>) -> PyResult<Self> {
        let crossing = if let Ok(number) = value.extract::<f64>() {
            Crossing::Value(number)
        } else {
            match value.extract::<&str>()? {
                "auto" => Crossing::Auto,
                "zero" => Crossing::Zero,
                "minimum" | "min" => Crossing::Minimum,
                "maximum" | "max" => Crossing::Maximum,
                _ => {
                    return Err(value_error(
                        "crossing must be auto, zero, min, max, or a number",
                    ))
                }
            }
        };
        Ok(Self(self.0.clone().crossing(crossing)))
    }

    #[pyo3(signature = (*, color=None, width=None, tick_length=None, tick_width=None, tick_color=None, number_color=None, label_color=None))]
    fn style(
        &self,
        color: Option<PyColor>,
        width: Option<f64>,
        tick_length: Option<f64>,
        tick_width: Option<f64>,
        tick_color: Option<PyColor>,
        number_color: Option<PyColor>,
        label_color: Option<PyColor>,
    ) -> PyResult<Self> {
        let color = color.map(|value| value.0);
        if let Some(value) = width {
            if !value.is_finite() || value < 0.0 {
                return Err(value_error("width must be finite and non-negative"));
            }
        }
        if let Some(value) = tick_length {
            if !value.is_finite() || value < 0.0 {
                return Err(value_error("tick_length must be finite and non-negative"));
            }
        }
        if let Some(value) = tick_width {
            if !value.is_finite() || value < 0.0 {
                return Err(value_error("tick_width must be finite and non-negative"));
            }
        }
        Ok(Self(self.0.clone().style_patch(AxisStylePatch {
            color,
            tick_color: tick_color.map(|value| value.0).or(color),
            width,
            tick_length,
            tick_width,
            number_color: number_color.map(|value| value.0),
            label_color: label_color.map(|value| value.0),
        })))
    }

    #[getter]
    fn domain(&self) -> (f64, f64) {
        self.0.domain()
    }
}

/// Immutable scale specification for non-positional and inferred channels.
#[pyclass(name = "Scale", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyScale(pub NativeScaleSpec);

#[pymethods]
impl PyScale {
    #[staticmethod]
    #[pyo3(signature = (domain=None, *, clamp=false))]
    fn linear(domain: Option<(f64, f64)>, clamp: bool) -> PyResult<Self> {
        NativeScaleSpec::linear(domain)
            .map(|scale| Self(scale.clamp(clamp)))
            .map_err(value_error)
    }

    #[staticmethod]
    #[pyo3(signature = (domain=None, *, base=10.0, clamp=false))]
    fn log(domain: Option<(f64, f64)>, base: f64, clamp: bool) -> PyResult<Self> {
        NativeScaleSpec::log(domain, base)
            .map(|scale| Self(scale.clamp(clamp)))
            .map_err(value_error)
    }

    #[staticmethod]
    #[pyo3(signature = (domain=None, *, base=10.0, threshold=1.0, clamp=false))]
    fn symlog(
        domain: Option<(f64, f64)>,
        base: f64,
        threshold: f64,
        clamp: bool,
    ) -> PyResult<Self> {
        NativeScaleSpec::symlog(domain, base, threshold)
            .map(|scale| Self(scale.clamp(clamp)))
            .map_err(value_error)
    }

    #[staticmethod]
    #[pyo3(signature = (domain=None, *, exponent=1.0, clamp=false))]
    fn power(domain: Option<(f64, f64)>, exponent: f64, clamp: bool) -> PyResult<Self> {
        NativeScaleSpec::power(domain, exponent)
            .map(|scale| Self(scale.clamp(clamp)))
            .map_err(value_error)
    }

    #[staticmethod]
    #[pyo3(signature = (domain=None, *, clamp=false))]
    fn time(domain: Option<(f64, f64)>, clamp: bool) -> PyResult<Self> {
        NativeScaleSpec::time(domain)
            .map(|scale| Self(scale.clamp(clamp)))
            .map_err(value_error)
    }

    #[staticmethod]
    #[pyo3(signature = (values=None))]
    fn category(values: Option<Vec<String>>) -> PyResult<Self> {
        NativeScaleSpec::category(values.unwrap_or_default())
            .map(Self)
            .map_err(value_error)
    }

    fn colors(&self, colors: Vec<PyColor>) -> Self {
        Self(
            self.0
                .clone()
                .colors(colors.into_iter().map(|color| color.0)),
        )
    }
}

/// A column-backed chart encoding.
#[pyclass(name = "Field", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyField(pub Encoding);

#[pymethods]
impl PyField {
    #[new]
    #[pyo3(signature = (column, *, scale=None))]
    fn new(column: String, scale: Option<&PyScale>) -> PyResult<Self> {
        if column.trim().is_empty() {
            return Err(value_error("field column must be non-empty"));
        }
        Ok(Self(match scale {
            Some(scale) => Encoding::scaled_field(column, scale.0.clone()),
            None => Encoding::field(column),
        }))
    }
}

fn constant_from_python(value: &Bound<'_, PyAny>) -> PyResult<ConstantValue> {
    if let Ok(color) = value.extract::<PyRef<'_, PyColor>>() {
        Ok(ConstantValue::Color(color.0))
    } else if let Ok(number) = value.extract::<f64>() {
        if number.is_finite() {
            Ok(ConstantValue::Number(number))
        } else {
            Err(value_error("chart constants must be finite"))
        }
    } else if let Ok(text) = value.extract::<String>() {
        Ok(ConstantValue::Text(text))
    } else {
        Err(PyTypeError::new_err(
            "chart constants must be float, str, or Color",
        ))
    }
}

/// A constant chart encoding.
#[pyclass(name = "Value", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyValue(pub ConstantValue);

#[pymethods]
impl PyValue {
    #[new]
    fn new(value: Bound<'_, PyAny>) -> PyResult<Self> {
        constant_from_python(&value).map(Self)
    }
}

/// Legend or continuous colorbar configuration.
#[pyclass(name = "Guide", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyGuide(pub GuideSpec);

#[pymethods]
impl PyGuide {
    #[staticmethod]
    #[pyo3(signature = (*, title=None))]
    fn legend(title: Option<String>) -> Self {
        Self(GuideSpec::Legend { title })
    }

    #[staticmethod]
    #[pyo3(signature = (*, title=None))]
    fn colorbar(title: Option<String>) -> Self {
        Self(GuideSpec::ColorBar { title })
    }

    #[staticmethod]
    fn disabled() -> Self {
        Self(GuideSpec::None)
    }
}

fn encoding_from_python(value: &Bound<'_, PyAny>) -> PyResult<Encoding> {
    if let Ok(field) = value.extract::<PyRef<'_, PyField>>() {
        Ok(field.0.clone())
    } else if let Ok(value) = value.extract::<PyRef<'_, PyValue>>() {
        Ok(Encoding::Value(value.0.clone()))
    } else if let Ok(column) = value.extract::<String>() {
        Ok(Encoding::field(column))
    } else {
        Err(PyTypeError::new_err(
            "encoding must be a column name, Field, or Value",
        ))
    }
}

/// Immutable declarative chart description.
#[pyclass(name = "ChartSpec", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyChartSpec(pub NativeChartSpec);

#[pymethods]
impl PyChartSpec {
    #[new]
    #[pyo3(signature = (data, *, key=None))]
    fn new(data: Bound<'_, PyAny>, key: Option<String>) -> PyResult<Self> {
        let table = if let Ok(source) = data.extract::<PyRef<'_, PyDataSource>>() {
            source.inner.snapshot()
        } else if let Ok(table) = data.extract::<PyRef<'_, PyDataTable>>() {
            table.0.clone()
        } else {
            table_from_python(&data)?
        };
        NativeChartSpec::new(table, key)
            .map(Self)
            .map_err(value_error)
    }

    #[pyo3(signature = (kind, **options))]
    fn mark(&self, kind: &str, options: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let kind = MarkKind::parse(kind).map_err(value_error)?;
        let mut native = BTreeMap::new();
        if let Some(options) = options {
            for (name, value) in options.iter() {
                native.insert(name.extract::<String>()?, constant_from_python(&value)?);
            }
        }
        Ok(Self(self.0.clone().mark(kind, native)))
    }

    #[pyo3(signature = (*, x=None, y=None, z=None, color=None, size=None, opacity=None, label=None))]
    #[allow(clippy::too_many_arguments)]
    fn encode(
        &self,
        x: Option<Bound<'_, PyAny>>,
        y: Option<Bound<'_, PyAny>>,
        z: Option<Bound<'_, PyAny>>,
        color: Option<Bound<'_, PyAny>>,
        size: Option<Bound<'_, PyAny>>,
        opacity: Option<Bound<'_, PyAny>>,
        label: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut spec = self.0.clone();
        for (channel, value) in [
            (Channel::X, x),
            (Channel::Y, y),
            (Channel::Z, z),
            (Channel::Color, color),
            (Channel::Size, size),
            (Channel::Opacity, opacity),
            (Channel::Label, label),
        ] {
            if let Some(value) = value {
                spec = spec
                    .encode(channel, encoding_from_python(&value)?)
                    .map_err(value_error)?;
            }
        }
        Ok(Self(spec))
    }

    #[pyo3(signature = (*, x=None, y=None, z=None))]
    fn axes(&self, x: Option<&PyAxis>, y: Option<&PyAxis>, z: Option<&PyAxis>) -> PyResult<Self> {
        let mut spec = self.0.clone();
        for (channel, axis) in [(Channel::X, x), (Channel::Y, y), (Channel::Z, z)] {
            if let Some(axis) = axis {
                spec = spec.axis(channel, axis.0.clone()).map_err(value_error)?;
            }
        }
        Ok(Self(spec))
    }

    #[pyo3(signature = (*, color=None, size=None, opacity=None))]
    fn guides(
        &self,
        color: Option<&PyGuide>,
        size: Option<&PyGuide>,
        opacity: Option<&PyGuide>,
    ) -> Self {
        let mut spec = self.0.clone();
        for (channel, guide) in [
            (Channel::Color, color),
            (Channel::Size, size),
            (Channel::Opacity, opacity),
        ] {
            if let Some(guide) = guide {
                spec = spec.guide(channel, guide.0.clone());
            }
        }
        Self(spec)
    }

    fn validate(&self) -> PyResult<()> {
        self.0.validate().map_err(value_error)
    }

    #[getter]
    fn key(&self) -> Option<String> {
        self.0.key().map(str::to_owned)
    }

    fn __len__(&self) -> usize {
        self.0.data().len()
    }
}

/// Private native expression tree used while tracing public Python lambdas.
///
/// This is intentionally registered as `_Expr`: applications construct
/// expressions by using `Parameter`/`Variable` values and `gaanim.math`, not
/// by depending on the AST implementation.
#[pyclass(name = "_Expr", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyExpr(pub NativeExpr);

pub(crate) fn extract_expr(value: Bound<'_, PyAny>) -> PyResult<NativeExpr> {
    if let Ok(expr) = value.extract::<PyRef<'_, PyExpr>>() {
        Ok(expr.0.clone())
    } else if let Ok(parameter) = value.extract::<PyRef<'_, PyParameter>>() {
        Ok(parameter.inner.expression())
    } else if let Ok(variable) = value.extract::<PyRef<'_, PyVariable>>() {
        Ok(variable.parameter.inner.expression())
    } else if let Ok(number) = value.extract::<f64>() {
        Ok(NativeExpr::constant(number))
    } else {
        Err(PyTypeError::new_err(
            "expected a traced scalar (Parameter, Variable, or number)",
        ))
    }
}

#[pymethods]
impl PyExpr {
    #[new]
    fn new(value: f64) -> Self {
        Self(NativeExpr::constant(value))
    }

    #[staticmethod]
    fn var(name: String) -> PyResult<Self> {
        if name.trim().is_empty() {
            return Err(value_error("variable name cannot be empty"));
        }
        Ok(Self(NativeExpr::variable(name)))
    }

    #[staticmethod]
    fn constant(value: f64) -> PyResult<Self> {
        if !value.is_finite() {
            return Err(value_error("constant must be finite"));
        }
        Ok(Self(NativeExpr::constant(value)))
    }

    fn derivative(&self, variable: &str) -> Self {
        Self(self.0.derivative(variable))
    }

    fn sin(&self) -> Self {
        Self(self.0.clone().sin())
    }

    fn cos(&self) -> Self {
        Self(self.0.clone().cos())
    }

    fn tan(&self) -> Self {
        Self(self.0.clone().tan())
    }

    fn exp(&self) -> Self {
        Self(self.0.clone().exp())
    }

    fn log(&self) -> Self {
        Self(self.0.clone().ln())
    }

    fn sqrt(&self) -> Self {
        Self(self.0.clone().sqrt())
    }

    fn abs(&self) -> Self {
        Self(self.0.clone().abs())
    }

    fn pow(&self, exponent: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(self.0.clone().pow(extract_expr(exponent)?)))
    }

    fn min(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(self.0.clone().min(extract_expr(other)?)))
    }

    fn max(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(self.0.clone().max(extract_expr(other)?)))
    }

    fn clamp(&self, minimum: Bound<'_, PyAny>, maximum: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(
            self.0
                .clone()
                .clamp(extract_expr(minimum)?, extract_expr(maximum)?),
        ))
    }

    fn if_positive(
        &self,
        when_true: Bound<'_, PyAny>,
        when_false: Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Ok(Self(self.0.clone().if_positive(
            extract_expr(when_true)?,
            extract_expr(when_false)?,
        )))
    }

    #[pyo3(signature = (**variables))]
    fn eval(&self, variables: Option<&Bound<'_, PyDict>>) -> PyResult<f64> {
        let mut context = EvalContext::new();
        if let Some(variables) = variables {
            for (name, value) in variables.iter() {
                context.set_variable(name.extract::<String>()?, value.extract::<f64>()?);
            }
        }
        self.0.eval(&context).map_err(value_error)
    }

    fn __neg__(&self) -> Self {
        Self(-self.0.clone())
    }

    fn __add__(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(self.0.clone() + extract_expr(other)?))
    }

    fn __radd__(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(extract_expr(other)? + self.0.clone()))
    }

    fn __sub__(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(self.0.clone() - extract_expr(other)?))
    }

    fn __rsub__(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(extract_expr(other)? - self.0.clone()))
    }

    fn __mul__(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(self.0.clone() * extract_expr(other)?))
    }

    fn __rmul__(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(extract_expr(other)? * self.0.clone()))
    }

    fn __truediv__(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(self.0.clone() / extract_expr(other)?))
    }

    fn __rtruediv__(&self, other: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(extract_expr(other)? / self.0.clone()))
    }

    fn __pow__(
        &self,
        other: Bound<'_, PyAny>,
        _modulo: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        self.pow(other)
    }

    fn __rpow__(
        &self,
        other: Bound<'_, PyAny>,
        _modulo: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self(extract_expr(other)?.pow(self.0.clone())))
    }

    fn __abs__(&self) -> Self {
        Self(self.0.clone().abs())
    }
}

/// Animatable scalar referenced from an [`Expr`].
#[pyclass(name = "Parameter", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyParameter {
    pub(crate) inner: NativeParameter,
}

#[pymethods]
impl PyParameter {
    #[getter]
    fn current(&self) -> f64 {
        self.inner.current()
    }

    fn set(&self, value: f64) -> PyResult<()> {
        self.inner.set(value).map_err(value_error)
    }

    #[pyo3(signature = (value, duration=None))]
    fn animate_to(&self, value: f64, duration: Option<f64>) -> PyResult<PyCanvasAnim> {
        let inner = self.inner.animate_to(value).map_err(value_error)?;
        Ok(PyCanvasAnim {
            inner: duration.map_or(inner.clone(), |seconds| inner.duration(seconds)),
        })
    }

    /// Drive this scalar directly from a Python callback.
    #[pyo3(signature = (callback, *, reset=None, fixed_dt=None))]
    fn add_updater_fn(
        &self,
        callback: Py<PyAny>,
        reset: Option<Py<PyAny>>,
        fixed_dt: Option<f64>,
    ) -> PyResult<Self> {
        if !Python::attach(|py| callback.bind(py).is_callable()) {
            return Err(PyValueError::new_err("callback must be callable"));
        }
        if let Some(reset) = reset.as_ref() {
            if !Python::attach(|py| reset.bind(py).is_callable()) {
                return Err(PyValueError::new_err("reset must be callable"));
            }
        }
        match (reset.is_some(), fixed_dt.is_some()) {
            (true, false) => {
                return Err(PyValueError::new_err(
                    "reset requires fixed_dt for deterministic replay",
                ));
            }
            (false, true) => {
                return Err(PyValueError::new_err(
                    "fixed_dt requires reset so seeks and exports can rebuild simulation state",
                ));
            }
            _ => {}
        }

        let callback_clone = callback.clone();
        let parameter = self.inner.clone();
        let updater_fn = move |dt: f64,
                               elapsed: f64,
                               entity: gaanim_scene::prelude::Entity,
                               world: &mut gaanim_scene::prelude::World| {
            let current = world
                .get::<gaanim_animation::FloatSignal>(entity)
                .map(|signal| signal.value)
                .unwrap_or(0.0);
            let result = Python::attach(|py| {
                callback_clone
                    .bind(py)
                    .call1((current, dt, elapsed))?
                    .extract::<f64>()
            });
            match result {
                Ok(value) if value.is_finite() => {
                    parameter.set_runtime_current(value);
                    if let Some(mut signal) = world.get_mut::<gaanim_animation::FloatSignal>(entity)
                    {
                        signal.value = value;
                    }
                    true
                }
                Ok(_) => {
                    Python::attach(|py| {
                        PyValueError::new_err("callback must return a finite scalar").print(py)
                    });
                    false
                }
                Err(error) => {
                    Python::attach(|py| error.print(py));
                    false
                }
            }
        };

        let updater = if let (Some(reset), Some(fixed_dt)) = (reset, fixed_dt) {
            let reset_clone = reset.clone();
            let reset_fn =
                move |_entity: gaanim_scene::prelude::Entity,
                      _world: &mut gaanim_scene::prelude::World| {
                    match Python::attach(|py| reset_clone.bind(py).call0().map(|_| ())) {
                        Ok(()) => true,
                        Err(error) => {
                            Python::attach(|py| error.print(py));
                            false
                        }
                    }
                };
            gaanim_animation::Updater::new_simulation(updater_fn, reset_fn, fixed_dt).map_err(
                |_| PyValueError::new_err("fixed_dt must be finite and greater than zero"),
            )?
        } else {
            gaanim_animation::Updater::new(updater_fn)
        };
        self.inner.drawable().add_custom_updater(updater);
        Ok(self.clone())
    }

    fn remove_updater(&self) {
        self.inner.drawable().remove_updater();
    }

    /// Drive this parameter's value along a sampled `(times, values)` series,
    /// evaluated natively as a pure function of timeline time — no per-frame
    /// Python callbacks, exact under seeks and paused scrubbing.
    ///
    /// The value becomes `offset + scale * sample` (absolute, clamped outside
    /// the series), so traced expressions, readouts, and reactive plots that
    /// reference this parameter follow the series for free.
    #[pyo3(signature = (times, values, *, interpolation = "linear", scale = 1.0, offset = 0.0))]
    fn drive_from_samples(
        &self,
        times: Vec<f64>,
        values: Vec<f64>,
        interpolation: &str,
        scale: f64,
        offset: f64,
    ) -> PyResult<Self> {
        let interpolation = crate::pydrawable::parse_sampled_interpolation(interpolation)?;
        self.inner
            .drawable()
            .drive_from_samples(
                times,
                values,
                gaanim_animation::SampledProperty::Signal,
                interpolation,
                scale,
                offset,
            )
            .map(|_| self.clone())
            .map_err(|_| {
                PyValueError::new_err(
                    "drive_from_samples requires non-empty matching times/values, finite values, \
                     and non-decreasing times",
                )
            })
    }

    fn __neg__(&self) -> PyExpr {
        PyExpr(-self.inner.expression())
    }

    fn __add__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(self.inner.expression() + extract_expr(other)?))
    }

    fn __radd__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(extract_expr(other)? + self.inner.expression()))
    }

    fn __sub__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(self.inner.expression() - extract_expr(other)?))
    }

    fn __rsub__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(extract_expr(other)? - self.inner.expression()))
    }

    fn __mul__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(self.inner.expression() * extract_expr(other)?))
    }

    fn __rmul__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(extract_expr(other)? * self.inner.expression()))
    }

    fn __truediv__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(self.inner.expression() / extract_expr(other)?))
    }

    fn __rtruediv__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(extract_expr(other)? / self.inner.expression()))
    }

    fn __pow__(
        &self,
        other: Bound<'_, PyAny>,
        _modulo: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyExpr> {
        Ok(PyExpr(self.inner.expression().pow(extract_expr(other)?)))
    }

    fn __rpow__(
        &self,
        other: Bound<'_, PyAny>,
        _modulo: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyExpr> {
        Ok(PyExpr(extract_expr(other)?.pow(self.inner.expression())))
    }

    fn __abs__(&self) -> PyExpr {
        PyExpr(self.inner.expression().abs())
    }
}

/// A visible parameter.  Its drawable base owns the stable readout group;
/// scalar operations still produce private traced expressions.
#[pyclass(name = "Variable", module = "gaanim_core", extends = PyDrawable, from_py_object)]
#[derive(Clone)]
pub struct PyVariable {
    pub(crate) parameter: PyParameter,
    label_part: Option<PyDrawable>,
    equals_part: Option<PyDrawable>,
    number_part: PyDrawable,
    unit_part: Option<PyDrawable>,
}

impl PyVariable {
    fn initializer(
        drawable: gaanim_api::canvas::DrawableHandle,
        parameter: PyParameter,
        label_part: Option<PyDrawable>,
        equals_part: Option<PyDrawable>,
        number_part: PyDrawable,
        unit_part: Option<PyDrawable>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyDrawable(drawable)).add_subclass(Self {
            parameter,
            label_part,
            equals_part,
            number_part,
            unit_part,
        })
    }
}

#[pymethods]
impl PyVariable {
    #[getter]
    fn current(&self) -> f64 {
        self.parameter.inner.current()
    }

    fn set(&self, value: f64) -> PyResult<()> {
        self.parameter.inner.set(value).map_err(value_error)
    }

    #[pyo3(signature = (value, duration=None))]
    fn animate_to(&self, value: f64, duration: Option<f64>) -> PyResult<PyCanvasAnim> {
        let inner = self
            .parameter
            .inner
            .animate_to(value)
            .map_err(value_error)?;
        Ok(PyCanvasAnim {
            inner: duration.map_or(inner.clone(), |seconds| inner.duration(seconds)),
        })
    }

    #[pyo3(signature = (callback, *, reset=None, fixed_dt=None))]
    fn add_updater_fn(
        &self,
        callback: Py<PyAny>,
        reset: Option<Py<PyAny>>,
        fixed_dt: Option<f64>,
    ) -> PyResult<()> {
        self.parameter.add_updater_fn(callback, reset, fixed_dt)?;
        Ok(())
    }

    fn remove_updater(&self) {
        self.parameter.remove_updater();
    }

    #[getter]
    fn label(&self) -> Option<PyDrawable> {
        self.label_part.clone()
    }
    #[getter]
    fn equals(&self) -> Option<PyDrawable> {
        self.equals_part.clone()
    }
    #[getter]
    fn number(&self) -> PyDrawable {
        self.number_part.clone()
    }
    #[getter]
    fn unit(&self) -> Option<PyDrawable> {
        self.unit_part.clone()
    }

    fn __neg__(&self) -> PyExpr {
        PyExpr(-self.parameter.inner.expression())
    }
    fn __add__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(
            self.parameter.inner.expression() + extract_expr(other)?,
        ))
    }
    fn __radd__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(
            extract_expr(other)? + self.parameter.inner.expression(),
        ))
    }
    fn __sub__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(
            self.parameter.inner.expression() - extract_expr(other)?,
        ))
    }
    fn __rsub__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(
            extract_expr(other)? - self.parameter.inner.expression(),
        ))
    }
    fn __mul__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(
            self.parameter.inner.expression() * extract_expr(other)?,
        ))
    }
    fn __rmul__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(
            extract_expr(other)? * self.parameter.inner.expression(),
        ))
    }
    fn __truediv__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(
            self.parameter.inner.expression() / extract_expr(other)?,
        ))
    }
    fn __rtruediv__(&self, other: Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr(
            extract_expr(other)? / self.parameter.inner.expression(),
        ))
    }
    fn __pow__(
        &self,
        other: Bound<'_, PyAny>,
        _modulo: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyExpr> {
        Ok(PyExpr(
            self.parameter.inner.expression().pow(extract_expr(other)?),
        ))
    }
    fn __rpow__(
        &self,
        other: Bound<'_, PyAny>,
        _modulo: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyExpr> {
        Ok(PyExpr(
            extract_expr(other)?.pow(self.parameter.inner.expression()),
        ))
    }
    fn __abs__(&self) -> PyExpr {
        PyExpr(self.parameter.inner.expression().abs())
    }
}

/// A stable group containing the visible parts of a reactive value.
#[pyclass(name = "Readout", module = "gaanim_core", extends = PyDrawable, from_py_object)]
#[derive(Clone)]
pub struct PyReadout {
    label_part: Option<PyDrawable>,
    equals_part: Option<PyDrawable>,
    number_part: PyDrawable,
    unit_part: Option<PyDrawable>,
}

impl PyReadout {
    fn initializer(
        drawable: gaanim_api::canvas::DrawableHandle,
        label_part: Option<PyDrawable>,
        equals_part: Option<PyDrawable>,
        number_part: PyDrawable,
        unit_part: Option<PyDrawable>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyDrawable(drawable)).add_subclass(Self {
            label_part,
            equals_part,
            number_part,
            unit_part,
        })
    }
}

#[pymethods]
impl PyReadout {
    #[getter]
    fn label(&self) -> Option<PyDrawable> {
        self.label_part.clone()
    }
    #[getter]
    fn equals(&self) -> Option<PyDrawable> {
        self.equals_part.clone()
    }
    #[getter]
    fn number(&self) -> PyDrawable {
        self.number_part.clone()
    }
    #[getter]
    fn unit(&self) -> Option<PyDrawable> {
        self.unit_part.clone()
    }
}

#[pyclass(name = "CoordinateRef", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyCoordinateRef(pub CoordinateRef);

#[pymethods]
impl PyCoordinateRef {
    fn place(&self, drawable: &PyDrawable) -> PyDrawable {
        PyDrawable(drawable.0.clone().at_coordinate(self.0))
    }
}

/// A typed 2D space that owns independently animatable visual layers.
#[pyclass(name = "CoordinateSpace", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyCoordinateSpace {
    pub(crate) inner: CoordinateSpaceHandle,
    canvas: Arc<Mutex<ApiCanvas>>,
}

/// A typed 3D coordinate space with native surface and curve sampling.
#[pyclass(
    name = "CoordinateSpace3D",
    module = "gaanim_core",
    skip_from_py_object
)]
#[derive(Clone)]
pub struct PyCoordinateSpace3D {
    inner: CoordinateSpace3DHandle,
    canvas: Arc<Mutex<ApiCanvas>>,
}

/// Materialized declarative chart with stable semantic layers.
#[pyclass(name = "Chart", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyChart {
    inner: ChartHandle,
    canvas: Arc<Mutex<ApiCanvas>>,
    inspection_fields: Vec<String>,
    inspection_format: Option<String>,
}

impl PyChart {
    fn new(inner: ChartHandle, canvas: Arc<Mutex<ApiCanvas>>) -> Self {
        Self {
            inner,
            canvas,
            inspection_fields: Vec::new(),
            inspection_format: None,
        }
    }
}

#[pymethods]
impl PyChart {
    fn drawable(&self) -> PyDrawable {
        PyDrawable(self.inner.drawable().clone())
    }

    fn layer(&self, name: &str) -> PyResult<PyDrawable> {
        self.inner
            .layer(name)
            .cloned()
            .map(PyDrawable)
            .ok_or_else(|| value_error("layer must be marks, axes, grid, guides, or labels"))
    }

    fn at(&self, x: f64, y: f64) -> Self {
        let mut result = self.clone();
        result.inner = result.inner.clone().at(x, y);
        result
    }

    fn at_3d(&self, x: f64, y: f64, z: f64) -> Self {
        let mut result = self.clone();
        result.inner = result.inner.clone().at_3d(x, y, z);
        result
    }

    fn scaled(&self, factor: f64) -> Self {
        let mut result = self.clone();
        result.inner = result.inner.clone().scaled(factor);
        result
    }

    #[pyo3(signature = (duration=None))]
    fn create(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().create(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn write(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().write(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn fade_in(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().fade_in(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn fade_out(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().fade_out(duration),
        }
    }

    #[pyo3(name = "to", signature = (target, *, match_="key", fallback="error"))]
    fn transition_to(
        &self,
        target: &PyChartSpec,
        match_: &str,
        fallback: &str,
    ) -> PyResult<PyCanvasAnim> {
        let matching = match match_ {
            "key" => MatchPolicy::Key,
            "index" => MatchPolicy::Index,
            _ => return Err(value_error("match_ must be 'key' or 'index'")),
        };
        let fallback = match fallback {
            "error" => TransitionFallback::Error,
            "crossfade" => TransitionFallback::Crossfade,
            _ => return Err(value_error("fallback must be 'error' or 'crossfade'")),
        };
        let target = self
            .canvas
            .lock()
            .expect("scene canvas poisoned")
            .chart(target.0.clone())
            .map_err(value_error)?;
        self.inner
            .transition_to(&target, matching, fallback)
            .map(|inner| PyCanvasAnim { inner })
            .map_err(value_error)
    }

    #[pyo3(signature = (fields, *, format=None))]
    fn inspect(&self, fields: Vec<String>, format: Option<String>) -> PyResult<Self> {
        for field in &fields {
            self.inner
                .spec()
                .data()
                .column(field)
                .map_err(value_error)?;
        }
        let mut result = self.clone();
        result.inspection_fields = fields;
        result.inspection_format = format;
        Ok(result)
    }

    #[getter]
    fn inspection_enabled(&self) -> bool {
        !self.inspection_fields.is_empty()
    }
}

#[pyclass(name = "NumberLine", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyNumberLine {
    inner: NumberLineHandle,
    canvas: Arc<Mutex<ApiCanvas>>,
}

#[pymethods]
impl PyNumberLine {
    fn drawable(&self) -> PyDrawable {
        PyDrawable(self.inner.drawable().clone())
    }

    fn coord(&self, value: f64) -> PyResult<PyCoordinateRef> {
        self.inner
            .coord(value)
            .map(PyCoordinateRef)
            .map_err(value_error)
    }

    fn data_to_local(&self, value: f64) -> PyResult<f64> {
        self.inner.data_to_local(value).map_err(value_error)
    }

    #[pyo3(signature = (value, *, normal_offset=None))]
    fn point_ref(
        &self,
        value: Bound<'_, PyAny>,
        normal_offset: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PyPointRef> {
        let value = extract_expr(value)?;
        let normal_offset = normal_offset
            .map(extract_expr)
            .transpose()?
            .unwrap_or_else(|| NativeExpr::constant(0.0));
        self.inner
            .point_ref(value, normal_offset)
            .map(PyPointRef)
            .map_err(value_error)
    }

    #[pyo3(signature = (function, domain=None, *, normal_scale=120.0, reveal=None, samples=None, tolerance=0.75))]
    fn function(
        &self,
        function: Bound<'_, PyAny>,
        domain: Option<(f64, f64)>,
        normal_scale: f64,
        reveal: Option<Bound<'_, PyAny>>,
        samples: Option<usize>,
        tolerance: f64,
    ) -> PyResult<PyDrawable> {
        let expression = trace_scalar_function(function, "x", "number-line function")?;
        let reveal = reveal.map(extract_expr).transpose()?;
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .number_line_expression_plot(
                &self.inner,
                expression,
                "x",
                domain.unwrap_or_else(|| self.inner.domain()),
                normal_scale,
                reveal,
                sampling(samples, tolerance)?,
            )
            .map(PyDrawable)
            .map_err(value_error)
    }

    fn layer(&self, name: &str) -> PyResult<PyDrawable> {
        let layer = match name {
            "axis" => SpaceLayer::Axes,
            "ticks" => SpaceLayer::Ticks,
            "numbers" => SpaceLayer::Numbers,
            "labels" => SpaceLayer::Labels,
            _ => return Err(value_error("layer must be axis, ticks, numbers, or labels")),
        };
        self.inner
            .layer(layer)
            .cloned()
            .map(PyDrawable)
            .ok_or_else(|| value_error("layer is not available"))
    }

    #[pyo3(signature = (duration=None))]
    fn create(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().create(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn write(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().write(duration),
        }
    }
}

#[pyclass(name = "PolarSpace", module = "gaanim_core", skip_from_py_object)]
#[derive(Clone)]
pub struct PyPolarSpace {
    inner: PolarSpaceHandle,
    canvas: Arc<Mutex<ApiCanvas>>,
}

#[pymethods]
impl PyPolarSpace {
    fn drawable(&self) -> PyDrawable {
        PyDrawable(self.inner.drawable().clone())
    }

    fn coord(&self, radius: f64, angle: f64) -> PyResult<PyCoordinateRef> {
        self.inner
            .coord(radius, angle)
            .map(PyCoordinateRef)
            .map_err(value_error)
    }

    fn layer(&self, name: &str) -> PyResult<PyDrawable> {
        let layer = match name {
            "grid" => SpaceLayer::MajorGrid,
            "axes" => SpaceLayer::Axes,
            "numbers" => SpaceLayer::Numbers,
            _ => return Err(value_error("layer must be grid, axes, or numbers")),
        };
        self.inner
            .layer(layer)
            .cloned()
            .map(PyDrawable)
            .ok_or_else(|| value_error("layer is not available"))
    }

    #[pyo3(signature = (function, domain=(0.0, std::f64::consts::TAU), *, samples=360))]
    fn plot(
        &self,
        function: Bound<'_, PyAny>,
        domain: (f64, f64),
        samples: usize,
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() {
            return Err(PyTypeError::new_err("function must be callable"));
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .polar_plot(&self.inner, domain, samples, |angle| {
                function
                    .call1((angle,))
                    .and_then(|value| value.extract::<f64>())
                    .ok()
            })
            .map(PyDrawable)
            .map_err(value_error)
    }

    #[pyo3(signature = (duration=None))]
    fn create(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().create(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn write(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().write(duration),
        }
    }
}

impl PyCoordinateSpace3D {
    fn new(inner: CoordinateSpace3DHandle, canvas: Arc<Mutex<ApiCanvas>>) -> Self {
        Self { inner, canvas }
    }
}

#[pymethods]
impl PyCoordinateSpace3D {
    fn drawable(&self) -> PyDrawable {
        PyDrawable(self.inner.drawable().clone())
    }

    fn layer(&self, name: &str) -> PyResult<PyDrawable> {
        let layer = match name {
            "grid" => SpaceLayer::MajorGrid,
            "axis" | "axes" => SpaceLayer::Axes,
            "ticks" => SpaceLayer::Ticks,
            "numbers" => SpaceLayer::Numbers,
            "labels" => SpaceLayer::Labels,
            _ => {
                return Err(value_error(
                    "layer must be grid, axes, ticks, numbers, or labels",
                ));
            }
        };
        self.inner
            .layer(layer)
            .cloned()
            .map(PyDrawable)
            .ok_or_else(|| value_error(format!("layer {name:?} is not present")))
    }

    fn at_3d(&self, x: f64, y: f64, z: f64) -> Self {
        Self::new(self.inner.clone().at([x, y, z]), self.canvas.clone())
    }

    fn scaled(&self, factor: f64) -> Self {
        Self::new(self.inner.clone().scaled(factor), self.canvas.clone())
    }

    #[pyo3(signature = (duration=None))]
    fn create(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().create(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn write(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().write(duration),
        }
    }

    fn data_to_local(&self, x: f64, y: f64, z: f64) -> PyResult<(f64, f64, f64)> {
        self.inner
            .data_to_local([x, y, z])
            .map(|point| (point[0], point[1], point[2]))
            .map_err(value_error)
    }

    fn local_to_data(&self, x: f64, y: f64, z: f64) -> PyResult<(f64, f64, f64)> {
        self.inner
            .local_to_data([x, y, z])
            .map(|point| (point[0], point[1], point[2]))
            .map_err(value_error)
    }

    #[pyo3(signature = (function, *, resolution=(64, 48)))]
    fn surface(
        &self,
        function: Bound<'_, PyAny>,
        resolution: (usize, usize),
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() {
            return Err(PyTypeError::new_err("function must be callable"));
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .surface_plot(&self.inner, [resolution.0, resolution.1], |x, y| {
                function
                    .call1((x, y))
                    .and_then(|value| value.extract::<f64>())
                    .ok()
            })
            .map(PyDrawable)
            .map_err(value_error)
    }

    #[pyo3(signature = (function, domain, *, samples=320))]
    fn parametric(
        &self,
        function: Bound<'_, PyAny>,
        domain: (f64, f64),
        samples: usize,
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() {
            return Err(PyTypeError::new_err("function must be callable"));
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .parametric_plot_3d(&self.inner, domain, samples, |t| {
                function
                    .call1((t,))
                    .and_then(|value| value.extract::<(f64, f64, f64)>())
                    .map(|point| [point.0, point.1, point.2])
                    .ok()
            })
            .map(PyDrawable)
            .map_err(value_error)
    }

    #[pyo3(signature = (function, *, resolution=(8, 8, 6), max_length=24.0))]
    fn vector_field(
        &self,
        function: Bound<'_, PyAny>,
        resolution: (usize, usize, usize),
        max_length: f64,
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() {
            return Err(PyTypeError::new_err("function must be callable"));
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .vector_field_plot_3d(
                &self.inner,
                [resolution.0, resolution.1, resolution.2],
                max_length,
                |x, y, z| {
                    function
                        .call1((x, y, z))
                        .and_then(|value| value.extract::<(f64, f64, f64)>())
                        .map(|vector| [vector.0, vector.1, vector.2])
                        .ok()
                },
            )
            .map(PyDrawable)
            .map_err(value_error)
    }
}

impl PyCoordinateSpace {
    fn new(inner: CoordinateSpaceHandle, canvas: Arc<Mutex<ApiCanvas>>) -> Self {
        Self { inner, canvas }
    }
}

#[pymethods]
impl PyCoordinateSpace {
    fn drawable(&self) -> PyDrawable {
        PyDrawable(self.inner.drawable().clone())
    }

    fn at(&self, x: f64, y: f64) -> Self {
        Self::new(self.inner.clone().at(x, y), self.canvas.clone())
    }

    fn scaled(&self, factor: f64) -> Self {
        Self::new(self.inner.clone().scaled(factor), self.canvas.clone())
    }

    fn rotated(&self, radians: f64) -> Self {
        Self::new(self.inner.clone().rotated(radians), self.canvas.clone())
    }

    #[pyo3(signature = (duration=None))]
    fn create(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().create(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn write(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.write(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn fade_in(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().fade_in(duration),
        }
    }

    #[pyo3(signature = (duration=None))]
    fn fade_out(&self, duration: Option<f64>) -> PyCanvasAnim {
        PyCanvasAnim {
            inner: self.inner.drawable().fade_out(duration),
        }
    }

    #[pyo3(signature = (x_domain, y_domain, *, duration=1.0))]
    fn animate_view(
        &self,
        x_domain: (f64, f64),
        y_domain: (f64, f64),
        duration: f64,
    ) -> PyResult<Vec<PyCanvasAnim>> {
        self.inner
            .animate_view(x_domain, y_domain, duration)
            .map(|animations| {
                animations
                    .into_iter()
                    .map(|inner| PyCanvasAnim { inner })
                    .collect()
            })
            .map_err(value_error)
    }

    fn coord(&self, x: f64, y: f64) -> PyResult<PyCoordinateRef> {
        self.inner
            .coord(x, y)
            .map(PyCoordinateRef)
            .map_err(value_error)
    }

    fn data_to_local(&self, x: f64, y: f64) -> PyResult<(f64, f64)> {
        self.inner.data_to_local(x, y).map_err(value_error)
    }

    fn local_to_data(&self, x: f64, y: f64) -> PyResult<(f64, f64)> {
        self.inner.local_to_data(x, y).map_err(value_error)
    }

    fn layer(&self, name: &str) -> PyResult<PyDrawable> {
        let layer = match name {
            "grid" | "major_grid" => SpaceLayer::MajorGrid,
            "minor_grid" => SpaceLayer::MinorGrid,
            "axis" | "axes" => SpaceLayer::Axes,
            "ticks" => SpaceLayer::Ticks,
            "numbers" => SpaceLayer::Numbers,
            "labels" => SpaceLayer::Labels,
            _ => {
                return Err(value_error(
                    "layer must be grid, minor_grid, axes, ticks, numbers, or labels",
                ));
            }
        };
        self.inner
            .layer(layer)
            .cloned()
            .map(PyDrawable)
            .ok_or_else(|| value_error("layer is not available on this space"))
    }

    #[pyo3(signature = (function, domain=None, *, samples=None, tolerance=0.75, derivative=0))]
    fn plot(
        &self,
        function: Bound<'_, PyAny>,
        domain: Option<(f64, f64)>,
        samples: Option<usize>,
        tolerance: f64,
        derivative: usize,
    ) -> PyResult<PyDrawable> {
        let domain = domain.unwrap_or_else(|| self.inner.map().x.domain());
        let sampling = sampling(samples, tolerance)?;
        let expression = trace_scalar_function(function, "x", "plot")?;
        let expression = (0..derivative).fold(expression, |value, _| value.derivative("x"));
        let mut canvas = self.canvas.lock().expect("scene canvas poisoned");
        canvas
            .expression_plot(&self.inner, expression, "x", domain, sampling)
            .map(PyDrawable)
            .map_err(value_error)
    }

    #[pyo3(name = "function", signature = (function, domain=None, *, samples=None, tolerance=0.75, derivative=0))]
    fn function_plot(
        &self,
        function: Bound<'_, PyAny>,
        domain: Option<(f64, f64)>,
        samples: Option<usize>,
        tolerance: f64,
        derivative: usize,
    ) -> PyResult<PyDrawable> {
        self.plot(function, domain, samples, tolerance, derivative)
    }

    #[pyo3(signature = (function, domain, *, samples=None, tolerance=0.75))]
    fn parametric(
        &self,
        function: Bound<'_, PyAny>,
        domain: (f64, f64),
        samples: Option<usize>,
        tolerance: f64,
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() {
            return Err(PyTypeError::new_err("function must be callable"));
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .parametric_plot(&self.inner, domain, sampling(samples, tolerance)?, |t| {
                function
                    .call1((t,))
                    .and_then(|value| value.extract::<(f64, f64)>())
                    .ok()
            })
            .map(PyDrawable)
            .map_err(value_error)
    }

    #[pyo3(signature = (function, *, resolution=(96, 64)))]
    fn implicit(
        &self,
        function: Bound<'_, PyAny>,
        resolution: (usize, usize),
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() {
            return Err(PyTypeError::new_err("function must be callable"));
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .implicit_plot(&self.inner, [resolution.0, resolution.1], |x, y| {
                function
                    .call1((x, y))
                    .and_then(|value| value.extract::<f64>())
                    .ok()
            })
            .map(PyDrawable)
            .map_err(value_error)
    }

    #[pyo3(signature = (function, levels, *, resolution=(96, 64)))]
    fn contour(
        &self,
        function: Bound<'_, PyAny>,
        levels: Vec<f64>,
        resolution: (usize, usize),
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() {
            return Err(PyTypeError::new_err("function must be callable"));
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .contour_plot(
                &self.inner,
                &levels,
                [resolution.0, resolution.1],
                |x, y| {
                    function
                        .call1((x, y))
                        .and_then(|value| value.extract::<f64>())
                        .ok()
                },
            )
            .map(PyDrawable)
            .map_err(value_error)
    }

    #[pyo3(signature = (function, *, resolution=(20, 12), max_length=28.0))]
    fn vector_field(
        &self,
        function: Bound<'_, PyAny>,
        resolution: (usize, usize),
        max_length: f64,
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() {
            return Err(PyTypeError::new_err("function must be callable"));
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .vector_field_plot(
                &self.inner,
                [resolution.0, resolution.1],
                max_length,
                |x, y| {
                    function
                        .call1((x, y))
                        .and_then(|value| value.extract::<(f64, f64)>())
                        .ok()
                },
            )
            .map(PyDrawable)
            .map_err(value_error)
    }

    /// Orthogonal projections from a data point to both coordinate axes.
    fn projections(&self, x: f64, y: f64) -> PyResult<PyDrawable> {
        let x_cross = self.inner.map().x.crossing_value();
        let y_cross = self.inner.map().y.crossing_value();
        let mut canvas = self.canvas.lock().expect("scene canvas poisoned");
        let horizontal = canvas
            .coordinate_segment(&self.inner, (x_cross, y), (x, y))
            .map_err(value_error)?
            .stroke(gaanim_core::peniko::Color::from_rgb8(0x80, 0x80, 0x80), 1.5);
        let vertical = canvas
            .coordinate_segment(&self.inner, (x, y_cross), (x, y))
            .map_err(value_error)?
            .stroke(gaanim_core::peniko::Color::from_rgb8(0x80, 0x80, 0x80), 1.5);
        Ok(PyDrawable(canvas.group(&[&horizontal, &vertical])))
    }

    fn secant(&self, function: Bound<'_, PyAny>, x0: f64, x1: f64) -> PyResult<PyDrawable> {
        if !function.is_callable() || !x0.is_finite() || !x1.is_finite() || x0 == x1 {
            return Err(value_error(
                "secant requires a callable and two distinct finite x values",
            ));
        }
        let y0 = function.call1((x0,))?.extract::<f64>()?;
        let y1 = function.call1((x1,))?.extract::<f64>()?;
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .coordinate_segment(&self.inner, (x0, y0), (x1, y1))
            .map(PyDrawable)
            .map_err(value_error)
    }

    #[pyo3(signature = (function, x, *, length=None, dx=None))]
    fn tangent(
        &self,
        function: Bound<'_, PyAny>,
        x: f64,
        length: Option<f64>,
        dx: Option<f64>,
    ) -> PyResult<PyDrawable> {
        self.calculus_line(function, x, length, dx, false)
    }

    #[pyo3(signature = (function, x, *, length=None, dx=None))]
    fn normal(
        &self,
        function: Bound<'_, PyAny>,
        x: f64,
        length: Option<f64>,
        dx: Option<f64>,
    ) -> PyResult<PyDrawable> {
        self.calculus_line(function, x, length, dx, true)
    }

    #[pyo3(signature = (function, domain, *, samples=160, baseline=0.0))]
    fn area_under(
        &self,
        function: Bound<'_, PyAny>,
        domain: (f64, f64),
        samples: usize,
        baseline: f64,
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() || samples < 2 || domain.0 >= domain.1 {
            return Err(value_error(
                "area_under requires a valid callable, domain, and samples >= 2",
            ));
        }
        let mut x_values = Vec::with_capacity(samples);
        let mut y_values = Vec::with_capacity(samples);
        for index in 0..samples {
            let progress = index as f64 / (samples - 1) as f64;
            let x = domain.0 + (domain.1 - domain.0) * progress;
            x_values.push(Some(x));
            y_values.push(Some(function.call1((x,))?.extract::<f64>()?));
        }
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .data_line(
                &self.inner,
                &x_values,
                &y_values,
                false,
                Some(baseline),
                NonFinitePolicy::Gap,
            )
            .map(PyDrawable)
            .map_err(value_error)
    }

    #[pyo3(signature = (xs, ys, *, step=false, baseline=None, policy="gap", color=None, width=None))]
    /// Plot a raw data series in this space's data coordinates.
    ///
    /// `xs` and `ys` are matching lists of floats (`None` marks a missing
    /// sample). The curve is drawn in data coordinates and follows the plane,
    /// so repositioning the space carries the series with it. `policy`
    /// controls non-finite samples: `"gap"` splits the line, `"drop"` skips
    /// the point but stays connected, `"error"` raises. Pass `step=True` for
    /// a step chart, `baseline` (data units) for a filled area, and
    /// `color`/`width` to restyle the stroke.
    ///
    /// ```python
    /// plane = scene.cartesian_2d(Axis.linear(0, 30), Axis.linear(-0.4, 0.4))
    /// curve = plane.plot_data(times, accel, color=CYAN, width=4)
    /// ```
    fn plot_data(
        &self,
        xs: Vec<Option<f64>>,
        ys: Vec<Option<f64>>,
        step: bool,
        baseline: Option<f64>,
        policy: &str,
        color: Option<PyColor>,
        width: Option<f64>,
    ) -> PyResult<PyDrawable> {
        if xs.is_empty() || xs.len() != ys.len() {
            return Err(value_error(
                "plot_data requires non-empty xs and ys lists of matching length",
            ));
        }
        let policy = parse_non_finite_policy(policy)?;
        let handle = self
            .canvas
            .lock()
            .expect("scene canvas poisoned")
            .data_line(&self.inner, &xs, &ys, step, baseline, policy)
            .map_err(value_error)?;
        let handle = match color {
            Some(color) => handle.stroke(color.0, width.unwrap_or(3.0)),
            None if width.is_some() => handle.stroke(
                gaanim_core::peniko::Color::from_rgb8(0x19, 0x32, 0x64),
                width.unwrap_or(3.0),
            ),
            None => handle,
        };
        Ok(PyDrawable(handle))
    }

    #[pyo3(signature = (xs, ys, *, radius=6.0, policy="gap", color=None))]
    /// Plot a data series as scatter dots in this space's data coordinates.
    ///
    /// `xs` and `ys` are matching lists of floats (`None` marks a missing
    /// sample); `policy` handles non-finite samples (`"gap"`, `"drop"`,
    /// `"error"`). Pass `color` to restyle the dots.
    fn scatter_data(
        &self,
        xs: Vec<Option<f64>>,
        ys: Vec<Option<f64>>,
        radius: f64,
        policy: &str,
        color: Option<PyColor>,
    ) -> PyResult<PyDrawable> {
        if xs.is_empty() || xs.len() != ys.len() {
            return Err(value_error(
                "scatter_data requires non-empty xs and ys lists of matching length",
            ));
        }
        if !radius.is_finite() || radius <= 0.0 {
            return Err(value_error("scatter_data requires a positive finite radius"));
        }
        let policy = parse_non_finite_policy(policy)?;
        let handle = self
            .canvas
            .lock()
            .expect("scene canvas poisoned")
            .data_scatter(&self.inner, &xs, &ys, radius, policy)
            .map_err(value_error)?;
        let handle = match color {
            Some(color) => handle.fill(color.0),
            None => handle,
        };
        Ok(PyDrawable(handle))
    }

    #[pyo3(signature = (function, domain, *, rectangles=12, method="midpoint", baseline=0.0))]
    fn riemann_sum(
        &self,
        function: Bound<'_, PyAny>,
        domain: (f64, f64),
        rectangles: usize,
        method: &str,
        baseline: f64,
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() || rectangles == 0 || domain.0 >= domain.1 {
            return Err(value_error(
                "riemann_sum requires a valid callable, domain, and positive rectangle count",
            ));
        }
        let width = (domain.1 - domain.0) / rectangles as f64;
        let offset = match method {
            "left" => 0.0,
            "midpoint" | "middle" => 0.5,
            "right" => 1.0,
            _ => return Err(value_error("method must be left, midpoint, or right")),
        };
        let mut centers = Vec::with_capacity(rectangles);
        let mut values = Vec::with_capacity(rectangles);
        for index in 0..rectangles {
            let left = domain.0 + index as f64 * width;
            let sample_x = left + offset * width;
            centers.push(left + width * 0.5);
            values.push(function.call1((sample_x,))?.extract::<f64>()?);
        }
        let table = NativeDataTable::numeric([("x".to_owned(), centers), ("y".to_owned(), values)])
            .map_err(value_error)?;
        let source = NativeDataSource::new(table);
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .data_mark(
                &self.inner,
                source,
                DataMarkKind::Bars {
                    x: "x".to_owned(),
                    y: "y".to_owned(),
                    width,
                    baseline,
                },
            )
            .map(PyDrawable)
            .map_err(value_error)
    }
}

impl PyCoordinateSpace {
    fn calculus_line(
        &self,
        function: Bound<'_, PyAny>,
        x: f64,
        length: Option<f64>,
        dx: Option<f64>,
        normal: bool,
    ) -> PyResult<PyDrawable> {
        if !function.is_callable() || !x.is_finite() {
            return Err(value_error("function must be callable and x finite"));
        }
        let x_domain = self.inner.map().x.domain();
        let length = length.unwrap_or((x_domain.1 - x_domain.0) * 0.3);
        let dx = dx.unwrap_or((x_domain.1 - x_domain.0).abs() * 1e-5);
        if !length.is_finite() || length <= 0.0 || !dx.is_finite() || dx <= 0.0 {
            return Err(value_error("length and dx must be finite and positive"));
        }
        let y = function.call1((x,))?.extract::<f64>()?;
        let y_minus = function.call1((x - dx,))?.extract::<f64>()?;
        let y_plus = function.call1((x + dx,))?.extract::<f64>()?;
        let derivative = (y_plus - y_minus) / (2.0 * dx);
        let slope = if normal {
            if derivative.abs() <= f64::EPSILON {
                f64::INFINITY
            } else {
                -1.0 / derivative
            }
        } else {
            derivative
        };
        let (start, end) = if slope.is_finite() {
            let half = length * 0.5;
            ((x - half, y - slope * half), (x + half, y + slope * half))
        } else {
            ((x, y - length * 0.5), (x, y + length * 0.5))
        };
        self.canvas
            .lock()
            .expect("scene canvas poisoned")
            .coordinate_segment(&self.inner, start, end)
            .map(PyDrawable)
            .map_err(value_error)
    }
}

fn normalize_columns<'py>(columns: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    if let Ok(dict) = columns.cast::<PyDict>() {
        return Ok(dict.clone());
    }
    if columns.hasattr("to_dict")? {
        let result = columns.call_method1("to_dict", ("list",))?;
        return result
            .cast_into::<PyDict>()
            .map_err(|_| PyTypeError::new_err("to_dict('list') did not return a mapping"));
    }
    Err(PyTypeError::new_err(
        "columns must be a dict or an object implementing to_dict('list')",
    ))
}

fn table_from_python(columns: &Bound<'_, PyAny>) -> PyResult<NativeDataTable> {
    let columns = normalize_columns(columns)?;
    let mut native = Vec::with_capacity(columns.len());
    for (name, values) in columns.iter() {
        let name = name.extract::<String>()?;
        let values = if values.hasattr("tolist")? {
            values.call_method0("tolist")?
        } else {
            values
        };
        if let Ok(numbers) = values.extract::<Vec<Option<f64>>>() {
            native.push((name, Column::Numeric(numbers)));
        } else if let Ok(strings) = values.extract::<Vec<Option<String>>>() {
            native.push((name, Column::Text(strings)));
        } else {
            return Err(PyTypeError::new_err(format!(
                "column '{name}' must contain only numbers/None or strings/None"
            )));
        }
    }
    NativeDataTable::new(native).map_err(value_error)
}

#[pyclass(name = "DataTable", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyDataTable(pub NativeDataTable);

#[pymethods]
impl PyDataTable {
    #[new]
    fn new(columns: Bound<'_, PyAny>) -> PyResult<Self> {
        table_from_python(&columns).map(Self)
    }

    fn __len__(&self) -> usize {
        self.0.len()
    }

    #[getter]
    fn columns(&self) -> Vec<String> {
        self.0.columns().map(|(name, _)| name.to_owned()).collect()
    }
}

#[pyclass(name = "DataSource", module = "gaanim_core", from_py_object)]
#[derive(Clone)]
pub struct PyDataSource {
    pub(crate) inner: NativeDataSource,
    key: Option<String>,
}

#[pymethods]
impl PyDataSource {
    #[new]
    #[pyo3(signature = (data, *, key=None))]
    fn new(data: Bound<'_, PyAny>, key: Option<String>) -> PyResult<Self> {
        let table = if let Ok(table) = data.extract::<PyRef<'_, PyDataTable>>() {
            table.0.clone()
        } else {
            table_from_python(&data)?
        };
        if let Some(key) = key.as_deref() {
            table.column(key).map_err(value_error)?;
        }
        Ok(Self {
            inner: NativeDataSource::new(table),
            key,
        })
    }

    fn replace(&self, data: Bound<'_, PyAny>) -> PyResult<()> {
        let table = if let Ok(table) = data.extract::<PyRef<'_, PyDataTable>>() {
            table.0.clone()
        } else {
            table_from_python(&data)?
        };
        if let Some(key) = self.key.as_deref() {
            table.column(key).map_err(value_error)?;
        }
        self.inner.replace(table);
        Ok(())
    }

    fn append(&self, data: Bound<'_, PyAny>) -> PyResult<()> {
        let table = if let Ok(table) = data.extract::<PyRef<'_, PyDataTable>>() {
            table.0.clone()
        } else {
            table_from_python(&data)?
        };
        self.inner.append(&table).map_err(value_error)
    }

    #[getter]
    fn version(&self) -> u64 {
        self.inner.version()
    }

    fn __len__(&self) -> usize {
        self.inner.snapshot().len()
    }
}

#[pymethods]
impl PyDrawable {
    fn at_coordinate(&self, coordinate: &PyCoordinateRef) -> Self {
        Self(self.0.clone().at_coordinate(coordinate.0))
    }
}

#[pymethods]
impl PyScene {
    fn parameter(&self, initial: f64) -> PyResult<PyParameter> {
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .parameter(initial)
            .map_err(value_error)?;
        Ok(PyParameter { inner })
    }

    #[pyo3(signature = (source, *, label=None, format=".2f", prefix="", suffix="", unit=None, font_size=None, color=None, invalid="—"))]
    #[allow(clippy::too_many_arguments)]
    fn readout<'py>(
        &self,
        py: Python<'py>,
        source: Bound<'py, PyAny>,
        label: Option<String>,
        format: &str,
        prefix: &str,
        suffix: &str,
        unit: Option<String>,
        font_size: Option<f64>,
        color: Option<PyColor>,
        invalid: &str,
    ) -> PyResult<Py<PyReadout>> {
        let expression = trace_readout_source(source)?;
        let (group, label_part, equals_part, number_part, unit_part) = build_readout_parts(
            &mut self.inner.lock().expect("scene canvas poisoned"),
            expression,
            label,
            format.to_owned(),
            prefix.to_owned(),
            suffix.to_owned(),
            unit,
            font_size,
            color,
            invalid.to_owned(),
        );
        Py::new(
            py,
            PyReadout::initializer(group, label_part, equals_part, number_part, unit_part),
        )
    }

    #[pyo3(signature = (initial, *, label, format=".2f", prefix="", suffix="", unit=None, font_size=None, color=None, invalid="—"))]
    #[allow(clippy::too_many_arguments)]
    fn variable<'py>(
        &self,
        py: Python<'py>,
        initial: f64,
        label: String,
        format: &str,
        prefix: &str,
        suffix: &str,
        unit: Option<String>,
        font_size: Option<f64>,
        color: Option<PyColor>,
        invalid: &str,
    ) -> PyResult<Py<PyVariable>> {
        let mut canvas = self.inner.lock().expect("scene canvas poisoned");
        let parameter = PyParameter {
            inner: canvas.parameter(initial).map_err(value_error)?,
        };
        let (group, label_part, equals_part, number_part, unit_part) = build_readout_parts(
            &mut canvas,
            parameter.inner.expression(),
            Some(label),
            format.to_owned(),
            prefix.to_owned(),
            suffix.to_owned(),
            unit,
            font_size,
            color,
            invalid.to_owned(),
        );
        Py::new(
            py,
            PyVariable::initializer(
                group,
                parameter,
                label_part,
                equals_part,
                number_part,
                unit_part,
            ),
        )
    }

    fn chart(&self, spec: &PyChartSpec) -> PyResult<PyChart> {
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .chart(spec.0.clone())
            .map_err(value_error)?;
        Ok(PyChart::new(inner, self.inner.clone()))
    }

    #[pyo3(signature = (x, y, *, width=None, height=None, grid=true))]
    fn cartesian_2d(
        &self,
        x: &PyAxis,
        y: &PyAxis,
        width: Option<f64>,
        height: Option<f64>,
        grid: bool,
    ) -> PyResult<PyCoordinateSpace> {
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .coordinate_axes(x.0.clone(), y.0.clone(), width, height, grid)
            .map_err(value_error)?;
        Ok(PyCoordinateSpace::new(inner, self.inner.clone()))
    }

    #[pyo3(signature = (x, y, z, *, size=(10.0, 8.0, 6.0), grid=true))]
    fn cartesian_3d(
        &self,
        x: &PyAxis,
        y: &PyAxis,
        z: &PyAxis,
        size: (f64, f64, f64),
        grid: bool,
    ) -> PyResult<PyCoordinateSpace3D> {
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .coordinate_axes_3d(
                x.0.clone(),
                y.0.clone(),
                z.0.clone(),
                [size.0, size.1, size.2],
                grid,
            )
            .map_err(value_error)?;
        Ok(PyCoordinateSpace3D::new(inner, self.inner.clone()))
    }

    #[pyo3(signature = (radial, *, radius=220.0, angle_divisions=12))]
    fn polar(
        &self,
        radial: &PyAxis,
        radius: f64,
        angle_divisions: usize,
    ) -> PyResult<PyPolarSpace> {
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .coordinate_polar_plane(radial.0.clone(), radius, angle_divisions)
            .map_err(value_error)?;
        Ok(PyPolarSpace {
            inner,
            canvas: self.inner.clone(),
        })
    }

    #[pyo3(signature = (x=None, y=None, *, width=None, height=None))]
    fn complex(
        &self,
        x: Option<&PyAxis>,
        y: Option<&PyAxis>,
        width: Option<f64>,
        height: Option<f64>,
    ) -> PyResult<PyCoordinateSpace> {
        let x = x.map(|axis| axis.0.clone()).unwrap_or(
            NativeAxis::linear(-5.0, 5.0)
                .map_err(value_error)?
                .label("Re"),
        );
        let y = y.map(|axis| axis.0.clone()).unwrap_or(
            NativeAxis::linear(-3.0, 3.0)
                .map_err(value_error)?
                .label("Im"),
        );
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .coordinate_axes(x, y, width, height, true)
            .map_err(value_error)?;
        Ok(PyCoordinateSpace::new(inner, self.inner.clone()))
    }

    #[pyo3(signature = (axis, *, length=None))]
    fn number_line(&self, axis: &PyAxis, length: Option<f64>) -> PyResult<PyNumberLine> {
        let inner = self
            .inner
            .lock()
            .expect("scene canvas poisoned")
            .coordinate_number_line(axis.0.clone(), length)
            .map_err(value_error)?;
        Ok(PyNumberLine {
            inner,
            canvas: self.inner.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_scene::prelude::World;
    use gaanim_timeline::timeline::Timeline;

    #[test]
    fn readout_parts_share_default_and_explicit_font_sizes() {
        for expected in [DEFAULT_REACTIVE_TEXT_SIZE, 56.0] {
            let mut canvas = ApiCanvas::new(1920, 1080);
            let (_, label, equals, _, unit) = build_readout_parts(
                &mut canvas,
                NativeExpr::constant(12.0),
                Some("$x$".to_owned()),
                ".1f".to_owned(),
                String::new(),
                String::new(),
                Some("m".to_owned()),
                (expected != DEFAULT_REACTIVE_TEXT_SIZE).then_some(expected),
                None,
                "—".to_owned(),
            );

            for part in [label.as_ref(), equals.as_ref(), unit.as_ref()] {
                assert_eq!(
                    part.expect("readout text part")
                        .0
                        .text_spec()
                        .expect("readout text spec")
                        .style
                        .size,
                    Some(expected)
                );
            }

            let mut world = World::new();
            world.insert_resource(Timeline::new());
            world.insert_resource(gaanim_text::font::FontRegistry::new());
            world.insert_resource(gaanim_text::prelude::TextConfig::default());
            canvas.compile(&mut world);
            world.flush();
            assert_eq!(
                world
                    .query::<&gaanim_animation::ReactiveReadout>()
                    .single(&world)
                    .expect("compiled readout")
                    .font_size,
                expected
            );
        }
    }
}
