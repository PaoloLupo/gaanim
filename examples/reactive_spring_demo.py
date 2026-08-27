"""A native helical spring that deforms as its mass moves."""

import os

from gaanim import Easing, Anchor, BLACK, BLUE, GOLD, GRAY, WHITE, Scene


scene = Scene(800, 420, background=WHITE)

anchor = scene.geometry.dot(9).fill(BLACK).move_to(-260, 0)
mass = scene.geometry.rect(72, 62).fill(GOLD).stroke(BLACK, 3).move_to(-20, 0)
spring = scene.mechanics.spring_between(anchor, mass, coils=10, amplitude=15, crossing=1.0).no_fill().stroke(BLUE, 3)
measurement = scene.mechanics.dimension_between(anchor, mass, -78).no_fill().stroke(GRAY, 2)
mass_label = scene.text("m").fill(BLACK)
mass_label.follow_to(mass, offset=(0, 54))
rail = scene.geometry.line(-285, -48, 245, -48).stroke(GRAY, 4)
label = scene.text("reactive helical spring").fill(GRAY).move_to(0, 120, anchor=Anchor.CENTER)

scene.play([
    anchor.animate.fade_in(),
    rail.animate.create().duration(0.4),
    mass.animate.create().duration(0.5),
    spring.animate.fade_in().duration(0.3),
    measurement.animate.fade_in().duration(0.3),
    mass_label.animate.fade_in().duration(0.3),
    label.animate.write().duration(0.4),
])
scene.play([mass.animate.shift_by(180, 0).duration(1.2).easing(Easing.SMOOTH)])
scene.play([mass.animate.shift_by(-110, 0).duration(0.8).easing(Easing.SMOOTH)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.7, 1.4, 2.1, 2.5])

scene.render()
