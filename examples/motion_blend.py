"""Per-object blend modes: light leaks screen over a panel, ink multiplies into it."""

import os

from gaanim import Scene


scene = Scene(frame=(16, 9), background="#0f172a")
panel = scene.geometry.rounded_rect(12.0, 5.0, 0.3).fill("#2563eb").move_to(0, -0.5)
title = scene.text("Modos de fusión", role="title").fill("#e2e8f0").move_to(0, 3.4)

modes = ["normal", "screen", "add", "multiply", "overlay", "difference"]
swatches = []
for index, mode in enumerate(modes):
    x = -5.0 + 2.0 * index
    scene.text(mode, size=0.32).fill("#e2e8f0").move_to(x, -3.6)
    swatch = scene.geometry.circle(0.8).fill("#f59e0b").move_to(x, 2.6).blend(mode)
    swatches.append(swatch)

# A group blends all of its members, like fill.
leaks = scene.geometry.group([
    scene.geometry.circle(1.4).fill("#f97316").move_to(-3.0, -0.4),
    scene.geometry.circle(1.1).fill("#ec4899").move_to(-1.6, -1.2),
]).blend("screen")

scene.wait(0.3)
scene.play([swatch.animate.move_to(-5.0 + 2.0 * i, -0.5).duration(1.0) for i, swatch in enumerate(swatches)])
scene.play([leaks.animate.shift_by(6.0, 0.0).duration(1.2)])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Swatches above the panel, swatches over it, and the leaks after crossing it.
    scene.snapshots(snapshots, [0.2, 1.4, 2.0, 3.0])
else:
    scene.render()
