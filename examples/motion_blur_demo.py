"""Motion blur: sub-frame averaging smears fast motion, an exempt title stays sharp."""

import os

from gaanim import BLUE, CORAL, GOLD, WHITE, Scene


scene = Scene(frame=(16, 9), background="#101826")
scene.motion_blur(180, samples=12)

scene.segment("fast")
# The title moves too, but stays sharp: it is exempt from the blur.
title = scene.text("Motion blur", role="title").fill(WHITE).move_to(0, 3.2).motion_blur(False)
wheel = scene.geometry.rect(4.0, 0.35).fill(GOLD).move_to(-3.5, -0.5)
ball = scene.geometry.circle(0.5).fill(CORAL).move_to(-6.5, 1.4)
box = scene.geometry.square(1.0).fill(BLUE).move_to(4.0, -2.5)
scene.play(
    [
        ball.animate.move_to(6.5, 1.4).duration(0.6),
        wheel.animate.rotate_by(6.2832).duration(1.0),
        title.animate.shift_by(3.0, 0.0).duration(0.6),
    ]
)
scene.play([box.animate.move_to(-4.0, -2.5).duration(0.4)])
scene.segment("cut")
scene.reuse(title, wheel)
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The streaking ball and wheel beside the sharp title, the spinning
    # wheel alone, the sliding box, and a segment start that must not blend
    # the previous segment.
    scene.snapshots(snapshots, [0.3, 0.8, 1.2, 1.4])
else:
    scene.render()
