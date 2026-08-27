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
scene.text("Diseño una vez, contenido después", role="title").move_to(0, 275)
scene.text(
    "TextStyle conserva la nueva API estructurada y Theme aporta la cascada.",
    role="body",
).move_to(0, 215)

scene.geometry.circle(62).move_to(-450, 70)
scene.geometry.square(118).style_class("warning").move_to(-260, 70)
scene.geometry.line(-510, -30, -190, -30)

x = Axis.linear(-3, 3).ticks(1).label("x")
y = Axis.linear(-2, 2).ticks(1).label("y")
scene.viz.cartesian_2d(x, y, width=560, height=330).move_to(260, -70)

scene.wait(1.0)
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
