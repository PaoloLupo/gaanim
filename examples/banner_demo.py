"""Safe-area-aware banner component."""

import os

from gaanim import BLACK, BLUE, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK, margin=0.7)

title = scene.text("Banners and safe areas", role="title").fill(WHITE).move_to(0, 1.75)
accent = scene.geometry.circle(1.075).fill(BLUE).move_to(0, -0.5)
banner = scene.slides.banner(
    "This banner stays inside the safe area and grows with wrapped text.",
    position="bottom",
    width=9.5,
    variant="accent",
)

scene.play([
    title.animate.write().duration(0.7),
    accent.animate.grow_from_center().duration(0.6),
])
scene.play([banner.animate.fade_in().duration(0.35)])
scene.wait(1.2)
scene.play([banner.animate.fade_out().duration(0.35)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.4, 2.5])
else:
    scene.render()
