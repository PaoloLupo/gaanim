"""Live zones: the audience's characters inside the scene, posed every frame by a Python function.

``scene.live_zone(audience, behavior)`` brings each player in as the
character they made on their phone, and ``behavior(p)`` returns where it
stands and how it looks with ``gaanim.live.pose``. The function is compiled
when the scene is authored, so a ``.gaanim`` runs it without Python. The
zone's options tune how the engine deforms characters as they move and how
it draws their nicknames. Outside a live presentation the rehearsal's
players fill the zones. Each label is the code that builds what it shows.
"""

import math
import os

from gaanim import Scene, Zones
from gaanim.live import pose

scene = Scene()
rule = scene.colors.rule
# Ten made-up players, all in the room from the start.
scene.rehearsal(10, seed=3, arrive=0)
audience = scene.audience()
seeks = []


def code(label, zone, anchor="left"):
    """A variant label: the code that builds it."""
    return scene.text(label, role="code").place(zone, anchor=anchor)


def outline(zone):
    """A rule-colored frame around ``zone``."""
    return scene.geometry.rect(zone.width, zone.height).no_fill().stroke(rule, 0.02).place(zone)


def feet(zone, lift=0.5):
    """Where a character stands in ``zone``: centered, ``lift`` above its bottom edge."""
    return zone.center[0], zone.bottom + lift




# pose() places a character by its feet and changes how it looks. Here each
# player takes one cell by p.index, its order of arrival, and one variant.
scene.segment("pose")
POSES = ["pose(x, y)", "rotation=0.6", "scale=1.5", "sx=1.4, sy=0.7",
         "lean=0.35", "look_x=-1", "flip=True", "show_name=False"]
cells = scene.layout.zones(Zones.grid(rows=2, columns=4, gap="24px", padding="page_padding"))
for label, cell in zip(POSES, cells):
    outline(cell)
    scene.text(label, role="code").place(cell, anchor="top", padding="16px")
# Module constants are frozen into the behavior when the zone compiles it.
SPOTS = tuple(feet(cell, 0.7) for cell in cells)


def one_pose(p):
    x, y = SPOTS[p.index % 8]
    if p.index == 0:
        return pose(x, y)
    if p.index == 1:
        return pose(x, y, rotation=0.6)
    if p.index == 2:
        return pose(x, y, scale=1.5)
    if p.index == 3:
        return pose(x, y, sx=1.4, sy=0.7)
    if p.index == 4:
        return pose(x, y, lean=0.35)
    if p.index == 5:
        return pose(x, y, look_x=-1)
    if p.index == 6:
        return pose(x, y, flip=True)
    return pose(x, y, show_name=False, visible=p.index == 7)


scene.live_zone(audience, one_pose, size=1.8, names=True, name_size=0.28)
scene.wait(1.5)
seeks.append(scene.cursor - 0.05)

# express plays an expression when the character appears, or at since, the
# p.t it starts; loop=True repeats it until another one.
scene.segment("expressions")
EXPRESSIONS = ["happy", "sad", "hurt", "winner", "surprised"]
page = scene.layout.zones(Zones.grid(rows=3, columns=["5fr"] + ["2fr"] * len(EXPRESSIONS), column_gap="32px",
                                     row_gap="24px", padding="page_padding"))
code("pose(x, y, ...)", page[0], anchor="right")
code("express=e", page[len(EXPRESSIONS) + 1], anchor="right")
code("express=e, loop=True", page[2 * (len(EXPRESSIONS) + 1)], anchor="right")
for index, expression in enumerate(EXPRESSIONS):
    code(expression, page[1 + index], anchor="center")
FACES = tuple(feet(page[row * (len(EXPRESSIONS) + 1) + 1 + column], 0.4)
              for row in (1, 2) for column in range(len(EXPRESSIONS)))


def express(p):
    x, y = FACES[p.index % 10]
    looping = p.index >= 5
    face = p.index % 5
    if face == 0:
        return pose(x, y, express="happy", loop=looping)
    if face == 1:
        return pose(x, y, express="sad", loop=looping)
    if face == 2:
        return pose(x, y, express="hurt", loop=looping)
    if face == 3:
        return pose(x, y, express="winner", loop=looping)
    return pose(x, y, express="surprised", loop=looping)


scene.live_zone(audience, express, size=2.2)
start = scene.cursor
scene.wait(3.0)
seeks.extend([start + 0.6, start + 1.4, scene.cursor - 0.05])

# The engine stretches characters along their velocity (squash, up to
# max_stretch), leans them into horizontal motion (lean, up to max_lean),
# swings their hats and ears (follow) and turns their eyes toward where they
# go (look); 0 turns each off. Each row is its own zone, clipped to bounds.
scene.segment("motion")
ROWS = [
    ("live_zone(audience, run)", {}),
    ("squash=0, lean=0", {"squash": 0, "lean": 0}),
    ("squash=0.12, max_stretch=1.8", {"squash": 0.12, "max_stretch": 1.8}),
    ("lean=0.3, max_lean=0.6", {"lean": 0.3, "max_lean": 0.6}),
    ("follow=0, look=0", {"follow": 0, "look": 0}),
]
grid = scene.layout.zones(Zones.grid(rows=len(ROWS), columns=["6fr", "9fr"], column_gap="48px", row_gap="16px",
                                     padding="page_padding"))
LANE = (grid[1].left + 0.6, grid[1].right - 0.6)
FLOOR = 0.0


def run(p):
    """Three runners dash across the lane and back, hopping as they go."""
    phase = 2 * math.pi * (p.t / 2.4 + p.index * 0.12)
    x = LANE[0] + (LANE[1] - LANE[0]) * (0.5 - 0.5 * math.cos(phase))
    hop = 0.3 * abs(math.sin(3 * phase))
    return pose(x, FLOOR + hop, visible=p.index < 3, show_name=False)


for row, (label, options) in enumerate(ROWS):
    lane = grid[2 * row + 1]
    code(label, grid[2 * row], anchor="right")
    outline(lane)
    # run() reads FLOOR as it is when this row's zone compiles it.
    FLOOR = lane.bottom + 0.1
    scene.live_zone(audience, run, bounds=(lane.left, lane.bottom, lane.right, lane.top), size=1.1, **options)
start = scene.cursor
scene.wait(2.4)
seeks.extend([start + 0.3, start + 0.9, start + 1.5])

# names=True draws each nickname under its character's feet: name_size tall,
# name_gap below them, in name_weight. Characters stay inside bounds, and
# zone.close() ends a zone before its segment does.
scene.segment("names")
ROWS = [
    ("names=True", {"names": True}),
    ("names=True, name_size=0.4", {"names": True, "name_size": 0.4}),
    ("name_gap=0.3, name_weight=400", {"names": True, "name_gap": 0.3, "name_weight": 400}),
    ("bounds=(x0, y0, x_mid, y1)", {}),
    ("zone.close()  # after 1 s", {}),
]
grid = scene.layout.zones(Zones.grid(rows=len(ROWS), columns=["7fr", "8fr"], column_gap="48px", row_gap="16px",
                                     padding="page_padding"))
LANE = (grid[1].left + 0.6, grid[1].right - 0.6)
zones = []


def stand(p):
    """Five players in a row, bobbing in turn."""
    x = LANE[0] + (LANE[1] - LANE[0]) * (p.index % 5) / 4
    bob = 0.1 * abs(math.sin(2 * math.pi * (p.t / 1.2 + p.index * 0.2)))
    return pose(x, FLOOR + bob, visible=p.index < 5)


for row, (label, options) in enumerate(ROWS):
    lane = grid[2 * row + 1]
    code(label, grid[2 * row], anchor="right")
    outline(lane)
    FLOOR = lane.bottom + 0.55
    right = lane.center[0] if label.startswith("bounds") else lane.right
    zones.append(scene.live_zone(audience, stand, bounds=(lane.left, lane.bottom, right, lane.top), size=0.9,
                                 **options))
start = scene.cursor
scene.wait(1.0)
zones[-1].close()
scene.wait(1.0)
seeks.extend([start + 0.5, scene.cursor - 0.05])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
