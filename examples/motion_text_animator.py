"""Text range animator: a cascading entry, a passing wave, blur-in with tracking."""

import os

from gaanim import CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background="#0f172a")
scene.text("Animador de rango", role="title").fill(WHITE).move_to(0, 3.6)

cascade = scene.text("Tipografía cinética").fill(WHITE).scale_by(1.4).move_to(0, 1.8)
entry = cascade.animator(by="grapheme", shape="smooth", order="forward")
entry.set(offset=(0, -0.5), opacity=0.0, scale=0.6, rotation=0.3)

swell = scene.text("una ola que pasa").fill(CYAN).scale_by(1.2).move_to(0, 0.0)
wave = swell.animator(by="grapheme", shape="round", order="center").set(offset=(0, 0.35), color=GOLD)

focus = scene.text("ENFOQUE").fill(WHITE).scale_by(1.4).move_to(0, -1.9)
focus.tracking(0.5)

for y, label in [(1.0, 'shape="smooth"'), (-0.8, 'shape="round"'), (-2.9, "blur_in + tracking")]:
    scene.text(label).fill(GRAY).scale_by(0.5).move_to(0, y)

scene.play([
    entry.animate.sweep().duration(1.4),
    wave.animate.sweep().duration(1.4),
    focus.animate.blur_in(sigma=0.3, by="grapheme", stagger=0.05).duration(1.4),
    focus.animate.tracking(0.0).duration(1.4),
])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.35, 0.7, 1.05])
else:
    scene.render()
