"""Hand-drawn notations (AN-01): marks that draw themselves around their targets."""

import os

from gaanim import BLUE, GREEN, RED, Scene, part


INK = "#1f2937"
scene = Scene(frame=(16, 9), background="#f8f5ee")
title = scene.text("Anotaciones a mano alzada", role="title").fill(INK).move_to(0, 3.4)
eq = scene.text.equation("E = ", part("rhs", "m c^2"), color=INK, size=0.9).move_to(-3.6, 1.0)
card = scene.geometry.rounded_rect(3.4, 1.6, 0.2).fill("#dbe7f5").move_to(3.6, 1.0)
price = scene.text("99 €", color=INK, size=0.8).move_to(-3.6, -2.0)
steps = scene.text("primer paso\nsegundo paso", color=INK, size=0.5).move_to(3.6, -2.0)

underline = title.annotate.underline(color=RED, seed=1)
circle = eq["rhs"].annotate.circle(color=RED, seed=2)
box = card.annotate.box(color=BLUE, seed=3)
crossed = price.annotate.crossed_off(color=RED, roughness=1.4, passes=1, seed=4)
bracket = steps.annotate.bracket("left", color=GREEN, seed=5)

scene.wait(0.3)
scene.play([underline.animate.create(), box.animate.create()], duration=1.0)
scene.play([circle.animate.create()], duration=0.8)
scene.play([crossed.animate.create(), bracket.animate.create()], duration=0.8)
# The box follows its target and keeps its scribble.
scene.play([card.animate.shift_by(0, -0.4)], duration=0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.8, 1.7, 2.5, 3.5])
else:
    scene.render()
