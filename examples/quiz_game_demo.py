"""A Kahoot-style game: a timed quiz and a podium, designed in the scene.

`gaanim --present quiz_game_demo.py` takes answers through the relay set with
`gaanim relay use <URL>`. Phones join with a nickname, answer against the
clock and learn their result when the presentation reveals it. The quiz and
the leaderboard only give data; every shape and text here is the scene's.
"""

import os

from gaanim import BLUE, GOLD, GRAY, GREEN, RED, WHITE, Scene

scene = Scene(frame=(16, 9), background="#121212")
COLORS = [RED, BLUE, GOLD, GREEN]

# --- Slide 1: the question -----------------------------------------------------
scene.segment("Pregunta", notes="Wait while phones answer, then advance to reveal.")
answers = ["x", "2x", "x²/2", "2"]
# Previews and exports play a made-up class of 20: they join, answer and score.
scene.rehearsal(20, seed=1)
quiz = scene.quiz("¿Cuál es la derivada de x²?", answers, correct=1, time=20, rehearse=0.7)

title = scene.text(quiz.question, role="title", color=WHITE).move_to(0, 3.4)
card = scene.geometry.rounded_rect(3.0, 3.0, 0.2).fill(WHITE).no_stroke().move_to(-6.0, 1.2)
qr = quiz.qr(2.6).move_to(-6.0, 1.2)
code = scene.text(quiz.code, size=0.45, color=WHITE).move_to(-6.0, -0.7)
clock = scene.viz.readout(quiz.remaining(), format=".0f", color=GOLD, font_size=0.9)
clock.move_to(-6.0, -2.2)

tiles = []
for index, answer in enumerate(answers):
    x = -1.6 + (index % 2) * 5.2
    y = 1.3 - (index // 2) * 2.1
    tile = scene.geometry.rounded_rect(4.8, 1.7, 0.2).fill(COLORS[index]).no_stroke().move_to(x, y)
    label = scene.text(answer, size=0.6, color=WHITE).move_to(x, y + 0.25)
    bar = quiz.bar(index, length=4.2, thickness=0.18, radius=0.09, scale="total")
    bar.fill(WHITE).no_stroke().move_to(x, y - 0.5)
    votes = scene.viz.readout(quiz.votes(index), format=".0f", color=WHITE, font_size=0.32)
    votes.move_to(x + 1.8, y + 0.25)
    tiles.append((tile, label, bar, votes))

scene.play([title.animate.write().duration(0.5)])
scene.play(
    [card.animate.fade_in(), qr.animate.fade_in(), code.animate.fade_in(), clock.animate.fade_in()]
    + [piece.animate.fade_in() for tile, label, _, _ in tiles for piece in (tile, label)]
)
scene.stop("respondiendo")

# Advancing reveals the answer on every phone, then shows how the room voted.
quiz.reveal()
scene.play(
    [tile.animate.opacity(0.25) for index, (tile, _, _, _) in enumerate(tiles) if index != quiz.correct]
    + [bar.animate.create() for _, _, bar, _ in tiles]
    + [votes.animate.fade_in() for _, _, _, votes in tiles]
)
scene.stop("respuesta")

# --- Slide 2: the podium ------------------------------------------------------
scene.segment("Podio", notes="The top three so far.")
board = scene.leaderboard()
heading = scene.text("Podio", role="title", color=GOLD).move_to(0, 3.4)
players = scene.viz.readout(board.players(), format=".0f", suffix=" jugadores", color=GRAY, font_size=0.35)
players.move_to(0, 2.6)

# Second, first and third, left to right, with the leader's column tallest.
podium = []
for rank, x in [(1, -4.0), (0, 0.0), (2, 4.0)]:
    column = board.bar(rank, length=4.0, thickness=2.6, direction="up", radius=0.15)
    column.fill([GOLD, "#c0c0c0", "#cd7f32"][rank]).no_stroke().move_to(x, -1.2)
    name = board.name(rank, size=0.55, align="center").fill(WHITE).move_to(x, 1.9)
    points = scene.viz.readout(board.points(rank), format=".0f", suffix=" pts", color=WHITE, font_size=0.35)
    points.move_to(x, 1.3)
    podium.append((column, name, points))

scene.play([heading.animate.write().duration(0.5), players.animate.fade_in()])
scene.play([column.animate.create().duration(0.8) for column, _, _ in podium])
scene.play([piece.animate.fade_in() for _, name, points in podium for piece in (name, points)])
scene.stop("podio")

if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [stop.time for stop in scene.stops])

scene.render()
