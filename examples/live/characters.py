"""Characters: the figures the audience builds on their phones, as drawables in the scene.

``scene.character(avatar)`` draws a character from five parts, ``[body,
color, eyes, mouth, extra]``, or from a nickname without one. It breathes
and blinks along the timeline and plays the expressions phones show; place,
scale and animate it like any drawable. Each row varies one part from the
same base, and each label is the code that builds it.
"""

import os

from gaanim import Character, Scene, Zones

BASE = [0, 0, 0, 1, 0]
COUNTS = {"body": 7, "color": 10, "eyes": 7, "mouth": 7, "extra": 9}
PARTS = list(COUNTS)

scene = Scene()
seeks = []


def code(label, zone, anchor="left"):
    """A variant label: the code that builds it."""
    return scene.text(label, role="code").place(zone, anchor=anchor)


def parts(names, size):
    """One row per part in ``names``, its label above a character for each choice."""
    rows = scene.layout.zones(Zones.rows(len(names), gap="32px", padding="page_padding"))
    for name, row in zip(names, rows):
        label, stage = row.split(Zones.rows(["1fr", "4fr"], gap="8px"))
        position = PARTS.index(name)
        avatar = ", ".join("i" if index == position else str(part) for index, part in enumerate(BASE))
        code(f"scene.character([{avatar}])  # {name}", label)
        cells = stage.split(Zones.columns(COUNTS[name], gap="16px"))
        for choice in range(COUNTS[name]):
            avatar = list(BASE)
            avatar[position] = choice
            scene.character(avatar, size=size).place(cells[choice])
    scene.wait(1.0)
    seeks.append(scene.cursor - 0.05)


# The body's shape and its color.
scene.segment("bodies")
parts(["body", "color"], 2.0)

# The face, and an extra on top: ears, hats, headphones or blush.
scene.segment("faces")
parts(["eyes", "mouth", "extra"], 1.8)

# express() plays an expression from the cursor: once, back to the
# character's own face, or with loop=True until the next one.
scene.segment("expressions")
EXPRESSIONS = Character.expressions()
page = scene.layout.zones(Zones.rows(["1fr", "1fr", "4fr", "1fr", "4fr"], gap="16px", padding="page_padding"))
code("for expression in Character.expressions():", page[0], anchor="left")
heroes = []
for row, label, loop in [(1, "hero.express(expression)", False), (3, "hero.express(expression, loop=True)", True)]:
    code(label, page[row], anchor="left")
    cells = page[row + 1].split(Zones.columns(len(EXPRESSIONS), gap="32px"))
    for index, (expression, cell) in enumerate(zip(EXPRESSIONS, cells)):
        stage, name = cell.split(Zones.rows(["4fr", "1fr"]))
        hero = scene.character([index, 2 * index, 0, 1, 0], size=2.2).place(stage)
        code(f'"{expression}"', name, anchor="center")
        heroes.append((hero, expression, loop))
start = scene.cursor
scene.wait(0.3)
for hero, expression, loop in heroes:
    hero.express(expression, loop=loop)
scene.wait(3.0)
seeks.extend([start + 0.8, start + 1.6, scene.cursor - 0.05])

# Without an avatar, the nickname picks the parts and how the character
# blinks, as for the audience's own characters.
scene.segment("names")
NAMES = ["Ana", "Beto", "Caro", "Dani", "Eli", "Fede"]
page = scene.layout.zones(Zones.rows(["1fr", "6fr", "1fr"], gap="16px", padding="page_padding"))
code("scene.character(name=name, size=3.0)", page[0], anchor="center")
stages = page[1].split(Zones.columns(len(NAMES), gap="32px"))
labels = page[2].split(Zones.columns(len(NAMES), gap="32px"))
for name, stage, label in zip(NAMES, stages, labels):
    scene.character(name=name, size=3.0).place(stage)
    code(f'name="{name}"', label, anchor="center")
scene.wait(1.0)
seeks.append(scene.cursor - 0.05)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
