"""Quick smoke test for template-backed segments and explicit stops."""

from gaanim import BLACK, BLUE, GOLD, GREEN, RED, Scene, comparison, title_slide

scene = Scene(frame=(16, 9), background=BLACK)
intro = scene.segment("intro", notes="Present the goal.", template=title_slide)
circle = scene.geometry.circle(0.5).fill(BLUE)
intro.bind(title=scene.text("Gaanim slides").scale_to(5.0).fill(GREEN), subtitle=circle)
scene.wait(0.3)
scene.stop("circle")

details = scene.segment("details", notes="Introduce the second shape.", template=comparison)
rect = scene.geometry.rect(1, 0.666667).fill(RED)
details.bind(title=scene.text("Shapes", role="title"), left=scene.text("Shapes").fill(GOLD), right=rect)
scene.play([rect.animate.create().duration(0.5)])
scene.wait(0.3)
scene.stop("rectangle")

finale = scene.segment("finale", notes="Close with the summary.", template=title_slide)
scene.reuse(circle)
label = scene.text("Slide 3").fill(GOLD)
finale.bind(title=label, subtitle=circle)
scene.play([label.animate.write().duration(0.5)])
scene.wait(0.3)
scene.render()
