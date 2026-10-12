"""Axonometric drawings: a 3D model drawn as flat shapes from isometric, dimetric, plan and other views.

``scene.geometry.axonometric(view)`` returns a projection whose polygons,
polylines and lines take model points (z up) and draw ordinary 2D shapes,
which ``depth_sort`` orders like a painter. Segments cover the named views,
custom angles, animating a drawing from one view to another, and the kinds
of drawing a projection makes.
"""

import os

from gaanim import Scene, Zones

DRAW = 1.2
MOVE = 1.6
HOLD = 0.8

scene = Scene()
accent = scene.canvas.color("accent")
rule = scene.canvas.color("rule")
muted = scene.canvas.color("muted")
chart = scene.canvas.color("chart")
ink = scene.canvas.color("background")
foreground = scene.canvas.color("foreground")

# A small building, in model units: a floor, walls (the front one with a
# door), an L-shaped roof slab, three columns and a beam above it.
WIDTH, DEPTH, HEIGHT = 4, 3, 2.4
CENTER = (WIDTH / 2, DEPTH / 2, HEIGHT / 2)
SCALE = 0.72


def face(projection, points, color):
    return projection.polygon(points).fill(color).stroke(ink, 0.03)


def building(projection):
    """Draw the building with ``projection`` and return its drawings."""
    w, d, h = WIDTH, DEPTH, HEIGHT
    top = h + 0.6
    drawings = [
        face(projection, [(0, 0, 0), (w, 0, 0), (w, d, 0), (0, d, 0)], muted),
        face(projection, [(0, d, 0), (w, d, 0), (w, d, h), (0, d, h)], accent),
        face(projection, [(0, 0, 0), (0, d, 0), (0, d, h), (0, 0, h)], rule),
        face(projection, [(0, 0, 0), (1.5, 0, 0), (1.5, 0, h), (0, 0, h)], accent),
        face(projection, [(2.5, 0, 0), (w, 0, 0), (w, 0, h), (2.5, 0, h)], accent),
        face(projection, [(0, 0, h), (w, 0, h), (w, 1.5, h), (2, 1.5, h), (2, d, h), (0, d, h)], chart),
    ]
    drawings += [projection.line((x, y, 0), (x, y, top)).stroke(foreground, 0.05) for x, y in [(w, d), (w, 1.5), (2, d)]]
    drawings.append(projection.polyline([(2, d, top), (w, d, top), (w, 1.5, top)]).no_fill().stroke(foreground, 0.05))
    return drawings


def centered(zone, view="isometric", **angles):
    """A projection with ``view`` that draws the building's center at the center of ``zone``."""
    x, y = scene.geometry.axonometric(view, scale=SCALE, **angles).point(*CENTER)
    cx, cy = zone.center
    return scene.geometry.axonometric(view, origin=(cx - x, cy - y), scale=SCALE, **angles)


def gallery(variants):
    """One column per (label, view, angles) variant, the building above its label."""
    columns = scene.layout.zones(Zones.columns(len(variants), gap="48px", padding="page_padding"))
    drawings = []
    for column, (label, view, angles) in zip(columns, variants):
        art, caption = column.split(Zones.rows(["1fr", 0.5]))
        scene.text(label, role="code").place(caption)
        projection = centered(art, view, **angles)
        parts = building(projection)
        projection.depth_sort(parts, z_index=10)
        drawings += parts
    scene.play([drawing.animate.create() for drawing in drawings], duration=DRAW)
    scene.wait(HOLD)


# Each label is the view of scene.geometry.axonometric(view, origin=..., scale=0.72).
# Isometric keeps the three axes at full length; dimetric is the 2:1
# drawing, whose receding axes rise one unit every two.
scene.segment("views")
gallery([('"isometric"', "isometric", {}), ('"dimetric"', "dimetric", {}), ('"plan"', "plan", {})])

# Elevations look straight along an axis: front along +y, side from +x.
scene.segment("elevations")
gallery([('"front"', "front", {}), ('"side"', "side", {})])

# azimuth (radians about z from the front view) and elevation (radians above
# the horizon) replace a view's angles: axonometric("isometric", elevation=-0.5)
# looks from below.
scene.segment("angles")
gallery([
    ("azimuth=0.3", "isometric", {"azimuth": 0.3}),
    ("elevation=1.2", "isometric", {"elevation": 1.2}),
    ("elevation=-0.5", "isometric", {"elevation": -0.5}),
])

# animate_to moves every drawing of a projection to another view, vertex by
# vertex. depth_sort(view=...) orders the drawings for the view they end in.
scene.segment("animate_to")
columns = scene.layout.zones(Zones.columns(2, gap="48px", padding="page_padding"))
moves = []
for column, (label, start, end) in zip(
    columns,
    [("iso.animate_to(plan)", {}, {"view": "plan"}), ("front.animate_to(iso)", {"view": "front"}, {})],
):
    art, caption = column.split(Zones.rows(["1fr", 0.5]))
    scene.text(label, role="code").place(caption)
    projection, target = centered(art, **start), centered(art, **end)
    parts = building(projection)
    projection.depth_sort(parts, z_index=10, view=target)
    moves.append(projection.animate_to(target).defaults(duration=MOVE))
scene.wait(HOLD)
scene.play(moves)
scene.wait(HOLD)

# A projection draws polygons, open or closed polylines and lines through
# model points; point() returns where a model point lands, to place labels.
STEPS = [(0, 0, 0), (0, 0, 0.7), (1, 0, 0.7), (1, 0, 1.4), (2, 0, 1.4), (2, 0, 2.1), (3, 0, 2.1), (3, 0, 0)]
WALL = [(0, 2, 0), (3, 2, 0), (3, 2, 2), (0, 2, 2)]
scene.segment("drawings")
grid = scene.layout.zones(Zones.grid(rows=2, columns=2, gap="48px", padding="page_padding"))


def floor_grid(projection):
    """Faint lines on the floor, three units square, to show the projection's axes."""
    lines = [projection.line((k, 0, 0), (k, 3, 0)) for k in range(4)]
    lines += [projection.line((0, k, 0), (3, k, 0)) for k in range(4)]
    return [line.stroke(rule, 0.02) for line in lines]


VARIANTS = [
    ("iso.polygon(WALL)", lambda iso: [iso.polygon(WALL).fill(accent)]),
    ("iso.polyline(STEPS)", lambda iso: [iso.polyline(STEPS).no_fill().stroke(accent, 0.06)]),
    ("iso.polyline(STEPS, closed=True)", lambda iso: [iso.polyline(STEPS, closed=True).no_fill().stroke(accent, 0.06)]),
    (
        "iso.line((3, 3, 0), (3, 3, 2))",
        lambda iso: [
            iso.line((3, 3, 0), (3, 3, 2)).stroke(accent, 0.06),
            scene.text("(3, 3, 2)", role="code").move_to(*iso.point(3, 3, 2.4)),
        ],
    ),
]
drawings = []
for zone, (label, draw) in zip(grid, VARIANTS):
    art, caption = zone.split(Zones.rows(["1fr", 0.5]))
    scene.text(label, role="code").place(caption)
    x, y = scene.geometry.axonometric(scale=0.62).point(1.5, 1.5, 1)
    cx, cy = art.center
    iso = scene.geometry.axonometric(origin=(cx - x, cy - y), scale=0.62)
    drawings += floor_grid(iso) + draw(iso)
scene.play([drawing.animate.create() for drawing in drawings], duration=DRAW)
scene.wait(HOLD)

# Drawn views in the first three segments, the move from view to view,
# and the drawings.
segment = DRAW + HOLD
move = 3 * segment
seeks = [segment * index + t for index in range(3) for t in (0.6, DRAW + 0.4)]
seeks += [move + HOLD - 0.2, move + HOLD + MOVE / 2, move + HOLD + MOVE + 0.4]
seeks += [move + 2 * HOLD + MOVE + t for t in (0.6, DRAW + 0.4)]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
