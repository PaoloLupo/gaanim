"""Small deterministic scene used by the cross-platform export smoke test."""

import os

from gaanim import BLUE, GOLD, Scene


scene = Scene(frame=(16, 9), background="#0b1020", margin=0.6)
circle = scene.geometry.circle(1.6).fill(BLUE).stroke(GOLD, 0.15)

if audio := os.environ.get("GAANIM_EXPORT_SMOKE_AUDIO"):
    audio = scene.media.audio(audio, duration=0.6, volume=0.5)
else:
    audio = None

scene.play([circle.animate.create().duration(0.3), *([audio] if audio else [])])
scene.play([circle.animate.shift_by(2.4, 0).duration(0.3)])
scene.render()
