"""Technical mechanism diagram built from public vector primitives."""

import os

from gaanim import BLUE, CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(960, 540, background=WHITE)

disk = scene.circle(200).no_fill().stroke(GRAY, 5).at(0, 15)
x_axis = scene.arrow(25, 0, 360, 0).stroke(GRAY, 4)
y_axis = scene.arrow(0, 25, 0, 215).stroke(GRAY, 4)

# An open polyline makes the rail and zig-zag spring ordinary animatable paths.
rail = scene.polyline([(-230, -55), (175, -55), (175, 35), (-230, 35), (-230, -55)]).no_fill().stroke(GRAY, 5)
spring = scene.polyline(
    [(-210, -10), (-175, 25), (-140, -45), (-105, 25), (-70, -45), (-35, 25), (0, -10)]
).no_fill().stroke(CYAN, 5)
mass = scene.rect(70, 62).fill(GOLD).stroke(GRAY, 4).at(48, -10)
mass_label = scene.text("m").fill(GRAY).at(48, -10)
mechanism = scene.group([rail, spring, mass, mass_label]).rotated(0.48).at(-35, 40)

# Technical annotations use dedicated primitives instead of hand-built paths.
angle_arc = scene.arc(-20, 20, 55, 0.48, 0.22).no_fill().stroke(GRAY, 3)
rotation = scene.curved_arrow(-20, -155, 145, -115, 0.9).fill(GRAY)
omega = scene.text("ω").fill(GRAY).at(115, -135)
extension = scene.dimension(85, 145, 175, 145, 35)
extension_label = scene.text("e").fill(GRAY).at(130, 195)

scene.text("x").fill(GRAY).at(385, 2)
scene.text("y").fill(GRAY).at(-5, 235)
equation = scene.equation("eta'' + (2k/m - omega^2) eta = 0").fill(GRAY).at(0, -215)

scene.play([
    disk.create().duration(0.7),
    x_axis.create().duration(0.5),
    y_axis.create().duration(0.5),
    rail.create().duration(1.1),
    spring.create().duration(1.1),
    angle_arc.create().duration(0.6),
    rotation.create().duration(0.7),
    omega.write().duration(0.4),
    extension.create().duration(0.8),
    extension_label.write().duration(0.4),
    equation.write().duration(0.8),
])
scene.play([mechanism.rotate(0.16).duration(0.8).smooth()])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.6, 1.4, 1.8])

scene.render()
