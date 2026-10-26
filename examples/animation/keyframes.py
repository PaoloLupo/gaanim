"""Keyframes: one clip through several stops, on any channel, with an easing per segment.

``animate.keyframes(times, ...)`` takes one value per time for each channel it
animates, and ``None`` keeps the value the drawable has when the clip starts.
The first segment animates every channel, the second the easings between
stops, the third how a position travels between them.
"""

import math
import os

from gaanim import Easing, Scene, Zones

DURATION = 1.6
HOLD = 0.4

scene = Scene()
foreground = scene.colors.foreground
rule = scene.colors.rule
success = scene.colors.success
danger = scene.colors.danger


def table(header, rows):
    """One row per keyframed drawable, its code to the left of it.

    ``header`` labels the call the rows share; ``rows`` are (label, build)
    pairs, and ``build(zone)`` draws in the row's stage and returns the clip.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows) + 1, columns=["4fr", "3fr"], column_gap="48px", padding="page_padding")
    )
    scene.text(header, role="code").place(grid[0], anchor="right")
    clips = []
    for index, (label, build) in enumerate(rows, start=1):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        clips.append(build(grid[2 * index + 1].inset((0, "48px"))).duration(DURATION))
    scene.play(clips)
    scene.wait(HOLD)


def square(zone, anchor="center"):
    """A square at ``anchor`` of ``zone``."""
    return scene.geometry.square(0.5).move_to(*zone.point(anchor))


def gauge(zone):
    """A circle scaled by a Parameter, and the keyframes of that Parameter."""
    level = scene.viz.parameter(1.0)
    scene.geometry.circle(0.25).move_to(*zone.center).scale_to(level)
    return level.animate.keyframes([0, 0.5, 1], values=[None, 1.6, 0.6])


# Each channel takes one value per time: displacements from the start (offset=,
# or absolute position= as in the next segments), angles, scale factors
# (uniform or per axis), opacities and paints. On a Parameter, values=
# keyframes the number itself.
channels_start = scene.cursor
scene.segment("channels")
table("keyframes(times=[0, 0.5, 1], ...)", [
    ("offset=[None, (2.5, 0.4), (5, 0)]",
     lambda z: square(z, "left").animate.keyframes([0, 0.5, 1], offset=[None, (2.5, 0.4), (5, 0)])),
    ("rotation=[None, math.pi / 4, 0]",
     lambda z: square(z).animate.keyframes([0, 0.5, 1], rotation=[None, math.pi / 4, 0])),
    ("scale=[None, 1.6, 1]",
     lambda z: square(z).animate.keyframes([0, 0.5, 1], scale=[None, 1.6, 1])),
    ("scale=[None, (2.4, 0.5), 1]",
     lambda z: square(z).animate.keyframes([0, 0.5, 1], scale=[None, (2.4, 0.5), 1])),
    ("opacity=[None, 0.15, 1]",
     lambda z: square(z).animate.keyframes([0, 0.5, 1], opacity=[None, 0.15, 1])),
    ("fill=[None, success, danger]",
     lambda z: square(z).animate.keyframes([0, 0.5, 1], fill=[None, success, danger])),
    ("stroke=[None, success, danger]",
     lambda z: square(z).stroke(foreground, 0.08).animate.keyframes([0, 0.5, 1], stroke=[None, success, danger])),
    ("values=[None, 1.6, 0.6]", gauge),
])


def track(zone, easing, remap=None):
    """Keyframe a dot through three stops along ``zone``: halfway at 30% of the time."""
    (start, y), (end, _) = zone.point("left"), zone.point("right")
    middle = (start + end) / 2
    scene.geometry.line(start, y, end, y).stroke(rule, 0.02)
    for x in (start, middle, end):
        scene.geometry.rect(0.02, 0.24).fill(rule).move_to(x, y)
    dot = scene.geometry.dot(0.12).move_to(start, y)
    clip = dot.animate.keyframes([0, 0.3, 1], position=[None, (middle, y), (end, y)], easing=easing)
    return clip if remap is None else clip.easing(remap)


# The easing of each segment between stops: one for every segment, or a list
# with one per segment. The clip's own .easing() remaps the whole clip instead,
# so the stops no longer land at their times.
easing_start = scene.cursor
scene.segment("easing")
table("keyframes(times=[0, 0.3, 1], ...)", [
    ("easing=None", lambda z: track(z, None)),
    ("easing=Easing.SMOOTH", lambda z: track(z, Easing.SMOOTH)),
    ("easing=[Easing.SMOOTH, Easing.BOUNCY]", lambda z: track(z, [Easing.SMOOTH, Easing.BOUNCY])),
    ("easing=[Easing.LINEAR, Easing.steps(4)]", lambda z: track(z, [Easing.LINEAR, Easing.steps(4)])),
    ("easing=[Easing.QUICK, Easing.LINGERING]", lambda z: track(z, [Easing.QUICK, Easing.LINGERING])),
    ("...).easing(Easing.SMOOTH)", lambda z: track(z, None, Easing.SMOOTH)),
])

# spatial= sets how position travels between stops: straight lines, or a
# smooth Catmull-Rom curve through every stop.
spatial_start = scene.cursor
scene.segment("spatial")
panels = scene.layout.zones(
    Zones.grid(rows=["1fr", "8fr"], columns=2, column_gap="96px", row_gap="24px", padding="page_padding")
)
clips = []
for column, mode in enumerate(["linear", "catmull_rom"]):
    scene.text(f'spatial="{mode}"', role="code").place(panels[column], anchor="center")
    stage = panels[2 + column].inset("48px")
    stops = [
        (stage.left + stage.width * fx, stage.bottom + stage.height * fy)
        for fx, fy in [(0.0, 0.1), (0.25, 0.9), (0.5, 0.1), (0.75, 0.9), (1.0, 0.1)]
    ]
    for x, y in stops:
        scene.geometry.dot(0.06).fill(rule).move_to(x, y)
    ball = scene.geometry.circle(0.22).move_to(*stops[0])
    trail = scene.geometry.traced_path(ball).stroke(foreground, 0.02)
    clips.append(trail.animate.fade_in().duration(0.1))
    clips.append(
        ball.animate.keyframes([0, 0.25, 0.5, 0.75, 1], position=[None, *stops[1:]], spatial=mode).duration(2.4)
    )
scene.play(clips)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Channels and easings early, at the middle stop, late and settled; the
    # spatial modes a quarter, half and all the way through.
    scene.snapshots(
        snapshots,
        [channels_start + t for t in (0.4, 0.8, 1.2, 1.9)]
        + [easing_start + t for t in (0.3, 0.6, 1.0, 1.9)]
        + [spatial_start + t for t in (0.7, 1.3, 2.0, 2.7)],
    )
else:
    scene.render()
