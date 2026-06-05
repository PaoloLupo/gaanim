# %% basic_circle
from gaanim import BLUE, GOLD, Scene

scene = Scene(1280, 720, title="Basic Circle")

circle = scene.circle(80).fill(BLUE).stroke(GOLD, 4).at(-100, 0)
rect = scene.rectangle(160, 100).fill(GOLD).at(100, 0)

scene.play(
    circle.animate().write(duration=2.0).smooth(),
    rect.animate().create(duration=1.5).linear(),
)

scene.wait(0.5)
scene.play(
    circle.animate().shift(200, 0).duration(1.0).spring(),
    rect.animate().fade_out().duration(0.5),
)

scene.export("basic_circle.webp", fps=30, quality="draft")

# %% text_demo
from gaanim import BLUE, CORAL, Scene, Theme

scene = Scene(1280, 720, theme=Theme.DARK)

title = scene.title("Gaanim Demo").at(0, 200)
eq = scene.equation("E = m c^2").at(0, 0).scale(1.5)
body = scene.body("Mass-energy equivalence").at(0, -120)

scene.play(
    title.animate().write(duration=1.5).smooth(),
    eq.animate().write(duration=2.5).linear(),
)

scene.wait(0.5)
scene.play(body.animate().fade_in().duration(1.0))

scene.wait(1.0)
scene.play(
    title.animate().unwrite(duration=1.0),
    eq.animate().fade_out().duration(0.5),
    body.animate().fade_out().duration(0.5),
)

scene.export("text_demo.webp", fps=30, quality="draft")

# %% shapes_gallery
from gaanim import BLUE, CORAL, GOLD, GREEN, PURPLE, RED, Scene, Theme

scene = Scene(1280, 720, theme=Theme.DARK)

circle = scene.circle(60).fill(BLUE).at(-300, 100)
rect = scene.rectangle(120, 80).fill(RED).at(-100, 100)
square = scene.square(90).fill(GREEN).at(100, 100)
ellipse = scene.ellipse(80, 50).fill(GOLD).at(300, 100)
star = scene.star(5, 60, 30).fill(CORAL).at(-200, -100)
triangle = scene.regular_polygon(3, 60).fill(PURPLE).at(0, -100)
hexagon = scene.regular_polygon(6, 60).fill(BLUE).at(200, -100)

scene.play(
    circle.animate().grow_from_center().duration(0.8).spring(),
    rect.animate().grow_from_center().duration(0.8).spring(),
    square.animate().grow_from_center().duration(0.8).spring(),
    ellipse.animate().grow_from_center().duration(0.8).spring(),
    star.animate().spin_in_from_nothing().duration(1.0).spring(),
    triangle.animate().grow_from_center().duration(0.8).spring(),
    hexagon.animate().grow_from_center().duration(0.8).spring(),
)

scene.wait(1.0)
scene.play(
    circle.animate().indicate(color=GOLD, scale_factor=1.3).duration(0.5),
    rect.animate().indicate(color=CORAL, scale_factor=1.3).duration(0.5),
    square.animate().indicate(color=RED, scale_factor=1.3).duration(0.5),
)

scene.wait(0.5)
scene.export("shapes_gallery.webp", fps=30, quality="draft")
