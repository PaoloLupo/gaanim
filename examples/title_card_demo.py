"""Opening title card followed by regular scene content."""

import os

from gaanim import BLACK, BLUE, WHITE, Direction, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

opening = scene.slides.title_card(
    "Vector motion",
    "A short technical explanation",
    accent=BLUE,
)

scene.play([opening.fade_in_from(Direction.DOWN, distance=56).duration(0.7)])
scene.wait(1.0)
scene.play([opening.fade_out().duration(0.45)])

circle = scene.geometry.circle(92).fill(BLUE).stroke(WHITE, 4).at(-180, -30)
label = scene.text("The main scene begins").fill(WHITE).at(150, -30)
scene.play([
    circle.create().duration(0.6),
    label.write().duration(0.6),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.6, 1.7, 2.5])
else:
    scene.render()
