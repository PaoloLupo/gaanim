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
        "title": TextStyle(font="New Computer Modern", size=58, weight=700),
        "body": TextStyle(size=30),
        "label": TextStyle(size=24, weight=600),
    },
    styles={
        "shape": Style(fill="brand"),
        "line": Style(stroke=StrokeStyle("ink", 3, cap="round")),
        ".warning": Style(fill="warning"),
        "axes": AxesStyle(
            axis=StrokeStyle("ink", 3),
            grid=StrokeStyle(colors.tailwind.slate[300], 1),
            ticks=StrokeStyle("ink", 2),
            numbers=TextStyle(size=20, color=colors.tailwind.slate[700]),
            labels=TextStyle(size=24, weight=600, color=colors.tailwind.slate[900]),
        ),
    },
    series=[colors.tailwind.blue[600], colors.tailwind.amber[500], colors.tailwind.rose[600]],
)

scene = Scene(1920, 1080, margin=56, theme=theme)
scene.text("Diseño una vez, contenido después", role="title").at(0, 275)
scene.text(
    "TextStyle conserva la nueva API estructurada y Theme aporta la cascada.",
    role="body",
).at(0, 215)

scene.circle(62).at(-450, 70)
scene.square(118).style_class("warning").at(-260, 70)
scene.line(-510, -30, -190, -30)

x = Axis.linear(-3, 3).ticks(1).label("x")
y = Axis.linear(-2, 2).ticks(1).label("y")
scene.axes(x, y, width=560, height=330).at(260, -70)

scene.wait(1.0)
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
