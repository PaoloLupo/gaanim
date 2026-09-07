"""Nested content, live backgrounds, and named connector ports."""
import os
from gaanim import Anchor, Scene

scene = Scene(frame=(16, 9))
scene.canvas.set_theme("paper")
ports = {"in": Anchor.LEFT, "out": (Anchor.RIGHT, (0.1, 0))}
left = scene.layout.card(
    [scene.geometry.rect(1, 0.5).fill("#e26d5c").no_stroke()],
    padding=0.3, background="#eeeeee", border="#626878", ports=ports,
).move_to(-3, 0)
right = scene.layout.card(
    [scene.geometry.circle(0.3).fill("#b7791f").no_stroke()],
    padding=0.3, background="#eeeeee", border="#626878", ports=ports,
).move_to(3, 0)
link = scene.geometry.connector(left.port("out"), right.port("in")).fill("#e26d5c")
scene.play([left.animate.fade_in(), right.animate.fade_in(), link.animate.create()], duration=0.5)
scene.play(right.animate.shift_by(0, 1), duration=0.5)
left.add(scene.geometry.rect(2, 0.4).fill("#b7791f").no_stroke())
scene.wait(0.5)
if path := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(path, [0.5, 1.25, 0.5, 1.25])
scene.render()
