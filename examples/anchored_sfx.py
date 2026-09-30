"""Sound effects anchored to the timeline: a free sfx, an animation's sound
and a transition's whoosh.

The sounds are tiny WAV files synthesized at startup, so the example needs no
audio assets. Export it to MP4 or WebM to hear them mixed with the video:

    gaanim export examples/anchored_sfx.py anchored_sfx.mp4
"""

import math
import os
import random
import tempfile
import wave

from gaanim import BLUE, CORAL, GOLD, WHITE, Scene, Transition


def synthesize(name: str, seconds: float, sample) -> str:
    """Write an 8 kHz mono WAV whose samples come from ``sample(t, u)``."""
    folder = os.path.join(tempfile.gettempdir(), "gaanim_anchored_sfx")
    os.makedirs(folder, exist_ok=True)
    path = os.path.join(folder, name)
    rate = 8000
    count = int(seconds * rate)
    frames = bytes(
        max(0, min(255, int(128 + 127 * sample(i / rate, i / count)))) for i in range(count)
    )
    with wave.open(path, "wb") as file:
        file.setnchannels(1)
        file.setsampwidth(1)
        file.setframerate(rate)
        file.writeframes(frames)
    return path


noise = random.Random(7)
whoosh = synthesize("whoosh.wav", 0.45, lambda t, u: noise.uniform(-0.6, 0.6) * math.sin(math.pi * u) ** 2)
typing = synthesize(
    "typing.wav", 0.4, lambda t, u: 0.7 * math.exp(-((t % 0.065) * 300)) * math.sin(2 * math.pi * 1800 * t)
)
pop = synthesize("pop.wav", 0.15, lambda t, u: 0.8 * (1 - u) * math.sin(2 * math.pi * (600 - 400 * u) * t))

scene = Scene(frame=(16, 9), background="#0b1020")

scene.segment("titulo")
title = scene.text("Efectos anclados", role="title").fill(WHITE).move_to(0, 1)
dot = scene.geometry.circle(0.6).fill(GOLD).move_to(0, -1.5)
# The typing sound starts with the write, wherever the write ends up.
scene.play(title.animate.write().duration(1.2).sound(typing, volume=0.6))
# Delaying the animation delays its pop as well.
scene.play(dot.animate.grow_from_center().duration(0.4).delay(0.3).sound(pop))
# A free effect at an absolute second, here the current cursor.
scene.media.sfx(pop, at=scene.cursor, volume=0.5)
scene.wait(0.6)

# The whoosh starts with the slide, at the start of the segment it enters.
scene.segment("detalle", Transition.slide(0.5, "left", sound=whoosh))
card = scene.geometry.rect(8, 3).fill(BLUE).move_to(0, 0)
label = scene.text("Se mueven con la animación").fill(WHITE).move_to(0, 0)
scene.play([card.animate.fade_in().duration(0.5), label.animate.fade_in().duration(0.5)])
scene.play(card.animate.fill(CORAL).duration(0.4).sound(pop, offset=-0.05))
scene.wait(0.8)

if "GAANIM_SNAPSHOTS" in os.environ:
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [1.0, 2.5, 3.2])

scene.render()
