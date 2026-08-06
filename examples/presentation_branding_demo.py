"""Reusable thesis branding and semantic slide templates."""

import os

from gaanim import Anchor, Scene


scene = Scene(1280, 720, margin=48)
scene.canvas.set_theme("presentation")
scene.brand(
    logo="tests/assets/thesis_brand.svg",
    footer="UNIVERSITY · MASTER THESIS · 2026",
    slide_numbers=True,
    rule=True,
    logo_scale=0.72,
)

cover = scene.slide("Cover", layout="cover", notes="Introduce the thesis.")
cover.region("title").place(scene.title("A semantic thesis deck"), Anchor.CENTER)
cover.region("subtitle").place(
    scene.subtitle("One theme, one brand, every slide"),
    Anchor.CENTER,
)
scene.play([scene.text("Researcher Name").at(0, -210).write().duration(0.4)])

content = scene.slide("Motivation", layout="content", notes="State the research gap.")
content.region("title").place(scene.title("Motivation"), Anchor.LEFT)
content.region("content").place(
    scene.paragraph(
        "Scientific presentations need repeatable hierarchy, safe spacing, "
        "and navigation without manually rebuilding chrome on every slide.",
        width=920,
        font_size=34,
    ),
    Anchor.CENTER,
)
scene.play([scene.text("Consistent by construction").at(0, -180).write().duration(0.4)])

comparison = scene.slide("Comparison", layout="comparison", notes="Compare both workflows.")
comparison.region("title").place(scene.title("Authoring workflow"), Anchor.LEFT)
comparison.region("before").place(
    scene.group(
        [
            scene.rounded_rect(310, 120, 18).fill(scene.canvas.color("panel")),
            scene.text("Manual"),
        ]
    ),
    Anchor.CENTER,
)
comparison.region("after").place(
    scene.group(
        [
            scene.rounded_rect(310, 120, 18).fill(scene.canvas.color("header")),
            scene.text("Semantic"),
        ]
    ),
    Anchor.CENTER,
)
scene.play([scene.arrow(-120, 0, 120, 0).create().duration(0.4)])

closing = scene.slide("Conclusion", layout="conclusion", notes="Close and invite questions.")
closing.region("title").place(scene.title("Ready for the defense"), Anchor.CENTER)
closing.region("subtitle").place(
    scene.subtitle("Brand once. Present consistently."),
    Anchor.CENTER,
)
scene.play([scene.text("Questions?").at(0, -180).write().duration(0.4)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.6, 1.0, 1.4])
else:
    scene.render()
