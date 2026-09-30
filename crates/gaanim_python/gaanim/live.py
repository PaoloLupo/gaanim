"""Behaviors for live zones: plain Python functions compiled for Rust.

A live zone's behavior is a function of one player that returns
``pose(...)``: where the player's character stands and which expression it
plays. Write it with ``math``, ``if``/``elif``/``else``, conditional
expressions, helper functions and module constants, as normal Python::

    import math
    from gaanim.live import pose

    WATER = 2.0

    def zipline(p):
        angle = math.radians(20 + 40 * p.random(0))
        land = 6 * math.cos(angle)
        if p.t < 1:
            return pose(-6 + land * p.t, 3 - 4 * p.t * p.t)
        wet = -6 + land > WATER
        return pose(-6 + land, -3, express="sad" if wet else "happy", since=1)

``scene.live_zone`` compiles it once, when the scene is authored: this
module reads the function's source, freezes the constants it refers to,
inlines its helpers and turns each ``if`` into a choice between values. The
result is a small program the ``.gaanim`` carries and Rust evaluates for
every player on every frame, so a presented bundle needs no Python.

The function also runs as ordinary Python on a :class:`Player`, which is
how tests check that both give the same poses. Where the two differ, the
compiled program is the one presented; see "Diferencias con Python" in the
documentation.
"""

from __future__ import annotations

import ast
import builtins
import inspect
import json
import math
import struct
import textwrap
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any, NamedTuple, Optional

__all__ = [
    "BehaviorError",
    "PLAYER_FIELDS",
    "Player",
    "Pose",
    "anticipate",
    "ballistic",
    "clamp",
    "compile_behavior",
    "ease_in",
    "ease_in_out",
    "ease_out",
    "ease_out_back",
    "impact",
    "landing",
    "lerp",
    "pose",
    "progress",
    "smoothstep",
    "spring",
    "wobble",
]

#: Version of the compiled programs; Rust refuses a newer major version.
PROGRAM_VERSION = (1, 0)

#: What a behavior reads from its player, as ``p.<name>``.
PLAYER_FIELDS = {
    "t": "seconds since the player arrived in the zone",
    "time": "seconds since the zone opened",
    "joined": "when the player arrived, in seconds since the zone opened",
    "index": "order of arrival, from 0",
    "count": "players in the zone",
    "rank": "position in the standings, 0 for the leader",
    "score": "the player's score",
    "leader": "the leader's score",
    "previous_rank": "the rank before the last change",
    "rank_since": "seconds since the rank last changed",
    "previous_score": "the score before the last change",
    "score_since": "seconds since the score last changed",
}


class Pose(NamedTuple):
    """A character's pose, as a behavior returns it."""

    x: float
    y: float
    rotation: float = 0.0
    scale: float = 1.0
    sx: float = 1.0
    sy: float = 1.0
    lean: float = 0.0
    flip: bool = False
    visible: bool = True
    express: Optional[str] = None
    since: Optional[float] = None
    loop: bool = False


def pose(
    x: float,
    y: float,
    *,
    rotation: float = 0.0,
    scale: float = 1.0,
    sx: float = 1.0,
    sy: float = 1.0,
    lean: float = 0.0,
    flip: bool = False,
    visible: bool = True,
    express: Optional[str] = None,
    since: Optional[float] = None,
    loop: bool = False,
) -> Pose:
    """Where a character stands and how it looks.

    ``(x, y)`` is where its feet are, in scene units. ``rotation`` turns it
    about its middle, in radians counterclockwise; ``scale`` multiplies the
    zone's size. ``sx`` and ``sy`` stretch it across and along its body
    about the feet, for squash and stretch (``impact`` and ``anticipate``
    give them); ``lean`` tilts it about the feet. ``flip`` mirrors it left
    to right; ``visible=False`` hides it. ``express`` plays an expression (``"happy"``, ``"sad"``, ``"hurt"``,
    ``"winner"``, ``"surprised"``), once or with ``loop=True`` until another
    one. It starts when it first appears, or at ``since``: the player's
    ``t`` when it started, for expressions that follow a moment the behavior
    computes, like landing.
    """
    return Pose(x, y, rotation, scale, sx, sy, lean, flip, visible, express, since, loop)


# --- Helpers ---------------------------------------------------------------
# Plain Python like any behavior: the compiler inlines them, and they run as
# they are in Python. Times are seconds; before its moment each returns rest.


def clamp(x: float, low: float = 0.0, high: float = 1.0) -> float:
    """``x`` kept between ``low`` and ``high``."""
    return min(max(x, low), high)


def lerp(a: float, b: float, u: float) -> float:
    """From ``a`` (u = 0) to ``b`` (u = 1)."""
    return a + (b - a) * u


def progress(t: float, start: float = 0.0, duration: float = 1.0) -> float:
    """0 before ``start``, 1 after ``start + duration``, linear between."""
    return clamp((t - start) / duration)


def smoothstep(u: float) -> float:
    """Slow in and slow out over u in [0, 1]."""
    u = clamp(u)
    return u * u * (3 - 2 * u)


def ease_in(u: float) -> float:
    """Slow in: starts gently, arrives fast (cubic)."""
    u = clamp(u)
    return u * u * u


def ease_out(u: float) -> float:
    """Slow out: leaves fast, arrives gently (cubic)."""
    u = 1 - clamp(u)
    return 1 - u * u * u


def ease_in_out(u: float) -> float:
    """Slow in and slow out (cubic)."""
    u = clamp(u)
    if u < 0.5:
        return 4 * u * u * u
    v = -2 * u + 2
    return 1 - v * v * v / 2


def ease_out_back(u: float, overshoot: float = 1.7) -> float:
    """Arrives past 1 and settles back: overshoot for exaggeration."""
    u = clamp(u) - 1
    return 1 + (overshoot + 1) * u * u * u + overshoot * u * u


def spring(t: float, frequency: float = 2.0, damping: float = 0.3) -> float:
    """A spring released at t = 0 going from 0 to 1: it overshoots and
    rings ``frequency`` times a second, dying out with ``damping`` (0 rings
    forever, 1 barely overshoots). 0 before release."""
    if t <= 0:
        return 0.0
    omega = 2 * math.pi * frequency
    return 1 - math.exp(-damping * omega * t) * math.cos(omega * t)


def wobble(t: float, amount: float = 1.0, frequency: float = 3.0, decay: float = 4.0) -> float:
    """A shake started at t = 0 that fades: follow-through after a stop.
    0 before it starts."""
    if t <= 0:
        return 0.0
    return amount * math.exp(-decay * t) * math.sin(2 * math.pi * frequency * t)


def impact(t: float, amount: float = 0.35, frequency: float = 2.5, decay: float = 6.0) -> tuple:
    """Squash on landing at t = 0, bouncing back: returns ``(sx, sy)`` for
    ``pose``, keeping the area. (1, 1) before the landing.

    Example: ``sx, sy = impact(p.t - land)``
    """
    if t <= 0:
        return 1.0, 1.0
    squash = amount * math.exp(-decay * t) * math.cos(2 * math.pi * frequency * t)
    return 1 + squash, 1 / (1 + squash)


def anticipate(t: float, at: float, amount: float = 0.25, duration: float = 0.3) -> tuple:
    """Crouch before an action at ``at``: the body squashes over the
    ``duration`` before it and springs up to a stretch as it happens.
    Returns ``(sx, sy)``; (1, 1) away from the action.

    Example: ``sx, sy = anticipate(p.t, at=0.4)`` then launch at 0.4.
    """
    if t < at:
        crouch = amount * ease_in(progress(t, at - duration, duration))
        return 1 + crouch * 0.6, 1 - crouch
    stretch = amount * 0.8 * math.exp(-8 * (t - at)) * math.cos(2 * math.pi * 1.5 * (t - at))
    return 1 / (1 + stretch), 1 + stretch


def ballistic(t: float, x: float, y: float, vx: float, vy: float, gravity: float = 9.8) -> tuple:
    """Where a throw from (x, y) with velocity (vx, vy) is after ``t``
    seconds: an arc. Returns ``(x, y)``."""
    return x + vx * t, y + vy * t - gravity * t * t / 2


def landing(vy: float, drop: float, gravity: float = 9.8) -> float:
    """Seconds until a throw going up at ``vy`` falls ``drop`` below its
    start."""
    return (vy + math.sqrt(vy * vy + 2 * gravity * drop)) / gravity


def _fnv1a(text: str) -> int:
    value = 0x811C9DC5
    for byte in text.encode("utf-8"):
        value = ((value ^ byte) * 0x01000193) & 0xFFFFFFFF
    return value


_MASK = (1 << 64) - 1


def _random(seed: int, k: float) -> float:
    k = float(k)
    if k == 0.0:
        k = 0.0
    bits = struct.unpack("<Q", struct.pack("<d", k))[0]
    z = (((seed << 32) ^ bits) + 0x9E3779B97F4A7C15) & _MASK
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & _MASK
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & _MASK
    z ^= z >> 31
    return (z >> 11) / float(1 << 53)


@dataclass
class Player:
    """A player as a behavior sees it, for running a behavior in Python.

    While presenting, the engine fills these for every player every frame.
    """

    name: str = "Ana"
    t: float = 0.0
    time: float = 0.0
    joined: float = 0.0
    index: int = 0
    count: int = 1
    rank: int = 0
    score: float = 0.0
    leader: float = 0.0
    previous_rank: int = 0
    rank_since: float = 0.0
    previous_score: float = 0.0
    score_since: float = 0.0
    seed: int = field(default=-1)

    def __post_init__(self) -> None:
        if self.seed < 0:
            self.seed = _fnv1a(self.name)

    def random(self, k: float = 0) -> float:
        """A number in [0, 1) that depends on the player and ``k`` only:
        the same for the same player on every frame and every machine."""
        return _random(self.seed, k)


class BehaviorError(ValueError):
    """A behavior uses Python the compiler does not support.

    The message points at the line, like a ``SyntaxError``.
    """


# What calls compile to: math functions and builtins, found by identity so
# ``from math import sin`` and ``import math as m`` work too.
_UNARY_MATH = {
    math.sqrt: "sqrt",
    math.exp: "exp",
    math.expm1: "expm1",
    math.log1p: "log1p",
    math.log2: "log2",
    math.log10: "log10",
    math.sin: "sin",
    math.cos: "cos",
    math.tan: "tan",
    math.asin: "asin",
    math.acos: "acos",
    math.atan: "atan",
    math.sinh: "sinh",
    math.cosh: "cosh",
    math.tanh: "tanh",
    math.fabs: "abs",
    math.floor: "floor",
    math.ceil: "ceil",
    math.trunc: "trunc",
}
_BINARY_MATH = {
    math.atan2: "atan2",
    math.hypot: "hypot",
    math.fmod: "fmod",
    math.copysign: "copy_sign",
    math.pow: "pow",
}
_PREDICATES = {math.isnan: "is_nan", math.isinf: "is_inf", math.isfinite: "is_finite"}
_BINOPS = {
    ast.Add: "add",
    ast.Sub: "sub",
    ast.Mult: "mul",
    ast.Div: "div",
    ast.FloorDiv: "floor_div",
    ast.Mod: "mod",
    ast.Pow: "pow",
}
_COMPARE = {
    ast.Lt: "lt",
    ast.LtE: "le",
    ast.Gt: "gt",
    ast.GtE: "ge",
    ast.Eq: "eq",
    ast.NotEq: "ne",
}
_POSE_FIELDS = Pose._fields
_UNROLL_LIMIT = 256


class _Fail(Exception):
    def __init__(self, node: Optional[ast.AST], message: str) -> None:
        super().__init__(message)
        self.node = node
        self.message = message
        self.source: Optional[_Source] = None


@dataclass(frozen=True)
class _Value:
    """A value while compiling: a register with a kind, a tuple of values,
    a pose, the player, or a Python object known while compiling."""

    kind: str  # "num", "bool", "str", "none", "optstr", "tuple", "pose", "player", "object"
    reg: int = -1
    items: tuple = ()
    obj: Any = None


@dataclass
class _Source:
    function: Callable
    filename: str
    first_line: int
    lines: list
    tree: ast.FunctionDef
    names: dict  # frozen globals, builtins and closure cells


def _source_of(function: Callable, node: Optional[ast.AST] = None) -> _Source:
    if not inspect.isfunction(function):
        raise BehaviorError(f"a behavior must be a Python function, got {function!r}")
    if function.__name__ == "<lambda>":
        raise BehaviorError(
            "a behavior must be a function defined with def, not a lambda"
        )
    try:
        lines, first_line = inspect.getsourcelines(function)
    except (OSError, TypeError) as error:
        raise BehaviorError(
            f"cannot read the source of {function.__qualname__}: {error}. "
            "Behaviors must be defined in a file."
        ) from None
    text = textwrap.dedent("".join(lines))
    module = ast.parse(text)
    tree = module.body[0]
    if not isinstance(tree, ast.FunctionDef):
        raise BehaviorError(f"{function.__qualname__} is not a plain def function")
    names = dict(vars(builtins))
    names.update(function.__globals__)
    if function.__closure__:
        for name, cell in zip(function.__code__.co_freevars, function.__closure__):
            try:
                names[name] = cell.cell_contents
            except ValueError:
                pass
    return _Source(
        function=function,
        filename=inspect.getsourcefile(function) or "<unknown>",
        first_line=first_line,
        lines=text.splitlines(),
        tree=tree,
        names=names,
    )


class _Frame:
    """One inlined function: its locals and how it returns."""

    def __init__(self, source: _Source) -> None:
        self.source = source
        self.env: dict = {}
        self.done: Optional[int] = None  # register: some return already fired
        self.out: Optional[_Value] = None


class _Compiler:
    def __init__(self) -> None:
        self.code: list = []
        self.memo: dict = {}
        self.strings: list = []
        self.stack: list = []  # functions being inlined, for recursion
        self.frame: Optional[_Frame] = None

    # -- instructions -----------------------------------------------------

    def emit(self, op: str, *args: Any) -> int:
        key = (op, args)
        if key in self.memo:
            return self.memo[key]
        if op in ("const", "input"):
            inst = {op: args[0]}
        elif len(args) == 1:
            inst = {op: args[0]}
        else:
            inst = {op: list(args)}
        self.code.append(inst)
        self.memo[key] = len(self.code) - 1
        return len(self.code) - 1

    def const(self, value: float) -> int:
        value = float(value)
        text = repr(value)
        if value == 0.0 and math.copysign(1.0, value) < 0:
            text = "-0.0"
        return self.emit("const", text)

    def num(self, value: float) -> _Value:
        # Whole numbers stay known while compiling, for range().
        known = value if isinstance(value, int) and not isinstance(value, bool) else None
        return _Value("num", self.const(value), obj=known)

    def boolean(self, value: bool) -> _Value:
        return _Value("bool", self.const(1.0 if value else 0.0))

    def string(self, text: str) -> _Value:
        if text not in self.strings:
            self.strings.append(text)
        return _Value("str", self.const(self.strings.index(text)), obj=text)

    def none(self) -> _Value:
        return _Value("none", self.const(-1.0), obj=None)

    # -- errors -------------------------------------------------------------

    def fail(self, node: Optional[ast.AST], message: str) -> _Fail:
        return _Fail(node, message)

    def located(self, error: _Fail) -> BehaviorError:
        source = error.source or (self.frame.source if self.frame else None)
        node = error.node
        if source is None or node is None or not hasattr(node, "lineno"):
            return BehaviorError(error.message)
        line = node.lineno
        text = source.lines[line - 1] if line - 1 < len(source.lines) else ""
        column = getattr(node, "col_offset", 0)
        end = getattr(node, "end_col_offset", None)
        if getattr(node, "end_lineno", line) != line or end is None:
            end = len(text)
        width = max(1, end - column)
        return BehaviorError(
            f'File "{source.filename}", line {source.first_line + line - 1}, '
            f"in {source.function.__qualname__}\n"
            f"    {text}\n"
            f"    {' ' * column}{'^' * width}\n"
            f"{error.message}"
        )

    # -- values ---------------------------------------------------------------

    def scalar(self, value: _Value, node: ast.AST, what: str = "a number") -> int:
        if value.kind in ("num", "bool"):
            return value.reg
        raise self.fail(node, f"expected {what}, got {self.describe(value)}")

    def truth(self, value: _Value, node: ast.AST) -> int:
        if value.kind == "bool":
            return value.reg
        if value.kind == "num":
            return self.emit("truth", value.reg)
        if value.kind == "str":
            return self.const(0.0 if value.obj == "" else 1.0)
        if value.kind == "none":
            return self.const(0.0)
        if value.kind == "optstr":
            return self.emit("ge", value.reg, self.const(0.0))
        if value.kind == "tuple":
            return self.const(1.0 if value.items else 0.0)
        raise self.fail(node, f"cannot test the truth of {self.describe(value)}")

    def describe(self, value: _Value) -> str:
        return {
            "num": "a number",
            "bool": "a boolean",
            "str": "a string",
            "none": "None",
            "optstr": "a string or None",
            "tuple": "a tuple",
            "pose": "a pose",
            "player": "the player",
        }.get(value.kind, repr(value.obj))

    def select(self, cond: int, a: _Value, b: _Value, node: ast.AST) -> _Value:
        if a == b:
            return a
        for value in (a, b):
            if value.kind == "object" and isinstance(value.obj, _Unbound):
                raise self.fail(
                    node,
                    f"{value.obj.name!r} is assigned on some paths only; "
                    "give it a value before the if",
                )
        if a.kind == "tuple" or b.kind == "tuple":
            if a.kind != b.kind or len(a.items) != len(b.items):
                raise self.fail(
                    node,
                    f"the branches give {self.describe(a)} and {self.describe(b)}; "
                    "tuples must have the same length on every branch",
                )
            return _Value(
                "tuple",
                items=tuple(self.select(cond, x, y, node) for x, y in zip(a.items, b.items)),
            )
        if a.kind == "pose" or b.kind == "pose":
            if a.kind != b.kind:
                raise self.fail(
                    node,
                    f"one branch gives a pose and another {self.describe(b if a.kind == 'pose' else a)}",
                )
            return _Value(
                "pose",
                items=tuple(self.select(cond, x, y, node) for x, y in zip(a.items, b.items)),
            )
        kinds = {a.kind, b.kind}
        if kinds <= {"num", "bool"}:
            kind = "bool" if kinds == {"bool"} else "num"
        elif kinds <= {"str", "none", "optstr"}:
            kind = "str" if kinds == {"str"} else "optstr"
            if kinds == {"none"}:
                kind = "none"
        else:
            raise self.fail(
                node,
                f"one branch gives {self.describe(a)} and another {self.describe(b)}",
            )
        if a.kind == "object" or b.kind == "object":
            raise self.fail(node, "cannot choose between Python objects at run time")
        return _Value(kind, self.emit("select", cond, a.reg, b.reg))

    def lift(self, obj: Any, node: ast.AST) -> _Value:
        """A Python object the function refers to, frozen now."""
        if isinstance(obj, bool):
            return self.boolean(obj)
        if isinstance(obj, (int, float)):
            return self.num(obj)
        if isinstance(obj, str):
            return self.string(obj)
        if obj is None:
            return self.none()
        if isinstance(obj, (tuple, list)):
            return _Value("tuple", items=tuple(self.lift(item, node) for item in obj))
        return _Value("object", obj=obj)

    # -- functions ------------------------------------------------------------

    def function(
        self,
        function: Callable,
        args: list,
        node: Optional[ast.AST],
        keywords: Optional[dict] = None,
    ) -> _Value:
        if function in self.stack:
            raise self.fail(node, f"{function.__qualname__} calls itself; recursion is not supported")
        source = _source_of(function)
        tree = source.tree
        spec = tree.args
        if spec.vararg or spec.kwarg or spec.kwonlyargs or spec.posonlyargs:
            raise self.fail(
                node, f"{function.__qualname__} must take plain positional parameters"
            )
        params = [arg.arg for arg in spec.args]
        given_defaults = function.__defaults__ or ()
        defaults = dict(zip(params[len(params) - len(given_defaults):], given_defaults))
        keywords = keywords or {}
        if len(args) > len(params):
            raise self.fail(
                node,
                f"{function.__qualname__} takes {len(params)} arguments, got {len(args)}",
            )
        bound = dict(zip(params, args))
        for name, value in keywords.items():
            if name not in params:
                raise self.fail(node, f"{function.__qualname__} has no parameter {name!r}")
            if name in bound:
                raise self.fail(node, f"{function.__qualname__} got {name!r} twice")
            bound[name] = value
        caller = self.frame
        values = {}
        for name in params:
            if name in bound:
                values[name] = bound[name]
            elif name in defaults:
                values[name] = self.lift(defaults[name], tree)
            else:
                raise self.fail(node, f"{function.__qualname__} needs {name!r}")
        frame = _Frame(source)
        self.frame = frame
        frame.env.update(values)
        self.stack.append(function)
        try:
            if not self.block(tree.body, None):
                raise self.fail(
                    tree,
                    f"{function.__qualname__} must end in return on every path",
                )
            return frame.out
        except _Fail as error:
            if error.source is None:
                error.source = source
            raise
        finally:
            self.stack.pop()
            self.frame = caller

    # -- statements -------------------------------------------------------------

    def guarded(self, guard: Optional[int]) -> Optional[int]:
        """The guard of a return here: the path is taken and nothing
        returned before."""
        frame = self.frame
        if frame.done is None:
            return guard
        pending = self.emit("not", frame.done)
        return pending if guard is None else self.emit("and", guard, pending)

    def ret(self, value: _Value, guard: Optional[int], node: ast.AST) -> None:
        frame = self.frame
        effective = self.guarded(guard)
        if frame.out is None or effective is None:
            frame.out = value
        else:
            frame.out = self.select(effective, value, frame.out, node)
        if effective is None:
            frame.done = self.const(1.0)
        elif frame.done is None:
            frame.done = effective
        else:
            frame.done = self.emit("or", frame.done, effective)

    def block(self, body: list, guard: Optional[int]) -> bool:
        """Compile statements; whether they return on every path."""
        for statement in body:
            if self.statement(statement, guard):
                return True
        return False

    def assign(self, target: ast.AST, value: _Value) -> None:
        if isinstance(target, ast.Name):
            self.frame.env[target.id] = value
            return
        if isinstance(target, (ast.Tuple, ast.List)):
            if value.kind != "tuple" or len(value.items) != len(target.elts):
                raise self.fail(
                    target,
                    f"cannot unpack {self.describe(value)} into {len(target.elts)} names",
                )
            for element, item in zip(target.elts, value.items):
                self.assign(element, item)
            return
        raise self.fail(target, "only names and tuples of names can be assigned")

    def statement(self, node: ast.stmt, guard: Optional[int]) -> bool:
        if isinstance(node, ast.Return):
            if node.value is None:
                raise self.fail(node, "return a value")
            self.ret(self.expr(node.value), guard, node)
            # The rest of this block never runs; the guard already says
            # when this return does.
            return True
        if isinstance(node, ast.Assign):
            value = self.expr(node.value)
            for target in node.targets:
                self.assign(target, value)
            return False
        if isinstance(node, ast.AnnAssign):
            if node.value is not None:
                self.assign(node.target, self.expr(node.value))
            return False
        if isinstance(node, ast.AugAssign):
            if not isinstance(node.target, ast.Name):
                raise self.fail(node.target, "only names can be updated")
            current = self.name(node.target.id, node.target)
            value = self.binop(node.op, current, self.expr(node.value), node)
            self.frame.env[node.target.id] = value
            return False
        if isinstance(node, ast.If):
            return self.branch(node, guard)
        if isinstance(node, ast.For):
            return self.loop(node, guard)
        if isinstance(node, ast.Pass):
            return False
        if isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant):
            return False  # a docstring
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
            raise self.fail(node, "define helper functions at module level, outside the behavior")
        if isinstance(node, ast.While):
            raise self.fail(node, "while loops are not supported; use for ... in range(N) with a fixed N")
        if isinstance(node, (ast.Global, ast.Nonlocal)):
            raise self.fail(node, "behaviors cannot change globals")
        raise self.fail(node, f"{type(node).__name__} statements are not supported in behaviors")

    def branch(self, node: ast.If, guard: Optional[int]) -> bool:
        cond = self.truth(self.expr(node.test), node.test)
        not_cond = self.emit("not", cond)
        then_guard = cond if guard is None else self.emit("and", guard, cond)
        else_guard = not_cond if guard is None else self.emit("and", guard, not_cond)
        before = dict(self.frame.env)
        then_returns = self.block(node.body, then_guard)
        then_env = self.frame.env
        self.frame.env = dict(before)
        else_returns = self.block(node.orelse, else_guard)
        else_env = self.frame.env
        if then_returns and else_returns:
            self.frame.env = before
            return True
        if then_returns:
            self.frame.env = else_env
            return False
        if else_returns:
            self.frame.env = then_env
            return False
        merged = {}
        for name in set(then_env) | set(else_env):
            if name in then_env and name in else_env:
                merged[name] = self.select(cond, then_env[name], else_env[name], node)
            else:
                # Assigned on one path only: reading it later is an error.
                merged[name] = _Value("object", obj=_Unbound(name))
        self.frame.env = merged
        return False

    def loop(self, node: ast.For, guard: Optional[int]) -> bool:
        if node.orelse:
            raise self.fail(node, "for ... else is not supported")
        iterable = node.iter
        if not (
            isinstance(iterable, ast.Call)
            and isinstance(iterable.func, ast.Name)
            and self.lookup(iterable.func.id, iterable.func) is range
        ):
            raise self.fail(iterable, "loops must be for ... in range(N) with a fixed N")
        bounds = [self.static_int(arg) for arg in iterable.args]
        steps = range(*bounds)
        if len(steps) > _UNROLL_LIMIT:
            raise self.fail(iterable, f"loops unroll at most {_UNROLL_LIMIT} times")
        for index in steps:
            self.assign(node.target, self.num(index))
            for statement in node.body:
                if isinstance(statement, (ast.Break, ast.Continue)):
                    raise self.fail(statement, "break and continue are not supported")
                if self.statement(statement, guard):
                    return True
        return False

    def static_int(self, node: ast.AST) -> int:
        """An int known while compiling, for range()."""
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.Name):
            value = self.frame.env.get(node.id)
            if value is None:
                obj = self.lookup(node.id, node)
                if isinstance(obj, int) and not isinstance(obj, bool):
                    return obj
            elif value.kind == "num" and isinstance(value.obj, int):
                return value.obj
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            return -self.static_int(node.operand)
        if isinstance(node, ast.BinOp) and type(node.op) in (ast.Add, ast.Sub, ast.Mult, ast.FloorDiv):
            a, b = self.static_int(node.left), self.static_int(node.right)
            return {ast.Add: a + b, ast.Sub: a - b, ast.Mult: a * b}.get(type(node.op), a // b if b else 0)
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and self.lookup(node.func.id, node.func) is len:
            if len(node.args) == 1:
                value = self.expr(node.args[0])
                if value.kind == "tuple":
                    return len(value.items)
        raise self.fail(node, "range() needs a whole number known before the scene runs")

    # -- expressions --------------------------------------------------------------

    def lookup(self, name: str, node: ast.AST) -> Any:
        names = self.frame.source.names
        if name not in names:
            raise self.fail(node, f"name {name!r} is not defined")
        return names[name]

    def name(self, name: str, node: ast.AST) -> _Value:
        if name in self.frame.env:
            value = self.frame.env[name]
            if value.kind == "object" and isinstance(value.obj, _Unbound):
                raise self.fail(
                    node,
                    f"{name!r} is assigned on some paths only; give it a value before the if",
                )
            return value
        return self.lift(self.lookup(name, node), node)

    def expr(self, node: ast.AST) -> _Value:
        if isinstance(node, ast.Constant):
            value = node.value
            if isinstance(value, (bool, int, float, str)) or value is None:
                return self.lift(value, node)
            raise self.fail(node, f"constants of type {type(value).__name__} are not supported")
        if isinstance(node, ast.Name):
            return self.name(node.id, node)
        if isinstance(node, ast.Tuple):
            return _Value("tuple", items=tuple(self.expr(element) for element in node.elts))
        if isinstance(node, ast.List):
            return _Value("tuple", items=tuple(self.expr(element) for element in node.elts))
        if isinstance(node, ast.Attribute):
            return self.attribute(node)
        if isinstance(node, ast.BinOp):
            return self.binop(node.op, self.expr(node.left), self.expr(node.right), node)
        if isinstance(node, ast.UnaryOp):
            operand = self.expr(node.operand)
            if isinstance(node.op, ast.Not):
                return _Value("bool", self.emit("not", self.truth(operand, node.operand)))
            if isinstance(node.op, ast.USub):
                return _Value("num", self.emit("neg", self.scalar(operand, node.operand)))
            if isinstance(node.op, ast.UAdd):
                return _Value("num", self.scalar(operand, node.operand))
            raise self.fail(node, "bitwise operators are not supported")
        if isinstance(node, ast.BoolOp):
            # Python's `a and b` is `b if a else a`; `a or b` is `a if a else b`.
            result = self.expr(node.values[0])
            for operand_node in node.values[1:]:
                operand = self.expr(operand_node)
                cond = self.truth(result, operand_node)
                if isinstance(node.op, ast.And):
                    result = self.select(cond, operand, result, node)
                else:
                    result = self.select(cond, result, operand, node)
            return result
        if isinstance(node, ast.Compare):
            return self.compare(node)
        if isinstance(node, ast.IfExp):
            cond = self.truth(self.expr(node.test), node.test)
            return self.select(cond, self.expr(node.body), self.expr(node.orelse), node)
        if isinstance(node, ast.Call):
            return self.call(node)
        if isinstance(node, ast.Subscript):
            return self.subscript(node)
        if isinstance(node, ast.NamedExpr):
            value = self.expr(node.value)
            self.assign(node.target, value)
            return value
        if isinstance(node, ast.Lambda):
            raise self.fail(node, "lambdas are not supported; define a helper function at module level")
        if isinstance(node, (ast.ListComp, ast.GeneratorExp, ast.SetComp, ast.DictComp)):
            raise self.fail(node, "comprehensions are not supported; use for ... in range(N)")
        if isinstance(node, (ast.Dict, ast.Set)):
            raise self.fail(node, "dicts and sets are not supported")
        if isinstance(node, ast.JoinedStr):
            raise self.fail(node, "f-strings are not supported; behaviors return poses, not text")
        raise self.fail(node, f"{type(node).__name__} is not supported in behaviors")

    def attribute(self, node: ast.Attribute) -> _Value:
        base = self.expr(node.value)
        if base.kind == "player":
            if node.attr in PLAYER_FIELDS:
                return _Value("num", self.emit("input", node.attr))
            if node.attr == "random":
                raise self.fail(node, "call p.random(k) with a number k")
            if node.attr in ("name", "seed"):
                raise self.fail(
                    node,
                    f"p.{node.attr} is not available; p.random(k) gives numbers read from the player",
                )
            raise self.fail(
                node,
                f"players have no {node.attr!r}; they have {', '.join(PLAYER_FIELDS)} and random(k)",
            )
        if base.kind == "pose":
            if node.attr in _POSE_FIELDS:
                return base.items[_POSE_FIELDS.index(node.attr)]
            raise self.fail(node, f"poses have no {node.attr!r}")
        if base.kind == "object":
            try:
                obj = getattr(base.obj, node.attr)
            except AttributeError:
                raise self.fail(node, f"{base.obj!r} has no attribute {node.attr!r}") from None
            return self.lift(obj, node)
        raise self.fail(node, f"{self.describe(base)} has no attribute {node.attr!r}")

    def subscript(self, node: ast.Subscript) -> _Value:
        base = self.expr(node.value)
        if base.kind != "tuple":
            raise self.fail(node.value, f"cannot index {self.describe(base)}")
        if isinstance(node.slice, ast.Slice):
            raise self.fail(node.slice, "slices are not supported")
        items = base.items
        try:
            index = self.static_int(node.slice)
        except _Fail:
            index = None
        if index is not None:
            if not -len(items) <= index < len(items):
                raise self.fail(node.slice, f"index {index} is out of range for {len(items)} items")
            return items[index]
        # An index known only while presenting: choose among the items.
        position = self.scalar(self.expr(node.slice), node.slice, "an index")
        if not items:
            raise self.fail(node, "cannot index an empty tuple")
        result = items[-1]
        for index in range(len(items) - 2, -1, -1):
            cond = self.emit("eq", position, self.const(index))
            result = self.select(cond, items[index], result, node)
        return result

    def binop(self, op: ast.operator, left: _Value, right: _Value, node: ast.AST) -> _Value:
        name = _BINOPS.get(type(op))
        if name is None:
            raise self.fail(node, f"the {type(op).__name__} operator is not supported")
        if left.kind == "tuple" or right.kind == "tuple":
            raise self.fail(node, "tuples do not support arithmetic; work on their items")
        a = self.scalar(left, node, "a number")
        b = self.scalar(right, node, "a number")
        return _Value("num", self.emit(name, a, b))

    def compare(self, node: ast.Compare) -> _Value:
        left = self.expr(node.left)
        result = None
        for op, right_node in zip(node.ops, node.comparators):
            right = self.expr(right_node)
            if isinstance(op, (ast.Is, ast.IsNot)):
                if not (isinstance(right_node, ast.Constant) and right_node.value is None):
                    raise self.fail(node, "is and is not only compare with None")
                test = "ge" if isinstance(op, ast.IsNot) else "lt"
                if left.kind == "none":
                    reg = self.const(0.0 if test == "ge" else 1.0)
                elif left.kind in ("str",):
                    reg = self.const(1.0 if test == "ge" else 0.0)
                elif left.kind == "optstr":
                    reg = self.emit(test, left.reg, self.const(0.0))
                else:
                    reg = self.const(1.0 if test == "ge" else 0.0)
            elif isinstance(op, (ast.In, ast.NotIn)):
                if right.kind != "tuple":
                    raise self.fail(right_node, "in needs a tuple")
                reg = self.const(0.0)
                for item in right.items:
                    reg = self.emit("or", reg, self.equal(left, item, node))
                if isinstance(op, ast.NotIn):
                    reg = self.emit("not", reg)
            else:
                name = _COMPARE[type(op)]
                if name in ("eq", "ne"):
                    reg = self.equal(left, right, node)
                    if name == "ne":
                        reg = self.emit("not", reg)
                else:
                    reg = self.emit(name, self.scalar(left, node), self.scalar(right, node))
            result = reg if result is None else self.emit("and", result, reg)
            left = right
        return _Value("bool", result)

    def equal(self, left: _Value, right: _Value, node: ast.AST) -> int:
        texts = {"str", "none", "optstr"}
        numbers = {"num", "bool"}
        if left.kind in numbers and right.kind in numbers:
            return self.emit("eq", left.reg, right.reg)
        if left.kind in texts and right.kind in texts:
            return self.emit("eq", left.reg, right.reg)
        if left.kind == "tuple" and right.kind == "tuple":
            if len(left.items) != len(right.items):
                return self.const(0.0)
            reg = self.const(1.0)
            for a, b in zip(left.items, right.items):
                reg = self.emit("and", reg, self.equal(a, b, node))
            return reg
        if {left.kind, right.kind} <= numbers | texts:
            return self.const(0.0)
        raise self.fail(node, f"cannot compare {self.describe(left)} with {self.describe(right)}")

    def call(self, node: ast.Call) -> _Value:
        func = node.func
        # p.random(k)
        if isinstance(func, ast.Attribute):
            base = self.expr(func.value)
            if base.kind == "player":
                if func.attr != "random":
                    raise self.fail(func, f"players have no method {func.attr!r}; they have random(k)")
                if node.keywords or len(node.args) > 1:
                    raise self.fail(node, "p.random takes one number")
                k = self.scalar(self.expr(node.args[0]), node.args[0]) if node.args else self.const(0.0)
                return _Value("num", self.emit("random", k))
            target = self.attribute(func)
        else:
            target = self.expr(func)
        if target.kind != "object":
            raise self.fail(func, f"{self.describe(target)} cannot be called")
        callee = target.obj
        if any(keyword.arg is None for keyword in node.keywords):
            raise self.fail(node, "**kwargs is not supported")
        if any(isinstance(arg, ast.Starred) for arg in node.args):
            raise self.fail(node, "*args is not supported")
        args = [self.expr(arg) for arg in node.args]
        keywords = {keyword.arg: self.expr(keyword.value) for keyword in node.keywords}
        if callee is pose or callee is Pose:
            return self.pose(node, args, keywords)
        if inspect.isfunction(callee):
            return self.function(callee, args, node, keywords)
        if keywords:
            raise self.fail(node, f"{getattr(callee, '__name__', callee)} takes no keywords here")
        return self.builtin(node, callee, args)

    def builtin(self, node: ast.Call, callee: Any, args: list) -> _Value:
        label = getattr(callee, "__name__", repr(callee))

        def arity(*counts: int) -> None:
            if len(args) not in counts:
                expected = " or ".join(str(count) for count in counts)
                raise self.fail(node, f"{label}() takes {expected} arguments, got {len(args)}")

        def reg(index: int) -> int:
            return self.scalar(args[index], node.args[index])

        if callee in _UNARY_MATH:
            arity(1)
            return _Value("num", self.emit(_UNARY_MATH[callee], reg(0)))
        if callee in _BINARY_MATH:
            arity(2)
            return _Value("num", self.emit(_BINARY_MATH[callee], reg(0), reg(1)))
        if callee in _PREDICATES:
            arity(1)
            return _Value("bool", self.emit(_PREDICATES[callee], reg(0)))
        if callee is math.log:
            arity(1, 2)
            ln = self.emit("ln", reg(0))
            if len(args) == 1:
                return _Value("num", ln)
            return _Value("num", self.emit("div", ln, self.emit("ln", reg(1))))
        if callee is math.radians:
            arity(1)
            return _Value("num", self.emit("mul", reg(0), self.const(math.pi / 180.0)))
        if callee is math.degrees:
            arity(1)
            return _Value("num", self.emit("mul", reg(0), self.const(180.0 / math.pi)))
        if callee is abs:
            arity(1)
            return _Value("num", self.emit("abs", reg(0)))
        if callee is round:
            if len(args) == 2:
                raise self.fail(node, "round(x, digits) is not supported; use round(x * 100) / 100")
            arity(1)
            return _Value("num", self.emit("round", reg(0)))
        if callee is int:
            arity(1)
            return _Value("num", self.emit("trunc", reg(0)))
        if callee is float:
            arity(1)
            return _Value("num", reg(0))
        if callee is bool:
            arity(1)
            return _Value("bool", self.truth(args[0], node.args[0]))
        if callee is len:
            arity(1)
            if args[0].kind != "tuple":
                raise self.fail(node, f"len() needs a tuple, got {self.describe(args[0])}")
            return self.num(len(args[0].items))
        if callee in (min, max):
            values = args
            if len(args) == 1 and args[0].kind == "tuple":
                values = list(args[0].items)
            if not values:
                raise self.fail(node, f"{label}() needs at least one value")
            op = "max" if callee is max else "min"
            result = self.scalar(values[0], node)
            for value in values[1:]:
                result = self.emit(op, result, self.scalar(value, node))
            return _Value("num", result)
        if callee is sum:
            arity(1)
            if args[0].kind != "tuple":
                raise self.fail(node, "sum() needs a tuple")
            result = self.const(0.0)
            for value in args[0].items:
                result = self.emit("add", result, self.scalar(value, node))
            return _Value("num", result)
        if inspect.isfunction(callee):
            return self.function(callee, args, node)
        if inspect.ismodule(callee) or callee is math:
            raise self.fail(node, f"module {label} cannot be called")
        raise self.fail(
            node,
            f"{label}() is not supported in behaviors; use math functions, "
            "abs, min, max, round, int, float, bool, len, sum or your own functions",
        )

    def pose(self, node: ast.Call, args: list, keywords: dict) -> _Value:
        if len(args) > 2:
            raise self.fail(node, "pose() takes x and y by position and the rest by name")
        unknown = [name for name in keywords if name not in _POSE_FIELDS]
        if unknown:
            raise self.fail(
                node,
                f"pose() has no {unknown[0]!r}; it takes {', '.join(_POSE_FIELDS)}",
            )
        given = dict(zip(("x", "y"), args))
        for name in keywords:
            if name in given:
                raise self.fail(node, f"pose() got {name!r} twice")
        given.update(keywords)
        for name in ("x", "y"):
            if name not in given:
                raise self.fail(node, f"pose() needs {name}")
        defaults = Pose(0.0, 0.0)
        items = []
        for name in _POSE_FIELDS:
            value = given.get(name)
            if value is None:
                default = getattr(defaults, name)
                value = self.none() if default is None else self.lift(default, node)
            if name == "express":
                if value.kind not in ("str", "none", "optstr"):
                    raise self.fail(node, f"express must be an expression name or None, got {self.describe(value)}")
                if value.kind == "none":
                    value = _Value("optstr", value.reg)
            elif name == "since":
                if value.kind == "none":
                    value = _Value("num", self.const(math.nan))
                elif value.kind not in ("num", "bool"):
                    raise self.fail(node, f"since must be a number or None, got {self.describe(value)}")
            elif name in ("flip", "visible", "loop"):
                value = _Value("bool", self.truth(value, node))
            else:
                value = _Value("num", self.scalar(value, node, f"{name} as a number"))
            items.append(value)
        return _Value("pose", items=tuple(items))

    # -- entry --------------------------------------------------------------------

    def compile(self, function: Callable) -> dict:
        source = _source_of(function)
        params = source.tree.args
        if (
            len(params.args) != 1
            or params.vararg
            or params.kwarg
            or params.kwonlyargs
            or params.posonlyargs
        ):
            raise BehaviorError(
                f"{function.__qualname__} must take one parameter, the player: def {function.__name__}(p):"
            )
        try:
            result = self.function(function, [_Value("player")], None)
        except _Fail as error:
            raise self.located(error) from None
        if result is None or result.kind != "pose":
            self.frame = _Frame(source)
            raise self.located(
                _Fail(source.tree, f"{function.__qualname__} must return pose(...), got {self.describe(result) if result else 'nothing'}")
            )
        regs = dict(zip(_POSE_FIELDS, (item.reg for item in result.items)))
        return {
            "version": list(PROGRAM_VERSION),
            "name": function.__qualname__,
            "strings": list(self.strings),
            "code": self.code,
            "pose": regs,
        }


@dataclass(frozen=True)
class _Unbound:
    name: str


def compile_behavior(function: Callable) -> str:
    """Compile a behavior to the JSON program Rust runs.

    Raises :class:`BehaviorError`, pointing at the line, for Python the
    compiler does not support.
    """
    compiler = _Compiler()
    try:
        program = compiler.compile(function)
    except _Fail as error:  # raised outside any frame
        raise compiler.located(error) from None
    return json.dumps(program, separators=(",", ":"))
