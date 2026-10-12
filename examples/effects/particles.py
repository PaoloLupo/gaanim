"""Particles: emitter shapes, motion, looks, bursts and confetti, all exact at any seek.

``scene.fx.particles`` returns an emitter drawable whose particles are closed
functions of the timeline time, so a seek shows what playback shows. The
segments vary where particles leave from, how they move, how they look, and
when they are emitted: continuously, for a while, or in bursts, ending with
the confetti preset. Each label is the code of its variant.
"""

import math
import os

from gaanim import Brush, Emitter, Scene, Zones

RUN = 2.0

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")
chart = scene.canvas.color("chart")
rule = scene.canvas.color("rule")
PALETTE = [accent, success, danger, chart]


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code", markup=False)


def gallery(variants, rows=3, columns=2, caption=0.45):
    """Draw each (label, build) variant in a panel; ``build(zone)`` makes its emitter.

    A build may return an animation, which plays while the particles run.
    """
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    plays = []
    for zone, (code, build) in zip(grid, variants):
        art, text_zone = zone.split(Zones.rows(["1fr", caption]))
        label(code).place(text_zone)
        scene.geometry.rect(art.width, art.height).no_fill().stroke(rule, 0.02).move_to(*art.center)
        play = build(art)
        if play is not None:
            plays.append(play)
    if plays:
        scene.play(plays, duration=RUN)
    else:
        scene.wait(RUN)


def drifting(emitter):
    """Slow particles that stay near ``emitter``, built for one panel."""

    def build(zone):
        scene.fx.particles(emitter.at(zone.center), rate=60, speed=(0.2, 0.5), lifetime=(1.0, 1.6), size=(0.08, 0.14), seed=1)
    return build


def trail(zone):
    """A dot that crosses ``zone`` and sheds particles where it was."""
    (left, y), (right, _) = zone.point("left"), zone.point("right")
    dot = scene.geometry.circle(0.15).move_to(left + 0.4, y)
    scene.fx.particles(Emitter.circle(0.15).at(dot), rate=80, speed=(0.1, 0.4), lifetime=(0.6, 1.2), size=(0.08, 0.14), seed=2)
    return dot.animate.move_to(right - 0.4, y)


# Emitter.point, circle (inside or on its rim), rect and line set where
# particles leave from; at() centers them on a point or follows a drawable.
emitters_start = scene.cursor
scene.segment("emitters")
gallery([
    ("Emitter.point()", drifting(Emitter.point())),
    ("Emitter.circle(0.6)", drifting(Emitter.circle(0.6))),
    ("Emitter.circle(0.6, edge=True)", drifting(Emitter.circle(0.6, edge=True))),
    ("Emitter.rect(5, 0.6)", drifting(Emitter.rect(5, 0.6))),
    ("Emitter.line(5, angle=0.25)", drifting(Emitter.line(5, angle=0.25))),
    ("Emitter.circle(0.15).at(dot)", trail),
])


def moving(**options):
    """Particles from the left of a panel (its bottom when they rise), moving as ``options`` say."""

    def build(zone):
        x, y = zone.point("bottom" if "direction" not in options else "left")
        start = (x, y + 0.1) if "direction" not in options else (x + 0.3, y)
        scene.fx.particles(Emitter.point().at(start), rate=60, size=(0.08, 0.12), seed=3, **options)
    return build


# speed heads within spread radians around direction (up by default) for a
# lifetime; gravity accelerates every particle, drag slows it, flutter sways
# it sideways and spin turns it. Each panel emits 60 particles a second.
motion_start = scene.cursor
scene.segment("motion")
gallery(
    [
        ("direction=0, spread=0.2,\nspeed=(3, 5)", moving(direction=0, spread=0.2, speed=(3, 5))),
        (
            "spread=0.6, speed=(2.5, 3.5),\ngravity=(0, -8), lifetime=0.8",
            moving(spread=0.6, speed=(2.5, 3.5), gravity=(0, -8), lifetime=0.8),
        ),
        ("direction=0, spread=0.6,\nspeed=(4, 8), drag=3.0", moving(direction=0, spread=0.6, speed=(4, 8), drag=3.0)),
        ("direction=0, spread=0.2,\nflutter=0.4", moving(direction=0, spread=0.2, flutter=0.4)),
        ('direction=0, spread=0.6,\nshape="square", spin=8', moving(direction=0, spread=0.6, shape="square", spin=8)),
        ("direction=0, spread=0.2,\nlifetime=(0.2, 2.0)", moving(direction=0, spread=0.2, lifetime=(0.2, 2.0))),
    ],
    caption=0.9,
)


def looking(**options):
    """Slow particles from the middle of a panel, drawn as ``options`` say."""

    def build(zone):
        settings = {"rate": 30, "speed": (0.3, 0.6), "lifetime": (1.2, 1.8), "size": (0.16, 0.26), "seed": 4, **options}
        scene.fx.particles(Emitter.circle(0.3).at(zone.center), **settings)
    return build


def streaks(zone):
    """Fast streaks from the left of a panel, stretched along their velocity."""
    x, y = zone.point("left")
    scene.fx.particles(
        Emitter.point().at((x + 0.3, y)), rate=40, shape="streak", direction=0, spread=0.3, speed=(3, 5), size=(0.06, 0.1), seed=4
    )


# shape is circle, square, rect (a 2:1 strip), triangle or streak (stretched
# along the velocity); size scales to size * size_end over each life, the last
# fade of each life fades it, and color is one color, a list or a gradient.
looks_start = scene.cursor
scene.segment("looks")
gallery([
    ('shape="rect", spin=4', looking(shape="rect", spin=4)),
    ('shape="triangle", spin=4', looking(shape="triangle", spin=4)),
    ('shape="streak", speed=(3, 5)', streaks),
    ("size_end=0.0, fade=0.0", looking(size_end=0.0, fade=0.0)),
    ("color=[accent, success, danger]", looking(color=[accent, success, danger])),
    ("color=Brush.linear(...)", looking(color=Brush.linear([danger, accent, success], start=(0, 0), end=(1, 0)))),
])


# rate=0 emits only bursts: burst() at the cursor, or animate.burst(), which
# lasts until its particles settle. duration stops a continuous emitter. The
# emitter is a drawable, so its opacity reaches every particle.
def bursting(zone):
    """An emitter that only bursts, when its animation starts."""
    sparks = scene.fx.particles(zone.center, rate=0, speed=(1, 3), drag=1.5, lifetime=(0.8, 1.4), size=(0.08, 0.14), color=PALETTE, seed=5)
    return sparks.animate.burst(120)


def burst_now(zone):
    """An emitter that bursts at the cursor, before anything plays."""
    scene.fx.particles(zone.center, rate=0, speed=(1, 3), drag=1.5, lifetime=(1.2, 1.8), size=(0.08, 0.14), color=accent, seed=6).burst(80)


def stopping(zone):
    """A stream that stops emitting after half a second."""
    x, y = zone.point("left")
    scene.fx.particles((x + 0.3, y), rate=80, duration=0.5, direction=0, spread=0.3, speed=(2, 3), lifetime=1.2, size=(0.08, 0.14), seed=7)


def faint(zone):
    """A stream drawn at a third of its opacity."""
    scene.fx.particles(Emitter.rect(zone.width * 0.8, 0.2).at(zone.center), rate=60, speed=(0.3, 0.6), size=(0.1, 0.18), seed=8).opacity(0.35)


bursts_start = scene.cursor
scene.segment("bursts")
gallery(
    [
        ("sparks.animate.burst(120)", bursting),
        ("sparks.burst(80)", burst_now),
        ("particles(..., duration=0.5)", stopping),
        ("particles(...).opacity(0.35)", faint),
    ],
    rows=2,
)

# confetti throws paper strips that spin, sway and fall; burst() throws more
# of the same, and colors, direction and spread aim them.
confetti_start = scene.cursor
scene.segment("confetti")
stage = scene.layout.zones(Zones.grid(rows=1, columns=1, padding="page_padding"))[0]
left, bottom = stage.point("bottom_left")
right, _ = stage.point("bottom_right")
label("party = scene.fx.confetti(\n    (x, y), count=120, colors=PALETTE)").place(stage, anchor="top_left")
label("later.animate.burst(60)").place(stage, anchor="top_right")
party = scene.fx.confetti((left + stage.width * 0.25, bottom), count=120, colors=PALETTE, seed=2)
later = scene.fx.confetti((right - stage.width * 0.25, bottom), count=0, colors=PALETTE, direction=math.pi / 2 + 0.3, seed=3)
scene.wait(1.0)
scene.play([later.animate.burst(60)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each gallery once the particles fill it, the bursts early and late, and the confetti flying and falling.
    scene.snapshots(
        snapshots,
        [start + 1.5 for start in (emitters_start, motion_start, looks_start)]
        + [bursts_start + t for t in (0.3, 1.2)]
        + [confetti_start + t for t in (0.6, 1.6, 2.8)],
    )
else:
    scene.render()
