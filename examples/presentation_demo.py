"""A multi-segment Layout v2 presentation for Presenter View."""

import os

from gaanim import BLACK, BLUE, GOLD, GRAY, WHITE, Scene, comparison, lecture, title_slide

scene = Scene(1920, 1080, background=BLACK, margin=64)

opening = scene.segment("Welcome", notes="Introduce the semantic segment model.", template=title_slide)
opening.bind(
    title=scene.text("Gaanim presentations", role="title").fill(GOLD),
    subtitle=scene.text("Animated slides, one timeline, and speaker notes", role="subtitle").fill(WHITE),
    footer=scene.text("Press Right to begin").fill(GRAY),
)
scene.wait(0.45)
scene.stop("opening-ready")

section = scene.segment("Why semantic slides?", notes="Contrast video and live talks.", template=title_slide)
section.bind(
    title=scene.text("Control the story", role="title").fill(WHITE),
    subtitle=scene.text("The speaker advances at the pace of the room.", role="subtitle").fill(GRAY),
    footer=scene.text("01 / THE IDEA · explicit stops wait for input").fill(GOLD),
)
scene.wait(0.45)
scene.stop("talking-point")

workflow = scene.segment("One source of truth", notes="Walk through the pipeline.", template=comparison)
workflow.bind(
    title=scene.text("One source of truth", role="title").fill(GOLD),
    left=scene.text("Python scene + layout + animations").fill(WHITE),
    right=scene.text("Presentation + timeline + export").fill(WHITE),
    footer=scene.text("One responsive tree").fill(BLUE),
)
scene.wait(0.55)
scene.stop("pipeline")
scene.wait(0.35)
scene.stop("same-timeline")

benefits = scene.column([
    scene.text("Named slides").fill(WHITE),
    scene.text("Speaker notes").fill(WHITE),
    scene.text("Direct navigation").fill(WHITE),
], gap=32, align="center")
reveal = scene.segment("Reveal in steps", notes="Advance once per benefit.", template=lecture)
reveal.bind(title=scene.text("Reveal only what matters", role="title").fill(GOLD), body=benefits)
scene.wait(0.35)
scene.stop("named-segments")
scene.wait(0.35)
scene.stop("speaker-notes")
scene.wait(0.35)
scene.stop("direct-navigation")

closing = scene.segment("Thank you", notes="Invite questions.", template=title_slide)
closing.bind(
    title=scene.text("Thank you", role="title").fill(GOLD),
    subtitle=scene.text("Build it once. Present it live.", role="subtitle").fill(WHITE),
    footer=scene.text("Questions? · gaanim --present presentation_demo.py").fill(BLUE),
)
scene.wait(0.5)
scene.stop("questions")
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.65, 1.1, 2.0, 2.5, 3.1, 4.0])
else:
    scene.render()
