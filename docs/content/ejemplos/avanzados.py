# Escenas completas de /ejemplos/avanzados/. Cada celda `# %% nombre` es un
# script independiente: cópiala a un .py y ábrela con `gaanim archivo.py`.

# %% transforms
from gaanim import Easing, BLACK, BLUE, GOLD, GREEN, WHITE, Scene, Transition

scene = Scene(frame=(16, 9), background=BLACK)
scene.segment("shapes")
circle = scene.geometry.circle(1).fill(BLUE).stroke(WHITE, 0.05).move_to(-2.25, 0)
scene.play([circle.animate.create().duration(0.8)])

scene.segment("text", Transition.cross_fade(0.4))
headline = scene.text("Una transformación estable", role="title").fill(GOLD).move_to(0, 0)
scene.play([circle.animate.replacement_transform_to(headline).duration(1.4).easing(Easing.spring(stiffness=90.0, damping=12.0))])

formula = scene.text.equation("E = m c^2").fill(GREEN).move_to(0, -1.875)
scene.play([headline.animate.transform_to(formula).duration(1.4).easing(Easing.SMOOTH)])
scene.render()

# %% reactive_path
import math
from gaanim import CYAN, GOLD, WHITE, Easing, Scene, computed

scene = Scene(frame=(16, 9), background="#0f172a")
t = scene.viz.parameter(0.0)
size = 3.2
tip = scene.geometry.point_ref(
    computed(lambda a: size * math.cos(3 * a) * math.cos(a), inputs=[t]),
    computed(lambda a: size * math.cos(3 * a) * math.sin(a), inputs=[t]),
)
arm = scene.geometry.tracking_line((0, 0), tip).stroke(WHITE, 0.02)
pen = scene.geometry.dot(0.12).fill(GOLD).follow(tip)
rose = scene.geometry.traced_path(pen).stroke(CYAN, 0.05).no_fill()

scene.play([item.animate.fade_in().duration(0.3) for item in (arm, pen, rose)])
scene.play([t.animate.set(math.pi).duration(3.0).easing(Easing.LINEAR)])
scene.play([arm.animate.fade_out().duration(0.3), pen.animate.fade_out().duration(0.3)])
scene.render()

# %% curve_parameter
from math import cos, pi, sin
from gaanim import GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
t = scene.viz.parameter(0.0)
curve = scene.geometry.polyline([
    (3.25 * cos(u), 1.875 * sin(2 * u))
    for u in (2 * pi * i / 240 for i in range(241))
]).no_fill().stroke(WHITE, 0.04)
point = scene.geometry.point_on_curve(curve, t).fill(GOLD)
tangent = scene.geometry.tangent_on_curve(curve, t, length=1.375).stroke(GOLD, 0.05)

scene.play([curve.animate.create().duration(0.7), point.animate.fade_in().duration(0.2), tangent.animate.fade_in().duration(0.2)])
scene.play([t.animate.set(1.0).duration(3.0)])
scene.render()

# %% structured_text
from gaanim import Scene, part

scene = Scene(frame=(16, 9), background="#0f172a")
before = scene.text.equation("E =", part("mass", "m"), "c^2", size=1.2)
after = scene.text.equation("E =", part("mass", "(m_1 + m_2)"), "c^2", size=1.2)

scene.play([before.animate.write(by="part").duration(0.8)])
scene.wait(0.3)
scene.play([before.animate.transform_to(after).duration(1.2)])
scene.wait(0.3)
scene.render()

# %% chart_story
from gaanim import Axis, ChartSpec, Direction, Scene

data = {"method": ["CPU", "GPU", "Cached"], "ms": [48, 15, 9]}
spec = (
    ChartSpec(data, key="method")
    .mark("bar", width=0.68)
    .encode(x="method", y="ms")
    .axes(x=Axis.category(data["method"]), y=Axis.linear(0, 50).ticks(10))
)

scene = Scene(frame=(16, 9), theme="technical")
heading = scene.slides.section_header(
    "Tiempo de renderizado",
    kicker="PERFIL",
    subtitle="Menor es mejor",
    align="center",
    variant="accent",
)
chart = scene.viz.chart(spec)
page = scene.layout.column([heading, scene.layout.item(chart.drawable(), grow=1)], within="safe", gap=0.3)
scene.play([page.animate.fade_in().duration(0.7), chart.layer("marks").animate.grow_from_edge(Direction.DOWN).duration(0.8)])
scene.wait(0.5)
scene.render()

# %% staggered_grid
from gaanim import CYAN, GOLD, Scene, distribute, stagger

scene = Scene(frame=(16, 9), background="#0f172a")
dots = [
    scene.geometry.circle(0.22).fill(CYAN).move_to(x * 0.8, y * 0.8)
    for y in range(-3, 4)
    for x in range(-7, 8)
]
sizes = distribute(dots, 0.6, 1.2, origin="center")

scene.play(stagger(*[dot.animate.grow_from_center().duration(0.4) for dot in dots], total=1.0, origin="center"))
scene.play(stagger(
    *[dot.animate.fill(GOLD).scale_to(size).duration(0.4) for dot, size in zip(dots, sizes)],
    total=0.8,
    origin="edges",
))
scene.render()

# %% vertical_layout
from gaanim import Scene

scene = Scene(frame=(9, 16), theme="presentation")
scene.canvas.set_preset("vertical")

header = scene.slides.section_header(
    "Tres ideas clave",
    subtitle="La composición responde al formato",
    align="center",
)
body = scene.slides.bullets([
    "Los objetos conocen su medida",
    "Layout distribuye el espacio",
    "El tema mantiene la identidad visual",
])
footer = scene.text("gaanim · explicación visual", role="caption")
page = scene.layout.column(
    [header, scene.layout.item(body, grow=1), footer],
    within="safe",
    width="fill",
    height="fill",
    gap=0.4,
)
scene.play([page.animate.fade_in().duration(0.8)])
scene.wait(0.5)
scene.render()

# %% title_card
from gaanim import BLUE, Scene

scene = Scene(frame=(16, 9), theme="presentation")
opening = scene.slides.title_card(
    "El movimiento cuenta una idea",
    "Una presentación construida con Gaanim",
    accent=BLUE,
)
scene.play([opening.animate.fade_in().duration(0.7)])
scene.wait(0.8)
scene.play([opening.animate.fade_out().duration(0.4)])
scene.render()

# %% scene_3d
from gaanim import CYAN, GOLD, Material3D, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
scene.geometry.lighting_3d("studio", intensity=1.0, shadows=True)
cube = scene.geometry.cube(2.0, material=Material3D.matte(CYAN)).move_to_3d(-1.8, 0, 0)
sphere = scene.geometry.sphere(1.1, material=Material3D.metal(GOLD)).move_to_3d(1.8, 0, 0)

scene.camera.perspective(fov_y=0.7)
scene.camera.look_at(eye=(0, 3, 10), target=(0, 0, 0))
scene.play([cube.animate.create().duration(0.8), sphere.animate.create().duration(0.8)])
scene.play([scene.camera.animate.orbit(delta_yaw=0.8, delta_pitch=0.35).duration(1.5)])
scene.render()
