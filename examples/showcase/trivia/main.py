"""Quiz Night: a live general-knowledge trivia played in teams.

Present it with `gaanim --present .` and let the audience scan the code. Each
player picks a team on their phone, builds a character and smashes into the
screen before landing on their team's side. The questions come from
`questions.md`: an icebreaker poll and quizzes with images and multiple
answers. When an answer is revealed the characters run to the one they
chose, and the game ends with a tug of war between the teams, a score race
and the podium.

In the preview and in exports a made-up class plays (`scene.rehearsal`): it
joins, picks teams, answers and competes the same way every time, so the
whole game can be designed without phones. Live zones (`scene.live_zone`)
take plain Python functions that are compiled when the scene is built, so
the exported `.gaanim` runs them without Python.
"""

import math
import os

from gaanim import (
    Background,
    Emitter,
    PostProcess,
    Scene,
    Theme,
    Transition,
    load_questions,
    magic_move,
)
from gaanim.live import (
    STEP,
    anticipate,
    ease_in,
    ease_in_out,
    ease_out_back,
    impact,
    lerp,
    pose,
    progress,
    smoothstep,
    spring,
    state,
    wobble,
)

INK = "#1c0b3f"
SEA = "#4a2a8c"
WHITE = "#fff8ff"
MUTED = "#cbb8ff"
GOLD = "#ffd23f"
NEON = ["#ff4f8b", "#2fb8ff", "#ff9f1c", "#2ed47a"]
TEAM_COLORS = ["#2fb8ff", "#ff9f1c"]
# Sounds by Kenney (www.kenney.nl, CC0): Interface Sounds and Music Jingles.
TITLE, SLIDE, SHRINK, TILE, POP, QUESTION, RIGHT, FACT, PODIUM = (
    f"assets/sounds/{name}.ogg"
    for name in ("title", "transition", "shrink", "tile", "pop", "question", "correct", "fact", "podium")
)

# Background: a regular grid of lavender dots sliding diagonally over a
# violet gradient. A soft wave swells the dots as it passes, and the center
# stays calmer so the text reads.
DOTS = r"""
fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let frame = gaanim_frame_size(resolution);
    let pixel = frame.y / resolution.y;
    let spacing = 0.6;
    // The grid drifts up and to the right.
    let p = uv * frame - vec2<f32>(0.35, 0.35) * time;
    let local = p - (floor(p / spacing) + 0.5) * spacing;
    let wave = 0.5 + 0.5 * sin((uv.x + uv.y) * 5.0 - time * 1.2);
    let r = 0.035 + 0.03 * wave;
    let cover = 1.0 - smoothstep(r - pixel, r + pixel, length(local));
    let top = vec3<f32>(0.20, 0.07, 0.42);
    let bottom = vec3<f32>(0.08, 0.03, 0.22);
    var color = mix(top, bottom, uv.y);
    let calm = 0.35 + 0.65 * smoothstep(0.1, 0.7, length((uv - 0.5) * vec2<f32>(1.6, 1.0)));
    color = mix(color, vec3<f32>(0.80, 0.72, 1.0), cover * (0.16 + 0.14 * wave) * calm);
    return vec4<f32>(color, 1.0);
}
"""

# Fredoka (Google Fonts, OFL license in assets/fonts): round and playful.
FONTS = Theme(font_dir="assets/fonts", fonts={"text": "Fredoka"})
scene = Scene(background=Background.shader(DOTS, fallback=INK), theme=FONTS)
# Post-processing: the neon colors glow, the corners darken a little and a
# fine grain adds texture.
scene.canvas.post = [
    PostProcess.bloom(threshold=0.72, intensity=0.45, radius=0.4),
    PostProcess.vignette(0.3),
    PostProcess.grain(0.025),
]
# Two teams that each player picks on their phone.
teams = scene.teams(["Owls", "Larks"], choose=True, colors=TEAM_COLORS)
# Outside a live presentation, a made-up class of 18 students joins, picks
# teams, answers and competes: the Owls know a little more.
scene.rehearsal(18, seed=7, skill=[0.7, 0.55])
audience = scene.audience()
board = scene.leaderboard()
questions = load_questions("questions.md")
quizzes = sum(1 for q in questions if q.is_quiz)


def logo():
    """The corner mark: magic move keeps it still between segments."""
    return scene.text("QUIZ NIGHT", size=0.26, color=MUTED, weight=700).move_to(-6.5, 4.15).named("logo")


def team_scores():
    """The team scoreboard, at the bottom on each side."""
    for team, x in [(0, -5.6), (1, 5.6)]:
        pill = scene.geometry.rounded_rect(3.6, 0.6, 0.3).fill(teams.colors[team]).no_stroke()
        pill.opacity(0.25).move_to(x, -4.15).named(f"team{team}")
        scene.viz.readout(teams.score(team), format=".0f", prefix=f"{teams.names[team]}  ",
                          suffix=" pts", color=WHITE, font_size=0.28).move_to(x, -4.15)


# --- Cover: the title drops in with a bounce and a comet circles it ------------------
scene.segment("Cover", notes="Welcome. Advance to open the room.")
logo()
title = scene.text("QUIZ NIGHT", size=1.5, color=WHITE, weight=700).move_to(0, 6.9).named("title")
subtitle = scene.text("general knowledge · live · in teams", size=0.42, color=MUTED).move_to(0, -0.5)
# Sparks rise from the bottom edge for the whole cover.
scene.fx.particles(Emitter.rect(16, 0.1).at((0, -4.7)), rate=14, lifetime=(3, 5), speed=(0.6, 1.4),
                   direction=math.pi / 2, spread=0.4, size=(0.04, 0.09), flutter=0.25,
                   color=NEON + [GOLD], seed=3)
# The comet: a dot that travels and a tail of delayed followers.
comet = scene.geometry.circle(0.13).fill(GOLD).no_stroke().move_to(-9, 3.2)
tail = []
for i in range(12):
    dot = scene.geometry.circle(0.12 * (1 - i / 13)).fill(NEON[i % 4]).no_stroke()
    tail.append(dot.follow(comet, delay=0.035 * (i + 1)))

scene.play([title.animate.move_to(0, 0.9, anchor="center").duration(0.55)
            .settle(overshoot=0.18, frequency=2.5, decay=5)
            .sound(TITLE)])
scene.play([subtitle.animate.typewriter(cps=40, cursor=None)]
           + [dot.animate.fade_in().duration(0.05) for dot in tail])
for x, y in [(-3, 2.4), (4.5, 1.9), (6.2, -1.2), (0, -2.2), (-6, -1.0), (-7.5, 2.6)]:
    scene.play([comet.animate.move_to(x, y).duration(0.45)])
scene.play([comet.animate.move_to(0, 2.2).duration(0.5).settle(overshoot=0.25, frequency=3, decay=4)])
scene.play([title.animate.indicate().duration(0.6).sound(POP)])
scene.stop("cover")


# --- Room: the characters smash into the screen and land with their team -------------
SIZE = 1.0  # height of a character in its seat
BIG = 6.5  # size when it hits the glass
FLY, STUCK, FALL = 0.55, 0.45, 0.8  # seconds: toward the camera, stuck, falling
LAND = FLY + STUCK + FALL


def side(p):
    """-1 for the first team (left), 1 for the second."""
    return -1 if p.team == 0 else 1


def seat(p):
    """Where it stands: rows of four on its team's side."""
    column, row = p.team_index % 4, p.team_index // 4
    return side(p) * (2.7 + column * 1.3), 0.9 - (row % 4) * 1.35


def feet(center_y, scale):
    """The feet of a character centered at ``center_y`` at that scale."""
    return center_y - SIZE * scale / 2


def smash(p):
    x, y = seat(p)
    # Where it hits the glass: near the center, different for every player.
    hit_x = -2.5 + 5 * p.random(1)
    hit_y = -0.3 + 1.6 * p.random(2)
    spin = 1 if p.random(3) < 0.5 else -1
    t = p.t

    if t < FLY:
        # It comes from far away, small, spinning straight at the camera.
        u = ease_in(t / FLY)
        scale = lerp(0.1, BIG, u)
        cx, cy = lerp(0.2 * hit_x, hit_x, u), lerp(0.2 * hit_y + 0.8, hit_y, u)
        return pose(cx, feet(cy, scale), scale=scale, rotation=spin * 2 * math.pi * (1 - u),
                    express="surprised", since=0, show_name=False)

    if t < FLY + STUCK:
        # Splat! Squashed against the glass, it slides down a little.
        stuck = t - FLY
        sx, sy = impact(stuck, amount=0.4, frequency=3, decay=5)
        cy = hit_y - 0.9 * stuck * stuck
        return pose(hit_x, feet(cy, BIG), scale=BIG, sx=sx * 1.08, sy=sy * 0.95,
                    rotation=wobble(stuck, amount=0.08, frequency=4), express="hurt", since=FLY,
                    show_name=False)

    if t < LAND:
        # It peels off and falls toward its team's side, shrinking.
        u = (t - FLY - STUCK) / FALL
        scale = lerp(BIG, 1, ease_in_out(u))
        start_y = hit_y - 0.9 * STUCK * STUCK
        cx = lerp(hit_x, x, smoothstep(u))
        cy = lerp(start_y, y + SIZE / 2, ease_in(u)) + 1.2 * math.sin(math.pi * u)
        return pose(cx, feet(cy, scale), scale=scale, rotation=spin * 0.5 * math.sin(2 * math.pi * u),
                    express="hurt", since=FLY, show_name=False)

    # In its seat: it bounces as it lands, cheers up and looks to the center.
    sx, sy = impact(t - LAND, amount=0.35)
    glance = -side(p) * (0.6 + 0.4 * math.sin(0.6 * t + 6 * p.random(4)))
    return pose(x, y, sx=sx, sy=sy, express="happy", since=LAND, look_x=glance, look_y=0.2)


scene.segment("Room", transition=Transition.magic_move(0.9, key="name", sound=SLIDE),
              notes="Wait for everyone to join and pick a team; advance when they are all in.")
logo()
scene.text("QUIZ NIGHT", size=0.62, color=WHITE, weight=700).move_to(0, 3.95).named("title")
card = scene.geometry.rounded_rect(3.0, 3.0, 0.3).fill(WHITE).no_stroke().move_to(0, 1.75)
qr = audience.qr(2.6).fill(INK).move_to(0, 1.75)
code = scene.text(audience.code, size=0.75, color=GOLD, weight=700).move_to(0, -0.25)
url = scene.text(audience.url.split("//")[-1], size=0.22, color=MUTED).move_to(0, -0.8)
count = scene.viz.readout(audience.count(), format=".0f", suffix=" in the room", color=WHITE,
                          font_size=0.3).move_to(0, -1.3)
headers = []
for team, x in [(0, -4.65), (1, 4.65)]:
    name = scene.text(teams.names[team], size=0.62, color=teams.colors[team], weight=700).move_to(x, 3.3)
    players = scene.viz.readout(teams.players(team), format=".0f", suffix=" players", color=MUTED,
                                font_size=0.26).move_to(x, 2.7)
    headers += [name, players]

scene.play([card.animate.grow_from_center().duration(0.5).settle(), qr.animate.fade_in().duration(0.5)])
scene.play([code.animate.fade_in(), url.animate.fade_in(), count.animate.fade_in()]
           + [piece.animate.fade_in().delay(0.1 * i) for i, piece in enumerate(headers)])
scene.live_zone(audience, smash, size=SIZE, squash=0.0, lean=0.0, names=True, name_size=0.2,
                name_color=WHITE)
scene.wait(9)
scene.stop("room")


# --- Questions -----------------------------------------------------------------------
COLUMNS = (-5.7, -1.9, 1.9, 5.7)  # answer columns at the reveal
ROW_Y = 1.75  # height of the answer row at the reveal


def gather(p, quiz):
    """Each character runs under the answer it chose; in a quiz, those who
    got it right jump and the others sulk."""
    if p.answer < 0:
        # No answer: it stays sad in the corner.
        return pose(-7.3 + 0.35 * (p.index % 6), -3.95, scale=0.6, lean=0.25, express="sad", loop=True,
                    show_name=False)
    x = COLUMNS[0] + (COLUMNS[1] - COLUMNS[0]) * p.answer + 2.6 * (p.random(5) - 0.5)
    y = -0.5 - 2.6 * p.random(6)
    start_x = 16 * p.random(7) - 8
    wait = 0.5 * p.random(8)
    run = progress(p.t, wait, wait + 0.75)
    cx = lerp(start_x, x, ease_out_back(run))
    cy = lerp(-6, y, smoothstep(run)) + 1.0 * math.sin(math.pi * run)
    landed = p.t - wait - 0.75
    if landed < 0:
        return pose(cx, cy, scale=0.85, express="surprised", since=0, show_name=False)
    if quiz and p.answer_points > 0:
        # Right! It jumps again and again, higher on a long streak.
        beat = (landed + p.random(9)) % 0.8
        hop = (0.35 + 0.1 * min(p.streak, 4)) * 4 * (beat / 0.8) * (1 - beat / 0.8)
        return pose(x, y + hop, scale=0.85, express="winner", loop=True, show_name=False)
    if quiz:
        sx, sy = impact(landed, amount=0.4)
        return pose(x, y, scale=0.85, sx=sx * 1.05, sy=sy * 0.9, lean=0.12 * side(p), express="sad",
                    since=0.75 + wait, show_name=False)
    sx, sy = impact(landed, amount=0.3)
    return pose(x, y, scale=0.85, sx=sx, sy=sy, express="happy", since=0.75 + wait, show_name=False,
                look_y=0.8)


def gather_quiz(p):
    return gather(p, True)


def gather_poll(p):
    return gather(p, False)


def tile(poll, index, width, height, label_size):
    """An answer with the color and icon it has on the phone, as a named group."""
    color = poll.color(index)
    box = scene.geometry.rounded_rect(width, height, min(0.3, height / 3)).fill(color).no_stroke()
    badge_x = -width / 2 + height * 0.5
    badge = scene.geometry.circle(height * 0.32).fill(WHITE).no_stroke().move_to(badge_x, 0)
    icon = poll.icon(index, height * 0.36).move_to(badge_x, 0)
    # Long answers shrink to fit their tile (Fredoka is about 0.55 of its
    # size per letter).
    room = width - height * 1.25
    text = poll.options[index]
    size = min(label_size, room / (0.55 * len(text)))
    label = scene.text(text, size=size, color=WHITE, weight=700, text_box="cap")
    label.move_to(badge_x + height * 0.45 + (width - height * 0.95) / 2, 0)
    return scene.geometry.group([box, badge, icon, label]).named(f"a{index}")


def kicker_text(q, number):
    if not q.is_quiz:
        return "POLL · PICK AS MANY AS YOU LIKE" if q.multiple else "POLL"
    head = f"QUESTION {number} OF {quizzes}"
    return head + " · PICK EVERY RIGHT ANSWER" if q.multiple else head


def question_slide(q, number):
    poll = scene.question(q)
    logo()
    team_scores()
    has_image = q.image is not None
    kicker = scene.text(kicker_text(q, number), size=0.26, color=GOLD if q.multiple else MUTED, weight=700)
    kicker.move_to(-0.6, 3.75)
    question = scene.text(q.text, size=0.56, color=WHITE, weight=700, wrap=12.0, text_align="center")
    question.move_to(-0.6, 3.0)
    timer = []
    if poll.is_quiz:
        ring = scene.geometry.annulus(0.75, 0.64).fill(SEA).no_stroke().move_to(6.9, 3.15)
        clock = scene.viz.readout(poll.remaining(), format=".0f", color=GOLD, font_size=0.6).move_to(6.6, 3.15)
        timer = [ring, clock]
    answered = scene.viz.readout(poll.total(), format=".0f", suffix=" answered", color=MUTED,
                                 font_size=0.26).move_to(0, -3.45)
    join = scene.text(f"Late? Join with {poll.code}", size=0.22, color=MUTED).move_to(0, -3.85)

    tiles = []
    for index in range(len(q.options)):
        if has_image:
            group = tile(poll, index, 7.4, 1.0, 0.42).move_to(-3.7, 1.55 - index * 1.2)
        else:
            group = tile(poll, index, 7.5, 1.9, 0.56).move_to(-3.95 + (index % 2) * 7.9,
                                                              0.95 - (index // 2) * 2.25)
        tiles.append(group)
    before = scene.geometry.group(tiles)
    picture = None
    if has_image:
        image = scene.media.image(q.image, width=6.6, height=3.9).move_to(3.95, -0.25)
        border = scene.geometry.rounded_rect(6.8, 4.1, 0.15).no_fill().stroke(WHITE, 0.06).move_to(3.95, -0.25)
        picture = scene.geometry.group([image, border])

    scene.play([kicker.animate.fade_in().sound(QUESTION, volume=0.7),
                question.animate.typewriter(cps=40, cursor=None)])
    entrance = [group.animate.grow_from_center().duration(0.4).delay(0.08 * index).settle(overshoot=0.08)
                .sound(TILE, volume=0.8) for index, group in enumerate(tiles)]
    if picture is not None:
        entrance.append(picture.animate.fade_in().duration(0.5))
    scene.play(entrance + [answered.animate.fade_in(), join.animate.fade_in()]
               + [piece.animate.fade_in() for piece in timer])

    if poll.is_quiz:
        # It moves on by itself once everyone answered or time is up.
        scene.stop("answering", until=poll.answered(share=1.0) | poll.time_up())
        poll.reveal()
    else:
        scene.stop("voting", until=poll.answered(share=1.0))

    # The answers shrink into a row at the top: magic move by name.
    row = [tile(poll, index, 3.5, 0.9, 0.3).move_to(COLUMNS[index], ROW_Y)
           for index in range(len(q.options))]
    after = scene.geometry.group(row)
    leaving = [answered.animate.fade_out(), join.animate.fade_out()]
    leaving += [piece.animate.fade_out() for piece in timer]
    if picture is not None:
        leaving.append(picture.animate.fade_out().duration(0.4))
    scene.play([magic_move(before, after, key="name").duration(0.8).sound(SHRINK)] + leaving)

    # Bars with the votes under each answer.
    reveal = []
    for index in range(len(q.options)):
        bar = poll.bar(index, length=2.6, thickness=0.14, radius=0.07, scale="total")
        bar.fill(WHITE).no_stroke().move_to(COLUMNS[index] - 0.35, ROW_Y - 0.7)
        votes = scene.viz.readout(poll.votes(index), format=".0f", color=WHITE, font_size=0.28)
        votes.move_to(COLUMNS[index] + 1.4, ROW_Y - 0.7)
        reveal += [bar.animate.create().duration(0.7), votes.animate.fade_in()]

    right = poll.correct if poll.is_quiz else None
    right = [] if right is None else ([right] if isinstance(right, int) else list(right))
    for index, group in enumerate(row):
        if right and index not in right:
            reveal.append(group.animate.opacity(0.25).duration(0.4))
        elif right:
            glow = group.animate.glow(poll.color(index), radius=0.4, intensity=1.3).duration(0.5)
            reveal.append(glow.sound(RIGHT) if index == right[0] else glow)
    scene.play(reveal)
    if right:
        # Confetti from every right answer, with a pop!
        bursts = []
        for k, index in enumerate(right):
            confetti = scene.fx.confetti((COLUMNS[index], ROW_Y + 0.3), count=0, seed=11 * number + index,
                                         speed=(4, 7), spread=0.8)
            bursts.append(confetti.animate.burst(70).duration(0.3).delay(0.12 * k).sound(POP, volume=0.8))
        scene.play(bursts + [row[index].animate.indicate().duration(0.6) for index in right])
    scene.live_zone(audience, gather_quiz if poll.is_quiz else gather_poll, size=SIZE, squash=0.03,
                    lean=0.05)
    if right:
        leader = board.name(0, size=0.26, weight=700).fill(GOLD).move_to(0, -3.75)
        lead_points = scene.viz.readout(board.points(0), format=".0f", prefix="leads with ",
                                        suffix=" pts", color=MUTED, font_size=0.22).move_to(0, -4.2)
        scene.play([leader.animate.fade_in().delay(1.2), lead_points.animate.fade_in().delay(1.2)])
    scene.wait(3)
    scene.stop("answer")


# --- Fact: the skyscraper race -----------------------------------------------------
# Architectural heights in meters, by the year each building was finished.
# A name missing from a year counts as 0 (the Twin Towers, from 2001).
# 2020 repeats 2015 so the last overtake finishes.
TOWERS = {
    1930: {"Chrysler": 319},
    1931: {"Chrysler": 319, "Empire State": 381},
    1972: {"Chrysler": 319, "Empire State": 381, "Twin Towers": 417},
    1974: {"Chrysler": 319, "Empire State": 381, "Twin Towers": 417, "Willis": 442},
    1998: {"Chrysler": 319, "Empire State": 381, "Twin Towers": 417, "Willis": 442, "Petronas": 452},
    2001: {"Chrysler": 319, "Empire State": 381, "Willis": 442, "Petronas": 452},
    2004: {"Empire State": 381, "Willis": 442, "Petronas": 452, "Taipei 101": 508},
    2010: {"Willis": 442, "Petronas": 452, "Taipei 101": 508, "Burj Khalifa": 828},
    2012: {"Petronas": 452, "Taipei 101": 508, "Abraj Al-Bait": 601, "Burj Khalifa": 828},
    2014: {"Taipei 101": 508, "One WTC": 541, "Abraj Al-Bait": 601, "Burj Khalifa": 828,
           "Petronas": 452},
    2015: {"Taipei 101": 508, "One WTC": 541, "Abraj Al-Bait": 601, "Shanghai Tower": 632,
           "Burj Khalifa": 828},
    2020: {"Taipei 101": 508, "One WTC": 541, "Abraj Al-Bait": 601, "Shanghai Tower": 632,
           "Burj Khalifa": 828},
}


def towers_slide():
    scene.segment("Skyscrapers", transition=Transition.magic_move(0.7, key="name", sound=SLIDE),
                  notes="How the height record kept being broken.")
    logo()
    team_scores()
    scene.text("How the record grew", size=0.6, color=WHITE, weight=700).move_to(0, 3.75)
    scene.text("the tallest buildings in the world, in meters", size=0.28, color=MUTED).move_to(0, 3.15)
    race = scene.viz.bar_race(TOWERS, top=5, rank_smoothing=0.5, value_format="{:,.0f} m", width=12.5,
                              height=5.6, colors=NEON + [GOLD, "#b48cff"], label_color=WHITE,
                              ticker_color=GOLD).move_to(0, -0.6)
    scene.play([race.animate.play().duration(10)])
    scene.media.sfx(FACT)
    scene.wait(1)
    scene.stop("skyscrapers")


number = 0
for q in questions:
    if q.is_quiz:
        number += 1
    scene.segment(q.text if len(q.text) < 40 else q.text[:37] + "…",
                  transition=Transition.magic_move(0.6, key="name", sound=SLIDE),
                  notes=q.notes or "Advance to reveal the answer.")
    question_slide(q, number)
    if q.text.startswith("Which is the tallest building"):
        towers_slide()


# --- Tug of war: the team ahead pulls --------------------------------------------------
PULL = 1.4  # how far the team ahead drags the rope


def tug(p):
    column, row = p.team_index % 5, p.team_index // 5
    x, y = side(p) * (1.6 + column * 0.95), -0.6 - (row % 3) * 1.25
    ahead = p.team_rank == 0
    shift = PULL * ease_out_back(min(p.time / 1.2, 1)) * (1 if ahead else -1)
    heave = 0.15 * math.sin(2 * math.pi * (p.t * 1.4 + p.team_index * 0.13))
    return pose(x + side(p) * (shift + heave), y + 0.6, lean=side(p) * 0.25, flip=side(p) > 0,
                express="winner" if ahead else "hurt", loop=True)


scene.segment("Tug of war", transition=Transition.magic_move(0.7, key="name", sound=SLIDE),
              notes="The team ahead pulls the rope.")
logo()
team_scores()
scene.text("Owls against Larks!", size=0.7, color=WHITE, weight=700).move_to(0, 3.6)
rope = scene.geometry.rect(15, 0.12).fill("#c89b6d").no_stroke().move_to(0, 0.4)
for team, x in [(0, -4.5), (1, 4.5)]:
    teams.bar(team, length=5.5, thickness=0.35, radius=0.17,
              direction="left" if team == 0 else "right").fill(teams.colors[team]).no_stroke().move_to(x, 2.5)
    scene.viz.readout(teams.average(team), format=".0f", suffix=" pts per player",
                      color=MUTED, font_size=0.28).move_to(x, 1.9)
scene.play([rope.animate.create().duration(0.6).sound(SLIDE)])
scene.live_zone(audience, tug, size=0.9, lean=0.0)
scene.wait(5)
scene.stop("tug")


# --- Race: the characters run along their scores -----------------------------------
LANES = 5
LANE_TOP, LANE_STEP = 2.3, 1.3
TRACK_START, TRACK_LENGTH = -5.2, 10.4


def lane_y(rank):
    return LANE_TOP - rank * LANE_STEP


def count_up(p):
    """The score each one shows climbs toward the real one (state kept between frames)."""
    return state(shown=p.state.shown + (p.score - p.state.shown) * min(1, 2.5 * STEP))


def race(p):
    # Changing lanes overshoots a little and settles.
    y = lerp(lane_y(p.previous_rank), lane_y(p.rank), ease_out_back(p.rank_since / 0.6))
    target = TRACK_START + TRACK_LENGTH * p.state.shown / max(p.leader, 1)
    x = lerp(TRACK_START - 1.2, target, spring(p.t, frequency=0.9, damping=0.45))
    stride = abs(math.sin(2 * math.pi * 1.6 * p.t))
    y += 0.16 * stride
    sx, sy = 1.06 - 0.06 * stride, 0.94 + 0.08 * stride
    mood = None
    if p.streak >= 3:
        mood = "winner"  # on a streak
    elif p.rank == 0:
        mood = "happy"
    return pose(x, y, sx=sx, sy=sy, express=mood, loop=True, visible=p.rank < LANES)


scene.segment("Race", transition=Transition.magic_move(0.7, key="name", sound=SLIDE), notes="The scores so far.")
logo()
team_scores()
scene.text("The race so far", size=0.6, color=WHITE, weight=700).move_to(0, 3.9)
for rank in range(LANES):
    y = lane_y(rank)
    scene.geometry.rect(TRACK_LENGTH + 0.6, 0.07).fill(SEA).no_stroke().move_to(
        TRACK_START + TRACK_LENGTH / 2, y - 0.04)
    board.name(rank, size=0.28, weight=700).fill(MUTED).move_to(-6.9, y + 0.35)
    scene.viz.readout(board.points(rank), format=".0f", color=GOLD, font_size=0.28).move_to(6.0, y + 0.35)
    scene.viz.readout(board.streak(rank), format=".0f", prefix="streak ", color=MUTED,
                      font_size=0.2).move_to(7.2, y + 0.35)
scene.live_zone(audience, race, size=0.85, squash=0.02, lean=0.06, state={"shown": 0.0},
                update=count_up)
scene.wait(5)
scene.stop("race")


# --- Podium: the winner jumps for joy and confetti rains -----------------------------
PLACES = ((0.0, 0.2), (-3.6, -0.8), (3.6, -1.4))
JUMP = 1.3


def podium(p):
    if p.rank == 0:
        x, y = PLACES[0]
        beat = p.t % JUMP
        crouch, air = 0.35, 0.55
        sx, sy = anticipate(beat, at=crouch, amount=0.3, duration=crouch)
        height = 0.0
        if crouch <= beat < crouch + air:
            u = (beat - crouch) / air
            height = 1.1 * 4 * u * (1 - u)
        elif beat >= crouch + air:
            sx, sy = impact(beat - crouch - air, amount=0.35)
        return pose(x, y + height, sx=sx, sy=sy, scale=1.4, express="winner", loop=True)
    if p.rank < len(PLACES):
        x, y = PLACES[p.rank]
        sway = 0.08 * math.sin(2 * math.pi * p.t / 1.3)
        return pose(x, y, scale=1.3, lean=sway, look_x=-x / abs(x), look_y=0.6)
    column = (p.rank - 3) % 8
    x = -6.3 + column * 1.8
    sway = 0.1 * math.sin(2 * math.pi * (p.t / 1.6 + column / 8))
    return pose(x, -4.0, scale=0.7, lean=sway, look_x=-x / 4, look_y=1)


scene.segment("Podium", transition=Transition.magic_move(0.7, key="name", sound=SLIDE),
              notes="Congratulate the top three and the winning team!")
logo()
heading = scene.text("Quiz champions!", size=0.7, color=WHITE, weight=700).move_to(0, 3.9)
medals = (GOLD, "#d7dbe8", "#e39b5f")
steps = []
for rank, ((x, y), height) in enumerate(zip(PLACES, (3.6, 2.6, 2.0))):
    block = scene.geometry.rect(2.8, height).fill(medals[rank]).no_stroke().move_to(x, y - height / 2)
    name = board.name(rank, size=0.36, weight=700).fill(INK).move_to(x, y - 0.5)
    points = scene.viz.readout(board.points(rank), format=".0f", suffix=" pts", color=INK,
                               font_size=0.3).move_to(x - 0.5, y - 1.0)
    steps.append(scene.geometry.group([block, name, points]).shift_by(0, -8))
scene.play([step.animate.shift_by(0, 8).duration(0.6).delay(0.2 * (2 - rank)).settle(overshoot=0.1)
            .sound(POP, volume=0.7) for rank, step in enumerate(steps)])
scene.live_zone(audience, podium, size=1.1)
confetti = scene.fx.confetti((0, -4.6), count=0, seed=21, spread=1.2, speed=(8, 12))
scene.play([confetti.animate.burst(110).duration(1.2).repeat(3).sound(POP, volume=0.6),
            heading.animate.indicate().duration(0.6).sound(PODIUM)])
scene.wait(3)
scene.stop("podium")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The rehearsal at the cover, the room, a poll, a quiz with a picture and
    # its reveal, the skyscraper race, the tug of war, the race and the podium.
    scene.snapshots(snapshots, [7.9, 19.6, 27.1, 50.5, 58.3, 100.5, 147.6, 152.6, 161.1])
else:
    scene.render()
