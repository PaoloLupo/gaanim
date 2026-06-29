"""Minimal Write smoke test.

A single circle outline is written from arc-length 0 to 1 over 2 seconds.
If the bug were still present, the circle would appear complete from frame 0.
"""

from gaanim import BLUE, GOLD, RED, Engine, Scene

engine = Engine(width=1920, height=1080, title="Write Smoke")

intro = engine.scene("intro")

# Single leaf: should write the outline once.
circle = intro.circle(80).stroke(RED, 4).no_fill().at(0, 0)

intro.play(circle.write(2.0).linear())

# Filled text: should write the outline of each glyph in sequence.
title = intro.title("Hola!").at(100, 10)
intro.play(title.write(3).smooth())

# Filled equation: should write outline of each glyph.
equation = intro.equation("x^2 + y^2 = z^2").at(200, -100)
intro.play(equation.write(3.0).linear())
engine.render()
