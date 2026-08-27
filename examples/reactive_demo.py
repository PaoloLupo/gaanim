"""A Parameter driving an always-redrawn curved arrow."""

import os

from gaanim import Anchor, BLACK, GOLD, WHITE, Scene


scene = Scene(960, 540, background=WHITE)
title = scene.text("Reactive curved arrow", role="title").fill(BLACK).move_to(0, 220, anchor=Anchor.CENTER)

disk = scene.geometry.circle(155).no_fill().stroke(BLACK, 4)
marker = scene.geometry.dot(11).fill(GOLD).move_to(155, 0)
theta = scene.viz.parameter(0.25)

# The sweep is regenerated natively every frame from the tracker value.
rotation = scene.geometry.always_redraw_arc(theta, 0, 0, 190, 0.0).fill(BLACK).stroke(BLACK, 3)
label = scene.text("theta").fill(BLACK).move_to(20, 45, anchor=Anchor.CENTER)

scene.play([
    title.animate.write(),
    disk.animate.create().duration(0.6),
    marker.animate.fade_in().duration(0.3),
    rotation.animate.fade_in().duration(0.3),
    theta.animate.set(4.8).duration(2.4).smooth(),
])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.7, 1.5, 2.3])

scene.render()
