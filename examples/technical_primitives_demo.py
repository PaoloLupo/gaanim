"""Visual regression coverage for technical and geometric primitives."""

import os

from gaanim import Anchor, BLACK, BLUE, GRAY, NAVY, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK, margin=0.7)

title = scene.text("Technical primitives", role="title").fill(WHITE).move_to(0, 3.25, anchor=Anchor.CENTER)
guide = scene.geometry.dashed_line(-5.25, 1.625, -1.25, 1.625, dash_length=0.25, gap_length=0.15).stroke(GRAY, 0.0375)
measure = scene.geometry.double_arrow(-5.25, 0.875, -1.25, 0.875).fill(BLUE)
star = scene.geometry.star(5, 0.975, 0.45).fill(NAVY).move_to(1, 1.25)
hexagon = scene.geometry.regular_polygon(6, 0.9).fill(BLUE).move_to(3.75, 1.25)
slice = scene.geometry.sector(-2.125, -1.8125, 1.125, 0.2, 1.9).fill(GRAY)
ring = scene.geometry.annulus(1.125, 0.65).fill(NAVY).move_to(2, -1.8125)

scene.play([
    title.animate.write().duration(0.5),
    guide.animate.create().duration(0.6),
    measure.animate.create().duration(0.6),
    star.animate.grow_from_center().duration(0.6),
    hexagon.animate.create().duration(0.6),
    slice.animate.create().duration(0.6),
    ring.animate.create().duration(0.6),
])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.5, 1.0])
else:
    scene.render()
