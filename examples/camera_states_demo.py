"""Reusable camera states, exact captures, and named restoration."""

from gaanim import Anchor, BLACK, BLUE, CORAL, GOLD, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK)
scene.text("Camera states", role="title").fill(WHITE).move_to(0, 3.666667, anchor=Anchor.CENTER)
scene.geometry.circle(1.5).fill(BLUE).move_to(-4.333333, -0.333333)
scene.geometry.rect(3.166667, 2).fill(GOLD).move_to(4, -0.333333)
scene.geometry.dot(0.333333).fill(CORAL).move_to(0, 0.333333)

# Concrete states are reusable values. Captures freeze the authored camera at
# this exact point of the timeline, even when earlier motion was reactive.
left_detail = scene.camera.state_2d(center=(-4.333333, -0.333333), zoom=2.0)
overview = scene.camera.capture()

scene.wait(0.4)
scene.play([scene.camera.animate.to(left_detail).duration(1.0)])
scene.camera.save("left detail")
scene.wait(0.3)
scene.play([scene.camera.animate.to(overview).duration(1.0)])
scene.play([scene.camera.animate.restore("left detail").duration(0.8)])
scene.play([scene.camera.animate.to(overview).duration(0.8)])

scene.render()
