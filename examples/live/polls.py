"""Polls: a question for the audience's phones and the live data a scene draws from it.

``scene.poll`` draws nothing. It gives a QR code and a session code to join,
bars that follow each answer's votes, live counts, and the shape and color
each answer has on the phones. Outside a live presentation the rehearsal,
a made-up audience, votes instead, so previews and snapshots show the poll
being answered. Each label is the code that builds what it shows.
"""

import os

from gaanim import Scene, Zones

QUESTION = "Which grows fastest?"
ANSWERS = ["x²", "2ˣ", "x·log x"]
VOTING = 2.4

scene = Scene()
foreground = scene.colors.foreground
background = scene.colors.background
accent = scene.colors.accent
track = scene.colors.header
# Previews, exports and snapshots play this made-up class of 24.
scene.rehearsal(24, seed=3)
seeks = []


def code(label, zone, anchor="left"):
    """A variant label: the code that builds it."""
    return scene.text(label, role="code").place(zone, anchor=anchor)


def vote(seconds=VOTING, early=True):
    """Let the rehearsal vote, then pause where a presentation waits; note the seeks."""
    start = scene.cursor
    scene.wait(seconds)
    seeks.extend(([start + 0.4] if early else []) + [scene.cursor - 0.05])


def bar(poll, index, zone, thickness, **options):
    """``poll``'s bar for ``index`` filling ``zone`` along its direction, over a track."""
    vertical = options.get("direction") in ("up", "down")
    length = zone.height if vertical else zone.width
    width, height = (thickness, length) if vertical else (length, thickness)
    radius = options.get("radius", 0)
    scene.geometry.rounded_rect(width, height, radius).fill(track).no_stroke().place(zone)
    return poll.bar(index, length=length, thickness=thickness, **options).fill(accent).no_stroke().place(zone)


# The QR code opens the relay's voting page; the session code is for phones
# that cannot scan. Both are ordinary drawables: a QR reads best dark on a
# light card with a margin. Presenting, the bars and counts follow the votes.
scene.segment("poll")
poll = scene.poll(QUESTION, ANSWERS, rehearse=[3, 7, 2])
page = scene.layout.zones(Zones.columns(["5fr", "11fr"], gap="96px", padding="page_padding"))
join = page[0].split(Zones.rows(["5fr", "1fr", "1fr", "1fr"], gap="8px"))
card = scene.geometry.rounded_rect(4.0, 4.0, 0.2).fill(foreground).no_stroke().place(join[0])
poll.qr(3.4).fill(background).place(card)
scene.text(poll.code, size=0.6, weight=700).place(join[1])
code("poll.qr(3.4)", join[2], anchor="center")
code("poll.code", join[3], anchor="center")
board = page[1].split(Zones.grid(rows=5, columns=["1fr", "4fr", "7fr", "2fr"], column_gap="48px"))
scene.text(poll.question, size=0.7, weight=700).place(board[0], anchor="left")
grow = []
for index, answer in enumerate(poll.options):
    first = 4 + 4 * index
    poll.icon(index, 0.7).place(board[first])
    scene.text(answer, size=0.7).place(board[first + 1], anchor="left")
    grow.append(bar(poll, index, board[first + 2], 0.6, radius=0.3).animate.create())
    scene.viz.readout(poll.votes(index), format=".0f", font_size=0.7).place(board[first + 3], anchor="left")
scene.viz.readout(poll.total(), format=".0f", suffix=" votes", font_size=0.5).place(board[18], anchor="left")
# A bar animates like any drawable: create() draws it in.
scene.play(grow)
vote()
# Presenting, this stop advances by itself once 90% of the phones voted;
# previews and exports treat it as a plain stop.
scene.stop("voting", until=poll.answered(share=0.9))

# A bar's bounds are its full box, so the layout holds still as votes arrive.
# By default the leading answer fills its bar; scale="total" makes each bar
# its share of the votes.
scene.segment("bars")
poll = scene.poll(QUESTION, ANSWERS, rehearse=[3, 7, 2])
grid = scene.layout.zones(Zones.grid(rows=4, columns=["6fr", "7fr"], column_gap="48px", row_gap="40px",
                                     padding="page_padding"))
VARIANTS = [
    ("poll.bar(i)", {}),
    ('poll.bar(i, scale="total")', {"scale": "total"}),
    ("poll.bar(i, radius=0.2)", {"radius": 0.2}),
    ('poll.bar(i, direction="left")', {"direction": "left"}),
]
for row, (label, options) in enumerate(VARIANTS):
    code(label, grid[2 * row], anchor="right")
    lanes = grid[2 * row + 1].split(Zones.rows(len(ANSWERS), gap="10px"))
    for index, lane in enumerate(lanes):
        bar(poll, index, lane, lane.height, **options)
vote()

# direction="up" and "down" grow columns; scale and radius combine with any
# direction.
scene.segment("columns")
poll = scene.poll(QUESTION, ANSWERS, rehearse=[3, 7, 2])
grid = scene.layout.zones(Zones.columns(2, gap="128px", padding="page_padding"))
for zone, direction in zip(grid, ["up", "down"]):
    cell = zone.split(Zones.rows(["1fr", "8fr", "1fr"], gap="24px"))
    code(f'poll.bar(i, direction="{direction}")', cell[0], anchor="center")
    columns = cell[1].split(Zones.columns(len(ANSWERS), gap="48px"))
    icons = cell[2].split(Zones.columns(len(ANSWERS), gap="48px"))
    for index, (column, icon) in enumerate(zip(columns, icons)):
        bar(poll, index, column, column.width, direction=direction)
        poll.icon(index, 0.6).place(icon)
vote()

# Live counts are parameters: show them with readouts, or feed them to
# computed values and anything else that takes a parameter.
scene.segment("numbers")
poll = scene.poll(QUESTION, ANSWERS, rehearse=[3, 7, 2])
grid = scene.layout.zones(Zones.grid(rows=5, columns=["4fr", "3fr", "3fr", "3fr"], column_gap="32px",
                                     padding="page_padding"))
for index, answer in enumerate(poll.options):
    header = grid[1 + index].split(Zones.columns(["2fr", "3fr"], gap="16px"))
    poll.icon(index, 0.6).place(header[0], anchor="right")
    scene.text(answer, size=0.6).place(header[1], anchor="left")
NUMBERS = [
    ("poll.votes(i)", poll.votes, {"format": ".0f"}),
    ("poll.share(i)", poll.share, {"format": ".2f"}),
    ("poll.percent(i)", poll.percent, {"format": ".0f", "suffix": "%"}),
]
for row, (label, number, options) in enumerate(NUMBERS, start=1):
    code(label, grid[4 * row], anchor="right")
    for index in range(len(ANSWERS)):
        scene.viz.readout(number(index), font_size=0.8, **options).place(grid[4 * row + 1 + index])
code("poll.total()", grid[16], anchor="right")
scene.viz.readout(poll.total(), format=".0f", font_size=0.8).place(grid[17])
vote()

# Each answer has a shape and a color on the phones, for the screen to match.
# A poll takes 2 to 6 answers.
scene.segment("shapes")
DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
poll = scene.poll("Which day suits the review?", DAYS)
grid = scene.layout.zones(Zones.grid(rows=2, columns=["5fr"] + ["2fr"] * len(DAYS), column_gap="32px",
                                     row_gap="96px", padding="page_padding"))
code("poll.icon(i, 1.4)", grid[0], anchor="right")
code("fill(poll.color(i))", grid[7], anchor="right")
for index, answer in enumerate(poll.options):
    poll.icon(index, 1.4).place(grid[1 + index])
    side = grid[8 + index].width
    tile = scene.geometry.rounded_rect(side, side, 0.2).fill(poll.color(index)).no_stroke().place(grid[8 + index])
    scene.text(answer, size=0.5, weight=700).fill(background).place(tile)
vote(early=False)

# The rehearsal leans each poll by one weight per answer, or some random way
# without rehearse. With multiple=True a phone picks several answers: votes
# count every choice and total() counts phones. Each poll here opens in turn
# and close() ends its voting, so the rehearsal answers it before the next.
scene.segment("rehearsal")
ROWS = [
    ("rehearse=None", {}),
    ("rehearse=[1, 1, 1]", {"rehearse": [1, 1, 1]}),
    ("rehearse=[6, 2, 1]", {"rehearse": [6, 2, 1]}),
    ("rehearse=[0, 1, 4]", {"rehearse": [0, 1, 4]}),
    ("multiple=True", {"multiple": True}),
]
grid = scene.layout.zones(Zones.grid(rows=len(ROWS) + 1, columns=["5fr", "4fr", "4fr", "4fr", "2fr"],
                                     column_gap="40px", padding="page_padding"))
for index, answer in enumerate(ANSWERS):
    scene.text(answer, size=0.6).place(grid[1 + index])
code("total()", grid[4])
start = scene.cursor
for row, (label, options) in enumerate(ROWS, start=1):
    poll = scene.poll(QUESTION, ANSWERS, **options)
    first = 5 * row
    code(label, grid[first], anchor="right")
    for index in range(len(ANSWERS)):
        cell = grid[first + 1 + index].split(Zones.columns(["2fr", "5fr"], gap="16px"))
        scene.viz.readout(poll.votes(index), format=".0f", font_size=0.6).place(cell[0], anchor="right")
        bar(poll, index, cell[1], 0.5, scale="total")
    scene.viz.readout(poll.total(), format=".0f", font_size=0.6).place(grid[first + 4])
    scene.wait(1.0)
    poll.close()
scene.wait(0.6)
seeks.extend([start + 1.5, start + 3.5, scene.cursor - 0.05])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
