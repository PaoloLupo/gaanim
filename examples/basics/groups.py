"""Groups: several objects that move, turn and animate as one, nested and lined up.

``scene.geometry.group`` gathers drawables into one: the group transforms about
the center of its box, each member keeps its own handle, and a group can hold
other groups, each turning about its own pivot. Each segment covers one part,
and each label is the code that animates or builds its row.
"""

import math
import os

from gaanim import Scene, Zones, parallel

DURATION = 1.4
HOLD = 0.6

scene = Scene()
foreground = scene.canvas.color("foreground")
rule = scene.canvas.color("rule")
panel = scene.canvas.color("panel")
danger = scene.canvas.color("danger")
success = scene.canvas.color("success")


def house(size=1.3):
    """A house grouped from a wall, a roof and a door; returns (house, roof, door)."""
    wall = scene.geometry.rect(size, 0.7 * size).move_to(0, -0.225 * size)
    roof = scene.geometry.polygon([(-0.6 * size, 0.125 * size), (0.6 * size, 0.125 * size), (0, 0.575 * size)])
    roof.fill(danger)
    door = scene.geometry.rect(0.24 * size, 0.4 * size).fill(panel).move_to(0.15 * size, -0.375 * size)
    return scene.geometry.group([wall, roof, door]), roof, door


def table(rows, tracks=None, label_width=8.0):
    """One row per (label, build) pair: the label, then the stage where ``build(stage)``
    draws the row and returns its animation."""
    grid = scene.layout.zones(
        Zones.grid(rows=tracks or len(rows), columns=[label_width, "1fr"], gap="24px", column_gap="64px",
                   padding="page_padding")
    )
    moves = []
    for index, (label, build) in enumerate(rows):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        moves.append(build(grid[2 * index + 1]))
    scene.play(moves, duration=DURATION)
    scene.wait(HOLD)


def centered(zone, size=1.3):
    """A house in the middle of ``zone``; returns (house, roof, door)."""
    parts = house(size)
    parts[0].move_to(*zone.center)
    return parts


# A group moves, turns, scales and shears as one, about the center of its box.
scene.segment("as_one")
table([
    ("house.animate.shift_by(3, 0)",
     lambda stage: house()[0].move_to(stage.left + 1.3, stage.center[1]).animate.shift_by(3, 0)),
    ("house.animate.rotate_by(math.pi / 4)", lambda stage: centered(stage)[0].animate.rotate_by(math.pi / 4)),
    ("house.animate.scale_to(1.3)", lambda stage: centered(stage)[0].animate.scale_to(1.3)),
    ("house.animate.skew_to(0.5, 0)", lambda stage: centered(stage)[0].animate.skew_to(0.5, 0)),
])

# Members keep their handles: animate the group, or one member inside it.
# A group's create() draws its members one after another; lag_ratio sets how
# much they overlap, from 0 (all together) to 1.
scene.segment("members")
table([
    ("house.animate.create()", lambda stage: centered(stage, 1.0)[0].animate.create()),
    ("house.animate.create().lag_ratio(0)", lambda stage: centered(stage, 1.0)[0].animate.create().lag_ratio(0)),
    ("house.animate.opacity(0.25)", lambda stage: centered(stage, 1.0)[0].animate.opacity(0.25)),
    ("roof.animate.shift_by(0, 0.4)", lambda stage: centered(stage, 1.0)[1].animate.shift_by(0, 0.4)),
    ("door.animate.fill(success)", lambda stage: centered(stage, 1.0)[2].animate.fill(success)),
])


def robot_arm(stage):
    """An arm holding a forearm holding a hand, each group turning about its own joint.

    Returns (arm, forearm, hand). The base under the shoulder is not grouped.
    """
    shoulder = (stage.left + 0.6, stage.bottom + 0.4)
    elbow = (shoulder[0] + 1.9, shoulder[1])
    wrist = (elbow[0] + 1.5, elbow[1])
    scene.geometry.rect(0.9, 0.3).fill(rule).move_to(shoulder[0], shoulder[1] - 0.25)
    upper = scene.geometry.rounded_rect(1.9, 0.3, 0.15).move_to(shoulder[0] + 0.95, shoulder[1])
    lower = scene.geometry.rounded_rect(1.5, 0.26, 0.13).move_to(elbow[0] + 0.75, elbow[1])
    wx, wy = wrist
    claws = [
        scene.geometry.polyline([(wx, wy), (wx + 0.3, wy + sign * 0.28), (wx + 0.6, wy + sign * 0.12)])
        .stroke(foreground, 0.1)
        for sign in (1, -1)
    ]
    joints = [scene.geometry.circle(0.17).fill(foreground).move_to(*point) for point in (shoulder, elbow, wrist)]
    hand = scene.geometry.group([*claws, joints[2]]).with_pivot(*wrist)
    forearm = scene.geometry.group([lower, joints[1], hand]).with_pivot(*elbow)
    arm = scene.geometry.group([upper, joints[0], forearm]).with_pivot(*shoulder)
    return arm, forearm, hand


def all_three(stage):
    arm, forearm, hand = robot_arm(stage)
    return parallel(arm.animate.rotate_by(0.2), forearm.animate.rotate_by(0.5), hand.animate.rotate_by(0.8))


# Groups nest: turning the arm carries the forearm and the hand along, while
# each inner group still turns about its own joint, set with with_pivot.
scene.segment("nesting")
table(
    [
        ("arm.animate.rotate_by(0.3)", lambda stage: robot_arm(stage)[0].animate.rotate_by(0.3)),
        ("forearm.animate.rotate_by(0.8)", lambda stage: robot_arm(stage)[1].animate.rotate_by(0.8)),
        ("hand.animate.rotate_by(1.2)", lambda stage: robot_arm(stage)[2].animate.rotate_by(1.2)),
        ("parallel(arm.animate.rotate_by(0.2),\n"
         "         forearm.animate.rotate_by(0.5),\n"
         "         hand.animate.rotate_by(0.8))", all_three),
    ],
    tracks=["2fr", "2fr", "2fr", "3fr"],
    label_width=8.8,
)


def lines(rows):
    """Each (label, build) pair gets its label above a wide stage, where
    ``build(zone)`` lines up houses and returns them as one group."""
    tracks = []
    for _ in rows:
        tracks += [0.45, "1fr"]
    grid = scene.layout.zones(Zones.rows(tracks, gap="12px", padding="page_padding"))
    groups = []
    for index, (label, build) in enumerate(rows):
        # markup=False keeps the * of *houses literal.
        scene.text(label, role="code", markup=False).place(grid[2 * index], anchor="left")
        groups.append(build(grid[2 * index + 1]))
    scene.play([group.animate.create().lag_ratio(0.5) for group in groups], duration=DURATION)
    scene.wait(HOLD)


def arranged(**options):
    """Five houses lined up once by ``zone.arrange``, then grouped."""
    def build(zone):
        houses = [house(1.0)[0] for _ in range(5)]
        zone.arrange(*houses, **options)
        return scene.geometry.group(houses)
    return build


def repeated(zone):
    first = house(1.0)[0].move_to(zone.left + 0.7, zone.center[1])
    return scene.geometry.repeat(first, 6, offset=(1.6, 0))


# Lining members up: zone.arrange places free objects once (Layout covers
# rows, grids and zones), and repeat() builds a group of shifted copies.
scene.segment("arrange")
lines([
    ("zone.arrange(*houses)", arranged()),
    ('zone.arrange(*houses, gap="120px")', arranged(gap="120px")),
    ('zone.arrange(*houses, justify="between")', arranged(justify="between")),
    ("scene.geometry.repeat(house, 6, offset=(1.6, 0))", repeated),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-animation and settled, in each of the four segments.
    scene.snapshots(snapshots, [(DURATION + HOLD) * index + t for index in range(4) for t in (0.6, 1.8)])
else:
    scene.render()
