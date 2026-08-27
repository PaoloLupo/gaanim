"""Quick smoke test for template-backed segments and explicit stops."""

from gaanim import BLACK, BLUE, GOLD, GREEN, RED, Scene, comparison, title_slide

scene = Scene(1920, 1080, background=BLACK)
intro = scene.segment("intro", notes="Present the goal.", template=title_slide)
circle = scene.geometry.circle(60).fill(BLUE)
intro.bind(title=scene.text("Gaanim slides").scaled(5.0).fill(GREEN), subtitle=circle)
scene.wait(0.3)
scene.stop("circle")

details = scene.segment("details", notes="Introduce the second shape.", template=comparison)
rect = scene.geometry.rect(120, 80).fill(RED)
details.bind(title=scene.text("Shapes", role="title"), left=scene.text("Shapes").fill(GOLD), right=rect)
scene.play([rect.create().duration(0.5)])
scene.wait(0.3)
scene.stop("rectangle")

finale = scene.segment("finale", notes="Close with the summary.", template=title_slide)
scene.reuse(circle)
label = scene.text("Slide 3").fill(GOLD)
finale.bind(title=label, subtitle=circle)
scene.play([label.write().duration(0.5)])
scene.wait(0.3)
scene.render()
