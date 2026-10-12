"""Grids: track sizes, gaps, spans, placement, flow and alignment in cells.

``L.grid`` places its children in cells. ``columns`` and ``rows`` take a
count or a list of tracks (lengths, percentages, ``"auto"`` and ``"Nfr"``
shares of the free space); each child fills the next cell, or the cells it
asks for with ``item()``. Every grid below is the same box with one rule
changed, and the label is that rule.
"""

import os

from gaanim import BoxStyle, Scene, Zones, stagger

scene = Scene()
L = scene.layout
accent = scene.canvas.color("accent")
header = scene.canvas.color("header")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")
dark = scene.canvas.color("background")
seeks = []

cell = BoxStyle(padding=("4px", "16px"), background=header, border=rule, align="center", justify="center",
                font_size="56px")
hot = cell.but(background=accent, color=dark)


def cells(*names, style=cell):
    """One cell per name."""
    return [L.box(name, style=style) for name in names]


def numbered(count, start=1, style=cell):
    """``count`` cells numbered from ``start``, so the fill order shows."""
    return cells(*[str(number) for number in range(start, start + count)], style=style)


def columns(count, split):
    """The frame cut into ``count`` rows of (label zone, example zone)."""
    grid = L.zones(Zones.grid(rows=count, columns=list(split), column_gap="48px", padding="page_padding"))
    return [(grid[2 * index], grid[2 * index + 1]) for index in range(count)]


def board_for(split):
    """The grid style, as wide as the example column of a table with ``split``."""
    width = columns(1, split)[0][1].width
    return BoxStyle(width=width, padding="8px", gap="8px", background=panel, border=rule)


def table(variants, split):
    """One row per (label, box): the code on the left, the box it builds on the right."""
    entrances = []
    for (label_zone, zone), (label, box) in zip(columns(len(variants), split), variants):
        code = scene.text(label, role="code").place(label_zone, anchor="right")
        box.place(zone, anchor="left")
        entrances += [code.animate.fade_in().duration(0.4), box.animate.fade_in().duration(0.4)]
    start = scene.cursor
    scene.play(stagger(*entrances, each=0.04))
    scene.wait(1.0)
    # Mid-entrance and settled.
    seeks.extend([start + 0.3, scene.cursor - 0.1])


# Tracks: a count of equal columns, fixed lengths, shares of the free space
# ("1fr", "2fr"), percentages of the grid, "auto" (the content) or numbers
# in scene units. Each cell shows its own track.
scene.segment("tracks")
SPLIT = ("1fr", "1fr")
board = board_for(SPLIT).but(height="144px")
table([
    ("columns=3", L.grid(*cells("1fr", "1fr", "1fr"), style=board, columns=3)),
    ('columns=["240px", "1fr", "2fr"]', L.grid(*cells("240px", "1fr", "2fr"), style=board,
                                               columns=["240px", "1fr", "2fr"])),
    ('columns=["25%", "1fr", "25%"]', L.grid(*cells("25%", "1fr", "25%"), style=board,
                                             columns=["25%", "1fr", "25%"])),
    ('columns=["auto", "1fr", "auto"]', L.grid(*cells("auto", "1fr", "wider auto"), style=board,
                                               columns=["auto", "1fr", "auto"])),
    ('columns=[2.5, "1fr", "1fr"]', L.grid(*cells("2.5", "1fr", "1fr"), style=board, columns=[2.5, "1fr", "1fr"])),
], SPLIT)

# Rows take the same tracks; gap separates both directions, or row_gap and
# column_gap one each.
scene.segment("rows_and_gaps")
board = board_for(SPLIT)
table([
    ('rows=["80px", "128px"]', L.grid(*numbered(6), style=board, columns=3, rows=["80px", "128px"])),
    ('gap="2px"', L.grid(*numbered(6), style=board, columns=3, gap="2px")),
    ('gap="32px"', L.grid(*numbered(6), style=board, columns=3, gap="32px")),
    ('row_gap="32px", column_gap="2px"', L.grid(*numbered(6), style=board, columns=3, row_gap="32px",
                                                column_gap="2px")),
], SPLIT)

# A child can span several tracks or ask for a cell (counted from 0); the
# rest flow around it, by rows or, with auto_flow="column", by columns.
scene.segment("placement")
table([
    ("item(column_span=2)", L.grid(L.box("1", style=hot).item(column_span=2), *numbered(4, start=2),
                                   style=board, columns=3)),
    ("item(row_span=2)", L.grid(L.box("1", style=hot).item(row_span=2), *numbered(4, start=2),
                                style=board, columns=3)),
    ("item(row=1, column=2)", L.grid(L.box("1", style=hot).item(row=1, column=2), *numbered(4, start=2),
                                     style=board, columns=3)),
    ('auto_flow="column"', L.grid(*numbered(6), style=board, columns=3, rows=2, auto_flow="column")),
], SPLIT)

# Cells stretch their children by default. align places them vertically in
# their cells, and a child's own align_self and anchor override it.
scene.segment("cells")
SPLIT = ("9fr", "7fr")
board = board_for(SPLIT)
wide, wide_hot = cell.but(min_width="144px"), hot.but(min_width="144px")
table([
    (f'align="{align}"', L.grid(*numbered(4, style=wide), style=board, columns=4, rows=["160px"], align=align))
    for align in ("stretch", "start", "center", "end")
] + [
    ('item(align_self="end", anchor="right")',
     L.grid(*numbered(3, style=wide), L.box("4", style=wide_hot).item(align_self="end", anchor="right"),
            style=board, columns=4, rows=["160px"])),
], SPLIT)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
