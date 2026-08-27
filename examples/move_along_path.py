import math
from gaanim import BLACK, BLUE, WHITE, Scene

# En gaanim las coordenadas son en píxeles (ej: 100px equivale a ~1 unidad Manim)
scene = Scene(1920, 1080, background=BLACK)

# 1. Creación de objetos
circle = scene.geometry.circle(100.0).stroke(BLUE, 4.0).no_fill().at(0, 0)
dot = scene.geometry.dot(8.0).fill(WHITE).at(0, 0)
dot2 = scene.geometry.dot(8.0).fill(WHITE).at(100, 0)

line = scene.geometry.line(300, 0, 500, 0).stroke(WHITE, 3.0)

# 2. Animaciones
# GrowFromCenter(circle)
scene.play([circle.grow_from_center().duration(1.0).
smooth()])

# Transform(dot, dot2)
scene.play([dot.transform(dot2).duration(1.0).smooth()])

# MoveAlongPath(dot, circle)
scene.play([dot.move_along_path(circle).duration(2.0).linear()])

# Rotating(dot, about_point=[2, 0, 0])
# En gaanim pivot(200, 0) define el punto sobre el cual rota
scene.play([dot.pivot(200, 0).rotate(math.pi).duration(1.5).linear()])

import os

if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0])
else:
    scene.render()
