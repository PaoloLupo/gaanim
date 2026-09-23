"""GrowArrow: straight, curved and arc arrows grow from their tails.

The tip travels along the spine with an undistorted head and constant stroke,
unlike Manim's GrowArrow, which scales the whole arrow about its start point.
Set GAANIM_SNAPSHOTS to capture exact seeks for visual regression.
"""

import os

from gaanim import BLACK, CYAN, GOLD, PINK, WHITE, Easing, Scene

scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("GrowArrow", role="title").fill(WHITE).move_to(0, 3.4)

straight = scene.geometry.arrow(-6, 1.5, -1.5, 1.5, head_length=0.4, head_width=0.34, body_width=0.08).fill(CYAN)
curved = scene.geometry.curved_arrow(-6, -2.2, -1.5, -2.2, 1.4, head_length=0.4, head_width=0.34, body_width=0.08).fill(GOLD)
orbit = scene.geometry.curved_arrow_arc(3.5, -0.3, 2.0, -1.2, 4.6, head_length=0.4, head_width=0.34, body_width=0.08).fill(PINK)

scene.play([straight.animate.grow_arrow().duration(1.2)])
scene.play([curved.animate.grow_arrow().duration(1.2).easing(Easing.SMOOTH)])
scene.play([orbit.animate.grow_arrow().duration(1.6).easing(Easing.spring(stiffness=90.0, damping=14.0))])
scene.wait(0.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.1, 0.6, 1.2, 1.8, 2.4, 3.2, 4.0, 4.5])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
