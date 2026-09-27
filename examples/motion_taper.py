"""Variable-width strokes: tapered arcs, a swelling wave and a pointed passing flash."""

import math
import os

from gaanim import CORAL, GOLD, TEAL, WHITE, Scene


scene = Scene(frame=(16, 9), background="#101826")
scene.text("Stroke profiles", role="title").fill(WHITE).move_to(0, 3.3)
brush = scene.geometry.arc(-4.0, 0.0, 2.0, 0.3, 4.2).stroke(GOLD, 0.35).stroke_taper(0.35, 0.5)
wave_points = [(x / 10.0, 1.0 + 0.6 * math.sin(x / 4.0)) for x in range(-10, 61)]
wave = scene.geometry.polyline(wave_points).stroke(TEAL, 0.3).stroke_profile([(0.0, 0.1), (0.5, 1.0), (1.0, 0.1)])
flash_path = scene.geometry.polyline([(-1.0, -2.2), (2.0, -1.2), (5.0, -2.6), (7.0, -1.8)]).stroke(CORAL, 0.25)
flash_path.stroke_taper(0.5, 0.5)

scene.play([brush.animate.create().duration(1.0), wave.animate.create().duration(1.0)])
scene.play([flash_path.animate.show_passing_flash(time_width=0.5).duration(1.0)])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawing on, drawn, and the tapered flash mid-pass.
    scene.snapshots(snapshots, [0.5, 1.0, 1.5])
else:
    scene.render()
