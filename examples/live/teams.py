"""Teams: the audience split into teams whose players' points add up, and a battle between them.

``scene.teams(names)`` deals each joining player to the smallest team (or
lets phones choose with ``choose=True``) and gives each team's players,
score, average and a bar against the leading team. Live zones know each
player's team, so the scene can line the teams up and stage the battle.
The rehearsal deals its made-up players the same way and, with one skill
per team, plays an uneven game. Each label is the code that builds what it
shows.
"""

import math
import os

from gaanim import Scene, Zones, computed
from gaanim.live import ease_out_back, impact, lerp, pose, smoothstep

scene = Scene()
muted = scene.colors.muted
track = scene.colors.header
teams = scene.teams(["Comets", "Rockets"])
# One skill per team: the first answers right far more often.
scene.rehearsal(16, seed=2, skill=[0.85, 0.4], arrive=3.0)
audience = scene.audience()
seeks = []

# Where the teams stand, in scene units: rows of five on each side.
COLUMN_STEP, ROW_STEP, FRONT = 1.1, 2.0, 1.4
FLOOR = -0.4


def code(label, zone, anchor="left"):
    """A variant label: the code that builds it."""
    return scene.text(label, role="code").place(zone, anchor=anchor)


def side(p):
    """-1 for the first team, on the left; 1 for the second, on the right."""
    return -1 if p.team == 0 else 1


def spot(p):
    """A player's place on its team's side, by its order of arrival in the team."""
    column, row = p.team_index % 5, p.team_index // 5
    return side(p) * (FRONT + column * COLUMN_STEP), FLOOR - row * ROW_STEP


def line_up(p):
    """Run in from the team's edge of the frame and land in place."""
    x, y = spot(p)
    run = smoothstep(min(p.t / 0.8, 1))
    sx, sy = impact(p.t - 0.8, amount=0.3)
    return pose(lerp(side(p) * 9, x, run), y, sx=sx, sy=sy, flip=side(p) > 0, express="happy", since=0.8)


def tug(p):
    """Everyone leans back on the rope; the leading team drags it to its side."""
    x, y = spot(p)
    ahead = p.team_rank == 0
    drag = 1.0 * ease_out_back(min(p.time / 1.2, 1)) * (1 if ahead else -1)
    heave = 0.12 * math.sin(2 * math.pi * (p.t * 1.4 + p.team_index * 0.13))
    return pose(x + side(p) * (drag + heave), y, lean=side(p) * 0.25, flip=side(p) > 0,
                express="winner" if ahead else "hurt", loop=True)


def team_header(zone):
    """Each team's name in its color, over its half of ``zone``."""
    for team, half in enumerate(zone.split(Zones.columns(2, gap="96px"))):
        scene.text(teams.names[team], size=0.8, weight=800).fill(teams.colors[team]).place(half)


# Players are dealt to the smaller team as they join. In a live zone each
# one knows p.team and p.team_index, its order of arrival in the team.
scene.segment("room")
page = scene.layout.zones(Zones.rows(["1fr", "1fr", "6fr", "1fr"], gap="16px", padding="page_padding"))
team_header(page[0])
for team, half in enumerate(page[1].split(Zones.columns(2, gap="96px"))):
    scene.viz.readout(teams.players(team), format=".0f", suffix=" players", font_size=0.5, color=muted).place(half)
code("teams.names[team], teams.players(team), live_zone(audience, line_up)", page[3], anchor="center")
scene.live_zone(audience, line_up, size=1.6, names=True, name_size=0.24)
start = scene.cursor
scene.wait(4.0)
seeks.extend([start + 1.2, start + 2.4, scene.cursor - 0.05])
scene.stop("room", until=audience.at_least(16))

# Two quizzes, revealed in turn. A player's points count for its team:
# score adds them up, average divides by the team's players, bar measures
# the score against the leading team's, and leader() is that team's index.
scene.segment("scores")
QUIZZES = [
    ("7 × 8?", ["54", "56", "64"], 1),
    ("Largest planet?", ["Saturn", "Earth", "Jupiter"], 2),
]
grid = scene.layout.zones(Zones.grid(rows=6, columns=["7fr", "6fr", "6fr"], column_gap="48px", row_gap="16px",
                                     padding="page_padding"))
caption = grid[0]
for team in range(len(teams)):
    scene.text(teams.names[team], size=0.7, weight=800).fill(teams.colors[team]).place(grid[1 + team])
ROWS = ["teams.score(team)", "teams.average(team)", "teams.bar(team)", "teams.leader() == team"]
for row, label in enumerate(ROWS, start=1):
    code(label, grid[3 * row], anchor="right")
leader = teams.leader()
for team in range(len(teams)):
    scene.viz.readout(teams.score(team), format=".0f", suffix=" pts", font_size=0.6).place(grid[4 + team])
    scene.viz.readout(teams.average(team), format=".0f", suffix=" pts each", font_size=0.6).place(grid[7 + team])
    lane = grid[10 + team]
    scene.geometry.rect(lane.width, 0.5).fill(track).no_stroke().place(lane)
    teams.bar(team, length=lane.width, thickness=0.5).fill(teams.colors[team]).no_stroke().place(lane)
    leading = computed(lambda index, team=team: 1.0 if round(index) == team else 0.15, inputs=[leader])
    scene.geometry.star(5, 0.4, 0.18).fill(teams.colors[team]).no_stroke().place(grid[13 + team]).opacity(leading)
questions = []
for question, options, correct in QUIZZES:
    quiz = scene.quiz(question, options, correct=correct)
    shown = scene.text(question, size=0.6, weight=700).place(caption, anchor="right").hidden()
    scene.play([shown.animate.fade_in().duration(0.3)] + [old.animate.fade_out().duration(0.3) for old in questions])
    questions = [shown]
    scene.wait(0.9)
    scene.stop()
    quiz.reveal()
    scene.wait(1.0)
    seeks.append(scene.cursor - 0.05)

# The battle: p.team_rank is 0 for the leading team, which drags the rope to
# its side; each bar grows away from the middle with direction.
scene.segment("battle")
page = scene.layout.zones(Zones.rows(["1fr", "1fr", "6fr", "1fr"], gap="16px", padding="page_padding"))
team_header(page[0])
for team, half in enumerate(page[1].split(Zones.columns(2, gap="96px"))):
    direction = "left" if team == 0 else "right"
    scene.geometry.rect(half.width, 0.35).fill(track).no_stroke().place(half)
    bar = teams.bar(team, length=half.width, thickness=0.35, direction=direction)
    bar.fill(teams.colors[team]).no_stroke().place(half)
scene.geometry.rect(15, 0.1).fill(muted).no_stroke().move_to(0, FLOOR + 0.6)
code('teams.bar(0, direction="left"), teams.bar(1), live_zone(audience, tug)', page[3], anchor="center")
scene.live_zone(audience, tug, size=1.6, lean=0.0)
start = scene.cursor
scene.wait(3.0)
seeks.extend([start + 0.6, scene.cursor - 0.05])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
