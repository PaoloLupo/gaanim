"""Reactive editorial callout that follows an animated mobject."""

import os

from gaanim import Easing, BLACK, BLUE, NAVY, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK, margin=0.7)

title = scene.text("Reactive callouts", role="title").fill(WHITE).move_to(0, 3.125)
rail = scene.geometry.line(-5.25, -1.125, 5.25, -1.125).stroke(WHITE, 0.05)
mass = scene.geometry.dot(0.35).fill(BLUE).stroke(WHITE, 0.0375).move_to(-3.75, -1.125)

callout = scene.slides.callout(
    "Moving mass",
    mass,
    offset=(2.125, 1.475),
    width=2.875,
    height=0.875,
    background=NAVY,
    color=WHITE,
)

scene.play([
    title.animate.write().duration(0.6),
    rail.animate.create().duration(0.6),
    mass.animate.grow_from_center().duration(0.5),
])
scene.play([mass.animate.shift_by(7.5, 0).duration(2.0).easing(Easing.SMOOTH), callout.animate.fade_in()])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 2.2, 3.0])
else:
    scene.render()
