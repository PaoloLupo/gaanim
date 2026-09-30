"""Behaviors the differential test compiles and runs in Python and Rust.

They are the examples the live zone was designed for (a zipline, a podium,
a bar race) plus corners where Python's numbers differ from Rust's.
"""

import math

from gaanim.live import pose

GRAVITY = 20.0
START = (-7.0, 3.0)
WATER = 1.0
PODIUM = ((0.0, 1.0), (-2.5, 0.2), (2.5, -0.4))
LANES = 5


def smoothstep(u):
    u = min(max(u, 0.0), 1.0)
    return u * u * (3 - 2 * u)


def lerp(a, b, u):
    return a + (b - a) * u


def flight(p):
    """Launch angle, speed and the time to land, from the player."""
    angle = math.radians(20 + 40 * p.random(0))
    vx, vy = 7 * math.cos(angle), 7 * math.sin(angle)
    fall = START[1] + 3
    land = (vy + math.sqrt(vy * vy + 2 * GRAVITY * fall)) / GRAVITY
    return vx, vy, land


def zipline(p):
    vx, vy, land = flight(p)
    if p.t < land:
        t = p.t
        return pose(
            START[0] + vx * t,
            START[1] + vy * t - GRAVITY * t * t / 2,
            rotation=-t * 2 * math.pi,
        )
    x = START[0] + vx * land
    wet = x > WATER
    return pose(x, -3.3 if wet else -3, express="sad" if wet else "happy", since=land)


def podium(p):
    if p.rank < len(PODIUM):
        x, y = PODIUM[p.rank]
        return pose(x, y, express="winner" if p.rank == 0 else None, loop=True)
    column = (p.rank - 3) % 6
    row = (p.rank - 3) // 6
    return pose(-6 + column * 1.1, -3 - row * 0.4, scale=0.7)


def race(p):
    lane = lambda_free_lane(p.rank)
    before = lambda_free_lane(p.previous_rank)
    glide = smoothstep(p.rank_since / 0.6)
    y = lerp(before, lane, glide)
    grown = lerp(p.previous_score, p.score, smoothstep(p.score_since / 0.8))
    x = -6 + 10 * grown / max(p.leader, 1)
    mood = None
    if p.rank_since < 1.5 and p.rank != p.previous_rank:
        mood = "happy" if p.rank < p.previous_rank else "sad"
    elif p.rank == 0:
        mood = "winner"
    return pose(x, y, express=mood, visible=p.rank < LANES, loop=p.rank == 0)


def lambda_free_lane(rank):
    return 2 - rank * 1.1


def corners(p):
    t = p.t - 3
    a = t % 1.5 + (-t) // 0.7 + round(t * 2.5) + round(-2.5) + round(0.5)
    b = max(t, math.nan) + min(math.nan, t) + math.copysign(1, -0.0)
    c = math.fmod(-7.5, 2) + (-7.5) % 2 + 7 // -2 + math.log(p.score + 1, 2)
    d = math.atan2(-t, 0.5) + math.hypot(t, 2) + abs(t) ** 0.5 + int(-2.7) + float(True)
    e = (p.index and 3) + (0 or p.t) + (not p.rank) + (1 < p.t <= 3)
    f = 0
    for i in range(1, 5):
        f += i * p.random(i)
    g = PODIUM[p.index % 3][1] + (1 if p.rank in (0, 2, 4) else -1)
    h = math.exp(-t) + math.tanh(t) + math.log10(p.index + 1) + math.floor(-0.5)
    return pose(a + b + c, d + e, rotation=f + g, scale=h, flip=p.index % 2 == 1)


BEHAVIORS = [zipline, podium, race, corners]
