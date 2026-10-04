"""Thousands of distinct animated drawables for the runtime performance harness.

1500 x scale separate shapes (circles, stars, polygons and rounded
rectangles, a quarter of them with linear gradients), 60 x scale text labels
and a few equations. Every shape owns its tweens, so each frame evaluates
and encodes every entity: this measures ECS, timeline and path encoding
cost rather than instancing (see `performance_repeater.py`). Set
GAANIM_BENCHMARK_SCALE to grow the counts; pass it to
`tests/benchmark_runtime.py --scene examples/performance_objects.py`.
"""

import math
import os

from gaanim import BLUE, CORAL, CYAN, GOLD, GREEN, PINK, WHITE, Brush, Easing, Scene


FPS = 30
FRAME_COUNT = max(30, int(os.environ.get("GAANIM_BENCHMARK_FRAMES", "300")))
DURATION = FRAME_COUNT / FPS
SCALE = max(1, int(os.environ.get("GAANIM_BENCHMARK_SCALE", "1")))
SHAPES = 1500 * SCALE
LABELS = 60 * SCALE

scene = Scene(frame=(16, 9), background="#08111f", margin=0.4)
colors = (BLUE, GOLD, GREEN, CORAL, CYAN, PINK)

columns = math.ceil(math.sqrt(SHAPES * 16 / 9))
rows = math.ceil(SHAPES / columns)
cell = min(15.2 / columns, 8.2 / rows)
size = cell * 0.36

shapes = []
for index in range(SHAPES):
    row, column = divmod(index, columns)
    x = (column - (columns - 1) / 2) * cell
    y = ((rows - 1) / 2 - row) * cell
    kind = index % 4
    if kind == 0:
        shape = scene.geometry.circle(size)
    elif kind == 1:
        shape = scene.geometry.star(5, size, size * 0.45)
    elif kind == 2:
        shape = scene.geometry.regular_polygon(6, size)
    else:
        shape = scene.geometry.rounded_rect(size * 1.8, size * 1.2, size * 0.3)
    color = colors[index % len(colors)]
    if index % 4 == 3:
        shape.fill(Brush.linear([color, WHITE], start=(-size, 0.0), end=(size, 0.0)))
    else:
        shape.fill(color)
    shapes.append(shape.stroke(WHITE, cell * 0.02).move_to(x, y))

labels = [
    scene.text(f"n{index:04}").fill("#cbd5e1").scale_to(0.18).move_to(
        -7.2 + (index % 12) * 1.3, 4.1 - (index // 12) * 0.32
    )
    for index in range(LABELS)
]
equations = [
    scene.text.equation(source).fill(WHITE).scale_to(0.5).move_to(-5.0 + 5.0 * index, -4.0)
    for index, source in enumerate(
        (r"e^{i \pi} + 1 = 0", r"\int_0^1 x^2 dx = \frac{1}{3}", r"\sum_{k=1}^{n} k = \frac{n(n+1)}{2}")
    )
]

entry_duration = min(0.5, DURATION * 0.2)
motion_duration = max(0.1, DURATION - entry_duration)

scene.play(
    [shape.animate.fade_in().duration(entry_duration) for shape in shapes]
    + [label.animate.fade_in().duration(entry_duration) for label in labels]
    + [equation.animate.write().duration(entry_duration) for equation in equations]
)
scene.play(
    [
        shape.animate.rotate_by(math.pi * (1 if index % 2 else -1))
        .shift_by(0.0, cell * (0.3 if (index // columns) % 2 else -0.3))
        .duration(motion_duration)
        .easing(Easing.SMOOTH)
        for index, shape in enumerate(shapes)
    ]
    + [label.animate.opacity(0.4).duration(motion_duration) for label in labels]
)

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
