"""Grids: fixed, fractional, percentage and auto tracks, spans and flow direction."""

import os

from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper", margin=0.5)
L = scene.layout
cell = BoxStyle(padding="10px", background="#dbeafe", radius="8px", font_size="18px",
                align="center", justify="center")
accent = cell.but(background="#fde68a")

# Tracks: 160px fixed, 1fr and 2fr share the rest; rows hug their content.
bento = L.grid(
    L.box("A: 2 filas", style=accent).item(row_span=2),
    L.box("B: 2 columnas", style=accent).item(column_span=2),
    L.box("C", style=cell),
    L.box("D", style=cell),
    L.box("E: 2 columnas", style=accent).item(column_span=2),
    L.box("F", style=cell),
    columns=["160px", "1fr", "2fr"],
    rows=["70px", "70px", "auto"],
    gap="12px", width="fill",
)
# Column-major flow fills each column before the next.
flow = L.grid(*[L.box(str(i), style=cell) for i in range(6)], rows=3, columns=2,
              auto_flow="column", gap="8px", width="300px")
# Percentage tracks (of the grid's width, gaps aside) and alignment in cells.
aligned = L.grid(
    L.box("inicio", style=cell).item(align_self="start"),
    L.box("centro", style=cell).item(align_self="center"),
    L.box("fin", style=cell).item(align_self="end"),
    L.box("estirado", style=cell),
    columns=["20%", "20%", "20%", "1fr"], rows=["90px"], gap="8px", width="fill",
    background="#f1f5f9", radius="10px", padding="8px",
)
page = L.column(
    L.box("Cuadrículas", role="title"),
    bento,
    L.row(flow, aligned.item(grow=1), gap="24px", align="center", width="fill"),
    gap="28px", within="safe", width="fill", height="fill", justify="center",
)
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
