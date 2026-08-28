"""Editorial gears, rack, cam contact, moment, and native visual couplings."""

import os
from math import cos, pi, sin

from gaanim import BLACK, BLUE, Direction, GOLD, GREEN, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK, margin=0.6, theme="technical")
scene.play([scene.text("Transmisiones y contacto", role="title").to_edge(Direction.UP).animate.write()])

driver = scene.mechanics.gear(1.075, 24, bore_radius=0.15, color=GOLD).move_to(-3.75, 0.5)
driven = scene.mechanics.gear(0.625, 14, bore_radius=0.1125, color=BLUE).move_to(-1.75, 0.5)
driven.bind_rotation_from(driver, ratio=-24 / 14)
rack = scene.mechanics.rack(4.5, 24, color=WHITE).move_to(-2.625, -1.5625)
rack.bind_translation_from_rotation(driver, axis=Direction.RIGHT, scale=1.075)

samples = [(2 * pi * i / 96, 0.8375 + 0.225 * cos(2 * pi * i / 96)) for i in range(96)]
cam = scene.mechanics.cam_profile(samples, bore_radius=0.125, color=GREEN).move_to(3.75, 0.4375)
cam_curve = scene.geometry.polygon(
    [(3.75 + radius * cos(angle), 0.4375 + radius * sin(angle)) for angle, radius in samples]
).opacity(0)
tracker = scene.viz.parameter(0.08)
contact = scene.mechanics.contact_on_curve(cam_curve, tracker, tangent_length=1.125, normal_length=1.5)
moment = scene.mechanics.moment_about(cam, 1.3125, direction="ccw", label="$M$")

scene.play([driver.animate.fade_in(), driven.animate.fade_in(), rack.animate.fade_in(), cam.animate.fade_in(), contact.animate.fade_in(), moment.animate.fade_in()])
scene.play([driver.animate.rotate_by(2 * pi).duration(4.0), tracker.animate.set(0.85).duration(4.0)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.2, 2.8, 4.6])
else:
    scene.render()
