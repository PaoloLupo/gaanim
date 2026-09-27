"""Multipass bloom: neon strokes glow while an animated intensity pulses."""

import os

from gaanim import PostProcess, Scene


scene = Scene(frame=(16, 9), background="#05060d")
glow = scene.viz.parameter(0.3)
scene.canvas.post = PostProcess.bloom(threshold=0.55, intensity=glow, radius=0.6)

ring = scene.geometry.circle(1.6).no_fill().stroke("#ff4fd8", 0.12).move_to(-4.0, 0.2)
square = scene.geometry.rounded_rect(3.0, 3.0, 0.4).no_fill().stroke("#3de3ff", 0.12).move_to(0.0, 0.2)
star = scene.geometry.regular_polygon(5, 1.6).no_fill().stroke("#ffe45c", 0.12).move_to(4.0, 0.2)
# Dim shapes stay below the threshold and do not glow.
dim = scene.geometry.rect(12.0, 0.25).fill("#4a4f66").move_to(0, -3.0)
title = scene.text("Bloom", role="title").fill("#f4f6ff").move_to(0, 3.2)

scene.play([ring.animate.create(), square.animate.create(), star.animate.create()])
scene.play([glow.animate.set(1.4).duration(0.8).repeat(1, yoyo=True)])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-draw, at the dimmest and brightest glow, and back.
    scene.snapshots(snapshots, [0.5, 1.05, 1.8, 2.6])
else:
    scene.render()
