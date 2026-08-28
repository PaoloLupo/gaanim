"""Scale-aware 3D axes: logarithmic, symlog, and power coordinates."""

import os

from gaanim import Anchor, Axis, BLACK, GOLD, RED, Scene

scene = Scene(frame=(16, 9), background=BLACK)
space = scene.viz.cartesian_3d(
    Axis.log(0.1, 1000, base=10).ticks(10).label("log x"),
    Axis.symlog(-100, 100, base=10, threshold=1).ticks(10).label("symlog y"),
    Axis.power(0, 16, 0.5).ticks(2).label("sqrt z"),
    size=(0.125, 0.1, 0.075),
)
curve = space.parametric(
    lambda t: (10 ** (3 * t - 1), 80 * (2 * t - 1), 16 * t * t), (0, 1)
).stroke(RED, 0.0375)
title = scene.text("Escalas no lineales en Cartesian3D").fill(GOLD).hud().move_to(0, 3.875, anchor=Anchor.CENTER)

scene.camera.perspective(fov_y=0.785, near=0.1, far=1000)
scene.camera.look_at(eye=(11, 9.5, 11), target=(0, 0, 0))
scene.play([space.layer("grid").animate.fade_in().duration(0.5), space.layer("axes").animate.create().duration(0.7)])
scene.play([space.layer("ticks").animate.fade_in().duration(0.4), space.layer("numbers").animate.write().duration(0.5), title.animate.write().duration(0.5)])
scene.play([curve.animate.create().duration(1.0)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.6, 1.2, 2.0])
else:
    scene.render()
