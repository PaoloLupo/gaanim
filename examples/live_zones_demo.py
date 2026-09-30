"""Live zones: the audience's characters play in the scene while presenting.

`gaanim --present live_zones_demo.py`, then join with a phone. Each zone's
behavior is a plain Python function of the player that returns a pose. It
is compiled when the scene is authored, so the exported `.gaanim` runs it
without Python. The world around the characters is ordinary scene content.

The behaviors use the principles of animation: characters crouch before
jumping (anticipation), fly in arcs, squash when they land and bounce back
(squash and stretch, follow-through), and ease in and out. The engine adds
a stretch along each character's velocity and a lean into its motion.
"""

import math

from gaanim import Scene
from gaanim.live import (
    anticipate,
    ballistic,
    ease_out_back,
    impact,
    landing,
    lerp,
    pose,
    progress,
    smoothstep,
    spring,
    wobble,
)

scene = Scene(frame=(16, 9), background="#0f1b2d")
scene.rehearsal(8)
audience = scene.audience()

# --- Zipline: joiners crouch, get launched at a sweeping angle, and land -------------
GRAVITY = 16.0
PLATFORM = (-7.0, 2.85)
SPEED = 8.0
CROUCH = 0.45  # seconds crouching before the launch
SHORE = 0.4  # floor to the left, water to the right
FLOOR, WATER = -3.0, -3.55


def launch_angle(joined):
    """The launcher sweeps between 10 and 75 degrees every 2.3 seconds."""
    sweep = 0.5 - 0.5 * math.cos(2 * math.pi * joined / 2.3)
    return math.radians(10 + 65 * sweep)


def zipline(p):
    angle = launch_angle(p.joined)
    vx, vy = SPEED * math.cos(angle), SPEED * math.sin(angle)
    x0, y0 = PLATFORM
    flight = landing(vy, y0 - FLOOR, gravity=GRAVITY)
    wet = x0 + vx * flight > SHORE
    if wet:
        flight = landing(vy, y0 - WATER, gravity=GRAVITY)
    land = CROUCH + flight
    ground_x = min(x0 + vx * flight, 7.4) + (p.random(1) - 0.5) * 0.5

    # Anticipation: crouch on the platform, then spring up into the jump.
    sx, sy = anticipate(p.t, at=CROUCH, amount=0.3, duration=CROUCH)
    if p.t < CROUCH:
        return pose(x0, y0, sx=sx, sy=sy, express="surprised")
    if p.t < land:
        x, y = ballistic(p.t - CROUCH, x0, y0, vx, vy, gravity=GRAVITY)
        # One somersault per flight, slow in and out, in an arc.
        spin = -2 * math.pi * smoothstep((p.t - CROUCH) / flight)
        return pose(x, y, rotation=spin, sx=sx, sy=sy)

    # Squash on impact and bounce back; a wobble follows through.
    sx, sy = impact(p.t - land, amount=0.45)
    tilt = wobble(p.t - land, amount=0.25, frequency=2.5, decay=5)
    if wet:
        sink = WATER - 0.35 * (1 - spring(p.t - land, frequency=1.2, damping=0.25))
        bob = 0.07 * math.sin(2 * math.pi * (p.t - land) / 1.6)
        return pose(ground_x, sink + bob, sx=sx, sy=sy, rotation=tilt, express="sad", since=land)
    return pose(ground_x, FLOOR, sx=sx, sy=sy, rotation=tilt, express="happy", since=land)


scene.segment("Sala", notes="The room fills while phones join.")
scene.geometry.rect(8.4, 1.5).fill("#5b8c3a").no_stroke().move_to(-3.8, -3.75)
scene.geometry.rect(8.2, 1.3).fill("#2d7fd3").no_stroke().move_to(4.5, -3.85)
scene.geometry.rect(0.5, 3.4).fill("#8a5a33").no_stroke().move_to(-7.2, 0.9)
scene.geometry.rect(1.6, 0.25).fill("#a86d3e").no_stroke().move_to(-7.0, 2.7)
scene.text("¡Entra y salta!", size=0.7, color="#f7f6ff", weight=800).move_to(2.5, 3.3)
scene.live_zone(audience, zipline, size=1.1, squash=0.03, lean=0.0)
scene.wait(8)
scene.stop("sala")

# --- Race: runners enter, bob as they run and react to overtaking --------------------
LANES = 5
LANE_TOP, LANE_STEP = 2.2, 1.25
TRACK_START, TRACK_LENGTH = -5.5, 10.0


def lane_y(rank):
    return LANE_TOP - rank * LANE_STEP - 0.45


def race(p):
    # Changing lanes overshoots a little and settles.
    y = lerp(lane_y(p.previous_rank), lane_y(p.rank), ease_out_back(p.rank_since / 0.6))
    score = lerp(p.previous_score, p.score, smoothstep(p.score_since / 0.8))
    target = TRACK_START + TRACK_LENGTH * score / max(p.leader, 1)
    # Run in from the start of the track, springing into place.
    x = lerp(TRACK_START - 1.5, target, spring(p.t, frequency=0.9, damping=0.45))
    # Running: a bouncing stride, squashing at each step.
    stride = abs(math.sin(2 * math.pi * 1.6 * p.t))
    y += 0.18 * stride
    sx, sy = 1.06 - 0.06 * stride, 0.94 + 0.08 * stride
    mood = None
    if p.rank != p.previous_rank and p.rank_since < 1.5:
        mood = "happy" if p.rank < p.previous_rank else "hurt"
        # Overtaking jumps; being overtaken stumbles.
        hop = math.sin(math.pi * progress(p.rank_since, 0, 0.45))
        y += 0.5 * hop if p.rank < p.previous_rank else -0.1 * hop
    elif p.rank == 0:
        mood = "winner"
    return pose(x, y, sx=sx, sy=sy, express=mood, loop=p.rank == 0, visible=p.rank < LANES)


scene.segment("Carrera", notes="Scores move the runners.")
for lane in range(LANES):
    scene.geometry.rect(TRACK_LENGTH + 1, 0.08).fill("#2a3a55").no_stroke().move_to(
        TRACK_START + TRACK_LENGTH / 2, lane_y(lane) - 0.05
    )
scene.live_zone(audience, race, size=0.9, squash=0.02, lean=0.06)
scene.wait(4)
scene.stop("carrera")

# --- Podium: the winner keeps jumping for joy -----------------------------------------
PLACES = ((0.0, 0.4), (-3.5, -0.6), (3.5, -1.2))
JUMP = 1.3  # seconds per jump: crouch, fly, land


def celebrate(t, x, y):
    """A jump every JUMP seconds: crouch, leap in an arc, squash on landing."""
    beat = t % JUMP
    crouch, air = 0.35, 0.55
    sx, sy = anticipate(beat, at=crouch, amount=0.3, duration=crouch)
    height = 0.0
    if crouch <= beat < crouch + air:
        u = (beat - crouch) / air
        height = 1.1 * 4 * u * (1 - u)
    elif beat >= crouch + air:
        sx, sy = impact(beat - crouch - air, amount=0.35)
    return pose(x, y + height, sx=sx, sy=sy, scale=1.3, express="winner", loop=True)


def podium(p):
    if p.rank == 0:
        x, y = PLACES[0]
        return celebrate(p.t, x, y)
    if p.rank < len(PLACES):
        x, y = PLACES[p.rank]
        # Runners-up sway, clapping along, and look up at the winner.
        sway = 0.08 * math.sin(2 * math.pi * p.t / 1.3)
        return pose(x, y, scale=1.3, lean=sway, look_x=-x / abs(x), look_y=0.6)
    column = (p.rank - 3) % 8
    x = -6.3 + column * 1.8
    sway = 0.1 * math.sin(2 * math.pi * (p.t / 1.6 + column / 8))
    return pose(x, -3.9, scale=0.7, lean=sway, look_x=-x / 4, look_y=1)


scene.segment("Podio", notes="Celebrate the top three.")
for (x, y), height, color in zip(PLACES, (3.6, 2.6, 2.0), ("#ffc933", "#d7dbe8", "#e39b5f")):
    scene.geometry.rect(2.6, height).fill(color).no_stroke().move_to(x, y - height / 2)
scene.live_zone(audience, podium, size=1.2)
scene.wait(4)
scene.stop("podio")

scene.render()
