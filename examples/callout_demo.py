"""Reactive editorial callout that follows an animated mobject."""

import os

from gaanim import BLACK, BLUE, NAVY, WHITE, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

title = scene.text("Reactive callouts", role="title").fill(WHITE).move_to(0, 250)
rail = scene.geometry.line(-420, -90, 420, -90).stroke(WHITE, 4)
mass = scene.geometry.dot(28).fill(BLUE).stroke(WHITE, 3).move_to(-300, -90)

callout = scene.slides.callout(
    "Moving mass",
    mass,
    offset=(170, 118),
    width=230,
    height=70,
    background=NAVY,
    color=WHITE,
)

scene.play([
    title.animate.write().duration(0.6),
    rail.animate.create().duration(0.6),
    mass.animate.grow_from_center().duration(0.5),
])
scene.play([mass.animate.shift_by(600, 0).duration(2.0).smooth(), callout.animate.fade_in()])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 2.2, 3.0])
else:
    scene.render()
