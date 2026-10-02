"""Non-destructive path modifiers: zig zag, round corners, pucker & bloat,
twist, wiggle and offset, with animated parameters."""

import os

from gaanim import BLUE, CORAL, GOLD, TEAL, WHITE, Scene


scene = Scene(frame=(16, 9))
scene.text("Path modifiers", role="title").fill(WHITE).move_to(0, 3.3)

# A star whose zig zag grows into teeth, then rounded by the next modifier.
star = scene.geometry.star(5, 1.1, 0.5).fill(GOLD).move_to(-5.0, 1.0)
zz = star.modifiers.zigzag(size=0.0, ridges=3)
star.modifiers.round_corners(0.04)

# A square puckering into a spiky star, then bloating.
square = scene.geometry.square(1.6).fill(CORAL).move_to(-1.7, 1.0)
pb = square.modifiers.pucker_bloat(0.0)

# A hexagon twisted about its center.
hexagon = scene.geometry.regular_polygon(6, 1.0).fill(TEAL).move_to(1.7, 1.0)
tw = hexagon.modifiers.twist(0.0)

# A wobbling blob, alive in time.
blob = scene.geometry.circle(0.9).fill(BLUE).move_to(5.0, 1.0)
blob.modifiers.wiggle_path(size=0.12, detail=3, frequency=1.5, seed=3)

# Concentric rings offset from one outline.
ring = scene.geometry.circle(0.5).no_fill().stroke(WHITE, 0.04, align="center").move_to(-3.0, -2.2)
rings = ring.modifiers.offset(0.15, join="round", copies=3)

# Text: every glyph is modified on its own.
word = scene.text("wobble").fill(CORAL).scale_to(1.4).move_to(3.0, -2.2)
word.modifiers.wiggle_path(size=0.015, detail=2, frequency=2.0, seed=7)

scene.wait(0.3)
scene.play(
    [
        zz.animate.size(0.12).duration(1.0),
        pb.animate.amount(-0.5).duration(1.0),
        tw.animate.angle(1.4).duration(1.0),
        rings.animate.amount(0.3).duration(1.0),
    ]
)
scene.play([pb.animate.amount(0.4).duration(0.8), tw.animate.angle(-1.0).duration(0.8)])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Authored, half way, puckered and twisted, bloated, wiggle later in time.
    scene.snapshots(snapshots, [0.1, 0.8, 1.3, 2.1, 2.5])
else:
    scene.render()
