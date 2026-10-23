"""Pointer: a mouse pointer that moves to targets, clicks and drags for software demonstrations.

``scene.ui.pointer`` draws a cursor whose tip is its pivot and keeps track of
where that tip is. ``animate.to`` takes it to a point, an object or a zone,
straight or along an arc; ``animate.click`` ripples at the tip; and
``animate.drag`` carries an object along. Each label is the code that builds
or animates its row.
"""

import os

from gaanim import Scene, Zones

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")
geometry = scene.geometry


def rows(labels, tracks=("2fr", "3fr")):
    """One row per label, the label to the left; return each row's stage."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(labels), columns=list(tracks), column_gap="48px", padding="page_padding")
    )
    for index, label in enumerate(labels):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
    return [grid[2 * index + 1] for index in range(len(labels))]


def button(stage, label):
    """A button on the right of ``stage``, a little in from its edge."""
    box = scene.layout.box(label, padding=("14px", "28px"), background=panel, border=rule, radius="10px")
    return box.place(stage.inset((0, "40px")), anchor="right")


# The three styles, each parked on its own stage; the tip is the point they
# point at, so the dot marks where a click would land.
scene.segment("styles")
stages = rows(['ui.pointer(style="arrow")', 'ui.pointer(style="hand")', 'ui.pointer(style="text")'])
for stage, style in zip(stages, ("arrow", "hand", "text")):
    x, y = stage.point("left")
    geometry.circle(0.04).fill(accent).move_to(x + 1.0, y)
    scene.ui.pointer(style=style, size=2.0, at=(x + 1.0, y))
scene.wait(1.0)

# animate.to takes the tip to a drawable's anchor, straight or along an
# arc, and animate.click ripples there while the pointer presses.
scene.segment("move and click")
stages = rows(
    ["animate.to(save)\nanimate.click()", "animate.to(open, arc=0.3)", 'animate.to(share, anchor="top_left")'],
    tracks=("3fr", "1fr"),
)
save = button(stages[0], "Save")
open_ = button(stages[1], "Open")
share = button(stages[2], "Share")
cursor = scene.ui.pointer(size=2.0, at=stages[0].point("left"))
scene.play(cursor.animate.to(save).duration(0.9))
scene.play(cursor.animate.click(color=success))
scene.play(cursor.animate.to(open_, arc=0.3).duration(0.9))
scene.play(cursor.animate.click())
scene.play(cursor.animate.to(share, anchor="top_left").duration(0.7))
scene.wait(0.5)

# animate.drag moves the tip and an object together, here a chip onto a slot.
scene.segment("drag")
stages = rows(["animate.drag(slot, chip)"])
slot = geometry.rounded_rect(1.6, 0.9, 0.12).no_fill().stroke(rule, 0.03).place(stages[0].inset((0, "40px")), anchor="right")
chip = geometry.rounded_rect(1.2, 0.6, 0.1).fill(accent).place(stages[0].inset((0, "40px")), anchor="left")
cursor = scene.ui.pointer(style="hand", size=2.0, at=stages[0].point("center"))
scene.play(cursor.animate.to(chip).duration(0.6))
scene.play(cursor.animate.drag(slot, chip, duration=1.0))
scene.play(cursor.animate.click(color=success))
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The styles; the first click mid-ripple; the arc halfway; the second
    # click; the last move done; the chip halfway to its slot; the slot's click.
    scene.snapshots(snapshots, [0.9, 2.2, 2.8, 3.5, 4.5, 5.98, 6.7])
else:
    scene.render()
