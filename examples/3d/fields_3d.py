"""Scalar fields in 3D: the surfaces where a field takes one value, and the field drawn whole as a glowing cloud.

isosurface(field, level, ...) samples field(x, y, z) on a grid over a box
and draws the smooth, lit surface where it equals level, facing out; a
surface the box cuts open is lit on both sides. volume(field, ...) draws
the whole field as a cloud: each value glows in the color a color map gives
it and is as opaque as opacity says, and each pixel gathers the cloud along
its ray up to what is opaque, so a solid inside it shows through. Each
label is the call that builds the drawable above it.
"""

import math
import os

from gaanim import Material3D, Scene, Zones

FADE = 0.8
TURN = 3.0
HOLD = 0.6
SEEKS = []

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
warning = scene.colors.warning
TILT = (0.5, -0.5, 0.0)

# Three balls of charge: the field is the sum of their pulls, and its
# surfaces at higher values hug the balls closer.
CENTERS = [(-0.45, -0.2, 0.0), (0.45, -0.2, 0.0), (0.0, 0.5, 0.15)]


def blobs(x, y, z):
    return sum(0.18 / ((x - cx) ** 2 + (y - cy) ** 2 + (z - cz) ** 2 + 1e-3) for cx, cy, cz in CENTERS)


def gyroid(x, y, z):
    return math.sin(x) * math.cos(y) + math.sin(y) * math.cos(z) + math.sin(z) * math.cos(x)


def cloud(x, y, z):
    """A soft ball, densest at its middle, rippled outward."""
    r = math.sqrt(x * x + y * y + z * z)
    return math.exp(-2.5 * r * r) * (0.75 + 0.25 * math.cos(9.0 * r))


def cells(count):
    """``count`` columns, each an area for a drawable above its caption."""
    columns = scene.layout.zones(Zones.columns(count, gap="48px", padding="page_padding"))
    return [column.split(Zones.rows(["1fr", 0.4])) for column in columns]


def fit(drawable, art, extent):
    """Scale ``drawable``, ``extent`` units across, into ``art``."""
    return drawable.scale_to(0.8 * min(art.width, art.height) / extent).move_to(*art.center).rotate_to_3d(*TILT)


def turn(drawables):
    """Fade ``drawables`` in and turn them a half turn about y."""
    scene.play([drawable.animate.fade_in() for drawable in drawables], duration=FADE)
    SEEKS.append(scene.cursor + TURN / 2)
    scene.play([drawable.animate.rotate_to_3d(TILT[0], TILT[1] + math.pi, 0) for drawable in drawables], duration=TURN)
    SEEKS.append(scene.cursor + HOLD / 2)
    scene.wait(HOLD)


# The surface of three balls of charge, a gyroid the box cuts open, and the
# balls' surface at a higher level, which parts them.
scene.segment("isosurfaces")
(blob_art, blob_caption), (gyroid_art, gyroid_caption), (level_art, level_caption) = cells(3)
box = dict(x_range=(-1.2, 1.2), y_range=(-1.2, 1.2), z_range=(-1.2, 1.2), resolution=40)
blob = fit(scene.geometry.isosurface(blobs, 2.2, material=Material3D.matte(accent), **box), blob_art, 2.4)
scene.text("isosurface(blobs, 2.2)", role="code").place(blob_caption, fit="scale_down")
cell = dict(x_range=(-math.pi, math.pi), y_range=(-math.pi, math.pi), z_range=(-math.pi, math.pi), resolution=40)
sponge = fit(scene.geometry.isosurface(gyroid, 0.0, material=Material3D.matte(success), **cell), gyroid_art, 2 * math.pi)
scene.text("isosurface(gyroid, 0)", role="code").place(gyroid_caption, fit="scale_down")
parted = fit(scene.geometry.isosurface(blobs, 4.0, material=Material3D.matte(warning), **box), level_art, 2.4)
scene.text("isosurface(blobs, 4.0)", role="code").place(level_caption, fit="scale_down")
turn([blob, sponge, parted])

# A cloud lit by every value, and the same cloud showing only its densest
# half, with a solid ring passing through it.
scene.segment("volumes")
(soft_art, soft_caption), (dense_art, dense_caption) = cells(2)
ball = dict(x_range=(-1, 1), y_range=(-1, 1), z_range=(-1, 1), resolution=48)
soft = fit(scene.geometry.volume(cloud, color_map="magma", opacity=(0.0, 0.1, 1.0), density=3.0, **ball), soft_art, 2.0)
scene.text('volume(cloud, color_map="magma", opacity=(0, 0.1, 1))', role="code").place(soft_caption, fit="scale_down")
dense = fit(scene.geometry.volume(cloud, color_map="viridis", opacity=(0.0, 0.0, 1.0), density=12.0, **ball), dense_art, 2.0)
side = 0.8 * min(dense_art.width, dense_art.height)
ring = scene.geometry.torus(side * 0.2, side * 0.035, material=Material3D.matte(accent)).move_to(*dense_art.center).rotate_to_3d(1.2, 0, 0)
scene.text("volume(cloud, opacity=(0, 0, 1)) and a ring", role="code").place(dense_caption, fit="scale_down")
turn([soft, dense, ring])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
