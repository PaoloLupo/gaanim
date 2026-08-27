"""Custom themes, inheritance, and semantic component colors."""

import os

from gaanim import Anchor, Axis, ChartSpec, Scene, Theme


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

scene.text("A theme is a visual system", role="title").move_to(0, 265, anchor=Anchor.CENTER)
scene.text("Nord, customized for a research presentation", role="subtitle").move_to(0, 205, anchor=Anchor.CENTER)
scene.geometry.line(-460, 165, 460, 165).stroke(scene.canvas.color("rule"), 2)

scene.slides.bullets(
    [
        "Semantic colors stay consistent",
        "Typography changes by role",
        "Components inherit the same palette",
    ],
    width=510,
    gap=64,
).move_to(-320, -30)

chart_spec = (
    ChartSpec({"x": [0, 1, 2], "value": [38, 64, 91]})
    .mark("bar", width=0.7)
    .encode(x="x", y="value")
    .axes(
        x=Axis.category(["Baseline", "Pilot", "Final"]),
        y=Axis.linear(0, 100).ticks(25),
    )
)
chart = scene.viz.chart(chart_spec).scale_to(0.64).move_to(330, -35)
chart.layer("marks").fill(scene.canvas.color("accent"))

scene.slides.banner(
    "Theme(...) derives any built-in scheme.",
    width=1080,
    margin=20,
    position="bottom",
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
