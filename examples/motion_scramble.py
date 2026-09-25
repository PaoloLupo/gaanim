"""Scramble / decode: glyphs cycle through a charset and settle left to right."""

import os

from gaanim import BLACK, CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Scramble", role="title").fill(WHITE).move_to(0, 3.6)

label = scene.text("LAUNCH SEQUENCE", role="title").fill(GOLD).move_to(0, 1.0)
code = scene.text("ACCESS GRANTED").fill(CYAN).move_to(0, -1.0)
note = scene.text('charset="upper" · "hex" · "01"').fill(GRAY).scale_by(0.5).move_to(0, -3.0)

# Both decode at once; the final width is reserved from the first frame.
scene.play([
    label.animate.scramble(charset="upper", reveal_delay=0.3, speed=20, seed=1).duration(1.4),
    code.animate.scramble(charset="hex", reveal_delay=0.5, speed=14, seed=2).duration(1.4),
])
scene.wait(0.4)
# Decode into new text, in binary.
scene.play([label.animate.scramble_to("LANZAMIENTO", charset="01", reveal_delay=0.2, seed=3).duration(1.2)])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.9, 2.2, 2.8])
else:
    scene.render()
