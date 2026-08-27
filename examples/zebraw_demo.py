"""Typst Universe packages are available from Gaanim's embedded world."""

import os

from gaanim import BLACK, GRAY, WHITE, Direction, Scene


scene = Scene(1920, 1080, background=WHITE, margin=56)

title = scene.text("Typst Universe package", role="title").fill(BLACK).at(0, 250)
subtitle = scene.text("@preview/zebraw:0.6.3 resolved by the embedded world", role="subtitle").fill(GRAY).at(0, 195)
layout = scene.text.typst('''
#import "@preview/zebraw:0.6.3": *
#show: zebraw.with(
  lang: true,
  lang-color: rgb("5b8fc9"),
  numbering-font-args: (fill: rgb("94a3b8")),
)

```typ
#grid(
  columns: (1fr, 1fr),
  [Hello], [world!],
)
```
''',width=200).scaled(3).at(0, -35)

scene.play([
    title.write(),
    subtitle.fade_in_from(Direction.DOWN, distance=24).duration(0.45),
    # layout.fade_in_from(Direction.DOWN, distance=30).duration(0.65),
    layout.write(3)
])
scene.wait(0.35)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
