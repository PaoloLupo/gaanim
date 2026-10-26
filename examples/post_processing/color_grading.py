"""Color grading: exposure, contrast, saturation and temperature, a 3D lookup table, tint and duotone.

Each segment grades the same test card through one full-frame pass, set with
``scene.segment(..., post=...)``: the tiles show hue and saturation, the ramp
below them shows the tone curve. The first pass leaves every color as it is.
Each label is the post-process of its segment.
"""

import os

from gaanim import Brush, PostProcess, Scene, Zones

HOLD = 0.8
LUT = "assets/teal_orange.cube"

scene = Scene()
background = scene.colors.background
foreground = scene.colors.foreground
accent = scene.colors.accent
page = scene.layout.zones(Zones.rows(["6fr", "1fr"], gap="24px", padding="page_padding"))


def test_card(zone):
    """Fill ``zone`` with four colored tiles, each holding a dark shape, above a gray ramp."""
    tiles, ramp = zone.split(Zones.rows(["4fr", "1fr"], gap="24px"))
    cells = tiles.split(Zones.columns(4, gap="24px"))
    roles = ["accent", "success", "danger", "chart"]
    shapes = [
        lambda: scene.geometry.circle(1.0),
        lambda: scene.geometry.regular_polygon(3, 1.0),
        lambda: scene.geometry.star(5, 1.0, 0.45),
        lambda: scene.geometry.annulus(1.0, 0.55),
    ]
    for cell, role, shape in zip(cells, roles, shapes):
        scene.geometry.rounded_rect(cell.width, cell.height, 0.2).fill(scene.colors[role]).place(cell)
        shape().fill(background).place(cell.inset("48px"), fit="contain")
    (left, y), (right, _) = ramp.point("left"), ramp.point("right")
    gray = Brush.linear([background, foreground], start=(left, y), end=(right, y))
    scene.geometry.rounded_rect(ramp.width, ramp.height, 0.2).fill(gray).place(ramp)


def grade(code, post):
    """One segment of the test card through ``post``, labelled with ``code``."""
    scene.segment(code, post=post)
    test_card(page[0])
    scene.text(code, role="code").place(page[1], anchor="left")
    scene.wait(HOLD)


# color_grade works in linear light: exposure is in stops and temperature
# runs from -1 (cool) to 1 (warm); then contrast pivots about mid-gray and
# saturation about luma. 1 keeps contrast and saturation, 0 the rest.
GRADES = [
    ("PostProcess.color_grade()", PostProcess.color_grade()),
    ("PostProcess.color_grade(exposure=1.0)", PostProcess.color_grade(exposure=1.0)),
    ("PostProcess.color_grade(exposure=-1.0)", PostProcess.color_grade(exposure=-1.0)),
    ("PostProcess.color_grade(contrast=1.8)", PostProcess.color_grade(contrast=1.8)),
    ("PostProcess.color_grade(contrast=0.5)", PostProcess.color_grade(contrast=0.5)),
    ("PostProcess.color_grade(saturation=0.0)", PostProcess.color_grade(saturation=0.0)),
    ("PostProcess.color_grade(saturation=2.0)", PostProcess.color_grade(saturation=2.0)),
    ("PostProcess.color_grade(temperature=1.0)", PostProcess.color_grade(temperature=1.0)),
    ("PostProcess.color_grade(temperature=-1.0)", PostProcess.color_grade(temperature=-1.0)),
    # A .cube lookup table maps every color to another; strength blends it
    # with the original.
    ("PostProcess.lut(LUT)", PostProcess.lut(LUT)),
    ("PostProcess.lut(LUT, strength=0.5)", PostProcess.lut(LUT, strength=0.5)),
    # tint turns the frame into a monochrome in one color, keeping its
    # brightness; duotone maps it from a shadow color to a highlight color.
    # amount blends either with the original.
    ("PostProcess.tint(accent)", PostProcess.tint(accent)),
    ("PostProcess.tint(accent, amount=0.5)", PostProcess.tint(accent, amount=0.5)),
    ("PostProcess.duotone(background, accent)", PostProcess.duotone(background, accent)),
]
for code, post in GRADES:
    grade(code, post)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The middle of every segment.
    scene.snapshots(snapshots, [HOLD * (index + 0.5) for index in range(len(GRADES))])
else:
    scene.render()
