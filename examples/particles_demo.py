"""Deterministic particles: sparks trail a moving star, burst, then confetti falls.

Every particle is a closed-form function of the timeline time, so the
snapshots below match continuous playback exactly.
"""

import math
import os

from gaanim import GOLD, ORANGE, RED, WHITE, Brush, Emitter, Scene

scene = Scene(frame=(16, 9), background="#0b1020")

logo = scene.geometry.star(5, 0.6, 0.25).fill(GOLD).move_to(-4, 0)

# Sparks leave from where the star was at their birth, so they trail it.
sparks = scene.fx.particles(
    emitter=Emitter.circle(radius=0.2).at(logo),
    rate=60, lifetime=(0.6, 1.2), speed=(2.0, 4.0), spread=math.tau,
    gravity=(0, -3), drag=0.8, size=(0.03, 0.08), shape="streak",
    color=Brush.linear([GOLD, ORANGE, RED], start=(0, 0), end=(1, 0)), seed=9,
)
# Slow dust that shrinks away, from a strip along the floor.
dust = scene.fx.particles(
    Emitter.rect(14, 0.2).at((0, -4)), rate=25, lifetime=(2.0, 3.0),
    speed=(0.2, 0.6), spread=0.8, size=(0.04, 0.09), size_end=0.0,
    color=WHITE, fade=0.5, seed=4,
).opacity(0.35)

scene.play([logo.animate.move_to(3, 1).duration(2.0)])
scene.play([sparks.animate.burst(80)])
confetti = scene.fx.confetti(origin=(0, -4), count=120, seed=2)
scene.wait(3.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.5, 1.0, 2.0, 2.3, 3.2, 4.0, 5.0, 6.0])
else:
    scene.render()
