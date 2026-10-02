"""A battle between two teams of the class.

`gaanim --present team_battle_demo.py`, then join with phones: each player is
dealt to the smaller team (pass `choose=True` to `scene.teams` to let phones
pick). Players line up on their team's side, answer two questions, and the
team ahead pulls the rope. Previews and exports play a made-up class of 20,
one team stronger than the other, so the battle can be designed without
phones.
"""

import math

from gaanim import Scene
from gaanim.live import ease_out_back, impact, lerp, pose, smoothstep

RED, BLUE = "#ff4f8b", "#2fb8ff"
INK, WHITE, MUTED, GOLD = "#1a1033", "#fff8ff", "#cbb8ff", "#ffd23f"

scene = Scene(frame=(16, 9), background=INK)
teams = scene.teams(["Rojo", "Azul"], colors=[RED, BLUE])
scene.rehearsal(20, seed=2, skill=[0.85, 0.4])
audience = scene.audience()


def side(p):
    """-1 for the first team (left), 1 for the second (right)."""
    return -1 if p.team == 0 else 1


def slot(p):
    """Where a player stands on their team's side: rows of five."""
    column, row = p.team_index % 5, p.team_index // 5
    return side(p) * (1.6 + column * 0.95), -0.6 - row * 1.25


# --- The room: players run in to their side --------------------------------------
def line_up(p):
    x, y = slot(p)
    run = smoothstep(min(p.t / 0.9, 1))
    start = side(p) * 9
    sx, sy = impact(p.t - 0.9, amount=0.3)
    return pose(lerp(start, x, run), y, sx=sx, sy=sy, flip=side(p) > 0,
                express="happy", since=0.9)


scene.segment("Sala", notes="Wait for everyone; teams are dealt as they join.")
qr_card = scene.geometry.rounded_rect(2.6, 2.6, 0.2).fill(WHITE).no_stroke().move_to(0, 2.6)
audience.qr(2.3).fill(INK).move_to(0, 2.6)
scene.text(audience.code, size=0.5, color=GOLD, weight=700).move_to(0, 0.95)
for team, x in [(0, -4.9), (1, 4.9)]:
    scene.text(teams.names[team], size=0.7, color=teams.colors[team], weight=800).move_to(x, 3.4)
    scene.viz.readout(teams.players(team), format=".0f", suffix=" jugadores", color=MUTED,
                      font_size=0.3).move_to(x, 2.6)
scene.live_zone(audience, line_up, size=0.95, names=True, name_size=0.2)
scene.wait(8)
scene.stop("sala")


# --- Questions ------------------------------------------------------------------------
QUESTIONS = [
    ("¿Cuánto es 7 × 8?", ["54", "56", "64", "48"], 1, 0.75),
    ("¿Qué planeta es el más grande?", ["Saturno", "Tierra", "Júpiter", "Neptuno"], 2, 0.6),
]
TILES = ["#d63a50", "#2f6fd0", "#a86f00", "#23824a"]

for number, (question, options, correct, right) in enumerate(QUESTIONS, start=1):
    scene.segment(f"Pregunta {number}", notes="Advance to reveal the answer.")
    quiz = scene.quiz(question, options, correct=correct, time=20, rehearse=right)
    title = scene.text(question, size=0.65, color=WHITE, weight=800).move_to(0, 3.3)
    clock = scene.viz.readout(quiz.remaining(), format=".0f", color=GOLD, font_size=0.7)
    clock.move_to(6.8, 3.3)
    tiles = []
    for index, option in enumerate(options):
        x, y = -3.9 + index % 2 * 7.8, 1.0 - index // 2 * 2.2
        tile = scene.geometry.rounded_rect(7.3, 1.9, 0.25).fill(TILES[index]).no_stroke().move_to(x, y)
        label = scene.text(option, size=0.6, color=WHITE, weight=700).move_to(x, y + 0.2)
        votes = scene.viz.readout(quiz.votes(index), format=".0f", color=WHITE, font_size=0.32)
        votes.move_to(x + 3.1, y + 0.2)
        tiles.append((tile, label, votes))
    for team, x in [(0, -4.5), (1, 4.5)]:
        scene.viz.readout(teams.score(team), format=".0f", prefix=f"{teams.names[team]}: ",
                          suffix=" pts", color=teams.colors[team], font_size=0.34).move_to(x, -3.9)
    scene.play([title.animate.fade_in(), clock.animate.fade_in()]
               + [piece.animate.fade_in() for tile, label, _ in tiles for piece in (tile, label)])
    scene.stop("respondiendo", until=quiz.answered(share=1.0) | quiz.time_up())
    quiz.reveal()
    scene.play([piece.animate.opacity(0.2) for index, (tile, label, _) in enumerate(tiles)
                if index != correct for piece in (tile, label)]
               + [votes.animate.fade_in() for _, _, votes in tiles])
    scene.stop("respuesta")


# --- The battle: the team ahead pulls the rope ------------------------------------
PULL = 1.4  # how far the leading team drags the rope


def tug(p):
    x, y = slot(p)
    ahead = p.team_rank == 0
    # Everyone leans back on the rope; the team ahead wins ground.
    shift = PULL * ease_out_back(min(p.time / 1.2, 1)) * (1 if ahead else -1)
    heave = 0.15 * math.sin(2 * math.pi * (p.t * 1.4 + p.team_index * 0.13))
    return pose(x + side(p) * (shift + heave), y + 0.6, lean=side(p) * 0.25, flip=side(p) > 0,
                express="winner" if ahead else "hurt", loop=True)


scene.segment("Batalla", notes="The team ahead pulls the rope.")
scene.text("¡Tira de la cuerda!", size=0.7, color=WHITE, weight=800).move_to(0, 3.6)
rope = scene.geometry.rect(15, 0.12).fill("#c89b6d").no_stroke().move_to(0, 0.4)
for team, x in [(0, -4.5), (1, 4.5)]:
    teams.bar(team, length=5.5, thickness=0.35, radius=0.17,
              direction="left" if team == 0 else "right").fill(teams.colors[team]).no_stroke().move_to(x, 2.5)
    scene.viz.readout(teams.average(team), format=".0f", suffix=" pts por jugador",
                      color=MUTED, font_size=0.28).move_to(x, 1.9)
scene.live_zone(audience, tug, size=0.9, lean=0.0)
scene.wait(5)
scene.stop("batalla")

scene.render()
