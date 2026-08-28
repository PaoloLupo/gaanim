import math
from gaanim import Easing, BLACK, BLUE, WHITE, Scene

# En gaanim las coordenadas son en píxeles (ej: 100px equivale a ~1 unidad Manim)
scene = Scene(frame=(16, 9), background=BLACK)

# 1. Creación de objetos
circle = scene.geometry.circle(0.833333).stroke(BLUE, 0.033333).no_fill().move_to(0, 0)
dot = scene.geometry.dot(0.066667).fill(WHITE).move_to(0, 0)
dot2 = scene.geometry.dot(0.066667).fill(WHITE).move_to(0.833333, 0)

line = scene.geometry.line(2.5, 0, 4.166667, 0).stroke(WHITE, 0.025)

# 2. Animaciones
# GrowFromCenter(circle)
scene.play([circle.animate.grow_from_center().duration(1.0).easing(Easing.SMOOTH)])

# Transform(dot, dot2)
scene.play([dot.animate.transform_to(dot2).duration(1.0).easing(Easing.SMOOTH)])

# MoveAlongPath(dot, circle)
scene.play([dot.animate.move_along(circle).duration(2.0).easing(Easing.LINEAR)])

# Rotating(dot, about_point=[2, 0, 0])
# En gaanim pivot(1.666667, 0) define el punto lógico sobre el cual rota
scene.play([dot.pivot(1.666667, 0).animate.rotate_by(math.pi).duration(1.5).easing(Easing.LINEAR)])

import os

if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0])
else:
    scene.render()
