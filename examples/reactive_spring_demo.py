"""A native helical spring that deforms as its mass moves."""

import os

from gaanim import Easing, Anchor, BLACK, BLUE, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(17.142857, 9), background=WHITE)

anchor = scene.geometry.dot(0.192857).fill(BLACK).move_to(-5.571429, 0)
mass = scene.geometry.rect(1.542857, 1.328571).fill(GOLD).stroke(BLACK, 0.064286).move_to(-0.428571, 0)
spring = scene.mechanics.spring_between(anchor, mass, coils=10, amplitude=0.321429, crossing=1.0).no_fill().stroke(BLUE, 0.064286)
measurement = scene.mechanics.dimension_between(anchor, mass, -1.671429).no_fill().stroke(GRAY, 0.042857)
mass_label = scene.text("m").fill(BLACK)
mass_label.follow_to(mass, offset=(0, 1.157143))
rail = scene.geometry.line(-6.107143, -1.028571, 5.25, -1.028571).stroke(GRAY, 0.085714)
label = scene.text("reactive helical spring").fill(GRAY).move_to(0, 2.571429, anchor=Anchor.CENTER)

scene.play([
    anchor.animate.fade_in(),
    rail.animate.create().duration(0.4),
    mass.animate.create().duration(0.5),
    spring.animate.fade_in().duration(0.3),
    measurement.animate.fade_in().duration(0.3),
    mass_label.animate.fade_in().duration(0.3),
    label.animate.write().duration(0.4),
])
scene.play([mass.animate.shift_by(3.857143, 0).duration(1.2).easing(Easing.SMOOTH)])
scene.play([mass.animate.shift_by(-2.357143, 0).duration(0.8).easing(Easing.SMOOTH)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.7, 1.4, 2.1, 2.5])

scene.render()
