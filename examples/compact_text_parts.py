"""Compact semantic equation parts with implicit mathematical spacing."""

import os

from gaanim import BLACK, GOLD, GRAY, WHITE, Scene, parts


scene = Scene(1280, 720, background=BLACK)

title = scene.text("Partes semánticas compactas", role="title").fill(WHITE).move_to(0, 250)
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
).move_to(0, 45).scale_to(1.25)
caption = scene.text(
    "equation() crea math de bloque; parts() conserva nombres semánticos",
    role="caption",
).fill(GRAY).move_to(0, -150)

scene.play([title.animate.write(0.7), equation.animate.write(1.8), caption.animate.fade_in(0.7)])
scene.play([equation["gravity"].animate.indicate(0.7)])
scene.play([equation["acceleration"].animate.fill(GOLD).duration(0.7)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.8, 2.15, 2.85, 3.2])
else:
    scene.render()
