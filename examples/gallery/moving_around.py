"""A square that slides, recolors, shrinks and turns, one call at a time and then all at once, squashing as it speeds.

Each step plays one animation of ``square.animate`` with its own easing and
shows the call below the stage. squash_stretch deforms the square along its
velocity, a trail of echoes shows its speed and a soft shadow keeps it on
the floor; the last step undoes every change in one parallel move.
"""

import os

from gaanim import Easing, Scene, Zones

scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
gold = scene.canvas.color("title")
muted = scene.canvas.color("muted")
shadow = scene.canvas.color("shadow")

stage, band = scene.layout.zones(Zones.rows(["5fr", "1fr"], padding="page_padding"))
x, y = stage.center
square = (
    scene.geometry.square(2.2)
    .fill(violet)
    .no_stroke()
    .move_to(x + 2.5, y)
    .shadow(shadow, x=0.15, y=-0.2, blur=0.25)
    .squash_stretch(amount=0.25)
    .echo(6, delay=0.05, decay=0.55)
)

STEPS = [
    ("square.animate.shift_by(-5, 0)", square.animate.shift_by(-5, 0).easing(Easing.SMOOTH_SPRING)),
    ("square.animate.fill(gold)", square.animate.fill(gold).easing(Easing.SMOOTH)),
    ("square.animate.scale_to(0.4)", square.animate.scale_to(0.4).easing(Easing.back())),
    ("square.animate.rotate_by(0.8)", square.animate.rotate_by(0.8).easing(Easing.elastic())),
]
captions = []
scene.play(square.animate.grow_from_center().easing(Easing.back()), duration=0.6)
for code, step in STEPS:
    caption = scene.text(code, role="code", color=muted).place(band, anchor="center").hidden()
    scene.play([caption.animate.fade_in()], duration=0.3)
    scene.play(step, duration=1.2)
    scene.wait(0.2)
    scene.play([caption.animate.fade_out()], duration=0.3)

caption = scene.text("parallel(...)", role="code", color=muted).place(band, anchor="center").hidden()
scene.play([caption.animate.fade_in()], duration=0.3)
scene.play(
    [
        square.animate.shift_by(5, 0),
        square.animate.fill(violet),
        square.animate.scale_to(1.0),
        square.animate.rotate_by(-0.8),
    ],
    duration=1.6,
    easing=Easing.SMOOTH_SPRING,
)
scene.wait(0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid slide, recolored, shrunk, turning, and the way back.
    scene.snapshots(snapshots, [1.3, 3.3, 5.2, 7.3, 9.6, 10.6])
else:
    scene.render()
