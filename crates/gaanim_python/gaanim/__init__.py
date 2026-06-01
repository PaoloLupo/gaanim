"""Gaanim Python bindings — high-performance GPU-accelerated vector animation engine.

A clean, fluent Python API for authoring Manim-style scenes. Every call returns
a fresh, immutable handle — chained calls produce new handles with the
modification applied. The actual scene is built only when `Scene.render()` is
called, at which point the deferred op queue is drained into a Bevy ECS app
and the Vello GPU window is shown.

Example::

    from gaanim import Scene, GOLD, CORAL, BLUE

    scene = Scene(width=1280, height=720, title="Demo")

    bg = scene.circle(80).fill(BLUE).z_index(-10)
    title = scene.title("Gaanim Vector Engine")
    formula = scene.equation("E = m c^2")
    sum_ = scene.equation("sum_(i=1)^n i = frac(n(n+1), 2)")

    scene.play(
        bg.animate().scale(1.2).duration(1.5).spring(),
        title.animate().translate_to(-230, 240).duration(1.8).spring(),
        formula.animate().translate_to(-100, 60).duration(1.0).smooth(),
        sum_.animate().translate_to(-200, -150).duration(2.0).spring(),
    )
    scene.wait(1.0)

    scene.select(formula, "m c^2").fill(GOLD)
    scene.select(sum_, "n(n+1)").fill(CORAL)
    scene.select(sum_, "n(n+1)").animate().shift(0, 30).duration(1.5).build()

    scene.wait(1.5)
    scene.render()
"""

from .gaanim_core import (
    # Classes
    Scene,
    Mobject,
    Selection,
    SelectionAnim,
    AnimSpec,
    Color,
    ObjectId,
    Theme,
    # Color palette
    GOLD, CORAL, BLUE, WHITE, BLACK, RED, GREEN, YELLOW,
    ORANGE, PURPLE, PINK, GRAY, CYAN, NAVY, TEAL,
)

__all__ = [
    "Scene", "Mobject", "Selection", "SelectionAnim", "AnimSpec", "Color", "ObjectId", "Theme",
    "GOLD", "CORAL", "BLUE", "WHITE", "BLACK", "RED", "GREEN", "YELLOW",
    "ORANGE", "PURPLE", "PINK", "GRAY", "CYAN", "NAVY", "TEAL",
]

__version__ = "0.2.0"
