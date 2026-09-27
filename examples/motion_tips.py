"""Tips on any stroke: a route, a curve and an arc grow with their heads in front."""

import os

from gaanim import BLACK, CORAL, CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Puntas en cualquier trazo", role="title").fill(WHITE).move_to(0, 3.6)

route = scene.geometry.polyline([(-6.4, -1.2), (-5.2, 1.2), (-4.0, -0.4), (-2.8, 1.4)]).no_fill()
route.stroke(CYAN, 0.06).tip(end="arrow", start="dot")
curve = scene.geometry.bezier((-1.4, -1.0), [(-0.6, 2.4), (0.6, -2.4)], (1.4, 1.0)).no_fill()
curve.stroke(GOLD, 0.06).tip(end="arrow", start="arrow")
orbit = scene.geometry.arc(4.6, 0.2, 1.3, 0.0, 4.4).no_fill()
orbit.stroke(CORAL, 0.06).tip(end="arrow", length=0.35, width=0.3)
for x, label in [(-4.6, "polyline"), (0.0, "bezier"), (4.6, "arc")]:
    scene.text(label).fill(GRAY).scale_by(0.5).move_to(x, -2.2)

scene.play([
    route.animate.grow_arrow().duration(1.5),
    curve.animate.grow_arrow().duration(1.5),
    orbit.animate.grow_arrow().duration(1.5),
])
scene.play([orbit.animate.trim(start=0.4).duration(0.8)])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Heads emerging, mid-growth, fully drawn, and the arc trimmed from its tail.
    scene.snapshots(snapshots, [0.1, 0.75, 1.5, 2.5])
else:
    scene.render()
