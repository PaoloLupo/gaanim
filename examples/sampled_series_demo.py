"""Una serie medida conduce la escena: plot de datos nativo y drivers temporales.

Cubre, en orden de aparición:
- rol tipográfico "kicker" y badges auto-dimensionados (scene.slides.badge),
- plot_data / scatter_data sobre un cartesian_2d en coordenadas de datos,
- drive_from_samples: serie muestreada nativa que anima sin callbacks,
- sequence / stagger / parallel para componer animaciones,
- measure_text para dimensionar cajas a partir del texto.

Set GAANIM_SNAPSHOTS to capture this scene before opening the viewer.
"""

import math
import os

from gaanim import CYAN, GOLD, Axis, Scene, parallel, sequence, stagger

scene = Scene(1920, 1080, margin=72)
scene.canvas.set_theme("gruvbox-dark")

# ---------------------------------------------------------------------------
# Serie muestreada: 8 segundos de un oscilador amortiguado (synth, sin assets)
# ---------------------------------------------------------------------------

DT = 0.02
DURATION = 8.0
TIMES = [index * DT for index in range(int(DURATION / DT) + 1)]


def damped_response(period, damping=0.08):
    omega = 2.0 * math.pi / period
    return [math.exp(-damping * omega * t) * math.sin(omega * t) for t in TIMES]


SWAY = damped_response(1.4)

# ---------------------------------------------------------------------------
# Segmento 1: kicker + badges
# ---------------------------------------------------------------------------

scene.segment("Novedades", notes="Rol kicker, badges y measure_text.")
kicker = scene.text("GAANIM · PLOT DE DATOS Y DRIVERS NATIVOS", role="kicker").move_to(0, 430)
title = scene.text("Una serie medida conduce la escena", role="title").move_to(0, 340)
tag_source = scene.slides.badge("serie muestreada nativa", color=CYAN).move_to(-330, 180)
tag_sway = scene.slides.badge("drive_from_samples", color=GOLD).move_to(330, 180)

scene.play(
    parallel(
        sequence(kicker.animate.fade_in().duration(0.4), title.animate.write().duration(0.8)),
        stagger(
            tag_source.animate.grow_from_center().duration(0.5),
            tag_sway.animate.grow_from_center().duration(0.5),
            each=0.15,
        ),
    ),
)
scene.wait(0.8)

# ---------------------------------------------------------------------------
# Segmento 2: plot_data / scatter_data + edificio con sway nativo
# ---------------------------------------------------------------------------

scene.segment("Plot de datos", notes="plot_data en coordenadas del plano; sway sin updater de Python.")
plane = scene.viz.cartesian_2d(
    Axis.linear(0, DURATION).ticks(1).label("tiempo (s)"),
    Axis.linear(-1.0, 1.0).ticks(0.5).label("u(t)"),
    width=880,
    height=460,
).move_to(-460, -60)
curve = plane.plot_data(TIMES, SWAY, color=CYAN, width=4)
peaks = plane.scatter_data(
    [TIMES[i] for i in range(0, len(TIMES), 4)],
    [SWAY[i] for i in range(0, len(TIMES), 4)],
    radius=4.0,
    color=GOLD,
)

ground = scene.geometry.line(695, 120, 865, 120).stroke(CYAN, 5)
building = scene.geometry.rounded_rect(150, 320, 12).fill(CYAN).opacity(0.25).move_to(780, -40)
floors = [
    scene.geometry.line(713, 120 - level * 55, 847, 120 - level * 55).stroke(GOLD, 2)
    for level in range(1, 6)
]
tower = scene.geometry.group([ground, building, *floors])
# La misma serie, ahora conduciendo la posición X del edificio: sin closures,
# sin interpolate() manual, y con seek determinista gratis.
tower.drive_from_samples(TIMES, SWAY, "y", scale=100.0)

scene.play(
    parallel(
        plane.animate.create().duration(0.8),
        stagger(tower.animate.fade_in().duration(0.6), *[line.animate.fade_in().duration(0.3) for line in floors], each=0.06),
    )
)
scene.play([curve.animate.create().duration(2.2), peaks.animate.fade_in().duration(0.5)])
scene.wait(2.0)

# ---------------------------------------------------------------------------
# Segmento 3: measure_text dimensiona una caja exactamente al pie
# ---------------------------------------------------------------------------

scene.segment("Medición", notes="measure_text reemplaza anchos a ojo.")
footnote = "u(t) = e^{-ζ ω t}·sin Δ t = 0.02 s"
width, height = scene.text.measure(footnote, role="caption")
box = scene.geometry.rounded_rect(width , height + 36, 16).fill(GOLD).opacity(0.12).move_to(0, -380)
label = scene.text.equation(footnote, role="caption").move_to(0, -380)
scene.play(parallel(box.animate.grow_from_center().duration(0.5), label.animate.write().duration(0.6)))
scene.wait(1.2)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [1.0, 3.0, 4.5, 6.0, 8.5])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
