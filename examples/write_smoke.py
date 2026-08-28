"""Visual smoke test for Create, Write, and DrawBorderThenFill parity."""

from gaanim import Easing, BLACK, BLUE, GOLD, RED, Scene

c = Scene(frame=(16, 9), background=BLACK)

c.segment("intro")

# Single leaf: should reveal the trimmed path without a first-frame flash.
circle = c.geometry.circle(0.666667).stroke(RED, 0.033333).no_fill().move_to(-3.5, 1)
c.play([circle.animate.write().duration(2.0).easing(Easing.LINEAR)])

# Create traces with DoubleSmooth, then fades the authored fill without
# changing the logical stroke width.
create_label = c.text("Create").fill(GOLD).move_to(-3.5, -0.166667)
create_title = c.text("Hola!").fill(BLUE).move_to(-3.5, -1.166667)
c.play([create_label.animate.write().duration(0.8).easing(Easing.LINEAR)])
c.play([create_title.animate.create().duration(2.8)])

# Write should keep the border-first split, but stagger glyphs with a
# typewriter cadence. The lag_ratio override makes the difference obvious.
write_label = c.text("Write").fill(GOLD).move_to(0, -0.166667)
write_formula = c.text.equation("x^2 + y^2 = z^2").move_to(0, -1.166667)
c.play([write_label.animate.write().duration(0.8).easing(Easing.LINEAR)])
c.play([write_formula.animate.write().duration(3.0).lag_ratio(0.12)])

# DrawBorderThenFill should also stagger child glyphs now instead of only animating the root.
dbtf_label = c.text("DrawBorderThenFill").fill(GOLD).move_to(3.5, -0.166667)
dbtf_text = c.text("Borde y relleno").fill(BLUE).move_to(3.5, -1.166667)
c.play([dbtf_label.animate.write().duration(0.8).easing(Easing.LINEAR)])
c.play([dbtf_text.animate.draw_border_then_fill().duration(3.0)])
c.render()
