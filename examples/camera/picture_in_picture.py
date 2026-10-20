"""Picture in picture: a demo that starts full screen moves into a screen at the side and keeps playing.

The demo sits on a view layer, so the main camera does not draw it. A
rectangle as large as the frame becomes a screen whose ``camera_view`` sees
the whole frame through a frame of the same size: while the screen covers the
frame the demo looks full screen. The zoom is the screen's size over the
frame's, so placing the screen in a smaller zone shrinks the view with it,
clipped to the screen, while the demo's own animations go on, and its border
grows in as it leaves the edges. ``exclude`` keeps what else the frame shows,
the characters watching and the captions, out of the screen. Each caption is
the call that moves the screen.
"""

import math
import os

from gaanim import Easing, Scene, Zones

MOVE = 1.5
HOLD = 1.5
BORDER = 0.04
SEEKS = []

scene = Scene()
scene.segment("picture_in_picture")
accent = scene.canvas.color("accent")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")

whole = scene.layout.frame
page = scene.layout.zones(Zones.rows(["1fr", "80px"]))
stage = page[0].inset("page_padding")
corner, _, crowd, _ = stage.split(Zones.grid(rows=2, columns=2, gap="48px"))
right = stage.split(Zones.columns(2, gap="48px"))[1]

# The demo, authored at full size around the middle of the frame, on a layer
# that only the screen shows: a radius sweeping a circle.
cx, cy = whole.center
radius = whole.height * 0.36
circle = scene.geometry.circle(radius).no_fill().stroke(muted, 0.03).move_to(cx, cy)
axis = scene.geometry.line(whole.left, cy, whole.right, cy).stroke(rule, 0.02)
arm = scene.geometry.line(cx, cy, cx + radius, cy).stroke(accent, 0.06)
tip = scene.geometry.dot(0.12).fill(accent).move_to(cx + radius, cy)
spinner = scene.geometry.group(arm, tip, origin=(cx, cy))
scene.geometry.group(circle, axis, spinner).view_layer("demo")

# The screen covers the frame, with no border while it fills it.
screen = scene.geometry.rect(whole.width, whole.height).no_fill().stroke(muted, 0).move_to(cx, cy)

# Characters watch from below and captions name each move; the screen does
# not show them, so they exist before it does.
watchers = [
    scene.character(name=name, size=zone.height).place(zone).hidden()
    for name, zone in zip(("ana", "beto", "caro"), crowd.split(Zones.columns(3, gap="24px")))
]
strip = page[1].inset((0, "page_padding"))
captions = [
    scene.text(label, role="code").place(strip, anchor="left").hidden()
    for label in (
        'screen.animate.place(corner, fit="contain")',
        'screen.animate.place(right, fit="contain")',
        'screen.animate.place(whole, fit="contain")',
    )
]
frame = scene.geometry.rect(whole.width, whole.height).no_fill().no_stroke().move_to(cx, cy)
screen.camera_view(frame, layers=["demo"], exclude=watchers + captions)


def move(index, *moves):
    """Swap in caption ``index``, play ``moves`` over it and hold the pose."""
    swap = [captions[index].animate.fade_in().duration(0.2)]
    if index:
        swap.append(captions[index - 1].animate.fade_out().duration(0.15))
    SEEKS.append(scene.cursor + MOVE / 2)
    scene.play(swap + [step.duration(MOVE) for step in moves])
    SEEKS.append(scene.cursor + HOLD / 2)
    scene.wait(HOLD)


# The demo runs on its own clock, across every move below.
scene.launch(spinner.animate.rotate_by(math.tau * 2).duration(1 + 3 * (MOVE + HOLD)).easing(Easing.LINEAR))
SEEKS.append(0.5)
scene.wait(1)
move(0, screen.animate.place(corner, fit="contain"), screen.animate.stroke_width(BORDER),
     *(watcher.animate.fade_in() for watcher in watchers))
move(1, screen.animate.place(right, fit="contain"))
move(2, screen.animate.place(whole, fit="contain"), screen.animate.stroke_width(0),
     *(watcher.animate.fade_out() for watcher in watchers))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
