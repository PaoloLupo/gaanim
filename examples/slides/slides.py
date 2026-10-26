"""Slides: a short deck of segments with templates, steps, notes and transitions.

Each scene.segment() is a slide: template= lays out its slots, notes= are
the speaker notes Presenter View shows, and a transition leads into it. Each
scene.stop() is a step where a presentation waits for the speaker; exports
and previews play straight through. Every slide shows the code that makes it.
"""

import math
import os

from gaanim import Scene, Transition, comparison, credits, lecture, minimal, title_slide

scene = Scene()
S = scene.slides
L = scene.layout
accent = scene.colors.accent
chart = scene.colors.chart
rule = scene.colors.rule
seeks = []


def code(*lines):
    """The code of a slide, one string per line."""
    return scene.text("\n".join(lines), role="code")


def heading(text, size=None):
    return scene.text(text, role="title", size=size)


# title_slide centers a title, a subtitle and a footer. thumbnail() picks the
# frame a .gaanim file shows as its cover, and stop() waits for the speaker.
cover = scene.segment("Cover", template=title_slide, notes="Say hello.")
title = heading("Orbital transfers", size=0.9)
subtitle = scene.text("Two burns between circular orbits", role="subtitle", size=0.45)
cover.bind(title=title, subtitle=subtitle,
           footer=code('scene.segment("Cover", template=title_slide, notes="Say hello.")'))
scene.play([title.animate.write().duration(0.8), subtitle.animate.fade_in().duration(0.6)])
scene.thumbnail()
seeks.append(scene.cursor - 0.1)
scene.stop("cover")

# lecture stacks a title, a body that takes the free space and a footer. A
# stop after each item makes one step per item while presenting.
plan_slide = scene.segment("Plan", Transition.slide(0.5, "left"), template=lecture, notes="One step per burn.")
plan = [scene.text(line, size=0.55) for line in (
    "1. Burn prograde in the low orbit",
    "2. Coast along half an ellipse",
    "3. Burn again to circularize",
)]
plan_slide.bind(
    title=heading("Three steps"),
    body=L.column(*plan, gap="48px", justify="center"),
    footer=code('scene.segment("Plan", Transition.slide(0.5, "left"),',
                '              template=lecture, notes="One step per burn.")',
                "for item in plan:",
                "    scene.play(item.animate.fade_in())",
                "    scene.stop()"),
)
seeks.append(scene.cursor + 0.25)
for index, item in enumerate(plan):
    scene.play(item.animate.fade_in().duration(0.4))
    scene.stop(f"burn {index + 1}")
seeks.append(scene.cursor - 0.05)

# comparison puts two items side by side, each growing to fill its half.
compare = scene.segment("Compare", Transition.push(0.6, "up"), template=comparison, notes="Cost against time.")
compare.bind(
    title=heading("Hohmann against bi-elliptic"),
    left=S.stat_card("3.9 km/s", "Hohmann transfer", delta="5 hours", width=4, variant="accent"),
    right=S.stat_card("3.8 km/s", "Bi-elliptic transfer", delta="40 hours", width=4, variant="success"),
    footer=code('scene.segment("Compare", Transition.push(0.6, "up"),',
                "              template=comparison)"),
)
scene.wait(1.2)
# Mid-push and settled.
seeks.extend([scene.cursor - 0.9, scene.cursor - 0.1])
scene.stop("compare")

# minimal fits one item in the frame. stop(loop=...) plays an animation that
# repeats while a presentation rests on the step; it should end where it
# starts.
orbit = scene.segment("Orbit", Transition.cross_fade(0.4), template=minimal, notes="Let it circle.")
planet = scene.geometry.circle(0.7).fill(chart)
low = scene.geometry.circle(1.4).no_fill().stroke(rule, 0.02)
high = scene.geometry.circle(2.6).no_fill().stroke(rule, 0.02)
transfer = scene.geometry.ellipse_arc(-0.6, 0, 2.0, 1.9, 0, math.pi).no_fill().stroke(accent, 0.03)
# The craft rides an invisible ring, so it turns about the planet.
craft = scene.geometry.group([scene.geometry.circle(1.4).no_fill().no_stroke(),
                              scene.geometry.dot(0.12).move_to(1.4, 0)])
orbit.bind(content=L.column(
    scene.geometry.group([planet, low, high, transfer, craft]),
    code('scene.segment("Orbit", Transition.cross_fade(0.4), template=minimal)',
         'scene.stop("orbit", loop=craft.animate.rotate_by(math.tau).duration(3))'),
    gap="48px", align="center",
))
scene.wait(0.5)
scene.stop("orbit", loop=craft.animate.rotate_by(math.tau).duration(3))
# Midway round and back at the start.
seeks.extend([scene.cursor - 1.5, scene.cursor - 0.05])

# credits centers a title over entries and a footer. bind() returns the
# template's root box, so the whole slide can be revealed piece by piece.
closing = scene.segment("Credits", Transition.cross_fade(0.4), template=credits, notes="Invite questions.")
root = closing.bind(
    title=heading("Thank you", size=0.9),
    entries=L.column("Animation · the team", "Orbits · public mission data", gap="24px", align="center",
                     font_size="48px"),
    footer=code('closing = scene.segment("Credits", Transition.cross_fade(0.4),',
                "                        template=credits)",
                "root = closing.bind(title=..., entries=..., footer=...)",
                "scene.play(root.reveal())"),
)
scene.play(root.reveal())
scene.wait(0.6)
seeks.extend([scene.cursor - 1.0, scene.cursor - 0.1])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
