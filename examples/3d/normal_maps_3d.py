"""glTF normal maps: a brick panel whose normal map gives its flat face the relief of its bricks, beside the same panel without one.

The file holds the panel in two scenes, both with the same color texture,
with and without the normal map; each label is the call that imports the
panel above it. A normal map tilts
the shading of each pixel as its texture says, so the bricks catch a light
that moves and a panel that turns, though the face stays flat.
"""

import os

from gaanim import Scene, Zones

FADE = 0.6
HOLD = 0.8
SWEEP = 3.0
TURN = 2.4

scene = Scene()

# The panel in its file: a square 2 units on a side, centered on its
# origin and facing the camera.
PANEL_SIZE = 2.0
SCENES = ["Plain", "NormalMap"]


def panels(share=0.9):
    """The two panels side by side, each above its label, and the zone they share."""
    art, captions = scene.layout.zones(Zones.rows(["1fr", 0.45], gap="12px", padding="page_padding"))
    columns = Zones.columns(len(SCENES), gap="48px")
    shown = []
    for zone, caption, name in zip(art.split(columns), captions.split(columns), SCENES):
        scale = share * min(zone.width, zone.height) / PANEL_SIZE
        shown.append(scene.media.gltf("Bricks.glb", scene=name).scale_to(scale).move_to(*zone.center))
        scene.text(f'gltf("Bricks.glb", scene="{name}")', role="code").place(caption, fit="scale_down")
    return art, shown


# A point light passing close in front of the panels, along their top,
# lights the upper and left sides of each brick, then its upper and right
# sides; without the map, the face only brightens and dims as a whole.
scene.segment("light")
art, shown = panels()
left_x, top_y = art.point("top_left")
right_x, _ = art.point("top_right")
DEPTH = 0.25 * art.height
lamp = scene.geometry.light_3d("point", intensity=1.6).move_to_3d(left_x, top_y, DEPTH)
scene.play([panel.animate.fade_in() for panel in shown], duration=FADE)
scene.play(lamp.animate.move_to_3d(right_x, top_y, DEPTH), duration=SWEEP)
scene.wait(HOLD)

# Under the studio light, the bricks of the mapped panel shade as the
# panel turns.
scene.segment("turn")
art, shown = panels(share=0.8)
for panel in shown:
    panel.rotate_to_3d(0.0, -0.7, 0.0)
scene.play([panel.animate.fade_in() for panel in shown], duration=FADE)
scene.play([panel.animate.rotate_to_3d(0.0, 0.7, 0.0) for panel in shown], duration=TURN)
scene.wait(HOLD)

# The light at the left, the middle and the right; the panels turned one
# way, facing the camera and turned the other way.
turn = FADE + SWEEP + HOLD
SEEKS = [
    FADE + 0.1,
    FADE + SWEEP / 2,
    FADE + SWEEP,
    turn + FADE,
    turn + FADE + TURN / 2,
    turn + FADE + TURN,
]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
