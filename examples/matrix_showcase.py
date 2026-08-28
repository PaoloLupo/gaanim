import os

from gaanim import BLUE, GOLD, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.4)
matrix = scene.viz.matrix(
    [[1, 2, 3], [4, 5, 6], [7, 8, 9]],
    delimiters="brackets",
    row_labels=["r_1", "r_2", "r_3"],
    column_labels=["c_1", "c_2", "c_3"],
).move_to(0, 0)

matrix.diagonal().fill(GOLD)
scene.play(matrix.entries.animate.write(order="spiral_in", stagger=0.06).duration(0.45))
scene.play(matrix.row(1).animate.fill(BLUE, order="column_major", stagger=0.05).duration(0.5))
scene.play(matrix[:, 0].animate.indicate(stagger=0.08).duration(0.5))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.8, 2.19])

scene.render()
