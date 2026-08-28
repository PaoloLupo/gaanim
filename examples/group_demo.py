"""Example: Scene grouping and simple timeline sequencing."""

from gaanim import Easing, BLACK, BLUE, GOLD, WHITE, Scene

c = Scene(frame=(16, 9), background=BLACK)

logo = c.geometry.circle(0.666667).fill(BLUE).move_to(0, 0)
c.play([logo.animate.fade_in().duration(1.0)])
c.wait(0.5)

title = c.text("Gaanim", role="title").fill(WHITE).move_to(0, 1.5)
c.play([title.animate.fade_in().duration(1.0).easing(Easing.spring(stiffness=300.0, damping=20.0))])
c.wait(1.0)

c.play([logo.animate.shift_by(-2.5, 0).duration(1.0)])
c.wait(0.3)

dot = c.geometry.dot(0.166667).fill(GOLD).move_to(-1.5, 0)
c.play([dot.animate.fade_in().duration(0.5)])

group = c.geometry.group([logo, dot])
c.play([group.animate.shift_by(0, -0.666667).duration(0.8)])
c.wait(1.0)

c.play([logo.animate.indicate().duration(0.5)])
c.wait(0.5)

c.play([group.animate.fade_out().duration(1.0)])
c.wait(0.5)

c.render()
