"""Gradient descent from two starts on the same curve: one ball settles in a shallow valley, the other finds the deepest one.

Each ball sits at (x, f(x)) for its own Parameter x. Every step sets x to
x - rate * f'(x), computed in Python before the animation, so the balls roll
along the curve to their next positions while their tangent lines turn with
them; the steps shrink as the slope flattens. Each position a ball leaves
stays as a faint mark.
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


def f(x):
    return 0.08 * x**4 - 0.6 * x**2 + 0.35 * x + 2.2


def slope(x):
    return 0.32 * x**3 - 1.2 * x + 0.35


band, stage = scene.layout.zones(Zones.rows(["1fr", "5fr"], gap="24px", padding="page_padding"))
X0, X1, Y0, Y1 = -3.2, 3.2, 0.0, 5.0
width, height = stage.width * 0.9, stage.height * 0.92
plane = scene.viz.cartesian_2d(Axis.linear(X0, X1).ticks(1), Axis.linear(Y0, Y1).ticks(1), width=width, height=height)
plane.move_to(*stage.center)


def at(x, y):
    cx, cy = stage.center
    return cx + (x - (X0 + X1) / 2) * width / (X1 - X0), cy + (y - (Y0 + Y1) / 2) * height / (Y1 - Y0)


def point(x, along=0.0):
    """The point ``along`` plot units from (x, f(x)) along the tangent, for a Parameter x."""
    def where(v):
        run = along / math.hypot(1, slope(v))
        return at(v + run, f(v) + run * slope(v))
    return geometry.point_ref(computed(lambda v: where(v)[0], inputs=[x]), computed(lambda v: where(v)[1], inputs=[x]))


curve = plane.plot(f).stroke(violet, 0.06)
RATE, STEPS = 0.08, 14
balls, paths = [], []
for start, color in ((3.0, gold), (-3.0, lilac)):
    x = scene.viz.parameter(start)
    tangent = geometry.tracking_line(point(x, -0.7), point(x, 0.7)).stroke(color, 0.035)
    ball = geometry.dot(0.15).fill(color).follow(point(x)).glow(color, radius=0.22)
    path = [start]
    while len(path) < STEPS:
        path.append(path[-1] - RATE * slope(path[-1]))
    balls.append((x, ball, tangent))
    paths.append(path)

readouts = [
    scene.viz.readout(computed(f, inputs=[balls[0][0]]), label="$f(x_1)$", format=".3f", font_size=0.55, color=gold),
    scene.viz.readout(computed(f, inputs=[balls[1][0]]), label="$f(x_2)$", format=".3f", font_size=0.55, color=lilac),
]
for readout, zone in zip(readouts, band.split(Zones.columns(2, gap="48px"))):
    readout.place(zone, anchor="center")

scene.play(plane.animate.create(), duration=0.8)
scene.play(curve.animate.create(), duration=1.0)
scene.play([part.animate.fade_in() for _, ball, tangent in balls for part in (ball, tangent)] +
           [readout.animate.fade_in() for readout in readouts], duration=0.5)
for step in range(1, STEPS):
    moves = []
    for (x, _, _), path in zip(balls, paths):
        geometry.dot(0.06).fill(muted).move_to(*at(path[step - 1], f(path[step - 1])))
        moves.append(x.animate.set(path[step]))
    scene.play(moves, duration=0.4, easing=Easing.SMOOTH)
labels = [
    scene.text(text, role="code", color=color).move_to(*at(path[-1], f(path[-1]) + 0.6)).hidden()
    for text, color, path in (("local minimum", gold, paths[0]), ("global minimum", lilac, paths[1]))
]
scene.play([*(ball.animate.flash() for _, ball, _ in balls), *(label.animate.fade_in() for label in labels)],
           duration=0.8)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The starts, the long first steps, the short ones near the bottoms, and both minima.
    scene.snapshots(snapshots, [2.4, 3.1, 3.9, 5.1, 6.9, 8.4])
else:
    scene.render()
