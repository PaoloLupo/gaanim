"""A tour of the scene: the factories that make objects and the animations that change them.

Every object comes from a factory on the scene (``geometry``, ``text``,
``layout``, ``viz``, ``media``), and every change is an animation from the
object's ``animate`` proxy, played on the timeline. Each row is one building
block, labelled with the code that makes or changes it; the other topics cover
each of them in full.
"""

import math
import os

from gaanim import Axis, Scene, Zones, parallel

DURATION = 1.4
HOLD = 0.8

scene = Scene()
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")
success = scene.canvas.color("success")


def table(rows, tracks=None):
    """One row per (label, build) pair: the label, then the stage where ``build(stage)``
    draws the row and returns its animation."""
    grid = scene.layout.zones(
        Zones.grid(rows=tracks or len(rows), columns=[8.8, "1fr"], gap="24px", column_gap="64px",
                   padding="page_padding")
    )
    moves = []
    for index, (label, build) in enumerate(rows):
        scene.text(label, role="code", markup=False).place(grid[2 * index], anchor="right")
        moves.append(build(grid[2 * index + 1]))
    scene.play(moves, duration=DURATION)
    scene.wait(HOLD)


SOURCE = 'def greet(name):\n    print("hi", name)'


def code_panel(stage):
    panel = scene.text.code(SOURCE, language="py", width=5.4, height=1.8, font_size=0.36)
    return panel.move_to(*stage.center).animate.fade_in()


# Drawing: shapes, text, equations and code, each with the entrance that suits it.
scene.segment("draw")
table(
    [
        ("scene.geometry.star(5, 0.75, 0.32)",
         lambda stage: scene.geometry.star(5, 0.75, 0.32).move_to(*stage.center).animate.create()),
        ('scene.text("Hello, Gaanim", size=0.75)',
         lambda stage: scene.text("Hello, Gaanim", size=0.75).move_to(*stage.center).animate.write()),
        ('scene.text.equation("E = m c^2", size=0.8)',
         lambda stage: scene.text.equation("E = m c^2", size=0.8).move_to(*stage.center).animate.write()),
        ('scene.text.code(SOURCE, language="py",\n'
         "                width=5.4, height=1.8,\n"
         "                font_size=0.36)", code_panel),
    ],
    tracks=["1fr", "1fr", "1fr", "2fr"],
)


def boxes(stage):
    words = ("one", "two", "three")
    boxes = [scene.layout.box(word, padding=("12px", "28px"), radius="12px", background=panel, font_size="64px") for word in words]
    row = scene.layout.row(*boxes, gap="32px")
    return row.place(stage).animate.fade_in()


def sine(stage):
    plane = scene.viz.cartesian_2d(Axis.linear(-3, 3), Axis.linear(-1.2, 1.2), width=4.6, height=1.5,
                                   numbers=False)
    plane.move_to(*stage.center)
    return parallel(plane.animate.create(), plane.plot(math.sin).animate.create())


def character(stage):
    return scene.media.svg("assets/character.svg").place(stage, fit="contain").animate.fade_in()


def clock(stage):
    readout = scene.viz.readout(scene.time, label="t", format=".2f", unit="s", font_size=0.6)
    return readout.place(stage).animate.fade_in()


# Building: boxes that lay themselves out, plots on axes, imported artwork and
# numbers that follow a value (here the playhead's time).
scene.segment("build")
table(
    [
        ('scene.layout.row(*boxes, gap="32px")', boxes),
        ("plane = scene.viz.cartesian_2d(x, y)\nplane.plot(math.sin)", sine),
        ('scene.media.svg("assets/character.svg")', character),
        ('scene.viz.readout(scene.time, label="t")', clock),
    ],
    tracks=["1fr", "1.3fr", "1.5fr", "1fr"],
)


def morph(stage):
    energy = scene.text.equation("E = m c^2", size=0.8).move_to(*stage.center)
    momentum = scene.text.equation("p = m v", size=0.8).move_to(*stage.center)
    return energy.animate.transform_to(momentum)


def travel(stage):
    (x0, y), (x1, _) = stage.point("left"), stage.point("right")
    path = scene.geometry.bezier((x0 + 0.3, y - 0.5), [(x0 + 2, y + 1.6), (x1 - 2, y - 1.6)], (x1 - 0.3, y + 0.5))
    path.no_fill().stroke(rule, 0.03)
    dot = scene.geometry.dot(0.16).move_to(x0 + 0.3, y - 0.5)
    return dot.animate.move_along(path)


# Changing: morphing one object into another, drawing attention, moving along a
# path and animating a property.
scene.segment("change")
table([
    ("energy.animate.transform_to(momentum)", morph),
    ("star.animate.indicate()",
     lambda stage: scene.geometry.star(5, 0.8, 0.34).move_to(*stage.center).animate.indicate()),
    ("dot.animate.move_along(path)", travel),
    ("circle.animate.fill(success)",
     lambda stage: scene.geometry.circle(0.75).move_to(*stage.center).animate.fill(success)),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-animation and settled, in each of the three segments.
    scene.snapshots(snapshots, [(DURATION + HOLD) * index + t for index in range(3) for t in (0.6, 1.9)])
else:
    scene.render()
