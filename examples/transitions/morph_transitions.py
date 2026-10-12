"""Morph transitions: drawables that travel across the cut, paired by hand or by name.

``Transition.morph`` carries each (source, target) pair from the outgoing
segment into the incoming one, changing place, size, color and shape, while
unpaired content cross-fades. Its pairs need both segments, so it is declared
with ``scene.link`` afterwards. ``Transition.magic_move`` finds the pairs
itself, by the names given with ``Drawable.named``. The label below the stage
is the transition that brought the current segment in.
"""

import os

from gaanim import Easing, Scene, Transition, Zones

HOLD = 1.4

scene = Scene()
background = scene.canvas.color("background")
page = scene.layout.zones(Zones.rows(["6fr", "1fr"], gap="24px", padding="page_padding"))
stage = page[0]
captions = []


def color(role):
    """The theme color of ``role``."""
    return scene.canvas.color(role)


def caption(*lines):
    """Show ``lines`` of code below the stage from the cursor on, in place of the previous caption.

    Persistent drawables stay out of transitions, so the caption reads through them.
    """
    for label in captions[-1:]:
        label.opacity(0)
    rows = page[1].split(Zones.rows(len(lines)))
    label = scene.geometry.group([scene.text(line, role="code").place(row, anchor="left") for line, row in zip(lines, rows)])
    scene.persist(label)
    captions.append(label)


# Five bars standing on the bottom of the stage.
starts = [scene.cursor]
bars_segment = scene.segment("bars")
columns = stage.split(Zones.columns(5, gap="48px"))
bars = []
for column, share in zip(columns, [0.45, 0.7, 1.0, 0.6, 0.35]):
    bar = scene.geometry.rect(column.width, column.height * share)
    bars.append(bar.place(column, anchor="bottom"))
scene.wait(HOLD)

# The tallest bar becomes a card holding three dots; the other bars fade.
caption("scene.link(bars_segment, card_segment,", "Transition.morph(0.8, pairs=[(bars[2], card)]))")
starts.append(scene.cursor)
card_segment = scene.segment("card")
card = scene.geometry.rounded_rect(stage.width * 0.6, stage.height, 0.3).fill(color("success")).place(stage)
slots = stage.inset((0, stage.width * 0.2)).split(Zones.columns(3, gap="48px"))
dots = [scene.geometry.circle(1.0).fill(background).place(slot.inset("72px"), fit="contain") for slot in slots]
scene.wait(HOLD)
scene.link(bars_segment, card_segment, Transition.morph(0.8, pairs=[(bars[2], card)]))

# Several pairs travel together, here eased. Each tile is named, for the
# magic_move below.
caption("scene.link(card_segment, tiles_segment,",
        "Transition.morph(0.8, pairs=pairs, easing=Easing.SMOOTH))")
starts.append(scene.cursor)
tiles_segment = scene.segment("tiles")
NAMES = ["first", "second", "third"]
ROLES = ["accent", "chart", "danger"]
tiles = [
    scene.geometry.rounded_rect(slot.width, slot.height, 0.3).fill(color(role)).place(slot).named(name)
    for slot, role, name in zip(stage.split(Zones.columns(3, gap="48px")), ROLES, NAMES)
]
scene.wait(HOLD)
pairs = list(zip(dots, tiles))
scene.link(card_segment, tiles_segment, Transition.morph(0.8, pairs=pairs, easing=Easing.SMOOTH))

# magic_move pairs drawables that share a name; the new bar matches nothing,
# so it fades in.
caption("Transition.magic_move(0.8)")
starts.append(scene.cursor)
scene.segment("column", Transition.magic_move(0.8))
left, right = stage.split(Zones.columns(["1fr", "2fr"], gap="48px"))
for row, role, name in zip(left.split(Zones.rows(3, gap="24px")), ROLES, NAMES):
    scene.geometry.circle(1.0).fill(color(role)).place(row, fit="contain").named(name)
scene.geometry.rounded_rect(right.width, right.height, 0.3).fill(color("success")).place(right)
scene.wait(HOLD)

# Every transition takes an easing; a spring carries the stars past their
# places and back while the fades never go back.
caption("Transition.magic_move(0.8, easing=Easing.SNAPPY)")
starts.append(scene.cursor)
scene.segment("diagonal", Transition.magic_move(0.8, easing=Easing.SNAPPY))
cells = stage.split(Zones.grid(rows=3, columns=3, gap="24px"))
for cell, role, name in zip([cells[6], cells[4], cells[2]], ROLES, NAMES):
    scene.geometry.star(5, 1.0, 0.45).fill(color(role)).place(cell, fit="contain").named(name)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The bars at rest, then each transition halfway (the spring past its
    # targets) and its segment settled.
    scene.snapshots(
        snapshots,
        [starts[0] + 0.5] + [start + t for start in starts[1:-1] for t in (0.4, 1.2)]
        + [starts[-1] + 0.1, starts[-1] + 1.2],
    )
else:
    scene.render()
