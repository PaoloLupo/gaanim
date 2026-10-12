"""Blend modes: how a drawable mixes with what is drawn beneath it, in each of the seventeen modes.

In every cell a disc crosses the same base, a band of the theme's hues over a
band from dark to light, so only the mode changes. The disc starts half off
the base, where it blends with the plain background. The last segment blends
whole groups, gives one member of a group its own mode, and blends text. Each
label is the call that sets the mode.
"""

import os

from gaanim import Brush, Scene, Zones

GROW = 0.6
CROSS = 2.0
HOLD = 1.0

scene = Scene()
background = scene.canvas.color("background")
foreground = scene.canvas.color("foreground")
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")

BAND = 0.7
DISC = Brush.radial([foreground, accent, danger], radius=0.62)


def base(zone, width):
    """The base of a cell, ``width`` wide: the hue band over the gray band, centered in ``zone``."""
    x, y = zone.center
    hues = Brush.linear([success, accent, danger], start=(-width / 2, 0), end=(width / 2, 0))
    grays = Brush.linear([background, foreground], start=(-width / 2, 0), end=(width / 2, 0))
    return [
        scene.geometry.rect(width, BAND).fill(hues).move_to(x, y + BAND / 2),
        scene.geometry.rect(width, BAND).fill(grays).move_to(x, y - BAND / 2),
    ]


def cells(variants, columns=3, width=3.6, caption="48px"):
    """One cell per (label, build): a base ``width`` wide, then what build(x, y) returns at its left end.

    The movers grow in, cross their bases together, and hold.
    """
    rows = -(-len(variants) // columns)
    grid = scene.layout.zones(
        Zones.grid(rows=rows, columns=columns, column_gap="72px", row_gap="36px", padding="page_padding")
    )
    bases, movers = [], []
    for cell, (code, build) in zip(grid, variants):
        art, text = cell.split(Zones.rows(["1fr", caption], gap="12px"))
        scene.text(code, role="code").place(text)
        bases += base(art, width)
        x, y = art.center
        movers.append(build(x - width / 2, y))
    scene.play(
        [shape.animate.fade_in() for shape in bases] + [mover.animate.grow_from_center() for mover in movers],
        duration=GROW,
    )
    scene.play([mover.animate.shift_by(width, 0) for mover in movers], duration=CROSS)
    scene.wait(HOLD)


def disc(mode):
    """A builder of the disc blended with ``mode``."""
    return lambda x, y: scene.geometry.circle(0.62).fill(DISC).no_stroke().move_to(x, y).blend(mode)


def trio(x, y):
    """Three overlapping discs, a, b and c, in success, accent and danger, grouped around ``(x, y)``."""
    discs = [
        scene.geometry.circle(0.42).fill(paint).no_stroke().move_to(x + dx, y + dy)
        for paint, (dx, dy) in zip([success, accent, danger], [(-0.26, -0.15), (0, 0.3), (0.26, -0.15)])
    ]
    return scene.geometry.group(discs), discs


def group_blend(mode, override=None):
    """A builder of the trio blended with ``mode``; ``override`` is (member index, its own mode)."""

    def build(x, y):
        group, discs = trio(x, y)
        group.blend(mode)
        if override is not None:
            # Set after the group's mode, the member's own mode wins.
            index, own = override
            discs[index].blend(own)
        return group

    return build


def word(mode, paint):
    """A builder of a bold word in ``paint`` blended with ``mode``."""
    return lambda x, y: scene.text("BLEND", size=0.6, weight=800, color=paint).move_to(x, y).blend(mode)


# Separable modes mix each color channel on its own: multiply and the
# darkening modes only darken, screen and the lightening modes only lighten,
# and overlay, hard_light and soft_light do one or the other by brightness.
scene.segment("separable")
SEPARABLE = ["normal", "multiply", "screen", "overlay", "darken", "lighten", "color_dodge", "color_burn", "hard_light"]
cells([(f'.blend("{mode}")', disc(mode)) for mode in SEPARABLE])

# The non-separable modes take hue, saturation or luminosity from the disc
# and the rest from the base; "add" sums light. The last cell is the same
# disc in "normal" again, for comparison.
scene.segment("non_separable")
NON_SEPARABLE = ["soft_light", "difference", "exclusion", "hue", "saturation", "color", "luminosity", "add", "normal"]
cells([(f'.blend("{mode}")', disc(mode)) for mode in NON_SEPARABLE])

# On a group the mode reaches every member: trio = group([a, b, c]) blends
# its three discs with the base and with each other. A member that sets its
# own mode afterwards keeps it, and on a text every glyph blends.
scene.segment("groups_and_text")
cells(
    [
        ('trio.blend("screen")', group_blend("screen")),
        ('trio.blend("multiply")', group_blend("multiply")),
        ('trio.blend("screen")\nb.blend("normal")', group_blend("screen", (1, "normal"))),
        ('trio.blend("multiply")\nc.blend("difference")', group_blend("multiply", (2, "difference"))),
        ('scene.text("BLEND")\n  .blend("difference")', word("difference", foreground)),
        ('scene.text("BLEND", color=accent)\n  .blend("multiply")', word("multiply", accent)),
    ],
    columns=2,
    width=4.2,
    caption="88px",
)

STEP = GROW + CROSS + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment with its movers half off the base, halfway across and at the far end.
    scene.snapshots(snapshots, [STEP * index + t for index in range(3) for t in (0.7, 1.6, 3.4)])
else:
    scene.render()
