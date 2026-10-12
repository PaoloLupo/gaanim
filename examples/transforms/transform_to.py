"""Transforms: morph a shape or a text into another with transform_to and its variants.

``source.animate.transform_to(target)`` reshapes the source into the
target's outline, place and style; the target is only a template and stays
hidden. Segments cover shapes and strokes, fills and gradients, which object
lives on afterwards and its opacity, and texts. Each label is the call that
morphs the row.
"""

import math
import os

from gaanim import Brush, Scene, Zones, part

MORPH = 1.4
STEP = 0.8
HOLD = 0.6

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")
warning = scene.canvas.color("warning")
foreground = scene.canvas.color("foreground")

# Shapes are about 1.9 units across; R is half of that.
R = 0.95


def rows(labels):
    """One row per label, the label to the left; return each row's stage."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(labels), columns=["1fr", "1fr"], column_gap="48px", padding="page_padding")
    )
    for index, label in enumerate(labels):
        scene.text(label, role="code", markup=False).place(grid[2 * index], anchor="right")
    return [grid[2 * index + 1] for index in range(len(labels))]


def ends(stage):
    """The points a source starts from and its target sits at: both ends of ``stage``."""
    (x, y), half = stage.center, stage.width / 2 - R - 0.1
    return (x - half, y), (x + half, y)


def morph(entries, travel=True):
    """Morph each row's source into its target, all rows together.

    ``entries`` are (label, source, target): functions that build a drawable
    at a point. With ``travel`` the source starts at the left end of the
    stage and its target sits at the right end; otherwise both are centered.
    """
    stages = rows([label for label, _, _ in entries])
    moves = []
    for (_, source, target), stage in zip(entries, stages):
        start, end = ends(stage) if travel else (stage.center, stage.center)
        moves.append(source(start).animate.transform_to(target(end)))
    scene.play(moves, duration=MORPH)
    scene.wait(HOLD)


def circle(point):
    return scene.geometry.circle(R).move_to(*point)


def square(point):
    return scene.geometry.square(1.75 * R).move_to(*point)


def star(point):
    return scene.geometry.star(5, 1.1 * R, 0.48 * R).move_to(*point)


def wedge(point, direction=1):
    """A triangle pointing right, or left with ``direction=-1``."""
    x, y = point
    tail, tip = x - 0.9 * R * direction, x + R * direction
    return scene.geometry.polygon([(tail, y - 0.9 * R), (tip, y), (tail, y + 0.9 * R)])


def ring(point):
    return circle(point).no_fill().stroke(foreground, 0.08)


def line(point):
    x, y = point
    return scene.geometry.line(x - R, y, x + R, y).stroke(foreground, 0.08)


def arc(point):
    x, y = point
    return scene.geometry.arc(x, y - R / 2, R, 0, math.pi).stroke(foreground, 0.08)


# Any outline morphs into any other; each pairs with the start point and
# winding that move its points least, so a shape and its mirror image turn
# over instead of collapsing.
scene.segment("shapes")
morph([
    ("circle.animate\n    .transform_to(square)", circle, square),
    ("square.animate\n    .transform_to(star)", square, star),
    ("wedge.animate\n    .transform_to(mirrored)", wedge, lambda point: wedge(point, -1)),
])

# Open strokes morph like closed ones, and an outline takes on the fill of
# its target.
scene.segment("outlines")
morph([
    ("star.animate\n    .transform_to(circle)", star, circle),
    ("ring.animate\n    .transform_to(square)", ring, square),
    ("line.animate\n    .transform_to(arc)", line, arc),
])


# Fill and stroke interpolate toward the target's. A solid blends into a
# gradient; two gradients of one kind blend their geometry and stops, and a
# linear and a radial gradient pass through a solid color halfway. ``a`` is
# a square and ``b`` a circle.
sunset = Brush.linear([accent, danger], start=(-R, 0), end=(R, 0))
dawn = Brush.linear([success, warning], start=(-R, 0), end=(R, 0))
glow = Brush.radial([warning, danger], radius=R)


def filled(paint):
    """A square filled with ``paint`` at a point."""
    return lambda point: square(point).fill(paint)


def round_one(paint):
    """A circle filled with ``paint`` at a point."""
    return lambda point: circle(point).fill(paint)


scene.segment("fills")
morph([
    ("a.animate\n    .transform_to(b.fill(success))", square, round_one(success)),
    ("a.animate\n    .transform_to(b.fill(sunset))", square, round_one(sunset)),
    ("a.fill(sunset).animate\n    .transform_to(b.fill(dawn))", filled(sunset), round_one(dawn)),
])

scene.segment("strokes")
morph([
    ("a.fill(sunset).animate\n    .transform_to(b.fill(glow))", filled(sunset), round_one(glow)),
    (
        "a.animate.transform_to(\n    b.stroke(foreground, 0.12))",
        square,
        lambda point: circle(point).stroke(foreground, 0.12),
    ),
    (
        "a.stroke(foreground, 0.12)\n    .animate.transform_to(b)",
        lambda point: square(point).stroke(foreground, 0.12),
        circle,
    ),
])


# transform_to keeps the source: it takes the target's shape and goes on
# animating, while the target stays hidden. replacement_transform_to and
# fade_transform_to hand over to the target, which animates next.
scene.segment("handover")
HANDOVERS = [
    ("a.animate.transform_to(b)\na.animate.rotate_by(math.pi / 4)", "transform_to", "source"),
    ("a.animate\n    .replacement_transform_to(b)\nb.animate.rotate_by(math.pi / 4)", "replacement_transform_to", "target"),
    ("a.animate.fade_transform_to(b)\nb.animate.rotate_by(math.pi / 4)", "fade_transform_to", "target"),
]
morphs, turns = [], []
for (_, method, survivor), stage in zip(HANDOVERS, rows([label for label, _, _ in HANDOVERS])):
    start, end = ends(stage)
    a, b = circle(start), square(end)
    morphs.append(getattr(a.animate, method)(b))
    turns.append((a if survivor == "source" else b).animate.rotate_by(math.pi / 4))
scene.play(morphs, duration=MORPH)
scene.play(turns, duration=STEP)
scene.wait(HOLD)

# The source keeps its own opacity: the target's is ignored, so play an
# opacity animation with the morph to fade the source as it changes.
scene.segment("opacity")
stages = rows([
    "[a.animate.transform_to(b),\n a.animate.opacity(0.25)]",
    "a.animate\n    .transform_to(b.opacity(0.25))",
])
(start, end), (kept_start, kept_end) = ends(stages[0]), ends(stages[1])
faded, kept = circle(start), circle(kept_start)
scene.play(
    [faded.animate.transform_to(square(end)), faded.animate.opacity(0.25), kept.animate.transform_to(square(kept_end).opacity(0.25))],
    duration=MORPH,
)
scene.wait(HOLD)


# Between texts, parts with the same name travel to their new place,
# matching glyphs morph and the rest shrinks away or grows in; every glyph
# keeps its part's color. Each target sits where its source is, so these
# morph in place.
SIZE = 1.1


def word(*content):
    return lambda point: scene.text(*content, size=SIZE).move_to(*point)


scene.segment("text")
morph(
    [
        ("energy.animate\n    .transform_to(momentum)", word("Energy"), word("Momentum")),
        (
            "pair.animate\n    .transform_to(swapped)",
            word(part("fast", "fast"), " and ", part("cheap", "cheap", color=accent)),
            word(part("cheap", "cheap", color=accent), " beats ", part("fast", "fast")),
        ),
        (
            "momentum.animate\n    .transform_to(formula)",
            word("momentum"),
            lambda point: scene.text.equation("p = m v", size=SIZE).move_to(*point),
        ),
    ],
    travel=False,
)

# A text and a shape morph outline to outline.
scene.segment("text_and_shapes")
morph(
    [
        ("star.animate\n    .transform_to(label)", star, word("Star")),
        ("label.animate\n    .transform_to(square)", word("Square"), square),
    ],
    travel=False,
)

SEGMENT = MORPH + HOLD
HANDOVER = 4 * SEGMENT
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Every segment midway and settled; the handover also mid-turn.
    scene.snapshots(
        snapshots,
        [SEGMENT * index + t for index in range(4) for t in (0.7, SEGMENT - 0.1)]
        + [HANDOVER + t for t in (0.7, MORPH + 0.4, MORPH + STEP + HOLD - 0.1)]
        + [HANDOVER + MORPH + STEP + HOLD + SEGMENT * index + t for index in range(3) for t in (0.7, SEGMENT - 0.1)],
    )
else:
    scene.render()
