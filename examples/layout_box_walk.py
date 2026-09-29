"""Traverse a box: walk, find, each, cascade, stagger and child reordering."""

import os

from gaanim import Box, BoxStyle, Scene, Text

scene = Scene(frame=(16, 9), theme="paper")
L = scene.layout
chip = BoxStyle(padding="14px", radius="10px", background="#e0e7ff", font_size="26px", width="130px",
                align="center")

row = L.row(*[L.box(name, style=chip) for name in ("Uno", "Dos", "Tres")], gap="16px")
card = L.column(
    L.box("Tarjeta", role="title"),
    row,
    L.box("pie", font_size="22px", color="#475569"),
    gap="24px", padding="30px", align="center",
    background="#f8fafc", border="#cbd5e1", radius="18px",
)

# walk lists backgrounds and children at any depth, in draw order.
assert len(card.walk()) > len(card.children)
assert card.find(type=Box, boxes=True) is not None

# Nothing shows before its turn, then the card reveals piece by piece.
marks = [0.3, 1.4]
scene.wait(0.5)
scene.play(card.cascade(each=0.12).grow_from_center())
scene.wait(0.3)
marks.append(scene.cursor)
scene.stop("cascade")

# find searches the pieces by type and predicate.
title = card.find(text="Tarjeta")
chips = row.find_all(type=Box)
assert len(chips) == 3 and title is not None and card.find(text="nada") is None
# each applies an immediate setter to every matching piece.
card.each(lambda piece: piece.fill("#1e3a8a"), type=Text)
scene.play([title.animate.scale_by(1.15).duration(0.4)])

# stagger builds one animation per piece; None skips a piece.
scene.play(row.stagger(lambda piece: piece.animate.shift_by(0, -0.15).duration(0.3)
                       if isinstance(piece, Box) else None, boxes=True, each=0.12))
scene.wait(0.3)
marks.append(scene.cursor)
scene.stop("find y stagger")

# Reordering slides the children to their new places.
row.swap(row[0], row[2], duration=0.6)
scene.wait(0.3)
marks.append(scene.cursor)
row.move_child(row[0], -1, duration=0.6)
scene.wait(0.3)
marks.append(scene.cursor)
row.reverse(duration=0.6)
scene.wait(0.3)
marks.append(scene.cursor)
scene.stop("reordenar")

scene.play(card.cascade(each=0.08, origin="center").fade_out())
marks.append(scene.cursor - 0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, marks)
else:
    scene.render()
