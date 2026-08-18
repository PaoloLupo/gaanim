"""Una serie medida conduce la escena: plot de datos nativo y drivers temporales.

Cubre, en orden de aparición:
- rol tipográfico "kicker" y badges auto-dimensionados (scene.badge),
- plot_data / scatter_data sobre un cartesian_2d en coordenadas de datos,
- drive_from_samples: serie muestreada nativa que anima sin callbacks,
- Succession / LaggedStart / AnimationGroup para componer animaciones,
- measure_text para dimensionar cajas a partir del texto.

Set GAANIM_SNAPSHOTS to capture this scene before opening the viewer.
"""

import math
import os

from gaanim import CYAN, GOLD, AnimationGroup, Axis, LaggedStart, Scene, Succession

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
kicker = scene.text("GAANIM · PLOT DE DATOS Y DRIVERS NATIVOS", role="kicker").at(0, 430)
title = scene.text("Una serie medida conduce la escena", role="title").at(0, 340)
tag_source = scene.badge("serie muestreada nativa", color=CYAN).at(-330, 180)
tag_sway = scene.badge("drive_from_samples", color=GOLD).at(330, 180)

scene.play(
    Succession(kicker.fade_in(0.4), title.write(0.8))
    + LaggedStart(tag_source.grow_from_center(0.5), tag_sway.grow_from_center(0.5), lag=0.15)
)
scene.wait(0.8)

# ---------------------------------------------------------------------------
# Segmento 2: plot_data / scatter_data + edificio con sway nativo
# ---------------------------------------------------------------------------

scene.segment("Plot de datos", notes="plot_data en coordenadas del plano; sway sin updater de Python.")
plane = scene.cartesian_2d(
    Axis.linear(0, DURATION).ticks(1).label("tiempo (s)"),
    Axis.linear(-1.0, 1.0).ticks(0.5).label("u(t)"),
    width=880,
    height=460,
).at(-460, -60)
curve = plane.plot_data(TIMES, SWAY, color=CYAN, width=4)
peaks = plane.scatter_data(
    [TIMES[i] for i in range(0, len(TIMES), 4)],
    [SWAY[i] for i in range(0, len(TIMES), 4)],
    radius=4.0,
    color=GOLD,
)

ground = scene.line(695, 120, 865, 120).stroke(CYAN, 5)
building = scene.rounded_rect(150, 320, 12).fill(CYAN).opacity(0.25).at(780, -40)
floors = [
    scene.line(713, 120 - level * 55, 847, 120 - level * 55).stroke(GOLD, 2)
    for level in range(1, 6)
]
tower = scene.group([ground, building, *floors])
# La misma serie, ahora conduciendo la posición X del edificio: sin closures,
# sin interpolate() manual, y con seek determinista gratis.
tower.drive_from_samples(TIMES, SWAY, "y", scale=100.0)

scene.play(
    AnimationGroup(
        plane.create(0.8),
        LaggedStart(tower.fade_in(0.6), *[line.fade_in(0.3) for line in floors], lag=0.06),
    )
)
scene.play([curve.create(2.2), peaks.fade_in(0.5)])
scene.wait(2.0)

# ---------------------------------------------------------------------------
# Segmento 3: measure_text dimensiona una caja exactamente al pie
# ---------------------------------------------------------------------------

scene.segment("Medición", notes="measure_text reemplaza anchos a ojo.")
footnote = "u(t) = e^{-ζ ω t}·sin Δ t = 0.02 s"
width, height = scene.measure_text(footnote, role="caption")
box = scene.rounded_rect(width , height + 36, 16).fill(GOLD).opacity(0.12).at(0, -380)
label = scene.equation(footnote, role="caption").at(0, -380)
scene.play(AnimationGroup(box.grow_from_center(0.5), label.write(0.6)))
scene.wait(1.2)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [1.0, 3.0, 4.5, 6.0, 8.5])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
