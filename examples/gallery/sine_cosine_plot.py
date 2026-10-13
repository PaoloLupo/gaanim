"""The sine and the cosine drawn over two turns, with a scanning line that reads both curves until it stops at two pi.

The curves are plots of the plane; the scanner is a vertical line at a
Parameter x, with a dot on each curve and live readouts of both values.
Plot coordinates map to the scene linearly, so the scanner's points are
computed from x the same way the plane places the curves.
"""

import math
import os

from gaanim import Axis, Easing, Scene, Zones, computed

scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
gold = scene.canvas.color("title")
paper = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
geometry = scene.geometry

band, stage = scene.layout.zones(Zones.rows(["1fr", "5fr"], gap="24px", padding="page_padding"))
X0, X1, Y0, Y1 = -2 * math.pi, 2 * math.pi, -1.5, 1.5
width, height = stage.width * 0.94, stage.height * 0.9
plane = scene.viz.cartesian_2d(
    Axis.linear(X0, X1).ticks(math.pi / 2).numbers("pi", denominator=2),
    Axis.linear(Y0, Y1).ticks(0.5),
    width=width,
    height=height,
).move_to(*stage.center)


def at(x, y):
    """Scene coordinates of the plot point (x, y)."""
    cx, cy = stage.center
    return cx + (x - (X0 + X1) / 2) * width / (X1 - X0), cy + (y - (Y0 + Y1) / 2) * height / (Y1 - Y0)


sine = plane.plot(math.sin).stroke(violet, 0.06)
cosine = plane.plot(math.cos).stroke(gold, 0.06)
sine_label = scene.text.equation("sin(x)", size=0.55, color=violet).move_to(*at(-1.5 * math.pi, 1.3))
cosine_label = scene.text.equation("cos(x)", size=0.55, color=gold).move_to(*at(-0.6 * math.pi, 1.3))

x = scene.viz.parameter(X0)
scan_x = computed(lambda v: at(v, 0)[0], inputs=[x])
scanner = geometry.tracking_line(
    geometry.point_ref(scan_x, at(0, Y0)[1]), geometry.point_ref(scan_x, at(0, Y1)[1])
).stroke(paper, 0.03)
dots = [
    geometry.dot(0.12).fill(color).follow(geometry.point_ref(scan_x, computed(lambda v, f=f: at(v, f(v))[1], inputs=[x])))
    for f, color in ((math.sin, violet), (math.cos, gold))
]
readouts = [
    scene.viz.readout(computed(lambda v: v / math.pi, inputs=[x]), label="$x$", suffix=" π", format=".2f",
                      font_size=0.55, color=muted),
    scene.viz.readout(computed(math.sin, inputs=[x]), label="$sin(x)$", format="z.2f", font_size=0.55, color=violet),
    scene.viz.readout(computed(math.cos, inputs=[x]), label="$cos(x)$", format="z.2f", font_size=0.55, color=gold),
]
for readout, zone in zip(readouts, band.split(Zones.columns(3, gap="48px"))):
    readout.place(zone, anchor="center")
mark = scene.text.equation("x = 2 pi", size=0.55, color=paper).move_to(*at(1.8 * math.pi, Y1 - 0.2)).hidden()

scene.play(plane.animate.create(), duration=1.0)
scene.play([sine.animate.create(), cosine.animate.create()], duration=1.6, easing=Easing.SMOOTH)
scene.play([sine_label.animate.write(), cosine_label.animate.write()], duration=0.6)
scene.play([scanner.animate.fade_in(), *(dot.animate.grow_from_center() for dot in dots),
            *(readout.animate.fade_in() for readout in readouts)], duration=0.5)
scene.play(x.animate.set(X1), duration=5.0, easing=Easing.SMOOTH)
scene.play(mark.animate.write(), duration=0.6)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The curves drawing, labelled, the scan, and stopped at two pi.
    scene.snapshots(snapshots, [1.8, 3.4, 4.6, 6.2, 7.8, 9.6])
else:
    scene.render()
