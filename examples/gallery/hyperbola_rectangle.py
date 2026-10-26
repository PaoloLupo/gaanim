"""A rectangle from the origin to a point that slides along the hyperbola y = 6/x, changing shape while its area stays six.

The corner on the curve is a point for a Parameter x; the rectangle is a
closed polyline whose corners are computed from it, so it reshapes every
frame. Its sides and its area are read out live, and the area never moves.
"""

import os

from gaanim import Axis, Easing, Scene, Zones, computed

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
paper = scene.colors.foreground
muted = scene.colors.muted
geometry = scene.geometry

AREA = 6.0
stage, side = scene.layout.zones(Zones.columns(["3fr", "1fr"], gap="48px", padding="page_padding"))
SIDE = min(stage.width, stage.height) * 0.92
plane = scene.viz.cartesian_2d(Axis.linear(0, 7).ticks(1), Axis.linear(0, 7).ticks(1), width=SIDE, height=SIDE)
plane.move_to(*stage.center)


def at(x, y):
    cx, cy = stage.center
    return cx + (x - 3.5) * SIDE / 7, cy + (y - 3.5) * SIDE / 7


curve = plane.plot(lambda v: AREA / v, domain=(AREA / 7, 7)).stroke(gold, 0.06)
x = scene.viz.parameter(2.0)
y = computed(lambda v: AREA / v, inputs=[x])
right = computed(lambda v: at(v, 0)[0], inputs=[x])
top = computed(lambda h: at(0, h)[1], inputs=[y])
left, bottom = at(0, 0)
rectangle = geometry.polyline([(left, bottom), (right, bottom), (right, top), (left, top)], closed=True)
rectangle.fill(violet).opacity(0.4).stroke(violet, 0.04)
corner = geometry.dot(0.15).fill(gold).follow(geometry.point_ref(right, top)).glow(gold, radius=0.22)

rows = side.split(Zones.rows(["1fr", "1fr", "1fr", "3fr"]))
readouts = [
    scene.viz.readout(x, label="$x$", format=".2f", font_size=0.5, color=paper),
    scene.viz.readout(y, label="$y$", format=".2f", font_size=0.5, color=paper),
    scene.viz.readout(computed(lambda a, b: a * b, inputs=[x, y]), label="$x y$", format=".2f", font_size=0.5,
                      color=gold),
]
for readout, row in zip(readouts, rows):
    readout.place(row, anchor="left")
law = scene.text.equation("y = 6 / x", size=0.55, color=gold).move_to(*at(5.6, 1.6))

scene.play(plane.animate.create(), duration=0.8)
scene.play([curve.animate.create(), law.animate.write()], duration=1.0)
scene.play([rectangle.animate.grow_from_point(*at(0, 0)), corner.animate.grow_from_center(),
            *(readout.animate.fade_in() for readout in readouts)], duration=0.8, easing=Easing.back())
for target in (5.5, 1.0, 3.2):
    scene.play(x.animate.set(target), duration=1.8, easing=Easing.SMOOTH)
    scene.wait(0.3)
scene.play(readouts[2].animate.indicate(), duration=0.8)
scene.wait(0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawn, wide and low, tall and thin, and back in the middle.
    scene.snapshots(snapshots, [2.6, 3.6, 4.8, 6.9, 8.6, 9.9])
else:
    scene.render()
