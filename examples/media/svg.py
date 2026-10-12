"""SVG: vector files imported as drawables, drawn in like any shape, with their parts addressed by id.

An SVG keeps its gradients, clip paths, filters, text outlines and reused
elements, and comes in at 100 SVG pixels per scene unit. Its elements
become parts named by their ids, which can be restyled and animated alone.
Each label is the code that builds its variant.
"""

import os

from gaanim import Scene, Zones

FEATURES = "assets/features.svg"  # 360 x 220 px: gradients, clip paths, filters, <use>
CHARACTER = "assets/character.svg"  # parts: head, face, torso, left-arm, right-arm, legs

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")

DRAW = 1.4
HOLD = 0.6


def columns(labels, rows=1, bands=("1fr", "7fr")):
    """A padded grid with a code label above each cell; return the stages under the labels."""
    grid = scene.layout.zones(
        Zones.grid(rows=rows, columns=len(labels) // rows, gap="48px", padding="page_padding")
    )
    stages = []
    for cell, label in zip(grid, labels):
        top, stage = cell.split(Zones.rows(list(bands)))
        scene.text(label, role="code", markup=False).place(top, anchor="bottom")
        stages.append(stage)
    return stages


# The whole file is one drawable: scale_to resizes it with its own stroke
# widths, and part(id) reaches any element that has an id.
scene.segment("import")
(stage,) = columns(['art = scene.media.svg(FEATURES).scale_to(3)'])
art = scene.media.svg(FEATURES).scale_to(3).move_to(*stage.center)
scene.wait(0.3)
scene.play([
    art.part("orb").animate.indicate(),
    art.part("spark-top").animate.rotate_by(0.8),
    art.part("spark-bottom").animate.rotate_by(-0.8),
], duration=DRAW)
scene.wait(HOLD)

# An SVG draws in like any drawable: write and create trace its contours
# and then fill them, fade_in brings it in whole.
scene.segment("drawing")
stages = columns(["svg.animate.write()", "svg.animate.create()", "svg.animate.fade_in()"])
figures = [scene.media.svg(CHARACTER).scale_to(1.5).move_to(*stage.center) for stage in stages]
scene.play(
    [figures[0].animate.write(), figures[1].animate.create(), figures[2].animate.fade_in()],
    duration=DRAW,
)
scene.wait(HOLD)

# Parts are drawables too: a fill or stroke set on one restyles that
# element only, and a part animates without moving the rest.
scene.segment("parts")
stages = columns([
    'svg.part("torso")\n    .fill(success)',
    'svg.part("right-arm")\n    .stroke(accent, 0.06)',
    'svg.part("head")\n    .animate.scale_to(1.3)',
    'svg.part("face")\n    .animate.fade_out()',
], rows=2, bands=("2fr", "5fr"))
people = [scene.media.svg(CHARACTER).scale_to(1.1).move_to(*stage.center) for stage in stages]
people[0].part("torso").fill(success)
people[1].part("right-arm").stroke(accent, 0.06)
scene.wait(0.3)
scene.play([people[2].part("head").animate.scale_to(1.3), people[3].part("face").animate.fade_out()], duration=DRAW)
scene.wait(HOLD)

IMPORT_END = 0.3 + DRAW + HOLD
DRAWING_END = IMPORT_END + DRAW + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The import before and while its parts move; each drawing midway and
    # done; the restyled parts and the animated ones midway and done.
    scene.snapshots(
        snapshots,
        [0.2, 0.3 + DRAW / 2, IMPORT_END - 0.1]
        + [IMPORT_END + DRAW * 0.4, DRAWING_END - 0.1]
        + [DRAWING_END + 0.2, DRAWING_END + 0.3 + DRAW / 2, DRAWING_END + 0.3 + DRAW + HOLD - 0.1],
    )
else:
    scene.render()
