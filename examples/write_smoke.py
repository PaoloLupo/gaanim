"""Visual smoke test for Create, Write, and DrawBorderThenFill parity."""

from gaanim import BLACK, BLUE, GOLD, RED, Canvas

c = Canvas(1920, 1080, background=BLACK)

c.segment("intro")

# Single leaf: should reveal the trimmed path without a first-frame flash.
circle = c.circle(80).stroke(RED, 4).no_fill().at(-420, 120)
circle.write(2.0).linear()

# Create should now grow fill + outline together, one glyph at a time.
create_label = c.text("Create").fill(GOLD).at(-420, -20)
create_title = c.title("Hola!").fill(BLUE).at(-420, -140)
create_label.write(0.8).linear()
create_title.create(2.8).linear()

# Write should keep the border-first split, but stagger glyphs with a
# typewriter cadence. The lag_ratio override makes the difference obvious.
write_label = c.text("Write").fill(GOLD).at(0, -20)
write_formula = c.equation("x^2 + y^2 = z^2").at(0, -140)
write_label.write(0.8).linear()
write_formula.write(3.0).lag_ratio(0.12)

# DrawBorderThenFill should also stagger child glyphs now instead of only animating the root.
dbtf_label = c.text("DrawBorderThenFill").fill(GOLD).at(420, -20)
dbtf_text = c.text("Borde y relleno").fill(BLUE).at(420, -140)
dbtf_label.write(0.8).linear()
dbtf_text.draw_border_then_fill().duration(3.0)
c.render()
