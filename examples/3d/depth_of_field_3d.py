"""Depth of field of 3D content: a row of solids going away from the camera, blurred by their distance from the focus.

camera.depth_of_field(focus, aperture=..., max_blur=...) blurs each pixel of
the 3D content by its circle of confusion, aperture * |1 - focus / depth|
scene units at most max_blur, where depth is its distance along the view
in units of the camera's distance to its look_at target: focus=1 keeps the
target sharp. animate.focus_to racks the focus to a nearer or farther
depth, and animate.depth_of_field opens the aperture or caps the blur. The
label, pinned to the frame, is the code of the segment.
"""

import os

from gaanim import Material3D, Scene, Zones

MOVE = 1.6
HOLD = 0.6
SEEKS = []

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
danger = scene.colors.danger
chart = scene.colors.chart
rule = scene.colors.rule
# The camera stands about 9 units from its target, the third solid.
HOME = scene.camera.state_3d(eye=(2.6, 3.2, 9.0), target=(0, -0.6, 0), fov_y=0.62)
# The solids from near to far: at depths of about 0.6, 0.8, 1, 1.3 and 1.7.
ROW = [(-1.6, 3.6), (-0.8, 1.8), (0.0, 0.0), (0.8, -2.8), (1.6, -6.4)]


def stage(name, label):
    """Start segment ``name`` with the row of solids, captioned by ``label``."""
    scene.segment(name)
    scene.camera.to(HOME)
    band = scene.layout.zones(Zones.rows(["1fr", 0.5], padding="page_padding"))[1]
    scene.text(label, role="code").hud().place(band, fit="scale_down")
    scene.geometry.plane(10, 18, subdivisions=(20, 36), material=Material3D.matte(rule)).move_to_3d(0, -0.8, -2)
    colors = [accent, success, danger, chart, accent]
    for index, ((x, z), color) in enumerate(zip(ROW, colors)):
        if index % 2:
            scene.geometry.sphere(0.6, material=Material3D.matte(color)).move_to_3d(x, -0.2, z)
        else:
            scene.geometry.cube(1.1, material=Material3D.matte(color)).move_to_3d(x, -0.25, z).rotate_to_3d(0, 0.5, 0)


def move(animation):
    """Play ``animation``, and hold where it ends."""
    SEEKS.append(scene.cursor + MOVE / 2)
    scene.play([animation], duration=MOVE)
    SEEKS.append(scene.cursor + HOLD / 2)
    scene.wait(HOLD)


# Rack the focus from the target to the nearest solid, then to the farthest.
stage("focus", "scene.camera.animate.focus_to(0.6), then focus_to(1.7)")
scene.camera.depth_of_field(focus=1.0, aperture=0.12, max_blur=0.5)
SEEKS.append(scene.cursor + HOLD / 2)
scene.wait(HOLD)
move(scene.camera.animate.focus_to(0.6))
move(scene.camera.animate.focus_to(1.7))

# Open the aperture: what lies away from the focus blurs more.
stage("aperture", "scene.camera.animate.depth_of_field(aperture=0.3)")
scene.camera.depth_of_field(focus=1.0, aperture=0.03, max_blur=0.5)
move(scene.camera.animate.depth_of_field(aperture=0.3))

# Cap the blur: past it, nothing blurs more.
stage("max_blur", "scene.camera.animate.depth_of_field(max_blur=0.02)")
scene.camera.depth_of_field(focus=1.0, aperture=0.3, max_blur=0.5)
move(scene.camera.animate.depth_of_field(max_blur=0.02))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
