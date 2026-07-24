"""Typst document layouts and mathematical matrices as vector drawables."""

import os

from gaanim import BLACK, GRAY, WHITE, Direction, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

title = scene.title("Typst-native layouts").fill(WHITE).at(0, 260)
caption = scene.subtitle("Document table and mathematical matrix").fill(GRAY).at(0, 205)
table = scene.typst('''
#set text(size: 19pt)
#table(
  columns: (2fr, 1fr),
  stroke: rgb("5b7088"),
  inset: 10pt,
  table.header([*Method*], [*Error*]),
  [Baseline], [0.18],
  [GPU], [0.04],
)
''').at(-220, -35)
matrix = scene.equation("mat(1, 2; 3, 4)").at(285, -35)

scene.play([
    title.write().duration(0.55),
    caption.fade_in_from(Direction.DOWN, distance=24).duration(0.45),
    table.fade_in_from(Direction.DOWN, distance=30).duration(0.65),
    matrix.fade_in_from(Direction.DOWN, distance=30).duration(0.65),
])
scene.wait(0.35)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
