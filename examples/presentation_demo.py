"""A multi-slide presentation for `gaanim --present` and Presenter View.

Run after `just build`:
    cargo run -p gaanim_editor -- --present examples/presentation_demo.py

Use Right/Left to move between semantic stops. Press O in the Presenter View
to search and jump directly to any slide.
"""

import os

from gaanim import Anchor, BLACK, BLUE, GOLD, GRAY, WHITE, Scene


scene = Scene(1920, 1080, background=BLACK, margin=64)

# 1. Opening ---------------------------------------------------------------
opening = scene.slide(
    "Welcome",
    notes="Welcome the audience and explain that this is a real semantic slide.",
    layout="title",
)
opening_title = opening.region("title").place(
    scene.title("Gaanim presentations").fill(GOLD), Anchor.CENTER
)
opening_subtitle = opening.region("subtitle").place(
    scene.subtitle("Animated slides, one timeline, and speaker notes").fill(WHITE),
    Anchor.CENTER,
)
opening_hint = scene.text("Press Right to begin").fill(GRAY).at(0, -210)
# Static content is shown automatically when this slide becomes active. Only
# real reveals need an animation or an explicit `step()`.
scene.play([opening_hint.write().duration(0.45)])
opening.step("opening-ready")

# 2. Section divider -------------------------------------------------------
section = scene.slide(
    "Why semantic slides?",
    notes="Introduce the difference between a timed video and an interactive talk.",
    layout="section",
)
section_eyebrow = section.region("eyebrow").place(
    scene.text("01 / THE IDEA").fill(GOLD), Anchor.CENTER
)
section_title = section.region("title").place(
    scene.title("Control the story").fill(WHITE), Anchor.CENTER
)
section_subtitle = section.region("subtitle").place(
    scene.subtitle("The speaker advances at the pace of the room.").fill(GRAY),
    Anchor.CENTER,
)
section_blurb = scene.text("Every slide is a named stop in the timeline.").fill(BLUE).at(0, -180)
scene.play([section_blurb.write().duration(0.45)])
section.step("talking-point")

# 3. Two columns -----------------------------------------------------------
workflow = scene.slide(
    "One source of truth",
    notes="Walk through authoring, semantic navigation, and output in order.",
    layout="two_columns",
)
workflow_title = workflow.region("title").place(
    scene.title("One source of truth").fill(GOLD), Anchor.CENTER
)
workflow_left = scene.text("Python scene + layout + animations").fill(WHITE).at(-430, -30)
workflow_right = scene.text("Presentation + timeline + export").fill(WHITE).at(430, -30)
bridge = scene.arrow(-120, -35, 120, -35).stroke(BLUE, 8)
scene.play([bridge.create().duration(0.55)])
workflow.step("pipeline")
scene.play([bridge.indicate().duration(0.35)])
workflow.step("same-timeline")

# 4. A reveal sequence -----------------------------------------------------
reveal = scene.slide(
    "Reveal in steps",
    notes="Advance once for each benefit. The overview can still jump to this slide directly.",
    layout="title_content",
)
reveal_title = reveal.region("title").place(
    scene.title("Reveal only what matters").fill(GOLD), Anchor.CENTER
)
benefit_one = scene.text("Named slides").fill(WHITE).at(0, 100)
benefit_two = scene.text("Speaker notes").fill(WHITE).at(0, 10)
benefit_three = scene.text("Direct navigation").fill(WHITE).at(0, -80)
scene.play([benefit_one.write().duration(0.35)])
reveal.step("named-slides")
scene.play([benefit_two.write().duration(0.35)])
reveal.step("speaker-notes")
scene.play([benefit_three.write().duration(0.35)])
reveal.step("direct-navigation")

# 5. Closing ---------------------------------------------------------------
closing = scene.slide(
    "Thank you",
    notes="Invite questions. Use O in Presenter View to revisit any topic.",
    layout="closing",
)
closing_title = scene.title("Thank you").fill(GOLD).at(0, 180)
closing_subtitle = scene.subtitle("Build it once. Present it live.").fill(WHITE).at(0, 40)
closing_command = scene.text("gaanim --present presentation_demo.py").fill(BLUE).at(0, -100)
closing_questions = scene.text("Questions?").fill(WHITE).at(0, -220)
scene.play([closing_questions.write().duration(0.5)])
closing.step("questions")
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Sample inside slides/reveals rather than exactly on zero-duration
    # visibility boundaries.
    scene.snapshots(snapshots, [0.2, 0.65, 1.1, 2.0, 2.5, 3.1, 4.0])
else:
    scene.render()
