"""Typst document layouts and mathematical matrices as vector drawables."""

import os

from gaanim import Anchor, BLACK, GRAY, GREEN, WHITE, Direction, Scene


scene = Scene(1280, 720, background=WHITE, margin=56)

title = scene.text("Typst-native layouts", role="title").fill(GREEN).at(0, 260, anchor=Anchor.CENTER)
caption = scene.text("Document table and mathematical matrix", role="subtitle").fill(GRAY).at(0, 205, anchor=Anchor.CENTER)
table = scene.typst('''
    #import "@preview/simple-plot:1.0.0": plot


    // Showcases: custom tick labels, grid-label-break with pi notation
    #plot(
      xmin: -2.0 * calc.pi, xmax: 2.0 * calc.pi,
      ymin: -1.5, ymax: 1.5,
      width: 10, height: 5,
      xlabel: $x$,
      ylabel: $y$,
      show-grid: "major",
      show-origin: false,  // Avoid duplicate "0" with custom xtick-labels
      xtick: (-2.0*calc.pi, -calc.pi, calc.pi, 2.0*calc.pi),
      xtick-labels: ($-2 pi$, $-pi$, $pi$, $2 pi$),
      (fn: x => calc.sin(x), stroke: blue + 1.2pt, samples: 200, label: $sin(x)$, label-pos: 0.625, label-side: "above"),
      (fn: x => calc.cos(x), stroke: red + 1.2pt, samples: 200, label: $cos(x)$, label-pos: 1.0, label-side: "above-left"),
    )
''').scaled(2).at(-220, -35)
matrix = scene.equation("sum_(k=1)^n k = (n(n+1)) / 2").fill(BLACK).at(285, -35, anchor=Anchor.CENTER)

scene.play([
    title.write().duration(0.55),
    caption.fade_in_from(Direction.DOWN, distance=24).duration(0.45),
    table.write(),
    # table.fade_in_from(Direction.DOWN, distance=30).duration(0.65),
    # matrix.fade_in_from(Direction.DOWN, distance=30).duration(0.65),
    matrix.write()
])
scene.wait(1)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
