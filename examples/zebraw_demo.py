"""Typst Universe packages are available from Gaanim's embedded world."""

import os

from gaanim import BLACK, GRAY, WHITE, Direction, Scene


scene = Scene(frame=(16, 9), background=WHITE, margin=0.466667)

title = scene.text("Typst Universe package", role="title").fill(BLACK).move_to(0, 2.083333)
subtitle = scene.text("@preview/zebraw:0.6.3 resolved by the embedded world", role="subtitle").fill(GRAY).move_to(0, 1.625)
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
''',width=1.666667).scale_to(3).move_to(0, -0.291667)

scene.play([
    title.animate.write(),
    subtitle.animate.fade_in_from(Direction.DOWN, distance=0.2).duration(0.45),
    # layout.animate.fade_in_from(Direction.DOWN, distance=30).duration(0.65),
    layout.animate.write().duration(3)
])
scene.wait(0.35)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
