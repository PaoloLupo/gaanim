"""Progress rings and a countdown timer driven by one parameter each."""

import os

from gaanim import CORAL, GOLD, GRAY, WHITE, Easing, Scene


scene = Scene(frame=(16, 9), background="#0f172a")
title = scene.text("Anillos de progreso", role="title").fill(WHITE).move_to(0, 3.6)

upload = scene.viz.progress_ring(0.0, radius=1.4, width=0.16).move_to(-4.5, 0.2)
quality = scene.viz.progress_ring(0.35, radius=1.4, width=0.16, color=GOLD, decimals=1).move_to(0, 0.2)
timer = scene.viz.countdown(3, radius=1.4, width=0.16, color=CORAL).move_to(4.5, 0.2)
for x, label in [(-4.5, "animate.set"), (0.0, "decimals=1"), (4.5, "countdown(3)")]:
    scene.text(label).fill(GRAY).scale_by(0.5).move_to(x, -2.0)

scene.play([
    upload.animate.set(0.75).duration(1.5).easing(Easing.SMOOTH),
    quality.animate.set(0.925).duration(1.5),
    timer.count_down(),
])
scene.play([upload.animate.set(1.0).duration(0.6)])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Start, mid-fill, the timer's last second, and the finished rings.
    scene.snapshots(snapshots, [0.0, 0.75, 2.5, 3.8])
else:
    scene.render()
