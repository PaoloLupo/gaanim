"""Test visual para double_arrow / curved_arrow / curved_arrow_arc con create.

Valida que el `fill` nunca se sale de la silueta durante la animacion
de `create` (antes se veia como si el fill siguiera al cursor).
"""

import os

from gaanim import Easing, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background=BLACK, margin=0.75)

# Titulo
title = scene.text("Arrows create – fill must stay inside", role="title").fill(WHITE).move_to(0, 3.5)

# 1. double_arrow horizontal
da = scene.geometry.double_arrow(-6.25, 1.5, -1.5, 1.5).fill(BLUE).stroke(WHITE, 0.03125)

# 2. curved_arrow con deflexion angular (semicircular)
ca = scene.geometry.curved_arrow(-1, 1.5, 3.25, 1.5, 0.9).fill(GOLD).stroke(WHITE, 0.03125)

# 3. curved_arrow_arc circular explícito
caa = scene.geometry.curved_arrow_arc(0, -1.75, 1.375, 0.2, 2.0).fill(WHITE).stroke(WHITE, 0.03125)
# referencia: circulo guia punteado para ver que la punta stay on radius
guide = scene.geometry.circle(1.375).stroke(WHITE, 0.0125).move_to(0, -1.75).opacity(0.15)

scene.play([
    title.animate.write().duration(0.5),
    da.animate.create().duration(1.2).easing(Easing.SMOOTH),
    ca.animate.create().duration(1.2).easing(Easing.SMOOTH),
    caa.animate.create().duration(1.2).easing(Easing.SMOOTH),
])

# Segundo bloque: mismo create pero con otro timing para visual diff
scene.wait(0.5)
scene.play([
    da.animate.uncreate().duration(0.8),
    ca.animate.uncreate().duration(0.8),
    caa.animate.uncreate().duration(0.8),
])
scene.wait(0.2)
scene.play([
    da.animate.create().duration(1.0),
    ca.animate.create().duration(1.0),
    caa.animate.create().duration(1.0),
])

scene.wait(0.5)

# Snapshots para `gaanim --diff`
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # 0.0 inicio, 0.6 mitad de create (aqui antes se veia el leak),
    # 1.2 fin create, 2.5 mitad uncreate, 3.5 recreado
    scene.snapshots(snapshots, [0.3, 0.6, 1.2, 2.0, 3.0, 3.8])
else:
    scene.render()
