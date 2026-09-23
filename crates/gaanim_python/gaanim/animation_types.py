"""Public typing helpers for reactive properties, playback, and pure animation callbacks."""

from __future__ import annotations

from typing import Literal, TypeAlias, TypedDict

from .gaanim_core import (
    Anim,
    Audio,
    Brush,
    Color,
    Composition,
    Computed,
    Lottie,
    Parameter,
    TimeInput,
    Variable,
    Video,
    VideoSegment,
)

ScalarSource: TypeAlias = float | Parameter | Variable | Computed | TimeInput
"""A fixed scalar or an explicitly owned, deterministic reactive source."""

Playable: TypeAlias = Anim | Audio | Video | VideoSegment | Lottie | Composition
"""Anything ``Scene.play`` and ``parallel``/``sequence``/``stagger`` accept."""

AnimationChannel: TypeAlias = Literal[
    "position", "rotation", "scale", "opacity", "fill", "stroke", "stroke_width"
]
"""A property that a custom animation declares before it is scheduled."""

_Paint: TypeAlias = Color | str | tuple[int, int, int] | tuple[int, int, int, int] | Brush

class CustomAnimationValues(TypedDict, total=False):
    """Absolute local values returned by ``Anim.custom``.

    Return exactly the keys declared in ``channels``. Position uses scene units;
    rotation is a Z angle in radians; scale is uniform or an XYZ triple. Opacity
    is in 0..1 and stroke width is nonnegative. All numeric values must be finite.
    """

    position: tuple[float, float] | tuple[float, float, float]
    rotation: float
    scale: float | tuple[float, float, float]
    opacity: float
    fill: _Paint
    stroke: _Paint
    stroke_width: float
