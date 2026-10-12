"""Camera insets: enlarged details of the scene in screens joined to what they show.

``scene.camera.inset`` builds a screen, a frame on the target that outlines
what the screen shows, and connectors between them, and returns a
``CameraView``. The map's labels are too small to read until an inset
magnifies them; a strip pinned to the screen names the call of each shot.
"""

import os

from gaanim import Anchor, Scene, Zones

HOLD = 0.8
SEEKS = []

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
muted = scene.canvas.color("muted")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")


def world(columns=range(-10, 11), rows=range(-5, 6)):
    """Unit tiles, each labelled with its coordinates in print too small to read unmagnified."""
    for x in columns:
        for y in rows:
            scene.geometry.rounded_rect(0.92, 0.92, 0.06).no_fill().stroke(rule, 0.01).move_to(x, y)
            scene.text(f"({x}, {y})", role="code").fill(muted).scale_to(0.4).move_to(x, y)


class Captions:
    """A strip pinned to the bottom of the screen: the caption of each shot on the left, values on the right."""

    def __init__(self, *values):
        strip = scene.layout.zones(Zones.rows(["1fr", "80px"]))[1]
        scene.geometry.rect(strip.width, strip.height).fill(panel).no_stroke().hud().move_to(*strip.center)
        self.zone = strip.inset((0, "page_padding"))
        self.current = None
        if values:
            readouts = [
                scene.viz.readout(value, label=name, color=accent, font="JetBrains Mono", font_size=0.36).hud()
                for name, value in values
            ]
            self.zone.arrange(*readouts, justify="end", gap="48px")

    def shot(self, label, *moves, duration=0.8, hold=HOLD, snap=True):
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


def show(label, view):
    """Pop ``view`` out of the region it shows under ``label``, hold it, and pop it back in."""
    captions.shot(label, view.animate.pop_out())
    scene.play([view.animate.pop_in().duration(0.5)])


# The target is a point, a drawable or an anchor point. at places the screen
# against a corner or edge of the frame, or centers it on a point; shape is
# "rounded" (the default), "rect" or "circle"; size is the screen's width (a
# circle's diameter), 30% of the frame by default, or (width, height); aspect
# sets the proportions; color strokes screen, frame and connectors.
scene.segment("placement")
world()
captions = Captions()
show("scene.camera.inset((-5, 2), zoom=4)", scene.camera.inset((-5, 2), zoom=4))
show(
    'inset((-3, -2), zoom=5, at=Anchor.RIGHT, shape="circle")',
    scene.camera.inset((-3, -2), zoom=5, at=Anchor.RIGHT, shape="circle"),
)
show(
    'inset((3, 2), zoom=3, at=Anchor.LEFT, shape="rect", aspect=1)',
    scene.camera.inset((3, 2), zoom=3, at=Anchor.LEFT, shape="rect", aspect=1),
)
show(
    "inset((0, -3), zoom=4, at=Anchor.TOP, size=(10, 2.4))",
    scene.camera.inset((0, -3), zoom=4, at=Anchor.TOP, size=(10, 2.4)),
)
show(
    "inset((5, -2), zoom=4, at=(-3, 1), connectors=False, color=success)",
    scene.camera.inset((5, -2), zoom=4, at=(-3, 1), connectors=False, color=success),
)

# A CameraView animates like the main camera: pan_to (a point or a drawable),
# zoom_to, at a constant perceived speed, and rotate_to, which turns the frame
# and the view the other way. view.zoom is a reactive value for readouts.
scene.segment("view_moves")
world()
probe = scene.geometry.dot(0.06).fill(accent).move_to(3.5, -2.5)
view = scene.camera.inset((-6, 3), zoom=4)
captions = Captions(("view.zoom", view.zoom))
captions.shot("view = scene.camera.inset((-6, 3), zoom=4)", view.animate.pop_out())
captions.shot("view.animate.pan_to(-2, 1)", view.animate.pan_to(-2, 1))
captions.shot("view.animate.zoom_to(8)", view.animate.zoom_to(8))
captions.shot("view.animate.rotate_to(0.4)", view.animate.rotate_to(0.4))
captions.shot("view.animate.pan_to(probe)", view.animate.pan_to(probe))
captions.shot("view.animate.pop_in()", view.animate.pop_in(), snap=False)

# follow=True keeps the frame on a moving target. fixed=True pins the screen
# to the output like a HUD overlay: when the main camera moves, the fixed
# screen stays put while the other one moves with the scene.
scene.segment("follow_fixed")
scene.camera.reset()
world()
probe = scene.geometry.dot(0.06).fill(accent).move_to(2.5, -2.3)
pinned = scene.camera.inset(probe, zoom=3, follow=True, fixed=True)
loose = scene.camera.inset((-5, -2), zoom=3, at=Anchor.TOP_LEFT, color=success)
captions = Captions()
captions.shot(
    "pinned = scene.camera.inset(probe, zoom=3, follow=True, fixed=True)",
    pinned.animate.pop_out(),
    loose.animate.pop_out(),
)
captions.shot("loose = scene.camera.inset((-5, -2), zoom=3, at=Anchor.TOP_LEFT)", snap=False)
captions.shot("probe.animate.move_to(5.5, -0.3)", probe.animate.move_to(5.5, -0.3), duration=1.6)
captions.shot("scene.camera.animate.pan_to(-2, -1)", scene.camera.animate.pan_to(-2, -1), duration=1.2)
captions.shot("scene.camera.animate.zoom_to(1.4)", scene.camera.animate.zoom_to(1.4), duration=1.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
