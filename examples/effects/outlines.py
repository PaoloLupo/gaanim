"""Outlines: a sticker border traced from a drawable's alpha, on an image, text, a shape and a group.

``outline(color, width=0.06)`` draws a border ``width`` scene units wide
around what a drawable covers, not around its box: the opaque pixels of a
cutout image, the glyphs of a text, the members of a group. It is traced
after any ``shader_effect``, and a ``shadow`` is cast from the outlined
result, so the two make a sticker. Each label is the code of its variant.
"""

import os

from gaanim import PostProcess, Scene, Zones

BADGE = "assets/badge.png"

scene = Scene()
foreground = scene.canvas.color("foreground")
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
# Shadows are dark, so they show on a light surface: stickers sit on tiles.
shade = scene.canvas.color("background")
surface = scene.canvas.color("chart")
gray = PostProcess.color_grade(saturation=0.0)

grid = scene.layout.zones(Zones.grid(rows=2, columns=4, gap="48px", padding="page_padding"))


def badge(area):
    """The cutout badge, as large as ``area`` allows."""
    return scene.media.image(BADGE, height=min(area.width, area.height)).move_to(*area.center)


def word(area):
    """A word in the accent color, as wide as ``area``."""
    return scene.text("Sticker", size=area.width / 4.2, color=accent).move_to(*area.center)


def star(area):
    """A star filling ``area``."""
    return scene.geometry.star(5, min(area.width, area.height) * 0.5, 0.45).fill(success).move_to(*area.center)


def pair(area):
    """A disc and a square side by side, in one group."""
    (x, y), size = area.center, min(area.width / 2.4, area.height)
    return scene.geometry.group([
        scene.geometry.circle(size / 2).fill(success).move_to(x - size * 0.55, y),
        scene.geometry.rect(size, size).fill(accent).move_to(x + size * 0.55, y),
    ])


# outline traces the drawable's alpha; width is in scene units. A shadow is
# cast from the outlined drawable, and a shader effect runs before the
# outline is traced, so it outlines what the effect leaves.
VARIANTS = [
    ("badge.outline(foreground)", lambda area: badge(area).outline(foreground)),
    ("badge.outline(foreground,\n    width=0.15)", lambda area: badge(area).outline(foreground, width=0.15)),
    ("badge.outline(foreground)\n    .shadow(shade)", lambda area: badge(area).outline(foreground).shadow(shade)),
    ("badge.shader_effect(gray)\n    .outline(foreground)", lambda area: badge(area).shader_effect(gray).outline(foreground)),
    ("word.outline(foreground)", lambda area: word(area).outline(foreground)),
    ("word.outline(foreground)\n    .shadow(shade)", lambda area: word(area).outline(foreground).shadow(shade)),
    ("star.outline(foreground)\n    .shadow(shade)", lambda area: star(area).outline(foreground).shadow(shade)),
    ("pair.outline(shade)", lambda area: pair(area).outline(shade)),
]
# Labels share one size: lines of up to 26 characters fill a column.
label_size = grid[0].width / 16
for zone, (code, build) in zip(grid, VARIANTS):
    stage, caption = zone.split(Zones.rows(["4fr", "1fr"], gap="16px"))
    scene.geometry.rounded_rect(stage.width, stage.height, 0.12).fill(surface).place(stage)
    build(stage.inset("40px"))
    scene.text(code, role="code", size=label_size).place(caption, anchor="top_left")
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5])
else:
    scene.render()
