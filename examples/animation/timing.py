"""Timing: repeats and loops, parallel, sequence and stagger, and reshaping a composition's time.

Each row is labelled with the code that times it. The first segment repeats
one move, the second composes five growing bars, the third reshapes the time of
one sequence, and the fourth staggers a grid of dots from every origin.
"""

import os

from gaanim import Direction, Easing, Scene, Zones, distribute, parallel, sequence, stagger

CYCLE = 0.6
GROW = 0.3
HOLD = 0.5

scene = Scene()
rule = scene.canvas.color("rule")


def rows(count, label_share="4fr", stage_share="3fr"):
    """``count`` rows of a label column and a stage column, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=[label_share, stage_share], column_gap="48px", padding="page_padding")
    )


def track(zone):
    """Draw a track across ``zone`` with a tick every third, and return its dot and step."""
    (start, y), (end, _) = zone.point("left"), zone.point("right")
    scene.geometry.line(start, y, end, y).stroke(rule, 0.02)
    step = (end - start) / 3
    for index in range(4):
        scene.geometry.rect(0.02, 0.24).fill(rule).move_to(start + index * step, y)
    return scene.geometry.dot(0.12).move_to(start, y), step


# One cycle is a move a third of the way along the track, which delay holds
# back. repeat plays it a number of times and loop as many times as fit. Each
# cycle restarts the move, except that yoyo and pingpong alternate its
# direction, and an offset loop or a repeated Composition continue from
# where the previous cycle ended.
repeat_start = scene.cursor
scene.segment("repeat")
grid = rows(10, "5fr", "3fr")
scene.text("move = dot.animate.shift_by(step, 0)", role="code").place(grid[0], anchor="right")
cycles = [
    ("move.delay(0.6)", lambda move: move.delay(0.6)),
    ("move.repeat(3)", lambda move: move.repeat(3)),
    ("move.repeat(3, delay=0.3)", lambda move: move.repeat(3, delay=0.3)),
    ("move.repeat(4, yoyo=True)", lambda move: move.repeat(4, yoyo=True)),
    ('move.loop("cycle", until=2.4)', lambda move: move.loop("cycle", until=2.4)),
    ('move.loop("cycle", until=2.4, delay=0.3)', lambda move: move.loop("cycle", until=2.4, delay=0.3)),
    ('move.loop("pingpong", until=2.4)', lambda move: move.loop("pingpong", until=2.4)),
    ('move.loop("offset", until=1.8)', lambda move: move.loop("offset", until=1.8)),
    ("parallel(move).repeat(3)", lambda move: parallel(move).repeat(3)),
]
moves = []
for index, (label, timing) in enumerate(cycles, start=1):
    scene.text(label, role="code").place(grid[2 * index], anchor="right")
    dot, step = track(grid[2 * index + 1].inset((0, "48px")))
    moves.append(timing(dot.animate.shift_by(step, 0).duration(CYCLE)))
scene.play(moves)
scene.wait(HOLD)


def bars(zone, count=5):
    """``count`` bars across ``zone``, and the animations that grow them."""
    width, (_, y) = zone.width / (2 * count - 1), zone.center
    shapes = [scene.geometry.rect(width, zone.height * 0.8).move_to(zone.left + width * (2 * index + 0.5), y)
              for index in range(count)]
    return [shape.animate.grow_from_edge(Direction.DOWN) for shape in shapes]


def compose(entries, label_share="5fr", stage_share="2fr"):
    """One row of five bars per entry, grown by ``compose(grows)``, its code to the left."""
    grid = rows(len(entries), label_share, stage_share)
    plays = []
    for index, (label, compose_grows) in enumerate(entries):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        plays.append(compose_grows(bars(grid[2 * index + 1].inset(("12px", 0)))))
    # Every grow lasts GROW seconds unless its composition says otherwise.
    scene.play(plays, duration=GROW)
    scene.wait(HOLD)


# parallel starts every item together, sequence chains them (gap spaces or
# overlaps them), and stagger offsets each start by `each` or spreads the
# starts over `total`, optionally eased.
compose_start = scene.cursor
scene.segment("compose")
compose([
    ("parallel(*grows)", lambda grows: parallel(*grows)),
    ("sequence(*grows)", lambda grows: sequence(*grows)),
    ("sequence(*grows, gap=0.15)", lambda grows: sequence(*grows, gap=0.15)),
    ("sequence(*grows, gap=-0.2)", lambda grows: sequence(*grows, gap=-0.2)),
    ("stagger(*grows)", lambda grows: stagger(*grows)),
    ("stagger(*grows, each=0.25)", lambda grows: stagger(*grows, each=0.25)),
    ("stagger(*grows, total=1.2)", lambda grows: stagger(*grows, total=1.2)),
    ("stagger(*grows, total=1.2, easing=Easing.SMOOTH)",
     lambda grows: stagger(*grows, total=1.2, easing=Easing.SMOOTH)),
])

# A Composition's time can be delayed, stretched to a span, given defaults,
# sped up or ramped, or remapped through an easing.
reshape_start = scene.cursor
scene.segment("reshape")
compose([
    ("sequence(*grows)", lambda grows: sequence(*grows)),
    ("sequence(*grows).delay(0.6)", lambda grows: sequence(*grows).delay(0.6)),
    ("sequence(*grows).stretch(0.8)", lambda grows: sequence(*grows).stretch(0.8)),
    ("sequence(*grows).defaults(duration=0.15)", lambda grows: sequence(*grows).defaults(duration=0.15)),
    ("sequence(*grows).speed_ramp(2.0)", lambda grows: sequence(*grows).speed_ramp(2.0)),
    ("sequence(*grows).speed_ramp({0: 1, 0.5: 0.3, 1: 1})",
     lambda grows: sequence(*grows).speed_ramp({0: 1, 0.5: 0.3, 1: 1})),
    ("sequence(*grows).time_remap(Easing.SMOOTH)", lambda grows: sequence(*grows).time_remap(Easing.SMOOTH)),
    ("sequence(*grows).time_remap(Easing.steps(3))", lambda grows: sequence(*grows).time_remap(Easing.steps(3))),
], label_share="4fr", stage_share="1fr")

# With an origin, each dot waits in proportion to its distance from it; grid
# measures in cells instead of positions. distribute spreads values (here,
# sizes) in the same order.
origins_start = scene.cursor
scene.segment("origins")
panels = scene.layout.zones(Zones.grid(rows=3, columns=3, gap="48px", padding="page_padding"))
corner = panels[6].point("bottom_left")
origins = [
    ("origin=None", {}),
    ('origin="start"', {"origin": "start"}),
    ('origin="end"', {"origin": "end"}),
    ('origin="center"', {"origin": "center"}),
    ('origin="edges"', {"origin": "edges"}),
    ('origin="random"', {"origin": "random"}),
    (f"origin=({corner[0]:.2f}, {corner[1]:.2f})", {"origin": corner}),
    ("grid=(4, 7)", {"grid": (4, 7)}),
    ("easing=Easing.SMOOTH", {"easing": Easing.SMOOTH}),
]
waves, sizes = [], []
for panel, (label, options) in zip(panels, origins):
    head, body = panel.split(Zones.rows(["1fr", "4fr"]))
    scene.text(label, role="code").place(head, anchor="center")
    spacing = min(body.width / 7, body.height / 4)
    left, top = body.center[0] - spacing * 3, body.center[1] + spacing * 1.5
    dots = [scene.geometry.dot(0.1).move_to(left + column * spacing, top - row * spacing)
            for row in range(4) for column in range(7)]
    waves.append(stagger(*[dot.animate.grow_from_center().duration(0.4) for dot in dots], total=1.0, **options))
    sizes += [dot.animate.scale_to(size).duration(0.6)
              for dot, size in zip(dots, distribute(dots, 1.6, 0.4, **options))]
scene.play(waves)
scene.play(sizes)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment early, midway and settled; the origins mid-wave and with
    # their distributed sizes.
    scene.snapshots(
        snapshots,
        [repeat_start + t for t in (0.4, 1.0, 1.6, 2.2, 2.8)]
        + [compose_start + t for t in (0.4, 0.8, 1.3, 2.4)]
        + [reshape_start + t for t in (0.4, 0.9, 1.5, 2.9)]
        + [origins_start + t for t in (0.45, 0.8, 2.3)],
    )
else:
    scene.render()
