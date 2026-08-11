"""A ValueTracker driving an always-redrawn curved arrow."""

import os

from gaanim import BLACK, GOLD, WHITE, Scene


scene = Scene(960, 540, background=WHITE)
title = scene.text("Reactive curved arrow", role="title").fill(BLACK).at(0, 220)

disk = scene.circle(155).no_fill().stroke(BLACK, 4)
marker = scene.dot(11).fill(GOLD).at(155, 0)
theta = scene.value_tracker(0.25)

# The sweep is regenerated natively every frame from the tracker value.
rotation = scene.always_redraw_arc(theta, 0, 0, 190, 0.0).fill(BLACK).stroke(BLACK, 3)
label = scene.text("theta").fill(BLACK).at(20, 45)

scene.play([
    title.write(),
    disk.create().duration(0.6),
    marker.fade_in().duration(0.3),
    rotation.fade_in().duration(0.3),
    theta.animate_to(4.8).duration(2.4).smooth(),
])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.7, 1.5, 2.3])

scene.render()
