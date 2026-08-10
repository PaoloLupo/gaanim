"""Native visualization API: typed axes, plots, coordinates, and calculus."""

import math
import os

from gaanim import BLACK, BLUE, GOLD, GREEN, ORANGE, RED, TEAL, Axis, Brush, Expr, Scene

scene = Scene(1920, 1080)
scene.canvas.set_theme("paper")

x_axis = Axis.linear(-6, 6).ticks(1).minor_ticks(2).label("$x$").style(color=BLACK)
y_axis = Axis.linear(-3, 3).ticks(1).minor_ticks(2).label("$f (x)$").style(color=BLACK)
plane = scene.number_plane(x_axis, y_axis, width=1500, height=900).at(100,0)

x = Expr.var("x")
amplitude = scene.parameter(1.0)
sine = plane.plot(amplitude.expr() * x.sin()).stroke(BLUE, 4)
parabola = plane.plot(lambda value: 0.12 * value * value - 1.2).no_fill().stroke(GREEN, 3)
tangent = plane.tangent(lambda value: math.sin(value), 1.2, length=3.0).stroke(RED, 3)
area = plane.area_under(lambda value: math.sin(value) , (0, math.pi), baseline=0).fill(TEAL).opacity(0.75)
riemann = plane.riemann_sum(lambda value: math.sin(value), (-2* math.pi,0), rectangles=20, baseline=0).fill(Brush.linear([ORANGE, GOLD], start=(0, 0), end=(0, 200), extend="reflect"))


point = scene.dot(7).fill(RED).at_coordinate(plane.coord(2, 2))
title = scene.text("CoordinateSpace + Expr + Parameter").fill(BLACK).at(0, 500)

scene.play([plane.write(), title.write().duration(0.6)])
scene.play([sine.write().duration(0.8), parabola.write().duration(0.8)])
scene.play([area.fade_in().duration(0.5), tangent.create().duration(0.5), point.fade_in().duration(0.3)])
scene.play([riemann.create().duration(1)])
scene.play([amplitude.animate_to(2.5).duration(1.2)])

# Every layer is a real drawable and can be animated independently.
scene.play([plane.layer("minor_grid").fade_to(0.15).duration(0.4)])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 1.5, 2.5, 3.7])
else:
    scene.render()
