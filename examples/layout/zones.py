"""Zones: templates that divide a region into rectangles to place free objects.

A ``Zones`` template cuts a region into rows, columns or a grid, with gaps,
padding and names. Applied to the frame, a zone or an object's box it gives
a ``ZoneSet``; objects placed in its zones stay free, so they can move to
other zones later. Each label is the code that builds what it describes.
"""

import os

from gaanim import Scene, Zones, stagger

scene = Scene()
L = scene.layout
accent = scene.canvas.color("accent")
header = scene.canvas.color("header")
rule = scene.canvas.color("rule")
seeks = []


def code(label):
    return scene.text(label, role="code", markup=False, text_align="left")


def outline(zone):
    """A rule-colored frame drawn around ``zone``, to make it visible."""
    return scene.geometry.rect(zone.width, zone.height).no_fill().stroke(rule, 0.02).place(zone)


def tile(zone):
    """A filled rectangle that covers ``zone`` exactly."""
    return scene.geometry.rect(zone.width, zone.height).fill(header).stroke(rule, 0.02).place(zone)


def show(*objects):
    """Fade the objects in one after another, hold, and note the seeks."""
    start = scene.cursor
    scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in objects], each=0.03))
    scene.wait(1.0)
    # Mid-entrance and settled.
    seeks.extend([start + 0.3, scene.cursor - 0.1])


def rows(count, split=("1fr", "1fr")):
    """The frame cut into ``count`` rows of (label zone, example zone)."""
    grid = L.zones(Zones.grid(rows=count, columns=list(split), column_gap="48px", padding="page_padding"))
    return [(grid[2 * index], grid[2 * index + 1].inset(("8px", 0))) for index in range(count)]


# Templates cut a region into rows, columns or a grid of tracks (lengths,
# percentages and "fr" shares), with gaps, padding and names. L.zones()
# applies one to the safe area, or within="frame", a zone or an object's
# box; each preview here is L.zones(template, within=zone).
def previews(templates):
    """One row per (label, template): the code on the left, its zones on the right."""
    shown = []
    for (label_zone, region), (label, template) in zip(rows(len(templates)), templates):
        region = region.inset(("16px", 0))
        shown += [code(label).place(label_zone, anchor="right"), outline(region)]
        zones = L.zones(template, within=region)
        shown += [tile(zone) for zone in zones]
        # A named template's zones are found by name too: zones["main"].
        shown += [code(name).place(zones[name]) for name in zones.names]
    show(*shown)


scene.segment("templates")
previews([
    ("Zones.rows(3)", Zones.rows(3)),
    ('Zones.columns(\n    ["1fr", "2fr", "1fr"])', Zones.columns(["1fr", "2fr", "1fr"])),
    ('Zones.columns(4, gap="24px")', Zones.columns(4, gap="24px")),
    ('Zones.columns(4, padding="16px")', Zones.columns(4, padding="16px")),
])

scene.segment("grid_templates")
previews([
    ("Zones.grid(rows=2, columns=6)", Zones.grid(rows=2, columns=6)),
    ('Zones.grid(rows=2, columns=6,\n           row_gap="16px")', Zones.grid(rows=2, columns=6, row_gap="16px")),
    ('Zones.columns(2,\n    names=["side", "main"])', Zones.columns(2, names=["side", "main"])),
])

# A ZoneSet is indexed by name or position, and every zone splits again
# with another template or shrinks with inset(). Each zone below is filled
# and labelled with the code that finds it.
scene.segment("nesting")
page = L.zones(Zones.rows(["120px", "1fr", "120px"], names=["header", "body", "footer"], gap="16px",
                          padding="page_padding"))
body = page["body"].split(Zones.columns(["1fr", "3fr"], names=["side", "main"], gap="16px"))
cells = body["main"].split(Zones.grid(rows=2, columns=2, gap="16px"))
labelled = [(page["header"], 'page["header"]'), (page["footer"], 'page["footer"]'),
            (body["side"], 'body["side"]'), *[(cell, f"cells[{index}]") for index, cell in enumerate(list(cells)[:3])]]
shown = []
for zone, label in labelled:
    shown += [tile(zone), code(label).place(zone)]
inner = cells[3].inset("32px")
shown += [outline(cells[3]), tile(inner), code('cells[3].inset("32px")').place(inner)]
show(*shown)

# place() puts an object in a zone, or on another object's box, by its
# anchor; padding insets the target and offset moves the object.
def placed(variants):
    """One row per (label, place): the code on the left; ``place(zone)`` draws in the outlined zone."""
    shown = []
    for (label_zone, zone), (label, place) in zip(rows(len(variants), split=("2fr", "1fr")), variants):
        zone = zone.inset(("16px", 0))
        shown += [code(label).place(label_zone, anchor="right"), outline(zone), *place(zone)]
    show(*shown)


def disc():
    return scene.geometry.circle(0.4)


scene.segment("place")
placed([
    ("place(zone)", lambda zone: [disc().place(zone)]),
    ('place(zone, anchor="top_left")', lambda zone: [disc().place(zone, anchor="top_left")]),
    ('place(zone, anchor="right", padding="24px")', lambda zone: [disc().place(zone, anchor="right", padding="24px")]),
    ('place(zone, anchor="left", offset=("160px", 0))',
     lambda zone: [disc().place(zone, anchor="left", offset=("160px", 0))]),
])


def on_card(zone):
    card = scene.geometry.rounded_rect(zone.width * 0.6, zone.height * 0.6, 0.12).fill(header).stroke(rule, 0.02)
    card.place(zone)
    return [card, scene.geometry.circle(0.25).place(card, anchor="top_right")]


# fit scales the object to the target; point() gives a zone's points in
# scene units.
scene.segment("place_fit")
placed([
    ('place(zone, fit="contain")', lambda zone: [disc().place(zone, fit="contain")]),
    ('place(zone, fit="stretch")', lambda zone: [disc().place(zone, fit="stretch")]),
    ('move_to(*zone.point("bottom_right"))', lambda zone: [disc().move_to(*zone.point("bottom_right"))]),
    ('place(card, anchor="top_right")', on_card),
])


# arrange() lines free objects up in a zone once, without owning them;
# fill() puts one object in each zone of a set.
def arranged(variants):
    """One row per (label, options): four dots arranged in the outlined zone with ``options``."""
    shown = []
    for (label_zone, zone), (label, options) in zip(rows(len(variants), split=("5fr", "2fr")), variants):
        zone = zone.inset(("24px", 0))
        if options is None:
            items = [scene.geometry.square(0.6) for _ in range(4)]
            quarters = zone.split(Zones.columns(4))
            quarters.fill(*items)
            frames = [outline(quarter) for quarter in quarters]
        else:
            items = [scene.geometry.circle(0.2) for _ in range(4)]
            zone.arrange(*items, **options)
            frames = [outline(zone)]
        shown += [code(label).place(label_zone, anchor="right"), *frames, *items]
    show(*shown)


scene.segment("arrange")
arranged([
    ("zone.arrange(*dots)", {}),
    ('zone.arrange(*dots, gap="48px")', {"gap": "48px"}),
    ('zone.arrange(*dots, justify="between")', {"justify": "between"}),
])

scene.segment("arrange_more")
arranged([
    ('zone.arrange(*dots, justify="end", align="end")', {"justify": "end", "align": "end"}),
    ('zone.arrange(*dots, direction="column")', {"direction": "column"}),
    ("zone.split(Zones.columns(4)).fill(*squares)", None),
])

# Placed objects stay free: animate.place() moves one to another zone, with
# the same anchor, padding and offset options; fit="contain" scales it.
scene.segment("moves")
page = L.zones(Zones.rows(["1fr", "96px"], names=["stage", "caption"], gap="24px", padding="page_padding"))
grid = page["stage"].split(Zones.grid(rows=2, columns=3, gap="24px"))
frames = [outline(zone) for zone in grid]
names = [code(f"grid[{index}]").place(zone, anchor="top_left", padding="12px") for index, zone in enumerate(grid)]
ball = scene.geometry.circle(0.5).place(grid[3])
scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in [*frames, *names, ball]], each=0.03))
MOVES = [
    ('place(grid[0], anchor="top_right")', grid[0], {"anchor": "top_right"}),
    ('place(grid[1], fit="contain", padding="32px")', grid[1], {"fit": "contain", "padding": "32px"}),
    ('place(grid[2], anchor="bottom_left", padding="32px")', grid[2], {"anchor": "bottom_left", "padding": "32px"}),
    ('place(grid[5], fit="contain", padding="96px")', grid[5], {"fit": "contain", "padding": "96px"}),
    ("place(grid[4])", grid[4], {}),
]
caption = None
for label, zone, options in MOVES:
    # Each move's code replaces the previous one under the grid.
    line = code(f"ball.animate.{label}").place(page["caption"]).hidden()
    moves = [ball.animate.place(zone, **options).duration(0.8), line.animate.fade_in().duration(0.3)]
    if caption is not None:
        moves.append(caption.animate.fade_out().duration(0.3))
    scene.play(moves)
    scene.wait(0.3)
    # Every move settled, and the first one midway.
    seeks.extend([scene.cursor - 0.1] if caption else [scene.cursor - 0.7, scene.cursor - 0.1])
    caption = line
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
