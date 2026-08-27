#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Ejemplos avanzados",
  description: "Recetas modernas de texto, datos, composición, reactividad, 3D y presentaciones",
  route: "/examples/advanced/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Taller de posibilidades

Estas recetas no forman una segunda referencia de la API. Cada una responde a
una pregunta de diseño y combina varias capacidades que normalmente se usan
juntas. Copia una receta, ejecútala y cambia una sola decisión cada vez.

== Transformaciones entre segmentos

```python
from gaanim import BLACK, BLUE, GOLD, GREEN, WHITE, Scene, Transition

scene = Scene(1280, 720, background=BLACK)
scene.segment("shapes")
circle = scene.geometry.circle(80).fill(BLUE).stroke(WHITE, 4).move_to(-180, 0)
scene.play([circle.animate.create().duration(0.8)])

scene.segment("text", Transition.cross_fade(0.4))
headline = scene.text("A stable transform", role="title").fill(GOLD)
scene.play([circle.animate.replacement_transform_to(headline).duration(1.4).spring()])

formula = scene.text.equation("E = m c^2").fill(GREEN).move_to(0, -150)
scene.play([headline.animate.transform_to(formula).duration(1.4).smooth()])
# Ejecuta este archivo con: gaanim transforms.py
```

== Grupos

```python
from gaanim import BLACK, BLUE, GREEN, RED, Scene

scene = Scene(1280, 720, background=BLACK)
left = scene.geometry.circle(40).fill(BLUE).move_to(-80, 0)
middle = scene.geometry.circle(40).fill(RED).move_to(0, 0)
right = scene.geometry.circle(40).fill(GREEN).move_to(80, 0)
group = scene.geometry.group([left, middle, right])

scene.play([group.animate.grow_from_center().duration(1.0).spring()])
scene.play([group.animate.shift_by(0, 120).duration(1.0), group.animate.rotate_by(3.14159).duration(1.0)])
# Ejecuta este archivo con: gaanim groups.py
```

== Trayectoria reactiva

```python
from gaanim import BLACK, Color, Scene, Updater

scene = Scene(1280, 720, background=BLACK)
dot = scene.geometry.dot(10).fill(Color(255, 180, 70)).move_to(200, 0)
dot.add_updater(Updater.orbit(0, 0, 200, 1.5))
trail = scene.geometry.traced_path(dot).stroke(Color(80, 220, 220), 3).no_fill()

scene.play([dot.animate.fade_in().duration(0.3), trail.animate.fade_in().duration(0.3)])
scene.wait(4.0)
dot.remove_updater()
# Ejecuta este archivo con: gaanim reactive_path.py
```

== Texto estructurado que conserva significado

Usa partes con nombre cuando una transformación deba relacionar conceptos y no
solo glifos parecidos.

```python
from gaanim import GOLD, Scene, part

scene = Scene(1280, 720, background="#0f172a")
before = scene.text.equation("E =", part("mass", "m"), "c^2")
after = scene.text.equation("E =", part("mass", "(m_1 + m_2)"), "c^2")
before["mass"].fill(GOLD)
after["mass"].fill(GOLD)

scene.play([before.animate.write(0.8, by="part")])
scene.play([before.animate.transform_to(after).duration(1.2)])
scene.render()
```

== Un gráfico como parte de una explicación

`ChartSpec` separa los datos de su representación. El gráfico resultante sigue
siendo un objeto normal: puede participar en Layout, temas y animaciones.

```python
from gaanim import Axis, ChartSpec, Scene

data = {"method": ["CPU", "GPU", "Cached"], "ms": [48, 15, 9]}
spec = (
    ChartSpec(data, key="method")
    .mark("bar", width=0.68)
    .encode(x="method", y="ms")
    .axes(x=Axis.category(data["method"]), y=Axis.linear(0, 50).ticks(10))
)

scene = Scene(1280, 720, theme="technical")
heading = scene.slides.section_header(
    "Tiempo de renderizado",
    kicker="PERFIL",
    subtitle="Menor es mejor",
    align="center",
    variant="accent",
)
chart = scene.viz.chart(spec)
page = scene.layout.column([heading, scene.layout.item(chart, grow=1)], within="safe", gap=24)
scene.play([page.animate.fade_in(0.7), chart.layer("marks").animate.grow_from_center(0.8)])
scene.render()
```

== Composición adaptable sin coordenadas manuales

Una escena editorial puede expresar jerarquía y espacio flexible sin calcular
posiciones a mano.

```python
from gaanim import BLUE, Scene

scene = Scene(1080, 1920, theme="presentation")
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
    gap=32,
)
scene.play([page.animate.fade_in(0.8)])
scene.render()
```

== Geometría conducida por un parámetro

`Parameter` permite buscar cualquier instante de la línea de tiempo sin
acumular error entre fotogramas.

```python
from gaanim import GOLD, WHITE, Scene
from math import cos, pi, sin

scene = Scene(1280, 720, background="#0f172a")
t = scene.viz.parameter(0.0)
curve = scene.geometry.polyline([
    (260 * cos(u), 150 * sin(2 * u))
    for u in (2 * pi * i / 240 for i in range(241))
]).no_fill().stroke(WHITE, 3)
point = scene.geometry.point_on_curve(curve, t).fill(GOLD)
tangent = scene.geometry.tangent_on_curve(curve, t, length=110).stroke(GOLD, 4)

scene.play([curve.animate.create(0.7), point.animate.fade_in(0.2), tangent.animate.fade_in(0.2)])
scene.play([t.animate.set(1.0).duration(4.0)])
scene.render()
```

== Una escena 3D con material y cámara

Empieza con iluminación y objetos inmóviles; anima la cámara cuando la
composición ya sea legible.

```python
from gaanim import BLUE, GOLD, Material3D, Scene

scene = Scene(1280, 720)
scene.geometry.lighting_3d("studio", intensity=1.0, shadows=True)
cube = scene.geometry.cube(2.0, material=Material3D.matte(BLUE)).move_to_3d(-1.8, 0, 0)
sphere = scene.geometry.sphere(
    1.1,
    material=Material3D.metal(GOLD),
).move_to_3d(1.8, 0, 0)

scene.play([cube.animate.create(0.8), sphere.animate.create(0.8)])
scene.play([scene.camera.animate.orbit(delta_yaw=0.8, delta_pitch=0.35).duration(1.5)])
scene.render()
```

== Una apertura reutilizable

Los componentes editoriales empaquetan decisiones de tipografía, alineación y
tema; no hace falta reconstruir una portada con textos sueltos.

```python
from gaanim import BLUE, Scene

scene = Scene(1920, 1080, theme="presentation")
opening = scene.slides.title_card(
    "El movimiento cuenta una idea",
    "Una presentación construida con Gaanim",
    accent=BLUE,
)
scene.play([opening.animate.fade_in(0.7)])
scene.wait(1.0)
scene.play([opening.animate.fade_out(0.4)])
scene.render()
```

== Cómo elegir la siguiente receta

- Si el problema es legibilidad, empieza por texto estructurado y Layout.
- Si el problema es explicar datos, usa espacios tipados o `ChartSpec`.
- Si una forma depende de otra, usa `Parameter`, bindings y geometría reactiva.
- Si el contenido es editorial, prefiere componentes temáticos antes que
  coordenadas sueltas.
- Si necesitas profundidad, valida primero material, luz y cámara por separado.
