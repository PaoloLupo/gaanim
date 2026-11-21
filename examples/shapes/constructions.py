"""Constructions: ruler-and-compass points, lines and circles that follow the points they are built from.

scene.geometry builds perpendicular bisectors, the circumcenter and the
circle through three points; angle bisectors, the incenter and the foot of a
perpendicular; and the tangents to a circle from a point. They are recomputed
every frame, so moving a vertex moves everything built on it without any
Computed in the script, and a construction with no answer hides until it has
one again.
"""

import os

from gaanim import Scene, Zones

scene = Scene()
G = scene.geometry
accent, muted, rule = scene.colors.accent, scene.colors.muted, scene.colors.rule
seeks = []


def stage(label):
    """The page's stage, with the code that builds it underneath."""
    zone, bottom = scene.layout.zones(Zones.rows(["1fr", "96px"], padding="page_padding"))
    scene.text(label, role="code").place(bottom)
    return zone.center


def vertex(x, y):
    return G.dot(0.12).fill(scene.colors.foreground).move_to(x, y)


def triangle(cx, cy):
    """Three vertices and the sides between them."""
    points = vertex(cx - 2.4, cy - 1.3), vertex(cx + 2.6, cy - 1.1), vertex(cx - 0.6, cy + 1.7)
    sides = [G.tracking_line(p, q).stroke(muted, 0.03) for p, q in zip(points, points[1:] + points[:1])]
    return points, sides


# The perpendicular bisectors meet at the circumcenter, the center of the
# circle through the three vertices; they follow the vertex as it moves.
scene.segment("circumcircle")
cx, cy = stage("G.perpendicular_bisector(A, B) · G.circle_through(A, B, C)")
(A, B, C), sides = triangle(cx, cy)
bisectors = [G.perpendicular_bisector(p, q).stroke(rule, 0.02) for p, q in ((A, B), (B, C), (C, A))]
circle = G.circle_through(A, B, C).no_fill().stroke(accent, 0.035)
center = G.marker(G.circumcenter(A, B, C)).fill(accent)
scene.play([line.animate.create() for line in sides + bisectors], duration=0.8)
scene.play([circle.animate.create(), center.animate.fade_in()], duration=0.8)
start = scene.cursor
scene.play([C.animate.move_to(cx + 1.8, cy + 1.5)], duration=2.0)
scene.play([A.animate.move_to(cx - 0.8, cy - 1.7)], duration=1.5)
seeks.extend([start - 0.05, start + 1.0, scene.cursor - 0.05])

# The angle bisectors meet at the incenter; the circle around it through the
# foot of its perpendicular to a side touches all three sides.
scene.segment("incircle")
cx, cy = stage("G.angle_bisector(B, A, C) · G.incenter(A, B, C) · G.foot(I, (A, B))")
(A, B, C), sides = triangle(cx, cy)
bisectors = [G.angle_bisector(p, o, q).stroke(rule, 0.02) for p, o, q in ((B, A, C), (C, B, A), (A, C, B))]
incenter = G.incenter(A, B, C)
incircle = G.circle_around(incenter, G.foot(incenter, (A, B))).no_fill().stroke(accent, 0.035)
touch = G.marker(G.foot(incenter, (A, B))).fill(accent)
scene.play([line.animate.create() for line in sides + bisectors], duration=0.8)
scene.play([incircle.animate.create(), touch.animate.fade_in()], duration=0.8)
start = scene.cursor
scene.play([B.animate.move_to(cx + 1.4, cy - 1.8)], duration=2.0)
seeks.extend([start + 1.0, scene.cursor - 0.05])

# Tangents from a point touch the circle where the perpendicular from its
# center meets them; from inside the circle there are none, and they hide.
scene.segment("tangents")
cx, cy = stage("t1, t2 = G.tangents(circle, from_=P)")
O = vertex(cx - 1.0, cy)
circle = G.circle_around(O, 1.4).no_fill().stroke(accent, 0.035)
P = vertex(cx + 3.2, cy + 0.8)
t1, t2 = G.tangents(circle, from_=P)
for line in (t1, t2):
    line.stroke(muted, 0.03)
touches = [G.marker(G.foot(O, line)).fill(accent) for line in (t1, t2)]
scene.play([circle.animate.create(), t1.animate.create(), t2.animate.create()] + [dot.animate.fade_in() for dot in touches],
           duration=0.8)
start = scene.cursor
scene.play([P.animate.move_to(cx - 0.6, cy + 0.4)], duration=2.0)
scene.play([P.animate.move_to(cx + 2.6, cy - 1.2)], duration=2.0)
seeks.extend([start - 0.05, start + 1.9, scene.cursor - 0.05])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
