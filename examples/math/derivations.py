"""Derivations: transform one equation into the next, and cancel, reveal, copy or move single terms.

``transform_to`` turns an equation into another: parts with the same name
travel to their new place, matching glyphs morph and the rest shrinks away or
grows in. Selections act on single terms. Each label is the call that
animates the row; the last segment chains the steps of a worked solution.
"""

import os

from gaanim import Scene, Zones, part

WRITE = 1.0
STEP = 1.2
HOLD = 0.8
SEGMENT = WRITE + STEP + HOLD
DERIVATION = 4 * SEGMENT + WRITE
# Equations are set larger than the theme's math size (0.44) so that they
# read at a glance; the worked solution larger still.
SIZE = 0.8

scene = Scene()


def equation(*content):
    return scene.text.equation(*content, size=SIZE)


def table(labels, tracks):
    """Lay out one row per label, labels on the left; return the zones beside them."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(labels), columns=list(tracks), column_gap="48px", padding="page_padding")
    )
    for index, label in enumerate(labels):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
    return [grid[2 * index + 1] for index in range(len(labels))]


def morph(steps, tracks):
    """Write each (label, before, after) row's equation, then morph it into the target.

    The label names the call; every row but the replacement_transform_to one
    plays transform_to.
    """
    zones = table([label for label, _, _ in steps], tracks)
    morphs = []
    for zone, (label, before, after) in zip(zones, steps):
        source = equation(*before).place(zone, anchor="left")
        # The target only lends its shape: hidden until the morph ends on it.
        target = equation(*after).place(zone, anchor="left").hidden()
        play = source.animate.replacement_transform_to if "replacement" in label else source.animate.transform_to
        morphs.append((source, play(target)))
    scene.play([source.animate.write().duration(WRITE) for source, _ in morphs])
    scene.play([anim.duration(STEP) for _, anim in morphs])
    scene.wait(HOLD)


# Each row writes an equation, then morphs it into the target named in its
# label: parts with the same name travel to their new place...
scene.segment("transform_to")
morph(
    [
        (
            "eq.animate.transform_to(moved)",
            ("2", part("x", "x"), part("three", "+ 3"), "=", part("rhs", "11")),
            ("2", part("x", "x"), "=", part("rhs", "11"), part("three", "- 3")),
        ),
        (
            "eq.animate.transform_to(divided)",
            (part("five", "5"), part("x", "x"), "=", part("rhs", "25")),
            (part("x", "x"), "=", part("rhs", "25"), "/", part("five", "5")),
        ),
        (
            "eq.animate.transform_to(expanded)",
            ("E =", part("mass", "m"), "c^2"),
            ("E =", part("mass", "(m_1 + m_2)"), "c^2"),
        ),
        (
            "eq.animate.transform_to(swapped)",
            (part("a", "a"), "+", part("b", "b"), "= c"),
            (part("b", "b"), "+", part("a", "a"), "= c"),
        ),
    ],
    tracks=("8fr", "7fr"),
)

# ...the unnamed row leaves the pairing to the glyphs alone, and
# replacement_transform_to ends with the target in the source's place.
scene.segment("matching")
morph(
    [
        (
            "eq.animate.transform_to(solved)",
            (part("x", "x"), "+ 3 =", part("rhs", "7")),
            (part("x", "x"), "=", part("rhs", "4")),
        ),
        ("eq.animate.transform_to(unnamed)", ("x + 3 = 7",), ("x = 7 - 3",)),
        (
            "eq.animate.replacement_transform_to(solved)",
            (part("x", "x"), "+ 3 =", part("rhs", "7")),
            (part("x", "x"), "=", part("rhs", "4")),
        ),
    ],
    tracks=("2fr", "1fr"),
)

# Selections animate one term inside an equation already on screen: a
# cancelled term is struck out, a revealed one stays hidden until it appears...
scene.segment("terms")
zones = table(
    [
        'eq["three"].animate.cancel()',
        'eq["rhs"].animate.reveal("fade")',
        'eq["rhs"].animate.reveal("from_below")',
        'eq["rhs"].animate.reveal("wipe")',
    ],
    tracks=("9fr", "7fr"),
)
cancelled = equation("x", part("three", "+ 3"), "= 7").place(zones[0], anchor="left")
pythagoras = [equation("a^2 + b^2 =", part("rhs", "c^2")).place(zone, anchor="left") for zone in zones[1:]]
scene.play([eq.animate.write().duration(WRITE) for eq in [cancelled, *pythagoras]])
scene.play([anim.duration(STEP) for anim in [
    cancelled["three"].animate.cancel(),
    pythagoras[0]["rhs"].animate.reveal("fade"),
    pythagoras[1]["rhs"].animate.reveal("from_below"),
    pythagoras[2]["rhs"].animate.reveal("wipe"),
]])
scene.wait(HOLD)

# ...and a term travels to its place in another equation, staying behind with
# copy_to or leaving with morph_to. The target term is hidden until it lands.
scene.segment("transfers")
zones = table(
    ['e["mass"].animate\n.copy_to(p["mass"])', 'e["mass"].animate\n.morph_to(p["mass"])'],
    tracks=("4fr", "9fr"),
)
energies = [equation("E =", part("mass", "m"), "c^2").place(zone, anchor="left") for zone in zones]
momenta = [equation("p =", part("mass", "m"), "v").place(zone, anchor="right") for zone in zones]
scene.play([eq.animate.write().duration(WRITE) for eq in [*energies, *momenta]])
scene.play([anim.duration(STEP) for anim in [
    energies[0]["mass"].animate.copy_to(momenta[0]["mass"]),
    energies[1]["mass"].animate.morph_to(momenta[1]["mass"]),
]])
scene.wait(HOLD)

# A worked solution: each step morphs the equation in place into the next,
# then a flash goes around the result.
scene.segment("derivation")
SOLUTION = [
    ("2", part("x", "x"), part("three", "+ 3"), "=", part("rhs", "11")),
    ("2", part("x", "x"), "=", part("rhs", "11"), part("three", "- 3")),
    ("2", part("x", "x"), "=", part("rhs", "8")),
    (part("x", "x"), "=", part("rhs", "8"), "/", part("two", "2")),
    (part("x", "x"), "=", part("result", "4")),
]
center = scene.layout.zones(Zones.grid(rows=1, columns=1, padding="page_padding"))[0]
steps = [scene.text.equation(*content, size=1.4).place(center) for content in SOLUTION]
for later in steps[1:]:
    # Each later step only lends its shape until a morph ends on it.
    later.hidden()
scene.play([steps[0].animate.write(by="part").duration(WRITE)])
for current, following in zip(steps, steps[1:]):
    scene.wait(0.3)
    scene.play([current.animate.transform_to(following).duration(STEP)])
scene.play([steps[-1]["result"].animate.flash_around().duration(STEP)])
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Written, mid-morph and settled in transform_to; mid-way and settled in
    # matching and transfers; written, mid-way and settled in terms; then the
    # first step, mid-way through the third, and the flashed result.
    scene.snapshots(snapshots, [
        WRITE - 0.05, WRITE + STEP / 2, SEGMENT - 0.1,
        SEGMENT + WRITE + STEP / 2, 2 * SEGMENT - 0.1,
        2 * SEGMENT + WRITE - 0.05, 2 * SEGMENT + WRITE + STEP / 2, 3 * SEGMENT - 0.1,
        3 * SEGMENT + WRITE + STEP / 2, 4 * SEGMENT - 0.1,
        DERIVATION + 0.3 + STEP / 2, DERIVATION + 3 * (0.3 + STEP) - STEP / 2,
        DERIVATION + 4 * (0.3 + STEP) + STEP / 2, DERIVATION + 4 * (0.3 + STEP) + STEP + HOLD - 0.1,
    ])
else:
    scene.render()
