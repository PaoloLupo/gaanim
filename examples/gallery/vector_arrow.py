"""A vector on a coordinate plane whose components, angle and coordinates follow it as it sweeps around.

The vector is a length and an angle, two Parameters; its tip is a polar point
built from them. The components are live arrows to the tip's foot on the x
axis, the arc measures the angle and the readouts show the coordinates, so
one animation of the two Parameters moves all of them, while the tip leaves
a trail that fades.
"""

import math
import os

from gaanim import Axis, Easing, Scene, Zones, computed

scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
gold = scene.canvas.color("title")
lilac = scene.canvas.color("chart")
paper = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")

# A plane one scene unit per unit, its origin in the middle of the frame.
plane = scene.viz.cartesian_2d(
    Axis.linear(-7, 7).ticks(1), Axis.linear(-4, 4).ticks(1), width=14, height=8, numbers=False, labels=False
)

geometry = scene.geometry
length = scene.viz.parameter(2 * math.sqrt(2))
angle = scene.viz.parameter(math.pi / 4)
x = computed(lambda r, a: r * math.cos(a), inputs=[length, angle])
y = computed(lambda r, a: r * math.sin(a), inputs=[length, angle])
origin = (0.0, 0.0)
tip = geometry.polar_point(origin, length, angle)
foot = geometry.point_ref(x, 0.0)

x_part = geometry.connector(origin, foot, body_width=0.04).fill(violet)
y_part = geometry.connector(foot, tip, body_width=0.04).fill(lilac)
turned = computed(lambda a: a % math.tau, inputs=[angle])
sweep = geometry.always_redraw_arc(turned, 0.0, 0.0, 0.7, 0.0).no_fill().stroke(muted, 0.03)
vector = geometry.connector(origin, tip, head_length=0.32, head_width=0.28, body_width=0.07).fill(gold)
pen = geometry.dot(0.1).fill(gold).follow(tip)
trail = geometry.traced_path(pen, dissipating_time=1.6).stroke(gold, 0.05)

# The coordinates in a column at the top left of the padded frame.
panel = scene.layout.zones(Zones.columns(["1fr", "3fr"], padding="page_padding"))[0]
rows = panel.split(Zones.rows(["1fr", "1fr", "1fr", "1fr", "6fr"]))
readouts = [
    scene.viz.readout(x, label="$x$", font_size=0.6, color=violet),
    scene.viz.readout(y, label="$y$", font_size=0.6, color=lilac),
    scene.viz.readout(length, label="$|v|$", font_size=0.6, color=gold),
    scene.viz.readout(computed(math.degrees, inputs=[turned]), label="$theta$", format=".0f", suffix="°",
                      font_size=0.6, color=muted),
]
for readout, row in zip(readouts, rows):
    readout.place(row, anchor="left")

scene.play(plane.animate.create(), duration=1.0)
scene.play(vector.animate.create(), duration=0.8)
scene.play([part.animate.fade_in() for part in (x_part, y_part, sweep, pen, *readouts)], duration=0.6)
scene.play(trail.animate.fade_in(), duration=0.01)
scene.wait(0.4)
# One sweep: a turn and a quarter while the length breathes out and back.
scene.play(
    [angle.animate.set(math.pi / 4 + 2.5 * math.pi), length.animate.set(3.6).easing(Easing.there_and_back())],
    duration=4.0,
    easing=Easing.SMOOTH,
)
scene.play([angle.animate.set(math.pi / 4 + 2 * math.pi), length.animate.set(2 * math.sqrt(2))], duration=1.4)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawn at (2, 2), along the sweep, and back.
    scene.snapshots(snapshots, [1.6, 3.2, 4.2, 5.0, 6.1, 8.8])
else:
    scene.render()
