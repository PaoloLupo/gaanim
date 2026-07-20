"""A native spring that deforms as its mass moves."""

import os

from gaanim import BLACK, BLUE, GOLD, GRAY, WHITE, Scene


scene = Scene(800, 420, background=WHITE)

anchor = scene.dot(9).fill(BLACK).at(-260, 0)
mass = scene.rect(72, 62).fill(GOLD).stroke(BLACK, 3).at(-20, 0)
spring = scene.spring_between(anchor, mass, coils=9, amplitude=18).no_fill().stroke(BLUE, 4)
measurement = scene.dimension_between(anchor, mass, -78).no_fill().stroke(GRAY, 2)
mass_label = scene.text("m").fill(BLACK)
mass_label.follow_to(mass, offset=(0, 54))
rail = scene.line(-285, -48, 245, -48).stroke(GRAY, 4)
label = scene.text("reactive spring").fill(GRAY).at(0, 120)

scene.play([
    rail.create().duration(0.4),
    mass.create().duration(0.5),
    label.write().duration(0.4),
])
scene.play([mass.move(180, 0).duration(1.2).smooth()])
scene.play([mass.move(-110, 0).duration(0.8).smooth()])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.7, 1.4, 2.1, 2.5])

scene.render()
