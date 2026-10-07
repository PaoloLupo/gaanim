"""Text on a path (TX-06): a ring label that spins and a caption riding a wave."""

import os

from gaanim import GOLD, GRAY, WHITE, Easing, Scene


scene = Scene(frame=(16, 9))

circle = scene.geometry.circle(2.2).no_fill().stroke(GRAY, 0.03).move_to(-3.6, 0.4)
ring = scene.text.on_path(
    "GAANIM · MOTION · DESIGN · ", circle, reverse=True, size=0.42, color=GOLD
)

wave = scene.geometry.path(
    [("move", [(0.6, -1.0)]), ("cubic", [(2.6, 2.4), (4.6, -3.4), (7.2, 0.6)])]
).no_fill().stroke(GRAY, 0.02)
caption = scene.text.on_path("una ola de texto", wave, align="center", size=0.4, color=WHITE)
caption.path_offset(-0.25)
upright = scene.text.on_path("sin girar", wave, orient=False, offset=0.7, size=0.3, color=GRAY)

scene.wait(0.2)
scene.play(
    [
        ring.animate.path_offset(1.0).duration(3).easing(Easing.LINEAR),
        caption.animate.path_offset(0.25).duration(3),
    ]
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 2.2, 3.2])
else:
    scene.render()
