# %% transforms
from gaanim import BLACK, BLUE, GOLD, GREEN, WHITE, Scene, Transition

scene = Scene(1280, 720, background=BLACK)
scene.segment("shapes")
circle = scene.geometry.circle(80).fill(BLUE).stroke(WHITE, 4).move_to(-180, 0)
scene.play([circle.animate.create().duration(0.8)])

scene.segment("text", Transition.cross_fade(0.4))
headline = scene.text("A stable transform", role="title").fill(GOLD).move_to(0, 0)
scene.play([circle.animate.replacement_transform_to(headline).duration(1.4).spring()])

formula = scene.text("$E = m c^2$").fill(GREEN).move_to(0, -150)
scene.play([headline.animate.transform_to(formula).duration(1.4).smooth()])
scene.render()

# %% groups
from gaanim import BLACK, BLUE, GREEN, RED, Scene

scene = Scene(1280, 720, background=BLACK)
left = scene.geometry.circle(40).fill(BLUE).move_to(-80, 0)
middle = scene.geometry.circle(40).fill(RED).move_to(0, 0)
right = scene.geometry.circle(40).fill(GREEN).move_to(80, 0)
group = scene.geometry.group([left, middle, right])

scene.play([group.animate.grow_from_center().duration(1.0).spring()])
scene.play([group.animate.shift_by(0, 120).duration(1.0), group.animate.rotate_by(3.14159).duration(1.0)])
scene.render()

# %% reactive_path
from gaanim import BLACK, Color, Scene, Updater

scene = Scene(1280, 720, background=BLACK)
dot = scene.geometry.dot(10).fill(Color(255, 180, 70)).move_to(200, 0)
dot.add_updater(Updater.orbit(0, 0, 200, 1.5))
trail = scene.geometry.traced_path(dot).stroke(Color(80, 220, 220), 3).no_fill()

scene.play([dot.animate.fade_in().duration(0.3), trail.animate.fade_in().duration(0.3)])
scene.wait(4.0)
dot.remove_updater()
scene.render()
