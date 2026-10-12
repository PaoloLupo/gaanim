"""Matrix selections: rows, columns, blocks, diagonals and predicates, traversal orders and cell animations.

Every entry of ``scene.viz.matrix`` is a drawable of its own, so a selection
of entries takes a style or an animation, cell after cell in the order you
pick. Each label is the code that differs from cell to cell.
"""

import os

from gaanim import MatrixSelection, Scene, TextStyle, Zones

WRITE = 1.0
CELL = 0.6
HOLD = 0.8
# A fill sweeps its selection in SPREAD seconds; a write staggers the nine
# entries 0.12 s apart; an animation staggers a row 0.2 s apart.
SPREAD = 0.45
FILL = WRITE + SPREAD + CELL + HOLD
ORDER = 8 * 0.12 + CELL + HOLD
ANIMATE = WRITE + 2 * 0.2 + CELL + HOLD
# Entries are set larger than the theme's math size (0.44) so that each
# matrix reads at a glance (larger still when a segment has one row of
# cells); gaps and delimiters grow to match.
SIZE = 0.72

scene = Scene()
accent = scene.canvas.color("accent")
DIGITS = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]


def matrix(size):
    """A 3 x 3 scene.viz.matrix with entries of ``size`` and its gaps and delimiters scaled to them."""
    growth = size / 0.44
    return scene.viz.matrix(
        DIGITS,
        entry_style=TextStyle(size=size),
        row_gap=0.24 * growth,
        column_gap=0.24 * growth,
        delimiter_gap=0.12 * growth,
        delimiter_size=1.8 * growth,
    )


def gallery(labels, rows=2):
    """Place a 3-column grid of cells, each label above a matrix; return the matrices."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=3, gap="24px", padding="page_padding"))
    size = SIZE if rows > 1 else 0.95
    matrices = []
    for zone, label in zip(grid, labels):
        # The label and the matrix (about 4.7 sizes tall) sit together, centered in the cell.
        _, top, body, _ = zone.split(Zones.rows(["1fr", "48px", 4.7 * size, "1fr"]))
        scene.text(label, role="code", markup=False, text_align="center").place(top, anchor="bottom")
        shown = matrix(size)
        # place() returns the matrix's layout box, so keep the matrix itself.
        shown.place(body)
        matrices.append(shown)
    return matrices


def is_even(entry, row, column):
    return entry.value % 2 == 0


def fill_in(selected):
    """Animations that fill a selection cell after cell, or a single entry, with the accent."""
    if isinstance(selected, MatrixSelection):
        stagger = SPREAD / max(len(selected) - 1, 1)
        return list(selected.animate.fill(accent, order="row_major", stagger=stagger).duration(CELL))
    return [selected.animate.fill(accent).duration(CELL)]


def pick(selections):
    """Write a matrix per (label, select) cell, then fill each selection with the accent."""
    matrices = gallery([label for label, _ in selections])
    scene.play([shown.animate.write().duration(WRITE) for shown in matrices])
    scene.play([anim for (_, select), shown in zip(selections, matrices) for anim in fill_in(select(shown))])
    scene.wait(HOLD)


# Selections pick entries by row, column, index, slice, block or predicate;
# here each one fills with the accent, cell after cell. Two indices give the
# entry itself, a drawable.
scene.segment("selections")
pick([
    ("m.row(1)", lambda m: m.row(1)),
    ("m.column(2)", lambda m: m.column(2)),
    ("m[1, 1]", lambda m: m[1, 1]),
    ("m[:2, 1:]", lambda m: m[:2, 1:]),
    ("m.block(1, [0, 2])", lambda m: m.block(1, [0, 2])),
    ("m.where(is_even)", lambda m: m.where(is_even)),
])

# Diagonals, shifted by an offset.
scene.segment("diagonals")
pick([
    ("m.diagonal()", lambda m: m.diagonal()),
    ("m.diagonal(1)", lambda m: m.diagonal(1)),
    ("m.diagonal(-1)", lambda m: m.diagonal(-1)),
    ("m.anti_diagonal()", lambda m: m.anti_diagonal()),
    ("m.anti_diagonal(1)", lambda m: m.anti_diagonal(1)),
    ("m.anti_diagonal(-1)", lambda m: m.anti_diagonal(-1)),
])


def traverse(orders, rows):
    """Write each cell's entries in its order: m.entries.animate.write(order=..., stagger=0.12)."""
    matrices = gallery([label for label, _ in orders], rows)
    scene.play([
        anim
        for (_, options), shown in zip(orders, matrices)
        for anim in shown.entries.animate.write(**options, stagger=0.12).duration(CELL)
    ])
    scene.wait(HOLD)


# order= sets the sequence in which the entries are written...
scene.segment("orders")
traverse(
    [(f'order="{name}"', {"order": name}) for name in (
        "row_major", "column_major", "main_diagonal", "anti_diagonal", "spiral_in", "spiral_out",
    )],
    rows=2,
)

# ...shuffled with a seed, or sorted by a key of (row, column).
scene.segment("custom_orders")
traverse(
    [
        ('order="random"', {"order": "random"}),
        ("order=lambda r, c: -c", {"order": lambda row, column: -column}),
        ('order="random", seed=7', {"order": "random", "seed": 7}),
    ],
    rows=1,
)


def act(actions, rows):
    """Write a matrix per (label, action) cell, then play the action on its middle row."""
    matrices = gallery([label for label, _ in actions], rows)
    scene.play([shown.animate.write().duration(WRITE) for shown in matrices])
    scene.play([anim for (_, action), shown in zip(actions, matrices) for anim in action(shown.row(1)).duration(CELL)])
    scene.wait(HOLD)


# The same calls animate any selection; each label abbreviates
# m.row(1).animate.<call>. Some return to rest...
scene.segment("animations")
act(
    [
        ("indicate()", lambda row: row.animate.indicate()),
        ("wiggle()", lambda row: row.animate.wiggle()),
        ("wiggle(stagger=0.2)", lambda row: row.animate.wiggle(stagger=0.2)),
        ("scale_by(1.4)", lambda row: row.animate.scale_by(1.4)),
        ("rotate_by(0.6)", lambda row: row.animate.rotate_by(0.6)),
        ("opacity(0.25)", lambda row: row.animate.opacity(0.25)),
    ],
    rows=2,
)

# ...and some change the entries for good.
scene.segment("fills_and_fades")
act(
    [
        ("fill(accent)", lambda row: row.animate.fill(accent)),
        ("fade_out()", lambda row: row.animate.fade_out()),
        ("fade_out(stagger=0.2)", lambda row: row.animate.fade_out(stagger=0.2)),
    ],
    rows=1,
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-way and settled, in each segment.
    scene.snapshots(snapshots, [
        WRITE + 0.4, FILL - 0.1,
        FILL + WRITE + 0.4, 2 * FILL - 0.1,
        2 * FILL + 0.7, 2 * FILL + ORDER - 0.1,
        2 * FILL + ORDER + 0.7, 2 * FILL + 2 * ORDER - 0.1,
        2 * FILL + 2 * ORDER + WRITE + 0.3, 2 * FILL + 2 * ORDER + ANIMATE - 0.1,
        2 * FILL + 2 * ORDER + ANIMATE + WRITE + 0.3, 2 * FILL + 2 * ORDER + 2 * ANIMATE - 0.1,
    ])
else:
    scene.render()
