"""One reusable field, rendered as arrows and deterministic streamlines."""

import os

from gaanim import Axis, ColorMap, Scene


scene = Scene(1280, 720)
scene.canvas.set_theme("technical")
plane = scene.cartesian_2d(
    Axis.linear(-4.5, 4.5).ticks(1).label("x", position="top"),
    Axis.linear(-2.8, 2.8).ticks(1).label("y", position="top"),
    width=1040,
    height=600,
)

# The callback receives coordinates directly and is evaluated from a stable
# numeric snapshot whenever an explicit input changes.
vortex = plane.field(lambda x, y: (-y - 0.12 * x, x - 0.12 * y))

arrows = vortex.arrows(
    resolution=(19, 13),
    max_length=34,
    width=2.2,
    colormap=ColorMap("batlow"),
)
streams = vortex.streamlines(
    seeds=(18, 12),
    tolerance=1e-5,
    max_time=4.0,
    separation=0.045,
    width=3.0,
    colormap="vik",
)
particles = vortex.particles(
    24,
    radius=4,
    duration=3.0,
    max_time=3.5,
    colormap="batlow",
)

scene.play(
    [
        plane.write(1.0),
        arrows.write(1.4),
        streams.create(1.4),
        particles.create(1.0),
    ]
)
scene.play(streams.flow(3.0, time_width=0.14) + particles.flow())
scene.wait(0.5)
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.4, 1.41, 2.2, 4.4, 4.9])
scene.render()
