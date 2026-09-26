"""SVG import: resolved vector paths remain normal animatable mobjects."""

import os
from pathlib import Path

from gaanim import Anchor, GOLD, Scene


scene = Scene(frame=(16, 9))
asset = Path(__file__).resolve().parents[1] / "tests" / "assets" / "svg_demo.svg"

title = scene.text("SVG vector import", role="title").move_to(0, 3.666667, anchor=Anchor.CENTER)
# SVG imports at 100 px per unit: the 360x220 px asset is 3.6 units wide, 8.1 after 2.25x.
art = scene.media.svg(str(asset)).scale_to(2.25).move_to(0, -0.25)
orb = art.part("orb")
top_spark = art.part("spark-top")
caption = scene.text("gradients, text outlines, clipPath, filters and <use>").fill(GOLD).move_to(0, -3.416667, anchor=Anchor.CENTER)

scene.wait(0.4)
scene.play([orb.animate.indicate().duration(0.8), top_spark.animate.rotate_by(0.8).duration(0.8)])
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.4, 0.6, 0.8, 1.2, 1.6])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
