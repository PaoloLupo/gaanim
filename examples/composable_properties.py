"""Chained reactive values, pure custom motion and animated gradient paints."""

import math
import os

from gaanim import BLUE, GOLD, WHITE, Brush, Easing, Scene, computed, parallel, sequence

scene = Scene(frame=(16, 9), margin=0.5)
phase = scene.viz.parameter(-2.0)
height = computed(lambda x: 0.25*x*x - 1, inputs=[phase])
brightness = computed(lambda y: 0.65 + 0.2*(y + 1), inputs=[height])
dot = scene.geometry.circle(0.25).fill(GOLD).move_to(phase, height).opacity(brightness)
target = scene.geometry.circle(0.35).fill(BLUE).move_to(3, 1)
gradient = Brush.linear([BLUE, GOLD, WHITE], start=(-0.35, 0), end=(0.35, 0))

scene.play(phase.animate.set(2).duration(2).easing(Easing.LINEAR))
# Numeric setters end their bindings, reversibly, at this cursor.
dot.move_to(2, 0).opacity(1)
custom = dot.animate.custom(
    lambda alpha: {
        "position": (2 + math.sin(math.pi*alpha), alpha),
        "opacity": 1 - 0.35*alpha,
    },
    channels=("position", "opacity"),
).duration(2).easing(Easing.SMOOTH)
scene.play(sequence(
    parallel(custom, target.animate.fill(gradient).duration(2)),
    dot.animate.move_to(target).opacity(1).duration(1),
))
scene.wait(0.5)

if "GAANIM_SNAPSHOTS" in os.environ:
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0, 0.5, 1, 2, 3, 4, 5])
scene.render()
