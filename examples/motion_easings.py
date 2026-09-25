"""A visual table of the expressive easing factories."""

import os

from gaanim import BLACK, CORAL, CYAN, GRAY, WHITE, Easing, Scene


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Easings expresivos", role="title").fill(WHITE).move_to(0, 3.7)
scene.geometry.line(-2.5, 3.0, -2.5, -3.0).stroke(GRAY, 0.02)
scene.geometry.line(4.5, 3.0, 4.5, -3.0).stroke(GRAY, 0.02)

rows = [
    ("back(2.5)", Easing.back(2.5)),
    ("elastic(1.2, 0.35)", Easing.elastic(1.2, 0.35)),
    ("bounce(0.6)", Easing.bounce(0.6)),
    ("slow_mo()", Easing.slow_mo()),
    ("rough(seed=4)", Easing.rough(0.8, points=16, seed=4)),
    ("squish(SMOOTH, 0.3, 0.7)", Easing.squish(Easing.SMOOTH, 0.3, 0.7)),
    ("from_svg(...)", Easing.from_svg("M0,0 C0.3,0 0.2,1.25 1,1")),
    ('steps(6, jump="both")', Easing.steps(6, jump="both")),
]
anims = []
for index, (name, easing) in enumerate(rows):
    y = 2.6 - index * 0.74
    scene.text(name).fill(GRAY).scale_by(0.55).move_to(-5.4, y)
    dot = scene.geometry.dot(0.13).fill(CORAL if index % 2 else CYAN).move_to(-2.5, y)
    anims.append(dot.animate.move_to(4.5, y).duration(1.6).easing(easing))

scene.play(anims)
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.4, 0.8, 1.2])
else:
    scene.render()
