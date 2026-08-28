"""Opening title card followed by regular scene content."""

import os

from gaanim import BLACK, BLUE, WHITE, Direction, Scene


scene = Scene(frame=(16, 9), background=BLACK, margin=0.7)

opening = scene.slides.title_card(
    "Vector motion",
    "A short technical explanation",
    accent=BLUE,
)

scene.play([opening.animate.fade_in_from(Direction.DOWN, distance=0.7).duration(0.7)])
scene.wait(1.0)
scene.play([opening.animate.fade_out().duration(0.45)])

circle = scene.geometry.circle(1.15).fill(BLUE).stroke(WHITE, 0.05).move_to(-2.25, -0.375)
label = scene.text("The main scene begins").fill(WHITE).move_to(1.875, -0.375)
scene.play([
    circle.animate.create().duration(0.6),
    label.animate.write().duration(0.6),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.6, 1.7, 2.5])
else:
    scene.render()
