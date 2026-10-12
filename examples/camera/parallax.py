"""Parallax: layers at different depths drift at their own speed, and depth of field blurs them.

``scene.layer(depth)`` makes a 2.5D layer: its members move by ``1 / depth``
of the camera's pan and zoom by ``zoom ** (1 / depth)``, so far layers drift
slowly and near ones sweep past. Each row of the landscape is one layer,
labelled with the call that made it; a strip pinned to the screen names the
camera call of each shot.
"""

import math
import os

from gaanim import Scene, Zones

MOVE = 1.6
HOLD = 0.5
SEEKS = []

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")

page = scene.layout.zones(Zones.rows(["1fr", "80px"]))
bands = page[0].inset("page_padding").split(Zones.rows(5, gap="12px"))


def sun(x, zone):
    return scene.geometry.circle(zone.height * 0.5).fill(accent).move_to(x, zone.center[1])


def mountain(x, zone):
    return scene.geometry.polygon([(x - 1.8, zone.bottom), (x, zone.top), (x + 1.8, zone.bottom)]).fill(rule)


def hill(x, zone):
    return scene.geometry.ellipse(1.3, zone.height / 2).fill(muted).move_to(x, zone.center[1])


def tree(x, zone):
    return scene.geometry.polygon([(x - 0.5, zone.bottom), (x, zone.top), (x + 0.5, zone.bottom)]).fill(success)


def rock(x, zone):
    return scene.geometry.rounded_rect(1.6, zone.height, 0.3).fill(foreground).move_to(x, zone.center[1])


ROWS = [
    ("math.inf", math.inf, sun, 8.0),
    ("4", 4, mountain, 3.6),
    ("2", 2, hill, 3.0),
    ("1", 1, tree, 1.4),
    ("0.6", 0.6, rock, 3.4),
]


def landscape():
    """One layer per band, its label above its shapes; returns the layers."""
    layers = []
    for band, (name, depth, shape, spacing) in zip(bands, ROWS):
        head, body = band.split(Zones.rows([0.42, "1fr"]))
        layer = scene.layer(depth=depth)
        first = band.left + (1.5 if shape is not sun else 6.5)
        layer.add(*[shape(first + spacing * index, body) for index in range(round(30 / spacing))])
        label = scene.text(f"scene.layer(depth={name})", role="code").fill(foreground)
        layer.add(label.place(head, anchor="left"))
        layers.append(layer)
    return layers


class Captions:
    """A strip pinned to the bottom of the screen whose caption names each shot."""

    def __init__(self):
        strip = page[1]
        scene.geometry.rect(strip.width, strip.height).fill(panel).no_stroke().hud().move_to(*strip.center)
        self.zone = strip.inset((0, "page_padding"))
        self.current = None

    def shot(self, label, *moves, duration=MOVE, hold=HOLD, midway=False):
        """Show ``label`` and play ``moves`` under it, each lasting ``duration``; snapshot the hold."""
        caption = scene.text(label, role="code").hud().place(self.zone, anchor="left").hidden()
        swap = [caption.animate.fade_in().duration(0.2)]
        if self.current is not None:
            swap.append(self.current.animate.fade_out().duration(0.15))
        self.current = caption
        if midway:
            SEEKS.append(scene.cursor + duration / 2)
        scene.play(swap + [move.duration(duration) for move in moves])
        scene.wait(hold)
        SEEKS.append(scene.cursor - hold / 2)


# Pans move each layer by 1 / depth of the camera's motion (math.inf never
# moves), zooms scale it by zoom ** (1 / depth), and rotation turns every
# layer alike. A layer is a Drawable: animating it acts on all its members.
scene.segment("layers")
layers = landscape()
captions = Captions()
captions.shot("scene.camera.animate.pan_to(8, 0)", scene.camera.animate.pan_to(8, 0), midway=True)
captions.shot("scene.camera.animate.zoom_to(1.6)", scene.camera.animate.zoom_to(1.6))
captions.shot("scene.camera.animate.rotate_to(0.12)", scene.camera.animate.rotate_to(0.12))
captions.shot("scene.camera.animate.reset()", scene.camera.animate.reset())
captions.shot("layers[2].animate.opacity(0.2)", layers[2].animate.opacity(0.2))

# depth_of_field blurs each layer by aperture * |1 - focus / depth| scene
# units, at most max_blur: the layer at the focus depth stays sharp. focus_to
# racks the focus, moving evenly through near and far layers.
scene.segment("depth_of_field")
scene.camera.reset()
landscape()
captions = Captions()
scene.camera.depth_of_field(focus=0.6, aperture=0.15)
captions.shot("scene.camera.depth_of_field(focus=0.6, aperture=0.15)")
captions.shot("scene.camera.animate.focus_to(4)", scene.camera.animate.focus_to(4), midway=True)
captions.shot(
    "scene.camera.animate.depth_of_field(max_blur=0.05)",
    scene.camera.animate.depth_of_field(max_blur=0.05),
)
captions.shot("scene.camera.animate.depth_of_field(aperture=0)", scene.camera.animate.depth_of_field(aperture=0))

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
