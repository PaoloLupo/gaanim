"""A live highlight frame moving between semantic equation parts."""

import os

from gaanim import GOLD, WHITE, Scene, part


scene = Scene(720, 405, background="#0f172a")

energy = scene.text.equation(
    "E =",
    part("mass", "m"),
    part("light", "c^2"),
).fill(WHITE).at(0, 70)
momentum = scene.text.equation(
    "p =",
    part("mass", "m"),
    part("velocity", "v"),
).fill(WHITE).at(0, -80)

frame = scene.geometry.surrounding_rect(energy["mass"]).stroke(GOLD, 4)

scene.play([energy.fade_in(), momentum.fade_in(), frame.create()])
scene.play([frame.retarget(energy["light"], duration=0.8).smooth()])
scene.play([frame.retarget(momentum["velocity"], duration=1.0).spring()])
scene.play([momentum.move(110, 0).duration(0.8)])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 1.8, 2.8, 3.6, 3.85])
else:
    scene.render()
