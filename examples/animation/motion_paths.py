"""Motion paths: travelling along any outline, and moving along an arc instead of a straight line.

``animate.move_along(path)`` follows the outline of any drawable, optionally
turning with it, and ``.path_arc(angle)`` bends a ``move_to`` or ``shift_by``
into a circular arc. Each panel is labelled with the call that moves its plane
or ball.
"""

import math
import os

from gaanim import Anchor, Easing, Scene, Zones

DURATION = 2.4
HOLD = 0.5

scene = Scene()
foreground = scene.canvas.color("foreground")
rule = scene.canvas.color("rule")


def panels(rows, columns, header):
    """Panels in a grid, each split into a label strip ``header`` units tall and a stage."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    return [panel.split(Zones.rows([header, "1fr"], gap="12px")) for panel in grid]


def plane():
    """A paper plane pointing along +x."""
    return scene.geometry.polygon([(0.32, 0.0), (-0.22, 0.18), (-0.1, 0.0), (-0.22, -0.18)])


def ring(zone):
    """A circle in the middle of ``zone``."""
    return scene.geometry.circle(zone.height * 0.4).no_fill().stroke(rule, 0.02).move_to(*zone.center)


def wave(zone):
    """One period of a sine wave across ``zone``."""
    (x, y), half, height = zone.center, zone.width * 0.42, zone.height * 0.3
    points = [(x - half + 2 * half * i / 60, y + height * math.sin(math.tau * i / 60)) for i in range(61)]
    return scene.geometry.polyline(points).no_fill().stroke(rule, 0.02)


def arrow(zone):
    """A curved arrow across ``zone``; move_along follows its axis from tail to tip."""
    (x, y), half = zone.center, zone.width * 0.42
    low = y - zone.height * 0.25
    return scene.geometry.curved_arrow(x - half, low, x + half, low, -1.0).fill(rule).stroke(rule, 0.02)


# move_along samples the outline by arc length. orient=True turns the plane
# with the tangent, rotate_offset adds to that angle, and start/end pick the
# stretch travelled (start > end travels it backwards).
scene.segment("move_along")
travels = [
    ("move_along(ring)", ring, {}),
    ("move_along(ring, orient=True)", ring, {"orient": True}),
    ("move_along(ring, orient=True,\n    rotate_offset=math.pi / 2)", ring,
     {"orient": True, "rotate_offset": math.pi / 2}),
    ("move_along(wave, orient=True)", wave, {"orient": True}),
    ("move_along(wave, orient=True,\n    start=0.8, end=0.2)", wave, {"orient": True, "start": 0.8, "end": 0.2}),
    ("move_along(arrow, orient=True)", arrow, {"orient": True}),
]
moves = []
for (head, stage), (label, path, options) in zip(panels(3, 2, 0.8), travels):
    scene.text(label, role="code").move_to(*head.point("top_left"), anchor=Anchor.TOP_LEFT)
    route = path(stage)
    moves.append(plane().move_to(*stage.center).animate.move_along(route, **options).easing(Easing.LINEAR))
scene.play(moves, duration=DURATION)
scene.wait(HOLD)

# path_arc turns the straight move into an arc of `angle` radians:
# ball.animate.move_to(*end).path_arc(angle). Positive angles turn
# counterclockwise, so moving right they swing below the line.
scene.segment("path_arc")
arcs = [
    ("move_to(*end)", None),
    ("path_arc(math.pi / 3)", math.pi / 3),
    ("path_arc(-math.pi / 3)", -math.pi / 3),
    ("path_arc(math.pi / 2)", math.pi / 2),
    ("path_arc(math.pi)", math.pi),
    ("path_arc(-math.pi)", -math.pi),
]
moves = []
for (head, stage), (label, angle) in zip(panels(2, 3, 0.5), arcs):
    scene.text(label, role="code").place(head, anchor="center")
    (x, y), half = stage.center, min(stage.width, stage.height) * 0.42
    start, end = (x - half, y), (x + half, y)
    for point in (start, end):
        scene.geometry.dot(0.06).fill(rule).move_to(*point)
    ball = scene.geometry.circle(0.16).move_to(*start)
    trail = scene.geometry.traced_path(ball).stroke(foreground, 0.02)
    move = ball.animate.move_to(*end)
    moves += [trail.animate.fade_in().duration(0.1), move if angle is None else move.path_arc(angle)]
scene.play(moves, duration=DURATION)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # A quarter, half and all the way along in each segment.
    scene.snapshots(
        snapshots,
        [segment * (DURATION + HOLD) + t for segment in range(2) for t in (0.6, 1.2, 1.8, 2.8)],
    )
else:
    scene.render()
