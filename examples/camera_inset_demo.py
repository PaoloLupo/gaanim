"""Camera insets: pop-out, explicit zoom, following, X-ray layers and fixed screens."""

import os

from gaanim import Anchor, Easing, Scene


scene = Scene(frame=(16, 9), margin=0.65)
scene.canvas.set_theme("tokyo-night")
scene.text("Camera insets", role="title").move_to(-7.35, 3.6, anchor=Anchor.LEFT).hud()

# A grid of dots with tiny labels that only the inset can read.
for column in range(-7, 2):
    for row in range(-1, 3):
        scene.geometry.dot(0.035).fill("#3B4261").no_stroke().move_to(column, row * 0.75 - 0.4)
for label, (x, y) in {"A1": (-6, -1.15), "B4": (-2, 1.1), "C2": (0.5, -0.4)}.items():
    scene.text(label, role="caption").scale_to(0.16).move_to(x + 0.25, y + 0.2)
probe = scene.geometry.dot(0.09).fill("#7DCFFF").no_stroke().move_to(-6, -1.15)

# The inset follows the probe, fixed on screen while the main camera moves.
inset = scene.camera.inset(probe, zoom=4, at=Anchor.TOP_RIGHT, follow=True, fixed=True)
zoom_label = scene.viz.readout(inset.zoom, format=".0f", prefix="zoom x").scale_to(0.55).hud().move_to(3.0, 3.1).opacity(0)

# Bones on the "xray" layer: only the lens, which lists that layer, shows them.
scene.geometry.rounded_rect(6.2, 1.9, 0.35).fill("#414868").no_stroke().move_to(4.15, -2.7)
for x in (2.2, 3.5, 4.8, 6.1):
    scene.geometry.rounded_rect(0.28, 1.3, 0.14).fill("#E0AF68").no_stroke().move_to(x, -2.7).view_layer("xray")
lens = scene.geometry.circle(0.85).stroke("#BB9AF7", 0.05).move_to(1.3, -2.7)
xray = lens.camera_view(center=(1.3, -2.7), zoom=1.25, layers=["xray"])

scene.play([inset.animate.pop_out().duration(0.8), zoom_label.animate.opacity(1).duration(0.8)])
scene.play([
    probe.animate.move_to(-2, 1.1).duration(1.0).easing(Easing.SMOOTH),
    lens.animate.move_to(3.8, -2.7).duration(2.0),
    xray.animate.pan_to(3.8, -2.7).duration(2.0),
])
scene.play([probe.animate.move_to(0.5, -0.4).duration(1.0).easing(Easing.SMOOTH)])
scene.play([
    inset.animate.zoom_to(8).duration(1.0),
    scene.camera.animate.zoom_to(1.3).duration(1.0),
    lens.animate.move_to(6.4, -2.7).duration(1.0),
    xray.animate.pan_to(6.4, -2.7).duration(1.0),
])
scene.play([scene.camera.animate.reset().duration(0.8)])
scene.play([inset.animate.pop_in().duration(0.6), zoom_label.animate.opacity(0).duration(0.3)])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.4, 0.8, 1.5, 2.3, 3.3, 3.8, 4.3, 4.8, 5.2, 5.9, 6.4])
else:
    scene.render()
