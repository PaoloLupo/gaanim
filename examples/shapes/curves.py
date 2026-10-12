"""Curves: Bézier segments, composed curve commands and sampled parametric paths.

The first segment draws quadratic and cubic Bézier curves with their control
handles, the second builds curves from cursor commands, and the third samples
parametric equations into polylines, one of them redrawn every frame. Each
label is the ``scene.geometry`` call that builds the curve.
"""

import math
import os

from gaanim import Scene, Zones, computed

DRAW = 1.4
HOLD = 0.6

scene = Scene()
rule = scene.canvas.color("rule")


def reveal(drawables):
    """Draw ``drawables`` together, then hold them."""
    scene.play([drawable.animate.create() for drawable in drawables], duration=DRAW)
    scene.wait(HOLD)


def table(variants, anchor="right"):
    """Place each (label, drawable) pair in a row, the label to the left of its drawable.

    ``anchor`` aligns the labels in their column: "left" suits listings.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(variants), columns=["2fr", "1fr"], gap="48px", padding="page_padding")
    )
    for index, (label, drawable) in enumerate(variants):
        scene.text(label, role="code").place(grid[2 * index], anchor=anchor)
        drawable.place(grid[2 * index + 1])
    reveal([drawable for _, drawable in variants])


def gallery(variants):
    """Place each (label, drawable) pair in a 2 x 2 grid, the drawable above its label."""
    grid = scene.layout.zones(Zones.grid(rows=2, columns=2, gap="48px", padding="page_padding"))
    for zone, (label, drawable) in zip(grid, variants):
        art, caption = zone.split(Zones.rows(["1fr", 0.5]))
        scene.text(label, role="code").place(caption)
        drawable.place(art)
    reveal([drawable for _, drawable in variants])


def with_handles(start, controls, end):
    """A Bézier curve grouped with dashed handles from each end to its control point."""
    points = [start, *controls, end]
    guides = [
        scene.geometry.dashed_line(tip, control, dash_length=0.08, gap_length=0.06)
        for tip, control in ((points[0], points[1]), (points[-1], points[-2]))
    ]
    dots = [scene.geometry.dot(0.06).move_to(*point) for point in points]
    marks = scene.geometry.group(guides + dots).stroke(rule, 0.015).fill(rule)
    return scene.geometry.group([marks, scene.geometry.bezier(start, controls, end)])


def listing(commands):
    """The source of ``curve(commands)``, one command per line."""
    lines = [f"    {command!r},".replace("'", '"') for command in commands]
    return "\n".join(["curve([", *lines, "])"])


def steps(end, count=480):
    """``count + 1`` evenly spaced values from 0 to ``end``."""
    return [end * k / count for k in range(count + 1)]


def lissajous(a, b):
    """The Lissajous figure x = sin(a t), y = sin(b t)."""
    return [(2 * math.sin(a * t), 1.3 * math.sin(b * t)) for t in steps(math.tau)]


def rose(k):
    """The rose r = cos(k θ), with k petals for an odd k."""
    return [(1.4 * math.cos(k * t) * math.cos(t), 1.4 * math.cos(k * t) * math.sin(t)) for t in steps(math.pi)]


def spiral(turns):
    """An Archimedean spiral of ``turns`` turns."""
    end = math.tau * turns
    return [(1.4 * t / end * math.cos(t), 1.4 * t / end * math.sin(t)) for t in steps(end)]


# One control point makes a quadratic curve and two a cubic one. Crossed
# controls make a cubic loop back over itself.
scene.segment("bezier")
table([
    ("bezier((0, -1), [(2, 1)], (4, -1))", with_handles((0, -1), [(2, 1)], (4, -1))),
    ("bezier((0, 0), [(1, 1), (3, -1)], (4, 0))", with_handles((0, 0), [(1, 1), (3, -1)], (4, 0))),
    ("bezier((0, 0), [(4, 1), (-1, 1)], (3, 0))", with_handles((0, 0), [(4, 1), (-1, 1)], (3, 0))),
])

# A curve is a list of cursor commands. "auto" mirrors the previous control
# for a smooth join, None collapses a handle onto its end, "_rel" points are
# relative to the cursor, and close_smooth closes with a mirrored curve.
scene.segment("commands")
CLOSED = [("move", [(0, 0)]), ("line", [(3, 0)]), ("quad", [(4, 1.5), (0, 1.5)]), ("close", [])]
SMOOTH = [("move", [(0, 0)]), ("cubic", [(1, 1), (2, 1), (2, 0)]), ("cubic", ["auto", (4, -1), (4, 0)])]
RELATIVE = [("move", [(0, 0)]), ("cubic_rel", [None, (2, 1), (3, 0)]), ("close_smooth", [])]
table([(listing(commands), scene.geometry.curve(commands)) for commands in (CLOSED, SMOOTH, RELATIVE)], anchor="left")

# Parametric equations sampled into points; path() with points is a polyline.
# A coordinate may also be a Computed of scene.time, redrawn every frame.
scene.segment("sampled")
live_wave = [
    (x, computed(lambda t, x=x: 0.6 * math.sin(2.4 * x - 3 * t), inputs=[scene.time]))
    for x in steps(4.4, 88)
]
gallery([
    ("polyline(lissajous(3, 2))", scene.geometry.polyline(lissajous(3, 2))),
    ("polyline(rose(5), closed=True)", scene.geometry.polyline(rose(5), closed=True)),
    ("path(spiral(3))", scene.geometry.path(spiral(3))),
    ("polyline(live_wave)", scene.geometry.polyline(live_wave)),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawing and settled in each segment, and the live wave a little later.
    scene.snapshots(snapshots, [(DRAW + HOLD) * index + t for index in range(3) for t in (0.6, 1.6)] + [5.9])
else:
    scene.render()
