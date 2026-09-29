"""Box.reveal on a slide: texts slide in, rules grow, connectors and growing stacks sit in their boxes."""

import os

from gaanim import Scene

scene = Scene(frame=(16, 9), theme="paper", margin=0.5)
L = scene.layout


def lines(count, label):
    return L.column(*[L.box(f"{label} línea {i}", font_size="26px") for i in range(count)], gap="6px")


def step(name, color):
    # A connector with fixed points is placed by its box, like an arrow.
    arrow = scene.geometry.connector((0, 0), (1.4, 0))
    return L.box(L.box(name, font_size="30px", weight=700), arrow, gap="10px", padding="16px",
                 background=color, radius="12px", width="240px")


header = L.column(
    L.box("Cómo funciona", role="title"),
    L.box(height="4px", width="fill", background="#4f46e5"),
    gap="10px", width="fill",
)
flow = L.row(step("Entrada", "#e0e7ff"), step("Proceso", "#dbeafe"), step("Salida", "#dcfce7"), gap="24px")

# Two growing cards in a column that hugs its content: it is as tall as both.
stack = L.column(
    L.box(lines(3, "uno"), background="#fee2e2", padding="14px", radius="10px").item(grow=1),
    L.box(lines(3, "dos"), background="#fef9c3", padding="14px", radius="10px").item(grow=1),
    gap="14px",
)
side = L.box("Nota al margen", background="#f1f5f9", padding="14px", radius="10px", font_size="26px")
body = L.row(stack, side, align="center", gap="30px")
footer = L.box("Pie de la diapositiva", font_size="26px", color="#475569")
slide = L.column(header, flow, body, footer, gap="30px", padding="20px", width="fill", height="fill",
                 background="#ffffff")

scene.wait(0.3)
scene.play(slide.reveal(each=0.07, duration=0.4))
scene.wait(0.4)
assert not slide.diagnostics(), slide.diagnostics()

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.7, 1.2, scene.cursor - 0.2])
else:
    scene.render()
