import os

from gaanim import GOLD, Scene

scene = Scene(960, 540, background="#0f172a", margin=48)
a = scene.viz.matrix([[1, 2], [3, 4]], delimiters="parentheses").move_to(-220, 70)
b = scene.viz.matrix([[2, 0], [1, 2]], delimiters="parentheses").move_to(40, 70)

product = a.matmul(b)
product.result.move_to(260, 70)
a.row(0).fill(GOLD)
b.column(0).fill(GOLD)
scene.play(product.animations(stagger=0.08).duration(0.5))

rref = a.rref()
rref.result.move_to(0, -130)
scene.play(rref.animations(order="main_diagonal", stagger=0.06).duration(0.45))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.369])

scene.render()
