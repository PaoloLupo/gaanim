"""Reuse a layout-managed child, detach it, and position it freely."""

import os

from gaanim import BLUE, GOLD, WHITE, Scene, Transition


scene = Scene(frame=(16, 9), background="#0f172a", margin=0.6)
scene.segment("cover")

title = scene.text("Reusable layout title", role="title").fill(GOLD)
subtitle = scene.text("Initially positioned by a responsive column").fill(WHITE)
marker = scene.geometry.circle(0.4).fill(BLUE)

page = scene.layout.column(
    [title, subtitle, marker],
    within="safe",
    width="fill",
    height="fill",
    gap=0.35,
    align="center",
    justify="center",
)
scene.play([title.animate.write().duration(0.5), subtitle.animate.fade_in().duration(0.4)])
scene.wait(0.4)

scene.segment("detail", Transition.cross_fade(0.5))
scene.reuse(title)
page.detach(title)
scene.play([title.animate.move_to(0, 3).duration(0.4)])
scene.wait(0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.25, 0.9, 1.3, 1.75, 2.1])
else:
    scene.render()
