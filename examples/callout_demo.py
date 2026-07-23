"""Reactive editorial callout that follows an animated mobject."""

import os

from gaanim import BLACK, BLUE, GOLD, NAVY, WHITE, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

title = scene.title("Reactive callouts").fill(WHITE).at(0, 250)
rail = scene.line(-420, -90, 420, -90).stroke(WHITE, 4)
mass = scene.dot(28).fill(GOLD).stroke(WHITE, 3).at(-300, -90)

callout = scene.callout(
    "Moving mass",
    mass,
    offset=(170, 118),
    width=230,
    height=70,
    background=NAVY,
    color=WHITE,
)

scene.play([
    title.write().duration(0.6),
    rail.create().duration(0.6),
    mass.grow_from_center().duration(0.5),
])
scene.play([mass.move(600, 0).duration(2.0).smooth()])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 2.2, 3.0])
else:
    scene.render()
