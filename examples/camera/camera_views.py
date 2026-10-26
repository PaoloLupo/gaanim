"""Camera views: shapes that show what a second camera sees, like a picture-in-picture or a lens.

``drawable.camera_view(...)`` turns a closed shape into a screen. Its camera
looks through a frame drawable, or at a center with a zoom of its own, and can
show view layers the main camera hides, and ``view.snapshot()`` keeps what it
shows at one instant. The map's labels are too small to read until a view
magnifies them; a strip pinned to the screen names each call.
"""

import os

from gaanim import Scene, Zones

MOVE = 1.2
HOLD = 0.5
SEEKS = []

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
danger = scene.colors.danger
muted = scene.colors.muted
panel = scene.colors.panel
rule = scene.colors.rule

page = scene.layout.zones(Zones.rows(["1fr", "80px"]))
stage = page[0].inset("page_padding")


def world(zone):
    """Unit tiles inside ``zone``, each labelled with its coordinates in print too small to read."""
    for x in range(round(zone.left + 0.5), round(zone.right - 0.5) + 1):
        for y in range(round(zone.bottom + 0.5), round(zone.top - 0.5) + 1):
            scene.geometry.rounded_rect(0.92, 0.92, 0.06).no_fill().stroke(rule, 0.01).move_to(x, y)
            scene.text(f"({x}, {y})", role="code").fill(muted).scale_to(0.4).move_to(x, y)


class Captions:
    """A strip pinned to the bottom of the screen whose caption names each shot."""

    def __init__(self):
        strip = page[1]
        scene.geometry.rect(strip.width, strip.height).fill(panel).no_stroke().hud().move_to(*strip.center)
        self.zone = strip.inset((0, "page_padding"))
        self.current = None

    def shot(self, label, *moves, duration=MOVE, hold=HOLD, snap=True):
        """Show ``label`` and play ``moves`` under it, each lasting ``duration``; snapshot the hold."""
        caption = scene.text(label, role="code").hud().place(self.zone, anchor="left").hidden()
        swap = [caption.animate.fade_in().duration(0.2)]
        if self.current is not None:
            swap.append(self.current.animate.fade_out().duration(0.15))
        self.current = caption
        scene.play(swap + [move.duration(duration) for move in moves])
        scene.wait(hold)
        if snap:
            SEEKS.append(scene.cursor - hold / 2)


# The frame is the camera: moving it pans the view, scaling it down zooms in
# (the zoom is the screen's size over the frame's) and turning it turns the
# view the other way. The screen and the frame stay out of the view.
scene.segment("frame")
left, right = stage.split(Zones.columns(["2fr", "3fr"], gap="48px"))
world(left)
frame = scene.geometry.rect(2.4, 1.35).no_fill().stroke(accent, 0.03).move_to(-4, 1)
screen = scene.geometry.rounded_rect(right.width, right.width * 9 / 16, 0.12).stroke(accent, 0.05)
screen.move_to(*right.center)
view = screen.camera_view(frame)
captions = Captions()
captions.shot("view = screen.camera_view(frame)", screen.animate.fade_in())
captions.shot("frame.animate.scale_to(0.5)", frame.animate.scale_to(0.5))
captions.shot("frame.animate.move_to(-6, -2)", frame.animate.move_to(-6, -2))
captions.shot("frame.animate.rotate_to(0.5)", frame.animate.rotate_to(0.5))

# When the screen's proportions differ from the frame's, fit decides: "contain"
# keeps the whole frame in view, "cover" fills the screen and crops, and
# "stretch" scales each axis on its own.
scene.segment("fit")
left, right = stage.split(Zones.columns(["1fr", "2fr"], gap="48px"))
world(left)
frame = scene.geometry.rect(2.4, 1.35).no_fill().stroke(accent, 0.03).move_to(-5, 1)
columns = right.split(Zones.columns(3, gap="32px"))
for column, fit in zip(columns, ["contain", "cover", "stretch"]):
    body, foot = column.split(Zones.rows(["1fr", 0.8]))
    side = min(body.width, body.height)
    square = scene.geometry.rounded_rect(side, side, 0.12).stroke(accent, 0.05).move_to(*body.center)
    square.camera_view(frame, fit=fit)
    scene.text(f'fit="{fit}"', role="code").place(foot, anchor="center")
captions = Captions()
captions.shot("frame.animate.move_to(-5, -2)", frame.animate.move_to(-5, -2), duration=2.0)

# Without a frame, the camera looks at center with zoom of its own. The view
# animates like the main camera: pan_to, zoom_to at a constant perceived speed
# and rotate_to. Here the lens and its view travel together.
scene.segment("lens")
world(stage)
lens = scene.geometry.circle(1.6).stroke(accent, 0.05).move_to(-5, 1)
view = lens.camera_view(center=(-5, 1), zoom=3)
captions = Captions()
captions.shot("view = lens.camera_view(center=(-5, 1), zoom=3)", lens.animate.fade_in())
captions.shot(
    "lens.animate.move_to(1, -1), view.animate.pan_to(1, -1)",
    lens.animate.move_to(1, -1),
    view.animate.pan_to(1, -1),
    duration=1.6,
)
captions.shot("view.animate.zoom_to(6)", view.animate.zoom_to(6))
captions.shot("view.animate.rotate_to(0.6)", view.animate.rotate_to(0.6))

# view_layer("xray") takes drawables out of the main camera: only views that
# list the layer show them. exclude hides drawables from the view, here the
# lids over the hidden shapes, and background=None lets the screen's own fill
# show behind the view instead of the scene background.
scene.segment("layers")
world(stage)
lids = [
    scene.geometry.rounded_rect(3.2, 3.2, 0.2).fill(panel).stroke(rule, 0.03).move_to(x, 0)
    for x in (-5, 0, 5)
]
scene.geometry.star(5, 1.0, 0.45).fill(accent).move_to(-5, 0).view_layer("xray")
scene.geometry.circle(0.9).fill(success).move_to(0, 0).view_layer("xray")
scene.geometry.square(1.6).fill(danger).move_to(5, 0).view_layer("xray")
# Without frame or center, the view looks at the origin with a zoom of 1.
lens = scene.geometry.circle(1.8).fill(panel).stroke(accent, 0.05)
view = lens.camera_view(layers=["xray"], exclude=lids, background=None)
captions = Captions()
captions.shot('lens.camera_view(layers=["xray"], exclude=lids, background=None)', lens.animate.fade_in())
captions.shot(
    "lens.animate.move_to(5, 0), view.animate.pan_to(5, 0)",
    lens.animate.move_to(5, 0),
    view.animate.pan_to(5, 0),
    duration=1.6,
)
captions.shot(
    "lens.animate.move_to(-5, 0), view.animate.pan_to(-5, 0)",
    lens.animate.move_to(-5, 0),
    view.animate.pan_to(-5, 0),
    duration=2.0,
)

# view.snapshot() freezes what the view shows at the cursor into a copy of
# its screen. The copy starts hidden, enters with its first animation and
# keeps that picture wherever it goes and however it scales, while the view
# goes on following the rover.
scene.segment("snapshots")
left, right = stage.split(Zones.columns(["2fr", "3fr"], gap="48px"))
world(left)
live, shelf = right.split(Zones.rows(["3fr", "2fr"], gap="32px"))
width = min(live.width, live.height * 16 / 9)
screen = scene.geometry.rounded_rect(width, width * 9 / 16, 0.12).stroke(accent, 0.05)
screen.move_to(*live.center)
# Tile corners, so that the rover hides none of the labels around it.
stops = [(-3.5, 0.5), (-5.5, -1.5), (-6.5, 1.5)]
rover = scene.geometry.dot(0.1).fill(accent).move_to(-5.5, -0.5)
frame = scene.geometry.rect(2.4, 1.35).no_fill().stroke(accent, 0.03).move_to(-5.5, -0.5)
view = screen.camera_view(frame)
view.follow(rover)
slots = shelf.split(Zones.columns(3, gap="24px"))
scale = min(slots[0].width / width, slots[0].height / (width * 9 / 16))
captions = Captions()
captions.shot("view.follow(rover)", screen.animate.fade_in(), rover.animate.move_to(*stops[0]))
for index, slot in enumerate(slots):
    photo = view.snapshot()
    moves = [photo.animate.move_to(*slot.center), photo.animate.scale_to(scale)]
    if index + 1 < len(stops):
        moves.append(rover.animate.move_to(*stops[index + 1]))
    captions.shot("photo = view.snapshot()", *moves)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
