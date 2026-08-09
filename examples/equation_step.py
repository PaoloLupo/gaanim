"""A derivation step that preserves the glyphs shared by both equations."""

import os

from gaanim import BLACK, GOLD, WHITE, Scene


scene = Scene(1920, 1080, background=BLACK)
title = scene.title("Resolver paso a paso").fill(WHITE).at(0, 220)
before = scene.equation(
    "x dot 5 = 25",
    tags={"variable": "x", "result": "25"},
).at(0, 0).scaled(2)
after = scene.equation(
    "x = 5",
    tags={"variable": "x", "result": "5"},
).at(0, 0).scaled(2)
before.tag("result").fill(GOLD)
after.tag("result").fill(GOLD)

scene.play([title.write(), before.write()])
scene.wait(0.4)
current = scene.step_equation(before, after, duration=0.8)
current.tag("result").indicate(duration=0.45)
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.4, 1.8, 2.2, 2.65])

scene.render()
