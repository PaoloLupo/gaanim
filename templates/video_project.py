"""Starter de video animado generado por `gaanim init video`."""

import os
from pathlib import Path

from gaanim import Easing, BLUE, GOLD, WHITE, Scene


ROOT = Path(__file__).resolve().parent

scene = Scene(frame=(16, 9), margin=0.6)
scene.assets.load_project(str(ROOT / "gaanim.toml"))
scene.canvas.set_theme("technical")

title = scene.text("Mi video con Gaanim", role="title").move_to(0, 2.5)
subtitle = scene.text(
    "Una escena, un timeline y exportación reproducible",
    role="subtitle",
).move_to(0, 1.833)
orb = scene.geometry.circle(1).fill(BLUE).stroke(WHITE, 0.042).move_to(-3.5, -0.167)
label = scene.text("Edita main.py para comenzar").fill(GOLD).move_to(1.5, -0.167)

scene.play(
    [
        title.animate.write().duration(0.6),
        subtitle.animate.fade_in().duration(0.5),
        orb.animate.grow_from_center().duration(0.7),
    ]
)
scene.play(
    [
        orb.animate.shift_by(3, 0).duration(1.0).easing(Easing.SMOOTH),
        label.animate.write().duration(0.8),
    ]
)
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.8, 1.6])
scene.render()
