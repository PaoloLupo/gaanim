"""Raster image mobjects: fitting, cropping, transforms, and texture reuse."""

import os
from pathlib import Path

from gaanim import Anchor, GOLD, WHITE, Scene


scene = Scene(960, 540)
source = (
    Path(__file__).resolve().parents[1]
    / "tests"
    / "visual"
    / "transform_demo"
    / "baseline"
    / "seek_0008_t_12_900000.png"
)

title = scene.text("ImageMobject", role="title").fill(WHITE).at(0, 220, anchor=Anchor.CENTER)
# Loading the same path repeatedly reuses the process-local decoded texture cache.
contain = scene.media.image(str(source), width=250, height=150, fit="contain").at(-300, 20)
cover = scene.media.image(str(source), width=250, height=150, fit="cover").at(0, 20)
crop = (
    scene.media.image(
        str(source),
        width=250,
        height=150,
        fit="stretch",
        crop=(360, 190, 960, 540),
    )
    .at(300, 20)
    .opacity(0.78)
    .rotated(-0.08)
)
caption = scene.text("contain • cover • crop + stretch").fill(GOLD).at(0, -205, anchor=Anchor.CENTER)

scene.play([title.write(0.6), contain.fade_in(0.8), cover.fade_in(0.8), crop.fade_in(0.8)])
scene.play([contain.move(20, 0).duration(0.7), crop.rotate(0.16).duration(0.7)])
scene.play([caption.write(0.5)])
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.6, 1.4, 2.1, 2.4])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
