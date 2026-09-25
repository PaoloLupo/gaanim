"""Narrated scene: the voice sets the pace instead of fixed durations.

Run it with `gaanim examples/narration_demo.py` and open the narration panel
(microphone button) to record each block. Until a block is recorded, its
length is estimated from its text, so the scene already plays in sync.
Takes are saved as examples/narration/<key>.wav plus a JSON of markers.
"""

import os

from gaanim import BLUE, GOLD, WHITE, Easing, Scene

scene = Scene(frame=(16, 9), margin=0.6)
scene.canvas.set_theme("technical")

scene.segment("idea", notes="Hoy vemos qué es una derivada: la pendiente de una curva en un punto.")
title = scene.text("La derivada", role="title").move_to(0, 2.6)
dot = scene.geometry.circle(0.18).fill(GOLD).move_to(-4, -1)
line = scene.geometry.line(-5, -2, 5, 1).stroke(BLUE, 0.06)

# The block's text defaults to the segment notes.
with scene.voiceover("idea") as vo:
    scene.play([title.animate.write()], duration=vo.until("derivada"))
    vo.wait_until("pendiente")
    scene.play([line.animate.create()], duration=vo.until("curva"))
    scene.play([dot.animate.grow_from_center()])

scene.segment("tangente", notes="El punto recorre la recta; la pendiente se mantiene.")
label = scene.text("pendiente = 3/10").fill(WHITE).move_to(0, -2.6)
# An explicit text replaces the notes as the block's script.
with scene.voiceover(
    "tangente",
    text="Movemos el punto por la recta y leemos su pendiente, que no cambia.",
) as vo:
    scene.play(
        [dot.animate.move_to(4, 0.2).easing(Easing.SMOOTH)],
        duration=vo.until("pendiente"),
    )
    scene.play([label.animate.write()])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 3.0, 6.0])
scene.render()
