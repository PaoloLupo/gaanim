"""Compact semantic equation parts with implicit mathematical spacing."""

import os

from gaanim import BLACK, GOLD, GRAY, WHITE, Scene, parts


scene = Scene(1280, 720, background=BLACK)

title = scene.text("Partes semánticas compactas", role="title").fill(WHITE).at(0, 250)
equation = scene.text(
    "$-",
    parts(
        mass_left="m",
        gravity="g sin(theta)",
    ),
    " = ",
    parts(
        mass_right="m",
        length="L",
        acceleration="theta''",
    ),
    "$",
).at(0, 45).scaled(1.25)
caption = scene.text(
    "parts() conserva nombres y separa términos adyacentes dentro de math",
    role="caption",
).fill(GRAY).at(0, -150)

equation["gravity"].fill(GOLD)
equation["acceleration"].fill(GOLD)
scene.play([title.write(0.7), equation.write(1.8, by="part"), caption.fade_in(0.7)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.8, 2.5])
else:
    scene.render()
