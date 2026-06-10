"""Quick smoke test for slide() and export_slides()."""

from gaanim import BLUE, GOLD, RED, Scene

scene = Scene(640, 360, title="Slide Test")

# Slide 1: circle
c = scene.circle(60).fill(BLUE).at(0, 0)
scene.play(c.create(0.5))
scene.wait(0.3)
scene.slide()

# Slide 2: rectangle
r = scene.rectangle(120, 80).fill(RED).at(200, 0)
scene.play(r.create(0.5))
scene.wait(0.3)
scene.slide()

# Slide 3: both animate
scene.play(c.animate().shift(100, 0).duration(0.5).smooth())
scene.wait(0.3)
scene.render()
