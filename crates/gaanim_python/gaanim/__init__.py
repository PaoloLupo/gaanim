"""Gaanim — GPU-accelerated vector animation engine."""

from .gaanim_core import (
    Scene,
    Mobject,
    Selection,
    SelectionAnim,
    AnimSpec,
    Color,
    ObjectId,
    Theme,
    GOLD, CORAL, BLUE, WHITE, BLACK, RED, GREEN, YELLOW,
    ORANGE, PURPLE, PINK, GRAY, CYAN, NAVY, TEAL,
)

__all__ = [
    "Scene", "Mobject", "Selection", "SelectionAnim", "AnimSpec", "Color", "ObjectId", "Theme",
    "GOLD", "CORAL", "BLUE", "WHITE", "BLACK", "RED", "GREEN", "YELLOW",
    "ORANGE", "PURPLE", "PINK", "GRAY", "CYAN", "NAVY", "TEAL",
]

__version__ = "0.2.0"
