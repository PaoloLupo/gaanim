"""A Parameter driving an always-redrawn curved arrow."""

import os

from gaanim import Easing, Anchor, BLACK, GOLD, WHITE, Scene


scene = Scene(frame=(16, 9), background=WHITE)
title = scene.text("Reactive curved arrow", role="title").fill(BLACK).move_to(0, 3.666667, anchor=Anchor.CENTER)

disk = scene.geometry.circle(2.583333).no_fill().stroke(BLACK, 0.066667)
marker = scene.geometry.dot(0.183333).fill(GOLD).move_to(2.583333, 0)
theta = scene.viz.parameter(0.25)

# The sweep is regenerated natively every frame from the tracker value.
rotation = scene.geometry.always_redraw_arc(theta, 0, 0, 3.166667, 0.0).fill(BLACK).stroke(BLACK, 0.05)
label = scene.text("theta").fill(BLACK).move_to(0.333333, 0.75, anchor=Anchor.CENTER)

scene.play([
    title.animate.write(),
    disk.animate.create().duration(0.6),
    marker.animate.fade_in().duration(0.3),
    rotation.animate.fade_in().duration(0.3),
    theta.animate.set(4.8).duration(2.4).easing(Easing.SMOOTH),
])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.7, 1.5, 2.3])

scene.render()
