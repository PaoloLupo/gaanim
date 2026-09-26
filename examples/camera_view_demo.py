"""Camera views: shapes that show what a second camera frames."""

import math
import os

from gaanim import Anchor, Scene


scene = Scene(frame=(16, 9), margin=0.65)
scene.canvas.set_theme("tokyo-night")
scene.text("Camera views", role="title").move_to(0, 3.5625, anchor=Anchor.CENTER)
scene.text("A shape can show what a second camera sees", role="subtitle").move_to(0, 2.875, anchor=Anchor.CENTER)

# A spiral of small dots whose core hides a label too small to read.
palette = ("#7AA2F7", "#BB9AF7", "#F7768E", "#E0AF68", "#9ECE6A", "#7DCFFF")
center = (-3.5, -0.25)
for index in range(150):
    angle = index * 0.42
    radius = 0.12 + 0.017 * index
    size = 0.018 + 0.00025 * index
    scene.geometry.dot(size).fill(palette[index % len(palette)]).no_stroke().move_to(
        center[0] + radius * math.cos(angle), center[1] + radius * math.sin(angle)
    )
core = scene.text("core", role="caption").scale_to(0.18).move_to(*center)

# The frame is the second camera; the screen shows what it sees four times larger.
frame = scene.geometry.rect(1.6, 0.9).no_fill().stroke("#E0AF68", 0.03).move_to(*center)
screen = scene.geometry.rounded_rect(6.4, 3.6, 0.18).stroke("#E0AF68", 0.05).move_to(3.9, -0.25)
screen.camera_view(frame)
scene.text("What the frame sees", role="caption").move_to(3.9, -2.4, anchor=Anchor.CENTER)

# A round lens magnifies whatever passes beneath it.
ruler = scene.text("small print under a magnifying glass", role="caption").scale_to(0.3).move_to(-3.5, -3.3)
lens = scene.geometry.circle(0.55).stroke("#7DCFFF", 0.04).move_to(-5.6, -3.3)
lens_frame = scene.geometry.circle(0.22).no_fill().no_stroke().move_to(-5.6, -3.3)
lens.camera_view(lens_frame)

scene.play([frame.animate.scale_to(0.5).duration(1.0)])
scene.play([
    frame.animate.move_to(center[0] + 1.5, center[1] + 0.9).rotate_to(0.4).duration(1.4),
    lens.animate.move_to(-1.4, -3.3).duration(1.4),
    lens_frame.animate.move_to(-1.4, -3.3).duration(1.4),
])
scene.play([frame.animate.move_to(*center).rotate_to(0.0).scale_to(1.0).duration(1.2)])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.5, 1.0, 1.7, 2.4, 3.0, 3.8])
else:
    scene.render()
