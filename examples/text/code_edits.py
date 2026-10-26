"""Code edits: one source turned into the next token by token, then lines and words picked out.

``scene.text.code`` returns a ``Code``. ``code.animate.edit(source)`` keeps
the lines that only move or change indentation, slides the tokens a changed
line keeps and fades the rest; ``code.animate.select`` dims every line but
some, and ``code.animate.highlight`` tints a word and back. The label above
the panel is the call that plays.
"""

import os

from gaanim import Scene, Zones

FADE = 0.5
EDIT = 1.0
SELECT = 0.6
HOLD = 0.8
PANEL = {"language": "python", "width": 8.8, "height": 3.6, "font_size": 0.32}

LOOP = "def total(prices):\n    result = 0\n    for p in prices:\n        result = result + p\n    return result"
TAXED = "def total(prices, tax):\n    result = 0\n    for p in prices:\n        result += p\n    return result * (1 + tax)"
SHORT = "def total(prices, tax):\n    return sum(prices) * (1 + tax)"

scene = Scene()


def stage():
    """A label strip over a stage, padded by the theme's page padding."""
    return scene.layout.zones(Zones.rows(["72px", "1fr"], gap="24px", padding="page_padding"))


def show(source, call):
    """Fade in a code panel and the label above it; return both."""
    strip, area = stage()
    label = scene.text(call, role="code").place(strip)
    code = scene.text.code(source, **PANEL).place(area)
    scene.play([label.animate.fade_in().duration(FADE), code.animate.fade_in().duration(FADE)])
    return label, code


def step(label, call, anim, duration):
    """Show ``call`` above the panel and play ``anim``, then hold."""
    label.become(call, role="code")
    scene.play([anim.duration(duration)])
    scene.wait(HOLD)


# Each edit starts from the code the previous one left.
scene.segment("edits")
label, code = show(LOOP, "scene.text.code(LOOP)")
scene.wait(HOLD)
step(label, "code.animate.edit(TAXED)", code.animate.edit(TAXED), EDIT)
step(label, "code.animate.edit(SHORT)", code.animate.edit(SHORT), EDIT)
EDITS = FADE + HOLD + 2 * (EDIT + HOLD)

# select dims everything else; select() brings every line back.
scene.segment("selections")
label, code = show(LOOP, "scene.text.code(LOOP)")
scene.wait(HOLD)
step(label, "code.animate.select(lines=(3, 4))", code.animate.select(lines=(3, 4)), SELECT)
step(label, 'code.animate.select(text="result")', code.animate.select(text="result"), SELECT)
step(label, "code.animate.select()", code.animate.select(), SELECT)
step(label, 'code.animate.highlight("p")', code.animate.highlight("p"), EDIT)

# Before, halfway through and after each edit; then each selection settled
# and the highlight at its peak.
START = FADE + HOLD
SECOND = START + EDIT + HOLD
PICKS = [START + (SELECT + HOLD) * index + SELECT + 0.2 for index in range(3)]
PICKS.append(START + 3 * (SELECT + HOLD) + EDIT / 2)
SEEKS = [START - 0.2, START + EDIT / 2, SECOND - 0.2, SECOND + EDIT / 2, EDITS - 0.2]
SEEKS += [EDITS + t for t in PICKS]

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
