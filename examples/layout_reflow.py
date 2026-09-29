"""Animated structure: add, remove, replace, set and item changes reflow smoothly."""

import os

from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper")
L = scene.layout
card = BoxStyle(padding="18px", radius="14px", background="#e0e7ff", font_size="26px", width="150px",
                align="center")

board = L.grid(*[L.box(name, style=card) for name in ("Uno", "Dos", "Tres")],
               columns=3, gap="18px")
status = L.box("Tres tarjetas", font_size="26px", color="#475569")
page = L.column(L.box("Reacomodo animado", role="title"), board, status, gap="40px", align="center")
scene.play([page.animate.fade_in().duration(0.5)])
scene.stop("inicio")

# Each change moves the others to their new places over its duration.
board.add(L.box("Nueva", style=card, background="#bbf7d0"), at=1, duration=0.6)
status.set(background="#fef9c3", padding=("4px", "10px"), radius="6px")
board.set(columns=2, duration=0.8)
scene.stop("dos columnas")

third = board[3]
board.remove(third, duration=0.6)
board.replace(board[0], L.box("Reemplazo", style=card, background="#fecaca"), duration=0.6)
scene.stop("reemplazo")

# Parallel changes: advance=False starts the next change at the same time.
board.set(gap="40px", duration=0.8, advance=False)
board[1].item(column_span=2, width="fill", duration=0.8)
scene.stop("fin")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 0.8, 1.1, 1.5, 1.9, 3.1, 3.5, 3.9])
else:
    scene.render()
