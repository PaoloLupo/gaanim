"""Visual smoke test for Create, Write, and DrawBorderThenFill parity."""

from gaanim import Easing, BLACK, BLUE, GOLD, RED, Scene

c = Scene(1920, 1080, background=BLACK)

c.segment("intro")

# Single leaf: should reveal the trimmed path without a first-frame flash.
circle = c.geometry.circle(80).stroke(RED, 4).no_fill().move_to(-420, 120)
c.play([circle.animate.write().duration(2.0).easing(Easing.LINEAR)])

# Create should now grow fill + outline together, one glyph at a time.
create_label = c.text("Create").fill(GOLD).move_to(-420, -20)
create_title = c.text("Hola!").fill(BLUE).move_to(-420, -140)
c.play([create_label.animate.write().duration(0.8).easing(Easing.LINEAR)])
c.play([create_title.animate.create().duration(2.8).easing(Easing.LINEAR)])

# Write should keep the border-first split, but stagger glyphs with a
# typewriter cadence. The lag_ratio override makes the difference obvious.
write_label = c.text("Write").fill(GOLD).move_to(0, -20)
write_formula = c.text.equation("x^2 + y^2 = z^2").move_to(0, -140)
c.play([write_label.animate.write().duration(0.8).easing(Easing.LINEAR)])
c.play([write_formula.animate.write().duration(3.0).lag_ratio(0.12)])

# DrawBorderThenFill should also stagger child glyphs now instead of only animating the root.
dbtf_label = c.text("DrawBorderThenFill").fill(GOLD).move_to(420, -20)
dbtf_text = c.text("Borde y relleno").fill(BLUE).move_to(420, -140)
c.play([dbtf_label.animate.write().duration(0.8).easing(Easing.LINEAR)])
c.play([dbtf_text.animate.draw_border_then_fill().duration(3.0)])
c.render()
