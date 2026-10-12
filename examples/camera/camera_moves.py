"""Camera moves: panning, zooming, rotating, framing objects and following them.

The camera looks at a map of tiles labelled with their coordinates, so every
pan lands on a tile you can read. A strip pinned to the screen with ``hud()``
names the call that moves the camera; each segment covers one family of moves.
"""

import os

from gaanim import Easing, Scene, Zones

MOVE = 1.0
LEG = 1.6
HOLD = 0.4
SPACING = 3.0
SEEKS = []

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")
muted = scene.canvas.color("muted")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")


def world(columns=range(-6, 7), rows=range(-4, 5)):
    """Tiles every ``SPACING`` units, each labelled with the coordinates of its center."""
    for column in columns:
        for row in rows:
            x, y = column * SPACING, row * SPACING
            scene.geometry.rounded_rect(2.8, 2.8, 0.16).no_fill().stroke(rule, 0.02).move_to(x, y)
            scene.text(f"({x:g}, {y:g})", role="code").fill(muted).scale_to(1.3).move_to(x, y - 0.95)


class Captions:
    """A strip pinned to the bottom of the screen whose caption names each shot."""

    def __init__(self):
        strip = scene.layout.zones(Zones.rows(["1fr", "80px"]))[1]
        scene.geometry.rect(strip.width, strip.height).fill(panel).no_stroke().hud().move_to(*strip.center)
        self.zone = strip.inset((0, "page_padding"))
        self.current = None

    def shot(self, label, *moves, duration=MOVE, hold=HOLD, midway=False, end=True, also=()):
        """Show ``label`` and play ``moves``, each lasting ``duration``, with ``also`` as they are.

        Records for the snapshots the middle of the shot when ``midway`` is
        set, and its end unless ``end`` is off.
        """
        caption = scene.text(label, role="code").hud().place(self.zone, anchor="left").hidden()
        swap = [caption.animate.fade_in().duration(0.2)]
        if self.current is not None:
            swap.append(self.current.animate.fade_out().duration(0.15))
        self.current = caption
        if midway:
            SEEKS.append(scene.cursor + duration / 2)
        scene.play(swap + [move.duration(duration) for move in moves] + list(also))
        scene.wait(hold)
        if end:
            SEEKS.append(scene.cursor - min(hold / 2, 0.1) - 0.05)


# Pan centers a point or a drawable, zoom scales the view about its center
# (values above one zoom in; the default interpolation is exponential, so a
# large zoom reads at a constant pace) and rotate turns it, in radians.
# Without .animate the same calls cut to the new pose at the cursor. The
# camera keeps its pose from one segment to the next, so later segments
# start with a cut back to the default pose.
scene.segment("pan_zoom_rotate")
world()
star = scene.geometry.star(5, 0.8, 0.36).fill(accent).move_to(-9, 3.2)
captions = Captions()
captions.shot("scene.camera.animate.pan_to(6, 3)", scene.camera.animate.pan_to(6, 3))
captions.shot("scene.camera.animate.pan_to(star)", scene.camera.animate.pan_to(star))
captions.shot("scene.camera.animate.zoom_to(2.5)", scene.camera.animate.zoom_to(2.5))
captions.shot(
    'scene.camera.animate.zoom_to(0.7, interpolation="linear")',
    scene.camera.animate.zoom_to(0.7, interpolation="linear"),
)
captions.shot("scene.camera.animate.rotate_to(0.3)", scene.camera.animate.rotate_to(0.3))
scene.camera.pan_to(-6, -3).zoom_to(1.5).rotate_to(0)
captions.shot("scene.camera.pan_to(-6, -3).zoom_to(1.5).rotate_to(0)")
captions.shot("scene.camera.animate.reset()", scene.camera.animate.reset())

# frame_to pans and zooms so the targets fit the view. The margin is half a
# unit by default, one value, (vertical, horizontal) or four sides in CSS
# order; dynamic=True keeps framing targets that move during the shot, and
# the camera keeps the last framing when it ends.
scene.segment("frame_to")
scene.camera.reset()
world()
circle = scene.geometry.circle(0.8).fill(success).move_to(-6, 3.2)
square = scene.geometry.square(1.4).fill(danger).move_to(6, -2.8)
captions = Captions()
captions.shot("scene.camera.animate.frame_to(circle)", scene.camera.animate.frame_to(circle))
captions.shot("frame_to(circle, margin=3)", scene.camera.animate.frame_to(circle, margin=3))
captions.shot("frame_to(square, margin=(0.5, 4))", scene.camera.animate.frame_to(square, margin=(0.5, 4)))
captions.shot(
    "frame_to(square, margin=(0.3, 0.3, 3, 0.3))",
    scene.camera.animate.frame_to(square, margin=(0.3, 0.3, 3, 0.3)),
)
captions.shot(
    'frame_to(circle, margin=1, interpolation="linear")',
    scene.camera.animate.frame_to(circle, margin=1, interpolation="linear"),
)
captions.shot("frame_to([circle, square], margin=1)", scene.camera.animate.frame_to([circle, square], margin=1))
captions.shot(
    "frame_to([circle, square], margin=1, dynamic=True)",
    scene.camera.animate.frame_to([circle, square], margin=1, dynamic=True),
    square.animate.move_to(15, -8.8),
    duration=1.6,
)

# follow keeps a target in view while it plays and leaves the camera where it
# took it: lag eases the camera onto the target over about that many seconds,
# offset shifts the view from the target, and offset_space="local" turns the
# offset with it. Each shot lasts through the hold, so the camera settles.
scene.segment("follow")
scene.camera.reset()
world()
route = scene.geometry.circle(9).no_fill().stroke(accent, 0.04)
rover = scene.geometry.polygon([(0.9, 0.0), (-0.7, 0.56), (-0.35, 0.0), (-0.7, -0.56)]).fill(accent).move_to(9, 0)
captions = Captions()


def follow(label, move, quarter):
    """Follow the rover with ``move`` while it drives a quarter of the route, and through the hold."""
    drive = rover.animate.move_along(route, orient=True, start=quarter / 4, end=(quarter + 1) / 4)
    drive = drive.duration(LEG).easing(Easing.LINEAR)
    captions.shot(label, move, duration=LEG + HOLD, hold=0, midway=True, end=False, also=[drive])


follow("scene.camera.animate.follow(rover)", scene.camera.animate.follow(rover), 0)
follow("follow(rover, lag=1)", scene.camera.animate.follow(rover, lag=1), 1)
follow("follow(rover, offset=(4, 0))", scene.camera.animate.follow(rover, offset=(4, 0)), 2)
follow(
    'follow(rover, offset=(4, 0), offset_space="local")',
    scene.camera.animate.follow(rover, offset=(4, 0), offset_space="local"),
    3,
)
captions.shot("scene.camera.animate.reset()", scene.camera.animate.reset())

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
