"""Rows and columns: justify, align, grow, basis, shrink, margins and wrapping."""

import os

from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper", margin=0.4)
L = scene.layout
chip = BoxStyle(padding=("4px", "10px"), background="#bfdbfe", radius="6px", font_size="18px")
lane = BoxStyle(direction="row", width="fill", padding="6px", background="#f1f5f9", radius="8px", gap="6px")


def label(text):
    return L.box(text, font_size="16px", color="#475569", width="150px")


rows = []
for justify in ("start", "center", "end", "between", "around", "evenly"):
    rows.append(L.row(label(f"justify={justify}"),
                      L.row(*[L.box(str(i), style=chip) for i in range(3)], style=lane, justify=justify, grow=1),
                      align="center", width="fill"))
for align in ("start", "center", "end", "stretch"):
    rows.append(L.row(label(f"align={align}"),
                      L.row(L.box("alto", style=chip, padding=("16px", "10px")), L.box("a", style=chip),
                            L.box("b", style=chip), style=lane, align=align, height="72px", grow=1),
                      align="center", width="fill"))
grow = L.row(L.box("grow=1", style=chip).item(grow=1), L.box("grow=2", style=chip).item(grow=2),
             L.box("fijo 120px", style=chip, width="120px"), style=lane)
rows.append(L.row(label("grow"), grow.item(grow=1), align="center", width="fill"))
margins = L.row(L.box("margin 20px", style=chip).item(margin=("0px", "20px")),
                L.box("sin margen", style=chip), L.box("margin auto →", style=chip).item(margin=(0, 0, 0, "auto")),
                style=lane)
rows.append(L.row(label("margin"), margins.item(grow=1), align="center", width="fill"))
wrapped = L.row(*[L.box(f"etiqueta {i}", style=chip) for i in range(24)], style=lane, wrap=True)
rows.append(L.row(label("wrap"), wrapped.item(grow=1), align="center", width="fill"))

page = L.column(*rows, gap="8px", width="fill", within="safe")
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
