"""A camera that zooms in on a point riding a sine wave, follows it along two turns and pulls back out.

The point sits on the curve at a Parameter x, with a tangent line that turns
with the slope and a trail of echoes. The camera zooms in and pans onto it,
follows it with a little lag while x runs across the plane, and pulls back
out from it; the readouts are pinned to the screen with hud(), so they stay
put while the camera moves.
"""

import math
import os

from gaanim import Axis, Easing, Scene, Zones, computed

scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
gold = scene.canvas.color("title")
lilac = scene.canvas.color("chart")
muted = scene.canvas.color("muted")
geometry = scene.geometry

band, stage = scene.layout.zones(Zones.rows(["1fr", "5fr"], gap="24px", padding="page_padding"))
X0, X1, Y0, Y1 = 0.0, 4 * math.pi, -1.5, 1.5
width, height = stage.width * 0.94, stage.height * 0.8
plane = scene.viz.cartesian_2d(
    Axis.linear(X0, X1).ticks(math.pi / 2).numbers("pi", denominator=2), Axis.linear(Y0, Y1).ticks(0.5),
    width=width, height=height,
).move_to(*stage.center)


def at(x, y):
    cx, cy = stage.center
    return cx + (x - (X0 + X1) / 2) * width / (X1 - X0), cy + (y - (Y0 + Y1) / 2) * height / (Y1 - Y0)


curve = plane.plot(math.sin).stroke(violet, 0.05)
x = scene.viz.parameter(0.0)


def along(offset):
    """A point ``offset`` plot units along the tangent from the rider."""
    def end(v):
        run = offset / math.hypot(1, math.cos(v))
        return at(v + run, math.sin(v) + run * math.cos(v))
    return geometry.point_ref(computed(lambda v: end(v)[0], inputs=[x]), computed(lambda v: end(v)[1], inputs=[x]))


tangent = geometry.tracking_line(along(-0.7), along(0.7)).stroke(lilac, 0.03)
rider = geometry.dot(0.1).fill(gold).follow(along(0.0)).glow(gold, radius=0.15).echo(8, delay=0.04, decay=0.7)

readouts = [
    scene.viz.readout(x, label="$x$", format=".2f", font_size=0.5, color=muted).hud(),
    scene.viz.readout(computed(math.sin, inputs=[x]), label="$sin(x)$", format="z.2f", font_size=0.5, color=gold).hud(),
    scene.viz.readout(computed(math.cos, inputs=[x]), label="$cos(x)$", format="z.2f", font_size=0.5, color=lilac).hud(),
]
for readout, zone in zip(readouts, band.split(Zones.columns(3, gap="48px"))):
    readout.place(zone, anchor="center")

scene.play(plane.animate.create(), duration=0.8)
scene.play(curve.animate.create(), duration=1.0)
scene.play([rider.animate.grow_from_center(), tangent.animate.create(), *(r.animate.fade_in() for r in readouts)],
           duration=0.5)
# pan_to eases onto the rider where it is drawn; follow then keeps it in view.
scene.play([scene.camera.animate.zoom_to(3.0), scene.camera.animate.pan_to(rider)], duration=1.2,
           easing=Easing.SMOOTH)
scene.play([x.animate.set(X1), scene.camera.animate.follow(rider, lag=0.15)], duration=6.0, easing=Easing.SMOOTH)
scene.play(scene.camera.animate.reset(), duration=1.4, easing=Easing.SMOOTH)
scene.wait(0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The whole plane, zoomed on the rider, following it, and pulled back.
    scene.snapshots(snapshots, [2.2, 3.6, 5.2, 7.4, 9.4, 11.6])
else:
    scene.render()
