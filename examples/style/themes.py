"""Built-in themes: the palette of every theme Gaanim ships, side by side on the same small composition.

A theme is scene-wide: ``Scene(theme=...)`` or ``scene.canvas.set_theme(...)``
restyles every object of the scene at once. To compare the themes in one
scene, each card here paints its parts with ``Theme(name).color(role)``, the
colors that theme gives them: its background, title, foreground and muted
text, panel and header, rule, the accent that fills unstyled shapes and the
chart color that strokes plots. The four bars are the theme's series
palette, [chart, accent, title, muted]. Each label is the code that selects
the theme.
"""

import math
import os

from gaanim import Scene, Theme, Zones

GROW = 0.8
HOLD = 1.4

scene = Scene()
muted = scene.canvas.color("muted")

ALIASES = [
    ("scientific", "technical"),
    ("deck", "presentation"),
    ("light", "paper"),
    ("gruvbox", "gruvbox-dark"),
    ("tokyo", "tokyo-night"),
    ("catppuccin", "catppuccin-mocha"),
    ("mocha", "catppuccin-mocha"),
    ("latte", "catppuccin-latte"),
]


def label(code):
    """A variant label: the code, in the code role."""
    return scene.text(code, role="code", markup=False)


def text_column(zone, theme):
    """The text half of a card: a heading, body and caption text, and a panel with a header."""
    color = theme.color
    heading, body, caption, panel = zone.split(Zones.rows(["5fr", "3fr", "3fr", "8fr"], gap="10px"))
    parts = [
        scene.text("Heading", size=0.56, color=color("title")).place(heading, anchor="bottom_left"),
        scene.text("Body text", size=0.34, color=color("foreground")).place(body, anchor="left"),
        scene.text("A muted caption", size=0.28, color=color("muted")).place(caption, anchor="left"),
    ]
    box = scene.geometry.rounded_rect(panel.width, panel.height, 0.1).fill(color("panel")).stroke(color("rule"), 0.02)
    parts.append(box.place(panel))
    strip, inside = panel.split(Zones.rows(["2fr", "3fr"]))
    parts.append(scene.geometry.rect(strip.width - 0.04, strip.height - 0.02).fill(color("header")).place(strip))
    parts.append(scene.text("header", size=0.26, color=color("foreground")).place(strip.inset((0, 0.2)), anchor="left"))
    parts.append(scene.text("panel", size=0.26, color=color("muted")).place(inside.inset((0, 0.2)), anchor="left"))
    return parts


def chart_column(zone, theme):
    """The chart half of a card: a grid, axes, four bars in the series colors and a plotted wave."""
    color = theme.color
    plot = zone.inset((0.1, 0.1, 0.1, 0.2))
    left, bottom, width, height = plot.left, plot.bottom, plot.width, plot.height
    parts = [
        scene.geometry.line((left, bottom + height * k / 4), (left + width, bottom + height * k / 4)).stroke(
            color("rule"), 0.015
        )
        for k in range(1, 5)
    ]
    series = [color("chart"), color("accent"), color("title"), color("muted")]
    for index, (paint, value) in enumerate(zip(series, [0.55, 0.85, 0.4, 0.7])):
        x = left + width * (index + 0.75) / 4.5
        parts.append(scene.geometry.rect(width / 7, height * value).fill(paint).move_to(x, bottom + height * value / 2))
    wave = [
        (left + width * k / 40, bottom + height * (0.5 + 0.32 * math.sin(k / 40 * math.tau * 1.2)))
        for k in range(41)
    ]
    parts.append(scene.geometry.polyline(wave).no_fill().stroke(color("chart"), 0.05))
    parts.append(scene.geometry.line((left, bottom), (left + width, bottom)).stroke(color("foreground"), 0.03))
    parts.append(scene.geometry.line((left, bottom), (left, bottom + height)).stroke(color("foreground"), 0.03))
    parts.append(scene.geometry.circle(0.22).fill(color("accent")).move_to(left + width - 0.3, bottom + height - 0.3))
    return parts


def card(zone, name):
    """One theme's card in ``zone``, under its label; returns the card's parts."""
    theme = Theme(name)
    caption, body = zone.split(Zones.rows(["44px", "1fr"], gap="12px"))
    label(f'Scene(theme="{name}")').place(caption, anchor="left")
    back = scene.geometry.rounded_rect(body.width, body.height, 0.16).fill(theme.color("background"))
    parts = [back.stroke(theme.color("rule"), 0.03, align="center").place(body)]
    words, chart = body.inset((0.3, 0.35)).split(Zones.columns(["11fr", "9fr"], gap="36px"))
    return parts + text_column(words, theme) + chart_column(chart, theme)


def aliases(zone):
    """The aliases each built-in theme answers to, as a two-column list under a label."""
    caption, body = zone.split(Zones.rows(["44px", "1fr"], gap="12px"))
    label("Scene(theme=alias)").place(caption, anchor="left")
    grid = body.split(Zones.grid(rows=len(ALIASES), columns=["2fr", "3fr"], column_gap="24px"))
    parts = []
    for index, (alias, name) in enumerate(ALIASES):
        parts.append(label(f'"{alias}"').place(grid[2 * index], anchor="right"))
        parts.append(label(f'→ "{name}"').fill(muted).place(grid[2 * index + 1], anchor="left"))
    return parts


def page(names, extra=None):
    """Lay out up to four cards in a 2 x 2 grid, ``extra(zone)`` filling a free cell, and fade them in."""
    grid = scene.layout.zones(Zones.grid(rows=2, columns=2, column_gap="72px", row_gap="36px", padding="page_padding"))
    parts = []
    for zone, name in zip(grid, names):
        parts += card(zone, name)
    if extra is not None:
        parts += extra(grid[len(names)])
    scene.play([part.animate.fade_in() for part in parts], duration=GROW)
    scene.wait(HOLD)


# Theme.schemes() lists the eleven built-in themes, without aliases.
SCHEMES = Theme.schemes()
scene.segment("neutral_dark")
page(SCHEMES[:2] + ["dracula", "nord"])
scene.segment("editor_dark")
page(["solarized-dark", "gruvbox-dark", "tokyo-night", "catppuccin-mocha"])
scene.segment("light")
page(["paper", "solarized-light", "catppuccin-latte"], extra=aliases)

STEP = GROW + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Fading in and settled, in each of the three segments.
    scene.snapshots(snapshots, [STEP * index + t for index in range(3) for t in (0.4, 1.8)])
else:
    scene.render()
