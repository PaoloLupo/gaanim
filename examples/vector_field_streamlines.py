"""One reusable field, rendered as arrows and deterministic streamlines."""

import os

from gaanim import Axis, ColorMap, Scene


scene = Scene(frame=(16, 9))
scene.canvas.set_theme("technical")
plane = scene.viz.cartesian_2d(
    Axis.linear(-4.5, 4.5).ticks(1).label("x", position="top"),
    Axis.linear(-2.8, 2.8).ticks(1).label("y", position="top"),
    width=13,
    height=7.5,
)

# The callback receives coordinates directly and is evaluated from a stable
# numeric snapshot whenever an explicit input changes.
vortex = plane.field(lambda x, y: (-y - 0.12 * x, x - 0.12 * y))

arrows = vortex.arrows(
    resolution=(19, 13),
    max_length=34,
    width=0.0275,
    colormap=ColorMap("batlow"),
)
streams = vortex.streamlines(
    seeds=(18, 12),
    tolerance=1e-5,
    max_time=4.0,
    separation=0.045,
    width=0.0375,
    colormap="vik",
)
particles = vortex.particles(
    24,
    radius=0.05,
    duration=3.0,
    max_time=3.5,
    colormap="batlow",
)

scene.play(
    [
        plane.animate.write().duration(1.0),
        arrows.animate.write().duration(1.4),
        streams.animate.create().duration(1.4),
        particles.animate.create().duration(1.0),
    ]
)
scene.play(streams.flow(3.0, time_width=0.14) + particles.flow())
scene.wait(0.5)
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.4, 1.41, 2.2, 4.4, 4.9])
scene.render()
