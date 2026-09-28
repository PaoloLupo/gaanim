"""Launch, tempo, held echo, the points setter and literal $ in code text.

A ring spins across every later cut (``launch``), dots land on the beats of
a 120 bpm grid (``tempo``, ``beats``, ``wait_until``), a puck leaves frozen
copies where it stops (``echo(hold=True)``), a trapezoid jumps to a new shape
(``points``) and a terminal prompt shows its ``$`` as typed.

Set GAANIM_SNAPSHOTS to capture exact seeks for the visual diff.
"""

import math
import os

from gaanim import BLUE, CYAN, GOLD, GREEN, RED, WHITE, Scene

scene = Scene(frame=(16, 9))
scene.tempo(120, offset=0.25, beats_per_bar=4)  # a beat every 0.5 s

prompt = scene.text("$ gaanim record --price $5", role="code").fill(CYAN).move_to(0, 3.6)

ring = scene.geometry.regular_polygon(6, 1.4).stroke(GOLD, 0.08).move_to(-5, 1)
dots = [
    scene.geometry.circle(0.22).fill(color).move_to(-1.5 + 1.0 * i, 1.6).opacity(0)
    for i, color in enumerate([RED, GOLD, GREEN, BLUE])
]
shape = scene.geometry.polygon([(-0.8, -0.6), (0.8, -0.6), (0.4, 0.8), (-0.4, 0.8)])
shape = shape.fill(BLUE).stroke(WHITE, 0.05).move_to(4.6, 1)
puck = scene.geometry.circle(0.35).fill(RED).echo(4, delay=0.12, decay=0.7, hold=True)
puck = puck.move_to(-5.5, -2.4)

# The ring keeps turning under everything scheduled after it.
scene.launch(ring.animate.rotate_by(math.tau).duration(6.0))

scene.wait_until(beat=0)
for dot in dots:
    scene.play(dot.animate.opacity(1).duration(scene.beats(1)))

scene.wait_until(bar=1.5)
shape.points([(-0.8, -0.8), (0.8, -0.8), (0.8, 0.8), (-0.8, 0.8)])
scene.play(puck.animate.move_to(-1.5, -2.4).duration(scene.beats(2)))
scene.wait(scene.beats(2))  # the copies stay behind while it rests
scene.play(puck.animate.move_to(3.5, -2.4).duration(scene.beats(2)))
scene.play(shape.animate.points([(-0.5, -1.0), (1.0, -0.3), (0.4, 1.0), (-1.0, 0.4)]).duration(1.0))
scene.wait_until(bar=4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.5, 1.5, 2.5, 3.5, 4.0, 4.8, 5.6, 6.8, 8.0])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
