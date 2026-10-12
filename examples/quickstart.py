"""A first scene: a circle bounces in, steps aside and a greeting is written beside it.

The frame is 16 by 9 units with the origin at its center. Unstyled shapes take
the theme's accent and text its foreground. Each ``play`` runs its animations
together and moves the timeline cursor to their end; ``render`` opens the result.
"""

import os

from gaanim import Easing, Scene

scene = Scene()

circle = scene.geometry.circle(1.5)
greeting = scene.text("Hello, Gaanim", size=1.2).move_to(1.8, 0)

scene.play(circle.animate.grow_from_center().easing(Easing.BOUNCY))
scene.play(circle.animate.move_to(-4.5, 0))
scene.play(greeting.animate.write())
scene.play(circle.animate.indicate())
scene.wait(1)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.4, 1.5, 2.5, 3.4, 4.5])
else:
    scene.render()
