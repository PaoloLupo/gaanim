from gaanim import Easing, BLACK, GRAY, WHITE, Anchor, Direction, Scene, stagger

c = Scene(width=1920, height=1080, background=GRAY)
title = c.text("Hola").to_edge(Direction.UP, 100.0)
box = c.geometry.rounded_rect(220.0, 90.0, 12.0).next_to(title, Direction.DOWN, 24.0).fill(BLACK)
label = c.text("Mundo").fill(WHITE).align_to(box, Anchor.CENTER)

c.play(stagger(
        title.animate.write().duration(2.0).easing(Easing.spring(stiffness=90.0, damping=12.0)),
        box.animate.write(),
        label.animate.write().easing(Easing.spring(stiffness=90.0, damping=12.0)),
    each=0.1,
))


c.render()
