"""Matching transforms: pair the pieces of two drawables, morph each pair and fade the rest.

``transform_matching_shapes`` pairs pieces, glyphs included, by shape, place
and color; ``transform_to`` between texts pairs named parts and matching
glyphs, and between groups and SVGs it pairs parts by name or SVG ``id``.
In every band the source starts on the left and an outline on the right
marks where its target is. Each label is the call that morphs the band.
"""

import os

from gaanim import Scene, Zones

MORPH = 1.4
GAP = 0.3
HOLD = 0.6

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")
rule = scene.canvas.color("rule")
geometry = scene.geometry

SHAPES = {
    "circle": lambda: geometry.circle(0.52),
    "square": lambda: geometry.square(0.94),
    "triangle": lambda: geometry.regular_polygon(3, 0.62),
    "star": lambda: geometry.star(5, 0.62, 0.27),
    "hexagon": lambda: geometry.regular_polygon(6, 0.54),
}


def bands(labels):
    """One band per label across the frame, the label above a wide stage; return the stages."""
    stages = []
    for band, label in zip(scene.layout.zones(Zones.rows(len(labels), gap="24px", padding="page_padding")), labels):
        top, stage = band.split(Zones.rows(["40px", "1fr"]))
        scene.text(label, role="code").place(top, anchor="left")
        stages.append(stage)
    return stages


def halves(stage):
    """The left and right halves of ``stage``: the source's side and the target's."""
    left, right = stage.split(Zones.columns(2, gap="96px"))
    return left, right


def row(zone, kinds, outline=False, named=False):
    """A group of shapes spread across ``zone``.

    ``kinds`` are shape names, or (name, color) pairs. With ``outline`` the
    shapes are only outlined, to mark where a hidden target is; with
    ``named`` each shape is named after its slot: "slot0", "slot1"...
    """
    members = []
    for index, (cell, kind) in enumerate(zip(zone.split(Zones.columns(len(kinds))), kinds)):
        name, color = kind if isinstance(kind, tuple) else (kind, accent)
        shape = SHAPES[name]().move_to(*cell.center)
        if outline:
            shape.no_fill().stroke(rule if color == accent else color, 0.03)
        else:
            shape.fill(color)
        members.append(shape.named(f"slot{index}") if named else shape)
    return geometry.group(members)


def shapes_pair(stage, before, after, named=False):
    """A group of ``before`` shapes on the left of ``stage``, its target of ``after`` on the right."""
    left, right = halves(stage)
    row(right, after, outline=True)
    return row(left, before, named=named), row(right, after, named=named)


SIZE = 0.82


def texts(stage, kind, before, after):
    """A text or equation on the left of ``stage``, its target on the right over a muted copy."""
    left, right = halves(stage)
    kind(after, size=SIZE, color=rule).move_to(*right.center)
    return kind(before, size=SIZE).move_to(*left.center), kind(after, size=SIZE).move_to(*right.center)


SHUFFLE = (["circle", "square", "triangle", "star"], ["triangle", "circle", "star", "square"])

# transform_matching_shapes plays at the cursor like scene.play and moves it
# on, so these bands morph one after another. Each piece pairs with the
# closest shape, place and color in the target; leftovers fade out and new
# pieces fade in.
scene.segment("shapes")
stages = bands([
    "geometry.transform_matching_shapes(shapes, shuffled_shapes)",
    "geometry.transform_matching_shapes(shapes, changed_shapes)",
    "geometry.transform_matching_shapes(colors, swapped_colors)",
])
CASES = [
    SHUFFLE,
    (["circle", "square", "square", "triangle"], ["square", "hexagon", "circle", "star"]),
    ([("circle", accent), ("circle", success), ("square", danger)], [("circle", success), ("square", danger), ("circle", accent)]),
]
for source, target in [shapes_pair(stage, before, after) for stage, (before, after) in zip(stages, CASES)]:
    geometry.transform_matching_shapes(source, target, duration=MORPH)
    scene.wait(GAP)
scene.wait(HOLD)


# Glyphs are pieces too: transform_matching(mode="shapes"), the general form,
# pairs letters by outline and place. transform_to between two texts instead
# moves named parts and matching glyphs and lets the others shrink and grow.
scene.segment("glyphs")
stages = bands([
    'geometry.transform_matching(listen, silent, mode="shapes")',
    "geometry.transform_matching_shapes(energy, momentum)",
    "listen.animate.transform_to(silent)",
    "energy.animate.transform_to(momentum)",
])
pairs = [
    texts(stages[0], scene.text, "listen", "silent"),
    texts(stages[1], scene.text.equation, "E = m c^2", "p = m v^2"),
    texts(stages[2], scene.text, "listen", "silent"),
    texts(stages[3], scene.text.equation, "E = m c^2", "p = m v^2"),
]
geometry.transform_matching(*pairs[0], mode="shapes", duration=MORPH)
scene.wait(GAP)
geometry.transform_matching_shapes(*pairs[1], duration=MORPH)
scene.wait(GAP)
scene.play([source.animate.transform_to(target) for source, target in pairs[2:]], duration=MORPH)
scene.wait(GAP + HOLD)


# transform_to between two groups morphs part by part: parts with the same
# name pair first, at any depth, then the remaining pieces pair by shape.
# The source keeps its identity, so later animations still address it.
scene.segment("parts")
stages = bands([
    "shapes.animate.transform_to(shuffled_shapes)",
    "slots.animate.transform_to(shuffled_slots)",
])
shapes, shuffled_shapes = shapes_pair(stages[0], *SHUFFLE)
# The same shapes, each named after its slot: names pair before shapes do.
slots, shuffled_slots = shapes_pair(stages[1], *SHUFFLE, named=True)
scene.play([shapes.animate.transform_to(shuffled_shapes), slots.animate.transform_to(shuffled_slots)], duration=MORPH)
scene.wait(HOLD)

# Between SVGs the ids of their elements name the parts.
scene.segment("svg_parts")
(stage,) = bands(["robot.animate.transform_to(character)"])
left, right = halves(stage)
robot = scene.media.svg("assets/robot.svg").scale_to(3.5).move_to(*left.center)
scene.media.svg("assets/character.svg").scale_to(1.7).move_to(*right.center).opacity(0.15)
character = scene.media.svg("assets/character.svg").scale_to(1.7).move_to(*right.center)
scene.wait(0.3)
scene.play([robot.animate.transform_to(character)], duration=MORPH)
scene.wait(HOLD)

SHAPES_END = 3 * (MORPH + GAP) + HOLD
GLYPHS_END = SHAPES_END + 3 * (MORPH + GAP) + HOLD
PARTS_END = GLYPHS_END + MORPH + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each matching morph midway, then every segment settled.
    scene.snapshots(
        snapshots,
        [index * (MORPH + GAP) + MORPH / 2 for index in range(3)]
        + [SHAPES_END - 0.1]
        + [SHAPES_END + index * (MORPH + GAP) + MORPH / 2 for index in range(3)]
        + [GLYPHS_END - 0.1, GLYPHS_END + 0.4, GLYPHS_END + MORPH / 2, PARTS_END - 0.1]
        + [PARTS_END + 0.2, PARTS_END + 0.3 + MORPH / 2, PARTS_END + 0.3 + MORPH + HOLD - 0.1],
    )
else:
    scene.render()
