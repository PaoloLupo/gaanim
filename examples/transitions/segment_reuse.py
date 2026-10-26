"""Content across segments: reuse, persist and release, and transitions declared later with link.

Objects belong to the segment that creates them and leave with it. The first
tile is reused by the next segment, so it holds still through the cross-fade
and then moves on; the badge persists, so later transitions leave it alone
until it is released and goes out with its segment; the last transition is
added with ``scene.link`` after both segments exist. The label below the stage
is the code of the current step.
"""

import os

from gaanim import Scene, Transition, Zones

HOLD = 1.2

scene = Scene()
background = scene.colors.background
foreground = scene.colors.foreground
page = scene.layout.zones(Zones.rows(["6fr", "1fr"], gap="24px", padding="page_padding"))
stage = page[0]
captions = []


def caption(*lines):
    """Show ``lines`` of code below the stage from the cursor on, in place of the previous caption.

    Captions persist too, so no transition moves or clips them.
    """
    for label in captions[-1:]:
        label.opacity(0)
    strip = page[1].inset(("-12px", "-120px", "-120px"))
    rows = page[1].split(Zones.rows(len(lines)))
    label = scene.geometry.group(
        [scene.geometry.rect(strip.width, strip.height).fill(background).place(strip)]
        + [scene.text(line, role="code").place(row, anchor="left") for line, row in zip(lines, rows)]
    )
    scene.persist(label)
    captions.append(label)


def card(role, shape):
    """Fill the stage with a card of ``role`` holding a large dark ``shape()``."""
    scene.geometry.rounded_rect(stage.width, stage.height, 0.3).fill(scene.colors[role]).place(stage)
    shape().fill(background).place(stage.inset("96px"), fit="contain")


# Three tiles, each the content of the first segment.
starts = [scene.cursor]
scene.segment("tiles")
caption('scene.segment("tiles")')
slots = stage.split(Zones.columns(3, gap="48px"))
tiles = [
    scene.geometry.rounded_rect(slot.width, slot.height, 0.3).fill(scene.colors[role]).place(slot)
    for slot, role in zip(slots, ["accent", "chart", "danger"])
]
scene.wait(HOLD)

# reuse adopts a drawable of the previous segment: it holds still while the
# transition runs, then belongs to this segment and can move on.
caption('scene.segment("focus", Transition.cross_fade(0.8))', "scene.reuse(tiles[0])")
starts.append(scene.cursor)
scene.segment("focus", Transition.cross_fade(0.8))
scene.reuse(tiles[0])
lines, aside = stage.split(Zones.columns(["2fr", "1fr"], gap="48px"))
for row in lines.split(Zones.rows(3, gap="48px")):
    scene.geometry.rounded_rect(row.width, row.height, row.height / 2).fill(scene.colors.chart).place(row)
scene.wait(0.8)
scene.play([tiles[0].animate.move_to(*aside.center).duration(0.6)])

# From the cursor on, a persistent drawable is global: it stays visible in
# later segments and their transitions leave it alone. Its z_index keeps it
# above the cards once it is released.
caption("scene.persist(badge)")
badge = scene.geometry.star(5, 0.9, 0.4).fill(foreground).place(aside).z_index(1)
scene.persist(badge)
scene.play([badge.animate.grow_from_center().duration(0.4)])
scene.wait(0.6)

caption('scene.segment("push", Transition.push(0.8, "left"))')
starts.append(scene.cursor)
scene.segment("push", Transition.push(0.8, "left"))
card("success", lambda: scene.geometry.regular_polygon(3, 1.0))
scene.wait(HOLD)

# release attaches it to the segment again: it holds still through this
# segment's transition and leaves with the segment at the next one.
caption('scene.segment("release", Transition.slide(0.8, "up"))', "scene.release(badge)")
starts.append(scene.cursor)
release_segment = scene.segment("release", Transition.slide(0.8, "up"))
scene.release(badge)
card("danger", lambda: scene.geometry.circle(1.0))
scene.wait(HOLD)

# link sets the transition between two declared segments, here after the
# fact; it replaces whatever transition the second one had.
caption('scene.link(release_segment, end_segment, Transition.wipe(0.8, "right"))')
starts.append(scene.cursor)
end_segment = scene.segment("end")
card("accent", lambda: scene.geometry.square(1.6))
scene.wait(HOLD)
scene.link(release_segment, end_segment, Transition.wipe(0.8, "right"))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The tiles; the cross-fade around the reused tile and the tile moved
    # on; the badge; then the push, the slide and the wipe halfway.
    scene.snapshots(
        snapshots,
        [starts[0] + 0.6, starts[1] + 0.4, starts[1] + 1.4, starts[1] + 2.2]
        + [start + 0.4 for start in starts[2:]],
    )
else:
    scene.render()
