"""Framing 3D content beside a panel: a lens shift that keeps a model aside, labels pinned above its points, and a camera driven by parameters.

A portal frame of two columns and a beam stands on a floor, with the steps
of its analysis on a panel at the right. screen_offset shifts the lens, so
the frame shows in the left zone while the camera orbits and dollies around
it, without turning. billboard(offset=, anchor=) keeps a label a fixed
distance above a 3D point on screen, however far it is. bind_3d takes
coordinates made of parameters, with no auxiliary points. The label at the
bottom is the code of each segment.
"""

import math
import os

from gaanim import Material3D, Scene, Zones, computed

HOLD = 0.6
MOVE = 2.0
SEEKS = []

scene = Scene()
accent = scene.canvas.color("accent")
rule = scene.canvas.color("rule")
success = scene.canvas.color("success")

page = scene.layout.zones(Zones.rows(["1fr", 0.5], padding="page_padding"))
model_zone, panel_zone = page[0].split(Zones.columns(["3fr", "2fr"], gap="32px"))
band = page[1]
# Where the frame's middle shows: the middle of the left zone, from the
# middle of the frame.
ASIDE = model_zone.center

HOME = scene.camera.state_3d(eye=(6, 4, 10), target=(0, 1.5, 0), fov_y=0.7)
# Points on top of the beam: its two nodes and its middle, where a load goes.
POINTS = {"B": (-2, 3.2, 0), "w": (0, 3.2, 0), "C": (2, 3.2, 0)}


def portal():
    """A floor, two columns and a beam between the nodes."""
    scene.geometry.plane(7, 4.5, material=Material3D.matte(rule))
    for x in (-2, 2):
        scene.geometry.cube(1, material=Material3D.matte(accent)).scale_to_3d(0.3, 3, 0.3).move_to_3d(x, 1.5, 0)
    scene.geometry.cube(1, material=Material3D.matte(success)).scale_to_3d(4.3, 0.35, 0.35).move_to_3d(0, 3, 0)


def panel():
    """The steps of the analysis, on the right."""
    rows = panel_zone.split(Zones.rows(3, gap="16px"))
    for step, row in zip(("1  Geometry", "2  Loads", "3  Deformed shape"), rows):
        scene.text(step, size=0.32).hud().place(row, anchor="left", fit="scale_down")


def shot(name, label):
    """Start segment ``name`` at the home pose, with the frame, the panel and the code label."""
    scene.segment(name)
    scene.camera.to(HOME)
    scene.text(label, role="code", text_align="left").hud().place(band, anchor="left")
    portal()
    panel()
    scene.wait(HOLD)


def move(animations):
    """Play camera moves together, then hold where they end."""
    SEEKS.append(scene.cursor + MOVE / 2)
    scene.play(animations, duration=MOVE)
    SEEKS.append(scene.cursor + HOLD / 2)
    scene.wait(HOLD)


# The lens shifts the frame into the left zone; then the camera orbits and
# dollies around it while it stays there.
shot("screen_offset", f"scene.camera.animate.screen_offset({ASIDE[0]:.1f}, {ASIDE[1]:.1f})")
move([scene.camera.animate.screen_offset(*ASIDE)])
move([scene.camera.animate.orbit(-1.1, 0.15, dolly=0.8)])

# Labels a fixed distance above their points on screen, near or far.
shot("billboard", 'text.billboard(offset=(0, 0.2), anchor="bottom")')
scene.camera.screen_offset(*ASIDE)
for name, point in POINTS.items():
    scene.text(name, size=0.4).move_to_3d(*point).billboard(offset=(0, 0.2), anchor="bottom")
move([scene.camera.animate.orbit(1.2, 0.2)])

# The eye's coordinates are computed from a yaw parameter: animating it
# circles the camera, with the lens still shifted.
shot("bind_3d", "scene.camera.bind_3d(eye=(computed(...), 4, computed(...)), screen_offset=...)")
yaw = scene.viz.parameter(0.55)
eye = (
    computed(lambda angle: 11 * math.sin(angle), inputs=[yaw]),
    4.0,
    computed(lambda angle: 11 * math.cos(angle), inputs=[yaw]),
)
scene.camera.bind_3d(eye=eye, target=(0, 1.5, 0), screen_offset=ASIDE)
move([yaw.animate.set(-0.9)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
