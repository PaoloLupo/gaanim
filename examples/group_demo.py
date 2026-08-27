"""Example: Scene grouping and simple timeline sequencing."""

from gaanim import Easing, BLACK, BLUE, GOLD, WHITE, Scene

c = Scene(1920, 1080, background=BLACK)

logo = c.geometry.circle(80.0).fill(BLUE).move_to(0.0, 0.0)
c.play([logo.animate.fade_in().duration(1.0)])
c.wait(0.5)

title = c.text("Gaanim", role="title").fill(WHITE).move_to(0.0, 180.0)
c.play([title.animate.fade_in().duration(1.0).easing(Easing.spring(stiffness=300.0, damping=20.0))])
c.wait(1.0)

c.play([logo.animate.shift_by(-300.0, 0.0).duration(1.0)])
c.wait(0.3)

dot = c.geometry.dot(20.0).fill(GOLD).move_to(-180.0, 0.0)
c.play([dot.animate.fade_in().duration(0.5)])

group = c.geometry.group([logo, dot])
c.play([group.animate.shift_by(0.0, -80.0).duration(0.8)])
c.wait(1.0)

c.play([logo.animate.indicate().duration(0.5)])
c.wait(0.5)

c.play([group.animate.fade_out().duration(1.0)])
c.wait(0.5)

c.render()
