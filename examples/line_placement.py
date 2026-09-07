"""Length-based lines: orientation and next_to placement are independent."""
import os

from gaanim import Direction, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
title = scene.text("Lines by length", size=0.6).fill(WHITE).move_to(0, 1.6)
rule = (
    scene.geometry.line(length=5)
    .stroke(GOLD, 0.04)
    .next_to(title, Direction.DOWN, spacing=0.3)
)
card = scene.geometry.rect(4, 1.5).no_fill().stroke(WHITE, 0.025).move_to(0, -1.2)
vertical = (
    scene.geometry.line(length=1.5, direction=Direction.UP)
    .stroke(GOLD, 0.04)
    .next_to(card, Direction.LEFT, spacing=0.3)
)
scene.wait(0.5)
scene.play([rule.animate.create(), vertical.animate.create()], duration=1)
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 2.0])
scene.render()
