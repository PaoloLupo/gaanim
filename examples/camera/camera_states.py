"""Camera states: reusable poses built with state_2d or captured, saved by name and restored.

A ``CameraState`` is a pose (center, zoom and rotation) the camera can cut or
animate to as many times as needed. The outlines on the map show the region
each state frames; a strip pinned to the screen names the call of each shot.
"""

import math
import os

from gaanim import Scene, Zones

MOVE = 1.2
HOLD = 0.5
SPACING = 3.0
SEEKS = []

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")
muted = scene.canvas.color("muted")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")


def world(columns=range(-4, 5), rows=range(-2, 3)):
    """Tiles every ``SPACING`` units, each labelled with the coordinates of its center."""
    for column in columns:
        for row in rows:
            x, y = column * SPACING, row * SPACING
            scene.geometry.rounded_rect(2.8, 2.8, 0.16).no_fill().stroke(rule, 0.02).move_to(x, y)
            scene.text(f"({x:g}, {y:g})", role="code").fill(muted).scale_to(1.3).move_to(x, y - 0.95)


def outline(name, center, zoom, rotation=0.0):
    """Outline the region a state with this ``center``, ``zoom`` and ``rotation`` frames, named below it."""
    (x, y), width, height = center, 16 / zoom, 9 / zoom
    frame = scene.geometry.rect(width, height).no_fill().stroke(accent, 0.05).move_to(x, y).rotate_to(rotation)
    # The name sits just outside the bottom edge and turns with it, so it
    # leaves the view when the camera reaches the state.
    below = height / 2 + 0.35
    label = scene.text(name, role="code").fill(accent).scale_to(1.3).rotate_to(rotation)
    label.move_to(x + below * math.sin(rotation), y - below * math.cos(rotation))
    return [frame.animate.create(), label.animate.fade_in()]


class Captions:
    """A strip pinned to the bottom of the screen whose caption names each shot."""

    def __init__(self):
        strip = scene.layout.zones(Zones.rows(["1fr", "80px"]))[1]
        scene.geometry.rect(strip.width, strip.height).fill(panel).no_stroke().hud().move_to(*strip.center)
        self.zone = strip.inset((0, "page_padding"))
        self.current = None

    def shot(self, label, *moves, duration=MOVE, hold=HOLD, snap=True):
        """Show ``label`` and play ``moves`` under it, each lasting ``duration``.

        Records the end of the shot for the snapshots unless ``snap`` is off.
        """
        caption = scene.text(label, role="code").hud().place(self.zone, anchor="left").hidden()
        swap = [caption.animate.fade_in().duration(0.2)]
        if self.current is not None:
            swap.append(self.current.animate.fade_out().duration(0.15))
        self.current = caption
        scene.play(swap + [move.duration(duration) for move in moves])
        scene.wait(hold)
        if snap:
            SEEKS.append(scene.cursor - 0.2)


# state_2d builds a pose without moving the camera, and capture() records the
# pose the camera has at the cursor. animate.to(state) moves there; to(state)
# cuts there. A state can be reused any number of times.
scene.segment("state_2d")
world()
scene.geometry.circle(0.5).fill(success).move_to(-3, 3.0)
scene.geometry.square(1.0).fill(danger).move_to(3, 0.3)
captions = Captions()
overview = scene.camera.capture()
captions.shot("overview = scene.camera.capture()", snap=False)
detail = scene.camera.state_2d(center=(-3, 2.2), zoom=3)
captions.shot("detail = scene.camera.state_2d(center=(-3, 2.2), zoom=3)", *outline("detail", (-3, 2.2), 3))
tilted = scene.camera.state_2d(center=(3, -0.5), zoom=2.5, rotation=0.3)
captions.shot(
    "tilted = state_2d(center=(3, -0.5), zoom=2.5, rotation=0.3)",
    *outline("tilted", (3, -0.5), 2.5, 0.3),
)
captions.shot("scene.camera.animate.to(detail)", scene.camera.animate.to(detail))
captions.shot("scene.camera.animate.to(tilted)", scene.camera.animate.to(tilted))
captions.shot("scene.camera.animate.to(overview)", scene.camera.animate.to(overview))
scene.camera.to(detail)
captions.shot("scene.camera.to(detail)")
captions.shot("scene.camera.animate.to(overview)", scene.camera.animate.to(overview))

# save(name) captures the pose under a name and returns its state;
# restore(name) cuts back to it and animate.restore(name) moves back to it.
scene.segment("save_restore")
scene.camera.reset()
world()
star = scene.geometry.star(5, 0.8, 0.36).fill(accent).move_to(-6, 0.35)
captions = Captions()
scene.camera.save("start")
captions.shot('scene.camera.save("start")', snap=False)
captions.shot("scene.camera.animate.frame_to(star, margin=1)", scene.camera.animate.frame_to(star, margin=1))
captions.shot("scene.camera.animate.rotate_to(-0.3)", scene.camera.animate.rotate_to(-0.3))
scene.camera.save("star")
captions.shot('scene.camera.save("star")', snap=False)
captions.shot('scene.camera.animate.restore("start")', scene.camera.animate.restore("start"))
scene.camera.restore("star")
captions.shot('scene.camera.restore("star")')
captions.shot('scene.camera.animate.restore("start")', scene.camera.animate.restore("start"))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
