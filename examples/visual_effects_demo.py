"""Retained vector glow, blur, and soft-shadow effects."""

import os

from gaanim import Brush, Scene


scene = Scene(1280, 720, margin=52)
scene.canvas.set_theme("tokyo-night")

scene.text("Visual effects", role="title").at(0, 285)
scene.text("Cached vector effects for presentation graphics", role="subtitle").at(0, 230)

scene.circle(115).fill(
    Brush.radial(["#E0F2FE", "#38BDF8", "#2563EB"], center=(-28, 32), radius=155)
).no_stroke().glow("#38BDF8", radius=34, intensity=1.25).at(-390, 15)
scene.text("Glow").at(-390, -145)

scene.circle(105).fill("#BB9AF7").no_stroke().blur(13).at(0, 15)
scene.circle(62).fill("#F7768E").no_stroke().blur(7).at(42, 42)
scene.text("Blur").at(0, -145)

scene.rounded_rect(255, 190, 30).fill("#9ECE6A").no_stroke().shadow(
    "#00000099", x=18, y=-18, blur=12
).at(390, 15)
scene.text("Shadow").fill("#1A1B26").at(390, 15)
scene.text("Soft shadow").at(390, -145)

scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
