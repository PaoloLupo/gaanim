"""Pyright fixture for color inputs; use an environment with Gaanim installed."""
from gaanim import Color, Scale, Scene


def color_inputs(scene: Scene, color: Color | str | None) -> None:
    scene.text("Text", color=color)
    scene.text.equation("x = 1", color=color)
    mixed: list[Color | str] = [Color("red"), "#123456"]
    Scale.category().colors(mixed)
    Scale.category().colors([(255, 0, 0), (0, 0, 255, 128)])
