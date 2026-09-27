"""The Gaanim symbol drawn by echo: one tween and its two past frames.

The brand symbol (tools/generate_brand.py) shows three frames of one tween,
square -> squircle -> circle, stepping 2 pixels right as onion skin. Here one
pixel shape moves vertex by vertex through those frames with `animate.points`,
and `echo(2, delay=STAGE)` draws the frames it has just left behind.
"""

import os

from gaanim import Easing, Scene

# Row widths of each 8x8 pixel frame, back to front, and dark-theme colors.
FRAMES = (
    (8, 8, 8, 8, 8, 8, 8, 8),  # square
    (6, 8, 8, 8, 8, 8, 8, 6),  # squircle
    (4, 6, 8, 8, 8, 8, 6, 4),  # circle
)
COLORS = ("#3F37A8", "#7C6CFF", "#FFC933")
PIXEL = 0.5  # scene units per brand pixel
STEP = 2 * PIXEL  # the frames step 2 pixels right
STAGE = 0.6  # seconds from one frame to the next


def outline(widths, cx):
    """Pixel frame centered on (cx, 0), always with the same 32 vertices."""
    top = len(widths) * PIXEL / 2
    right, left = [], []
    for row, width in enumerate(widths):
        y0, y1 = top - row * PIXEL, top - (row + 1) * PIXEL
        half = width * PIXEL / 2
        right += [(cx + half, y0), (cx + half, y1)]
        left += [(cx - half, y1), (cx - half, y0)]
    return right + left[::-1]


scene = Scene(frame=(16, 9), background="#1A1A1A")
start = -STEP
symbol = scene.geometry.polygon(outline(FRAMES[0], start)).fill(COLORS[0])
# Copy k shows the tween k * STAGE seconds ago, one frame back per copy.
symbol.echo(2, delay=STAGE, decay=1.0)
for index in (1, 2):
    scene.play(
        [
            symbol.animate.points(outline(FRAMES[index], start + index * STEP))
            .fill(COLORS[index])
            .duration(STAGE)
            .easing(Easing.LINEAR)
        ]
    )
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Halfway to the squircle, the squircle with the square behind it, the
    # complete symbol, and the copies catching up once the tween stops.
    scene.snapshots(snapshots, [0.3, STAGE, 2 * STAGE, 2 * STAGE + 0.3, 2 * STAGE + 1.0])
else:
    scene.render()
