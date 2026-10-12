"""Finishing passes: vignette, grain, chromatic aberration, halftone, dither, CRT, pixelate and glitch.

Each segment draws the same test card through one PostProcess preset, set
for the whole frame with ``scene.segment(..., post=...)``. The preset's main
argument rises from a value that leaves the frame untouched to the one in
the label, so the label reads before the look takes over.
"""

import os

from gaanim import Brush, PostProcess, Scene, Zones

LEAD = 0.3
RAMP = 0.6
HOLD = 0.6

scene = Scene()
background = scene.canvas.color("background")
foreground = scene.canvas.color("foreground")
page = scene.layout.zones(Zones.rows(["6fr", "1fr"], gap="24px", padding="page_padding"))


def test_card(zone):
    """Fill ``zone`` with four colored tiles, each holding a dark shape, above a gray ramp."""
    tiles, ramp = zone.split(Zones.rows(["4fr", "1fr"], gap="24px"))
    cells = tiles.split(Zones.columns(4, gap="24px"))
    roles = ["accent", "success", "danger", "chart"]
    shapes = [
        lambda: scene.geometry.circle(1.0),
        lambda: scene.geometry.regular_polygon(3, 1.0),
        lambda: scene.geometry.star(5, 1.0, 0.45),
        lambda: scene.geometry.annulus(1.0, 0.55),
    ]
    for cell, role, shape in zip(cells, roles, shapes):
        scene.geometry.rounded_rect(cell.width, cell.height, 0.2).fill(scene.canvas.color(role)).place(cell)
        shape().fill(background).place(cell.inset("48px"), fit="contain")
    (left, y), (right, _) = ramp.point("left"), ramp.point("right")
    gray = Brush.linear([background, foreground], start=(left, y), end=(right, y))
    scene.geometry.rounded_rect(ramp.width, ramp.height, 0.2).fill(gray).place(ramp)


def finish(template, build, start, end):
    """One segment of the test card through ``build(amount)``, as ``amount`` goes from ``start`` to ``end``.

    Every preset argument takes a number, a Parameter or a Computed; the
    label shows the preset with its final number. Returns the segment start.
    """
    amount = scene.viz.parameter(start)
    code = template.format(end)
    begin = scene.cursor
    scene.segment(code, post=build(amount))
    test_card(page[0])
    scene.text(code, role="code").place(page[1], anchor="left")
    scene.wait(LEAD)
    scene.play([amount.animate.set(end).duration(RAMP)])
    scene.wait(HOLD)
    return begin


# Film finishes: vignette darkens toward the corners (softness sets where the
# falloff starts), grain adds brightness noise in cells of `size` pixels,
# renewed 24 times a second unless animated=False, and chromatic aberration
# splits red and blue toward the corners.
FINISHES = [
    ("PostProcess.vignette({})", lambda amount: PostProcess.vignette(amount), 0.0, 0.8),
    ("PostProcess.vignette({}, softness=0.2)", lambda amount: PostProcess.vignette(amount, softness=0.2), 0.0, 1.0),
    ("PostProcess.grain({})", lambda amount: PostProcess.grain(amount), 0.0, 0.15),
    ("PostProcess.grain({}, size=4)", lambda amount: PostProcess.grain(amount, size=4), 0.0, 0.3),
    ("PostProcess.chromatic_aberration({})", lambda amount: PostProcess.chromatic_aberration(amount), 0.0, 0.02),
    # Styles: print dots, ordered dithering to a few levels per channel, a
    # curved screen with scanlines, blocks, and bands that jump 12 times a
    # second (seed picks another pattern). Sizes are pixels of a 1080p frame.
    ("PostProcess.halftone({})", lambda amount: PostProcess.halftone(amount), 1, 12),
    ("PostProcess.dither({})", lambda amount: PostProcess.dither(amount), 256, 4),
    ("PostProcess.dither({})", lambda amount: PostProcess.dither(amount), 256, 2),
    ("PostProcess.crt({})", lambda amount: PostProcess.crt(amount), 0.0, 1.0),
    ("PostProcess.pixelate({})", lambda amount: PostProcess.pixelate(amount), 1, 16),
    ("PostProcess.glitch({})", lambda amount: PostProcess.glitch(amount), 0.0, 0.5),
    ("PostProcess.glitch({}, seed=3)", lambda amount: PostProcess.glitch(amount, seed=3), 0.0, 1.0),
]
starts = {template.format(end): finish(template, build, start, end) for template, build, start, end in FINISHES}

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Every look at full strength; halftone and pixelate also early in their
    # ramp, while their labels still read.
    scene.snapshots(snapshots, sorted(
        [begin + LEAD + RAMP + HOLD - 0.2 for begin in starts.values()]
        + [starts[code] + LEAD + 0.3 * RAMP for code in ("PostProcess.halftone(12)", "PostProcess.pixelate(16)")]
    ))
else:
    scene.render()
