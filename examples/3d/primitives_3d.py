"""3D primitives: the lit meshes scene.geometry builds, and their resolution options.

Each primitive is a Primitive3D tilted toward the viewer so its depth shows,
and appears with ``create()``, which grows the mesh from its center. The
default camera is orthographic and looks down the z axis, so the z = 0 plane
lines up with the layout zones. Without a material a primitive is lit matte
white; each label is the ``scene.geometry`` call that builds the variant.
"""

import os

from gaanim import Scene, Zones

DRAW = 1.2
HOLD = 0.8
# Turn every primitive about x to show its top and about y to show a side.
TILT = (0.45, -0.6, 0)

scene = Scene()


def gallery(rows, tilt=TILT):
    """Lay out rows of (label, primitive) pairs, each primitive above its label.

    Every row splits its width evenly between its variants, and every
    primitive turns by ``tilt``.
    """
    bands = scene.layout.zones(Zones.rows(len(rows), gap="48px", padding="page_padding"))
    shapes = []
    for band, variants in zip(bands, rows):
        for cell, (label, shape) in zip(band.split(Zones.columns(len(variants), gap="48px")), variants):
            art, caption = cell.split(Zones.rows(["1fr", 0.5]))
            scene.text(label, role="code").place(caption)
            shapes.append(shape.move_to(*art.center).rotate_to_3d(*tilt))
    scene.play([shape.animate.create() for shape in shapes], duration=DRAW)
    scene.wait(HOLD)


# Cylinders and cones stand along y; a plane lies flat in x and z, facing up,
# and takes subdivisions=(columns, rows) to split it into cells.
scene.segment("solids")
gallery([
    [
        ("cube(2.2)", scene.geometry.cube(2.2)),
        ("sphere(1.5)", scene.geometry.sphere(1.5)),
        ("cylinder(1.1, 2.4)", scene.geometry.cylinder(1.1, 2.4)),
    ],
    [
        ("cone(1.3, 2.3)", scene.geometry.cone(1.3, 2.3)),
        ("plane(4, 3)", scene.geometry.plane(4, 3)),
    ],
])

# segments sets how many sides round primitives have, and a sphere's rings how
# many bands run from pole to pole: few of them make faceted solids.
scene.segment("facets")
gallery([
    [
        ("sphere(1.4, segments=8, rings=4)", scene.geometry.sphere(1.4, segments=8, rings=4)),
        ("sphere(1.4, segments=12, rings=8)", scene.geometry.sphere(1.4, segments=12, rings=8)),
    ],
    [
        ("cylinder(1.1, 2.2, segments=6)", scene.geometry.cylinder(1.1, 2.2, segments=6)),
        ("cone(1.3, 2.4, segments=4)", scene.geometry.cone(1.3, 2.4, segments=4)),
    ],
])

# Without their caps, cylinders and cones are open shells, here seen from
# below. Lit meshes hide the faces turned away from the viewer, so the inside
# of the far wall does not draw.
scene.segment("open")
gallery(
    [[
        ("cylinder(1.6, 3.4, caps=False)", scene.geometry.cylinder(1.6, 3.4, caps=False)),
        ("cone(1.8, 3.4, cap=False)", scene.geometry.cone(1.8, 3.4, cap=False)),
    ]],
    tilt=(-0.5, -0.6, 0),
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Growing and settled, in each of the three segments.
    scene.snapshots(snapshots, [(DRAW + HOLD) * index + t for index in range(3) for t in (0.5, 1.6)])
else:
    scene.render()
