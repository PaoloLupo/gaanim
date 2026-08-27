"""Visual regression coverage for technical and geometric primitives."""

import os

from gaanim import Anchor, BLACK, BLUE, GRAY, NAVY, WHITE, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

title = scene.text("Technical primitives", role="title").fill(WHITE).move_to(0, 260, anchor=Anchor.CENTER)
guide = scene.geometry.dashed_line(-420, 130, -100, 130, dash_length=20, gap_length=12).stroke(GRAY, 3)
measure = scene.geometry.double_arrow(-420, 70, -100, 70).fill(BLUE)
star = scene.geometry.star(5, 78, 36).fill(NAVY).move_to(80, 100)
hexagon = scene.geometry.regular_polygon(6, 72).fill(BLUE).move_to(300, 100)
slice = scene.geometry.sector(-170, -145, 90, 0.2, 1.9).fill(GRAY)
ring = scene.geometry.annulus(90, 52).fill(NAVY).move_to(160, -145)

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
