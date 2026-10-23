"""Deform 3D: a displacement field that bends a whole model, eased in or swayed by a parameter.

``drawable.deform(field)`` and ``drawable.animate.deform(field)`` displace a
group and everything in it by ``field(x, y, z) -> (dx, dy, dz)``, read in the
world: the nodes move, the columns drawn as ``line_3d`` between them follow,
the slabs move with their floor and the wall, a subdivided ``plane``, bends
vertex by vertex. Here the field is a structure's first mode, a quarter sine
up its height. Each label is the code that deforms the model.
"""

import math
import os

from gaanim import Material3D, Scene, Zones

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
muted = scene.canvas.color("muted")
foreground = scene.canvas.color("foreground")

HEIGHT = 6.0
FLOORS = (0.0, 3.0, 6.0)
CORNERS = [(x, z) for x in (-2.0, 2.0) for z in (-1.5, 1.5)]


def mode(x, y, z):
    """The first mode: x moves with a quarter sine of the height."""
    return (math.sin(math.pi * y / (2 * HEIGHT)), 0.0, 0.0)


def frame(label):
    """A two-storey frame, its label pinned at the top, under a 3D camera."""
    zones = scene.layout.zones(Zones.rows(["120px", "1fr"], padding="page_padding"))
    scene.text(label, role="code").hud().place(zones[0], anchor="left")
    scene.camera.to(scene.camera.state_3d(eye=(9, 7, 13), target=(0, 2.6, 0), fov_y=0.7, screen_offset=(0, -0.6)))
    nodes = {
        (x, y, z): scene.geometry.sphere(0.12, material=Material3D.matte(accent)).move_to_3d(x, y, z)
        for x, z in CORNERS
        for y in FLOORS
    }
    columns = [
        scene.geometry.line_3d(nodes[(x, low, z)], nodes[(x, high, z)], 0.08, material=Material3D.matte(muted))
        for x, z in CORNERS
        for low, high in zip(FLOORS, FLOORS[1:])
    ]
    slabs = [
        scene.geometry.cube(1.0, material=Material3D.matte(foreground)).scale_to_3d(4.4, 0.15, 3.4).move_to_3d(0, y, 0)
        for y in FLOORS[1:]
    ]
    # A wall standing in y/z on the left side, subdivided so it can bend.
    wall = scene.geometry.plane(HEIGHT, 3.0, subdivisions=(12, 4), material=Material3D.matte(success))
    # Turned so its lit side faces in, toward the camera.
    wall.rotate_to_3d(0, 0, -math.pi / 2).move_to_3d(-2.0, HEIGHT / 2, 0)
    floor = scene.geometry.plane(7.0, 5.0, material=Material3D.matte(muted)).move_to_3d(0, -0.01, 0)
    return scene.geometry.group(*nodes.values(), *columns, *slabs, wall), floor


shown = []

# animate.deform eases the model into the field, and deform(None) back.
scene.segment("mode")
model, _ = frame("model.animate.deform(mode)")
scene.wait(0.4)
scene.play(model.animate.deform(mode).duration(1.5))
shown.append(scene.cursor - 0.1)
scene.wait(0.4)
scene.play(model.animate.deform(None).duration(1.0))
scene.wait(0.3)

# With amount= a parameter, the field scales with it: animating the
# parameter sways the model and damps it.
scene.segment("sway")
model, _ = frame("model.deform(mode, amount=sway)")
sway = scene.viz.parameter(0.0)
model.deform(mode, amount=sway)
scene.wait(0.3)
for value in (0.9, -0.7, 0.45, -0.25, 0.0):
    scene.play(sway.animate.set(value).duration(0.45))
    if value == -0.7:
        shown.append(scene.cursor - 0.02)
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The mode reached, and the sway at its far side.
    scene.snapshots(snapshots, shown)
else:
    scene.render()
