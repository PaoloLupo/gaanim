"""Gaanim — GPU-accelerated vector animation engine."""

from .gaanim_core import (
    Scene,
    Engine,
    SceneBuilder,
    Transition,
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
    "Scene", "Engine", "SceneBuilder", "Transition",
    "Mobject", "Selection", "SelectionAnim", "AnimSpec", "Color", "ObjectId", "Theme",
    "GOLD", "CORAL", "BLUE", "WHITE", "BLACK", "RED", "GREEN", "YELLOW",
    "ORANGE", "PURPLE", "PINK", "GRAY", "CYAN", "NAVY", "TEAL",
]

__version__ = "0.2.0"
