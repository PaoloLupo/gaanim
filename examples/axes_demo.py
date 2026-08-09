"""Native visualization API: typed axes, plots, coordinates, and calculus."""

import math
import os

from gaanim import Axis, BLACK, BLUE, Expr, GOLD, RED, TEAL, Scene


scene = Scene(1920, 1080)
scene.canvas.set_theme("paper")

x_axis = Axis.linear(-6, 6).ticks(1).minor_ticks(2).label("x").style(color=BLACK)
y_axis = Axis.linear(-3, 4).ticks(1).label("f(x)").style(color=BLACK)
plane = scene.number_plane(x_axis, y_axis, width=1500, height=760)

x = Expr.var("x")
amplitude = scene.parameter(1.0)
sine = plane.plot(amplitude.expr() * x.sin()).no_fill().stroke(BLUE, 4)
parabola = plane.plot(lambda value: 0.12 * value * value - 1.2).no_fill().stroke(GOLD, 3)
tangent = plane.tangent(lambda value: math.sin(value), 1.2, length=3.5).stroke(RED, 3)
area = plane.area_under(lambda value: math.sin(value) , (0, math.pi), baseline=0).fill(TEAL).opacity(0.75)

point = scene.dot(7).fill(RED).at_coordinate(plane.coord(1.2, math.sin(1.2)))
title = scene.text("CoordinateSpace + Expr + Parameter").fill(BLACK).at(0, 480)

scene.play([plane.create().duration(1.0), title.write().duration(0.6)])
scene.play([sine.write().duration(0.8), parabola.create().duration(0.8)])
scene.play([area.fade_in().duration(0.5), tangent.create().duration(0.5), point.fade_in().duration(0.3)])
scene.play([amplitude.animate_to(2.0).duration(1.2)])

# Every layer is a real drawable and can be animated independently.
scene.play([plane.layer("minor_grid").fade_to(0.15).duration(0.4)])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 1.5, 2.5, 3.7])
else:
    scene.render()
