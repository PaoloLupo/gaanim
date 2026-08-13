"""SVG import: resolved vector paths remain normal animatable mobjects."""

import os
from pathlib import Path

from gaanim import Anchor, GOLD, NAVY, Scene


scene = Scene(960, 540)
asset = Path(__file__).resolve().parents[1] / "tests" / "assets" / "svg_demo.svg"

title = scene.text("SVG vector import", role="title").fill(NAVY).at(0, 220, anchor=Anchor.CENTER)
art = scene.svg(str(asset)).scaled(1.35).at(0, -15)
orb = art.part("orb")
top_spark = art.part("spark-top")
caption = scene.text("gradients, text outlines, clipPath, filters and <use>").fill(GOLD).at(0, -205, anchor=Anchor.CENTER)

scene.wait(0.4)
scene.play([orb.indicate(0.8), top_spark.rotate(0.8).duration(0.8)])
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.4, 0.6, 0.8, 1.2, 1.6])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
