"""Relative positioning: frame edges, next_to, align_to, move_to and constraints.

Free objects (outside any box) are placed by relations: against an edge or a
corner of the frame, beside another object, aligned with one of its anchors,
with one of their own anchors on a point, or by linear constraints between
edges, centers and sizes. In the first two segments each label is placed by
the code it shows; in the others it is the code that places the accent shape
beside it.
"""

import os

from gaanim import Anchor, Direction, Scene, Zones, stagger

scene = Scene()
L = scene.layout
header = scene.canvas.color("header")
rule = scene.canvas.color("rule")
foreground = scene.canvas.color("foreground")
seeks = []


def code(label):
    return scene.text(label, role="code", text_align="left")


def show(*objects):
    """Fade the objects in one after another, hold, and note the seeks."""
    start = scene.cursor
    scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in objects], each=0.04))
    scene.wait(1.0)
    # Mid-entrance and settled.
    seeks.extend([start + 0.3, scene.cursor - 0.1])


def rows(count, split=("1fr", "1fr")):
    """The frame cut into ``count`` rows of (label zone, example zone)."""
    grid = L.zones(Zones.grid(rows=count, columns=list(split), column_gap="48px", padding="page_padding"))
    return [(grid[2 * index], grid[2 * index + 1]) for index in range(count)]


def reference(zone, width=4.4, height=0.8, anchor="center"):
    """The object the others are placed against, in ``zone`` by ``anchor``."""
    return scene.geometry.rect(width, height).fill(header).stroke(rule, 0.02).place(zone, anchor=anchor)


def chip(label):
    """A box holding ``label``: the object that the label's code moves."""
    return L.box(label, role="code", font_size="44px", padding=("12px", "20px"), background=header, border=rule)


def table(variants, place):
    """One row per (label, args): the code on the left, ``place(zone, args)``'s objects on the right."""
    shown = []
    for (label_zone, zone), (label, args) in zip(rows(len(variants)), variants):
        shown += [code(label).place(label_zone, anchor="right"), *place(zone, args)]
    show(*shown)


# to_edge() and to_corner() push an object against the frame, buff away
# from it (0.24 by default). Each box here is moved by the code it holds.
scene.segment("edges")
show(
    chip("to_edge(Direction.UP)").to_edge(Direction.UP),
    chip("to_edge(Direction.LEFT)").to_edge(Direction.LEFT),
    chip("move_to(0, 0)").move_to(0, 0),
    chip("to_edge(Direction.RIGHT)").to_edge(Direction.RIGHT),
    chip("to_edge(Direction.DOWN, buff=1.2)").to_edge(Direction.DOWN, buff=1.2),
)

scene.segment("corners")
show(
    chip("to_corner(Anchor.TOP_LEFT)").to_corner(Anchor.TOP_LEFT),
    chip("to_corner(Anchor.TOP_RIGHT)").to_corner(Anchor.TOP_RIGHT),
    chip("to_corner(Anchor.TOP_LEFT, buff=1.2)").to_corner(Anchor.TOP_LEFT, buff=1.2),
    chip("to_corner(Anchor.BOTTOM_LEFT)").to_corner(Anchor.BOTTOM_LEFT),
    chip("to_corner(Anchor.BOTTOM_RIGHT)").to_corner(Anchor.BOTTOM_RIGHT),
)


def beside(zone, args):
    box = reference(zone)
    return [box, scene.geometry.square(0.6).next_to(box, *args)]


# next_to() puts an object beside another, spacing away (0.24 by default)
# in a direction; aligned_edge lines up one of their edges across it.
scene.segment("next_to")
table([
    ("next_to(box, Direction.RIGHT)", (Direction.RIGHT,)),
    ("next_to(box, Direction.LEFT, 0.8)", (Direction.LEFT, 0.8)),
    ("next_to(box, Direction.UP)", (Direction.UP,)),
], beside)

scene.segment("next_to_aligned")
table([
    ("next_to(box, Direction.DOWN,\n        0.24, Anchor.LEFT)", (Direction.DOWN, 0.24, Anchor.LEFT)),
    ("next_to(box, Direction.DOWN,\n        0.24, Anchor.RIGHT)", (Direction.DOWN, 0.24, Anchor.RIGHT)),
    ("next_to(box, Direction.UP_RIGHT)", (Direction.UP_RIGHT,)),
], beside)


def aligned(zone, anchors):
    box = reference(zone)
    return [box, scene.geometry.square(0.6).align_to(box, *anchors)]


# align_to() moves an object so that one of its anchors meets an anchor of
# another: the same one, or the second anchor given.
scene.segment("align_to")
table([
    ("align_to(box, Anchor.LEFT)", (Anchor.LEFT,)),
    ("align_to(box, Anchor.TOP_RIGHT)", (Anchor.TOP_RIGHT,)),
    ("align_to(box, Anchor.CENTER)", (Anchor.CENTER,)),
], aligned)

scene.segment("align_to_anchor")
table([
    ("align_to(box, Anchor.BOTTOM,\n         Anchor.TOP)", (Anchor.BOTTOM, Anchor.TOP)),
    ("align_to(box, Anchor.RIGHT,\n         Anchor.LEFT)", (Anchor.RIGHT, Anchor.LEFT)),
    ("align_to(box, Anchor.TOP_LEFT,\n         Anchor.BOTTOM_RIGHT)", (Anchor.TOP_LEFT, Anchor.BOTTOM_RIGHT)),
], aligned)


def on_point(zone, anchor):
    # The dot marks the point (x, y): the middle of the zone.
    x, y = zone.center
    square = scene.geometry.square(0.6)
    square = square.move_to(x, y) if anchor is None else square.move_to(x, y, anchor)
    return [square, scene.geometry.circle(0.08).fill(foreground).move_to(x, y)]


# move_to() puts an anchor of the object (its center by default) on a
# point, on another object's center or on a point that follows another
# object. bounds() measures an object where it ends up.
scene.segment("move_to")
table([
    ("move_to(x, y)", None),
    ("move_to(x, y, Anchor.TOP_LEFT)", Anchor.TOP_LEFT),
    ("move_to(x, y,\n        Anchor.BOTTOM_RIGHT)", Anchor.BOTTOM_RIGHT),
], on_point)

scene.segment("move_to_objects")
lanes = rows(3)
box = reference(lanes[0][1])
shown = [code("move_to(box)").place(lanes[0][0], anchor="right"), box, scene.geometry.square(0.6).move_to(box)]
box = reference(lanes[1][1])
shown += [code("move_to(box.anchor_point(\n    Anchor.TOP_RIGHT))").place(lanes[1][0], anchor="right"), box,
          scene.geometry.square(0.6).move_to(box.anchor_point(Anchor.TOP_RIGHT))]
word = scene.text("measured", size=1.0).place(lanes[2][1])
b = word.bounds()
shown += [code("rect(b.width, b.height)\n    .move_to(*b.center)").place(lanes[2][0], anchor="right"), word,
          scene.geometry.rect(b.width, b.height).no_fill().stroke(rule, 0.03).move_to(*b.center)]
show(*shown)

# constrain() solves linear relations between the left, right, top, bottom,
# center_x, center_y, width and height of objects, even objects in
# different boxes. Relations are required unless strong(), medium() or
# weak(); a relaxed weak one is reported by check_layout() under the
# label that named() gives it.
scene.segment("constrain")
SPLIT = ("5fr", "2fr")
lanes, shown = iter(rows(3, SPLIT)), []


def lane(label):
    """Place ``label`` in the next row and return the zone beside it."""
    label_zone, zone = next(lanes)
    shown.append(code(label).place(label_zone, anchor="right"))
    return zone


zone = lane("L.constrain(sq.left == box.right + 0.2,\n            sq.center_y == box.center_y)")
box, sq = reference(zone, 2.4), scene.geometry.square(0.6)
L.constrain(sq.left == box.right + 0.2, sq.center_y == box.center_y)
shown += [box, sq]

zone = lane("L.constrain(sq.center_x == box.center_x,\n            sq.bottom == box.top)")
box, sq = reference(zone, 2.4), scene.geometry.square(0.6)
L.constrain(sq.center_x == box.center_x, sq.bottom == box.top)
shown += [box, sq]

zone = lane("L.constrain(\n    sq.center_x == (a.right + b.left) / 2,\n    sq.center_y == a.center_y)")
a, b = reference(zone, 1.4, anchor="left"), reference(zone, 1.4, anchor="right")
sq = scene.geometry.square(0.6)
L.constrain(sq.center_x == (a.right + b.left) / 2, sq.center_y == a.center_y)
shown += [a, b, sq]
show(*shown)

# Sizes are expressions too, and when relations conflict the stronger one
# wins.
scene.segment("constrain_sizes")
lanes, shown = iter(rows(2, SPLIT)), []

zone = lane("L.constrain(bar.left == box.left,\n            bar.center_y == box.center_y,\n"
            "            bar.width == box.width * 0.6)")
box, bar = reference(zone, 3.0), scene.geometry.rect(1, 0.4)
L.constrain(bar.left == box.left, bar.center_y == box.center_y, bar.width == box.width * 0.6)
shown += [box, bar]

zone = lane("L.constrain(box.width == 2, sq.bottom == box.top,\n    (sq.center_x == box.left).weak(),\n"
            "    (sq.center_x == box.right).strong())")
box, sq = reference(zone, 2.0), scene.geometry.square(0.6)
L.constrain(box.width == 2, sq.bottom == box.top, (sq.center_x == box.left).weak(),
            (sq.center_x == box.right).strong())
shown += [box, sq]
show(*shown)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
