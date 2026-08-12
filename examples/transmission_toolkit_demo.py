"""Editorial gears, rack, cam contact, moment, and native visual couplings."""

import os
from math import cos, pi, sin

from gaanim import BLACK, BLUE, Direction, GOLD, GREEN, WHITE, Scene


scene = Scene(1280, 720, background=BLACK, margin=48, theme="technical")
scene.text("Transmisiones y contacto", role="title").to_edge(Direction.UP).write()

driver = scene.gear(86, 24, bore_radius=12, color=GOLD).at(-300, 40)
driven = scene.gear(50, 14, bore_radius=9, color=BLUE).at(-140, 40)
driven.bind_rotation_from(driver, ratio=-24 / 14)
rack = scene.rack(360, 24, color=WHITE).at(-210, -125)
rack.bind_translation_from_rotation(driver, axis=Direction.RIGHT, scale=86)

samples = [(2 * pi * i / 96, 67 + 18 * cos(2 * pi * i / 96)) for i in range(96)]
cam = scene.cam_profile(samples, bore_radius=10, color=GREEN).at(300, 35)
cam_curve = scene.polygon(
    [(300 + radius * cos(angle), 35 + radius * sin(angle)) for angle, radius in samples]
).opacity(0)
tracker = scene.parameter(0.08)
contact = scene.contact_on_curve(cam_curve, tracker, tangent_length=90, normal_length=120)
moment = scene.moment_about(cam, 105, direction="ccw", label="$M$")

scene.play([driver.fade_in(), driven.fade_in(), rack.fade_in(), cam.fade_in(), contact.fade_in(), moment.fade_in()])
scene.play([driver.rotate(2 * pi).duration(4.0), tracker.animate_to(0.85, duration=4.0)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.2, 2.8, 4.6])
else:
    scene.render()
