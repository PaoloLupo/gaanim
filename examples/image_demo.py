"""Raster image mobjects: fitting, cropping, transforms, and texture reuse."""

import os
from pathlib import Path

from gaanim import Anchor, GOLD, WHITE, Scene


scene = Scene(frame=(16, 9))
source = (
    Path(__file__).resolve().parents[1]
    / "tests"
    / "visual"
    / "transform_demo"
    / "baseline"
    / "seek_0008_t_12_900000.png"
)

title = scene.text("ImageMobject", role="title").fill(WHITE).move_to(0, 3.666667, anchor=Anchor.CENTER)
# Loading the same path repeatedly reuses the process-local decoded texture cache.
contain = scene.media.image(str(source), width=4.166667, height=2.5, fit="contain").move_to(-5, 0.333333)
cover = scene.media.image(str(source), width=4.166667, height=2.5, fit="cover").move_to(0, 0.333333)
crop = (
    scene.media.image(
        str(source),
        width=4.166667,
        height=2.5,
        fit="stretch",
        crop=(360, 190, 960, 540),
    )
    .move_to(5, 0.333333)
    .opacity(0.78)
    .rotate_to(-0.08)
)
caption = scene.text("contain • cover • crop + stretch").fill(GOLD).move_to(0, -3.416667, anchor=Anchor.CENTER)

scene.play([title.animate.write().duration(0.6), contain.animate.fade_in().duration(0.8), cover.animate.fade_in().duration(0.8), crop.animate.fade_in().duration(0.8)])
scene.play([contain.animate.shift_by(0.333333, 0).duration(0.7), crop.animate.rotate_by(0.16).duration(0.7)])
scene.play([caption.animate.write().duration(0.5)])
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.6, 1.4, 2.1, 2.4])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
