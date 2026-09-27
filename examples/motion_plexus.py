"""Plexus: live lines between a drifting cloud of points, fading with distance."""

import os

from gaanim import CYAN, WHITE, Distribution, Scene


scene = Scene(frame=(16, 9), background="#060b16")
scene.text("Plexus", role="title").fill(WHITE).move_to(0, 3.4)
dot = scene.geometry.dot(0.06).fill(CYAN)
cloud = scene.geometry.duplicate(dot, Distribution.random(40, (-6.5, -3.0, 6.5, 2.6), seed=7))
links = scene.geometry.connect(cloud, max_distance=2.2, mode="range").stroke(CYAN, 0.025)
chain = scene.geometry.group([scene.geometry.dot(0.08).fill(WHITE).move_to(x, -3.6) for x in range(-6, 7, 2)])
path = scene.geometry.connect(chain, max_distance=10.0, mode="sequential", fade_by_distance=False).stroke(WHITE, 0.03)

scene.play([cloud.animate.rotate_by(0.6).duration(2.0)])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The links follow the points while the cloud turns.
    scene.snapshots(snapshots, [0.0, 1.0, 2.2])
else:
    scene.render()
