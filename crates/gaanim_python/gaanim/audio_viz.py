"""Audio visualizers built from signals: ``scene.viz.equalizer``.

An equalizer draws one element per value (a spectrum band, a point of a
waveform, any signal from 0 to 1) and lets each part be chosen or replaced:
the shape of the elements, how they are laid out, and how they are
colored. Every element is an ordinary drawable in the returned group, so
it can also be styled, animated or given effects afterwards.
"""

from __future__ import annotations

import math
from typing import Any, Callable, Sequence

_SHAPES = ("bar", "capsule", "dot", "line", "area")
_LAYOUTS = ("row", "mirror", "radial")
_DEFAULT_FILL = "#38bdf8"
_CAP_POINTS = 6


class _Slot:
    """Where element ``index`` grows from and in which direction."""

    __slots__ = ("x", "y", "angle", "centered")

    def __init__(self, x: float, y: float, angle: float, centered: bool) -> None:
        self.x, self.y, self.angle, self.centered = float(x), float(y), float(angle), centered

    @property
    def direction(self) -> tuple[float, float]:
        return math.cos(self.angle), math.sin(self.angle)

    @property
    def across(self) -> tuple[float, float]:
        ux, uy = self.direction
        return -uy, ux


def _positive(value: Any, name: str) -> float:
    number = float(value)
    if not math.isfinite(number) or number <= 0.0:
        raise ValueError(f"{name} must be a positive number")
    return number


def _slots(layout: Any, count: int, center: tuple[float, float], width: float,
           height: float, radius: float, start_angle: float) -> tuple[list[_Slot], float]:
    """The slot of every element and the room each one has across."""
    cx, cy = center
    if callable(layout):
        slots = []
        for index in range(count):
            placed = layout(index, count)
            if not (isinstance(placed, (tuple, list)) and len(placed) == 3):
                raise TypeError("a layout callable returns (x, y, angle)")
            slots.append(_Slot(*placed, centered=False))
        if count > 1:
            room = min(math.dist((a.x, a.y), (b.x, b.y)) for a, b in zip(slots, slots[1:]))
        else:
            room = width
        return slots, room
    if layout == "radial":
        room = 2.0 * math.pi * radius / count
        slots = []
        for index in range(count):
            angle = start_angle - 2.0 * math.pi * index / count
            slots.append(_Slot(cx + radius * math.cos(angle), cy + radius * math.sin(angle), angle, False))
        return slots, room
    room = width / count
    left = cx - width / 2.0 + room / 2.0
    if layout == "mirror":
        return [_Slot(left + room * index, cy, math.pi / 2.0, True) for index in range(count)], room
    base = cy - height / 2.0
    return [_Slot(left + room * index, base, math.pi / 2.0, False) for index in range(count)], room


def _paint_for(fill: Any, index: int, count: int) -> Any:
    if callable(fill):
        return fill(index, count)
    if isinstance(fill, (list, tuple)):
        if not fill:
            raise ValueError("fill needs at least one paint")
        return fill[index % len(fill)]
    return fill


def _build_equalizer(scene: Any, values: Any, **options: Any) -> Any:
    from gaanim import Falloff, FalloffColor, computed

    known = {"shape", "layout", "center", "width", "height", "radius", "gap",
             "thickness", "min_length", "fill", "value_colors", "start_angle", "stretch"}
    unknown = set(options) - known
    if unknown:
        raise TypeError(f"equalizer() got unexpected options: {', '.join(sorted(unknown))}")
    if isinstance(values, (str, bytes)) or not isinstance(values, Sequence):
        raise TypeError("values must be a sequence of signals, such as Audio.spectrum()")
    values = list(values)
    count = len(values)
    if count == 0:
        raise ValueError("values must not be empty")

    shape = options.get("shape", "bar")
    layout = options.get("layout", "row")
    if not callable(shape) and shape not in _SHAPES:
        raise ValueError(f"shape must be one of {', '.join(_SHAPES)} or a callable")
    if not callable(layout) and layout not in _LAYOUTS:
        raise ValueError(f"layout must be one of {', '.join(_LAYOUTS)} or a callable")
    center = options.get("center", (0.0, 0.0))
    center = (float(center[0]), float(center[1]))
    width = _positive(options.get("width", 8.0), "width")
    height = _positive(options.get("height", 2.0), "height")
    radius = _positive(options.get("radius", 1.5), "radius")
    gap = float(options.get("gap", 0.25))
    if not 0.0 <= gap < 1.0:
        raise ValueError("gap must be in [0, 1)")
    min_length = float(options.get("min_length", 0.04))
    if not 0.0 <= min_length < height:
        raise ValueError("min_length must be in [0, height)")
    start_angle = float(options.get("start_angle", math.pi / 2.0))
    fill = options.get("fill")
    if fill is None and not callable(shape):
        fill = _DEFAULT_FILL
    # A Falloff ramp colors the whole group by member; anything else colors
    # each member.
    ramp = isinstance(fill, FalloffColor)
    value_colors = options.get("value_colors")
    if value_colors is not None and len(value_colors) < 2:
        raise ValueError("value_colors needs at least two colors")

    slots, room = _slots(layout, count, center, width, height, radius, start_angle)
    thickness = options.get("thickness")
    thickness = room * (1.0 - gap) if thickness is None else _positive(thickness, "thickness")

    span = height - min_length

    def length_of(value: Any) -> Any:
        # Values are read from 0 to 1; anything outside is clamped.
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            return min_length + span * min(1.0, max(0.0, float(value)))
        return computed(lambda v: min_length + span * min(1.0, max(0.0, v)), inputs=[value])

    lengths = [length_of(value) for value in values]

    def point(slot: _Slot, length: Any, along: float, dx: float = 0.0, dy: float = 0.0) -> tuple[Any, Any]:
        """``slot``'s point ``along`` its length, moved by ``(dx, dy)``."""
        ux, uy = slot.direction
        start = -0.5 if slot.centered else 0.0
        if isinstance(length, float):
            return (slot.x + ux * length * (start + along) + dx,
                    slot.y + uy * length * (start + along) + dy)
        return (
            computed(lambda l, s=slot: s.x + ux * l * (start + along) + dx, inputs=[length]),
            computed(lambda l, s=slot: s.y + uy * l * (start + along) + dy, inputs=[length]),
        )

    def outline(slot: _Slot, length: Any) -> list[Any]:
        """Corners of a bar, or of a capsule with half-round ends."""
        (ux, uy), (px, py) = slot.direction, slot.across
        half = thickness / 2.0
        if shape == "bar":
            return [point(slot, length, along, px * side * half, py * side * half)
                    for along, side in ((0.0, -1.0), (1.0, -1.0), (1.0, 1.0), (0.0, 1.0))]
        corners = []
        for along, sign in ((1.0, 1.0), (0.0, -1.0)):
            # Half circles across the thickness, over the tip and under the base.
            for step in range(_CAP_POINTS + 1):
                turn = math.pi * step / _CAP_POINTS
                across, out = sign * math.cos(turn) * half, sign * math.sin(turn) * half
                corners.append(point(slot, length, along, px * across + ux * out, py * across + uy * out))
        return corners

    members: list[Any] = []
    if shape in ("line", "area"):
        tips = [point(slot, length, 1.0) for slot, length in zip(slots, lengths)]
        closed_loop = layout == "radial"
        if any(slot.centered for slot in slots):
            # A mirrored outline runs along the tops and back along the bottoms.
            bottoms = [point(slot, length, 0.0) for slot, length in zip(slots, lengths)]
            outline = tips + bottoms[::-1]
            closed = True
        elif shape == "area" and not closed_loop:
            first, last = slots[0], slots[-1]
            outline = [(first.x, first.y)] + tips + [(last.x, last.y)]
            closed = True
        else:
            outline, closed = tips, closed_loop
        drawable = scene.geometry.polyline(outline, closed=closed)
        if shape == "line":
            paint = _DEFAULT_FILL if ramp or fill is None else _paint_for(fill, 0, 1)
            drawable.no_fill().stroke(paint, max(0.02, thickness * 0.35))
        else:
            drawable.fill(_DEFAULT_FILL if ramp or fill is None else _paint_for(fill, 0, 1)).no_stroke()
        members.append(drawable)
    else:
        for index, (slot, length, value) in enumerate(zip(slots, lengths, values)):
            if callable(shape):
                drawable = _custom_member(shape, scene, index, count, slot, length, computed,
                                          bool(options.get("stretch", True)))
            elif shape == "dot":
                drawable = scene.geometry.circle(thickness / 2.0).no_stroke()
                drawable.move_to(*point(slot, length, 1.0))
            else:
                corners = outline(slot, length)
                drawable = scene.geometry.polyline(corners, closed=True).no_stroke()
            if ramp:
                # The ramp recolors a fill the member must already have.
                drawable.fill(_DEFAULT_FILL)
            elif fill is not None:
                drawable.fill(_paint_for(fill, index, count))
            if value_colors is not None and not isinstance(value, (int, float)):
                drawable.drive("fill", Falloff.source(value).gradient(*value_colors))
            members.append(drawable)

    group = scene.geometry.group(members)
    if ramp and value_colors is None:
        group.drive("fill", fill)
    return group


def _custom_member(shape: Callable[..., Any], scene: Any, index: int, count: int,
                   slot: _Slot, length: Any, computed: Callable[..., Any], stretch: bool) -> Any:
    """Place a user drawable at the middle of its slot, along its direction,
    stretched to the slot's length, or scaled whole with it."""
    drawable = shape(scene, index, count)
    if drawable is None:
        raise TypeError("a shape callable returns a drawable")
    bounds = drawable.bounds()
    natural = bounds.top - bounds.bottom
    if natural <= 1e-9:
        raise ValueError("a shape callable must return a drawable with some height")
    (ux, uy) = slot.direction
    start = -0.5 if slot.centered else 0.0
    drawable.rotate_to(slot.angle - math.pi / 2.0)
    if isinstance(length, float):
        factor = length / natural
        middle = (slot.x + ux * length * (start + 0.5), slot.y + uy * length * (start + 0.5))
    else:
        factor = computed(lambda l: l / natural, inputs=[length])
        middle = (
            computed(lambda l: slot.x + ux * l * (start + 0.5), inputs=[length]),
            computed(lambda l: slot.y + uy * l * (start + 0.5), inputs=[length]),
        )
    if stretch:
        drawable.scale_to_3d(1.0, factor, 1.0)
    else:
        drawable.scale_to(factor)
    drawable.move_to(*middle)
    return drawable
