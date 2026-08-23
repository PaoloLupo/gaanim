"""Small deterministic scene used by the cross-platform export smoke test."""

import os

from gaanim import BLUE, GOLD, Scene


scene = Scene(320, 180, background="#0b1020", margin=12)
circle = scene.circle(32).fill(BLUE).stroke(GOLD, 3)

if audio := os.environ.get("GAANIM_EXPORT_SMOKE_AUDIO"):
    scene.audio(audio, duration=0.6, volume=0.5)

scene.play([circle.create().duration(0.3)])
scene.play([circle.move(48, 0).duration(0.3)])
scene.render()
