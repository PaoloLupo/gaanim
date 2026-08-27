"""Light/dark editorial catalog for the built-in mechanism supports."""

import os

from gaanim import BLACK, Direction, Scene, WHITE


scene = Scene(1280, 720, background=BLACK, margin=48, theme="technical")
scene.play([scene.text("Apoyos mecánicos editoriales", role="title").to_edge(Direction.UP).animate.write()])

kinds = ["fixed", "pin", "roller", "simple", "guided", "prismatic", "cable", "spring"]
for index, kind in enumerate(kinds):
    column = index % 4
    row = index // 4
    x = -450 + column * 300
    y = 150 - row * 270
    point = (x, y)
    support = scene.mechanics.support_at(point, kind=kind, direction=Direction.UP, size=54)
    label = scene.text(kind, role="caption").fill(WHITE).follow(point, offset=(0, -118))
    scene.play([support.animate.fade_in().duration(0.2), label.animate.write().duration(0.2)])

scene.wait(0.5)
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 2.0, 3.0])
else:
    scene.render()
