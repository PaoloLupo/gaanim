"""Masked reveals by line and by word, and the symmetric conceal."""

import os

from gaanim import GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background="#0f172a")
scene.text("Revelados con máscara", role="title").fill(WHITE).move_to(0, 3.6)

lines = scene.text("Cada línea sube\ndesde detrás\nde su máscara").fill(WHITE).move_to(-4.2, 0.6)
words = scene.text("palabra por palabra").fill(GOLD).move_to(3.6, 1.6)
blurred = scene.text("con desenfoque").fill(WHITE).move_to(3.6, 0.2)
scaled = scene.text("o con escala").fill(WHITE).move_to(3.6, -1.2)
for x, y, label in [
    (-4.2, -1.6, 'by="line", "slide_up"'),
    (3.6, 1.0, 'by="word", "slide_down"'),
    (3.6, -0.4, 'by="word", "blur"'),
    (3.6, -1.8, 'by="grapheme", "scale"'),
]:
    scene.text(label).fill(GRAY).scale_by(0.45).move_to(x, y)

scene.play([
    lines.animate.reveal(by="line", style="slide_up", stagger=0.15).duration(1.2),
    words.animate.reveal(by="word", style="slide_down", stagger=0.12).duration(1.2),
    blurred.animate.reveal(by="word", style="blur", stagger=0.1).duration(1.2),
    scaled.animate.reveal(by="grapheme", style="scale", stagger=0.04).duration(1.2),
])
scene.wait(0.4)
scene.play([lines.animate.conceal(by="line", style="slide_up", stagger=0.15).duration(1.0)])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.3, 0.6, 0.9, 2.0])
else:
    scene.render()
