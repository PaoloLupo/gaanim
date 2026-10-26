"""Angle dimensions: angles between rays with symbols and readouts, in every sweep, with their options.

``scene.mechanics.angle_between(vertex, from_, to)`` draws the arc of an angle
with arrowheads and short extension lines; each ray is a point or a
``Direction``, and a ray that moves turns the arc with it. Here the ray to p
turns while the ray to q stays put. Each label is the code that builds the
variant.
"""

import os

from gaanim import Direction, Easing, Scene, Zones

TURN = 2.4
HOLD = 0.6

scene = Scene()
M = scene.mechanics
accent = scene.colors.accent
muted = scene.colors.muted


def angles(variants, end=2.2, length=2.6, centered=False):
    """Label each cell and measure, at its vertex o, the angle from q (to its right) to p, which turns to ``end``."""
    grid = scene.layout.zones(Zones.columns(len(variants), gap="32px", padding="page_padding"))
    theta = scene.viz.parameter(0.5)
    parts = []
    for zone, (text, build) in zip(grid, variants):
        label_zone, stage = zone.split(Zones.rows([2.0, "1fr"], gap="16px"))
        scene.text(text, role="code", text_align="left").place(label_zone, anchor="bottom")
        if centered:
            o = stage.center
        else:
            o = (stage.center[0] - 0.3 * length, stage.center[1] - 0.4 * length)
        q = (o[0] + length, o[1])
        p = scene.geometry.polar_point(o, length, theta)
        parts += [
            scene.geometry.line(o, q).stroke(muted, 0.03),
            scene.geometry.tracking_line(o, p).stroke(muted, 0.03),
            scene.geometry.dot(0.08).fill(muted).follow(p),
            build(o, q, p),
        ]
    scene.play([part.animate.fade_in() for part in parts], duration=0.4)
    scene.play([theta.animate.set(end).duration(TURN).easing(Easing.SMOOTH)])
    scene.wait(HOLD)


# By default the arc takes the minor angle, at radius 0.64, with arrowheads
# at both ends. A readout sits label_gap beyond the arc, measured to the
# middle of the text, so a wide one needs a wide gap.
scene.segment("angles")
angles([
    ("angle_between(o, q, p)", lambda o, q, p: M.angle_between(o, q, p)),
    ('angle_between(o, q, p,\n  label="$theta$")', lambda o, q, p: M.angle_between(o, q, p, label="$theta$")),
    (
        "angle_between(o, q, p,\n  show_value=True,\n  label_gap=0.9)",
        lambda o, q, p: M.angle_between(o, q, p, show_value=True, label_gap=0.9),
    ),
])

# Readouts in radians, a larger radius, and a ray given as a Direction.
scene.segment("readouts")
angles([
    (
        'angle_between(o, q, p,\n  show_value=True,\n  unit="rad",\n  label_gap=0.9)',
        lambda o, q, p: M.angle_between(o, q, p, show_value=True, unit="rad", label_gap=0.9),
    ),
    (
        'angle_between(o, q, p,\n  radius=1.6,\n  label="$theta$")',
        lambda o, q, p: M.angle_between(o, q, p, radius=1.6, label="$theta$"),
    ),
    (
        'angle_between(o,\n  Direction.UP, p,\n  radius=1.2,\n  label="$phi$")',
        lambda o, q, p: M.angle_between(o, Direction.UP, p, radius=1.2, label="$phi$"),
    ),
])

# sweep picks the minor (default) or major angle, or the one turning
# clockwise ("cw") or counterclockwise ("ccw") from the first ray. As p
# passes half a turn, minor and major swap sides; cw and ccw do not.
scene.segment("sweeps")
angles(
    [
        ("angle_between(\n  o, q, p)", lambda o, q, p: M.angle_between(o, q, p)),
        ('angle_between(\n  o, q, p,\n  sweep="major")', lambda o, q, p: M.angle_between(o, q, p, sweep="major")),
        ('angle_between(\n  o, q, p,\n  sweep="cw")', lambda o, q, p: M.angle_between(o, q, p, sweep="cw")),
        ('angle_between(\n  o, q, p,\n  sweep="ccw")', lambda o, q, p: M.angle_between(o, q, p, sweep="ccw")),
    ],
    end=5.2,
    length=1.5,
    centered=True,
)

# Arrowheads at the "start", the "end", "both" or "none"; extension lines
# can go; color paints the arc and its label; aligned labels follow the arc.
scene.segment("style")
angles([
    ('angle_between(o, q, p,\n  arrowheads="end")', lambda o, q, p: M.angle_between(o, q, p, arrowheads="end")),
    (
        'angle_between(o, q, p,\n  arrowheads="none",\n  show_extensions=False)',
        lambda o, q, p: M.angle_between(o, q, p, arrowheads="none", show_extensions=False),
    ),
    (
        'angle_between(o, q, p,\n  label="$theta$",\n  label_orientation=\n    "aligned",\n  color=accent)',
        lambda o, q, p: M.angle_between(o, q, p, label="$theta$", label_orientation="aligned", color=accent),
    ),
])

# Every segment part-way through the turn and settled; the sweeps before
# and after p passes half a turn.
step = 0.4 + TURN + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [step * index + t for index in range(4) for t in (0.4 + TURN / 3, step - 0.1)])
else:
    scene.render()
