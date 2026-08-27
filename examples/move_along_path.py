import math
from gaanim import Easing, BLACK, BLUE, WHITE, Scene

# En gaanim las coordenadas son en píxeles (ej: 100px equivale a ~1 unidad Manim)
scene = Scene(1920, 1080, background=BLACK)

# 1. Creación de objetos
circle = scene.geometry.circle(100.0).stroke(BLUE, 4.0).no_fill().move_to(0, 0)
dot = scene.geometry.dot(8.0).fill(WHITE).move_to(0, 0)
dot2 = scene.geometry.dot(8.0).fill(WHITE).move_to(100, 0)

line = scene.geometry.line(300, 0, 500, 0).stroke(WHITE, 3.0)

# 2. Animaciones
# GrowFromCenter(circle)
scene.play([circle.animate.grow_from_center().duration(1.0).easing(Easing.SMOOTH)])

# Transform(dot, dot2)
scene.play([dot.animate.transform_to(dot2).duration(1.0).easing(Easing.SMOOTH)])

# MoveAlongPath(dot, circle)
scene.play([dot.animate.move_along(circle).duration(2.0).easing(Easing.LINEAR)])

# Rotating(dot, about_point=[2, 0, 0])
# En gaanim pivot(200, 0) define el punto sobre el cual rota
scene.play([dot.pivot(200, 0).animate.rotate_by(math.pi).duration(1.5).easing(Easing.LINEAR)])

import os

if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0])
else:
    scene.render()
