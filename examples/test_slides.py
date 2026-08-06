"""Quick smoke test for semantic Scene slides and reveal steps."""

from gaanim import GREEN, Anchor, BLACK, BLUE, GOLD, RED, Scene

scene = Scene(1920,1080, background=BLACK)
intro = scene.slide("intro", notes="Present the goal.", layout="title")
title = scene.text("Gaanim slides").scaled(5.0).fill(GREEN).at(0,200)
circle = scene.circle(60).fill(BLUE)
scene.play([title.write(3),circle.create().duration(0.5)])
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
