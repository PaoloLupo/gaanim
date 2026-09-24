"""Edge and point growth, a sampled custom easing, a morph between segments,
and selection reveal, brace, and annotation."""

import os

from gaanim import BLACK, BLUE, CORAL, CYAN, GOLD, GRAY, WHITE, Direction, Easing, Scene, Transition, part


scene = Scene(frame=(16, 9), background=BLACK)
# Quartic ease-out sampled once from Python; rendering never calls back.
settle = Easing.custom(lambda t: 1 - (1 - t) ** 4, samples=128)

bars_segment = scene.segment("Barras")
title = scene.text("Crecer desde un borde o un punto", role="title").fill(WHITE).move_to(0, 3.375)
heights = [(-4.5, 2.0, BLUE), (-2.5, 3.5, CYAN), (-0.5, 2.75, BLUE)]
bars = [scene.geometry.rect(1.4, h).fill(color).move_to(x, h / 2 - 2.5) for x, h, color in heights]
badge = scene.geometry.circle(0.9).fill(GOLD).move_to(4.0, 1.25)

scene.play([title.animate.fade_in().duration(0.4)])
scene.play([bar.animate.grow_from_edge(Direction.DOWN).duration(0.9).easing(settle) for bar in bars])
scene.play([badge.animate.grow_from_point(1.0, -2.5).duration(0.6)])
scene.wait(0.5)

equation_segment = scene.segment("Ecuación")
card = scene.geometry.rect(9.0, 4.5).fill("#16213a").move_to(0, 0.5)
formula = scene.text.equation(
    part("energy", "E"), part("equals", "="), part("mass", "m"), part("light_speed", "c^2")
).move_to(0, 0.75)
formula["mass"].fill(GOLD)
formula["light_speed"].fill(CORAL)
caption = scene.text("Revelar, nombrar y anotar términos").fill(GRAY).move_to(0, -3.25)

scene.wait(0.8)
scene.play([formula["light_speed"].animate.reveal("from_below").duration(0.6)])
scene.play([formula["mass"].animate.brace("masa").duration(0.7)])
scene.play([formula["light_speed"].animate.annotate("velocidad de la luz", offset=(2.5, 1.2)).duration(0.7)])
scene.play([caption.animate.fade_in().duration(0.4)])
scene.wait(0.8)

# The tallest bar becomes the equation card across the cut.
scene.link(bars_segment, equation_segment, Transition.morph(0.8, pairs=[(bars[1], card)]))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-growth, grown, mid-morph, and the annotated equation.
    scene.snapshots(snapshots, [0.8, 2.2, 2.8, 6.0])
else:
    scene.render()
