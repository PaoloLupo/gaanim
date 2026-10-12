"""Connectors: points, followers, bindings and lines that keep up with moving objects.

Reference points (``point_ref``, ``offset_point``, ``point_between``,
``polar_point``, anchor points) are computed every frame from what they depend
on; ``follow`` and its relatives place one object on another; ``bind_*`` and
``look_at`` copy a coordinate or a turn; and lines, labels and frames track
the objects they join. Each panel or row is labelled with its code; the
followers are drawn in the accent color, what they follow in gray.
"""

import math
import os

from gaanim import Anchor, Easing, Scene, Zones, computed

RUN = 4.0
HOLD = 0.5

scene = Scene()
accent = scene.canvas.color("accent")
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")


def code(text):
    """A code label, its lines aligned left."""
    return scene.text(text, role="code", text_align="left")


def panels(rows, columns, head="0.9"):
    """A rows x columns grid of panels, each split into a label head and a stage."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows([float(head), "1fr"], gap="12px")) for zone in grid]


def turn(angle):
    """Turn ``angle`` once around, at a steady pace."""
    scene.play(angle.animate.set(math.tau).duration(RUN).easing(Easing.LINEAR))
    scene.wait(HOLD)


# A PointRef is a point that is never drawn: lines start and end on it and
# drawables follow it, with dot.follow(ref). Each is recomputed every frame
# from numbers, Parameters, computed values or other moving endpoints.
points_start = scene.cursor
scene.segment("points")
angle = scene.viz.parameter(0.0)
reveals = []
cells = panels(2, 3, head="1.2")
for (head, _), label in zip(cells, [
    "point_ref(x, y)",
    "offset_point(\n    body, 0, 0.7)",
    "point_between(a, b)",
    "point_between(a, b,\n    alpha=0.25,\n    offset=(0, 0.6))",
    "polar_point(center,\n    0.9, angle)",
    "box.anchor_point(\n    Anchor.RIGHT)",
]):
    code(label).place(head, anchor="center")
stages = [stage for _, stage in cells]


def sweep(stage, phase=0.0):
    """An x that crosses ``stage`` and back as angle turns."""
    left, width = stage.left + 0.4, stage.width - 0.8
    return computed(lambda a: left + width * (0.5 - 0.5 * math.cos(a + phase)), inputs=[angle])


def follower(ref, size=0.13):
    """An accent dot on ``ref``, revealed with the others."""
    dot = scene.geometry.dot(size).fill(accent).follow(ref)
    reveals.append(dot)
    return dot


# point_ref: both coordinates computed from angle.
(cx, cy), stage = stages[0].center, stages[0]
ref = scene.geometry.point_ref(sweep(stage), computed(lambda a: cy + 0.8 * math.sin(2 * a), inputs=[angle]))
reveals.append(scene.geometry.traced_path(follower(ref)).stroke(muted, 0.02))
# offset_point: a tag 0.6 above a moving body.
stage = stages[1]
body = scene.geometry.circle(0.3).fill(muted).move_to(sweep(stage), stage.center[1] - 0.4)
tag = scene.text("body", role="code").fill(accent).follow(scene.geometry.offset_point(body, 0, 0.7))
reveals.append(tag)
# point_between: the middle of two moving dots, and a point a quarter of the way, raised.
for index, options in [(2, {}), (3, {"alpha": 0.25, "offset": (0, 0.6)})]:
    stage = stages[index]
    a = scene.geometry.dot(0.16).fill(muted).move_to(stage.left + 0.4, stage.center[1] - 0.4)
    b = scene.geometry.dot(0.16).fill(muted).move_to(sweep(stage, math.pi), stage.center[1] - 0.4)
    reveals.append(scene.geometry.tracking_line(a, b).stroke(rule, 0.03))
    follower(scene.geometry.point_between(a, b, **options), 0.16)
# polar_point: a point at a radius and angle from a center.
center = stages[4].center
scene.geometry.circle(0.9).no_fill().stroke(rule, 0.03).move_to(*center)
tip = scene.geometry.polar_point(center, 0.9, angle)
reveals.append(scene.geometry.tracking_line(center, tip).stroke(muted, 0.03))
follower(tip, 0.16)
# anchor_point: a point on a drawable's box that turns and moves with it.
box = scene.geometry.rect(1.8, 1.0).fill(muted).move_to(*stages[5].center).rotate_to(angle)
follower(box.anchor_point(Anchor.RIGHT), 0.16)
scene.play([item.animate.fade_in() for item in reveals], duration=0.2)
turn(angle)

# follow places a drawable on any endpoint every frame, with an offset that
# stays level (world) or turns with the leader (local). attach_to sits on the
# leader's center and follow_to keeps a fixed offset from it.
follow_start = scene.cursor
scene.segment("follow")
grid = scene.layout.zones(Zones.grid(rows=4, columns=["7fr", "6fr"], column_gap="48px", padding="page_padding"))
followers = []
for row, (label, attach) in enumerate([
    ("ring.attach_to(leader)", lambda ring, leader: ring.attach_to(leader)),
    ("dot.follow(leader, offset=(0, 0.6))", lambda dot, leader: dot.follow(leader, offset=(0, 0.6))),
    ('dot.follow(leader, offset=(0.9, 0),\n           offset_space="local")',
     lambda dot, leader: dot.follow(leader, offset=(0.9, 0), offset_space="local")),
    ("dot.follow_to(leader, (0, -0.6))", lambda dot, leader: dot.follow_to(leader, (0, -0.6))),
]):
    code(label).place(grid[2 * row], anchor="right")
    stage = grid[2 * row + 1]
    (left, y), right = stage.point("left"), stage.right
    scene.geometry.line(left, y, right, y).stroke(rule, 0.03)
    leader = scene.geometry.square(0.5).fill(muted).move_to(left + 0.4, y)
    mark = scene.geometry.circle(0.45).no_fill().stroke(accent, 0.05) if row == 0 else scene.geometry.dot(0.14)
    attach(mark, leader)
    followers.append(mark.animate.fade_in())
    followers.append(leader.animate.move_to(right - 0.4, y).rotate_by(math.tau).duration(RUN))
scene.play(followers)
scene.wait(HOLD)

# Each accent shape calls the method in its label. bind_x_from and
# bind_y_from copy one coordinate of a source, wherever it is; look_at turns
# a drawable's x axis toward a target; bind_rotation_from copies a turn as
# source * ratio + phase. Each gray leader circles its stage.
bind_start = scene.cursor
scene.segment("bind")
angle = scene.viz.parameter(0.0)
reveals = []
cells = panels(2, 3, head="1.2")
for (head, _), label in zip(cells, [
    "bind_x_from(leader)",
    "bind_y_from(leader)",
    "look_at(leader)",
    "look_at(leader,\n    offset=math.pi)",
    "bind_rotation_from(\n    driver, ratio=-2)",
    "bind_rotation_from(\n    driver, ratio=0.5,\n    phase=math.pi / 2)",
]):
    code(label).place(head, anchor="center")
stages = [stage for _, stage in cells]


def circling(stage):
    """A gray leader that circles the middle of ``stage`` as angle turns."""
    radius = min(stage.width, stage.height) / 2 - 0.4
    scene.geometry.circle(radius).no_fill().stroke(rule, 0.03).move_to(*stage.center)
    leader = scene.geometry.dot(0.16).fill(muted).follow(scene.geometry.polar_point(stage.center, radius, angle))
    reveals.append(leader)
    return leader


stage = stages[0]
leader = circling(stage)
scene.geometry.line(stage.left, stage.bottom, stage.right, stage.bottom).stroke(rule, 0.03)
shadow = scene.geometry.dot(0.14).fill(accent).move_to(stage.center[0], stage.bottom)
shadow.bind_x_from(leader)
reveals.append(shadow)
stage = stages[1]
leader = circling(stage)
scene.geometry.line(stage.left + 0.2, stage.bottom, stage.left + 0.2, stage.top).stroke(rule, 0.03)
shadow = scene.geometry.dot(0.14).fill(accent).move_to(stage.left + 0.2, stage.center[1])
shadow.bind_y_from(leader)
reveals.append(shadow)
for stage, options in [(stages[2], {}), (stages[3], {"offset": math.pi})]:
    leader = circling(stage)
    needle = scene.geometry.polygon([(0.55, 0), (-0.35, 0.22), (-0.35, -0.22)]).move_to(*stage.center)
    needle.look_at(leader, **options)
for stage, options in [(stages[4], {"ratio": -2.0}), (stages[5], {"ratio": 0.5, "phase": math.pi / 2})]:
    (x, y) = stage.center
    driver = scene.geometry.rect(1.5, 0.22).fill(muted).move_to(x - 1.0, y).rotate_to(angle)
    bar = scene.geometry.rect(1.5, 0.22).move_to(x + 1.0, y)
    bar.bind_rotation_from(driver, **options)
    reveals.append(bar)
scene.play([item.animate.fade_in() for item in reveals], duration=0.2)
turn(angle)

# Lines, labels and frames that keep up: a tracking_line between two moving
# objects, a readout of their distance riding the point between them, a live
# frame around both, and a connector between anchor points of their boxes.
tracking_start = scene.cursor
scene.segment("tracking")
angle = scene.viz.parameter(0.0)
reveals = []
cells = panels(2, 2, head="1.4")
for (head, _), label in zip(cells, [
    "tracking_line(a, b)",
    "readout(gap).follow(\n    point_between(a, b,\n                  offset=(0, 0.5)))",
    "surrounding_rect([a, b])",
    "connector(\n    a.anchor_point(Anchor.RIGHT),\n    b.anchor_point(Anchor.LEFT))",
]):
    code(label).place(head, anchor="center")
for index, (_, stage) in enumerate(cells):
    (x, y) = stage.center
    ax = computed(lambda v, x=x: x - 1.6 - 0.8 * math.sin(v), inputs=[angle])
    by = computed(lambda v, y=y: y - 0.5 * math.sin(2 * v), inputs=[angle])
    a = scene.geometry.square(0.5).fill(muted).move_to(ax, y - 0.2)
    b = scene.geometry.circle(0.3).fill(muted).move_to(x + 1.8, by)
    if index == 0:
        reveals.append(scene.geometry.tracking_line(a, b).stroke(accent, 0.05))
    elif index == 1:
        reveals.append(scene.geometry.tracking_line(a, b).stroke(rule, 0.03))
        gap = computed(lambda v, by, x=x, y=y: math.hypot(x + 1.8 - v, by - y + 0.2), inputs=[ax, by])
        readout = scene.viz.readout(gap, label="$d$", font_size=0.6, color=accent)
        reveals.append(readout.follow(scene.geometry.point_between(a, b, offset=(0, 0.5))))
    elif index == 2:
        reveals.append(scene.geometry.surrounding_rect([a, b], padding=0.2).stroke(accent, 0.04))
    else:
        reveals.append(scene.geometry.connector(a.anchor_point(Anchor.RIGHT), b.anchor_point(Anchor.LEFT)).fill(accent))
scene.play([item.animate.fade_in() for item in reveals], duration=0.2)
turn(angle)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment early, midway and late in its motion.
    scene.snapshots(
        snapshots,
        [points_start + at for at in (0.9, 1.6, 3.4)]
        + [follow_start + at for at in (0.8, 2.0, 3.4)]
        + [bind_start + at for at in (0.9, 2.2, 3.4)]
        + [tracking_start + at for at in (0.9, 2.2, 3.4)],
    )
else:
    scene.render()
