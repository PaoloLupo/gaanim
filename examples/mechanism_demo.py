"""Technical mechanism diagram built from public vector primitives."""

import os

from gaanim import Easing, Anchor, BLUE, CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background=WHITE)

disk = scene.geometry.circle(3.333333).no_fill().stroke(GRAY, 0.083333).move_to(0, 0.25)
x_axis = scene.geometry.arrow(0.416667, 0, 6, 0).stroke(GRAY, 0.066667)
y_axis = scene.geometry.arrow(0, 0.416667, 0, 3.583333).stroke(GRAY, 0.066667)

# An open polyline makes the rail and zig-zag spring ordinary animatable paths.
rail = scene.geometry.path([(-3.833333, -0.916667), (2.916667, -0.916667), (2.916667, 0.583333), (-3.833333, 0.583333), (-3.833333, -0.916667)]).no_fill().stroke(GRAY, 0.083333)
spring = scene.geometry.path(
    [(-3.5, -0.166667), (-2.916667, 0.416667), (-2.333333, -0.75), (-1.75, 0.416667), (-1.166667, -0.75), (-0.583333, 0.416667), (0, -0.166667)]
).no_fill().stroke(CYAN, 0.083333)
mass = scene.geometry.rect(1.166667, 1.033333).fill(GOLD).stroke(GRAY, 0.066667).move_to(0.8, -0.166667)
mass_label = scene.text("m").fill(GRAY).move_to(0.8, -0.166667, anchor=Anchor.CENTER)
mechanism = scene.geometry.group([rail, spring, mass, mass_label]).move_to(-0.583333, 0.666667).pivot(-0.291667, 0.333333).rotate_to(0.48)

# Technical annotations use dedicated primitives instead of hand-built paths.
angle_arc = scene.geometry.arc(-0.333333, 0.333333, 0.916667, 0.48, 0.22).no_fill().stroke(GRAY, 0.05)
rotation = scene.geometry.curved_arrow(-0.333333, -2.583333, 2.416667, -1.916667, 0.9).fill(GRAY)
omega = scene.text.equation("omega").fill(GRAY).move_to(1.916667, -2.25, anchor=Anchor.CENTER)
extension = scene.mechanics.dimension(1.416667, 2.416667, 2.916667, 2.416667, 0.583333)
extension_label = scene.text("e").fill(GRAY).move_to(2.166667, 3.25, anchor=Anchor.CENTER)

scene.text("x").fill(GRAY).move_to(6.416667, 0.033333, anchor=Anchor.CENTER)
scene.text("y").fill(GRAY).move_to(-0.083333, 3.916667, anchor=Anchor.CENTER)
equation = scene.text.equation("eta'' + (2k/m - omega^2) eta = 0").fill(GRAY).move_to(0, -3.583333, anchor=Anchor.CENTER)

scene.play([
    disk.animate.create().duration(0.7),
    x_axis.animate.create().duration(0.5),
    y_axis.animate.create().duration(0.5),
    rail.animate.create().duration(1.1),
    spring.animate.create().duration(1.1),
    angle_arc.animate.create().duration(0.6),
    rotation.animate.create().duration(0.7),
    omega.animate.write().duration(0.4),
    extension.animate.create().duration(0.8),
    extension_label.animate.write().duration(0.4),
    equation.animate.write().duration(0.8),
])
scene.play([mechanism.animate.rotate_by(0.16).duration(0.8).easing(Easing.SMOOTH)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.6, 1.4, 1.8])

scene.render()
