"""Glow, blur and drop shadow: their parameters, the drawables they apply to, and how they animate.

The first segments set each effect once per variant: glow radius against
intensity, blur on fills, strokes, text and groups, and shadow offset against
softness, color and alpha. The last two animate them: a glow that grows,
pulses and fades, a blur that focuses and defocuses, and a shadow that lifts a
card, grows from under it and fades away. Each label, or row label joined to
its column header, is the code of its variant.
"""

import os

from gaanim import Easing, Scene, Zones

HOLD = 1.0
MOVE = 1.2

scene = Scene()
accent = scene.colors.accent
foreground = scene.colors.foreground
success = scene.colors.success
# Shadows are dark, so they show on a light surface: cards cast them on tiles.
shade = scene.colors.background
surface = scene.colors.chart


def faded(color, alpha):
    """``color`` at ``alpha`` (0 to 1) opacity; a shadow color's alpha scales the shadow."""
    return (color.r, color.g, color.b, round(alpha * 255))


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code")


def table(rows, columns=None, build=None, corner=None, labels="12fr"):
    """Draw one variant per cell, with each row's label to the left of its cells.

    ``rows`` are (label, value) pairs. Without ``columns`` each row has one
    cell and its value is ``build(zone)``. Otherwise ``columns`` are (label,
    value) pairs labelled in a header row, after ``corner``, and each cell is
    ``build(zone, row_value, column_value)``. Builds may return an animation;
    they play together, between two holds.
    """
    header = columns is not None
    cells = columns or [(None, None)]
    tracks = [labels] + ["9fr"] * len(cells)
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows) + header, columns=tracks, column_gap="48px", row_gap="24px", padding="page_padding")
    )
    if corner:
        label(corner).place(grid[0], anchor="right")
    for index, (code, _) in enumerate(cells if header else [], start=1):
        label(code).place(grid[index])
    plays = []
    for row_index, (code, row) in enumerate(rows, start=header):
        first = row_index * len(tracks)
        label(code).place(grid[first], anchor="right")
        for index, (_, column) in enumerate(cells, start=1):
            zone = grid[first + index]
            plays.append(row(zone) if columns is None else build(zone, row, column))
    plays = [play for play in plays if hasattr(play, "duration")]
    scene.wait(HOLD if not plays else 0.4)
    if plays:
        scene.play(plays, duration=MOVE)
        scene.wait(HOLD)


def gallery(variants, rows=2, columns=2):
    """Lay out (label, build) pairs in a grid, each label under the variant ``build(zone)`` draws."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    for zone, (code, build) in zip(grid, variants):
        art, caption = zone.split(Zones.rows(["1fr", 0.45]))
        label(code).place(caption)
        build(art)
    scene.wait(HOLD)


def disc(zone, size=0.28):
    """A disc in the middle of ``zone``, ``size`` of its smaller side in radius, filled with the accent."""
    return scene.geometry.circle(min(zone.width, zone.height) * size).move_to(*zone.center)


def pill(zone, width=0.7, height=0.5):
    """A pill ``width`` and ``height`` fractions of ``zone``, in its middle, filled with the accent."""
    tall = zone.height * height
    return scene.geometry.rounded_rect(zone.width * width, tall, tall / 2).move_to(*zone.center)


def outline(zone):
    """A pill drawn as a stroke with no fill."""
    return pill(zone).no_fill().stroke(accent, 0.1)


def word(zone, text):
    """A large word in the middle of ``zone``."""
    return scene.text(text, size=min(zone.height * 0.7, zone.width / (0.6 * len(text)))).place(zone)


def coins(zone):
    """Three overlapping discs in one group, as tall as ``zone`` allows."""
    (x, y), radius = zone.center, min(zone.height * 0.32, zone.width / 9)
    return scene.geometry.group([scene.geometry.circle(radius).move_to(x + dx * radius, y) for dx in (-1.3, 0.0, 1.3)])


def tile(zone):
    """A light tile filling ``zone``, where shadows show."""
    return scene.geometry.rounded_rect(zone.width, zone.height, 0.12).fill(surface).move_to(*zone.center)


def card(zone):
    """A light card on a tile filling ``zone``."""
    tile(zone)
    return pill(zone, 0.6, 0.5).fill(foreground)


# glow(color, radius=0.16, intensity=1.0) adds a halo of ``color`` around the
# fill and stroke: radius is how far it reaches, in scene units, and
# intensity how strong it is.
glow_start = scene.cursor
scene.segment("glow")
table(
    [("radius=0.16", 0.16), ("radius=0.5", 0.5), ("radius=1.0", 1.0)],
    [("intensity=1.0", 1.0), ("intensity=2.0", 2.0), ("intensity=4.0", 4.0)],
    lambda zone, radius, intensity: disc(zone, 0.3).glow(accent, radius=radius, intensity=intensity),
    corner="disc.glow(accent, ...)",
    labels="16fr",
)

# Any drawable glows: a stroke, a text, a group, in any color.
glow_kinds_start = scene.cursor
scene.segment("glow_drawables")
gallery([
    ("outline.glow(accent, 0.4, 2.0)", lambda zone: outline(zone).glow(accent, 0.4, 2.0)),
    ("word.glow(accent, 0.4, 2.0)", lambda zone: word(zone, "Glow").glow(accent, 0.4, 2.0)),
    ("coins.glow(success, 0.4, 2.0)", lambda zone: coins(zone).glow(success, 0.4, 2.0)),
    ("word.glow(foreground, 0.3, 3.0)", lambda zone: word(zone, "Neon").fill(accent).glow(foreground, 0.3, 3.0)),
])

# blur(sigma=0.04) is a Gaussian blur of ``sigma`` scene units.
blur_start = scene.cursor
scene.segment("blur")
table(
    [("pill", pill), ("outline", outline), ("word", lambda zone: word(zone, "Blur")), ("coins", coins)],
    [(".blur()", None), (".blur(0.1)", 0.1), (".blur(0.3)", 0.3)],
    lambda zone, make, sigma: make(zone).blur() if sigma is None else make(zone).blur(sigma),
    labels="4fr",
)

# shadow(color, x=0.08, y=-0.08, blur=0.06) draws ``color`` offset by (x, y)
# and blurred by ``blur`` scene units, beneath the drawable.
shadow_start = scene.cursor
scene.segment("shadow")
table(
    [("x=0.08, y=-0.08", (0.08, -0.08)), ("x=0.3, y=-0.2", (0.3, -0.2)), ("x=0, y=-0.3", (0, -0.3))],
    [("blur=0", 0.0), ("blur=0.06", 0.06), ("blur=0.3", 0.3)],
    lambda zone, offset, blur: card(zone).shadow(shade, *offset, blur=blur),
    corner="card.shadow(shade, ...)",
    labels="16fr",
)

# The color's alpha scales the shadow. A text or a group casts one shadow
# from its silhouette, beneath all its members, so overlaps do not darken.
# no_effects() removes glow, blur and shadow, and keeps fill and stroke.
shadow_kinds_start = scene.cursor
scene.segment("shadow_drawables")
table(
    [
        ("card.shadow(faded(shade, 0.4), 0.2, -0.2)", lambda zone: card(zone).shadow(faded(shade, 0.4), 0.2, -0.2)),
        ("card.shadow(accent, 0.2, -0.2)", lambda zone: card(zone).shadow(accent, 0.2, -0.2)),
        ("word.shadow(shade, 0.08, -0.08)", lambda zone: (tile(zone), word(zone, "Shadow").shadow(shade, 0.08, -0.08))),
        ("coins.shadow(shade, 0.15, -0.15)", lambda zone: (tile(zone), coins(zone).shadow(shade, 0.15, -0.15))),
        ("card.shadow(shade).no_effects()", lambda zone: card(zone).shadow(shade).no_effects()),
    ],
    labels="15fr",
)


# animate.glow and animate.blur go from the current effect to a new one. A
# drawable without glow grows it from zero; glow(None) fades it out.
def glowing(build, start=None):
    """A row whose pill starts with a glow of radius ``start`` (or none) and plays ``build(pill)``."""

    def row(zone):
        orb = pill(zone, 0.7, 0.45)
        if start:
            orb.glow(accent, radius=start, intensity=2.0)
        return build(orb)
    return row


def blurred(sigma, target):
    """A row whose pill starts blurred by ``sigma`` (``0`` is sharp) and animates to ``target``."""

    def row(zone):
        return pill(zone, 0.7, 0.45).blur(sigma).animate.blur(target)
    return row


animate_start = scene.cursor
scene.segment("animate")
table(
    [
        ("orb.animate.glow(accent, 0.5, 2.0)", glowing(lambda orb: orb.animate.glow(accent, 0.5, 2.0))),
        (
            "orb.animate.glow(accent, 0.5, 2.0)\n    .repeat(3, yoyo=True)",
            glowing(lambda orb: orb.animate.glow(accent, 0.5, 2.0).duration(MOVE / 6).repeat(3, yoyo=True)),
        ),
        ("orb.glow(accent, 0.5, 2.0)\norb.animate.glow(None)", glowing(lambda orb: orb.animate.glow(None), start=0.5)),
        ("orb.blur(0.3)\norb.animate.blur(0.0)", blurred(0.3, 0.0)),
        ("orb.animate.blur(0.3)", blurred(0.0, 0.3)),
    ],
    labels="15fr",
)


# animate.shadow grows a new shadow from under the drawable, moves an existing
# one and, with None, fades it out. It combines with other targets in one Anim.
def shadowed(build, start=True):
    """A row whose card starts with the default shadow (or none) and plays ``build(card)``."""

    def row(zone):
        lifted = card(zone)
        if start:
            lifted.shadow(shade)
        return build(lifted)
    return row


shadow_motion_start = scene.cursor
scene.segment("animate_shadow")
table(
    [
        (
            "card.animate.shadow(shade, 0, -0.25, 0.3)\n    .scale_to(1.08)",
            shadowed(lambda lifted: lifted.animate.shadow(shade, 0, -0.25, 0.3).scale_to(1.08).easing(Easing.SMOOTH)),
        ),
        ("card.animate.shadow(shade, 0.3, -0.2)", shadowed(lambda lifted: lifted.animate.shadow(shade, 0.3, -0.2), start=False)),
        ("card.animate.shadow(None)", shadowed(lambda lifted: lifted.animate.shadow(None))),
    ],
    labels="15fr",
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The static segments, then the animated ones before, during and after their moves.
    scene.snapshots(
        snapshots,
        [start + 0.5 for start in (glow_start, glow_kinds_start, blur_start, shadow_start, shadow_kinds_start)]
        + [start + t for start in (animate_start, shadow_motion_start) for t in (0.2, 0.4 + MOVE / 2, 0.4 + MOVE + 0.5)],
    )
else:
    scene.render()
