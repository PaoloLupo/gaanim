"""Quick smoke test for Scene slide breakpoints."""

from gaanim import BLACK, BLUE, GOLD, RED, Scene

scene = Scene(640, 360, background=BLACK)
circle = scene.circle(60).fill(BLUE)
scene.play([circle.create().duration(0.5)])
scene.wait(0.3)
scene.slide()

rect = scene.rect(120, 80).fill(RED).at(200, 0)
scene.play([rect.create().duration(0.5)])
scene.wait(0.3)
scene.slide()

label = scene.text("Slide 3").fill(GOLD).at(0, 120)
scene.play([circle.move(100, 0).duration(0.5).smooth(), label.write().duration(0.5)])
scene.wait(0.3)
scene.render()
