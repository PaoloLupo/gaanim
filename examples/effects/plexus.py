"""Plexus: live lines between drifting points, by range, nearest neighbors or sequence.

``scene.geometry.connect`` joins points with lines that follow them in every
frame. Each panel scatters the same kind of cloud and lets every point drift
its own way, so links appear and break as distances change. The labels are
the code that connects each cloud, after ``dots`` is a group of points.
"""

import os
import random

from gaanim import Distribution, Easing, Scene, Zones

DRIFT = 3.0

scene = Scene()
accent = scene.canvas.color("accent")
foreground = scene.canvas.color("foreground")


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code")


def cloud(zone, count, seed):
    """``count`` dots scattered in ``zone``, and their drifts, each its own way."""
    bounds = (zone.left + 0.4, zone.bottom + 0.3, zone.right - 0.4, zone.top - 0.3)
    points = [scene.geometry.dot(0.08).fill(foreground).move_to(x, y) for x, y in Distribution.random(count, bounds, seed=seed).points]
    turns = random.Random(seed)
    drifts = [
        point.animate.shift_by(turns.uniform(-0.8, 0.8), turns.uniform(-0.3, 0.3)).easing(Easing.SMOOTH)
        for point in points
    ]
    return scene.geometry.group(points), drifts


# mode="range" joins every pair closer than max_distance, "nearest" each point
# to its nearest neighbors, and "sequential" each point to the next one.
# fade_by_distance makes longer links fainter.
VARIANTS = [
    ("connect(dots)", lambda dots: scene.geometry.connect(dots)),
    ("connect(dots, max_distance=2.5)", lambda dots: scene.geometry.connect(dots, max_distance=2.5)),
    ('connect(dots, 3, mode="nearest")', lambda dots: scene.geometry.connect(dots, 3, mode="nearest")),
    ('connect(dots, 3, mode="nearest",\n        neighbors=4)', lambda dots: scene.geometry.connect(dots, 3, mode="nearest", neighbors=4)),
    ('connect(dots, 4,\n        mode="sequential")', lambda dots: scene.geometry.connect(dots, 4, mode="sequential")),
    ("connect(dots, 2.5,\n        fade_by_distance=False)", lambda dots: scene.geometry.connect(dots, 2.5, fade_by_distance=False)),
]
grid = scene.layout.zones(Zones.grid(rows=3, columns=2, gap="48px", padding="page_padding"))
drifts = []
for index, (zone, (code, build)) in enumerate(zip(grid, VARIANTS)):
    art, caption = zone.split(Zones.rows(["1fr", 0.9]))
    label(code).place(caption)
    dots, moves = cloud(art, 22, seed=index + 1)
    build(dots).stroke(accent, 0.025)
    drifts += moves
scene.wait(0.3)
scene.play(drifts, duration=DRIFT)
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The clouds at rest, drifting, and settled where the links have changed.
    scene.snapshots(snapshots, [0.2, 0.3 + DRIFT / 3, 0.3 + 2 * DRIFT / 3, 0.3 + DRIFT + 0.3])
else:
    scene.render()
