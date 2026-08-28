# %% basic_circle
from gaanim import Easing, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background=BLACK)
circle = scene.geometry.circle(1).fill(BLUE).stroke(WHITE, 0.05).move_to(-1.5, 0)
rect = scene.geometry.rect(2, 1.25).fill(GOLD).move_to(1.5, 0)

scene.play([
    circle.animate.create().duration(1.0).easing(Easing.SMOOTH),
    rect.animate.grow_from_center().duration(1.0).easing(Easing.spring(stiffness=90.0, damping=12.0)),
])
scene.wait(0.5)
scene.play([circle.animate.shift_by(2.25, 0).duration(1.0), rect.animate.fade_out().duration(0.5)])
scene.render()

# %% text_and_math
from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Mass-energy equivalence", role="title").fill(WHITE).move_to(0, 2.25)
equation = scene.text("$E = m c^2$").fill(GOLD).move_to(0, 0)
caption = scene.text("Energy and mass are related", role="subtitle").fill(BLUE).move_to(0, -1.875)

scene.play([title.animate.write().duration(1.0), equation.animate.write().duration(1.5)])
scene.play([caption.animate.fade_in().duration(0.8)])
scene.wait(1.0)
scene.render()

# %% shapes_gallery
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene, stagger

scene = Scene(frame=(16, 9), background=BLACK)
circle = scene.geometry.circle(0.75).fill(BLUE).move_to(-3, 1)
rect = scene.geometry.rect(1.625, 1).fill(RED).move_to(0, 1)
square = scene.geometry.square(1.125).fill(GREEN).move_to(3, 1)
ellipse = scene.geometry.ellipse(1.125, 0.6875).fill(GOLD).move_to(-1.5, -1.5)
arrow = scene.geometry.arrow(0, -1.5, 2.75, -1.5).stroke(WHITE, 0.05)

scene.play(stagger(
    circle.animate.grow_from_center().duration(0.8),
    rect.animate.grow_from_center().duration(0.8),
    square.animate.grow_from_center().duration(0.8),
    ellipse.animate.create().duration(0.8),
    arrow.animate.create().duration(0.8),
    each=0.1,
))
scene.wait(1.0)
scene.render()
