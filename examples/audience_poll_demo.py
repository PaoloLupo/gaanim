"""Ask the audience and present the answers your way.

`gaanim --present audience_poll_demo.py` takes votes through the relay set
with `gaanim relay use <URL>`. The poll only gives data: a QR code, the
session code and live values. Everything on screen is ordinary scene
content, so it can be styled, laid out and animated like the rest.
"""

import os

from gaanim import BLUE, GOLD, GRAY, GREEN, RED, WHITE, Scene, computed

scene = Scene(frame=(16, 9), background="#121212")
COLORS = [RED, BLUE, GOLD, GREEN]

# --- Slide 1: horizontal bars next to the QR code ---------------------------
scene.segment("Pregunta", notes="Wait for the room to scan the code.")
answers = ["x²", "2ˣ", "x log x"]
poll = scene.poll("¿Qué curva crece más rápido?", answers, preview=[6, 14, 4])

card = scene.geometry.rounded_rect(4.2, 4.2, 0.25).fill(WHITE).no_stroke().move_to(-5, -0.3)
qr = poll.qr(3.6).move_to(-5, -0.3)
code = scene.text(poll.code, size=0.55, color=WHITE).move_to(-5, -3.0)
question = scene.text(poll.question, role="title", color=GOLD).move_to(0, 3.3)

rows = []
for index, answer in enumerate(answers):
    y = 1.2 - index * 1.4
    label = scene.text(answer, size=0.5, color=WHITE).move_to(-1.6, y)
    track = scene.geometry.rounded_rect(6.0, 0.6, 0.3).fill("#242424").no_stroke().move_to(3.2, y)
    bar = poll.bar(index, length=6.0, thickness=0.6, radius=0.3).fill(COLORS[index]).no_stroke()
    bar.move_to(3.2, y)
    votes = scene.viz.readout(poll.votes(index), format=".0f", color=WHITE, font_size=0.4)
    votes.move_to(6.8, y)
    rows.append((label, track, bar, votes))

total = scene.viz.readout(poll.total(), format=".0f", suffix=" votos", color=GRAY, font_size=0.35)
total.move_to(3.2, -3.0)

scene.play([question.animate.write().duration(0.6)])
scene.play([card.animate.fade_in(), qr.animate.fade_in(), code.animate.fade_in()])
scene.play(
    [piece.animate.fade_in() for label, track, _, votes in rows for piece in (label, track, votes)]
    + [bar.animate.create() for _, _, bar, _ in rows]
    + [total.animate.fade_in()]
)
scene.stop("votando")

# --- Slide 2: columns with percentages, from the same kind of data ----------
scene.segment("Opinión", notes="Close the poll before commenting.")
feedback = scene.poll("¿Te sirvió la explicación?", ["Sí", "Más o menos", "No"], preview=[18, 7, 2])
title = scene.text(feedback.question, role="title", color=GOLD).move_to(0, 3.3)
scene.text(f"Vota en {feedback.url}", size=0.35, color=GRAY).move_to(0, 2.5)

columns = []
for index, answer in enumerate(feedback.options):
    x = -4 + index * 4
    column = feedback.bar(index, length=4.0, thickness=1.6, direction="up", scale="total")
    column.fill(COLORS[index]).no_stroke().move_to(x, -0.6)
    percent = scene.viz.readout(
        computed(lambda share: 100 * share, inputs=[feedback.share(index)]),
        format=".0f",
        suffix="%",
        color=WHITE,
        font_size=0.45,
    ).move_to(x, 1.8)
    label = scene.text(answer, size=0.4, color=WHITE).move_to(x, -3.1)
    columns.append((column, percent, label))

scene.play([title.animate.write().duration(0.6)])
scene.play([piece.animate.fade_in() for column in columns for piece in column])
scene.stop("votando")
scene.wait(0.5)
feedback.close()
scene.stop("resultado")

if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [stop.time for stop in scene.stops])

scene.render()
