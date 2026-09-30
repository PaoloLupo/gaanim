"""Órbita: a two-question game with a lobby and a podium.

`gaanim --present quiz_orbit_demo.py`, then scan the code with a phone.
The quizzes and the leaderboard only give data; the orbits, the burning
fuse, the tiles and the podium are ordinary scene content.
"""

import math
import os

from gaanim import Easing, Scene, computed, parallel

INK = "#0b0d17"
VIOLET = "#a99fff"
GOLD = "#ffc933"
MUTED = "#8b8fa8"
WHITE = "#f7f6ff"
# One neon color and one shape per answer, as on the phones.
NEON = ["#ff4d6d", "#4d9bff", "#ffb020", "#3ddc97"]

scene = Scene(frame=(16, 9), background=INK)
TIME = 20


def answer_shape(index: int, size: float):
    """The answer's shape: triangle, diamond, circle, square."""
    if index == 0:
        return scene.geometry.regular_polygon(3, size)
    if index == 1:
        return scene.geometry.regular_polygon(4, size)
    if index == 2:
        return scene.geometry.circle(size * 0.82)
    return scene.geometry.square(size * 1.35)


# --- Lobby: scan while the answer shapes orbit the code ------------------------
# A warm-up poll gives the QR code and gets the room voting while it waits.
scene.segment("Lobby", notes="Wait until the room has scanned the code.")
warmup = scene.poll("¿Cómo llegas hoy?", ["¡Con todo!", "Tranqui", "Con sueño"], rehearse=[3, 2, 1])
center = (-4.0, -0.2)
halo = scene.geometry.circle(3.3).no_fill().stroke(VIOLET, 0.02).move_to(*center).opacity(0.35)
card = scene.geometry.rounded_rect(4.4, 4.4, 0.35).fill(WHITE).no_stroke().move_to(*center)
card.glow(VIOLET, radius=0.5, intensity=0.9)
qr = warmup.qr(3.8).fill(INK).move_to(*center)
orbiters = []
for index in range(4):
    angle = math.pi / 4 + index * math.pi / 2
    shape = answer_shape(index, 0.42).fill(NEON[index]).no_stroke()
    shape.move_to(center[0] + 3.3 * math.cos(angle), center[1] + 3.3 * math.sin(angle))
    shape.glow(NEON[index], radius=0.35, intensity=1.2)
    orbiters.append(shape)

kicker = scene.text("GAANIM · EN VIVO", size=0.32, color=VIOLET, weight=700).move_to(3.4, 3.6)
title = scene.text("¿Listos para jugar?", size=0.78, color=WHITE, weight=700).move_to(3.4, 2.8)
code = scene.text(warmup.code, size=1.1, color=GOLD, weight=800).move_to(3.4, 1.65)
code.glow(GOLD, radius=0.25, intensity=0.6)
url = scene.text(warmup.url.split("//")[-1], size=0.26, color=MUTED).move_to(3.4, 0.85)
# The warm-up's answers fill in live while the room waits.
ask = scene.text(warmup.question, size=0.4, color=WHITE, weight=700).move_to(3.4, -0.25)
moods = []
for index, mood in enumerate(warmup.options):
    y = -1.15 - index * 0.85
    label = scene.text(mood, size=0.32, color=MUTED).move_to(1.3, y)
    bar = warmup.bar(index, length=3.4, thickness=0.28, radius=0.14, scale="leader")
    bar.fill(NEON[index]).no_stroke().move_to(4.3, y)
    moods.append((label, bar))

scene.play([halo.animate.create().duration(0.8), card.animate.grow_from_center().duration(0.6)])
scene.play(
    [qr.animate.fade_in().duration(0.4)]
    + [shape.animate.spin_in_from_nothing().duration(0.7) for shape in orbiters]
)
scene.play([kicker.animate.fade_in(), title.animate.typewriter(cps=28, cursor=None)])
scene.play(
    [code.animate.fade_in(), url.animate.fade_in(), ask.animate.fade_in()]
    + [label.animate.fade_in() for label, _ in moods]
    + [bar.animate.create().duration(0.8) for _, bar in moods]
)
# The shapes keep orbiting while the room scans.
scene.stop(
    "lobby",
    loop=parallel(
        *[
            shape.animate.rotate_by(math.tau)
            .about_point(*center)
            .duration(14)
            .easing(Easing.LINEAR)
            for shape in orbiters
        ]
    ),
)


# --- A question ----------------------------------------------------------------
def question_slide(number: int, quiz, answers: list[str]) -> None:
    kicker = scene.text(f"PREGUNTA {number} / 2", size=0.3, color=VIOLET, weight=700)
    kicker.move_to(-5.9, 3.75)
    question = scene.text(quiz.question, size=0.72, color=WHITE, weight=700).move_to(-1.9, 2.85)

    # The fuse burns down with the quiz's clock.
    remaining = quiz.remaining()
    left, length, y = -7.3, 11.4, 1.8
    track = scene.geometry.line(left, y, left + length, y).stroke("#23263a", 0.12)
    fuse = scene.geometry.tracking_line(
        (left, y),
        scene.geometry.point_ref(
            computed(lambda seconds: left + length * seconds / quiz.time, inputs=[remaining]),
            y,
        ),
    ).stroke(GOLD, 0.12)
    fuse.glow(GOLD, radius=0.3, intensity=1.1)
    ring = scene.geometry.annulus(1.12, 0.98).fill("#23263a").no_stroke().move_to(6.1, 2.6)
    # A readout is placed by its left edge, so its digits never jump as the
    # value changes: shift it by about half its width to center it.
    clock = scene.viz.readout(remaining, format=".0f", color=GOLD, font_size=1.05).move_to(5.6, 2.6)
    answered = scene.viz.readout(
        quiz.total(), format=".0f", suffix=" respuestas", color=MUTED, font_size=0.3
    ).move_to(-7.3, 1.38)

    tiles = []
    for index, answer in enumerate(answers):
        x = -3.95 + (index % 2) * 7.9
        y_tile = -0.15 - (index // 2) * 2.5
        tile = scene.geometry.rounded_rect(7.5, 2.25, 0.3).fill(NEON[index]).no_stroke()
        tile.move_to(x, y_tile)
        icon = answer_shape(index, 0.38).fill(WHITE).no_stroke().move_to(x - 3.0, y_tile + 0.15)
        label = scene.text(answer, size=0.62, color=WHITE, weight=700).move_to(x - 0.2, y_tile + 0.15)
        bar = quiz.bar(index, length=6.7, thickness=0.16, radius=0.08, scale="total")
        bar.fill(WHITE).no_stroke().move_to(x, y_tile - 0.78)
        votes = scene.viz.readout(quiz.votes(index), format=".0f", color=WHITE, font_size=0.36)
        votes.move_to(x + 3.2, y_tile + 0.15)
        tiles.append((tile, icon, label, bar, votes))
    code = scene.text(f"Únete con el código {quiz.code}", size=0.26, color=MUTED).move_to(-5.6, -4.2)

    scene.play([kicker.animate.fade_in(), question.animate.typewriter(cps=34, cursor=None)])
    scene.play(
        [track.animate.create().duration(0.4), fuse.animate.create().duration(0.4)]
        + [ring.animate.grow_from_center(), clock.animate.fade_in(), answered.animate.fade_in()]
        + [code.animate.fade_in()]
        + [
            piece.animate.spin_in_from_nothing().duration(0.55).delay(0.08 * index)
            for index, (tile, icon, label, _, _) in enumerate(tiles)
            for piece in (tile, icon, label)
        ]
    )
    scene.stop("respondiendo")

    # Advancing reveals the answer on every phone, then here.
    quiz.reveal()
    right = tiles[quiz.correct]
    scene.play(
        [
            piece.animate.opacity(0.16).duration(0.5)
            for index, (tile, icon, label, _, _) in enumerate(tiles)
            if index != quiz.correct
            for piece in (tile, icon, label)
        ]
        + [fuse.animate.fade_out(), track.animate.fade_out(), ring.animate.fade_out(), clock.animate.fade_out()]
        + [right[0].animate.glow(NEON[quiz.correct], radius=0.55, intensity=1.4).duration(0.5)]
    )
    scene.play(
        [right[0].animate.indicate().duration(0.6), right[0].animate.flash_around(color=WHITE, width=0.06)]
        + [bar.animate.create().duration(0.9) for _, _, _, bar, _ in tiles]
        + [votes.animate.fade_in() for _, _, _, _, votes in tiles]
    )
    scene.stop("respuesta")


scene.segment("Pregunta 1", notes="Advance to reveal once the fuse burns out.")
first = scene.quiz(
    "¿Cuál es la derivada de x²?",
    ["x", "2x", "x²/2", "2"],
    correct=1,
    time=TIME,
    rehearse=0.75,
)
question_slide(1, first, ["x", "2x", "x²/2", "2"])

scene.segment("Pregunta 2", notes="Advance to reveal once the fuse burns out.")
second = scene.quiz(
    "¿Qué polígono tiene más lados?",
    ["Triángulo", "Cuadrado", "Hexágono", "Pentágono"],
    correct=2,
    time=TIME,
    rehearse=0.6,
)
question_slide(2, second, ["Triángulo", "Cuadrado", "Hexágono", "Pentágono"])

# --- Podium ----------------------------------------------------------------------
scene.segment("Podio", notes="Celebrate the top three.")
board = scene.leaderboard()
heading = scene.text("Podio", size=0.85, color=WHITE, weight=800).move_to(0, 3.45)
players = scene.viz.readout(
    board.players(), format=".0f", suffix=" jugadores", color=MUTED, font_size=0.34
).move_to(-0.85, 2.7)
medals = [GOLD, "#d7dbe8", "#e39b5f"]
podium = []
for rank, x in [(1, -4.4), (0, 0.0), (2, 4.4)]:
    column = board.bar(rank, length=4.6, thickness=3.2, direction="up", radius=0.3)
    column.fill(medals[rank]).no_stroke().move_to(x, -1.95).opacity(0.9)
    medal = scene.geometry.circle(0.42).fill(INK).stroke(medals[rank], 0.08).move_to(x, -3.55)
    place = scene.text(str(rank + 1), size=0.45, color=medals[rank], weight=800).move_to(x, -3.55)
    name = board.name(rank, size=0.62, weight=700, align="center").fill(WHITE).move_to(x, 1.55)
    points = scene.viz.readout(
        board.points(rank), format=".0f", suffix=" pts", color=medals[rank], font_size=0.36
    ).move_to(x - 0.65, 0.9)
    podium.append((column, medal, place, name, points))

scene.play([heading.animate.typewriter(cps=20, cursor=None), players.animate.fade_in()])
scene.play(
    [column.animate.create().duration(0.9).delay(0.25 * order) for order, (column, *_rest) in enumerate(podium)]
)
scene.play(
    [piece.animate.fade_in() for _, medal, place, name, points in podium for piece in (medal, place, name, points)]
    + [podium[1][0].animate.glow(GOLD, radius=0.6, intensity=1.3)]
)
scene.stop("podio")

if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [stop.time for stop in scene.stops])

scene.render()
