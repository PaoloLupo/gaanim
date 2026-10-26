"""Matrices: delimiters, row and column labels, spacing, cell content and cell styles.

``scene.viz.matrix`` lays out one drawable per entry between two
delimiters, with optional labels beside the rows and above the columns. Each
cell shows one option, labelled with the code that differs from cell to cell.
"""

import os

from gaanim import MatrixEntry, Scene, TextStyle, Zones

WRITE = 1.0
HOLD = 0.8
SHOW = WRITE + HOLD
SEGMENTS = 5
# Entries are set larger than the theme's math size (0.44) so that each
# matrix reads at a glance; gaps and labels grow to match, and the delimiters
# follow the entries.
SIZE = 0.8
GROWTH = SIZE / 0.44

scene = Scene()
accent = scene.colors.accent
ACCENT = TextStyle(size=SIZE, color=accent)
DIGITS = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
SQUARE = [[1, 2], [3, 4]]


def matrix(data, **options):
    """scene.viz.matrix with entries of SIZE and its gaps scaled to them."""
    sized = {
        "entry_style": TextStyle(size=SIZE),
        "label_style": TextStyle(size=0.8 * SIZE),
        "row_gap": 0.24 * GROWTH,
        "column_gap": 0.24 * GROWTH,
        "delimiter_gap": 0.12 * GROWTH,
    }
    return scene.viz.matrix(data, **{**sized, **options})


def show(cells, columns, rows, label_height="48px"):
    """Place (label, matrix) cells in a grid, each label above its matrix, and write them."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="24px", padding="page_padding"))
    for zone, (label, shown) in zip(grid, cells):
        top, body = zone.split(Zones.rows([label_height, "1fr"]))
        scene.text(label, role="code", text_align="center").place(top, anchor="bottom")
        shown.place(body)
    scene.play([shown.animate.write().duration(WRITE) for _, shown in cells])
    scene.wait(HOLD)


# Delimiters around the entries.
scene.segment("delimiters")
show(
    [
        (f'delimiters="{name}"', matrix(DIGITS, delimiters=name))
        for name in ("brackets", "parentheses", "braces", "bars", "double_bars", "none")
    ],
    columns=3,
    rows=2,
)

# Labels beside the rows and above the columns, as math or as plain text.
scene.segment("labels")
show(
    [
        ('row_labels=["x", "y"]', matrix(SQUARE, row_labels=["x", "y"])),
        ('column_labels=["u", "v"]', matrix(SQUARE, column_labels=["u", "v"])),
        (
            'row_labels=["x", "y"],\ncolumn_labels=["u", "v"]',
            matrix(SQUARE, row_labels=["x", "y"], column_labels=["u", "v"]),
        ),
        (
            'column_labels=["in", "out"],\nlabel_mode="text"',
            matrix([[3, 9], [7, 2]], column_labels=["in", "out"], label_mode="text"),
        ),
    ],
    columns=2,
    rows=2,
    label_height="96px",
)

# The gaps between entries and delimiters, and the delimiters' size and weight.
scene.segment("spacing")
show(
    [
        ("row_gap=1.0, column_gap=1.0", matrix(SQUARE, row_gap=1.0, column_gap=1.0)),
        ("delimiter_gap=0.6", matrix(SQUARE, delimiter_gap=0.6)),
        ("delimiter_size=3.0", matrix(SQUARE, delimiter_size=3.0)),
        ("delimiter_weight=700", matrix(SQUARE, delimiter_weight=700)),
    ],
    columns=2,
    rows=2,
)


def dot(value, row, column):
    """A cell_factory: a dot whose area follows the entry."""
    return scene.geometry.dot(0.1 * value**0.5)


# Entries are Typst math by default; text mode, a number format or a factory
# change what each cell shows. matrix() is scene.viz.matrix with larger entries.
scene.segment("entries")
ROTATION = [["cos theta", "-sin theta"], ["sin theta", "cos theta"]]
show(
    [
        ("matrix(ROTATION)", matrix(ROTATION)),
        ('cell_mode="text"', matrix([["red", "green"], ["blue", "gray"]], cell_mode="text")),
        ('numeric_format=".2f"', matrix([[3.14159, 2.71828], [1.41421, 1.73205]], numeric_format=".2f")),
        ("cell_factory=dot", matrix(DIGITS, cell_factory=dot)),
    ],
    columns=2,
    rows=2,
)

# Styles for all the entries, for one entry or for the labels.
scene.segment("styles")
show(
    [
        ("entry_style=ACCENT", matrix(SQUARE, entry_style=ACCENT)),
        ("MatrixEntry(4, style=ACCENT)", matrix([[1, 2], [3, MatrixEntry(4, style=ACCENT)]])),
        ('row_labels=["x", "y"],\nlabel_style=ACCENT', matrix(SQUARE, row_labels=["x", "y"], label_style=ACCENT)),
        ("entry_style=TextStyle(size=0.4)", matrix(SQUARE, entry_style=TextStyle(size=0.4))),
    ],
    columns=2,
    rows=2,
    label_height="96px",
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-write and settled, in each segment.
    scene.snapshots(snapshots, [SHOW * index + t for index in range(SEGMENTS) for t in (WRITE / 2, SHOW - 0.1)])
else:
    scene.render()
