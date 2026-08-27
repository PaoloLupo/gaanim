"""Editorial gears, rack, cam contact, moment, and native visual couplings."""

import os
from math import cos, pi, sin

from gaanim import BLACK, BLUE, Direction, GOLD, GREEN, WHITE, Scene


scene = Scene(1280, 720, background=BLACK, margin=48, theme="technical")
scene.play([scene.text("Transmisiones y contacto", role="title").to_edge(Direction.UP).animate.write()])

driver = scene.mechanics.gear(86, 24, bore_radius=12, color=GOLD).move_to(-300, 40)
driven = scene.mechanics.gear(50, 14, bore_radius=9, color=BLUE).move_to(-140, 40)
driven.bind_rotation_from(driver, ratio=-24 / 14)
rack = scene.mechanics.rack(360, 24, color=WHITE).move_to(-210, -125)
rack.bind_translation_from_rotation(driver, axis=Direction.RIGHT, scale=86)

samples = [(2 * pi * i / 96, 67 + 18 * cos(2 * pi * i / 96)) for i in range(96)]
cam = scene.mechanics.cam_profile(samples, bore_radius=10, color=GREEN).move_to(300, 35)
cam_curve = scene.geometry.polygon(
    [(300 + radius * cos(angle), 35 + radius * sin(angle)) for angle, radius in samples]
).opacity(0)
tracker = scene.viz.parameter(0.08)
contact = scene.mechanics.contact_on_curve(cam_curve, tracker, tangent_length=90, normal_length=120)
moment = scene.mechanics.moment_about(cam, 105, direction="ccw", label="$M$")

scene.play([driver.animate.fade_in(), driven.animate.fade_in(), rack.animate.fade_in(), cam.animate.fade_in(), contact.animate.fade_in(), moment.animate.fade_in()])
scene.play([driver.animate.rotate_by(2 * pi).duration(4.0), tracker.animate.set(0.85).duration(4.0)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.2, 2.8, 4.6])
else:
    scene.render()
