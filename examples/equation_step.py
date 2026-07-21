"""A derivation step that preserves the glyphs shared by both equations."""

from gaanim import BLACK, WHITE, Scene


scene = Scene(1920, 1080, background=BLACK)
title = scene.title("Resolver paso a paso").fill(WHITE).at(0, 220)
before = scene.equation("x + 3 = 7").at(0, 0)
after = scene.equation("x = 4").at(0, 0)

scene.play([title.write(), before.write()])
scene.wait(0.4)
scene.step_equation(before, after, duration=0.8)
scene.wait(0.4)
scene.render()
