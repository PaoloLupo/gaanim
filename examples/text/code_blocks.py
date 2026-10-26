"""Code blocks: syntax highlighting by language, color options, a .tmTheme, fonts and sizes.

``scene.text.code(source, language=...)`` draws a code panel: the language
name over a rule and the highlighted source under it, in JetBrains Mono,
bundled with Gaanim. Token colors come from the theme, from a ``.tmTheme``
file or from the ``color``/``accent``/``background`` options. Each label is
the option that builds the panel under it.
"""

import os

from gaanim import Scene, Zones

FADE = 0.6
HOLD = 1.0
PANEL = {"width": 6.6, "height": 2.0, "font_size": 0.26}

PYTHON = "def area(radius):\n    # the area of a circle\n    return 3.14159 * radius ** 2"
SOURCES = {
    "python": PYTHON,
    "rust": "fn area(radius: f64) -> f64 {\n    // the area of a circle\n    PI * radius * radius\n}",
    "js": "const area = (radius) =>\n  // the area of a circle\n  Math.PI * radius ** 2;",
    "typst": "#let area(r) = calc.pi * r * r\n= Circles\nA unit circle has area #area(1).",
    "json": '{\n  "shape": "circle",\n  "radius": 2\n}',
    "text": "An unknown language,\nsuch as text, stays plain.",
}

scene = Scene()
success = scene.colors.success
muted = scene.colors.muted
foreground = scene.colors.foreground


def python(**options):
    """The Python sample as a panel: PANEL with ``options`` on top."""
    return scene.text.code(PYTHON, language="python", **(PANEL | options))


def panels(variants, rows):
    """Fade in one code panel per cell of a rows x 2 grid, each under its label.

    ``variants`` are (label, panel) pairs.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=rows, columns=2, column_gap="96px", row_gap="36px", padding="page_padding")
    )
    for cell, (code, panel) in zip(grid, variants):
        strip, stage = cell.split(Zones.rows(["48px", "1fr"], gap="12px"))
        scene.text(code, role="code").place(strip, anchor="left")
        panel.place(stage, anchor="left")
    scene.play([panel.animate.fade_in().duration(FADE) for _, panel in variants])
    scene.wait(HOLD)


# language is a syntax name or a file extension. Every panel here is
# scene.text.code(source, language=..., **PANEL).
scene.segment("languages")
panels(
    [
        (f'language="{language}"', scene.text.code(source, language=language, **PANEL))
        for language, source in SOURCES.items()
    ],
    rows=3,
)

# highlight=False draws every token in one color; a .tmTheme file brings its
# own token and panel colors; color is the plain text, accent the border, rule
# and language name. On a background where the theme's colors would not read,
# the text and tokens shift until they do. Each label is the change from PANEL.
scene.segment("colors")
panels(
    [
        ("highlight=False", python(highlight=False)),
        ('theme="assets/paper.tmTheme"', python(theme="assets/paper.tmTheme")),
        ("accent=success, color=muted", python(accent=success, color=muted)),
        ("background=foreground", python(background=foreground)),
    ],
    rows=2,
)

# font picks the monospaced family (Typst's bundled DejaVu Sans Mono here);
# font_size scales the code, drawn at 80 % of it, inside a width x height
# panel.
scene.segment("sizes")
panels(
    [
        ('font="DejaVu Sans Mono"', python(font="DejaVu Sans Mono")),
        ("font_size=0.2", python(font_size=0.2)),
        ("font_size=0.32", python(font_size=0.32)),
        ("width=4.8, height=2.6", python(width=4.8, height=2.6)),
    ],
    rows=2,
)

STEP = FADE + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Fading in and settled in each segment.
    scene.snapshots(snapshots, [STEP * index + t for index in range(3) for t in (0.3, 1.2)])
else:
    scene.render()
