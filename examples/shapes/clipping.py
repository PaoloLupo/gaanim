"""Clipping: vector masks with their fill rules, and fills that rise to a level.

The first segment clips a group of bars and a word to moving masks, drawn
as dashed outlines. The second fills shapes to an animated level from each
direction, with or without their outline, and from a value that follows the
scene clock. Each label is the call that builds the variant.
"""

import math
import os

from gaanim import Scene, StrokeStyle, Zones, computed

MOVE = 1.6
HOLD = 0.6

scene = Scene()
accent = scene.colors.accent
muted = scene.colors.muted
OUTLINE = StrokeStyle(muted, 0.02, dashes=[0.08, 0.06])

# A five-pointed star drawn as one self-crossing polygon.
PENTAGRAM = [(math.cos(math.pi / 2 + k * 0.8 * math.pi), math.sin(math.pi / 2 + k * 0.8 * math.pi)) for k in range(5)]


def outline(shape):
    """Show a mask as a dashed outline, above what it clips."""
    return shape.no_fill().stroke_style(OUTLINE).z_index(1)


def make_bars(zone):
    """A group of 14 vertical bars filling the middle of ``zone``."""
    x, y = zone.center
    return scene.geometry.group([
        scene.geometry.rect(0.2, 1.6).fill(accent if index % 2 else muted).move_to(x + 0.24 * (index - 6.5), y)
        for index in range(14)
    ])


def gallery(variants):
    """Build each (label, build) pair in a 2 x 3 grid, above its label.

    ``build(zone)`` draws the variant and returns the animation of its mask.
    """
    grid = scene.layout.zones(Zones.grid(rows=3, columns=2, gap="48px", padding="page_padding"))
    moves = []
    for zone, (label, build) in zip(grid, variants):
        art, caption = zone.split(Zones.rows(["1fr", 0.5]))
        scene.text(label, role="code").place(caption)
        moves.append(build(art))
    scene.wait(HOLD)
    scene.play(moves, duration=MOVE)
    scene.wait(HOLD)


def lens(zone, dx):
    """A circular mask left of the center of ``zone``, which will slide ``dx`` across."""
    x, y = zone.center
    return outline(scene.geometry.circle(0.75).move_to(x - dx / 2, y))


def clip_circle(zone, **options):
    """Bars clipped to a sliding circle; ``options`` go to ``clip``."""
    mask = lens(zone, 2)
    make_bars(zone).clip(mask, **options)
    return mask.animate.shift_by(2, 0)


def clip_star(zone, **options):
    """Bars clipped to a turning self-crossing star; ``options`` go to ``clip``."""
    x, y = zone.center
    star = outline(scene.geometry.polygon(PENTAGRAM).scale_by(0.85).move_to(x, y))
    make_bars(zone).clip(star, **options)
    return star.animate.rotate_by(math.pi / 5)


def clip_word(zone):
    """A word clipped to a sliding circle."""
    mask = lens(zone, 2)
    scene.text("CLIP", size=1.4).place(zone).clip(mask)
    return mask.animate.shift_by(2, 0)


def word_mask(zone):
    """Bars clipped to the glyphs of a word, which sways."""
    word = outline(scene.text("CLIP", size=1.4).place(zone))
    make_bars(zone).clip(word)
    return word.animate.shift_by(0.4, 0)


# A mask can be any vector drawable, text included, and keeps following its
# own motion. invert keeps what lies outside it; rule="evenodd" leaves the
# middle of a self-crossing mask out. no_clip() removes a clip.
scene.segment("masks")
gallery([
    ("bars.clip(mask)", clip_circle),
    ("bars.clip(mask, invert=True)", lambda zone: clip_circle(zone, invert=True)),
    ("bars.clip(star)", clip_star),
    ('bars.clip(star, rule="evenodd")', lambda zone: clip_star(zone, rule="evenodd")),
    ("word.clip(mask)", clip_word),
    ("bars.clip(word)", word_mask),
])

# fill_level fills a mask up to a level in [0, 1], growing in its direction
# (up by default), here animated with animate.fill_level(0.7). The mask stays
# hidden; keep_outline draws its stroke. A level can also follow a Parameter
# or a Computed, and set_fill_level() rebinds or fixes it later.
scene.segment("fill_level")
wave = computed(lambda t: 0.5 + 0.35 * math.sin(3 * t), inputs=[scene.time])
LEVELS = [
    ("fill_level(mask, accent)", {}),
    ('fill_level(mask, accent, direction="down")', {"direction": "down"}),
    ('fill_level(mask, accent, direction="left")', {"direction": "left"}),
    ('fill_level(mask, accent, direction="right")', {"direction": "right"}),
    ("fill_level(mask, accent, keep_outline=False)", {"keep_outline": False}),
    ("fill_level(mask, accent, wave)", {"level": wave}),
]
grid = scene.layout.zones(Zones.grid(rows=len(LEVELS), columns=["3fr", "1fr"], gap="24px", padding="page_padding"))
fills = []
for index, (label, options) in enumerate(LEVELS):
    scene.text(label, role="code").place(grid[2 * index], anchor="right")
    mask = scene.geometry.rounded_rect(2.6, 0.8, 0.4).no_fill().stroke(muted, 0.03).opacity(0)
    mask.place(grid[2 * index + 1])
    level = scene.geometry.fill_level(mask, accent, **options)
    if "level" not in options:
        fills.append(level.animate.fill_level(0.7))
scene.wait(HOLD)
scene.play(fills, duration=MOVE)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Before, while and after the masks move, then the levels empty, rising and full.
    scene.snapshots(snapshots, [0.3, 1.4, 2.7, 3.1, 4.2, 5.5])
else:
    scene.render()
