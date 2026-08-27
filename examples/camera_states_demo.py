"""Reusable camera states, exact captures, and named restoration."""

from gaanim import Anchor, BLACK, BLUE, CORAL, GOLD, WHITE, Scene


scene = Scene(960, 540, background=BLACK)
scene.text("Camera states", role="title").fill(WHITE).at(0, 220, anchor=Anchor.CENTER)
scene.geometry.circle(90).fill(BLUE).at(-260, -20)
scene.geometry.rect(190, 120).fill(GOLD).at(240, -20)
scene.geometry.dot(20).fill(CORAL).at(0, 20)

# Concrete states are reusable values. Captures freeze the authored camera at
# this exact point of the timeline, even when earlier motion was reactive.
left_detail = scene.camera.state_2d(center=(-260, -20), zoom=2.0)
overview = scene.camera.capture()

scene.wait(0.4)
scene.camera.to(left_detail, duration=1.0)
scene.camera.save("left detail")
scene.wait(0.3)
scene.camera.to(overview, duration=1.0)
scene.camera.restore("left detail", duration=0.8)
scene.camera.to(overview, duration=0.8)

scene.render()
