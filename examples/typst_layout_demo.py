"""Typst document layouts and mathematical matrices as vector drawables."""

import os

from gaanim import Anchor, BLACK, GRAY, GREEN, WHITE, Direction, Scene


scene = Scene(frame=(16, 9), background=WHITE, margin=0.7)

title = scene.text("Typst-native layouts", role="title").fill(GREEN).move_to(0, 3.25, anchor=Anchor.CENTER)
caption = scene.text("Document table and mathematical matrix", role="subtitle").fill(GRAY).move_to(0, 2.5625, anchor=Anchor.CENTER)
table = scene.text.typst('''
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
''').scale_to(2).move_to(-2.75, -0.4375)
matrix = scene.text.equation("sum_(k=1)^n k = (n(n+1)) / 2").fill(BLACK).move_to(3.5625, -0.4375, anchor=Anchor.CENTER)

scene.play([
    title.animate.write().duration(0.55),
    caption.animate.fade_in_from(Direction.DOWN, distance=0.3).duration(0.45),
    table.animate.write(),
    # table.animate.fade_in_from(Direction.DOWN, distance=30).duration(0.65),
    # matrix.animate.fade_in_from(Direction.DOWN, distance=30).duration(0.65),
    matrix.animate.write()
])
scene.wait(1)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
