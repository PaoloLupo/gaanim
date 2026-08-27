"""Small deterministic scene used by the cross-platform export smoke test."""

import os

from gaanim import BLUE, GOLD, Scene


scene = Scene(320, 180, background="#0b1020", margin=12)
circle = scene.geometry.circle(32).fill(BLUE).stroke(GOLD, 3)

if audio := os.environ.get("GAANIM_EXPORT_SMOKE_AUDIO"):
    audio = scene.media.audio(audio, duration=0.6, volume=0.5)
else:
    audio = None

scene.play([circle.animate.create().duration(0.3), *([audio] if audio else [])])
scene.play([circle.animate.shift_by(48, 0).duration(0.3)])
scene.render()
