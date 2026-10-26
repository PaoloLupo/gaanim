"""The product rule written term by term, with a frame and a band that glide from one term to the next.

Each term of the equation is a named part. The frame is a live
surrounding_rect: retarget slides its edges onto another part and it stays
on it, and the shaded band behind it does the same while the framed term
takes the accent color.
"""

import os

from gaanim import Easing, Scene, part

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
paper = scene.colors.foreground
geometry = scene.geometry

rule = scene.text.equation(
    part("derivative", "frac(d, d x) [f(x) g(x)]"),
    " = ",
    part("first", "f(x) frac(d g, d x)"),
    " + ",
    part("second", "g(x) frac(d f, d x)"),
    size=0.8,
).move_to(0, 0.3)

frame = geometry.surrounding_rect(rule["derivative"], padding=0.2, corner_radius=0.0).stroke(gold, 0.05)
band = geometry.surrounding_rect(rule["derivative"], padding=0.2, corner_radius=0.0).fill(violet).opacity(0.25)
band.no_stroke().z_index(-1)

scene.play(rule.animate.write(by="part"), duration=2.4)
scene.play([frame.animate.create(), band.animate.fade_in()], duration=0.8)
scene.wait(0.6)
for name in ("first", "second", "derivative"):
    scene.play(
        [frame.retarget(rule[name]), band.retarget(rule[name]), rule[name].animate.fill(gold)],
        duration=1.1,
        easing=Easing.SMOOTH_SPRING,
    )
    scene.wait(0.5)
    scene.play(rule[name].animate.fill(paper), duration=0.3)
scene.play([frame.retarget([rule["first"], rule["second"]]), band.retarget([rule["first"], rule["second"]])],
           duration=1.2, easing=Easing.SMOOTH)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Written, framed, on each term in turn, and around the whole right side.
    scene.snapshots(snapshots, [1.4, 3.4, 4.6, 6.5, 8.4, 11.2])
else:
    scene.render()
