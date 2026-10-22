"""Order-independent transparency: translucent solids and surfaces that cross, nest and fold blend in the order they lie.

transparency_3d("order_independent") blends every translucent surface in
its depth order at each pixel, up to layers surfaces deep, where the
default draws each translucent drawable as one layer and blends the
drawables from back to front. Panels that cross show the nearer one over
the other on each side of where they cross, a ball passing through a panel
shows where it pierces it, nested shells show each one inside the last,
and a knot or a twisted strip shows its far side through its near one.
Each label is the call that builds the drawables beside it.
"""

import math
import os

from gaanim import Material3D, Scene, Zones

FADE = 0.6
MOVE = 2.4
HOLD = 0.6
SEEKS = []

scene = Scene()
scene.geometry.transparency_3d("order_independent", layers=6)
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
warning = scene.canvas.color("warning")


def cells(count):
    """``count`` columns, each an area for the drawables above a caption."""
    columns = scene.layout.zones(Zones.columns(count, gap="48px", padding="page_padding"))
    return [column.split(Zones.rows(["1fr", 0.4])) for column in columns]


def panel(side, color):
    """A square ``side`` units wide facing the camera, lit on its front."""
    half = side / 2
    return scene.geometry.mesh(
        [(-half, -half, 0), (half, -half, 0), (half, half, 0), (-half, half, 0)],
        [(0, 1, 2), (0, 2, 3)],
        material=Material3D.matte(color),
    )


def knot(size, segments=240):
    """A trefoil knot about the origin, ``size`` units across."""
    scale = size / 6
    return [
        (
            scale * (math.sin(t) + 2 * math.sin(2 * t)),
            scale * (math.cos(t) - 2 * math.cos(2 * t)),
            scale * -math.sin(3 * t),
        )
        for t in (math.tau * step / segments for step in range(segments))
    ]


def strip(radius, width, segments=96):
    """A Möbius strip about the origin, half ``accent`` and half ``success``
    along its length; each face has vertices of its own, so that it is lit
    on its own side."""
    side = []
    for step in range(segments + 1):
        u = math.tau * step / segments
        for v in (-width / 2, width / 2):
            r = radius + v * math.cos(u / 2)
            side.append((r * math.cos(u), v * math.sin(u / 2), r * math.sin(u)))
    back = len(side)
    triangles = []
    for step in range(segments):
        a, b, c, d = 2 * step, 2 * step + 1, 2 * step + 2, 2 * step + 3
        triangles += [(a, c, b), (b, c, d), (back + a, back + b, back + c), (back + b, back + d, back + c)]
    colors = [accent if step < segments / 2 else success for step in range(segments + 1) for _ in range(2)]
    return scene.geometry.mesh(side + side, triangles, colors=colors + colors)


def show(drawables, label, caption):
    """Fade ``drawables`` in, captioned by the call that builds them."""
    scene.text(label, role="code").place(caption, fit="scale_down")
    return [drawable.opacity(0.5) for drawable in drawables]


# Two panels that cross, turning together, and a ball that passes through a
# panel from behind it to in front of it.
scene.segment("crossing")
(cross_art, cross_caption), (pierce_art, pierce_caption) = cells(2)
side = min(cross_art.width, cross_art.height) * 0.7
crossed = show(
    [
        panel(side, color).move_to(*cross_art.center).rotate_to_3d(0, angle, 0)
        for color, angle in ((accent, -0.9), (success, 0.3))
    ],
    "panel(side).opacity(0.5) twice, crossing",
    cross_caption,
)
side = min(pierce_art.width, pierce_art.height) * 0.7
wall = panel(side, accent).move_to(*pierce_art.center)
ball = scene.geometry.sphere(side * 0.3, material=Material3D.matte(success))
ball.move_to_3d(*pierce_art.center, -side * 0.4)
pierced = show([wall, ball], "sphere(r).opacity(0.5) through a panel", pierce_caption)
scene.play([drawable.animate.fade_in() for drawable in crossed + pierced], duration=FADE)
# The ball half through the panel, and through it.
SEEKS += [scene.cursor + MOVE * 0.4, scene.cursor + MOVE * 0.8]
scene.play(
    [crossed[0].animate.rotate_to_3d(0, -0.3, 0), crossed[1].animate.rotate_to_3d(0, 0.9, 0)]
    + [ball.animate.move_to_3d(*pierce_art.center, side * 0.4)],
    duration=MOVE,
)
scene.wait(HOLD)

# Three shells, each inside the last, and a ring inside a box, turning.
scene.segment("nested")
(shell_art, shell_caption), (box_art, box_caption) = cells(2)
radius = min(shell_art.width, shell_art.height) * 0.4
shells = show(
    [
        scene.geometry.sphere(radius * share, material=Material3D.matte(color)).move_to(*shell_art.center)
        for share, color in ((1.0, accent), (0.65, success), (0.3, warning))
    ],
    "sphere(r * k).opacity(0.5) for k in (1, 0.65, 0.3)",
    shell_caption,
)
side = min(box_art.width, box_art.height) * 0.62
box = scene.geometry.cube(side, material=Material3D.matte(accent)).move_to(*box_art.center)
ring = scene.geometry.torus(side * 0.3, side * 0.09, material=Material3D.matte(success)).move_to(*box_art.center)
held = show([box, ring], "torus(...) in cube(side), both opacity(0.5)", box_caption)
for drawable in held:
    drawable.rotate_to_3d(0.5, -0.6, 0)
scene.play([drawable.animate.fade_in() for drawable in shells + held], duration=FADE)
SEEKS += [scene.cursor + MOVE * 0.5]
scene.play([box.animate.rotate_to_3d(0.5, 0.6, 0), ring.animate.rotate_to_3d(1.6, 0.6, 0)], duration=MOVE)
SEEKS += [scene.cursor]
scene.wait(HOLD)

# A knot, whose strands pass in front of and behind each other, and a
# twisted strip, whose two faces fold over each other.
scene.segment("folds")
(knot_art, knot_caption), (strip_art, strip_caption) = cells(2)
size = min(knot_art.width, knot_art.height) * 0.85
tube = scene.geometry.tube(knot(size), size * 0.06, closed=True, material=Material3D.matte(warning))
folded = show([tube.move_to(*knot_art.center).rotate_to_3d(0.9, 0, 0)], "tube(knot, r, closed=True).opacity(0.5)", knot_caption)
size = min(strip_art.width, strip_art.height)
twisted = show([strip(size * 0.3, size * 0.22).move_to(*strip_art.center).rotate_to_3d(0.6, 0, 0)], "mesh(möbius).opacity(0.5)", strip_caption)
scene.play([drawable.animate.fade_in() for drawable in folded + twisted], duration=FADE)
SEEKS += [scene.cursor + MOVE * 0.4]
scene.play([drawable.animate.rotate_to_3d(0.9 - 0.6, 1.2, 0) for drawable in folded + twisted], duration=MOVE)
SEEKS += [scene.cursor]
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
