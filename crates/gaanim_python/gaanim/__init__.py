"""Gaanim — GPU-accelerated vector animation engine."""

import warnings

from .gaanim_core import (
    Anchor,
    Direction,
    FrameLayout,
    Flow,
    GridLayout,
    LayoutRegion,
    BLACK,
    BLUE,
    CORAL,
    CYAN,
    GOLD,
    GRAY,
    GREEN,
    NAVY,
    ORANGE,
    PINK,
    PURPLE,
    RED,
    TEAL,
    WHITE,
    YELLOW,
    Anim,
    Color,
    Drawable,
    Transition,
    Updater,
    ValueTracker,
    Scene,
)


def Canvas(*args, **kwargs):
    """Deprecated compatibility constructor; use :class:`Scene` instead."""
    warnings.warn(
        "Canvas is deprecated as the animation facade; use Scene instead. "
        "Use scene.canvas for viewport configuration.",
        DeprecationWarning,
        stacklevel=2,
    )
    return Scene(*args, **kwargs)

__all__ = [
    "Scene",
    "Canvas",
    "Drawable",
    "Anim",
    "Transition",
    "Color",
    "Anchor",
    "Direction",
    "LayoutRegion",
    "GridLayout",
    "FrameLayout",
    "Flow",
    "Updater",
    "ValueTracker",
    "GOLD",
    "CORAL",
    "BLUE",
    "WHITE",
    "BLACK",
    "RED",
    "GREEN",
    "YELLOW",
    "ORANGE",
    "PURPLE",
    "PINK",
    "GRAY",
    "CYAN",
    "NAVY",
    "TEAL",
]

__version__ = "0.3.0"
