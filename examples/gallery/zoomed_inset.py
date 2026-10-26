"""A Koch snowflake explored through a round magnifier that follows a point along its edge and zooms deeper.

The snowflake is one closed polyline of five Koch iterations, far finer than
the frame shows. camera.inset with ``follow=True`` keeps a circular screen
on the point that travels the edge, its zoom a Parameter that grows from four
to sixteen, so the detail each iteration adds comes into view.
"""

import math
import os

from gaanim import Anchor, Easing, Scene

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
geometry = scene.geometry


def koch(a, b, depth):
    """The points of a Koch curve from a to b, without b."""
    if depth == 0:
        return [a]
    (ax, ay), (bx, by) = a, b
    dx, dy = (bx - ax) / 3, (by - ay) / 3
    p, q = (ax + dx, ay + dy), (ax + 2 * dx, ay + 2 * dy)
    peak = (p[0] + dx / 2 - dy * math.sqrt(3) / 2, p[1] + dy / 2 + dx * math.sqrt(3) / 2)
    points = []
    for start, end in ((a, p), (p, peak), (peak, q), (q, b)):
        points += koch(start, end, depth - 1)
    return points


CENTER, RADIUS = (-3.4, -0.2), 3.3
corners = [(CENTER[0] + RADIUS * math.cos(a), CENTER[1] + RADIUS * math.sin(a))
           for a in (math.pi / 2 + k * 2 * math.pi / 3 for k in (0, 2, 1))]
outline = []
for start, end in zip(corners, corners[1:] + corners[:1]):
    outline += koch(start, end, 5)
flake = geometry.polyline(outline, closed=True).fill(violet).opacity(0.85).stroke(lilac, 0.006)

probe = geometry.dot(0.03).fill(gold).no_stroke().move_to(*corners[0])
zoom = scene.viz.parameter(4.0)
view = scene.camera.inset(probe, zoom=zoom, at=(4.4, 0.0), size=6.2, shape="circle", follow=True, color=gold)

scene.play(flake.animate.create(), duration=1.6)
scene.play([probe.animate.grow_from_center(), view.animate.pop_out()], duration=0.8)
scene.play([probe.animate.move_along(flake, end=0.34), zoom.animate.set(16.0)], duration=6.0, easing=Easing.SMOOTH)
scene.play(zoom.animate.set(6.0), duration=1.2, easing=Easing.SMOOTH)
scene.wait(0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The flake, the magnifier out, deeper along the edge, and eased back.
    scene.snapshots(snapshots, [1.4, 2.6, 4.4, 6.4, 8.4, 10.2])
else:
    scene.render()
