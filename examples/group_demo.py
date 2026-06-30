"""Example: Canvas grouping and simple timeline sequencing."""

from gaanim import BLACK, BLUE, GOLD, WHITE, Canvas

c = Canvas(1920, 1080, background=BLACK)

logo = c.circle(80.0).fill(BLUE).at(0.0, 0.0)
logo.fade_in().duration(1.0)
c.wait(0.5)

title = c.title("Gaanim").fill(WHITE).at(0.0, 180.0)
title.fade_in().duration(1.0).ease("spring")
c.wait(1.0)

logo.move(-300.0, 0.0).duration(1.0)
c.wait(0.3)

dot = c.dot(20.0).fill(GOLD).at(-180.0, 0.0)
dot.fade_in().duration(0.5)

group = c.group([logo, dot])
group.move(0.0, -80.0).duration(0.8)
c.wait(1.0)

logo.indicate().duration(0.5)
c.wait(0.5)

group.fade_out().duration(1.0)
c.wait(0.5)

c.render()
