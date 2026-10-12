"""Tables on slides: scene.slides.table and data tables built from layout rows.

scene.slides.table draws a compact table in one call. A table built from a
column of grid rows keeps every row and cell as a box, so columns can be
aligned, and rows, columns and cells highlighted or revealed one at a time.
Each label is the code that builds or changes the table above it.
"""

import os

from gaanim import Scene, Text, Zones, stagger

scene = Scene()
S = scene.slides
L = scene.layout
accent = scene.canvas.color("accent")
chart = scene.canvas.color("chart")
dark = scene.canvas.color("background")
header = scene.canvas.color("header")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")
seeks = []

HEADERS = ["Method", "Residual", "Time"]
ROWS = [["Baseline", "1.8e-1", "48 ms"], ["Cached", "7.2e-2", "32 ms"], ["GPU", "4.0e-2", "15 ms"]]


def code(label):
    return scene.text(label, role="code")




def stacked(variants):
    """Each (label, table) in its own row of the page, the label above the table."""
    shown = []
    for zone, (label, table) in zip(L.zones(Zones.rows(len(variants), gap="48px", padding="page_padding")),
                                    variants):
        top, rest = zone.split(Zones.rows(["64px", "1fr"]))
        shown += [code(label).place(top), table.place(rest)]
    start = scene.cursor
    scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in shown], each=0.1))
    scene.wait(1.0)
    # Mid-entrance and settled.
    seeks.extend([start + 0.4, scene.cursor - 0.1])


# scene.slides.table takes the header and the rows as strings; columns share
# width= equally and every row is row_height= tall.
scene.segment("slide_table")
stacked([
    ("table(HEADERS, ROWS, width=10)", S.table(HEADERS, ROWS, width=10)),
    ("table(HEADERS, ROWS, width=10, row_height=0.8)", S.table(HEADERS, ROWS, width=10, row_height=0.8)),
])

# header_background=, rule_color= and color= replace the theme's header,
# rule and foreground colors.
scene.segment("slide_table_colors")
stacked([
    ("table(HEADERS, ROWS, width=10, header_background=rule)",
     S.table(HEADERS, ROWS, width=10, header_background=rule)),
    ("table(HEADERS, ROWS, width=10, rule_color=accent, color=chart)",
     S.table(HEADERS, ROWS, width=10, rule_color=accent, color=chart)),
])

# A data table is a column of rows, and each row a grid of cell boxes on the
# same column tracks. align= places a cell's content: text to the left,
# numbers to the right, a chip in the middle.
TRACKS = ["3fr", "2fr", "2fr", "2fr"]
ALIGN = ["start", "end", "end", "center"]
DATA = [
    ("Baseline", "0.180", "48 ms", ("Slow", "warning")),
    ("Cached", "0.072", "32 ms", ("Good", "accent")),
    ("GPU", "0.040", "15 ms", ("Best", "success")),
    ("Sparse", "0.410", "9 ms", ("Unstable", "danger")),
]
page = L.zones(Zones.rows(["64px", "1fr", "180px"], gap="16px", padding="page_padding"))


def data_table():
    """A data table as wide as the page, and its rows: the header first."""
    def row(values, background=None, weight=None):
        cells = [L.box(value, align=align, justify="center", padding=("0px", "24px"), font_size="52px",
                       weight=weight) for value, align in zip(values, ALIGN)]
        return L.grid(*cells, columns=TRACKS, width="fill", height="128px", radius="8px", background=background)

    rows = [row(["Method", "Residual", "Time", "Status"], background=header, weight=700)]
    for index, (name, residual, time, (status, variant)) in enumerate(DATA):
        chip = S.chip(status, variant=variant, font_size=0.4)
        rows.append(row([name, residual, time, chip], background=panel if index % 2 else None))
    table = L.column(*rows, width=page[1].width).place(page[1])
    return table, rows


scene.segment("alignment")
table, rows = data_table()
labels = page[0].split(Zones.columns(TRACKS))
SIDES = {"start": "left", "center": "center", "end": "right"}
shown = [code(f'align="{align}"').place(zone.inset((0, "24px")), anchor=SIDES[align])
         for zone, align in zip(labels, ALIGN)]
start = scene.cursor
scene.play(stagger(table.animate.fade_in().duration(0.5),
                   *[label.animate.fade_in().duration(0.4) for label in shown], each=0.1))
scene.wait(1.0)
seeks.extend([start + 0.4, scene.cursor - 0.1])


def steps(changes):
    """For each (label, change): show the label under the table, then make the change."""
    caption = None
    for label, change in changes:
        line = code(label).place(page[2]).hidden()
        fades = [line.animate.fade_in().duration(0.3)]
        if caption is not None:
            fades.append(caption.animate.fade_out().duration(0.3))
        scene.play(fades)
        start = scene.cursor
        change()
        # Midway and settled.
        seeks.extend([(start + scene.cursor) / 2, scene.cursor + 0.4])
        scene.wait(0.5)
        caption = line


# set() restyles a row or a cell; with duration= the change animates. The
# texts of a box are found with find_all() and animated like any text.
scene.segment("highlight_rows")
table, rows = data_table()
scene.play([table.animate.fade_in().duration(0.4)])


def highlight_row():
    rows[3].set(background=accent, duration=0.6, advance=False)
    texts = rows[3].find_all(type=Text)
    scene.play([t.animate.fill(dark) for t in texts])


steps([
    ("rows[3].set(background=accent, duration=0.6, advance=False)\n"
     "texts = rows[3].find_all(type=Text)\n"
     "scene.play([t.animate.fill(dark) for t in texts])", highlight_row),
    ('rows[4][1].set(border=accent, border_width="4px", duration=0.6)',
     lambda: rows[4][1].set(border=accent, border_width="4px", duration=0.6)),
])


# Each set() with duration= advances the cursor, so a loop of them runs one
# change after another, down a column.
def highlight_column():
    for row in rows:
        row[2].set(background=rule, duration=0.25)


scene.segment("highlight_column")
table, rows = data_table()
scene.play([table.animate.fade_in().duration(0.4)])
steps([("for row in rows: row[2].set(background=rule, duration=0.25)", highlight_column)])

# reveal() shows a box piece by piece: backgrounds fade in and texts rise.
scene.segment("reveal")
table, rows = data_table()
steps([("scene.play(table.reveal())", lambda: scene.play(table.reveal()))])


# Revealing one row after each stop builds the table a step at a time while
# presenting; elsewhere the stops do not pause.
def row_by_row():
    for index, row in enumerate(rows):
        scene.play(row.reveal())
        scene.stop(f"row {index}")


scene.segment("row_by_row")
table, rows = data_table()
steps([("for row in rows: scene.play(row.reveal()); scene.stop()", row_by_row)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
