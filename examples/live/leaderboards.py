"""Leaderboards: the players of a game ranked by points, and what each one answered.

``scene.leaderboard()`` draws nothing: for each rank, 0 for the leader, it
gives the nickname as live text, the points and other counts as parameters,
and a bar against the leader's score. It also tells what the player at a
rank did on one question. Outside a live presentation the rehearsal plays
the quizzes, so the standings change as each answer is revealed. Each label
is the code that builds what it shows.
"""

import os

from gaanim import Scene, Zones

scene = Scene()
foreground = scene.colors.foreground
accent = scene.colors.accent
muted = scene.colors.muted
rule = scene.colors.rule
track = scene.colors.header
scene.rehearsal(12, seed=4)
board = scene.leaderboard()
RANKS = 6
COLUMNS = 4
seeks = []


def code(label, zone, anchor="left"):
    """A variant label: the code that builds it."""
    return scene.text(label, role="code").place(zone, anchor=anchor)


def number(parameter, zone, **options):
    """A readout of ``parameter`` centered in ``zone``."""
    return scene.viz.readout(parameter, format=".0f", font_size=0.55, **options).place(zone)


# Three quizzes, opened and revealed in turn while the standings show. The
# leaderboard counts a quiz's points when it is revealed.
scene.segment("standings")
QUIZZES = [
    ("7 × 8?", ["54", "56", "64"], 1),
    ("Capital of Australia?", ["Sydney", "Canberra", "Perth"], 1),
    ("Largest planet?", ["Saturn", "Earth", "Jupiter"], 2),
]
page = scene.layout.zones(Zones.rows(["1fr", "6fr"], gap="32px", padding="page_padding"))
top = page[0].split(Zones.columns(["9fr", "3fr", "1fr"], gap="48px"))
code("board.players()", top[1], anchor="right")
scene.viz.readout(board.players(), format=".0f", font_size=0.6).place(top[2], anchor="left")
grid = page[1].split(Zones.grid(rows=RANKS + 1, columns=["1fr", "4fr", "8fr", "4fr"], column_gap="32px"))
for column, label in enumerate(["board.name(rank)", "board.bar(rank)", "board.points(rank)"], start=1):
    code(label, grid[column], anchor="left" if column == 1 else "center")
for rank in range(RANKS):
    first = 4 * (rank + 1)
    scene.text(str(rank + 1), size=0.55).fill(muted).place(grid[first])
    board.name(rank, size=0.55).place(grid[first + 1], anchor="left")
    scene.geometry.rect(grid[first + 2].width, 0.4).fill(track).no_stroke().place(grid[first + 2])
    board.bar(rank, length=grid[first + 2].width, thickness=0.4).fill(accent).no_stroke().place(grid[first + 2])
    number(board.points(rank), grid[first + 3])
captions = []
for question, options, correct in QUIZZES:
    quiz = scene.quiz(question, options, correct=correct)
    caption = scene.text(question, size=0.6, weight=700).place(top[0], anchor="left").hidden()
    scene.play([caption.animate.fade_in().duration(0.3)] + [old.animate.fade_out().duration(0.3) for old in captions])
    captions = [caption]
    scene.wait(0.9)
    scene.stop()
    quiz.reveal()
    scene.wait(1.0)
    seeks.append(scene.cursor - 0.05)
last_quiz = quiz

# A nickname is live text placed by its left edge, center or right edge, so
# names of any length line up; size, weight and fill style it.
scene.segment("names")
VARIANTS = [
    ('board.name(0, align="left")', {"align": "left"}),
    ('board.name(0, align="center")', {"align": "center"}),
    ('board.name(0, align="right")', {"align": "right"}),
    ("board.name(1, size=0.9)", {"size": 0.9, "rank": 1}),
    ("board.name(2, weight=800)", {"weight": 800, "rank": 2}),
    ("board.name(3).fill(accent)", {"rank": 3, "fill": accent}),
]
grid = scene.layout.zones(Zones.grid(rows=len(VARIANTS), columns=["6fr", "5fr"], column_gap="48px",
                                     padding="page_padding"))
for row, (label, options) in enumerate(VARIANTS):
    code(label, grid[2 * row], anchor="right")
    zone = grid[2 * row + 1]
    options = dict(options)
    rank, fill = options.pop("rank", 0), options.pop("fill", None)
    options.setdefault("size", 0.6)
    name = board.name(rank, **options)
    if "align" in options:
        # The name's position, marked by a rule: align picks the edge that sits on it.
        (x, y) = zone.center
        scene.geometry.line(x, y - 0.45, x, y + 0.45).stroke(rule, 0.03)
        name.move_to(x, y)
    else:
        name.place(zone, anchor="left")
    if fill is not None:
        name.fill(fill)
scene.wait(1.5)
seeks.append(scene.cursor - 0.05)

# The game so far for each rank: points, right answers, answered quizzes,
# the streak of right answers up to the last reveal.
scene.segment("game")
ROWS = [
    ("board.points(rank)", board.points),
    ("board.correct(rank)", board.correct),
    ("board.answered(rank)", board.answered),
    ("board.streak(rank)", board.streak),
]


def by_rank(rows):
    """A table with one row per entry of ``rows`` and a column for each of the first ranks."""
    grid = scene.layout.zones(Zones.grid(rows=len(rows) + 1, columns=["9fr"] + ["2fr"] * COLUMNS, column_gap="32px",
                                         padding="page_padding"))
    for rank in range(COLUMNS):
        board.name(rank, size=0.5, weight=700, align="center").move_to(*grid[1 + rank].center)
    for row, (label, value) in enumerate(rows, start=1):
        first = (COLUMNS + 1) * row
        code(label, grid[first], anchor="right")
        for rank in range(COLUMNS):
            value(rank, grid[first + 1 + rank])


by_rank([(label, lambda rank, zone, value=value: number(value(rank), zone)) for label, value in ROWS])
scene.wait(1.5)
seeks.append(scene.cursor - 0.05)

# One question for each rank: which answer the player chose, whether it
# responded and was right, the points it earned and the seconds it took.
# scene.audience() gives the same by joining order instead of rank.
scene.segment("answers")


def chosen(rank, zone):
    """The answers stacked in one place, each shown only if the player chose it."""
    for index, answer in enumerate(last_quiz.options):
        scene.text(answer, size=0.45).place(zone).opacity(board.chose(rank, last_quiz, index))


by_rank([
    ("board.chose(rank, quiz, i)", chosen),
    ("board.responded(rank, quiz)", lambda rank, zone: number(board.responded(rank, last_quiz), zone)),
    ("board.right(rank, quiz)", lambda rank, zone: number(board.right(rank, last_quiz), zone)),
    ("board.earned(rank, quiz)", lambda rank, zone: number(board.earned(rank, last_quiz), zone)),
    ("board.answer_time(rank, quiz)",
     lambda rank, zone: scene.viz.readout(board.answer_time(rank, last_quiz), format=".1f", suffix=" s",
                                          font_size=0.55).place(zone)),
])
scene.wait(1.5)
seeks.append(scene.cursor - 0.05)

# A podium: second, first and third from left to right, each bar measuring
# its points against the leader's.
scene.segment("podium")
page = scene.layout.zones(Zones.rows(["8fr", "1fr"], gap="24px", padding="page_padding"))
grid = page[0].split(Zones.columns(3, gap="96px"))
MEDALS = [accent, foreground, muted]
for rank, zone in zip([1, 0, 2], grid):
    step = zone.split(Zones.rows(["1fr", "1fr", "6fr"], gap="16px"))
    board.name(rank, size=0.7, weight=700, align="center").move_to(*step[0].center)
    number(board.points(rank), step[1], suffix=" pts")
    column = board.bar(rank, length=step[2].height, thickness=step[2].width * 0.8, direction="up", radius=0.15)
    column.fill(MEDALS[rank]).no_stroke().place(step[2])
code('board.bar(rank, direction="up", radius=0.15)', page[1], anchor="center")
scene.wait(1.5)
seeks.append(scene.cursor - 0.05)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
