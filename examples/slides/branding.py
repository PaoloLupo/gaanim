"""Branding: a deck theme derived from "presentation" and a logo, footer and slide number on every slide.

A Theme derived from a built-in one replaces only the colors it names, and
every component, template and text role picks them up. scene.slides.brand()
draws the same logo, footer, rule and slide number on every slide except the
cover. The slides show the code that sets the brand up.
"""

import os

from gaanim import Anchor, Scene, Theme, Transition, comparison, lecture, title_slide

# The brand's colors, taken from its logo.
BRAND = {"accent": "#FFD166", "title": "#FFD166", "panel": "#17233D", "header": "#22345A",
         "background": "#0E1626", "rule": "#3A4A6B"}
# The margin keeps the brand's chrome off the edges of the frame.
theme = Theme("presentation", name="studio", colors=BRAND)
scene = Scene(theme=theme, margin=0.5)
S = scene.slides
L = scene.layout

# brand() is set once and applies to every segment: the logo in the top-right
# corner, the footer and its rule along the bottom, and the slide number,
# here on its own at the bottom right. show_on_cover=False leaves covers clean.
BRAND_CALL = """scene.slides.brand(
    logo="assets/brand.svg",
    footer="RESEARCH STUDIO · 2026",
    number_anchor=Anchor.BOTTOM_RIGHT,
    logo_scale=0.9,
)"""
scene.slides.brand(
    logo="assets/brand.svg",
    footer="RESEARCH STUDIO · 2026",
    number_anchor=Anchor.BOTTOM_RIGHT,
    logo_scale=0.9,
)
seeks = []


def code(*lines):
    return scene.text("\n".join(lines), role="code", markup=False)


cover = scene.segment("Cover", template=title_slide)
title = scene.text("Quarterly report", role="title", size=0.9)
cover.bind(title=title, subtitle=scene.text("Research studio", role="subtitle", size=0.45),
           footer=code('theme = Theme("presentation", name="studio", colors=BRAND)',
                       "scene = Scene(theme=theme, margin=0.5)"))
scene.play([title.animate.write().duration(0.8)])
seeks.append(scene.cursor - 0.1)

identity = scene.segment("Identity", Transition.cross_fade(0.4), template=lecture)
identity.bind(title=scene.text("One brand on every slide", role="title"),
              body=scene.text.code(BRAND_CALL, language="python", width=11, height=4.4, font_size=0.48))
scene.wait(1.2)
seeks.extend([scene.cursor - 0.9, scene.cursor - 0.1])

# Components take the brand's colors through the theme roles.
results = scene.segment("Results", Transition.cross_fade(0.4), template=comparison)
results.bind(
    title=scene.text("Results", role="title"),
    left=S.stat_card("98%", "Accuracy", delta="+4.2%", width=4, variant="accent"),
    right=S.card("Next quarter", "Extend the study to three more sites.", width=4.6),
)
scene.wait(1.0)
seeks.append(scene.cursor - 0.1)

closing = scene.segment("Closing", Transition.cross_fade(0.4), template=title_slide)
closing.bind(title=scene.text("Thank you", role="title", size=0.9),
             footer=code("scene.slides.brand(..., show_on_cover=False)"))
scene.wait(1.0)
seeks.append(scene.cursor - 0.1)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
