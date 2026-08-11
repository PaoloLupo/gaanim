"""Test visual para double_arrow / curved_arrow / curved_arrow_arc con create.

Valida que el `fill` nunca se sale de la silueta durante la animacion
de `create` (antes se veia como si el fill siguiera al cursor).
"""

import os

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background=BLACK, margin=60)

# Titulo
title = scene.text("Arrows create – fill must stay inside", role="title").fill(WHITE).at(0, 280)

# 1. double_arrow horizontal
da = scene.double_arrow(-500, 120, -120, 120).fill(BLUE).stroke(WHITE, 2.5)

# 2. curved_arrow con deflexion angular (semicircular)
ca = scene.curved_arrow(-80, 120, 260, 120, 0.9).fill(GOLD).stroke(WHITE, 2.5)

# 3. curved_arrow_arc circular explícito
caa = scene.curved_arrow_arc(0, -140, 110, 0.2, 2.0).fill(WHITE).stroke(WHITE, 2.5)
# referencia: circulo guia punteado para ver que la punta stay on radius
guide = scene.circle(110).stroke(WHITE, 1).at(0, -140).opacity(0.15)

scene.play([
    title.write().duration(0.5),
    da.create().duration(1.2).smooth(),
    ca.create().duration(1.2).smooth(),
    caa.create().duration(1.2).smooth(),
])

# Segundo bloque: mismo create pero con otro timing para visual diff
scene.wait(0.5)
scene.play([
    da.uncreate().duration(0.8),
    ca.uncreate().duration(0.8),
    caa.uncreate().duration(0.8),
])
scene.wait(0.2)
scene.play([
    da.create().duration(1.0),
    ca.create().duration(1.0),
    caa.create().duration(1.0),
])

scene.wait(0.5)

# Snapshots para `gaanim --diff`
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # 0.0 inicio, 0.6 mitad de create (aqui antes se veia el leak),
    # 1.2 fin create, 2.5 mitad uncreate, 3.5 recreado
    scene.snapshots(snapshots, [0.3, 0.6, 1.2, 2.0, 3.0, 3.8])
else:
    scene.render()
