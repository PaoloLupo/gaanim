"""1000 duplicated instances for the runtime performance harness.

The copies enter with `animate.count`, then the whole group turns and
shrinks, so every frame redraws all 1000 instances. Pass it to
`tests/benchmark_runtime.py --scene examples/performance_repeater.py`.
"""

import os

from gaanim import CYAN, Distribution, Easing, Scene


FPS = 30
FRAME_COUNT = max(30, int(os.environ.get("GAANIM_BENCHMARK_FRAMES", "300")))
DURATION = FRAME_COUNT / FPS
COLUMNS, ROWS = 40, 25

scene = Scene(frame=(16, 9), background="#08111f", margin=0.4)
cell = scene.geometry.star(4, 0.12, 0.05).fill(CYAN)
field = scene.geometry.duplicate(cell, Distribution.grid(COLUMNS, ROWS, (0.36, 0.3))).count(0)

entry_duration = min(1.0, DURATION * 0.3)
motion_duration = max(0.1, DURATION - entry_duration)

scene.play([field.animate.count(COLUMNS * ROWS).duration(entry_duration).easing(Easing.LINEAR)])
scene.play([field.animate.rotate_by(0.35).scale_by(0.8).duration(motion_duration).easing(Easing.SMOOTH)])

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
