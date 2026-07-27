"""Quick smoke test for semantic Scene slides and reveal steps."""

from gaanim import Anchor, BLACK, BLUE, GOLD, RED, Scene

scene = Scene(640, 360, background=BLACK)
intro = scene.slide("intro", notes="Present the goal.", layout="title")
intro.region("title").place(scene.text("Gaanim slides").fill(GOLD), Anchor.CENTER)
circle = scene.circle(60).fill(BLUE)
scene.play([circle.create().duration(0.5)])
scene.wait(0.3)
intro.step("circle")

details = scene.slide("details", notes="Introduce the second shape.", layout="two_columns")
details.region("left").place(scene.text("Shapes").fill(GOLD), Anchor.CENTER)
rect = details.region("right").place(scene.rect(120, 80).fill(RED), Anchor.CENTER)
scene.play([rect.create().duration(0.5)])
scene.wait(0.3)
details.step("rectangle")

finale = scene.slide("finale", notes="Close with the summary.", layout="closing")
label = finale.region("title").place(scene.text("Slide 3").fill(GOLD), Anchor.CENTER)
scene.play([circle.move(100, 0).duration(0.5).smooth(), label.write().duration(0.5)])
scene.wait(0.3)
scene.render()
