"""Finishing presets: one segment per built-in post-process."""

import os
from pathlib import Path

from gaanim import BLUE, CORAL, GOLD, WHITE, Brush, PostProcess, Scene


LUT = Path(__file__).parent / "assets" / "teal_orange.cube"
PRESETS = [
    ("color_grade", PostProcess.color_grade(exposure=0.2, contrast=1.2, saturation=1.3, temperature=0.4)),
    ("lut", PostProcess.lut(LUT)),
    ("chromatic_aberration", PostProcess.chromatic_aberration(0.01)),
    ("halftone", PostProcess.halftone(10)),
    ("dither", PostProcess.dither(3)),
    ("crt", PostProcess.crt()),
    ("pixelate", PostProcess.pixelate(14)),
    ("glitch", PostProcess.glitch(0.8, seed=3)),
]

scene = Scene(frame=(16, 9), background="#101826")
art = []
for index, (name, post) in enumerate(PRESETS):
    scene.segment(name, post=post)
    if index == 0:
        title = scene.text("Presets de acabado", role="title").fill(WHITE).move_to(0, 3.0)
        disc = scene.geometry.circle(1.5).fill(GOLD).move_to(-4.0, -0.2)
        square = scene.geometry.rounded_rect(3.0, 3.0, 0.3).fill(BLUE).move_to(0.0, -0.2)
        ring = scene.geometry.circle(1.4).no_fill().stroke(CORAL, 0.25).move_to(4.0, -0.2)
        # A black-to-white ramp shows dithering and the LUT's tone curve.
        ramp = Brush.linear(["#000000", "#2d8aa8", "#ffffff"], start=(-7.0, -2.9), end=(7.0, -2.9))
        band = scene.geometry.rect(14.0, 0.6).fill(ramp).move_to(0, -2.9)
        art = [title, disc, square, ring, band]
    else:
        scene.reuse(*art)
    scene.text(name, size=0.5).fill(WHITE).move_to(0, 2.1)
    scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The middle of every segment.
    scene.snapshots(snapshots, [0.25 + 0.5 * index for index in range(len(PRESETS))])
else:
    scene.render()
