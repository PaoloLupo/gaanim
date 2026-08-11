"""Numeric-symoblic functions for reactive Gaanim expressions.

Use this module inside lambdas passed to visualization and readout factories.
It mirrors a focused subset of :mod:`math` while preserving native expression
tracing; standard-library ``math`` functions cannot trace a ``Parameter``.
"""

from math import e, pi, tau

from .gaanim_core import Parameter, Variable, _Expr


TracedScalar = float | Parameter | Variable | _Expr


def _expr(value: TracedScalar) -> _Expr:
    if isinstance(value, _Expr):
        return value
    if isinstance(value, (int, float)):
        return _Expr(float(value))
    # Parameter and Variable expose scalar operators which deliberately yield
    # the private traced representation.
    return value + 0.0


def sin(value: TracedScalar) -> _Expr: return _expr(value).sin()
def cos(value: TracedScalar) -> _Expr: return _expr(value).cos()
def tan(value: TracedScalar) -> _Expr: return _expr(value).tan()
def exp(value: TracedScalar) -> _Expr: return _expr(value).exp()
def log(value: TracedScalar) -> _Expr: return _expr(value).log()
def sqrt(value: TracedScalar) -> _Expr: return _expr(value).sqrt()
def fabs(value: TracedScalar) -> _Expr: return abs(_expr(value))
def pow(base: TracedScalar, exponent: TracedScalar) -> _Expr: return _expr(base).pow(exponent)
def minimum(left: TracedScalar, right: TracedScalar) -> _Expr: return _expr(left).min(right)
def maximum(left: TracedScalar, right: TracedScalar) -> _Expr: return _expr(left).max(right)
def clamp(value: TracedScalar, minimum_value: TracedScalar, maximum_value: TracedScalar) -> _Expr: return _expr(value).clamp(minimum_value, maximum_value)
def where_positive(condition: TracedScalar, when_true: TracedScalar, when_false: TracedScalar) -> _Expr: return _expr(condition).if_positive(when_true, when_false)


__all__ = [
    "pi", "e", "tau", "sin", "cos", "tan", "exp", "log", "sqrt", "fabs",
    "pow", "minimum", "maximum", "clamp", "where_positive",
]
