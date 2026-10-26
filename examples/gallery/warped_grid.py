"""A sentence and an equation that write themselves and transform, then a grid bent by a nonlinear function.

The text and Euler's identity are written, and the equation turns into
another one with transform_to. The grid's lines are parametric curves of
the plane that take a Parameter t: each point (x, y) moves to
(x + t sin y, y + t sin x), so animating t from 0 to 1 bends the whole grid
smoothly. Its lines are colored from a color map by position.
"""

import math
import os

from gaanim import Axis, ColorMap, Easing, Scene, Zones

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
paper = scene.colors.foreground
mist = scene.colors.rule

top, middle = scene.layout.zones(Zones.rows(["1fr", "1fr"], padding="page_padding"))
sentence = scene.text("Some text, and an identity", size=0.7, color=paper).place(top, anchor="bottom")
identity = scene.text.equation("e^(i pi) + 1 = 0", size=1.1, color=gold).place(middle, anchor="top")
expansion = scene.text.equation("e^(i theta) = cos theta + i sin theta", size=1.1, color=gold).place(
    middle, anchor="top"
).hidden()

X0, X1, Y0, Y1 = -8.0, 8.0, -4.5, 4.5
plane = scene.viz.cartesian_2d(
    Axis.linear(X0, X1), Axis.linear(Y0, Y1), width=16, height=9, grid=False, axes=False, ticks=False,
    numbers=False, labels=False,
)
t = scene.viz.parameter(0.0)
ramp = ColorMap.from_colors([mist, violet, gold])


def bend(x, y, amount):
    return x + amount * math.sin(y), y + amount * math.sin(x)


lines = []
for index, x in enumerate(range(-8, 9)):
    line = plane.parametric(lambda s, amount, x=x: bend(x, s, amount), (Y0, Y1), inputs=[t])
    lines.append(line.stroke(ramp.sample(index / 16), 0.03))
for index, y in enumerate(range(-4, 5)):
    line = plane.parametric(lambda s, amount, y=y: bend(s, y, amount), (X0, X1), inputs=[t])
    lines.append(line.stroke(ramp.sample(index / 8), 0.03))
for line in lines:
    line.hidden()

scene.play(sentence.animate.write(), duration=1.2)
scene.play(identity.animate.write(), duration=1.2)
scene.wait(0.4)
scene.play(identity.animate.transform_to(expansion), duration=1.4, easing=Easing.SMOOTH)
scene.wait(0.6)
scene.play([sentence.animate.fade_out(), identity.animate.fade_out()], duration=0.6)
scene.play([line.animate.create() for line in lines], duration=1.4)
scene.play(t.animate.set(1.0), duration=3.0, easing=Easing.SMOOTH)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The text, the transform, the straight grid, and the grid bending.
    scene.snapshots(snapshots, [1.9, 3.6, 4.9, 7.4, 8.6, 10.6])
else:
    scene.render()
