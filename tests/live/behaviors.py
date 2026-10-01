"""Behaviors the differential test compiles and runs in Python and Rust.

They are the examples the live zone was designed for (a zipline, a podium,
a bar race) plus corners where Python's numbers differ from Rust's.
"""

import math

from gaanim.live import (
    anticipate,
    ballistic,
    ease_in_out,
    ease_out_back,
    impact,
    landing,
    pose,
    spring,
    state,
    wobble,
)

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


def animated(p):
    """The helpers: anticipation, an arc, an impact and springs."""
    launch = 0.4
    vx, vy = 5 + 2 * p.random(3), 6 + 3 * p.random(4)
    land = launch + landing(vy, 2.0, gravity=GRAVITY)
    sx, sy = anticipate(p.t, at=launch)
    if p.t < launch:
        return pose(-6, 1, sx=sx, sy=sy)
    if p.t < land:
        x, y = ballistic(p.t - launch, -6, 1, vx, vy, gravity=GRAVITY)
        return pose(x, y, sx=sx, sy=sy, rotation=wobble(p.t - launch, amount=0.4))
    x = -6 + vx * (land - launch)
    sx, sy = impact(p.t - land, amount=0.4)
    bounce = spring(p.rank_since, frequency=3) * ease_out_back(p.score_since) + ease_in_out(p.t / 9)
    return pose(
        x,
        -1 + 0.1 * bounce,
        sx=sx,
        sy=sy,
        lean=-0.1 * spring(p.t - land, damping=0.2),
        look_x=-1 if p.rank else 0.5,
        show_name=p.t > land,
        look_y=0.3,
    )


def battle(p):
    """Two teams face each other; the leading one jumps."""
    side = -1 if p.team == 0 else 1
    row = p.team_index % 4
    x = side * (2 + p.team_index // 4 * 1.2)
    y = -2 + row * 1.1
    if p.team_rank == 0:
        y += 0.5 * abs(math.sin(3 * p.t))
    push = 0.3 * math.tanh((p.team_score - 500) / 200)
    return pose(x + side * push, y, flip=side > 0, express="winner" if p.team_rank == 0 else None,
                loop=True)


# A zone that keeps numbers per player (state=, update=).
STEP_SECONDS = 1 / 60
KEPT = ("energy", "hops", "best")


def charge(p):
    """Energy fills while the player leads and drains otherwise; a full
    charge counts a hop and empties it."""
    gain = 0.8 if p.rank == 0 else -0.3
    energy = min(max(p.state.energy + gain * STEP_SECONDS, 0), 1)
    if energy >= 1:
        return state(energy=0, hops=p.state.hops + 1, best=max(p.state.best, p.score))
    return state(energy=energy)


def hopper(p):
    lift = p.state.energy * 2 + 0.3 * p.state.hops
    return pose(-6 + p.index, -2 + lift, scale=1 + 0.1 * min(p.state.hops, 5),
                express="winner" if p.state.best > 900 else None)


BEHAVIORS = [zipline, podium, race, corners, animated, battle]
#: Behaviors and updates of a zone that keeps KEPT.
KEPT_BEHAVIORS = [hopper]
KEPT_UPDATES = [charge]
