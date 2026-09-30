"""Deterministic descriptions of Python callbacks for hot-reload fingerprints.

``recipe(function)`` returns text that fully describes what ``function``
computes from its arguments, or ``None`` when that cannot be shown. A recipe
covers the bytecode (not its file or line numbers), argument defaults, captured
variables and the globals the code reads. Every value involved must be a plain
immutable value, a function that has a recipe itself, or a standard-library or
installed module or builtin. Modules of the project are rejected: hot reload
imports them again, so the same name may hold different values.
"""

import builtins
import sys
import sysconfig
import types

_PLAIN = (int, float, complex, str, bytes, bool, type(None), type(Ellipsis))
_MAX_DEPTH = 24
_LIBRARY_ROOTS = tuple(
    root.lower()
    for root in {sysconfig.get_path("stdlib"), sysconfig.get_path("platstdlib"),
                 sysconfig.get_path("purelib"), sysconfig.get_path("platlib")}
    if root
)


class _Opaque(Exception):
    pass


def recipe(function):
    try:
        return _function(function, {}, 0)
    except Exception:
        return None


def _function(function, seen, depth):
    if type(function) is not types.FunctionType or depth > _MAX_DEPTH:
        raise _Opaque
    key = id(function)
    if key in seen:
        return seen[key]
    seen[key] = f"#{len(seen)}"
    code = function.__code__
    kwdefaults = tuple(sorted((function.__kwdefaults__ or {}).items()))
    closure = tuple(cell.cell_contents for cell in function.__closure__ or ())
    parts = (
        _code(code, depth),
        _value(function.__defaults__, seen, depth),
        _value(kwdefaults, seen, depth),
        _value(closure, seen, depth),
        _globals(function, code, seen, depth),
    )
    return "fn(" + "|".join(parts) + ")"


def _code(code, depth):
    if depth > _MAX_DEPTH:
        raise _Opaque
    consts = tuple(
        _code(const, depth + 1) if isinstance(const, types.CodeType) else _value(const, {}, depth)
        for const in code.co_consts
    )
    return repr((
        code.co_code,
        consts,
        code.co_names,
        code.co_varnames,
        code.co_freevars,
        code.co_cellvars,
        code.co_argcount,
        code.co_posonlyargcount,
        code.co_kwonlyargcount,
        code.co_flags,
        getattr(code, "co_exceptiontable", b""),
    ))


def _names(code):
    names = set(code.co_names)
    for const in code.co_consts:
        if isinstance(const, types.CodeType):
            names |= _names(const)
    return names


def _globals(function, code, seen, depth):
    namespace = function.__globals__
    parts = []
    for name in sorted(_names(code)):
        if name in namespace:
            parts.append(f"{name}={_value(namespace[name], seen, depth)}")
        elif hasattr(builtins, name):
            parts.append(f"{name}=builtin")
        # Otherwise an attribute name, already part of the bytecode.
    return ";".join(parts)


def _value(value, seen, depth):
    if depth > _MAX_DEPTH:
        raise _Opaque
    kind = type(value)
    if kind in _PLAIN:
        return repr(value)
    if kind is tuple:
        return "(" + ",".join(_value(item, seen, depth + 1) for item in value) + ",)"
    if kind is frozenset:
        return "frozenset(" + ",".join(sorted(_value(item, seen, depth + 1) for item in value)) + ")"
    if kind is types.FunctionType:
        return _function(value, seen, depth + 1)
    if kind is types.ModuleType and _is_library(value):
        return f"module {value.__name__}"
    if kind is types.BuiltinFunctionType:
        owner = value.__self__
        if type(owner) is types.ModuleType and _is_library(owner):
            return f"builtin {owner.__name__}.{value.__name__}"
    raise _Opaque


def _is_library(module):
    """Whether hot reload keeps `module` as it is: builtin, standard or installed."""
    name = module.__name__
    if name in sys.builtin_module_names:
        return True
    path = getattr(module, "__file__", None)
    if path is None:
        # Frozen standard modules have no file; a project module always has one.
        return name.partition(".")[0] in sys.stdlib_module_names
    return path.lower().startswith(_LIBRARY_ROOTS)
