"""Marker highlights: bands sweep behind selected words from the left edge."""

import os

from gaanim import GREEN, PINK, Scene


INK = "#1f2937"
scene = Scene(frame=(16, 9), background="#f8f5ee")
card = scene.geometry.rect(12.0, 4.6).fill("#fffdf7").stroke("#e7e0cf", 0.03).move_to(0, -0.4)
title = scene.text("Resaltador tipo marcador", role="title").fill(INK).move_to(0, 3.4)
first = scene.text("Lo que no se mide no se puede mejorar,", color=INK, size=0.62).move_to(0, 0.6)
second = scene.text("y lo que no se mejora siempre se degrada.", color=INK, size=0.62).move_to(0, -1.4)
# Static form: this band is already in place when the scene starts.
title.words[2].marker(PINK, opacity=0.35)

scene.wait(0.4)
# Default translucent yellow, drawn over the card but behind the glyphs.
scene.play([first.words[5:9].animate.marker().duration(1.2)])
scene.wait(0.3)
scene.play([second.words[6:9].animate.marker(GREEN, skew=-0.04, blend="multiply", opacity=0.4).duration(0.8)])
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # First band mid-sweep, first band done, green band mid-sweep, and the end.
    scene.snapshots(snapshots, [1.0, 1.8, 2.3, 3.2])
else:
    scene.render()
