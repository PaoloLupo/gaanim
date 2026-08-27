"""Retained vector glow, blur, and soft-shadow effects."""

import os

from gaanim import Anchor, Brush, Scene


scene = Scene(1280, 720, margin=52)
scene.canvas.set_theme("tokyo-night")

scene.text("Visual effects", role="title").move_to(0, 285, anchor=Anchor.CENTER)
scene.text("Cached vector effects for presentation graphics", role="subtitle").move_to(0, 230, anchor=Anchor.CENTER)

scene.geometry.circle(115).fill(
    Brush.radial(["#E0F2FE", "#38BDF8", "#2563EB"], center=(-28, 32), radius=155)
).no_stroke().glow("#38BDF8", radius=34, intensity=1.25).move_to(-390, 15)
scene.text("Glow").move_to(-390, -145, anchor=Anchor.CENTER)

scene.geometry.circle(105).fill("#BB9AF7").no_stroke().blur(13).move_to(0, 15)
scene.geometry.circle(62).fill("#F7768E").no_stroke().blur(7).move_to(42, 42)
scene.text("Blur").move_to(0, -145, anchor=Anchor.CENTER)

scene.geometry.rounded_rect(255, 190, 30).fill("#9ECE6A").no_stroke().shadow(
    "#00000099", x=18, y=-18, blur=12
).move_to(390, 15)
scene.text("Shadow").fill("#1A1B26").move_to(390, 15, anchor=Anchor.CENTER)
scene.text("Soft shadow").move_to(390, -145, anchor=Anchor.CENTER)

scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
