"""Lower-third caption component with safe-area-aware placement."""

import os

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

title = scene.text("Captions and lower thirds", role="title").fill(WHITE).at(0, 210)
accent = scene.circle(86).fill(BLUE).at(0, -20)
caption = scene.caption(
    "This caption stays inside the safe area and supports two lines of text.",
    position="bottom",
    width=760,
)

scene.play([
    title.write().duration(0.7),
    accent.grow_from_center().duration(0.6),
])
scene.play([caption.fade_in().duration(0.35)])
scene.wait(1.2)
scene.play([caption.fade_out().duration(0.35)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.4, 2.7])
else:
    scene.render()
