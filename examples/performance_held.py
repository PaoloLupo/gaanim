"""A heavy scene that mostly holds still, for the runtime performance harness.

Presentations spend most of their time in `wait()` and stops. Here 1000
instances, cards with effects, glass and a post-process chain enter during
the first tenth of the run and then hold for the rest. Nothing in the frame
depends on time while it holds (solid background, static grain), so every
held frame equals the one before it: an export that re-renders them pays
the full scene cost per frame, one that reuses them pays almost nothing.
Pass it to `tests/benchmark_runtime.py --scene examples/performance_held.py`.
"""

import math
import os

from gaanim import BLUE, CORAL, CYAN, GOLD, GREEN, PINK, WHITE, Distribution, PostProcess, Scene


FPS = 30
FRAME_COUNT = max(30, int(os.environ.get("GAANIM_BENCHMARK_FRAMES", "300")))
DURATION = FRAME_COUNT / FPS
SCALE = max(1, int(os.environ.get("GAANIM_BENCHMARK_SCALE", "1")))

scene = Scene(frame=(16, 9), background="#0b1020", margin=0.4)
scene.canvas.post = [
    PostProcess.bloom(threshold=0.6, intensity=0.6, radius=0.5),
    PostProcess.grain(0.05, animated=False),
    PostProcess.vignette(0.45),
]
colors = (BLUE, GOLD, GREEN, CORAL, CYAN, PINK)

cell = scene.geometry.star(4, 0.1, 0.04).fill(CYAN)
field = scene.geometry.duplicate(cell, Distribution.grid(40 * SCALE, 25, (14.4 / (40 * SCALE), 0.3)))
cards = []
for index in range(12 * SCALE):
    card = scene.geometry.rounded_rect(1.6, 1.0, 0.15).fill(colors[index % len(colors)]).stroke(WHITE, 0.02)
    card.move_to(-5.5 + 2.2 * (index % 6), 1.5 - 1.4 * ((index // 6) % 3))
    if index % 3 == 0:
        card.shader_effect(PostProcess.pixelate(4), margin=0.1)
    elif index % 3 == 1:
        card.glow(colors[index % len(colors)], radius=0.2)
    else:
        card.shadow("#000000aa", blur=0.1)
    cards.append(card)
panel = scene.geometry.rounded_rect(4.0, 1.4, 0.3).fill("#ffffff18").stroke("#ffffff55", 0.02).move_to(0, -2.8)
panel.glass(blur=0.25, refraction=0.1)
title = scene.text("Frames retenidos").fill(WHITE).scale_to(1.0).move_to(0, 3.8)

entry_duration = max(1 / FPS, DURATION * 0.1)
scene.play(
    [field.animate.fade_in().duration(entry_duration), title.animate.write().duration(entry_duration)]
    + [card.animate.fade_in().duration(entry_duration) for card in cards]
    + [panel.animate.fade_in().duration(entry_duration)]
)
scene.wait(max(0.0, DURATION - entry_duration))

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scenario = os.environ.get("GAANIM_BENCHMARK_SCENARIO", "seek")
    if scenario == "preview":
        times = [min(index / FPS, DURATION) for index in range(FRAME_COUNT)]
    else:
        # Same coprime stride as performance_benchmark.py: random access
        # without a PRNG.
        times = [((index * 37) % FRAME_COUNT) / FPS for index in range(FRAME_COUNT)]
    scene.snapshots(snapshot_dir, times)
else:
    scene.render()
