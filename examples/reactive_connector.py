"""A single arrow follows card anchors and relative return waypoints."""
import os
from gaanim import Anchor, Scene

scene = Scene(frame=(16, 9))
scene.canvas.set_theme("paper")
left = scene.geometry.rect(3, 1.5).move_to(-4, 0).no_fill().stroke("#626878", 0.025)
right = scene.geometry.rect(3, 1.5).move_to(4, 0).no_fill().stroke("#626878", 0.025)
arrow = scene.geometry.connector(left.anchor_point(Anchor.RIGHT), right.anchor_point(Anchor.LEFT)).fill("#e26d5c")
loop = scene.geometry.connector(
    right.anchor_point(Anchor.BOTTOM), left.anchor_point(Anchor.BOTTOM),
    via=[right.anchor_point(Anchor.BOTTOM, offset=(0, -1.5)), left.anchor_point(Anchor.BOTTOM, offset=(0, -1.5))],
).fill("#b7791f")
scene.play([left.animate.fade_in(), right.animate.fade_in()], duration=0.2)
scene.play([arrow.animate.create(), loop.animate.create()], duration=0.8)
scene.play(right.animate.shift_by(-1, 1), duration=1)
scene.wait(0.5)
if path := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(path, [0.8, 1.5, 2.2, 1.5])
scene.render()
