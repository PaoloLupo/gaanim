"""Quizzes: questions with a right answer, a countdown and a reveal, and the kinds of quiz.

``scene.quiz`` is a poll with a correct answer: phones answer once against
the clock, and a right answer earns more points the sooner it comes. The
scene draws the clock, keeps the counts hidden until ``reveal()`` and marks
the right answer; ``load_questions`` reads quizzes and polls from a Markdown
file. The rehearsal answers every quiz, so snapshots show the clock running
down and the room's answers. Each label is the code that builds what it shows.
"""

import os

from gaanim import Scene, Zones, load_questions

scene = Scene()
background = scene.canvas.color("background")
chart = scene.canvas.color("chart")
success = scene.canvas.color("success")
track = scene.canvas.color("header")
scene.rehearsal(20, seed=1)
seeks = []


def code(label, zone, anchor="left"):
    """A variant label: the code that builds it."""
    return scene.text(label, role="code").place(zone, anchor=anchor)


def bar(poll, index, zone, thickness=0.4):
    """``poll``'s bar for ``index`` across ``zone``, over a track, sized by its share."""
    scene.geometry.rect(zone.width, thickness).fill(track).no_stroke().place(zone)
    return poll.bar(index, length=zone.width, thickness=thickness, scale="total").fill(chart).no_stroke().place(zone)


def answer_cell(poll, index, zone):
    """Draw a vote count and a bar for one answer, and return the bar."""
    count, stage = zone.split(Zones.columns(["2fr", "5fr"], gap="16px"))
    scene.viz.readout(poll.votes(index), format=".0f", font_size=0.55).place(count, anchor="right")
    return bar(poll, index, stage)


def mark_right(quiz, bars):
    """After the reveal, turn the right answers' bars to the success color."""
    right = quiz.correct if isinstance(quiz.correct, list) else [quiz.correct]
    scene.play([bars[index].animate.fill(success).duration(0.3) for index in right])


# The question: how many players answered, a countdown and a tile per answer
# with its shape and color (phones shuffle a quiz's answers so that nobody
# copies a neighbor). The counts stay hidden while the room answers:
# quiz.revealed() is 0 until reveal() and 1 after, so it drives their opacity.
scene.segment("question")
quiz = scene.quiz("What is the derivative of x²?", ["x", "2x", "x²/2", "2"], correct=1, time=20, rehearse=0.7)
page = scene.layout.zones(Zones.rows(["2fr", "7fr", "1fr"], gap="24px", padding="page_padding"))
top = page[0].split(Zones.columns(["11fr", "4fr"], gap="48px"))
title, answered = top[0].split(Zones.rows(["2fr", "1fr"]))
scene.text(quiz.question, size=0.7, weight=700).place(title, anchor="left")
count, label = answered.split(Zones.columns(["1fr", "1fr"], gap="32px"))
scene.viz.readout(quiz.total(), format=".0f", suffix=" answered", font_size=0.45).place(count, anchor="left")
code("quiz.total()", label, anchor="left")
clock, label = top[1].split(Zones.rows(["3fr", "1fr"]))
scene.viz.readout(quiz.remaining(), format=".0f", font_size=1.3).place(clock)
code("quiz.remaining()", label, anchor="center")
grid = page[1].split(Zones.grid(rows=2, columns=2, gap="32px"))
shown = quiz.revealed()
tiles = []
for index, (answer, zone) in enumerate(zip(quiz.options, grid)):
    tile = scene.geometry.rounded_rect(zone.width, zone.height, 0.25).fill(quiz.color(index)).no_stroke().place(zone)
    face, counts = zone.inset(("48px", "24px")).split(Zones.rows(["3fr", "1fr"]))
    icon_zone, label_zone = face.split(Zones.columns(["1fr", "5fr"], gap="24px"))
    icon = quiz.icon(index, 0.8).fill(background).place(icon_zone)
    label = scene.text(answer, size=0.9, weight=700).fill(background).place(label_zone, anchor="left")
    count, stage = counts.split(Zones.columns(["1fr", "5fr"], gap="24px"))
    hidden_bar = quiz.bar(index, length=stage.width, thickness=0.3, scale="total").fill(background).no_stroke()
    hidden_bar.place(stage).opacity(shown)
    scene.viz.readout(quiz.votes(index), format=".0f", font_size=0.5, color=background).place(count).opacity(shown)
    tiles.append((tile, icon, label))
waiting = code("scene.stop(until=quiz.answered(share=1.0) | quiz.time_up())", page[2], anchor="center")
start = scene.cursor
scene.play([piece.animate.fade_in().duration(0.5) for tile in tiles for piece in tile])
scene.wait(3.0)
seeks.extend([start + 1.2, scene.cursor - 0.2])
# Presenting, the stop advances by itself once every player answered or
# the time is up; reveal() then shows each phone its result.
scene.stop("answering", until=quiz.answered(share=1.0) | quiz.time_up())
quiz.reveal()
revealing = code("quiz.reveal()", page[2], anchor="center").hidden()
scene.play(
    [piece.animate.opacity(0.25) for index, tile in enumerate(tiles) if index != quiz.correct for piece in tile]
    + [waiting.animate.fade_out().duration(0.3), revealing.animate.fade_in().duration(0.3)]
)
scene.wait(1.0)
seeks.append(scene.cursor - 0.05)
scene.stop("answer")

# Quizzes opened in turn, each answered by the rehearsal before its stop and
# then revealed. rehearse is the share that gets it right, or one weight per
# answer; correct=[...] makes a multiple choice quiz, right only when a phone
# picks exactly those answers; time is the countdown.
scene.segment("kinds")
ANSWERS = ["A", "B", "C"]
ROWS = [
    ("correct=1", {"correct": 1}),
    ("rehearse=0.9", {"correct": 1, "rehearse": 0.9}),
    ("rehearse=0.2", {"correct": 1, "rehearse": 0.2}),
    ("rehearse=[5, 1, 1]", {"correct": 1, "rehearse": [5, 1, 1]}),
    ("correct=[0, 2]", {"correct": [0, 2]}),
    ("time=8", {"correct": 1, "time": 8}),
]
grid = scene.layout.zones(Zones.grid(rows=len(ROWS) + 1, columns=["7fr", "4fr", "4fr", "4fr", "4fr"],
                                     column_gap="32px", padding="page_padding"))
code("scene.quiz(...)", grid[0], anchor="right")
code("remaining()", grid[1], anchor="center")
for index, answer in enumerate(ANSWERS):
    scene.text(answer, size=0.6).place(grid[2 + index])
start = scene.cursor
for row, (label, options) in enumerate(ROWS, start=1):
    quiz = scene.quiz("Which one?", ANSWERS, **options)
    first = 5 * row
    code(label, grid[first], anchor="right")
    scene.viz.readout(quiz.remaining(), format=".0f", font_size=0.55).place(grid[first + 1])
    bars = [answer_cell(quiz, index, grid[first + 2 + index]) for index in range(len(ANSWERS))]
    scene.wait(1.0)
    scene.stop()
    quiz.reveal()
    mark_right(quiz, bars)
scene.wait(0.6)
seeks.extend([start + 0.5, start + 6.9, scene.cursor - 0.05])

# Questions written in a Markdown file: each heading a question, each list
# item an answer, [x] the right ones and key: value lines for the time, a
# picture, rehearsal weights or several answers. scene.question(q) opens a
# quiz when q has right answers and a poll otherwise.
scene.segment("files")
questions = load_questions("assets/questions.md")
page = scene.layout.zones(Zones.rows(["1fr"] + ["4fr"] * len(questions), gap="32px", padding="page_padding"))
code('questions = load_questions("assets/questions.md")', page[0], anchor="left")
start = scene.cursor
for row, question in enumerate(questions):
    label, body = page[1 + row].split(Zones.columns(["4fr", "13fr"], gap="32px"))
    code(f"questions[{row}]", label, anchor="right")
    title, data = body.split(Zones.rows(["1fr", "2fr"], gap="8px"))
    scene.text(question.text, size=0.45, weight=700).place(title, anchor="left")
    cells = data.split(Zones.columns(["4fr", "3fr", "3fr", "3fr", "3fr"], gap="32px"))
    if question.image:
        # Phones show the picture above the question; the scene places it itself.
        scene.media.image(question.image, height=cells[0].height).place(cells[0], anchor="left")
        code(f"q.image, q.time == {question.time}", title, anchor="right")
    elif question.is_quiz:
        code(f"q.correct == {question.correct}", title, anchor="right")
    else:
        code(f"q.multiple == {question.multiple}", title, anchor="right")
    poll = scene.question(question)
    bars = []
    for index, answer in enumerate(question.options):
        name, stage = cells[1 + index].split(Zones.rows(["1fr", "1fr"]))
        scene.text(answer, size=0.4).place(name, anchor="left")
        bars.append(answer_cell(poll, index, stage))
    scene.wait(1.2)
    if question.is_quiz:
        scene.stop()
        poll.reveal()
        mark_right(poll, bars)
    else:
        poll.close()
scene.wait(0.6)
seeks.extend([start + 2.0, scene.cursor - 0.05])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
