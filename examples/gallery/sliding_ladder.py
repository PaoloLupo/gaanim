"""A ladder whose feet slide along a wall and the floor, its midpoint tracing a quarter circle and its past positions outlining an astroid.

One Parameter, the ladder's angle with the floor, places both feet: one on
the floor and one on the wall, always the same length apart. The line
between them follows the feet and its midpoint leaves a trace. A faint copy
of the ladder appears each time it passes another tenth of its fall, and the
copies outline the curve that every position touches, drawn at the end with
its equation.
"""

import math
import os

from gaanim import Easing, Scene, Zones, computed, stagger

scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
gold = scene.canvas.color("title")
lilac = scene.canvas.color("chart")
paper = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
geometry = scene.geometry

stage, side = scene.layout.zones(Zones.columns(["3fr", "2fr"], gap="48px", padding="page_padding"))
LENGTH = stage.height * 0.95
ox, oy = stage.left + 0.4, stage.bottom + 0.1
FROM, TO = math.radians(88), math.radians(2)
FALL = 5.0

angle = scene.viz.parameter(FROM)
floor_foot = geometry.point_ref(computed(lambda a: ox + LENGTH * math.cos(a), inputs=[angle]), oy)
wall_foot = geometry.point_ref(ox, computed(lambda a: oy + LENGTH * math.sin(a), inputs=[angle]))
middle = geometry.point_between(floor_foot, wall_foot)

floor = geometry.line(ox - 0.3, oy, ox + LENGTH + 0.4, oy).stroke(paper, 0.05)
wall = geometry.line(ox, oy - 0.3, ox, oy + LENGTH + 0.2).stroke(paper, 0.05)
# Copies at evenly spaced angles; the fall is linear, so the k-th copy
# appears when the ladder passes it.
COPIES = 24
copies = []
for k in range(COPIES):
    a = FROM + (TO - FROM) * k / (COPIES - 1)
    copy = geometry.line(ox + LENGTH * math.cos(a), oy, ox, oy + LENGTH * math.sin(a)).stroke(violet, 0.025)
    copies.append(copy.opacity(0.45).hidden())
ladder = geometry.tracking_line(floor_foot, wall_foot).stroke(violet, 0.07)
feet = [geometry.dot(0.13).fill(lilac).follow(foot) for foot in (floor_foot, wall_foot)]
pen = geometry.dot(0.13).fill(gold).follow(middle)
trace = geometry.traced_path(pen).stroke(gold, 0.05)
# The curve the copies outline: x^(2/3) + y^(2/3) = L^(2/3).
astroid = geometry.polyline(
    [(ox + LENGTH * math.cos(t) ** 3, oy + LENGTH * math.sin(t) ** 3) for t in (i * math.pi / 160 for i in range(81))]
).no_fill().stroke(lilac, 0.04).hidden()

top, bottom = side.split(Zones.rows(2, gap="24px"))
curve = scene.text.equation("x^(2/3) + y^(2/3) = L^(2/3)", size=0.8, color=lilac).place(top, anchor="bottom").hidden()
circle = scene.text.equation("x^2 + y^2 = (L/2)^2", size=0.8, color=gold).place(bottom, anchor="top").hidden()

scene.play([floor.animate.create(), wall.animate.create()], duration=0.8)
scene.play([ladder.animate.create(), *(foot.animate.grow_from_center() for foot in feet), pen.animate.grow_from_center()],
           duration=0.6)
scene.play(trace.animate.fade_in(), duration=0.01)
scene.play([
    angle.animate.set(TO).duration(FALL).easing(Easing.LINEAR),
    stagger(*(copy.animate.fade_in().duration(0.15) for copy in copies), each=FALL / (COPIES - 1)),
])
scene.play(circle.animate.write(), duration=1.0)
scene.play([astroid.animate.create(), curve.animate.write()], duration=1.2)
scene.wait(1.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Upright, along the fall, flat, and both curves named.
    scene.snapshots(snapshots, [1.5, 2.8, 4.1, 5.4, 7.4, 9.6])
else:
    scene.render()
