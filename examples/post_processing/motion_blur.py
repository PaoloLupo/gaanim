"""Motion blur: sub-frame averaging that smears motion by its speed, and drawables kept sharp.

``scene.canvas.motion_blur`` averages sub-frames over the time a film
shutter stays open, in exports and snapshots (the interactive preview stays
sharp). The first segment moves dots across their tracks at three speeds,
the second spins, scales and orbits shapes, and the last four give a
segment a shutter of its own. Each label is the code that moves its row or
panel, or that sets its segment's shutter.
"""

import math
import os

from gaanim import Easing, Scene, Zones

scene = Scene()
rule = scene.colors.rule

# A 360 degree shutter stays open for a whole frame (180 is the film
# standard, 720 the maximum), sampled 32 times; phase= moves the shutter
# relative to the frame time and None turns the blur off. It applies to
# every segment without a shutter of its own.
scene.canvas.motion_blur(360, samples=32)

# Every dot crosses its track at a constant speed: the faster it goes, the
# longer its smear. motion_blur(False) keeps one drawable sharp.
speeds_start = scene.cursor
scene.segment("speeds")
heading, body = scene.layout.zones(Zones.rows(["2fr", "5fr"], gap="24px", padding="page_padding"))
lines = heading.split(Zones.rows(2))
scene.text("scene.canvas.motion_blur(360, samples=32)", role="code").place(lines[0], anchor="left")
scene.text("move = dot.animate.shift_by(width, 0).easing(Easing.LINEAR)", role="code").place(lines[1], anchor="left")
rows = body.split(Zones.grid(rows=4, columns=["3fr", "5fr"], column_gap="48px"))
SPEEDS = [
    ("move.duration(1.6)", 1.6, True),
    ("move.duration(0.4)", 0.4, True),
    ("move.duration(0.2)", 0.2, True),
    ("dot.motion_blur(False)", 0.2, False),
]
moves = []
for index, (label, duration, blurred) in enumerate(SPEEDS):
    scene.text(label, role="code").place(rows[2 * index], anchor="right")
    track = rows[2 * index + 1]
    (start, y), (end, _) = track.point("left"), track.point("right")
    scene.geometry.line(start, y, end, y).stroke(rule, 0.02)
    dot = scene.geometry.dot(0.32).move_to(start, y).motion_blur(blurred)
    moves.append(dot.animate.shift_by(track.width, 0).easing(Easing.LINEAR).duration(duration))
scene.wait(0.3)
scene.play(moves)
scene.wait(0.3)


def spin(stage, size):
    """A bar across ``stage`` that turns once."""
    x, y = stage.center
    return scene.geometry.rect(size, size * 0.12).move_to(x, y).animate.rotate_by(math.tau)


def shrink(stage, size):
    """A ring filling ``stage`` that shrinks."""
    x, y = stage.center
    return scene.geometry.annulus(size / 2, size / 2 - 0.3).move_to(x, y).animate.scale_to(0.4)


def orbit(stage, size):
    """A dot that goes once around a circle filling ``stage``."""
    x, y = stage.center
    path = scene.geometry.circle(size / 2 - 0.3).no_fill().stroke(rule, 0.02).move_to(x, y)
    return scene.geometry.dot(0.32).move_to(x + size / 2 - 0.3, y).animate.move_along(path)


# Rotation, scale and travel along a curve (each label is the animate call of
# its shape) blur along the path each point follows within the frame.
shapes_start = scene.cursor
scene.segment("shapes")
panels = scene.layout.zones(Zones.columns(3, gap="48px", padding="page_padding"))
plays = []
for panel, (label, build) in zip(panels, [
    ("rotate_by(math.tau)", spin),
    ("scale_to(0.4)", shrink),
    ("move_along(orbit)", orbit),
]):
    stage, caption = panel.split(Zones.rows(["5fr", "1fr"]))
    scene.text(label, role="code").place(caption)
    plays.append(build(stage, min(stage.width, stage.height) * 0.9).duration(0.5).easing(Easing.LINEAR))
scene.wait(0.3)
scene.play(plays)
scene.wait(0.3)

# Segment.motion_blur gives one segment a shutter of its own, with the
# arguments of scene.canvas.motion_blur; None keeps it sharp. A ring kept
# sharp marks where the dot is at the frame time: a centered shutter smears
# around it, while phase=0 opens the shutter there and smears ahead.
SHUTTERS = [
    ("narrow", {"shutter_angle": 90}, "motion_blur(90, samples=32)"),
    ("wide", {"shutter_angle": 720}, "motion_blur(720, samples=32)"),
    ("ahead", {"shutter_angle": 360, "phase": 0}, "motion_blur(360, samples=32, phase=0)"),
    ("sharp", None, "motion_blur(None)"),
]
shutter_starts = []
for name, options, call in SHUTTERS:
    shutter_starts.append(scene.cursor)
    segment = scene.segment(name)
    if options is None:
        segment.motion_blur(None)
    else:
        segment.motion_blur(samples=32, **options)
    heading, body = scene.layout.zones(Zones.rows(["2fr", "5fr"], gap="24px", padding="page_padding"))
    scene.text(f'scene.segment("{name}").{call}', role="code").place(heading, anchor="left")
    track = body.inset(("0px", "96px"))
    (start, y), (end, _) = track.point("left"), track.point("right")
    scene.geometry.line(start, y, end, y).stroke(rule, 0.02)
    dot = scene.geometry.dot(0.32).move_to(start, y)
    ring = scene.geometry.circle(0.42).no_fill().stroke(rule, 0.03).move_to(start, y).motion_blur(False)
    scene.wait(0.3)
    scene.play([
        shape.animate.shift_by(track.width, 0).easing(Easing.LINEAR).duration(0.4) for shape in (dot, ring)
    ])
    scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The dots while all four move and as the slowest goes on alone; the
    # shapes at a quarter and halfway through their motion; each shutter
    # halfway along the track.
    scene.snapshots(
        snapshots,
        [speeds_start + 0.4, speeds_start + 0.8, shapes_start + 0.42, shapes_start + 0.55]
        + [start + 0.5 for start in shutter_starts],
    )
else:
    scene.render()
