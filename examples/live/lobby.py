"""Lobby: the room the audience joins before the first question, slot by slot.

``scene.audience()`` draws nothing: it gives the QR code and session code to
join, and for each slot, 0 for the first player in, a live nickname and
parameters that tell whether the slot is taken and for how long. A scene
that uses it asks each phone for a nickname as soon as it opens the page.
Outside a live presentation the rehearsal's players join one after another
until the room's first stop. Each label is the code that builds what it shows.
"""

import os

from gaanim import Scene, Zones, computed

scene = Scene()
foreground = scene.canvas.color("foreground")
background = scene.canvas.color("background")
accent = scene.canvas.color("accent")
track = scene.canvas.color("header")
# The rehearsal's 14 players join one after another over 6 seconds.
scene.rehearsal(14, seed=5, arrive=6.0)
audience = scene.audience()
# Phones ask for one more thing when joining; only the saved results show it.
scene.roster("Student code")
SLOTS = 6
seeks = []


def code(label, zone, anchor="left"):
    """A variant label: the code that builds it."""
    return scene.text(label, role="code").place(zone, anchor=anchor)


# Each slot while the room fills: its nickname, joined (0 while empty, then
# 1) and age, the seconds since its player joined, up to 60.
scene.segment("slots")
grid = scene.layout.zones(Zones.grid(rows=4, columns=["7fr"] + ["2fr"] * SLOTS, column_gap="32px",
                                     padding="page_padding"))
code("slot", grid[0], anchor="right")
for slot in range(SLOTS):
    code(str(slot), grid[1 + slot], anchor="center")
code("audience.name(slot)", grid[7], anchor="right")
code("audience.joined(slot)", grid[14], anchor="right")
code("audience.age(slot)", grid[21], anchor="right")
for slot in range(SLOTS):
    audience.name(slot, size=0.55).place(grid[8 + slot])
    cell = grid[15 + slot]
    scene.geometry.circle(0.3).fill(track).no_stroke().place(cell)
    scene.geometry.circle(0.3).fill(accent).no_stroke().place(cell).opacity(audience.joined(slot))
    scene.viz.readout(audience.age(slot), format=".1f", font_size=0.5).place(grid[22 + slot])
start = scene.cursor
scene.wait(2.5)
seeks.extend([start + 0.8, scene.cursor - 0.05])

# The lobby: the QR code on a light card, the session code for phones that
# cannot scan, the head count, and a grid of nicknames that pop in as their
# players join: age drives each name's scale.
scene.segment("room")
page = scene.layout.zones(Zones.columns(["5fr", "11fr"], gap="96px", padding="page_padding"))
join = page[0].split(Zones.rows(["6fr", "1fr", "1fr", "1fr", "1fr", "1fr"], gap="8px"))
card = scene.geometry.rounded_rect(4.0, 4.0, 0.2).fill(foreground).no_stroke().place(join[0])
audience.qr(3.4).fill(background).place(card)
code("audience.qr(3.4)", join[1], anchor="center")
scene.text(audience.code, size=0.6, weight=700).place(join[2])
code("audience.code", join[3], anchor="center")
scene.viz.readout(audience.count(), format=".0f", suffix=" joined", font_size=0.55).place(join[4])
code("audience.count()", join[5], anchor="center")
room, caption = page[1].split(Zones.rows(["7fr", "1fr"], gap="24px"))
seats = room.split(Zones.grid(rows=4, columns=4, gap="24px"))
for slot, seat in enumerate(seats):
    scene.geometry.rounded_rect(seat.width, seat.height, 0.15).fill(track).no_stroke().place(seat)
    # Grows from 0 to full size in the 0.4 s after the slot's player joins.
    pop = computed(lambda age: min(age / 0.4, 1), inputs=[audience.age(slot)])
    audience.name(slot, size=0.6, weight=700).place(seat).scale_to(pop)
code("audience.name(slot).scale_to(pop)", caption, anchor="center")
start = scene.cursor
scene.wait(4.0)
seeks.extend([start + 0.8, start + 2.2, scene.cursor - 0.05])
# Presenting, the room's stop advances by itself once 12 players are in.
scene.stop("room", until=audience.at_least(12))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
