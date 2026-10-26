"""Camera shots: travelling along a path, whip pans and shakes.

``follow_path`` drives the camera along any outline, ``whip_pan`` jumps to a
new subject in a blink with motion blur, and ``shake`` jolts the view and
settles it back. The camera looks at a map of tiles labelled with their
coordinates; a strip pinned to the screen names the call of each shot.
"""

import math
import os

from gaanim import Scene, Zones

HOLD = 0.4
SPACING = 3.0
SEEKS = []

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
danger = scene.colors.danger
muted = scene.colors.muted
panel = scene.colors.panel
rule = scene.colors.rule


def world(columns=range(-7, 8), rows=range(-4, 5)):
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

    def shot(self, label, *moves, duration=None, hold=HOLD, lead=0.0, peek=None, end=True):
        """Show ``label`` and play ``moves`` under it, ``lead`` seconds later, lasting ``duration`` when given.

        Records for the snapshots the instant ``peek`` seconds after the moves
        start when given, and the end of the shot unless ``end`` is off.
        """
        caption = scene.text(label, role="code").hud().place(self.zone, anchor="left").hidden()
        swap = [caption.animate.fade_in().duration(0.2)]
        if self.current is not None:
            swap.append(self.current.animate.fade_out().duration(0.15))
        self.current = caption
        if peek is not None:
            SEEKS.append(scene.cursor + lead + peek)
        moves = [move if duration is None else move.duration(duration) for move in moves]
        scene.play(swap + [move.delay(lead) for move in moves])
        scene.wait(hold)
        if end:
            SEEKS.append(scene.cursor - 0.15)


def crosshair():
    """Mark the center of the screen; hud() keeps it there while the camera moves."""
    for dx, dy in ((1, 0), (0, 1)):
        scene.geometry.line(-0.5 * dx, -0.5 * dy, 0.5 * dx, 0.5 * dy).stroke(accent, 0.05).hud()


# follow_path moves the camera along the path of a drawable. orient=True turns
# the view so the direction of travel points right, rotate_offset adds to that
# turn (-math.pi / 2 points it up, like a driving view), and start/end choose
# the stretch travelled, backwards when start > end. The default easing is smooth.
scene.segment("follow_path")
world()
road = scene.geometry.path([
    ("move", [(-15, -6)]),
    ("cubic", [(-9, 9), (-3, 6), (0, 0)]),
    ("cubic", [(3, -6), (9, -9), (15, 6)]),
]).no_fill().stroke(rule, 0.3)
crosshair()
captions = Captions()
TRIP = 3.0
captions.shot(
    "scene.camera.animate.follow_path(road)",
    scene.camera.animate.follow_path(road),
    duration=TRIP,
    peek=TRIP / 2,
)
captions.shot(
    "follow_path(road, orient=True, start=1, end=0)",
    scene.camera.animate.follow_path(road, orient=True, start=1, end=0),
    duration=TRIP,
    peek=TRIP / 2,
)
captions.shot(
    "follow_path(road, orient=True, rotate_offset=-math.pi / 2)",
    scene.camera.animate.follow_path(road, orient=True, rotate_offset=-math.pi / 2),
    duration=TRIP,
    peek=TRIP / 2,
)
captions.shot("scene.camera.animate.reset()", scene.camera.animate.reset(), duration=1.0)

# whip_pan takes pan_to's targets and lasts 0.4 s unless told otherwise, with
# most of the travel in a blink around the middle. Exported frames and
# snapshots of the move get motion blur unless blur=False.
scene.segment("whip_pan")
scene.camera.reset()
world(columns=range(-8, 9), rows=range(-2, 3))
scene.geometry.circle(1.0).fill(success).move_to(-18, 0.3)
square = scene.geometry.square(1.8).fill(danger).move_to(0, 0.3)
star = scene.geometry.star(5, 1.2, 0.5).fill(accent).move_to(18, 0.3)
captions = Captions()
captions.shot("scene.camera.animate.whip_pan(star)", scene.camera.animate.whip_pan(star), peek=0.2)
captions.shot("whip_pan(-18, 0, blur=False)", scene.camera.animate.whip_pan(-18, 0, blur=False), peek=0.2)
captions.shot("whip_pan(square).duration(1.2)", scene.camera.animate.whip_pan(square).duration(1.2), peek=0.6)

# shake is deterministic and returns to rest. By default it follows a trauma
# model: trauma (0..1) decays by decay per second and the offset grows with
# trauma squared, up to amplitude units and rotation radians, from noise
# sampled at frequency Hz. Passing only amplitude (and frequency) keeps the
# older sine shake. Offsets are in scene units, so a zoomed-in view shakes
# harder on screen. The crosshair stays on the screen's center, so the ring at
# the origin shows how far the view moves.
scene.segment("shake")
scene.camera.reset()
world(columns=range(-4, 5), rows=range(-2, 3))
scene.geometry.circle(0.35).no_fill().stroke(success, 0.05)
crosshair()
captions = Captions()
scene.camera.zoom_to(2)
captions.shot("scene.camera.zoom_to(2)", end=False)


def jolt(label, shake, peek):
    """Shake shortly after the caption appears; snapshot ``peek`` seconds into it."""
    captions.shot(label, shake, lead=0.3, peek=peek, end=False)


jolt("scene.camera.animate.shake()", scene.camera.animate.shake(), 0.12)
jolt("shake(trauma=0.4)", scene.camera.animate.shake(trauma=0.4), 0.08)
jolt("shake(trauma=1, decay=0.5)", scene.camera.animate.shake(trauma=1, decay=0.5), 0.3)
jolt(
    "shake(amplitude=1.2, rotation=0.1, seed=7)",
    scene.camera.animate.shake(amplitude=1.2, rotation=0.1, seed=7),
    0.12,
)
jolt("shake(frequency=3)", scene.camera.animate.shake(frequency=3), 0.2)
jolt("shake(0.4, 6)", scene.camera.animate.shake(0.4, 6), 0.12)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
