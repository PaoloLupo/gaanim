"""One centralized theme for text, primitives, classes, and axes."""

import os

from gaanim import AxesStyle, Axis, Scene, StrokeStyle, Style, TextStyle, Theme, colors

theme = Theme(
    "paper",
    name="tailwind-editorial",
    colors={
        "brand": colors.tailwind.blue[600],
        "warning": colors.tailwind.rose[600],
        "ink": colors.tailwind.slate[900],
    },
    text={
        "title": TextStyle(font="New Computer Modern", size=0.483333, weight=700),
        "body": TextStyle(size=0.25),
        "label": TextStyle(size=0.2, weight=600),
    },
    styles={
        "shape": Style(fill="brand"),
        "line": Style(stroke=StrokeStyle("ink", 3, cap="round")),
        ".warning": Style(fill="warning"),
        "axes": AxesStyle(
            axis=StrokeStyle("ink", 3),
            grid=StrokeStyle(colors.tailwind.slate[300], 1),
            ticks=StrokeStyle("ink", 2),
            numbers=TextStyle(size=0.166667, color=colors.tailwind.slate[700]),
            labels=TextStyle(size=0.2, weight=600, color=colors.tailwind.slate[900]),
        ),
    },
    series=[colors.tailwind.blue[600], colors.tailwind.amber[500], colors.tailwind.rose[600]],
)

scene = Scene(frame=(16, 9), margin=0.466667, theme=theme)
scene.text("Diseño una vez, contenido después", role="title").move_to(0, 2.291667)
scene.text(
    "TextStyle conserva la nueva API estructurada y Theme aporta la cascada.",
    role="body",
).move_to(0, 1.791667)

scene.geometry.circle(0.516667).move_to(-3.75, 0.583333)
scene.geometry.square(0.983333).style_class("warning").move_to(-2.166667, 0.583333)
scene.geometry.line(-4.25, -0.25, -1.583333, -0.25)

x = Axis.linear(-3, 3).ticks(1).label("x")
y = Axis.linear(-2, 2).ticks(1).label("y")
scene.viz.cartesian_2d(x, y, width=4.666667, height=2.75).move_to(2.166667, -0.583333)

scene.wait(1.0)
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
