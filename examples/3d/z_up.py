"""Z up: a world with z up and the plan in x/y, and drawings laid on the planes of a model.

``Scene(up="z")`` reads 3D coordinates as engineering models write them:
camera orbits turn about z from the -y front, cylinders stand along z,
planes lie on the x/y floor and the key light shines from above. With
``in_3d(plane=, origin=)`` a 2D drawing lands on a plane of the model, its
scene origin at ``origin``: a slab drawn as a plan lies in x/y at its
height, and a wall drawn in (y, z) stands in y/z. The label is the code that
builds the model.
"""

import math
import os

from gaanim import Material3D, Scene, Zones

scene = Scene(up="z")
accent = scene.colors.accent
success = scene.colors.success
muted = scene.colors.muted
foreground = scene.colors.foreground

zones = scene.layout.zones(Zones.rows(["150px", "1fr"], padding="page_padding"))
scene.text(
    'scene = Scene(up="z")\n'
    'slab.in_3d(plane="xy", origin=(0, 0, 3))\n'
    'wall.in_3d(plane="yz", origin=(-3.2, -2, 0))',
    role="code",
).hud().place(zones[0], anchor="left")

# The model sits below the label.
low = (0.0, -0.8)
front = scene.camera.state_orbit((0, 0, 1.5), math.radians(30), math.radians(25), distance=15, screen_offset=low)
side = scene.camera.state_orbit((0, 0, 1.5), math.radians(120), math.radians(35), distance=15, screen_offset=low)
scene.camera.to(front)

# The floor lies in x/y and the columns stand along z, three units high.
floor = scene.geometry.plane(8, 6, material=Material3D.matte(foreground))
columns = [
    scene.geometry.cylinder(0.25, 3.0, material=Material3D.matte(muted)).move_to_3d(x, y, 1.5)
    for x in (-3, 3)
    for y in (-2, 2)
]
# A plan drawn in x/y lies on top of them; a wall drawn in (y, z) stands on
# the left side.
slab = scene.geometry.rect(6.4, 4.4).fill(accent).in_3d(plane="xy", origin=(0, 0, 3.0))
wall = scene.geometry.polygon([(0, 0), (4, 0), (4, 3), (0, 3)]).fill(success).in_3d(
    plane="yz", origin=(-3.2, -2.0, 0.0)
)
scene.play([floor.animate.create(), *[column.animate.create() for column in columns]], duration=1.0)
scene.play([slab.animate.fade_in(), wall.animate.fade_in()], duration=0.6)
built = scene.cursor
scene.wait(0.4)

# The orbit turns about z, from the front to the side.
scene.play([scene.camera.animate.to(side, path="orbit")], duration=2.0)
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The model built, the orbit halfway and at the side.
    scene.snapshots(snapshots, [built + 0.2, built + 1.4, built + 2.6])
else:
    scene.render()
