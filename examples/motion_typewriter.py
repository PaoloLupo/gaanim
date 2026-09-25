"""Typewriter with a cursor: type, backspace and retype a terminal command."""

import os

from gaanim import BLACK, CYAN, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Typewriter", role="title").fill(WHITE).move_to(0, 3.6)
hint = scene.text("cps=18 · jitter=0.2 · cursor=▍").fill(GRAY).scale_by(0.5).move_to(0, 2.4)

prompt = scene.text("> gaanim render --preview").fill(CYAN).move_to(-2.2, 0.4)

# 25 graphemes at 18 cps: about 1.4 s, then the cursor blinks while idle.
scene.play([prompt.animate.typewriter(cps=18, cursor="▍", blink=2.0, jitter=0.2, seed=7)])
scene.wait(0.6)
# Delete "render --preview" (16 graphemes at 24 cps) ...
scene.play([prompt.animate.backspace(16)])
scene.wait(0.3)
# ... and type the new command after the kept "> gaanim ".
scene.play([prompt.animate.retype("> gaanim export --from clímax", cps=18, seed=3)])
scene.wait(0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.7, 2.3, 3.6, 4.2])
else:
    scene.render()
