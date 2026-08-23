"""Deterministic medium scene used by the runtime performance harness."""

import os

from gaanim import BLUE, GOLD, GREEN, RED, WHITE, Scene


FPS = 30
FRAME_COUNT = max(30, int(os.environ.get("GAANIM_BENCHMARK_FRAMES", "300")))
DURATION = FRAME_COUNT / FPS

scene = Scene(1920, 1080, background="#08111f", margin=48)
colors = (BLUE, GOLD, GREEN, RED)
objects = []

for row in range(6):
    for column in range(8):
        x = -630 + column * 180
        y = 330 - row * 132
        shape = (
            scene.circle(28 + (row + column) % 3 * 4)
            .fill(colors[(row + column) % len(colors)])
            .stroke(WHITE, 2)
            .at(x, y)
        )
        objects.append(shape)

entry_duration = min(0.4, DURATION * 0.25)
exit_duration = min(0.4, DURATION * 0.25)
motion_duration = max(0.1, DURATION - entry_duration - exit_duration)

scene.play([shape.create().duration(entry_duration) for shape in objects])
scene.play(
    [
        shape.move(
            72 if index % 2 == 0 else -72,
            54 if (index // 2) % 2 == 0 else -54,
        )
        .duration(motion_duration)
        .smooth()
        for index, shape in enumerate(objects)
    ]
)
scene.wait(exit_duration)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scenario = os.environ.get("GAANIM_BENCHMARK_SCENARIO", "seek")
    if scenario == "preview":
        times = [min(index / FPS, DURATION) for index in range(FRAME_COUNT)]
    else:
        # A coprime stride gives a deterministic random-access order without
        # importing a PRNG or making reports depend on a seed implementation.
        times = [((index * 37) % FRAME_COUNT) / FPS for index in range(FRAME_COUNT)]
    scene.snapshots(snapshot_dir, times)
else:
    scene.render()
