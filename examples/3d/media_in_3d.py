"""Images and Lottie scenes in the 3D world, which meshes in front hide pixel by pixel.

An image or a Lottie scene placed with in_3d() is drawn by the 3D pass in its
own plane, as text and shapes are: it turns and tilts with the scene, and a
mesh passing in front hides it pixel by pixel, where a 2D drawable would be
drawn over the mesh. A Lottie scene keeps playing on the timeline. One kept
facing the camera with billboard() is hidden the same way. Each label is the
call that places the variant.
"""

import os

from gaanim import Material3D, Scene, Zones

BANNER = "assets/banner.png"
BADGE = "assets/badge.png"
TOGGLE = "assets/toggle.lottie"  # animations idle and active
SWEEP = 2.4
HOLD = 0.6
# Turn the flat drawables about y to show they lie in the 3D world.
TURN = (0, 0.5, 0)
SEEKS = []

scene = Scene()
accent = scene.colors.accent
columns = scene.layout.zones(Zones.columns(3, gap="48px", padding="page_padding"))
cells = [column.split(Zones.rows(["1fr", 0.4])) for column in columns]
variants = [
    ("image(BANNER).in_3d()",
     lambda side: scene.media.image(BANNER, width=side * 0.9).in_3d().rotate_to_3d(*TURN)),
    ("lottie(TOGGLE).in_3d()",
     lambda side: scene.media.lottie(TOGGLE, width=side * 0.8, height=side * 0.8).in_3d().rotate_to_3d(*TURN)),
    ("image(BADGE).billboard()",
     lambda side: scene.media.image(BADGE, height=side * 0.6).billboard()),
]
# A ball sweeps in front of each variant, across most of its zone.
sweeps = []
media = []
for (art, caption), (label, build) in zip(cells, variants):
    side = min(art.width, art.height)
    media.append(build(side).move_to(*art.center))
    scene.text(label, role="code").place(caption, fit="scale_down")
    x, y = art.center
    reach = art.width * 0.4
    ball = scene.geometry.sphere(side * 0.16, material=Material3D.matte(accent)).move_to_3d(x - reach, y, side * 0.5)
    sweeps.append(ball.animate.move_to_3d(x + reach, y, side * 0.5))

scene.wait(HOLD)
SEEKS.extend(scene.cursor + SWEEP * share for share in (0.3, 0.5, 0.7))
scene.play([media[1]] + sweeps, duration=SWEEP)
scene.wait(HOLD)
SEEKS.append(scene.cursor)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
