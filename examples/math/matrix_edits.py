"""Matrix edits: insert, remove, swap and reorder rows and columns, set entries, and morph into another matrix.

Structural edits change the matrix in place at the timeline cursor, so each
cell shows its matrix before and after the cut; ``morph_to`` instead
animates the entries into their places in another matrix, which stays. Each
label is the call that edits the matrix below it.
"""

import os

from gaanim import MatrixEntry, Scene, TextStyle, Zones

WRITE = 1.0
HOLD = 1.0
EDIT = WRITE + 2 * HOLD
MORPH = 1.4

scene = Scene()
accent = scene.canvas.color("accent")
DIGITS = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]


def matrix(data, size=0.66, **options):
    """scene.viz.matrix with entries of ``size``, larger than the theme's math size (0.44).

    The delimiters keep their automatic size, which follows the entries and
    the number of rows through every edit.
    """
    return scene.viz.matrix(data, entry_style=TextStyle(size=size), **{"column_gap": 0.36, **options})


# Morphs keep their shape, so their matrices are larger.
LARGE = {"size": 1.1, "row_gap": 0.4, "column_gap": 0.6}


def gallery(labels, data=None, **options):
    """Place a 2 x 2 grid of cells, each label above a matrix of ``data`` (DIGITS by default).

    ``options`` go to matrix(). Returns (matrix, zone) pairs, the zone being
    where the matrix sits.
    """
    grid = scene.layout.zones(Zones.grid(rows=2, columns=2, gap="24px", padding="page_padding"))
    cells = []
    for zone, label, values in zip(grid, labels, data or [DIGITS] * len(labels)):
        top, body = zone.split(Zones.rows(["48px", "1fr"]))
        scene.text(label, role="code").place(top, anchor="bottom")
        cells.append((matrix(values, **options).place(body), body))
    return cells


def edit(rows):
    """Write each cell's matrix, then apply every (label, change) at the same cut.

    ``change(m)`` edits the matrix, which stays in its zone.
    """
    cells = gallery([label for label, _ in rows])
    scene.play([shown.animate.write().duration(WRITE) for shown, _ in cells])
    scene.wait(HOLD)
    for (shown, _), (_, change) in zip(cells, rows):
        change(shown)
    scene.wait(HOLD)


scene.segment("rows")
edit([
    ("m.insert_row(1, [0, 0, 0])", lambda m: m.insert_row(1, [0, 0, 0])),
    ("m.remove_row(0)", lambda m: m.remove_row(0)),
    ("m.swap_rows(0, 2)", lambda m: m.swap_rows(0, 2)),
    ("m.reorder_rows([2, 0, 1])", lambda m: m.reorder_rows([2, 0, 1])),
])

scene.segment("columns")
edit([
    ("m.insert_column(1, [0, 0, 0])", lambda m: m.insert_column(1, [0, 0, 0])),
    ("m.remove_column(0)", lambda m: m.remove_column(0)),
    ("m.swap_columns(0, 2)", lambda m: m.swap_columns(0, 2)),
    ("m.reorder_columns([2, 0, 1])", lambda m: m.reorder_columns([2, 0, 1])),
])

# set() replaces one entry and returns its new drawable; become() replaces
# all the data, even with another shape.
scene.segment("values")
edit([
    ("m.set(1, 1, 0)", lambda m: m.set(1, 1, 0)),
    ("m.set(1, 1, 0).fill(accent)", lambda m: m.set(1, 1, 0).fill(accent)),
    ("m.become([[1, 0], [0, 1]])", lambda m: m.become([[1, 0], [0, 1]])),
    ("m.become([[1, 2, 3, 4]])", lambda m: m.become([[1, 2, 3, 4]])),
])

# morph_to pairs the entries of a with those of b (hidden, in the same
# place) by MatrixEntry key, then by value, then by position, unless match
# picks one rule.
scene.segment("morph")


def keyed(rows):
    return [[MatrixEntry(name, key=name) for name in row] for row in rows]


MORPHS = [
    ("a.morph_to(b)", [[1, 2], [3, 4]], [[3, 1], [2, 5]], "auto"),
    ('a.morph_to(b, match="key")', keyed([["a", "b"], ["c", "d"]]), keyed([["a", "c"], ["b", "d"]]), "key"),
    ('a.morph_to(b, match="value")', [[1, 2], [3, 4]], [[2, 4], [1, 3]], "value"),
    ('a.morph_to(b, match="position")', [[1, 2], [3, 4]], [[2, 4], [1, 3]], "position"),
]
cells = gallery([label for label, *_ in MORPHS], [before for _, before, _, _ in MORPHS], **LARGE)
pairs = []
for (source, zone), (_, _, after, match) in zip(cells, MORPHS):
    target = matrix(after, **LARGE)
    target.place(zone)
    target.hidden()
    pairs.append((source, target, match))
scene.play([source.animate.write().duration(WRITE) for source, _, _ in pairs])
scene.wait(HOLD / 2)
scene.play([anim for source, target, match in pairs for anim in source.morph_to(target, match=match).duration(MORPH)])
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Before and after the cut in each edit segment; then mid-morph and settled.
    scene.snapshots(snapshots, [
        *[EDIT * index + t for index in range(3) for t in (WRITE + HOLD / 2, EDIT - 0.1)],
        3 * EDIT + WRITE + HOLD / 2 + MORPH / 2, 3 * EDIT + WRITE + HOLD / 2 + MORPH + HOLD - 0.1,
    ])
else:
    scene.render()
