import os

from gaanim import BLUE, GOLD, Scene

scene = Scene(1920, 1080, background="#0f172a", margin=48)
matrix = scene.matrix(
    [[1, 2, 3], [4, 5, 6], [7, 8, 9]],
    delimiters="brackets",
    row_labels=["r_1", "r_2", "r_3"],
    column_labels=["c_1", "c_2", "c_3"],
).at(0, 0)

matrix.diagonal().fill(GOLD)
scene.play(matrix.entries.write(0.45, order="spiral_in", stagger=0.06))
scene.play(matrix.row(1).animate(order="column_major", stagger=0.05).color(BLUE).duration(0.5))
scene.play(matrix[:, 0].indicate(0.5, stagger=0.08))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.8, 2.19])

scene.render()
