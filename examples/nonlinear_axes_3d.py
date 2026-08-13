"""Scale-aware 3D axes: logarithmic, symlog, and power coordinates."""

import os

from gaanim import Anchor, Axis, BLACK, GOLD, Scene

scene = Scene(1280, 720, background=BLACK)
space = scene.cartesian_3d(
    Axis.log(0.1, 1000, base=10).ticks(10).label("log x"),
    Axis.symlog(-100, 100, base=10, threshold=1).ticks(10).label("symlog y"),
    Axis.power(0, 16, 0.5).ticks(2).label("sqrt z"),
    size=(10, 8, 6),
)
curve = space.parametric(lambda t: (10 ** (3 * t - 1), 80 * (2 * t - 1), 16 * t * t), (0, 1))
title = scene.text("Escalas no lineales en Cartesian3D").fill(GOLD).hud().at(0, 310, anchor=Anchor.CENTER)

scene.camera.perspective(fov_y=0.785, near=0.1, far=1000, duration=0.0)
scene.camera.look_at(eye=(8, 7, 8), target=(0, 0, 0), duration=0.0)
scene.play([space.layer("grid").fade_in(0.5), space.layer("axes").create(0.7)])
scene.play([space.layer("ticks").fade_in(0.4), space.layer("numbers").write(0.5), title.write(0.5)])
scene.play([curve.create(1.0)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.6, 1.2, 2.0])
else:
    scene.render()
