//! Native scalar and vector expression trees used by reactive visualizations.

use gaanim_core::ObjectId;
use std::collections::HashMap;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// Evaluation errors are explicit so invalid domains become plot gaps instead
/// of leaking NaNs through the renderer.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EvalError {
    #[error("variable '{0}' has no value")]
    MissingVariable(String),
    #[error("parameter {0} has no value")]
    MissingParameter(ObjectId),
    #[error("expression evaluated outside its real-valued domain")]
    Domain,
    #[error("expression produced a non-finite value")]
    NonFinite,
}

/// Values supplied to an [`Expr`] during evaluation.
#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    variables: HashMap<String, f64>,
    parameters: HashMap<ObjectId, f64>,
}

impl EvalContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_variable(mut self, name: impl Into<String>, value: f64) -> Self {
        self.variables.insert(name.into(), value);
        self
    }

    pub fn with_parameter(mut self, id: ObjectId, value: f64) -> Self {
        self.parameters.insert(id, value);
        self
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: f64) {
        self.variables.insert(name.into(), value);
    }

    pub fn set_parameter(&mut self, id: ObjectId, value: f64) {
        self.parameters.insert(id, value);
    }

    pub fn variable(&self, name: &str) -> Option<f64> {
        self.variables.get(name).copied()
    }

    pub fn parameter(&self, id: ObjectId) -> Option<f64> {
        self.parameters.get(&id).copied()
    }
}

/// A real-valued mathematical expression.
#[derive(Debug, Clone)]
pub enum Expr {
    Constant(f64),
    Variable(String),
    Parameter(ObjectId),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Sin(Box<Expr>),
    Cos(Box<Expr>),
    Tan(Box<Expr>),
    Exp(Box<Expr>),
    Ln(Box<Expr>),
    Sqrt(Box<Expr>),
    Abs(Box<Expr>),
    Min(Box<Expr>, Box<Expr>),
    Max(Box<Expr>, Box<Expr>),
    Clamp {
        value: Box<Expr>,
        min: Box<Expr>,
        max: Box<Expr>,
    },
    /// Selects `when_true` when `condition > 0`, otherwise `when_false`.
    IfPositive {
        condition: Box<Expr>,
        when_true: Box<Expr>,
        when_false: Box<Expr>,
    },
}

impl Expr {
    pub fn constant(value: f64) -> Self {
        Self::Constant(value)
    }

    pub fn variable(name: impl Into<String>) -> Self {
        Self::Variable(name.into())
    }

    pub fn parameter(id: ObjectId) -> Self {
        Self::Parameter(id)
    }

    pub fn pow(self, exponent: impl Into<Expr>) -> Self {
        Self::Pow(Box::new(self), Box::new(exponent.into()))
    }

    pub fn sin(self) -> Self {
        Self::Sin(Box::new(self))
    }

    pub fn cos(self) -> Self {
        Self::Cos(Box::new(self))
    }

    pub fn tan(self) -> Self {
        Self::Tan(Box::new(self))
    }

    pub fn exp(self) -> Self {
        Self::Exp(Box::new(self))
    }

    pub fn ln(self) -> Self {
        Self::Ln(Box::new(self))
    }

    pub fn sqrt(self) -> Self {
        Self::Sqrt(Box::new(self))
    }

    pub fn abs(self) -> Self {
        Self::Abs(Box::new(self))
    }

    pub fn min(self, other: impl Into<Expr>) -> Self {
        Self::Min(Box::new(self), Box::new(other.into()))
    }

    pub fn max(self, other: impl Into<Expr>) -> Self {
        Self::Max(Box::new(self), Box::new(other.into()))
    }

    pub fn clamp(self, min: impl Into<Expr>, max: impl Into<Expr>) -> Self {
        Self::Clamp {
            value: Box::new(self),
            min: Box::new(min.into()),
            max: Box::new(max.into()),
        }
    }

    pub fn if_positive(self, when_true: impl Into<Expr>, when_false: impl Into<Expr>) -> Self {
        Self::IfPositive {
            condition: Box::new(self),
            when_true: Box::new(when_true.into()),
            when_false: Box::new(when_false.into()),
        }
    }

    pub fn eval(&self, context: &EvalContext) -> Result<f64, EvalError> {
        let value = match self {
            Self::Constant(value) => *value,
            Self::Variable(name) => context
                .variable(name)
                .ok_or_else(|| EvalError::MissingVariable(name.clone()))?,
            Self::Parameter(id) => context
                .parameter(*id)
                .ok_or(EvalError::MissingParameter(*id))?,
            Self::Neg(value) => -value.eval(context)?,
            Self::Add(left, right) => left.eval(context)? + right.eval(context)?,
            Self::Sub(left, right) => left.eval(context)? - right.eval(context)?,
            Self::Mul(left, right) => left.eval(context)? * right.eval(context)?,
            Self::Div(left, right) => {
                let denominator = right.eval(context)?;
                if denominator.abs() <= f64::EPSILON {
                    return Err(EvalError::Domain);
                }
                left.eval(context)? / denominator
            }
            Self::Pow(base, exponent) => base.eval(context)?.powf(exponent.eval(context)?),
            Self::Sin(value) => value.eval(context)?.sin(),
            Self::Cos(value) => value.eval(context)?.cos(),
            Self::Tan(value) => value.eval(context)?.tan(),
            Self::Exp(value) => value.eval(context)?.exp(),
            Self::Ln(value) => {
                let value = value.eval(context)?;
                if value <= 0.0 {
                    return Err(EvalError::Domain);
                }
                value.ln()
            }
            Self::Sqrt(value) => {
                let value = value.eval(context)?;
                if value < 0.0 {
                    return Err(EvalError::Domain);
                }
                value.sqrt()
            }
            Self::Abs(value) => value.eval(context)?.abs(),
            Self::Min(left, right) => left.eval(context)?.min(right.eval(context)?),
            Self::Max(left, right) => left.eval(context)?.max(right.eval(context)?),
            Self::Clamp { value, min, max } => value
                .eval(context)?
                .clamp(min.eval(context)?, max.eval(context)?),
            Self::IfPositive {
                condition,
                when_true,
                when_false,
            } => {
                if condition.eval(context)? > 0.0 {
                    when_true.eval(context)?
                } else {
                    when_false.eval(context)?
                }
            }
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(EvalError::NonFinite)
        }
    }

    /// Returns the symbolic first derivative with respect to `variable`.
    pub fn derivative(&self, variable: &str) -> Expr {
        let zero = || Expr::Constant(0.0);
        let one = || Expr::Constant(1.0);
        match self {
            Self::Constant(_) | Self::Parameter(_) => zero(),
            Self::Variable(name) => Expr::Constant((name == variable) as u8 as f64),
            Self::Neg(value) => -value.derivative(variable),
            Self::Add(left, right) => left.derivative(variable) + right.derivative(variable),
            Self::Sub(left, right) => left.derivative(variable) - right.derivative(variable),
            Self::Mul(left, right) => {
                left.derivative(variable) * (**right).clone()
                    + (**left).clone() * right.derivative(variable)
            }
            Self::Div(left, right) => {
                (left.derivative(variable) * (**right).clone()
                    - (**left).clone() * right.derivative(variable))
                    / ((**right).clone().pow(2.0))
            }
            Self::Pow(base, exponent) => {
                if let Expr::Constant(power) = **exponent {
                    if power == 0.0 {
                        zero()
                    } else {
                        Expr::Constant(power)
                            * (**base).clone().pow(power - 1.0)
                            * base.derivative(variable)
                    }
                } else {
                    let base_expr = (**base).clone();
                    let exponent_expr = (**exponent).clone();
                    base_expr.clone().pow(exponent_expr.clone())
                        * (exponent.derivative(variable) * base_expr.clone().ln()
                            + exponent_expr * base.derivative(variable) / base_expr)
                }
            }
            Self::Sin(value) => value.clone().cos() * value.derivative(variable),
            Self::Cos(value) => -((**value).clone().sin() * value.derivative(variable)),
            Self::Tan(value) => value.derivative(variable) / ((**value).clone().cos().pow(2.0)),
            Self::Exp(value) => (**value).clone().exp() * value.derivative(variable),
            Self::Ln(value) => value.derivative(variable) / (**value).clone(),
            Self::Sqrt(value) => {
                value.derivative(variable) / (Expr::Constant(2.0) * (**value).clone().sqrt())
            }
            Self::Abs(value) => {
                (**value).clone().if_positive(one(), Expr::Constant(-1.0))
                    * value.derivative(variable)
            }
            // At non-differentiable switching boundaries we choose the active
            // branch derivative, matching the expression evaluator.
            Self::Min(left, right) => ((**right).clone() - (**left).clone())
                .if_positive(left.derivative(variable), right.derivative(variable)),
            Self::Max(left, right) => ((**left).clone() - (**right).clone())
                .if_positive(left.derivative(variable), right.derivative(variable)),
            Self::Clamp { value, min, max } => {
                let below = (**value).clone() - (**min).clone();
                let above = (**max).clone() - (**value).clone();
                below.if_positive(
                    above.if_positive(value.derivative(variable), max.derivative(variable)),
                    min.derivative(variable),
                )
            }
            Self::IfPositive {
                condition,
                when_true,
                when_false,
            } => (**condition).clone().if_positive(
                when_true.derivative(variable),
                when_false.derivative(variable),
            ),
        }
    }

    pub fn parameter_ids(&self) -> Vec<ObjectId> {
        fn collect(expr: &Expr, ids: &mut Vec<ObjectId>) {
            match expr {
                Expr::Parameter(id) => ids.push(*id),
                Expr::Neg(value)
                | Expr::Sin(value)
                | Expr::Cos(value)
                | Expr::Tan(value)
                | Expr::Exp(value)
                | Expr::Ln(value)
                | Expr::Sqrt(value)
                | Expr::Abs(value) => collect(value, ids),
                Expr::Add(left, right)
                | Expr::Sub(left, right)
                | Expr::Mul(left, right)
                | Expr::Div(left, right)
                | Expr::Pow(left, right)
                | Expr::Min(left, right)
                | Expr::Max(left, right) => {
                    collect(left, ids);
                    collect(right, ids);
                }
                Expr::Clamp { value, min, max } => {
                    collect(value, ids);
                    collect(min, ids);
                    collect(max, ids);
                }
                Expr::IfPositive {
                    condition,
                    when_true,
                    when_false,
                } => {
                    collect(condition, ids);
                    collect(when_true, ids);
                    collect(when_false, ids);
                }
                Expr::Constant(_) | Expr::Variable(_) => {}
            }
        }
        let mut ids = Vec::new();
        collect(self, &mut ids);
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

impl From<f64> for Expr {
    fn from(value: f64) -> Self {
        Self::Constant(value)
    }
}

impl Add for Expr {
    type Output = Expr;
    fn add(self, rhs: Expr) -> Self::Output {
        Expr::Add(Box::new(self), Box::new(rhs))
    }
}

impl Sub for Expr {
    type Output = Expr;
    fn sub(self, rhs: Expr) -> Self::Output {
        Expr::Sub(Box::new(self), Box::new(rhs))
    }
}

impl Mul for Expr {
    type Output = Expr;
    fn mul(self, rhs: Expr) -> Self::Output {
        Expr::Mul(Box::new(self), Box::new(rhs))
    }
}

impl Div for Expr {
    type Output = Expr;
    fn div(self, rhs: Expr) -> Self::Output {
        Expr::Div(Box::new(self), Box::new(rhs))
    }
}

impl Neg for Expr {
    type Output = Expr;
    fn neg(self) -> Self::Output {
        Expr::Neg(Box::new(self))
    }
}

macro_rules! impl_scalar_op {
    ($trait:ident, $method:ident, $variant:ident) => {
        impl $trait<f64> for Expr {
            type Output = Expr;
            fn $method(self, rhs: f64) -> Self::Output {
                Expr::$variant(Box::new(self), Box::new(Expr::Constant(rhs)))
            }
        }
        impl $trait<Expr> for f64 {
            type Output = Expr;
            fn $method(self, rhs: Expr) -> Self::Output {
                Expr::$variant(Box::new(Expr::Constant(self)), Box::new(rhs))
            }
        }
    };
}

impl_scalar_op!(Add, add, Add);
impl_scalar_op!(Sub, sub, Sub);
impl_scalar_op!(Mul, mul, Mul);
impl_scalar_op!(Div, div, Div);

/// A small vector of scalar expressions, used by parametric plots and fields.
#[derive(Debug, Clone)]
pub struct VectorExpr(pub Vec<Expr>);

impl VectorExpr {
    pub fn new(values: impl IntoIterator<Item = Expr>) -> Self {
        Self(values.into_iter().collect())
    }

    pub fn eval(&self, context: &EvalContext) -> Result<Vec<f64>, EvalError> {
        self.0.iter().map(|expr| expr.eval(context)).collect()
    }

    pub fn derivative(&self, variable: &str) -> Self {
        Self(
            self.0
                .iter()
                .map(|expr| expr.derivative(variable))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_variables_and_parameters() {
        let parameter = ObjectId::from_raw(42);
        let x = Expr::variable("x");
        let expr = x.clone().sin() * Expr::parameter(parameter) + 2.0;
        let context = EvalContext::new()
            .with_variable("x", std::f64::consts::FRAC_PI_2)
            .with_parameter(parameter, 3.0);
        assert!((expr.eval(&context).unwrap() - 5.0).abs() < 1e-12);
        assert_eq!(expr.parameter_ids(), [parameter]);
    }

    #[test]
    fn differentiates_composed_expression() {
        let x = Expr::variable("x");
        let expr = x.clone().pow(3.0) + x.clone().sin();
        let derivative = expr.derivative("x");
        let context = EvalContext::new().with_variable("x", 2.0);
        let expected = 12.0 + 2.0_f64.cos();
        assert!((derivative.eval(&context).unwrap() - expected).abs() < 1e-10);
    }

    #[test]
    fn reports_real_domain_failures() {
        let context = EvalContext::new();
        assert_eq!(
            Expr::constant(-1.0).sqrt().eval(&context),
            Err(EvalError::Domain)
        );
        assert_eq!(
            Expr::constant(0.0).ln().eval(&context),
            Err(EvalError::Domain)
        );
    }

    #[test]
    fn polynomial_derivative_is_defined_at_zero() {
        let x = Expr::variable("x");
        let derivative = x.pow(2.0).derivative("x");
        let context = EvalContext::new().with_variable("x", 0.0);
        assert_eq!(derivative.eval(&context), Ok(0.0));
    }
}
