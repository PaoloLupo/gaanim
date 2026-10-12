"""Zone games: characters that move with the game, by the answer each player chose and its rank.

A live zone's behavior reads the game from its player: ``p.answer`` and
``p.answer_points`` for the last question, ``p.rank``, ``p.score`` and
``p.leader`` for the standings, and ``p.previous_rank`` and ``p.rank_since``
to animate a change of place. Outside a live presentation the rehearsal
answers the quizzes, so the characters run, race and celebrate in
snapshots too. Each label is the code that places the characters.
"""

import math
import os

from gaanim import Scene, Zones
from gaanim.live import ease_out_back, impact, lerp, pose, progress, smoothstep, spring

scene = Scene()
foreground = scene.canvas.color("foreground")
background = scene.canvas.color("background")
accent = scene.canvas.color("accent")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")
scene.rehearsal(12, seed=8, arrive=0)
audience = scene.audience()
board = scene.leaderboard()
seeks = []


def code(label, zone, anchor="left"):
    """A variant label: the code that builds it."""
    return scene.text(label, role="code", markup=False).place(zone, anchor=anchor)


# After the reveal each character runs under the answer it chose, p.answer
# (-1 without one). Right answers earned points, so those players jump; the
# rest slump, and players who did not answer wait in a corner.
scene.segment("answers")
quiz = scene.quiz("How many bones does an adult have?", ["186", "206", "306"], correct=1, rehearse=0.55)
page = scene.layout.zones(Zones.rows(["1fr", "2fr", "6fr", "1fr"], gap="16px", padding="page_padding"))
scene.text(quiz.question, size=0.6, weight=700).place(page[0])
tiles = page[1].split(Zones.columns(3, gap="48px"))
for index, tile in enumerate(tiles):
    scene.geometry.rounded_rect(tile.width, tile.height, 0.2).fill(quiz.color(index)).no_stroke().place(tile)
    scene.text(quiz.options[index], size=0.7, weight=700).fill(background).place(tile)
code("x = COLUMNS[p.answer], jumping if p.answer_points > 0", page[3], anchor="center")
COLUMNS = tuple(tile.center[0] for tile in tiles)
FLOOR, SPREAD = page[2].bottom + 0.3, tiles[0].width * 0.7
CORNER = (page[2].left + 0.4, page[2].bottom + 0.2)
scene.wait(1.0)
scene.stop("answering")
quiz.reveal()


def run_to_answer(p):
    if p.answer < 0:
        return pose(CORNER[0] + 0.5 * p.index, CORNER[1], scale=0.6, express="sad", loop=True)
    x = COLUMNS[p.answer] + SPREAD * (p.random(1) - 0.5)
    y = FLOOR + 2.4 * p.random(2)
    leave = 0.4 * p.random(3)
    run = progress(p.t, leave, 0.8)
    if run < 1:
        # Runs in from below the frame, in an arc.
        return pose(lerp(16 * p.random(4) - 8, x, ease_out_back(run)),
                    lerp(-6, y, smoothstep(run)) + math.sin(math.pi * run), express="surprised", since=0)
    arrived = p.t - leave - 0.8
    if p.answer_points > 0:
        beat = (arrived + p.random(5)) % 0.8 / 0.8
        return pose(x, y + 1.2 * beat * (1 - beat), express="winner", loop=True)
    sx, sy = impact(arrived, amount=0.4)
    return pose(x, y, sx=sx, sy=sy, express="sad", since=leave + 0.8)


scene.live_zone(audience, run_to_answer, size=1.1)
start = scene.cursor
scene.wait(2.5)
seeks.extend([start + 0.5, scene.cursor - 0.05])

# The five leaders race: each runs to its score against the leader's, in the
# lane of its rank, and changes lane with an overshoot when it overtakes or
# is overtaken. Quizzes revealed in this segment move the race.
scene.segment("race")
LANES = 5
page = scene.layout.zones(Zones.rows(["1fr", "6fr", "1fr"], gap="16px", padding="page_padding"))
lanes = page[1].split(Zones.grid(rows=LANES, columns=["2fr", "11fr"], column_gap="32px"))
for rank in range(LANES):
    board.name(rank, size=0.45, weight=700).place(lanes[2 * rank], anchor="left")
    lane = lanes[2 * rank + 1]
    scene.geometry.rect(lane.width, 0.03).fill(rule).no_stroke().place(lane, anchor="bottom")
code("y = LANE[p.rank], x = lerp(START, END, p.score / p.leader)", page[2], anchor="center")
LANE = tuple(lanes[2 * rank + 1].bottom + 0.05 for rank in range(LANES))
START, END = lanes[1].left + 0.4, lanes[1].right - 0.4


def score_race(p):
    y = lerp(LANE[min(p.previous_rank, LANES - 1)], LANE[min(p.rank, LANES - 1)], ease_out_back(p.rank_since / 0.6))
    score = lerp(p.previous_score, p.score, smoothstep(p.score_since / 0.8))
    x = lerp(START - 1, lerp(START, END, score / max(p.leader, 1)), spring(p.t, 0.9, 0.45))
    stride = abs(math.sin(2 * math.pi * 1.6 * p.t))
    mood = "winner" if p.rank == 0 else None
    return pose(x, y + 0.12 * stride, express=mood, loop=True, visible=p.rank < LANES, show_name=False)


scene.live_zone(audience, score_race, size=1.0)
QUIZZES = [("7 × 8?", ["54", "56", "64"], 1), ("Largest planet?", ["Saturn", "Earth", "Jupiter"], 2)]
captions = []
for question, options, correct in QUIZZES:
    quiz = scene.quiz(question, options, correct=correct)
    caption = scene.text(question, size=0.55, weight=700).place(page[0]).hidden()
    scene.play([caption.animate.fade_in().duration(0.3)] + [old.animate.fade_out().duration(0.3) for old in captions])
    captions = [caption]
    scene.wait(0.7)
    scene.stop()
    quiz.reveal()
    reveal = scene.cursor
    scene.wait(1.6)
    seeks.extend([reveal + 0.3, scene.cursor - 0.05])

# The podium: ranks 0 to 2 stand on their blocks and the leader keeps
# jumping; everyone else cheers from the floor.
scene.segment("podium")
page = scene.layout.zones(Zones.rows(["6fr", "2fr", "1fr"], gap="16px", padding="page_padding"))
blocks = page[0].split(Zones.columns(3, gap="64px"))
HEIGHTS = (3.0, 2.2, 1.5)
PLACES = []
for rank, block in zip((1, 0, 2), blocks):
    height = HEIGHTS[rank]
    color = (accent, foreground, muted)[rank]
    scene.geometry.rect(block.width, height).fill(color).no_stroke().place(block, anchor="bottom")
    name = board.name(rank, size=0.5, weight=700, align="center").fill(background)
    name.move_to(block.center[0], block.bottom + height - 0.4)
    PLACES.append((rank, block.center[0], block.bottom + height))
PODIUM = tuple((x, top) for _, x, top in sorted(PLACES))
CROWD = (page[1].left + 0.6, page[1].right - 0.6, page[1].bottom + 0.2)
code("PODIUM[p.rank] if p.rank < 3 else the crowd", page[2], anchor="center")


def celebrate(p):
    if p.rank < 3:
        x, top = PODIUM[p.rank]
        if p.rank > 0:
            return pose(x, top, scale=1.2, lean=0.08 * math.sin(2 * math.pi * p.t / 1.3), look_y=0.6)
        beat = p.t % 1.0
        return pose(x, top + 1.6 * beat * (1 - beat), scale=1.3, express="winner", loop=True)
    left, right, floor = CROWD
    x = lerp(left, right, (p.rank - 3) / 8)
    return pose(x, floor + 0.15 * abs(math.sin(2 * math.pi * (p.t / 0.9 + p.rank / 8))), scale=0.8, look_y=1)


scene.live_zone(audience, celebrate, size=1.2)
start = scene.cursor
scene.wait(2.0)
seeks.extend([start + 0.5, scene.cursor - 0.05])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
