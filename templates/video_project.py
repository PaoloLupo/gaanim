"""Starter de video animado generado por `gaanim init video`."""

import os
from pathlib import Path

from gaanim import BLUE, GOLD, WHITE, Scene


ROOT = Path(__file__).resolve().parent

scene = Scene(1920, 1080, margin=72)
scene.load_project(str(ROOT / "gaanim.toml"))
scene.canvas.set_theme("technical")

title = scene.text("Mi video con Gaanim", role="title").move_to(0, 300)
subtitle = scene.text(
    "Una escena, un timeline y exportación reproducible",
    role="subtitle",
).move_to(0, 220)
orb = scene.geometry.circle(120).fill(BLUE).stroke(WHITE, 5).move_to(-420, -20)
label = scene.text("Edita main.py para comenzar").fill(GOLD).move_to(180, -20)

scene.play(
    [
        title.animate.write().duration(0.6),
        subtitle.animate.fade_in().duration(0.5),
        orb.animate.grow_from_center().duration(0.7),
    ]
)
scene.play(
    [
        orb.move(360, 0).duration(1.0).smooth(),
        label.animate.write().duration(0.8),
    ]
)
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.8, 1.6])
scene.render()
