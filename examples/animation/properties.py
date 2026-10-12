"""Property animations: every target ``drawable.animate`` can move, transform, restyle or compute.

Each row animates one shape from the state its outline marks, and is labelled
with the call chained on ``shape.animate``. Segments cover position, transforms,
style, and custom animations computed by your own function.
"""

import math
import os

from gaanim import Anchor, Brush, Scene, Zones

DURATION = 1.4
HOLD = 0.6

scene = Scene()
accent = scene.canvas.color("accent")
foreground = scene.canvas.color("foreground")
rule = scene.canvas.color("rule")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")


def table(entries):
    """One row per entry, its code to the left: ``build(stage)`` returns the row's animation."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(entries), columns=["5fr", "2fr"], column_gap="48px", padding="page_padding")
    )
    moves = []
    for index, (label, build) in enumerate(entries):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        moves.append(build(grid[2 * index + 1].inset((0, "48px"))))
    scene.play(moves, duration=DURATION)
    scene.wait(HOLD)


def mark(x, y):
    """A small dot marking the scene point ``(x, y)``."""
    scene.geometry.dot(0.05).fill(rule).move_to(x, y)


def square(x, y, size=0.5):
    """A square at ``(x, y)`` over an outline that keeps its starting place."""
    scene.geometry.square(size).no_fill().stroke(rule, 0.02).move_to(x, y)
    return scene.geometry.square(size).move_to(x, y)


def plane(x, y):
    """A paper plane pointing along +x at ``(x, y)``, over an outline of where it starts."""
    points = [(0.32, 0.0), (-0.22, 0.18), (-0.1, 0.0), (-0.22, -0.18)]
    scene.geometry.polygon(points).no_fill().stroke(rule, 0.02).move_to(x, y)
    return scene.geometry.polygon(points).move_to(x, y)


def start(zone):
    """The point near the left end of ``zone`` where moving shapes start."""
    return zone.left + 0.4, zone.center[1]


# Position: a point (with the anchor that lands on it), another drawable or
# one of its anchor points, a relative shift, or a layout zone.
def moving_to(zone):
    x, y = zone.right - 0.4, zone.center[1] + 0.2
    mark(x, y)
    return square(*start(zone)).animate.move_to(x, y)


def anchored(zone):
    x, y = zone.center
    mark(x, y)
    return square(*start(zone)).animate.move_to(x, y, anchor=Anchor.LEFT)


def to_target(zone):
    target = scene.geometry.circle(0.3).no_fill().stroke(foreground, 0.02).move_to(zone.right - 0.5, zone.center[1])
    return square(*start(zone)).animate.move_to(target)


def to_anchor(zone):
    target = scene.geometry.circle(0.3).no_fill().stroke(foreground, 0.02).move_to(zone.right - 0.5, zone.center[1])
    return square(*start(zone)).animate.move_to(target.anchor_point(Anchor.LEFT))


scene.segment("position")
table([
    ("move_to(x, y)", moving_to),
    ("move_to(x, y, anchor=Anchor.LEFT)", anchored),
    ("move_to(target)", to_target),
    ("move_to(target.anchor_point(Anchor.LEFT))", to_anchor),
    ("shift_by(2.0, 0.3)", lambda zone: square(*start(zone)).animate.shift_by(2.0, 0.3)),
    ('place(stage, anchor="right")', lambda zone: square(*start(zone)).animate.place(zone, anchor="right")),
])


# Transforms: uniform or per-axis scale, absolute or relative rotation (about
# the shape's pivot or any scene point), shear, and a general linear map.
def orbiting(zone):
    x, y = zone.center
    mark(x - 0.45, y)
    return plane(x, y).animate.rotate_by(math.pi).about_point(x - 0.45, y)


scene.segment("transform")
table([
    ("scale_to(1.8)", lambda zone: square(*zone.center).animate.scale_to(1.8)),
    ("scale_by(0.5)", lambda zone: square(*zone.center).animate.scale_by(0.5)),
    ("scale_to_3d(2.5, 0.5, 1.0)", lambda zone: square(*zone.center).animate.scale_to_3d(2.5, 0.5, 1.0)),
    ("rotate_to(math.pi / 4)", lambda zone: plane(*zone.center).animate.rotate_to(math.pi / 4)),
    ("rotate_by(-math.pi / 2)", lambda zone: plane(*zone.center).animate.rotate_by(-math.pi / 2)),
    ("rotate_by(math.pi).about_point(x, y)", orbiting),
    ("skew_to(0.6, 0)", lambda zone: square(*zone.center).animate.skew_to(0.6, 0)),
    ("matrix_to(((1, 0.5), (0.3, 1)))", lambda zone: square(*zone.center).animate.matrix_to(((1, 0.5), (0.3, 1)))),
])


# Style: opacity, a fill color or gradient, a stroke appearing or widening,
# the vertices of a polygon, and several targets sharing one animation.
def card(zone):
    """A card in the middle of ``zone`` over an outline of its starting shape."""
    scene.geometry.rounded_rect(1.8, 0.5, 0.08).no_fill().stroke(rule, 0.02).move_to(*zone.center)
    return scene.geometry.rounded_rect(1.8, 0.5, 0.08).move_to(*zone.center)


def reshaping(zone):
    x, y = zone.center
    triangle = [(x - 0.9, y - 0.25), (x + 0.9, y - 0.25), (x, y + 0.25)]
    flipped = [(x - 0.9, y + 0.25), (x + 0.9, y + 0.25), (x + 0.6, y - 0.25)]
    scene.geometry.polygon(triangle).no_fill().stroke(rule, 0.02)
    return scene.geometry.polygon(triangle).animate.points(flipped)


def combined(zone):
    x, y = zone.right - 0.5, zone.center[1]
    mark(x, y)
    return square(*start(zone)).animate.move_to(x, y).scale_to(1.4).fill(danger)


gradient = Brush.linear([accent, danger], start=(-0.9, 0), end=(0.9, 0))
scene.segment("style")
table([
    ("opacity(0.2)", lambda zone: card(zone).animate.opacity(0.2)),
    ("fill(success)", lambda zone: card(zone).animate.fill(success)),
    ("fill(gradient)", lambda zone: card(zone).animate.fill(gradient)),
    ("stroke(danger, 0.08)", lambda zone: card(zone).animate.stroke(danger, 0.08)),
    ("stroke_width(0.12)", lambda zone: card(zone).stroke(foreground, 0.02).animate.stroke_width(0.12)),
    ("points(flipped)", reshaping),
    ("move_to(x, y).scale_to(1.4).fill(danger)", combined),
])


# custom calls your function with the eased progress and takes the values it
# returns for the declared channels. A Parameter animates with set.
def hopping(zone):
    (x, y), distance = start(zone), zone.width - 0.8

    def hop(a):
        return {"position": (x + distance * a, y + 0.35 * math.sin(math.pi * a))}

    return square(x, y).animate.custom(hop, channels=("position",))


def spinning(zone):
    def spin(a):
        return {"rotation": math.tau * a, "scale": 1 + 0.6 * math.sin(math.pi * a)}

    return square(*zone.center).animate.custom(spin, channels=("rotation", "scale"))


def flickering(zone):
    def flicker(a):
        return {"opacity": 0.6 + 0.4 * math.cos(6 * math.pi * a)}

    return card(zone).animate.custom(flicker, channels=("opacity",))


def breathing(zone):
    def breathe(a):
        return {"stroke_width": 0.02 + 0.12 * math.sin(math.pi * a)}

    return card(zone).stroke(foreground, 0.02).animate.custom(breathe, channels=("stroke_width",))


def switching(zone):
    def switch(a):
        return {"fill": accent if a < 0.5 else success}

    return card(zone).animate.custom(switch, channels=("fill",))


def setting(zone):
    level = scene.viz.parameter(1.0)
    scene.geometry.circle(0.2).no_fill().stroke(rule, 0.02).move_to(*zone.center)
    scene.geometry.circle(0.2).move_to(*zone.center).scale_to(level)
    return level.animate.set(2.0)


scene.segment("custom")
table([
    ('custom(hop, channels=("position",))', hopping),
    ('custom(spin, channels=("rotation", "scale"))', spinning),
    ('custom(flicker, channels=("opacity",))', flickering),
    ('custom(breathe, channels=("stroke_width",))', breathing),
    ('custom(switch, channels=("fill",))', switching),
    ("level.animate.set(2.0)", setting),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment midway and settled.
    scene.snapshots(
        snapshots,
        [segment * (DURATION + HOLD) + t for segment in range(4) for t in (0.5, 0.9, 1.8)],
    )
else:
    scene.render()
