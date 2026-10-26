"""Glass: frosted and liquid glass over what is drawn behind, every parameter, and backdrop blur.

A drawable turned into glass shows what lies behind it blurred, saturated and
bent at its rim, then draws itself on top, so a faint fill tints the glass.
In every panel a glass shape slides over stripes and a word. The first two
segments vary the parameters of ``glass``, frosted and then clear to show
its rim, the third shows ``liquid_glass`` and its options, also on melting
metaballs, and the last plain ``backdrop_blur`` and ``no_glass``. Each label
is the code of its variant.
"""

import os

from gaanim import Scene, Zones

MOVE = 1.6
HOLD = 0.6

scene = Scene()
foreground = scene.colors.foreground
background = scene.colors.background
COLORS = [scene.colors[role] for role in ("accent", "success", "danger", "chart")]


def faded(color, alpha):
    """``color`` at ``alpha`` (0 to 1) opacity, for a fill that only tints the glass."""
    return (color.r, color.g, color.b, round(alpha * 255))


TINT = faded(foreground, 0.08)
FRAME = faded(foreground, 0.4)


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code")


def gallery(rows, columns, caption=0.45):
    """The zones of a grid of panels, each split into its art and the caption under it."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="60px", padding="page_padding"))
    return [zone.split(Zones.rows(["1fr", caption])) for zone in grid]


def backdrop(zone):
    """Thin stripes of theme colors across ``zone`` and a dark word over them."""
    count = 18
    width = zone.width / count
    x, y = zone.center
    for index in range(count):
        scene.geometry.rect(width, zone.height * 0.8).fill(COLORS[index % len(COLORS)]).move_to(x + width * (index - (count - 1) / 2), y)
    scene.text("GLASS", size=zone.height * 0.55, weight=800).fill(background).place(zone)


def card(zone):
    """A rounded card left of the middle of ``zone``, tinted faintly, and its slide to the right."""
    width, height = zone.width * 0.42, zone.height * 0.9
    (left, y), (right, _) = zone.point("left"), zone.point("right")
    shape = scene.geometry.rounded_rect(width, height, height * 0.25).fill(TINT).stroke(FRAME, 0.03)
    shape.move_to(left + width * 0.6, y)
    return shape, shape.animate.move_to(right - width * 0.6, y)


def panels(variants, rows, columns, caption=0.45):
    """Draw each (label, build) variant on a backdrop, where ``build(zone)`` returns its slide."""
    slides = []
    for (art, text_zone), (code, build) in zip(gallery(rows, columns, caption), variants):
        label(code).place(text_zone)
        backdrop(art)
        slide = build(art)
        slides.extend(slide if isinstance(slide, list) else [slide])
    scene.wait(HOLD)
    scene.play(slides, duration=MOVE)
    scene.wait(HOLD)


def glassy(*args, **options):
    """A card turned into glass with ``glass(*args, **options)``."""

    def build(zone):
        shape, slide = card(zone)
        shape.glass(*args, **options)
        return slide
    return build


# glass(blur=0.25, saturation=1.4, ..., transparency=1): blur and saturation
# act on what is behind the glass, and transparency below 1 turns it milky.
frosted_start = scene.cursor
scene.segment("frosted")
panels(
    [
        ("glass()", glassy()),
        ("glass(blur=0.6)", glassy(blur=0.6)),
        ("glass(saturation=0.0)", glassy(saturation=0.0)),
        ("glass(transparency=0.4)", glassy(transparency=0.4)),
    ],
    rows=2,
    columns=2,
)

# Clear glass (blur 0) shows the rim: bevel scene units wide, it bends what
# is behind by up to refraction; edge is a thin light on the outline.
rim_start = scene.cursor
scene.segment("rim")
panels(
    [
        ("glass(0)", glassy(0)),
        ("glass(0, refraction=0.5)", glassy(0, refraction=0.5)),
        ("glass(0, refraction=0.5,\n      bevel=0.6)", glassy(0, refraction=0.5, bevel=0.6)),
        ("glass(0, edge=1.0)", glassy(0, edge=1.0)),
    ],
    rows=2,
    columns=2,
    caption=0.9,
)

# Across the rim, dispersion splits what is bent into its colors and twist
# slides it along the outline, clockwise when positive.
fringes_start = scene.cursor
scene.segment("fringes")
panels(
    [
        ("glass(0, refraction=0.5,\n      bevel=0.4)", glassy(0, refraction=0.5, bevel=0.4)),
        ("glass(0, refraction=0.5,\n      bevel=0.4, dispersion=1.0)", glassy(0, refraction=0.5, bevel=0.4, dispersion=1.0)),
        ("glass(0, refraction=0.5,\n      bevel=0.4, twist=2.0)", glassy(0, refraction=0.5, bevel=0.4, twist=2.0)),
        ("glass(0, refraction=0.5,\n      bevel=0.4, twist=-2.0)", glassy(0, refraction=0.5, bevel=0.4, twist=-2.0)),
    ],
    rows=2,
    columns=2,
    caption=0.9,
)


# liquid_glass is glass with Liquid Glass defaults: almost no blur, a wide
# rim that bends strongly and slides along the outline, faint color fringes
# and a bright outline. On metaballs the drops keep their lens as they melt.
def liquid(**options):
    """A disc of liquid glass with ``options`` that slides across."""

    def build(zone):
        radius = zone.height * 0.45
        (left, y), (right, _) = zone.point("left"), zone.point("right")
        lens = scene.geometry.circle(radius).fill(TINT).move_to(left + radius * 1.5, y).liquid_glass(**options)
        return lens.animate.move_to(right - radius * 1.5, y)
    return build


def melting(zone):
    """Two drops of liquid glass that meet in the middle of ``zone``."""
    radius = zone.height * 0.4
    (left, y), (right, _) = zone.point("left"), zone.point("right")
    drops = [scene.geometry.circle(radius).move_to(left + radius * 1.5, y), scene.geometry.circle(radius * 0.8).move_to(right - radius * 1.5, y)]
    scene.geometry.metaballs(drops, smoothness=0.9).fill(TINT).liquid_glass()
    x, _ = zone.center
    return [drops[0].animate.move_to(x - radius * 0.6, y), drops[1].animate.move_to(x + radius * 0.6, y)]


liquid_start = scene.cursor
scene.segment("liquid_glass")
panels(
    [
        ("liquid_glass()", liquid()),
        ("liquid_glass(twist=-0.6)", liquid(twist=-0.6)),
        ("liquid_glass(dispersion=0.6)", liquid(dispersion=0.6)),
        ("metaballs(drops).liquid_glass()", melting),
    ],
    rows=2,
    columns=2,
)


# backdrop_blur(radius=0.25, saturation=1.0) only blurs what is behind, like
# glass without refraction or edge; no_glass() removes glass or backdrop blur.
def blurring(*args, **options):
    """A card that blurs its backdrop with ``backdrop_blur(*args, **options)``."""

    def build(zone):
        shape, slide = card(zone)
        shape.backdrop_blur(*args, **options)
        return slide
    return build


def plain(zone):
    """A card whose glass is removed again, so only its faint tint shows."""
    shape, slide = card(zone)
    shape.glass().no_glass()
    return slide


backdrop_start = scene.cursor
scene.segment("backdrop_blur")
panels(
    [
        ("backdrop_blur()", blurring()),
        ("backdrop_blur(0.6)", blurring(0.6)),
        ("backdrop_blur(0.3, saturation=0)", blurring(0.3, saturation=0)),
        ("glass().no_glass()", plain),
    ],
    rows=2,
    columns=2,
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment before, during and after its glass slides.
    scene.snapshots(
        snapshots,
        [
            start + t
            for start in (frosted_start, rim_start, fringes_start, liquid_start, backdrop_start)
            for t in (0.3, HOLD + MOVE / 2, HOLD + MOVE + 0.3)
        ],
    )
else:
    scene.render()
