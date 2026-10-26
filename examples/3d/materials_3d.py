"""3D materials: Material3D presets, surface finish, emission and material animations.

Presets and animations show each material on a cube, whose flat faces each
take one shade, and on a sphere, whose smooth shading shows the highlight;
the channel tables use spheres alone. Segments cover the presets, the
roughness and metallic channels of the constructor, emission, and animating
a primitive from one material to another.
"""

import os

from gaanim import Material3D, Scene, Zones

DRAW = 1.0
DURATION = 1.6
HOLD = 0.8
TILT = (0.45, -0.6, 0)

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
danger = scene.colors.danger
rule = scene.colors.rule

# One studio rig lights every lit primitive of the scene: sky and ground
# light, a soft key light from the upper left and a faint rim. intensity
# scales it, and "none" leaves only what materials emit.
scene.geometry.lighting_3d("studio", intensity=1.0)


def swatch(zone, material):
    """A tilted cube and a sphere side by side in ``zone``, both of ``material``."""
    left, right = zone.split(Zones.columns(2, gap="24px"))
    side = min(left.width, left.height)
    cube = scene.geometry.cube(side * 0.6, material=material).move_to(*left.center).rotate_to_3d(*TILT)
    sphere = scene.geometry.sphere(side * 0.45, material=material).move_to(*right.center)
    return [cube, sphere]


def reveal(shapes):
    """Grow ``shapes`` together, then hold them."""
    scene.play([shape.animate.create() for shape in shapes], duration=DRAW)
    scene.wait(HOLD)


def gallery(variants):
    """Two columns of (label, material) pairs, each swatch above its label.

    The second column is wider, for the longer labels.
    """
    grid = scene.layout.zones(Zones.grid(rows=2, columns=["1fr", "1.4fr"], gap="48px", padding="page_padding"))
    shapes = []
    for zone, (label, material) in zip(grid, variants):
        art, caption = zone.split(Zones.rows(["1fr", 0.5]))
        scene.text(label, role="code").place(caption)
        shapes += swatch(art, material)
    reveal(shapes)


def matrix(rows, columns, build, label_track):
    """One sphere per (row, column) pair, labelled by its row on the left and its column above.

    ``rows`` and ``columns`` are (label, value) pairs; ``build(row, column)``
    makes the cell's material. ``label_track`` is the width of the label column.
    """
    grid = scene.layout.zones(
        Zones.grid(
            rows=[0.5] + ["1fr"] * len(rows),
            columns=[label_track] + ["1fr"] * len(columns),
            row_gap="24px",
            column_gap="48px",
            padding="page_padding",
        )
    )
    width = len(columns) + 1
    for index, (label, _) in enumerate(columns, start=1):
        scene.text(label, role="code").place(grid[index])
    shapes = []
    for row_index, (label, row) in enumerate(rows, start=1):
        scene.text(label, role="code").place(grid[row_index * width], anchor="right")
        for index, (_, column) in enumerate(columns, start=1):
            cell = grid[row_index * width + index]
            radius = min(cell.width, cell.height) * 0.45
            shapes.append(scene.geometry.sphere(radius, material=build(row, column)).move_to(*cell.center))
    reveal(shapes)


# Presets: matte, polished metal, and a material that glows in its own color.
scene.segment("presets")
gallery([
    ("Material3D.matte(accent)", Material3D.matte(accent)),
    ("Material3D.metal(accent)", Material3D.metal(accent)),
    ("Material3D.emissive(accent)", Material3D.emissive(accent)),
    ("Material3D.emissive(accent, strength=2)", Material3D.emissive(accent, strength=2)),
])

# The constructor sets each channel: Material3D(accent, roughness=0.1, metallic=0.5).
# Low roughness gives a sharp highlight; metals reflect the sky instead of
# scattering light.
scene.segment("finish")
matrix(
    [("roughness=0.1", 0.1), ("roughness=0.5", 0.5), ("roughness=0.9", 0.9)],
    [("metallic=0", 0.0), ("metallic=0.5", 0.5), ("metallic=1", 1.0)],
    lambda roughness, metallic: Material3D(accent, roughness=roughness, metallic=metallic),
    "2fr",
)

# Emission adds its color on top of the lit surface:
# Material3D(rule, emissive=accent, emissive_strength=0.5).
scene.segment("emission")
matrix(
    [("emissive_strength=0", 0.0), ("emissive_strength=0.5", 0.5), ("emissive_strength=1", 1.0)],
    [("emissive=accent", accent), ("emissive=success", success)],
    lambda strength, color: Material3D(rule, emissive=color, emissive_strength=strength),
    "1fr",
)

# animate.material() blends every channel at once and animate.fill() only the
# base color; .material(...) without animate swaps it at the cursor.
scene.segment("animate")
CHANGES = [
    ("animate.material(Material3D.metal(accent))", lambda shape: shape.animate.material(Material3D.metal(accent))),
    (
        "animate.material(Material3D.emissive(accent))",
        lambda shape: shape.animate.material(Material3D.emissive(accent)),
    ),
    ("animate.fill(success)", lambda shape: shape.animate.fill(success)),
    ("animate.fill(danger)", lambda shape: shape.animate.fill(danger)),
]
grid = scene.layout.zones(
    Zones.grid(rows=len(CHANGES), columns=["2fr", "1fr"], gap="24px", column_gap="48px", padding="page_padding")
)
moves = []
for index, (label, change) in enumerate(CHANGES):
    scene.text(label, role="code").place(grid[2 * index], anchor="right")
    moves += [change(shape) for shape in swatch(grid[2 * index + 1], Material3D.matte(accent))]
scene.wait(HOLD)
scene.play(moves, duration=DURATION)
scene.wait(HOLD)

# Growing and settled in the first segment, settled in the next two, and
# before, during and after the change in the last.
segment = DRAW + HOLD
SEEKS = [0.6, 1.4, segment + 1.4, 2 * segment + 1.4] + [3 * segment + t for t in (0.4, 1.6, 3.0)]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
