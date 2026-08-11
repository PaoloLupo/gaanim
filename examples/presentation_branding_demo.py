"""Reusable branding and template-backed semantic segments."""

import os

from gaanim import Scene, comparison, lecture, title_slide

scene = Scene(1280, 720, margin=48)
scene.canvas.set_theme("presentation")
scene.brand(
    logo="tests/assets/slides_brand.svg",
    footer="RESEARCH STUDIO · 2026",
    slide_numbers=True,
    rule=True,
    logo_scale=0.72,
)

cover = scene.segment("Cover", template=title_slide, notes="Introduce the topic.")
cover.bind(
    title=scene.title("A semantic slide deck"),
    subtitle=scene.subtitle("One theme, one brand, every slide"),
    footer=scene.text("Researcher Name"),
)
scene.wait(0.4)

content = scene.segment("Motivation", template=lecture, notes="State the research gap.")
content.bind(
    title=scene.title("Motivation"),
    body=scene.paragraph(
        "Scientific presentations need repeatable hierarchy, safe spacing, "
        "and navigation without manually rebuilding chrome on every slide.",
        font_size=34,
    ),
    footer=scene.text("Consistent by construction"),
)
scene.wait(0.4)

manual = scene.stack([
    scene.rounded_rect(310, 120, 18).fill(scene.canvas.color("panel")),
    scene.text("Manual"),
], width=310, height=120)
semantic = scene.stack([
    scene.rounded_rect(310, 120, 18).fill(scene.canvas.color("header")),
    scene.text("Semantic"),
], width=310, height=120)
compare = scene.segment("Comparison", template=comparison, notes="Compare both workflows.")
compare.bind(title=scene.title("Authoring workflow"), left=manual, right=semantic)
scene.wait(0.4)

closing = scene.segment("Conclusion", template=title_slide, notes="Close and invite questions.")
closing.bind(
    title=scene.title("Ready to present"),
    subtitle=scene.subtitle("Brand once. Present consistently."),
    footer=scene.text("Questions?"),
)
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.6, 1.0, 1.4])
else:
    scene.render()
