"""Camera bindings: constraints that tie the camera to drawables and reactive values.

``bind_2d`` keeps writing the camera's center, zoom or rotation every frame,
across plays and segments, until it is disabled. Each binding is a
``CameraConstraint`` with ``enable`` and ``disable``; ``influence`` blends it
with the pose below it. A strip pinned to the bottom of the screen names each
call, and one at the top shows the values that drive the camera.
"""

import os

from gaanim import Scene, Zones, computed

MOVE = 1.4
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


def world(columns=range(-5, 6), rows=range(-3, 4)):
    """Tiles every ``SPACING`` units, each labelled with the coordinates of its center."""
    for column in columns:
        for row in rows:
            x, y = column * SPACING, row * SPACING
            scene.geometry.rounded_rect(2.8, 2.8, 0.16).no_fill().stroke(rule, 0.02).move_to(x, y)
            scene.text(f"({x:g}, {y:g})", role="code").fill(muted).scale_to(1.3).move_to(x, y - 0.95)


def strip(zone):
    """A panel pinned to the screen over ``zone``; returns the zone inset for its contents."""
    scene.geometry.rect(zone.width, zone.height).fill(panel).no_stroke().hud().move_to(*zone.center)
    return zone.inset((0, "page_padding"))


class Captions:
    """Strips pinned to the screen: the caption of each shot at the bottom, the values at the top."""

    def __init__(self, *values):
        top, _, bottom = scene.layout.zones(Zones.rows(["80px", "1fr", "80px"]))
        self.zone = strip(bottom)
        self.current = None
        if values:
            zone = strip(top)
            readouts = [
                scene.viz.readout(value, label=name, color=accent, font="JetBrains Mono", font_size=0.36).hud()
                for name, value in values
            ]
            zone.arrange(*readouts, justify="end", gap="48px")

    def shot(self, label, *moves, duration=MOVE, hold=HOLD, snap=True, midway=False):
        """Show ``label`` and play ``moves`` under it, each lasting ``duration``.

        Records the end of the shot for the snapshots unless ``snap`` is off,
        and its middle too when ``midway`` is set.
        """
        caption = scene.text(label, role="code").hud().place(self.zone, anchor="left").hidden()
        swap = [caption.animate.fade_in().duration(0.2)]
        if self.current is not None:
            swap.append(self.current.animate.fade_out().duration(0.15))
        self.current = caption
        if midway:
            SEEKS.append(scene.cursor + duration / 2)
        scene.play(swap + [move.duration(duration) for move in moves])
        scene.wait(hold)
        if snap:
            SEEKS.append(scene.cursor - 0.15)


# A binding to a drawable keeps it in the center through every later move.
# disable() and enable() switch it at the cursor: the camera cuts back to its
# own pose, or onto the binding again.
scene.segment("center")
world()
probe = scene.geometry.dot(0.4).fill(accent)
captions = Captions()
rig = scene.camera.bind_2d(center=probe)
captions.shot("rig = scene.camera.bind_2d(center=probe)", snap=False)
captions.shot("probe.animate.move_to(9, 3)", probe.animate.move_to(9, 3), midway=True)
captions.shot("probe.animate.move_to(-6, -3)", probe.animate.move_to(-6, -3))
rig.disable()
captions.shot("rig.disable()")
captions.shot("probe.animate.move_to(3, 3)", probe.animate.move_to(3, 3))
rig.enable()
captions.shot("rig.enable()")

# Zoom and rotation take any scalar source: a number, a Parameter, or a
# Computed of other sources (scene.time too). Animating the source moves the
# camera. Bindings outlive their segment, so the last one is disabled first.
scene.segment("zoom_rotation")
rig.disable()
world()
zoom = scene.viz.parameter(1.0)
spin = computed(lambda z: (z - 1) / 4, inputs=[zoom])
scene.geometry.dot(0.4).fill(accent)
captions = Captions(("zoom", zoom), ("spin", spin))
captions.shot("zoom = scene.viz.parameter(1.0)", snap=False)
captions.shot("spin = computed(lambda z: (z - 1) / 4, inputs=[zoom])", snap=False)
rig = scene.camera.bind_2d(zoom=zoom, rotation=spin)
captions.shot("rig = scene.camera.bind_2d(zoom=zoom, rotation=spin)", snap=False)
captions.shot("zoom.animate.set(3)", zoom.animate.set(3), midway=True)
captions.shot("zoom.animate.set(0.6)", zoom.animate.set(0.6))

# Later bindings compose over earlier ones, each weighted by its influence
# (0..1). A binding created with enabled=False waits for enable().
scene.segment("influence")
rig.disable()
world()
circle = scene.geometry.circle(0.8).fill(success).move_to(-6, 3.3)
square = scene.geometry.square(1.4).fill(danger).move_to(6, -2.7)
mix = scene.viz.parameter(0.0)
captions = Captions(("mix", mix))
scene.camera.bind_2d(center=circle)
captions.shot("scene.camera.bind_2d(center=circle)")
scene.camera.bind_2d(center=square, influence=mix)
captions.shot("scene.camera.bind_2d(center=square, influence=mix)", snap=False)
captions.shot("mix.animate.set(1)", mix.animate.set(1))
captions.shot("mix.animate.set(0.5)", mix.animate.set(0.5))
wide = scene.camera.bind_2d(zoom=0.6, enabled=False)
captions.shot("wide = scene.camera.bind_2d(zoom=0.6, enabled=False)", snap=False)
wide.enable()
captions.shot("wide.enable()")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
