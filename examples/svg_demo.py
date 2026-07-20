"""SVG import: resolved vector paths remain normal animatable mobjects."""

import os
from pathlib import Path

from gaanim import GOLD, WHITE, Scene


scene = Scene(960, 540)
asset = Path(__file__).resolve().parents[1] / "tests" / "assets" / "svg_demo.svg"

title = scene.title("SVG vector import").fill(WHITE).at(0, 220)
art = scene.svg(str(asset)).scaled(1.35).at(0, -15)
caption = scene.text("paths, fills, strokes, transforms and <use>").fill(GOLD).at(0, -205)

scene.play([title.write(0.6)])
scene.wait(0.8)
scene.play([caption.write(0.45)])
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.6, 0.9, 1.4, 2.05])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
