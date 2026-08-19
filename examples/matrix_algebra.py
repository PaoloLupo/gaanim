import os

from gaanim import GOLD, Scene

scene = Scene(960, 540, background="#0f172a", margin=48)
a = scene.matrix([[1, 2], [3, 4]], delimiters="parentheses").at(-220, 70)
b = scene.matrix([[2, 0], [1, 2]], delimiters="parentheses").at(40, 70)

product = a.matmul(b)
product.result.at(260, 70)
a.row(0).fill(GOLD)
b.column(0).fill(GOLD)
scene.play(product.animate(duration=0.5, stagger=0.08))

rref = a.rref()
rref.result.at(0, -130)
scene.play(rref.animate(duration=0.45, order="main_diagonal", stagger=0.06))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.369])

scene.render()
