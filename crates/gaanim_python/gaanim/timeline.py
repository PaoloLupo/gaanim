"""Timelines with labelled events in lanes: ``scene.viz.timeline``.

A timeline maps dates to scene x along an axis, piecewise linear when the
axis has scale breaks. Each event gets a dot on the axis, a stem and a
label (a title and an optional body) placed in the first lane of its side
where it does not touch the labels already there, growing toward the side
where it fits inside the axis. Every part is an ordinary drawable, so the
timeline can be revealed, styled or animated piece by piece.
"""

from __future__ import annotations

import math
from typing import Any, Literal, Sequence

from .gaanim_core import Drawable

_SIDES = ("above", "below", "auto")
_PAD = 0.06
_LINE_GAP = 0.06


def _finite(value: Any, name: str) -> float:
    number = float(value)
    if not math.isfinite(number):
        raise ValueError(f"{name} must be finite")
    return number


def _positive(value: Any, name: str) -> float:
    number = _finite(value, name)
    if number <= 0.0:
        raise ValueError(f"{name} must be positive")
    return number


def _non_negative(value: Any, name: str) -> float:
    number = _finite(value, name)
    if number < 0.0:
        raise ValueError(f"{name} must not be negative")
    return number


class TimelineEvent:
    """One event of a timeline: its ``dot``, ``stem`` and ``label``."""

    def __init__(self, date: float, side: str, lane: int, dot: Drawable, stem: Drawable, label: Drawable, title: Drawable, body: Drawable | None) -> None:
        self.date = date
        self.side = side
        self.lane = lane
        self.dot = dot
        self.stem = stem
        self.label = label
        self.title = title
        self.body = body

    @property
    def parts(self) -> list[Drawable]:
        """The dot, the stem and the label, in the order a reveal shows them."""
        return [self.dot, self.stem, self.label]


class Timeline:
    """An axis from ``start`` to ``end`` whose events stack in lanes that do not overlap."""

    def __init__(self, scene: Any, start: float, end: float, *, x_range: tuple[float, float], y: float,
                 breaks: Sequence[tuple[float, float]], lane_height: float, gap: float,
                 label_width: float | None, color: Any) -> None:
        self._scene = scene
        self.start = start
        self.end = end
        self.y = y
        self._x_range = x_range
        self._lane_height = lane_height
        self._gap = gap
        self._label_width = label_width
        self._color = color
        self._knots = [(start, 0.0), *breaks, (end, 1.0)]
        # Occupied x intervals by side and lane.
        self._taken: dict[str, dict[int, list[tuple[float, float]]]] = {"above": {}, "below": {}}
        self._last_side = "below"
        self.events: list[TimelineEvent] = []
        self.axis = self._draw_axis([fraction for _, fraction in breaks])

    def x(self, date: float) -> float:
        """The scene x of ``date``, piecewise linear between the breaks."""
        date = _finite(date, "date")
        if not self.start <= date <= self.end:
            raise ValueError(f"date {date:g} is outside the timeline ({self.start:g} to {self.end:g})")
        left, right = self._x_range
        for (d0, f0), (d1, f1) in zip(self._knots, self._knots[1:]):
            if date <= d1:
                fraction = f0 + (f1 - f0) * (date - d0) / (d1 - d0)
                return left + (right - left) * fraction
        return right

    def reserve(self, start: float, end: float, *, side: Literal["above", "below"], lane: int | None = None) -> "Timeline":
        """Keep the dates from ``start`` to ``end`` free on ``side``: in one ``lane``, or in every lane."""
        if side not in ("above", "below"):
            raise ValueError("side must be 'above' or 'below'")
        x0, x1 = sorted((self.x(start), self.x(end)))
        if lane is None:
            self._taken[side].setdefault(-1, []).append((x0, x1))
        else:
            if int(lane) != lane or lane < 0:
                raise ValueError("lane must be a non-negative integer")
            self._taken[side].setdefault(int(lane), []).append((x0, x1))
        return self

    def event(self, date: float, title: str, body: str | None = None, *,
              side: Literal["above", "below", "auto"] = "auto", color: Any = None,
              knockout: bool = True) -> TimelineEvent:
        """Mark ``date`` with a dot, a stem and a label in the first free lane of ``side``.

        ``"auto"`` takes the side whose free lane is nearer the axis, and
        alternates when both are as near.
        """
        if side not in _SIDES:
            raise ValueError("side must be 'above', 'below' or 'auto'")
        scene = self._scene
        x = self.x(date)
        paint = color if color is not None else self._color
        wrap = self._label_width if self._label_width is not None else False
        title_text = scene.text(title, role="label", weight=700, wrap=wrap)
        body_text = scene.text(body, role="caption", color=scene.colors.muted, wrap=wrap) if body else None
        tb = title_text.bounds()
        bb = body_text.bounds() if body_text is not None else None
        width = max(tb.width, bb.width if bb is not None else 0.0)
        height = tb.height + (bb.height + _LINE_GAP if bb is not None else 0.0)
        lanes = math.ceil((height + self._gap) / self._lane_height)
        # It grows to the right unless it would pass the end of the axis; the
        # other way, when it stays within the axis too, is the fallback when a
        # reservation blocks the first.
        fits_right = x + width <= self._x_range[1]
        fits_left = x - width >= self._x_range[0]
        right = fits_right or not fits_left
        ways = [right] + ([not right] if (fits_left if right else fits_right) else [])
        options = {}
        for candidate in (("above", "below") if side == "auto" else (side,)):
            for grows_right in ways:
                x0, x1 = (x, x + width) if grows_right else (x - width, x)
                lane = self._free_lane(candidate, x0, x1, lanes)
                if lane is not None:
                    options[candidate] = (lane, grows_right, x0, x1)
                    break
        if not options:
            title_text.hidden()
            if body_text is not None:
                body_text.hidden()
            raise ValueError(f"the dates around {date:g} are reserved in every lane of the {side} side")
        if len(options) == 2 and options["above"][0] == options["below"][0]:
            side = "below" if self._last_side == "above" else "above"
        else:
            side = min(options, key=lambda name: options[name][0])
        lane, grows_right, x0, x1 = options[side]
        self._last_side = side
        for index in range(lane, lane + lanes):
            self._taken[side].setdefault(index, []).append((x0, x1))
        # The label's edge nearest the axis lies one lane height further out per lane.
        near = (lane + 1) * self._lane_height
        sign = 1.0 if side == "above" else -1.0
        bottom = self.y + near if side == "above" else self.y - near - height
        anchor = "left" if grows_right else "right"
        title_text.move_to(x, bottom + height, anchor=f"top_{anchor}")
        members = []
        if knockout:
            # Made after the texts, so it is ordered below them explicitly.
            plate = scene.geometry.rect(width + 2 * _PAD, height + 2 * _PAD).fill(scene.colors.background).no_stroke()
            plate.move_to((x0 + x1) / 2, bottom + height / 2).z_index(0)
            members.append(plate)
        members.append(title_text.z_index(1))
        if body_text is not None:
            body_text.move_to(x, bottom, anchor=f"bottom_{anchor}")
            members.append(body_text.z_index(1))
        label = scene.geometry.group(*members).z_index(1)
        stem = scene.geometry.line(x, self.y, x, self.y + sign * near).stroke(paint, 0.02)
        dot = scene.geometry.circle(0.07).fill(paint).no_stroke().move_to(x, self.y).z_index(2)
        event = TimelineEvent(float(date), side, lane, dot, stem, label, title_text, body_text)
        self.events.append(event)
        return event

    def _free_lane(self, side: str, x0: float, x1: float, lanes: int) -> int | None:
        """The first lane from which ``lanes`` lanes are free over ``x0..x1``, with
        the gap, or ``None`` when that span is reserved in every lane."""
        taken = self._taken[side]

        def clear(spans: list[tuple[float, float]]) -> bool:
            return all(x1 + self._gap <= a or b + self._gap <= x0 for a, b in spans)

        # Lane -1 holds what is reserved in every lane.
        if not clear(taken.get(-1, [])):
            return None
        lane = 0
        while not all(clear(taken.get(index, [])) for index in range(lane, lane + lanes)):
            lane += 1
        return lane

    def _draw_axis(self, breaks: list[float]) -> Drawable:
        scene = self._scene
        ink = scene.colors.foreground
        left, right = self._x_range
        cuts = [left + (right - left) * fraction for fraction in breaks]
        half = 0.06
        pieces = []
        edges = [left, *[edge for cut in cuts for edge in (cut - half, cut + half)], right]
        for a, b in zip(edges[::2], edges[1::2]):
            pieces.append(scene.geometry.line(a, self.y, b, self.y).stroke(ink, 0.03))
        for cut in cuts:
            for edge in (cut - half, cut + half):
                pieces.append(scene.geometry.line(edge - 0.05, self.y - 0.13, edge + 0.05, self.y + 0.13).stroke(ink, 0.03))
        return scene.geometry.group(*pieces)


def _build_timeline(
    scene: Any,
    start: float,
    end: float,
    *,
    x_range: tuple[float, float] = (-6.5, 6.5),
    y: float = 0.0,
    breaks: Sequence[tuple[float, float]] = (),
    lane_height: float = 0.6,
    gap: float = 0.2,
    label_width: float | None = None,
    color: Any = None,
) -> Timeline:
    """Build the timeline ``scene.viz.timeline`` returns."""
    start, end = _finite(start, "start"), _finite(end, "end")
    if end <= start:
        raise ValueError("end must be after start")
    left, right = (_finite(value, "x_range") for value in x_range)
    if right <= left:
        raise ValueError("x_range must go from left to right")
    knots = []
    for date, fraction in breaks:
        knots.append((_finite(date, "a break date"), _finite(fraction, "a break fraction")))
    previous = (start, 0.0)
    for date, fraction in [*knots, (end, 1.0)]:
        if not (date > previous[0] and fraction > previous[1]):
            raise ValueError("breaks go forward in date and in fraction, strictly inside the timeline")
        previous = (date, fraction)
    if label_width is not None:
        label_width = _positive(label_width, "label_width")
    return Timeline(
        scene, start, end, x_range=(left, right), y=_finite(y, "y"), breaks=knots,
        lane_height=_positive(lane_height, "lane_height"), gap=_non_negative(gap, "gap"),
        label_width=label_width, color=color if color is not None else scene.colors.accent,
    )
