from gaanim import Engine, Transition

engine = Engine(width=1920, height=1080, title="Presentation")

intro = engine.scene("intro")
title = intro.title("Bienvenidoa")
intro.play(title.write())
intro.wait(1.0)

demo = engine.scene("demo")
circle = demo.circle(10.0)
demo.play(circle.create())
demo.wait(1.0)

engine.sequence([intro, demo], Transition.cross_fade(0.5))
engine.render()
