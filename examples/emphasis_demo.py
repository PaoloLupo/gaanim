"""Extra emphasis: broadcast, blink, flash_under, flash_around, spotlight and animated_boundary."""

import os

from gaanim import BLUE, CYAN, GOLD, PINK, PURPLE, WHITE, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")

pin = scene.geometry.dot(0.16).fill(PINK).move_to(-5.0, 1.6)
caret = scene.geometry.rect(0.12, 0.8).fill(WHITE).move_to(-5.0, -1.6)
label = scene.text("Gaanim resalta lo importante", role="body").fill(WHITE).move_to(0.6, 1.6)
card = scene.geometry.rounded_rect(3.6, 2.0, 0.25).fill("#1e293b").stroke(BLUE, 0.03).move_to(3.6, -1.6)
tag = scene.text("tarjeta", role="body").fill(WHITE).move_to(3.6, -1.6)

# A live frame whose stroke cycles through the colors: visible from the start.
border = card.animated_boundary([BLUE, PURPLE, CYAN], cycle_rate=0.5, width=0.05, padding=0.18)

marks = [0.0]
# Ripples spread from the pin, one after another.
scene.play([pin.animate.broadcast(count=4, max_scale=4.0, lag=0.25).duration(1.6)])
marks.append(scene.cursor - 0.8)
# The caret blinks while the word is underlined.
scene.play([
    caret.animate.blink(3).duration(1.5),
    label["resalta"].animate.flash_under(color=GOLD, width=0.05, time_width=0.6).duration(1.5),
])
marks.append(scene.cursor - 0.9)
scene.play([label["importante"].animate.flash_around(color=PINK, width=0.04).duration(1.2)])
marks.append(scene.cursor - 0.6)
# Everything but the card dims, then comes back.
scene.play([scene.fx.spotlight(card, dim=0.75, padding=0.4).duration(2.0).easing(Easing.SMOOTH)])
marks.append(scene.cursor - 1.0)
scene.wait(0.5)
marks.append(scene.cursor)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, marks)
else:
    scene.render()
