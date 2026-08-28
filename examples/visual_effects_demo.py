"""Retained vector glow, blur, and soft-shadow effects."""

import os

from gaanim import Anchor, Brush, Scene


scene = Scene(frame=(16, 9), margin=0.65)
scene.canvas.set_theme("tokyo-night")

scene.text("Visual effects", role="title").move_to(0, 3.5625, anchor=Anchor.CENTER)
scene.text("Cached vector effects for presentation graphics", role="subtitle").move_to(0, 2.875, anchor=Anchor.CENTER)

scene.geometry.circle(1.4375).fill(
    Brush.radial(["#E0F2FE", "#38BDF8", "#2563EB"], center=(-0.35, 0.4), radius=1.9375)
).no_stroke().glow("#38BDF8", radius=0.425, intensity=1.25).move_to(-4.875, 0.1875)
scene.text("Glow").move_to(-4.875, -1.8125, anchor=Anchor.CENTER)

scene.geometry.circle(1.3125).fill("#BB9AF7").no_stroke().blur(0.1625).move_to(0, 0.1875)
scene.geometry.circle(0.775).fill("#F7768E").no_stroke().blur(0.0875).move_to(0.525, 0.525)
scene.text("Blur").move_to(0, -1.8125, anchor=Anchor.CENTER)

scene.geometry.rounded_rect(3.1875, 2.375, 0.375).fill("#9ECE6A").no_stroke().shadow(
    "#00000099", x=0.225, y=-0.225, blur=0.15
).move_to(4.875, 0.1875)
scene.text("Shadow").fill("#1A1B26").move_to(4.875, 0.1875, anchor=Anchor.CENTER)
scene.text("Soft shadow").move_to(4.875, -1.8125, anchor=Anchor.CENTER)

scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
