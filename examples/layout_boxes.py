"""The box model: padding, border, radius, shadow, clipping, styles and classes."""

import os

from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper")
L = scene.layout

# Named styles, like CSS classes; later definitions and inline props win.
L.classes(
    card=BoxStyle(gap="10px", padding="22px", radius="16px", background="#f8fafc",
                  border="#cbd5e1", border_width="2px"),
    elevated=BoxStyle(shadow={"color": "#0f172a33", "x": "0px", "y": "-6px", "blur": "18px"}),
    pill=BoxStyle(direction="row", padding=("5px", "14px"), radius="full", font_size="20px",
                  color="white", background="#4f46e5"),
    muted=BoxStyle(color="#64748b", font_size="20px"),
)
tag = BoxStyle(direction="row", padding=("4px", "12px"), radius="6px", font_size="18px",
               background="#e0f2fe", color="#075985")

cards = L.row(
    L.box(
        L.box("Tarjeta simple", font_size="30px", weight=700),
        L.box("Padding, borde y radio en px.", class_="muted", line_spacing=1.4),
        class_="card", width="300px",
    ),
    L.box(
        L.box("Con sombra", font_size="30px", weight=700),
        L.row(L.box("Nuevo", class_="pill"), L.box("v2", style=tag), gap="8px"),
        class_="card elevated", width="300px",
    ),
    L.box(
        L.box("Recorte", font_size="30px", weight=700),
        # clip=True cuts the oversized circle at the rounded box's edge.
        L.stack(scene.geometry.circle(1.2).fill("#f97316").item(anchor="top"), height="110px", width="fill", clip=True,
                background="#fff7ed", radius="12px"),
        class_="card", width="300px", background="#fffbeb", border="#fcd34d",
    ),
    gap="28px", align="start",
)
pills = L.row(*[L.box(name, class_="pill") for name in ("Diseño", "Datos", "$3.1k", "50%")],
              gap="10px", wrap=True)
page = L.column(
    L.box("Modelo de caja", role="title"),
    cards,
    pills,
    gap="36px", align="center", justify="center", within="safe", width="fill", height="fill",
)

scene.play([page.animate.fade_in().duration(0.6)])
scene.wait(0.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.3, 0.8])
else:
    scene.render()
