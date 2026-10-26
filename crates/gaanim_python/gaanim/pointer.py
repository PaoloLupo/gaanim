"""A mouse pointer for software demonstrations: ``scene.ui.pointer``.

The pointer is an ordinary group drawable whose pivot is the tip of the
cursor, so it moves, fades and scales like any other drawable. Its
``animate`` proxy adds the moves a demonstration is made of: ``to`` a point,
an object or a zone, ``click`` (a ripple at the tip and a small press) and
``drag`` (an object travels with the pointer). Each returns something
``scene.play`` takes. The pointer keeps track of where its tip is, so a
click always leaves from it.
"""

from __future__ import annotations

import math
from typing import Any, Literal, Sequence

from .gaanim_core import Anim, Composition, Drawable, parallel, sequence

_STYLES = ("arrow", "hand", "text")
_ANCHORS = {
    "center": (0.5, 0.5),
    "top": (0.5, 1.0),
    "bottom": (0.5, 0.0),
    "left": (0.0, 0.5),
    "right": (1.0, 0.5),
    "top_left": (0.0, 1.0),
    "top_right": (1.0, 1.0),
    "bottom_left": (0.0, 0.0),
    "bottom_right": (1.0, 0.0),
}
# Outlines drawn from the hot spot (0, 0), y up, about 0.35 units tall.
_SHAPES: dict[str, list[tuple[float, float]]] = {
    "arrow": [
        (0.0, 0.0), (0.0, -0.3), (0.072, -0.232), (0.118, -0.335),
        (0.158, -0.318), (0.113, -0.217), (0.205, -0.212),
    ],
    "hand": [
        (-0.035, 0.0), (0.035, 0.0), (0.045, -0.2), (0.075, -0.205), (0.105, -0.225),
        (0.135, -0.23), (0.165, -0.255), (0.175, -0.36), (0.12, -0.44),
        (-0.07, -0.44), (-0.13, -0.33), (-0.1, -0.3), (-0.05, -0.33), (-0.04, -0.2),
    ],
    "text": [
        (-0.06, 0.17), (0.06, 0.17), (0.06, 0.145), (0.012, 0.145), (0.012, -0.145),
        (0.06, -0.145), (0.06, -0.17), (-0.06, -0.17), (-0.06, -0.145),
        (-0.012, -0.145), (-0.012, 0.145), (-0.06, 0.145),
    ],
}
_PRESS = 0.88
_CLICK = 0.44


def _positive(value: Any, name: str) -> float:
    number = float(value)
    if not math.isfinite(number) or number <= 0.0:
        raise ValueError(f"{name} must be a positive number")
    return number


def _point(values: Sequence[Any], name: str) -> tuple[float, ...]:
    if len(values) not in (2, 3):
        raise ValueError(f"{name} takes two or three coordinates")
    numbers = tuple(float(value) for value in values)
    if not all(math.isfinite(number) for number in numbers):
        raise ValueError(f"{name} must be finite")
    return numbers


class Pointer:
    """A mouse pointer whose tip is the pivot of its ``drawable``.

    A pointer made at a 2D point moves between points, objects and zones of
    the plane; one made at a 3D point (``at=(x, y, z)``) faces the camera as
    a billboard and moves between 3D points.
    """

    def __init__(self, scene: Any, drawable: Drawable, at: tuple[float, ...]) -> None:
        self._scene = scene
        self._drawable = drawable
        self._where = at

    @property
    def drawable(self) -> Drawable:
        """The group that draws the pointer: fade it in, scale it or style it as any drawable."""
        return self._drawable

    @property
    def position(self) -> tuple[float, ...]:
        """Where the tip is now: ``(x, y)``, or ``(x, y, z)`` for a 3D pointer."""
        return self._where

    @property
    def animate(self) -> "PointerAnimator":
        """The moves of a demonstration, as animations for ``scene.play``."""
        return PointerAnimator(self)

    def jump(self, target: Any, *, anchor: str = "center") -> "Pointer":
        """Put the tip on ``target`` at once, without moving there."""
        where = self._resolve(target, anchor)
        if len(where) == 3:
            self._drawable.move_to_3d(*where)
        else:
            self._drawable.move_to(*where)
        self._where = where
        return self

    def _resolve(self, target: Any, anchor: str) -> tuple[float, ...]:
        """The point a target names, in the pointer's space (2D or 3D)."""
        if anchor not in _ANCHORS:
            raise ValueError(f"anchor must be one of {', '.join(_ANCHORS)}")
        if isinstance(target, (tuple, list)):
            where = _point(target, "a point")
        elif callable(getattr(target, "bounds", None)):
            where = self._anchor_in(target.bounds(), anchor)
        elif all(hasattr(target, name) for name in ("left", "right", "top", "bottom")):
            where = self._anchor_in(target, anchor)
        else:
            raise TypeError("a pointer goes to a point, a Drawable or a Zone")
        if len(where) != len(self._where):
            if len(self._where) == 3:
                raise ValueError("a pointer made at a 3D point goes to 3D points")
            raise ValueError("a pointer goes to 3D points when it is made at one: at=(x, y, z)")
        return where

    @staticmethod
    def _anchor_in(box: Any, anchor: str) -> tuple[float, float]:
        fx, fy = _ANCHORS[anchor]
        return box.left + (box.right - box.left) * fx, box.bottom + (box.top - box.bottom) * fy


class PointerAnimator:
    """``pointer.animate``: moves a demonstration is made of.

    Each method returns something ``scene.play`` takes and updates where the
    pointer is, so the next one starts from the tip. ``to`` returns an
    animation, so ``duration`` and ``easing`` chain on it; ``drag`` takes a
    ``duration``, and ``click`` lasts 0.44 s.
    """

    def __init__(self, pointer: Pointer) -> None:
        self._pointer = pointer

    def to(self, target: Any, *, anchor: str = "center", arc: float = 0.0) -> Anim:
        """Move the tip to ``target``: a point, a ``Drawable`` (its ``anchor``) or a ``Zone``.

        ``arc`` bends the way so that its middle lies that fraction of the
        distance to the left of the straight line (negative to the right);
        0 goes straight. A 3D pointer goes straight.
        """
        pointer = self._pointer
        arc = float(arc)
        if not math.isfinite(arc):
            raise ValueError("arc must be finite")
        where = pointer._resolve(target, anchor)
        if len(where) == 3 and arc != 0.0:
            raise ValueError("arc bends the way of a 2D pointer")
        start = pointer._where
        pointer._where = where
        if len(where) == 3:
            return pointer._drawable.animate.move_to_3d(*where)
        if arc == 0.0 or start == where:
            return pointer._drawable.animate.move_to(*where)
        (x0, y0), (x1, y1) = start, where
        # A quadratic curve's middle lies halfway to its control point.
        control = ((x0 + x1) / 2 - (y1 - y0) * 2 * arc, (y0 + y1) / 2 + (x1 - x0) * 2 * arc)
        way = pointer._scene.geometry.path([("move", [(x0, y0)]), ("quad", [control, (x1, y1)])])
        way.no_fill().no_stroke().hidden()
        return pointer._drawable.animate.move_along(way)

    def click(self, *, color: Any = None, radius: float = 0.3) -> Composition:
        """A ripple that grows to ``radius`` and fades at the tip while the pointer presses and lets go."""
        pointer = self._pointer
        radius = _positive(radius, "radius")
        scene = pointer._scene
        ring = scene.geometry.circle(radius).no_fill()
        ring.stroke(color if color is not None else scene.colors.accent, 0.025)
        ring.scale_to(0.2).hidden()
        if len(pointer._where) == 3:
            ring.move_to_3d(*pointer._where).billboard()
        else:
            ring.move_to(*pointer._where).z_index(999)
        # scale_by keeps any size the author gave the pointer.
        press = pointer._drawable.animate.scale_by(_PRESS).duration(0.08)
        release = pointer._drawable.animate.scale_by(1.0 / _PRESS).duration(0.12)
        grow = _CLICK - 0.04
        return parallel(
            sequence(press, release),
            sequence(
                ring.animate.fade_in().duration(0.04),
                parallel(ring.animate.scale_to(1.0).duration(grow), ring.animate.fade_out().duration(grow)),
            ),
        )

    def drag(
        self, target: Any, obj: Drawable | None = None, *, anchor: str = "center", duration: float = 1.0
    ) -> Composition:
        """Move the tip to ``target`` in ``duration`` seconds while ``obj`` travels the same way."""
        pointer = self._pointer
        duration = _positive(duration, "duration")
        start = pointer._where
        move = self.to(target, anchor=anchor).duration(duration)
        if obj is None:
            return parallel(move)
        delta = [end - begin for begin, end in zip(start, pointer._where)]
        carry = obj.animate.shift_by_3d(*delta) if len(delta) == 3 else obj.animate.shift_by(*delta)
        return parallel(move, carry.duration(duration))


def _build_pointer(
    scene: Any,
    *,
    style: Literal["arrow", "hand", "text"] = "arrow",
    size: float = 1.0,
    color: Any = None,
    at: Sequence[float] = (0.0, 0.0),
) -> Pointer:
    """Build the pointer ``scene.ui.pointer`` returns."""
    if style not in _STYLES:
        raise ValueError(f"style must be one of {', '.join(_STYLES)}")
    size = _positive(size, "size")
    where = _point(at, "at")
    fill = color if color is not None else scene.colors.foreground
    shape = scene.geometry.polygon([(x * size, y * size) for x, y in _SHAPES[style]])
    shape.fill(fill).stroke(scene.colors.background, 0.016 * size)
    group = scene.geometry.group(shape, origin=(0.0, 0.0))
    if len(where) == 3:
        group.move_to_3d(*where).billboard()
    else:
        group.move_to(*where).z_index(1000)
    return Pointer(scene, group, where)
