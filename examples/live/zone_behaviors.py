"""Zone behaviors: the motion helpers of gaanim.live and numbers a zone keeps per player.

A live zone's behavior is a function of the player's time, ``p.t``, so
``gaanim.live`` brings the pieces to animate with: curves from 0 to 1,
springs and wobbles, squash on landing and a crouch before a jump, throws
in an arc, and a random number of each player's own. For memory between
frames, ``state`` and ``update`` keep up to eight numbers per player. Each
label is the code that moves its character.
"""

import math
import os

from gaanim import Scene, Zones
from gaanim.live import (
    STEP,
    anticipate,
    ballistic,
    ease_in,
    ease_in_out,
    ease_out,
    ease_out_back,
    impact,
    landing,
    lerp,
    pose,
    progress,
    smoothstep,
    spring,
    state,
    wobble,
)

scene = Scene()
rule = scene.canvas.color("rule")
track = scene.canvas.color("header")
scene.rehearsal(10, seed=6, arrive=0)
audience = scene.audience()
seeks = []

CROSS = 1.6  # seconds to cross a lane
PERIOD = 2.0  # seconds per loop of the physics cells


def code(label, zone, anchor="left"):
    """A variant label: the code that builds it."""
    return scene.text(label, role="code", markup=False).place(zone, anchor=anchor)


def outline(zone):
    """A rule-colored frame around ``zone``."""
    return scene.geometry.rect(zone.width, zone.height).no_fill().stroke(rule, 0.02).place(zone)


# Curves from 0 to 1, applied to u = progress(p.t, 0.2, 1.6): one runner per
# lane, picked by p.index, crosses in the same time with another curve.
scene.segment("curves")
CURVES = ["lerp(x0, x1, u)", "lerp(x0, x1, smoothstep(u))", "lerp(x0, x1, ease_in(u))",
          "lerp(x0, x1, ease_out(u))", "lerp(x0, x1, ease_in_out(u))", "lerp(x0, x1, ease_out_back(u))"]
grid = scene.layout.zones(Zones.grid(rows=len(CURVES), columns=["7fr", "8fr"], column_gap="48px",
                                     padding="page_padding"))
for row, label in enumerate(CURVES):
    code(label, grid[2 * row], anchor="right")
    lane = grid[2 * row + 1]
    scene.geometry.rect(lane.width, 0.03).fill(rule).no_stroke().place(lane, anchor="bottom", offset=(0, "12px"))
# Module constants are frozen into the behavior when the zone compiles it.
# A tenth of each lane stays free at both ends, room for ease_out_back to overshoot.
X0, X1 = grid[1].left + grid[1].width * 0.1, grid[1].right - grid[1].width * 0.1
FLOORS = tuple(grid[2 * row + 1].bottom + 0.12 for row in range(len(CURVES)))


def race(p):
    u = progress(p.t, 0.2, CROSS)
    lane = p.index % 6
    if lane == 0:
        x = lerp(X0, X1, u)
    elif lane == 1:
        x = lerp(X0, X1, smoothstep(u))
    elif lane == 2:
        x = lerp(X0, X1, ease_in(u))
    elif lane == 3:
        x = lerp(X0, X1, ease_out(u))
    elif lane == 4:
        x = lerp(X0, X1, ease_in_out(u))
    else:
        x = lerp(X0, X1, ease_out_back(u))
    return pose(x, FLOORS[lane], visible=p.index < 6, look_x=1, look_y=0)


scene.live_zone(audience, race, size=1.2, squash=0, lean=0)
start = scene.cursor
scene.wait(CROSS + 0.6)
seeks.extend([start + 0.7, start + 1.1, scene.cursor - 0.05])

# Physical motion, looping every two seconds: a spring that overshoots, a
# wobble that dies out, squash on landing, a crouch before a jump, a throw
# in an arc, and p.random(k), a number of each player's own.
scene.segment("physics")
PHYSICS = ["spring(t, 0.8, 0.15)", "wobble(t, 0.8, 1, 1)", "impact(t - land)",
           "anticipate(t, at=0.8)", "ballistic(t, x, y, vx, vy)", "p.random(k)"]
cells = scene.layout.zones(Zones.grid(rows=3, columns=2, gap="24px", padding="page_padding"))
for label, cell in zip(PHYSICS, cells):
    outline(cell)
    scene.text(label, role="code", markup=False).place(cell, anchor="top_left", padding="16px")
BOXES = tuple((cell.left, cell.bottom, cell.right, cell.top) for cell in cells)


def physics(p):
    t = p.t % PERIOD
    cell = min(5, p.index)
    left, bottom, right, top = BOXES[cell]
    middle, floor = (left + right) / 2, bottom + 0.15
    if cell == 0:
        # Released from the left, it overshoots the middle and settles.
        return pose(lerp(left + 0.8, right - 1.6, spring(t, 0.8, 0.15)), floor)
    if cell == 1:
        # Knocked at t = 0, it rocks and comes to rest.
        return pose(middle, floor, rotation=wobble(t, 0.8, 1, 1), express="hurt")
    if cell == 2:
        # Falls for 0.85 s, then squashes on the floor and bounces back.
        land = 0.85
        sx, sy = impact(t - land, amount=0.45)
        y = lerp(top - 1.0, floor, ease_in(min(t / land, 1)))
        return pose(middle, y, sx=sx, sy=sy)
    if cell == 3:
        # Crouches before leaping at 0.8 s, and stretches as it leaves.
        sx, sy = anticipate(t, at=0.8, amount=0.35, duration=0.5)
        jump = max(0, t - 0.8)
        height = max(0, 4.5 * jump - 4.5 * jump * jump / 0.9)
        return pose(middle, floor + height, sx=sx, sy=sy)
    if cell == 4:
        # Thrown up and right; landing() says when it is back on the floor.
        vx, vy = 2.4, 4.5
        air = landing(vy, 0, gravity=9.8)
        x, y = ballistic(min(t, air), middle - 1.1, floor, vx, vy, gravity=9.8)
        return pose(x, y, rotation=-2 * math.pi * min(t / air, 1))
    # Each player its own spot and pace from p.random(k).
    x = lerp(left + 0.6, right - 0.6, p.random(1))
    hop = 0.4 * abs(math.sin(math.pi * (t / PERIOD * (1 + 2 * p.random(2)))))
    return pose(x, floor + hop, scale=0.7 + 0.3 * p.random(3))


scene.live_zone(audience, physics, size=1.4, squash=0, lean=0)
start = scene.cursor
scene.wait(PERIOD)
seeks.extend([start + 0.3, start + 0.9, start + 1.5])

# state gives each player numbers that last from one frame to the next, and
# update(p) computes the next ones every STEP (1/60 s). Here energy charges
# at each player's own pace and every full charge climbs one step.
scene.segment("state")
STEPS = 5
page = scene.layout.zones(Zones.rows(["7fr", "1fr"], gap="24px", padding="page_padding"))
stairs = page[0].split(Zones.columns(5, gap="48px"))
RISE = stairs[0].height / (STEPS + 1)
for stair in stairs:
    width = stair.width / (STEPS + 1)
    for step in range(STEPS + 1):
        height = RISE * (step + 0.2)
        block = scene.geometry.rect(width, height).fill(track).no_stroke()
        block.move_to(stair.left + width * (step + 0.5), stair.bottom + height / 2)
code('state={"energy": 0.0, "steps": 0}, update=charge', page[1], anchor="center")
STARTS = tuple((stair.left, stair.bottom) for stair in stairs)
WIDTH = stairs[0].width / (STEPS + 1)


def charge(p):
    energy = p.state.energy + (0.9 + 1.2 * p.random(4)) * STEP
    if energy >= 1 and p.state.steps < STEPS:
        return state(energy=0, steps=p.state.steps + 1)
    return state(energy=min(energy, 1))


def climb(p):
    left, bottom = STARTS[p.index % 5]
    steps = p.state.steps
    # The fuller its energy, the lower it crouches before the next step.
    crouch = 0.35 * p.state.energy if steps < STEPS else 0
    return pose(left + WIDTH * (steps + 0.5), bottom + RISE * (steps + 0.2), sx=1 + crouch * 0.6, sy=1 - crouch,
                visible=p.index < 5, express="winner" if steps == STEPS else None, loop=True)


scene.live_zone(audience, climb, size=1.2, state={"energy": 0.0, "steps": 0}, update=charge)
start = scene.cursor
scene.wait(4.0)
seeks.extend([start + 1.0, start + 2.5, scene.cursor - 0.05])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
