"""Animated structure: boxes that gain, lose, reorder and resize their children.

Every structural change accepts ``duration=``: the children slide to their
new places, what enters fades in and what leaves fades out, and the change
advances the cursor like ``scene.play``. Each step shows its code under the
box, then makes the change.
"""

import os

from gaanim import BoxStyle, Scene, Zones

scene = Scene()
L = scene.layout
accent = scene.canvas.color("accent")
header = scene.canvas.color("header")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")
dark = scene.canvas.color("background")
seeks = []

# The boxes sit in the middle of the frame; the code of each step goes in a
# band along the bottom.
caption_zone = L.zones(Zones.rows(["1fr", "120px"], padding="page_padding"))[1]
card_style = BoxStyle(padding=("20px", "40px"), background=header, border=rule, radius="16px", min_width="280px",
                      align="center", justify="center", font_size="80px")
new_style = card_style.but(background=accent, color=dark)


def card(name, new=False):
    return L.box(name, style=new_style if new else card_style)


def steps(box, changes):
    """Fade ``box`` in, then for each (label, change) show the label under it and run the change.

    A label of two lines is two statements that ``change`` runs in order.
    """
    scene.play([box.animate.fade_in().duration(0.5)])
    caption = None
    for index, (label, change) in enumerate(changes):
        line = scene.text(label, role="code").place(caption_zone).hidden()
        fades = [line.animate.fade_in().duration(0.3)]
        if caption is not None:
            fades.append(caption.animate.fade_out().duration(0.3))
        scene.play(fades)
        start = scene.cursor
        change()
        # The first change midway, then every change settled.
        if index == 0:
            seeks.append((start + scene.cursor) / 2)
        scene.wait(0.5)
        seeks.append(scene.cursor - 0.1)
        caption = line


# add() inserts a child (at the end or at an index), remove() takes one out
# and replace() puts another in its place.
scene.segment("children")
board = L.row(card("One"), card("Two"), card("Three"), gap="32px")
steps(board, [
    ('board.add(card("Four", new=True), duration=0.6)', lambda: board.add(card("Four", new=True), duration=0.6)),
    ('board.add(card("Zero", new=True), at=0, duration=0.6)',
     lambda: board.add(card("Zero", new=True), at=0, duration=0.6)),
    ("board.remove(board[2], duration=0.6)", lambda: board.remove(board[2], duration=0.6)),
    ('board.replace(board[0], card("New", new=True), duration=0.6)',
     lambda: board.replace(board[0], card("New", new=True), duration=0.6)),
])

# set() changes any box property and item() any rule of a child; with
# advance=False the next change starts at the same time.
scene.segment("properties")
grid = L.grid(*[card(name) for name in "ABCDEF"], columns=3, gap="24px", padding="16px", background=panel,
              border=rule)
steps(grid, [
    ("grid.set(columns=2, duration=0.8)", lambda: grid.set(columns=2, duration=0.8)),
    ('grid.set(gap="32px", duration=0.8)', lambda: grid.set(gap="32px", duration=0.8)),
    ("grid[0].item(column_span=2, duration=0.8)", lambda: grid[0].item(column_span=2, duration=0.8)),
    ('grid.set(columns=3, gap="24px", duration=0.8, advance=False)\ngrid[0].item(row_span=2, duration=0.8)',
     lambda: (grid.set(columns=3, gap="24px", duration=0.8, advance=False), grid[0].item(row_span=2, duration=0.8))),
])

# move_child(), swap() and reverse() reorder the children, which slide to
# their new places.
scene.segment("order")
row = L.row(*[card(str(number)) for number in range(1, 6)], gap="32px")
steps(row, [
    ("row.swap(row[0], row[4], duration=0.6)", lambda: row.swap(row[0], row[4], duration=0.6)),
    ("row.move_child(row[0], 2, duration=0.6)", lambda: row.move_child(row[0], 2, duration=0.6)),
    ("row.reverse(duration=0.6)", lambda: row.reverse(duration=0.6)),
])

# A child still animates like any object: it starts where the box put it,
# does not push its siblings and keeps its offset when the box reflows.
# detach() lets a child go without hiding it, free to move on its own.
scene.segment("free")
three = card("Three")
row = L.row(card("One"), card("Two"), three, gap="32px")
steps(row, [
    ("scene.play([row[1].animate.shift_by(0, 1.0)])", lambda: scene.play([row[1].animate.shift_by(0, 1.0)])),
    ('row.add(card("Four", new=True), at=0, duration=0.6)',
     lambda: row.add(card("Four", new=True), at=0, duration=0.6)),
    ("row.detach(three, duration=0.6)", lambda: row.detach(three, duration=0.6)),
    ("scene.play([three.animate.shift_by(0, -1.8)])", lambda: scene.play([three.animate.shift_by(0, -1.8)])),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
