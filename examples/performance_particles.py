"""2000 live particles for the runtime performance harness.

A burst of 1400 long-lived particles plus a continuous stream that keeps
about 600 more alive, so every frame after the start evaluates and draws
roughly 2000 particles. Pass it to
`tests/benchmark_runtime.py --scene examples/performance_particles.py`.
"""

import math
import os

from gaanim import CYAN, GOLD, PINK, Emitter, Scene


FPS = 30
FRAME_COUNT = max(30, int(os.environ.get("GAANIM_BENCHMARK_FRAMES", "300")))
DURATION = FRAME_COUNT / FPS
LIFETIME = (max(1.0, DURATION), max(1.0, DURATION) * 1.2)

scene = Scene(frame=(16, 9), background="#08111f", margin=0.4)
# The stream keeps rate * mean life (about 600) alive once it settles.
stream = scene.fx.particles(
    Emitter.circle(3.0).at((0, 0)), rate=600 / 3.0, lifetime=(2.5, 3.5),
    speed=(0.3, 1.2), spread=math.tau, drag=0.4, size=(0.03, 0.07),
    color=[CYAN, PINK], seed=1,
)
cloud = scene.fx.particles(
    Emitter.rect(14, 7).at((0, 0)), rate=0, lifetime=LIFETIME, speed=(0.1, 0.5),
    spread=math.tau, spin=2.0, shape="square", size=(0.04, 0.08), color=GOLD, fade=0.2, seed=2,
)
cloud.burst(1400)
scene.wait(DURATION)

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
