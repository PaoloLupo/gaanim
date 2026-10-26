"""Bloom: threshold, intensity and radius, and a glow that pulses with a Parameter.

Each segment draws the same outlines and bars through one bloom pass over the
whole frame. Everything brighter than the threshold glows: the bright outlines
by default, the gray bars only when the threshold drops below their
brightness. Each label is the post-process of its segment.
"""

import os

from gaanim import PostProcess, Scene, Zones

HOLD = 1.0
PULSE = 0.8

scene = Scene()
page = scene.layout.zones(Zones.rows(["6fr", "1fr"], gap="24px", padding="page_padding"))


def neon(zone):
    """Bright outlines over three filled bars of decreasing brightness."""
    shapes, bars = zone.split(Zones.rows(["4fr", "1fr"], gap="48px"))
    outlines = [
        (lambda: scene.geometry.circle(1.0), "accent"),
        (lambda: scene.geometry.star(5, 1.0, 0.45), "success"),
        (lambda: scene.geometry.square(1.6), "danger"),
        (lambda: scene.geometry.regular_polygon(3, 1.0), "foreground"),
    ]
    for cell, (shape, role) in zip(shapes.split(Zones.columns(4, gap="48px")), outlines):
        shape().no_fill().stroke(scene.colors[role], 0.16).place(cell, fit="contain")
    for cell, role in zip(bars.split(Zones.columns(3, gap="48px")), ["chart", "muted", "rule"]):
        scene.geometry.rounded_rect(cell.width, cell.height, cell.height / 2).fill(scene.colors[role]).place(cell)


def glow(code, post):
    """One segment of the outlines through ``post``, labelled with ``code``."""
    scene.segment(code, post=post)
    neon(page[0])
    scene.text(code, role="code").place(page[1], anchor="left")


# threshold (0..1) is the brightness where the glow starts, intensity scales
# the added light and radius (0..1) spreads it from a tight halo to a haze.
BLOOMS = [
    ("PostProcess.bloom()", PostProcess.bloom()),
    ("PostProcess.bloom(intensity=2.0)", PostProcess.bloom(intensity=2.0)),
    ("PostProcess.bloom(intensity=2.0, threshold=0.4)", PostProcess.bloom(intensity=2.0, threshold=0.4)),
    ("PostProcess.bloom(intensity=2.0, radius=0.1)", PostProcess.bloom(intensity=2.0, radius=0.1)),
    ("PostProcess.bloom(intensity=2.0, radius=1.0)", PostProcess.bloom(intensity=2.0, radius=1.0)),
]
for code, post in BLOOMS:
    glow(code, post)
    scene.wait(HOLD)

# Any of the three takes a Parameter, so the glow animates like any value.
pulse_start = scene.cursor
level = scene.viz.parameter(0.0)
scene.segment("pulse", post=PostProcess.bloom(intensity=level))
neon(page[0])
lines = page[1].split(Zones.rows(2))
scene.text("PostProcess.bloom(intensity=level)", role="code").place(lines[0], anchor="left")
scene.text("level.animate.set(4.0).repeat(2, yoyo=True)", role="code").place(lines[1], anchor="left")
scene.play([level.animate.set(4.0).duration(PULSE).repeat(2, yoyo=True)])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The middle of every segment, then the pulse rising and at its peak.
    scene.snapshots(
        snapshots,
        [HOLD * (index + 0.5) for index in range(len(BLOOMS))] + [pulse_start + 0.4, pulse_start + PULSE],
    )
else:
    scene.render()
