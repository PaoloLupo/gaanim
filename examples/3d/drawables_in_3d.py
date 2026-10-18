"""2D drawables placed in the 3D world with in_3d(), keeping their animations and colors.

``drawable.in_3d()`` hands a 2D drawable to the 3D pass, which draws it from
its outlines in its own plane: ``move_to_3d`` and ``rotate_to_3d`` place and
turn it like a 3D primitive, perspective tilts it, and meshes in front of it
hide it pixel by pixel, while its edges stay sharp at any size. It keeps what
it is in 2D: text, equations and shapes, fills and strokes, and animations
such as ``write`` or a change of color. ``in_3d(material)`` lights it, and
shadows fall on it, as on a surface. ``billboard()`` instead keeps a label
facing the camera, above the 3D content, and ``hud()`` pins it to the frame.
"""

import math
import os

from gaanim import Material3D, Scene, Zones

DRAW = 1.2
TURN = 1.4
HOLD = 0.8
# The poses the drawables turn to in the first segment: lying back, then
# turned aside.
TURNS = [(-1.0, 0, 0), (0.3, 1.0, 0)]

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
rule = scene.canvas.color("rule")
foreground = scene.canvas.color("foreground")
# Times worth a snapshot: while drawables are written, and once each move settles.
moments = []


# The default camera is orthographic and looks down the z axis, so the z = 0
# plane lines up with the layout zones. Each label is the source drawable,
# which is placed in its zone, then put in the 3D world with .in_3d(),
# written, and turned: it foreshortens like a card, its stroke and fill
# turning with it.
scene.segment("turned")
sources = [
    ('scene.text("Hola")', lambda: scene.text("Hola")),
    ('scene.text.equation("e^(i pi)")', lambda: scene.text.equation("e^(i pi)")),
    ("scene.geometry.star(5, 1, 0.4)", lambda: scene.geometry.star(5, 1, 0.4)),
    ("scene.geometry.circle(1).stroke(success, 0.1)", lambda: scene.geometry.circle(1).stroke(success, 0.1)),
]
table = scene.layout.zones(
    Zones.grid(rows=len(sources), columns=["2fr", "1fr"], gap="24px", column_gap="48px", padding="page_padding")
)
placed = []
for index, (label, build) in enumerate(sources):
    scene.text(label, role="code").place(table[2 * index], anchor="right")
    art = table[2 * index + 1]
    placed.append(build().place(art.inset(art.height * 0.2), fit="contain").in_3d())
moments.append(scene.cursor + DRAW / 2)
scene.play([drawable.animate.write() for drawable in placed], duration=DRAW)
moments.append(scene.cursor)
for turn in TURNS:
    scene.play([drawable.animate.rotate_to_3d(*turn) for drawable in placed], duration=TURN)
    moments.append(scene.cursor)
scene.wait(HOLD)

# Under a perspective camera, a sign stands through a sphere, which hides
# part of it pixel by pixel; a line lies on the floor, lit and in the
# sphere's shadow; and a billboard label stays facing the camera as it
# orbits, drawn above the 3D content. The text of each is the code that
# places it. The sign then changes color, as it would in 2D.
scene.segment("world")
scene.camera.to(scene.camera.state_3d(eye=(0, 2.2, 9), target=(0, -0.2, 0), fov_y=0.7))
# From above and behind, so the sphere's shadow falls toward the camera.
scene.geometry.light_3d("directional").move_to_3d(0, 6, -3)
scene.geometry.plane(12, 8, material=Material3D.matte(rule)).move_to_3d(0, -1.5, 0)
scene.geometry.sphere(1.0, material=Material3D.matte(accent)).move_to_3d(0.8, -0.3, 0)
sign = scene.text("text.in_3d()", role="code", size=0.7).in_3d().move_to_3d(0, 0.4, 0)
floor = scene.text("text.in_3d(material)", role="code", size=0.45).in_3d(Material3D.matte(foreground))
floor.rotate_to_3d(-math.pi / 2, 0, 0).move_to_3d(0.6, -1.49, 1.0)
label = scene.text("text.billboard()", role="code", size=0.35).move_to_3d(2.6, -0.2, -1.2).billboard()
moments.append(scene.cursor + DRAW / 2)
scene.play([sign.animate.write(), floor.animate.write(), label.animate.write()], duration=DRAW)
moments.append(scene.cursor)
scene.play([scene.camera.animate.orbit(delta_yaw=0.7, delta_pitch=0.15)], duration=TURN)
moments.append(scene.cursor)
scene.play([sign.animate.fill(success)], duration=DRAW / 2)
moments.append(scene.cursor)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, moments)
else:
    scene.render()
