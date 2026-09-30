"""Live zones: the audience's characters play in the scene while presenting.

`gaanim --present live_zones_demo.py`, then join with a phone. Each zone's
behavior is a plain Python function of the player that returns a pose. It
is compiled when the scene is authored, so the exported `.gaanim` runs it
without Python. The world around the characters is ordinary scene content.
"""

import math

from gaanim import Scene
from gaanim.live import pose

scene = Scene(frame=(16, 9), background="#0f1b2d")
audience = scene.audience(preview=["Ana", "Beto", "Caro", "Dani", "Eli", "Fede", "Gabi", "Hugo"])

# --- Zipline: joiners are launched at a sweeping angle -------------------------
GRAVITY = 16.0
LAUNCH = (-6.6, 3.2)
SPEED = 8.0
SHORE = 0.4  # floor to the left, water to the right
FLOOR, WATER = -3.0, -3.55


def launch_angle(joined):
    """The launcher sweeps between 10 and 75 degrees every 2.3 seconds."""
    sweep = 0.5 - 0.5 * math.cos(2 * math.pi * joined / 2.3)
    return math.radians(10 + 65 * sweep)


def time_to_fall(vy, drop):
    return (vy + math.sqrt(vy * vy + 2 * GRAVITY * drop)) / GRAVITY


def zipline(p):
    angle = launch_angle(p.joined)
    vx, vy = SPEED * math.cos(angle), SPEED * math.sin(angle)
    land = time_to_fall(vy, LAUNCH[1] - FLOOR)
    x = LAUNCH[0] + vx * land
    wet = x > SHORE
    if wet:
        land = time_to_fall(vy, LAUNCH[1] - WATER)
        x = LAUNCH[0] + vx * land
    x = min(x, 7.4) + (p.random(1) - 0.5) * 0.5
    if p.t < land:
        t = p.t
        return pose(
            LAUNCH[0] + vx * t,
            LAUNCH[1] + vy * t - GRAVITY * t * t / 2,
            rotation=-2 * math.pi * 1.3 * t,
        )
    if wet:
        bob = 0.07 * math.sin(2 * math.pi * (p.t - land) / 1.6)
        return pose(x, WATER + bob, express="sad", since=land)
    return pose(x, FLOOR, express="happy", since=land)


scene.segment("Sala", notes="The room fills while phones join.")
scene.geometry.rect(8.4, 1.5).fill("#5b8c3a").no_stroke().move_to(-3.8, -3.75)
scene.geometry.rect(8.2, 1.3).fill("#2d7fd3").no_stroke().move_to(4.5, -3.85)
scene.geometry.rect(0.5, 3.4).fill("#8a5a33").no_stroke().move_to(-7.2, 0.9)
scene.geometry.rect(1.6, 0.25).fill("#a86d3e").no_stroke().move_to(-7.0, 2.7)
scene.text("¡Entra y salta!", size=0.7, color="#f7f6ff", weight=800).move_to(2.5, 3.3)
scene.live_zone(audience, zipline, size=1.1, preview_every=0.45)
scene.wait(8)
scene.stop("sala")

# --- Race: characters ride their bars and react to overtaking --------------------
LANES = 5
LANE_TOP, LANE_STEP = 2.2, 1.25
TRACK_START, TRACK_LENGTH = -5.5, 10.0


def smoothstep(u):
    u = min(max(u, 0.0), 1.0)
    return u * u * (3 - 2 * u)


def lane_y(rank):
    return LANE_TOP - rank * LANE_STEP


def race(p):
    y = lane_y(p.previous_rank) + (lane_y(p.rank) - lane_y(p.previous_rank)) * smoothstep(
        p.rank_since / 0.6
    )
    score = p.previous_score + (p.score - p.previous_score) * smoothstep(p.score_since / 0.8)
    x = TRACK_START + TRACK_LENGTH * score / max(p.leader, 1)
    mood = None
    if p.rank != p.previous_rank and p.rank_since < 1.5:
        mood = "happy" if p.rank < p.previous_rank else "hurt"
    elif p.rank == 0:
        mood = "winner"
    return pose(x, y - 0.45, express=mood, loop=p.rank == 0, visible=p.rank < LANES)


scene.segment("Carrera", notes="Scores move the runners.")
for lane in range(LANES):
    y = lane_y(lane) - 0.5
    scene.geometry.rect(TRACK_LENGTH + 1, 0.08).fill("#2a3a55").no_stroke().move_to(
        TRACK_START + TRACK_LENGTH / 2, y
    )
scene.live_zone(audience, race, size=0.9, preview_every=0.2)
scene.wait(4)
scene.stop("carrera")

# --- Podium: the winner celebrates in a loop -----------------------------------------
PLACES = ((0.0, 0.4), (-3.5, -0.6), (3.5, -1.2))


def podium(p):
    if p.rank < len(PLACES):
        x, y = PLACES[p.rank]
        return pose(x, y, scale=1.3, express="winner" if p.rank == 0 else None, loop=True)
    column = (p.rank - 3) % 8
    return pose(-6.3 + column * 1.8, -3.9, scale=0.7, flip=column % 2 == 1)


scene.segment("Podio", notes="Celebrate the top three.")
for (x, y), height, color in zip(PLACES, (3.6, 2.6, 2.0), ("#ffc933", "#d7dbe8", "#e39b5f")):
    scene.geometry.rect(2.6, height).fill(color).no_stroke().move_to(x, y - height / 2)
scene.live_zone(audience, podium, size=1.2, preview_every=0.1)
scene.wait(4)
scene.stop("podio")

scene.render()
