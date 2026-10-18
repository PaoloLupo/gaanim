"""3D plots: surfaces, parametric curves and vector fields drawn in a cartesian_3d space.

Each row draws one plot in its own space, seen by a distant perspective
camera with z pointing up, and is labelled with the call on the space that
builds it; the functions it plots are defined above each segment, and
curves are stroked with the accent.
Segments cover surfaces z = f(x, y), plots that follow animated inputs,
parametric curves, the arrows and streamlines of a 3D vector field, and
surfaces lit by a material.
"""

import math
import os

from gaanim import Axis, Material3D, Scene, Zones

DRAW = 1.4
HOLD = 0.8

X = Axis.linear(-3, 3).ticks(1)
Y = Axis.linear(-3, 3).ticks(1)
Z = Axis.linear(-2, 2).ticks(1)

scene = Scene()
accent = scene.canvas.color("accent")

# The camera looks at the origin from AZIMUTH radians around z and ELEVATION
# radians above the floor, with z up. From DISTANCE away, a field of view as
# tall as the frame draws the plane through the origin that faces the camera
# at the layout's scale, so on_screen() finds the 3D point under a zone.
AZIMUTH, ELEVATION, DISTANCE = 0.65, 0.45, 40
RIGHT = (math.cos(AZIMUTH), math.sin(AZIMUTH), 0)
ABOVE = (-math.sin(AZIMUTH) * math.sin(ELEVATION), math.cos(AZIMUTH) * math.sin(ELEVATION), math.cos(ELEVATION))
TOWARD_EYE = (math.sin(AZIMUTH) * math.cos(ELEVATION), -math.cos(AZIMUTH) * math.cos(ELEVATION), math.sin(ELEVATION))
frame = scene.layout.zones(Zones.rows(1))[0]
scene.camera.perspective(fov_y=2 * math.atan(frame.height / 2 / DISTANCE))
scene.camera.look_at(eye=tuple(DISTANCE * c for c in TOWARD_EYE), target=(0, 0, 0), up=(0, 0, 1))


def on_screen(zone):
    """The 3D point the camera draws at the center of ``zone``."""
    x, y = zone.center
    return tuple(x * right + y * above for right, above in zip(RIGHT, ABOVE))


def pair(variants):
    """Draw two (label, plot) variants in rows, the code to the left of a fresh space.

    ``plot(space)`` draws in the space and returns what to reveal.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(variants), columns=["5fr", "2fr"], gap="24px", column_gap="48px", padding="page_padding")
    )
    reveals = []
    for index, (label, plot) in enumerate(variants):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        space = scene.viz.cartesian_3d(X, Y, Z, size=(2.8, 2.8, 1.9), xz_grid=False)
        space.move_to_3d(*on_screen(grid[2 * index + 1]))
        reveals += [space.animate.create(), plot(space).animate.create()]
    scene.play(reveals, duration=DRAW)


# A surface samples z = f(x, y) on a grid of resolution points and colors
# it by height; fewer points give a coarser mesh.
def wave(x, y):
    return math.sin(x) * math.cos(y)


def bowl(x, y):
    return (x * x + y * y) / 6 - 1.5


scene.segment("surfaces")
pair([
    ("space.surface(wave)", lambda space: space.surface(wave)),
    ("space.surface(bowl, resolution=(7, 7))", lambda space: space.surface(bowl, resolution=(7, 7))),
])
scene.wait(HOLD)


# Plots take extra arguments from inputs, reactive values such as a
# parameter, and redraw whenever they change: here phase and turns animate.
def travel(x, y, phase):
    return math.sin(x + phase) * 0.8 * math.cos(y / 2)


def coil(t, turns):
    angle = turns * math.tau * t
    return 2.4 * math.cos(angle), 2.4 * math.sin(angle), 3.6 * t - 1.8


scene.segment("inputs")
phase = scene.viz.parameter(0.0)
turns = scene.viz.parameter(0.5)
pair([
    ("space.surface(travel, inputs=[phase])", lambda space: space.surface(travel, inputs=[phase])),
    (
        "space.parametric(coil, (0, 1), inputs=[turns])",
        lambda space: space.parametric(coil, (0, 1), inputs=[turns]).stroke(accent, 0.04),
    ),
])
scene.play([phase.animate.set(math.tau), turns.animate.set(3)], duration=2.4)
scene.wait(HOLD)


# A parametric curve maps t over its domain to (x, y, z); samples sets how
# many points trace it.
def helix(t):
    return 2.2 * math.cos(t), 2.2 * math.sin(t), t / 4 - 1.6


scene.segment("curves")
pair([
    ("space.parametric(helix, (0, 12.6))", lambda space: space.parametric(helix, (0, 12.6)).stroke(accent, 0.04)),
    (
        "space.parametric(helix, (0, 12.6), samples=16)",
        lambda space: space.parametric(helix, (0, 12.6), samples=16).stroke(accent, 0.04),
    ),
])
scene.wait(HOLD)


# A field maps (x, y, z) to a vector: field = space.field(swirl). Its arrows
# sample it on a 3D grid and its streamlines follow it from seed points,
# both colored by magnitude.
def swirl(x, y, z):
    return -y, x, 0.6


scene.segment("fields")
pair([
    ("field.arrows(resolution=(6, 6, 4))", lambda space: space.field(swirl).arrows(resolution=(6, 6, 4))),
    ("field.streamlines(seeds=(3, 3, 2))", lambda space: space.field(swirl).streamlines(seeds=(3, 3, 2))),
])
scene.wait(HOLD)


# A material lights a surface on both sides, keeping its colors, so its
# relief shows in light and shadow: matte = Material3D.matte() and
# metal = Material3D.metal(), under a low light from the camera's left.
matte = Material3D.matte()
metal = Material3D.metal()

scene.segment("lighting")
scene.geometry.light_3d("directional").move_to_3d(-9, -3, 5)
pair([
    ("space.surface(wave, material=matte)", lambda space: space.surface(wave, material=matte)),
    ("space.surface(bowl, material=metal)", lambda space: space.surface(bowl, material=metal)),
])
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawing and drawn plots; the inputs segment also as its inputs animate.
    scene.snapshots(snapshots, [1.0, 2.0, 3.0, 4.6, 5.6, 6.6, 7.6, 8.7, 9.6, 10.9, 11.8, 13.1])
else:
    scene.render()
