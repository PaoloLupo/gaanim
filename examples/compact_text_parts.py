"""Compact semantic equation parts with implicit mathematical spacing."""

import os

from gaanim import BLACK, GOLD, GRAY, WHITE, Scene, parts


scene = Scene(frame=(16, 9), background=BLACK)

title = scene.text("Partes semánticas compactas", role="title").fill(WHITE).move_to(0, 3.125)
equation = scene.text.equation(
    "-",
    parts(
        mass_left="m",
        gravity="g sin(theta)",
    ),
    "=",
    parts(
        mass_right="m",
        length="L",
        acceleration="theta''",
    ),
).move_to(0, 0.5625).scale_to(1.25)
caption = scene.text(
    "equation() crea math de bloque; parts() conserva nombres semánticos",
    role="caption",
).fill(GRAY).move_to(0, -1.875)

scene.play([title.animate.write().duration(0.7), equation.animate.write().duration(1.8), caption.animate.fade_in().duration(0.7)])
scene.play([equation["gravity"].animate.indicate().duration(0.7)])
scene.play([equation["acceleration"].animate.fill(GOLD).duration(0.7)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.8, 2.15, 2.85, 3.2])
else:
    scene.render()
