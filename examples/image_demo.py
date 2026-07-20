"""Raster image mobjects: native size, transforms, alpha, and texture reuse."""

import os
from pathlib import Path

from gaanim import GOLD, WHITE, Scene


scene = Scene(960, 540)
source = (
    Path(__file__).resolve().parents[1]
    / "tests"
    / "visual"
    / "transform_demo"
    / "baseline"
    / "seek_0008_t_12_900000.png"
)

title = scene.title("ImageMobject").fill(WHITE).at(0, 220)
# Loading the same path twice reuses the process-local decoded texture cache.
left = scene.image(str(source)).scaled(0.24).at(-235, -20)
right = scene.image(str(source)).scaled(0.14).at(235, -20).opacity(0.72).rotated(-0.15)
caption = scene.text("PNG texture • scale • opacity • rotation").fill(GOLD).at(0, -225)

scene.play([title.write(0.6), left.fade_in(0.8), right.fade_in(0.8)])
scene.play([left.move(40, 0).duration(0.7), right.rotate(0.3).duration(0.7)])
scene.play([caption.write(0.5)])
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.6, 1.4, 2.1, 2.4])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
