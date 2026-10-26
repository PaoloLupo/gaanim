"""Drop shadows and blurs on 3D content, as on 2D drawables.

shadow() casts a drop shadow from what the camera sees of a primitive, or of a
drawable placed with in_3d(), moved by its offset and blurred in scene units;
blur() blurs the drawable over what lies behind it, and animate.blur()
sharpens or blurs it on the timeline. A solid in front hides both, as a 2D
drawable drawn after them covers them. Each label is the call that gives the
variant its effect.
"""

import os

from gaanim import Material3D, Scene, Zones

DRAW = 1.0
FOCUS = 1.6
HOLD = 0.6
# Turn the solids about x to show their top and about y to show a side.
TILT = (0.45, -0.6, 0)
SEEKS = []

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
panel = scene.colors.panel
shade = scene.colors.shadow


def cells(count):
    """``count`` columns, each an area for the drawable above a caption."""
    columns = scene.layout.zones(Zones.columns(count, gap="48px", padding="page_padding"))
    return [column.split(Zones.rows(["1fr", 0.4])) for column in columns]


# A shadow moved down and right, on a solid and on a text in the 3D world,
# over a panel behind them that it falls on.
scene.segment("shadow")
shadowed = []
for (art, caption), (label, build) in zip(cells(2), [
    ("ball.shadow(shade, 0.2, -0.2, 0.1)",
     lambda side: scene.geometry.sphere(side * 0.3, material=Material3D.matte(accent))
     .shadow(shade, 0.2, -0.2, 0.1)),
    ("word.shadow(shade, 0.1, -0.1, 0.05)",
     lambda side: scene.text("Hola", size=side * 0.3).in_3d().rotate_to_3d(-0.5, 0.4, 0)
     .shadow(shade, 0.1, -0.1, 0.05)),
]):
    side = min(art.width, art.height)
    scene.geometry.rect(art.width, art.height).fill(panel).no_stroke().in_3d().move_to(*art.center)
    shadowed.append(build(side).move_to_3d(*art.center, side * 0.4))
    scene.text(label, role="code").place(caption, fit="scale_down")
SEEKS.append(scene.cursor + DRAW / 2)
scene.play([drawable.animate.fade_in() for drawable in shadowed], duration=DRAW)
SEEKS.append(scene.cursor + HOLD / 2)
scene.wait(HOLD)

# A blur that the timeline sharpens, and one behind a sharp solid, which
# hides the blur where it stands in front.
scene.segment("blur")
(focus_art, focus_caption), (behind_art, behind_caption) = cells(2)
side = min(focus_art.width, focus_art.height)
cube = scene.geometry.cube(side * 0.45, material=Material3D.matte(accent)).move_to(*focus_art.center).rotate_to_3d(*TILT)
cube.blur(0.3)
scene.text("cube.animate.blur(0)", role="code").place(focus_caption, fit="scale_down")
side = min(behind_art.width, behind_art.height)
ring = scene.geometry.torus(side * 0.3, side * 0.1, material=Material3D.matte(success)).move_to(*behind_art.center).rotate_to_3d(1.1, 0, 0)
ring.blur(0.2)
scene.geometry.cube(side * 0.25, material=Material3D.matte(accent)).move_to_3d(behind_art.center[0], behind_art.center[1], side * 0.4).rotate_to_3d(*TILT)
scene.text("ring.blur(0.2)", role="code").place(behind_caption, fit="scale_down")
scene.wait(HOLD / 2)
SEEKS.append(scene.cursor)
scene.play([cube.animate.blur(0)], duration=FOCUS)
SEEKS.append(scene.cursor - FOCUS / 2)
SEEKS.append(scene.cursor + HOLD / 2)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
