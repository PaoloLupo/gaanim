"""Multi-channel keyframes: one clip with several stops and an easing per segment."""

import os

from gaanim import BLUE, CORAL, GOLD, TEAL, WHITE, Easing, EasingCurve, Scene


scene = Scene(frame=(16, 9))
scene.text("Keyframes", role="title").fill(WHITE).move_to(0, 3.3)

# A ball that hops through three stops on a smooth spatial curve, squashing
# on the landing stop; its first position is wherever it already is.
ball = scene.geometry.circle(0.45).fill(GOLD).move_to(-5.5, -1.5)
hop = ball.animate.keyframes(
    times=[0.0, 0.35, 0.7, 1.0],
    position=[None, (-2.0, 1.5), (1.0, -1.5), (5.0, -1.5)],
    scale=[1.0, 1.0, (1.35, 0.7), 1.0],
    easing=[
        Easing.ease_out(EasingCurve.QUADRATIC),
        Easing.ease_in(EasingCurve.QUADRATIC),
        Easing.SMOOTH,
    ],
    spatial="catmull_rom",
).duration(1.6)

# Rotation, opacity and fill on one clip, linear segments by default.
card = scene.geometry.rounded_rect(1.6, 1.0, 0.15).fill(BLUE).move_to(-4.0, 1.8)
spin = card.animate.keyframes(
    times=[0.0, 0.5, 1.0],
    rotation=[0.0, 1.2, 0.0],
    opacity=[1.0, 0.35, 1.0],
    fill=[BLUE, CORAL, TEAL],
).duration(1.6)

# A parameter keyframed through a peak, driving a dot's scale.
level = scene.viz.parameter(0.4)
dot = scene.geometry.circle(0.5).fill(CORAL).move_to(4.0, 1.8)
dot.scale_to(level)
pulse = level.animate.keyframes(
    times=[0, 0.3, 1], values=[None, 1.6, 0.8], easing=Easing.SMOOTH
).duration(1.6)

scene.play([hop, spin, pulse])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # First stop, mid-flight, the squash, landed.
    scene.snapshots(snapshots, [0.0, 0.3, 0.56, 0.8, 1.12, 1.6, 2.0])
else:
    scene.render()
