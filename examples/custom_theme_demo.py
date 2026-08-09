"""Custom themes, inheritance, and semantic component colors."""

import os

from gaanim import Axis, DataSource, Scene, Theme


scene = Scene(1280, 720, margin=54)

# Derive Nord and replace only the identity of this presentation.
# Omitted roles continue to come from Nord.
research_theme = Theme(
    "nord",
    name="research-lab",
    colors={
        "title": "#A3D9FF",
        "accent": "#FFB86C",
        "chart": "#88C0D0",
    },
    fonts={
        "text": "Segoe UI",
        "code": "Consolas",
    },
    sizes={
        "title": 58,
        "body": 30,
        "caption": 22,
    },
)
scene.canvas.set_theme(research_theme)
assert not scene.canvas.validate_theme()

scene.title("A theme is a visual system").at(0, 265)
scene.subtitle("Nord, customized for a research presentation").at(0, 205)
scene.line(-460, 165, 460, 165).stroke(scene.canvas.color("rule"), 2)

scene.bullets(
    [
        "Semantic colors stay consistent",
        "Typography changes by role",
        "Components inherit the same palette",
    ],
    width=510,
    gap=64,
).at(-320, -30)

chart_data = DataSource({"x": [0, 1, 2], "value": [38, 64, 91]})
chart_space = scene.axes(
    Axis.category(["Baseline", "Pilot", "Final"]),
    Axis.linear(0, 100).ticks(25),
    width=500,
    height=310,
).at(330, -35)
chart_space.bars(chart_data, "x", "value", width=0.7).fill(scene.canvas.color("accent"))

scene.caption(
    "Use Theme(...) to start from scratch, or derive any built-in scheme.",
    width=920,
    height=70,
    margin=28,
)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()

# To ship a font file with a project:
#
# custom = Theme(
#     research_theme,
#     fonts={"text": "My Slides Sans"},
#     font_files={"My Slides Sans": "assets/MySlidesSans.ttf"},
# )
