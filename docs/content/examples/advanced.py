# %% equation_select
from gaanim import CORAL, GOLD, Scene, Theme

scene = Scene(1280, 720, theme=Theme.DARK)

eq = scene.equation("sum_(i=1)^n i = frac(n(n+1), 2)").at(0, 0)
scene.play(eq.animate().write(duration=3.0).smooth())
scene.wait(0.5)

# Select and highlight parts
lhs = scene.select(eq, "sum_(i=1)^n i")
scene.fill_selection(lhs, CORAL)
scene.wait(0.3)

rhs = scene.select(eq, "n(n+1)")
scene.fill_selection(rhs, GOLD)

scene.wait(1.0)

# Animate selected glyphs upward
sel_anim = scene.selection_anim(rhs, dx=0.0, dy=40.0)
sel_anim.duration(1.0).spring()
scene.play(sel_anim.build(scene))

scene.wait(0.5)
scene.export("equation_select.webp", fps=30, quality="draft")

# %% theme_switch
from gaanim import BLUE, GOLD, RED, Scene, Theme

scene = Scene(1280, 720, theme=Theme.DRACULA)

title = scene.title("Theme Demo").at(0, 200)
circle = scene.circle(80).fill(BLUE).at(-150, -50)
rect = scene.rectangle(160, 100).fill(RED).at(150, -50)

scene.play(
    title.animate().write(duration=1.0),
    circle.animate().grow_from_center().duration(1.0).spring(),
    rect.animate().grow_from_center().duration(1.0).spring(),
)

scene.wait(0.5)

# Switch to Gruvbox
scene.set_theme(Theme.GRUVBOX)
scene.play(
    circle.animate().fill_color(GOLD).duration(1.0),
    rect.animate().fill_color(RED).duration(1.0),
)

scene.wait(0.5)
scene.export("theme_switch.webp", fps=30, quality="draft")

# %% group_demo
from gaanim import BLUE, GREEN, RED, Scene, Theme

scene = Scene(1280, 720, theme=Theme.DARK)

c1 = scene.circle(40).fill(BLUE).at(-80, 0)
c2 = scene.circle(40).fill(RED).at(0, 0)
c3 = scene.circle(40).fill(GREEN).at(80, 0)

group = scene.group([c1, c2, c3]).at(0, 0)

scene.play(group.animate().grow_from_center().duration(1.0).spring())
scene.wait(0.5)

# Animate the whole group
scene.play(group.animate().shift(0, 100).duration(1.0).spring())
scene.play(group.animate().rotate(3.14).duration(1.5).smooth())

scene.wait(0.5)

# Animate individual children within the group
scene.play(
    c1.animate().shift(-60, 0).duration(0.8).spring(),
    c3.animate().shift(60, 0).duration(0.8).spring(),
)

scene.wait(0.5)
scene.export("group_demo.webp", fps=30, quality="draft")

# %% path_trace
from gaanim import CYAN, ORANGE, Scene, Theme

scene = Scene(1280, 720, theme=Theme.DARK)

dot = scene.circle(12).fill(ORANGE).at(200, 0)
dot.add_orbit_updater(scene, cx=0.0, cy=0.0, radius=200.0, speed=1.5)

trail = scene.traced_path(dot, color=CYAN, width=3.0, min_distance=2.0, max_points=200)

scene.play(dot.animate().fade_in().duration(0.3))
scene.wait(4.0)

scene.export("path_trace.webp", fps=30, quality="draft")
