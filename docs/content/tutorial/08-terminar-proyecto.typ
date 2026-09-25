#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Terminar el proyecto",
  description: "Organizar el archivo, validar, exportar y saber qué aprender después",
  route: "/tutorial/terminar-proyecto/",
)[

= De experimento a proyecto

Una escena está terminada cuando otra persona puede abrirla, entender su
estructura y producir el mismo resultado. El último paso no es añadir más
efectos: es hacer explícitas las decisiones.

== Organizar `main.py`

Divide el archivo en cinco zonas reconocibles:

```python
# 1. Imports y paleta
# 2. Scene y configuración
# 3. Objetos estáticos
# 4. Relaciones reactivas
# 5. Timeline y salida
```

No escondas cada línea en una función. Crea funciones cuando nombren una idea
reutilizable, como `build_axes()` o `build_reactive_projection()`, y conserva la
timeline principal visible de arriba abajo.

== Elegir una salida

Durante la edición termina con:

```python
>>>from gaanim import BLUE, WHITE, YELLOW, Color, Scene
>>>from gaanim import Easing, stagger
>>>BACKGROUND = Color(15, 23, 42)
>>>PRIMARY = BLUE
>>>ACCENT = YELLOW
>>>MUTED = Color(148, 163, 184)
>>>scene = Scene(frame=(16, 9), background=BACKGROUND, margin=0.6)
>>>title = scene.text("Movimiento circular", role="title").fill(WHITE).move_to(0, 3.25)
>>>caption = scene.text("Un punto, un radio constante", role="subtitle").fill(MUTED).move_to(0, 2.69)
>>>orbit = scene.geometry.circle(1.5).stroke(PRIMARY, 0.05).no_fill().move_to(-4, 0)
>>>system = scene.geometry.group([orbit])
>>>formula = scene.text("$y(t) = r sin(omega t)$", role="subtitle").fill(WHITE)
>>>explanation = scene.text("La altura del punto se convertirá en una curva.", role="body").fill(MUTED)
>>>panel = scene.layout.column([formula, explanation], gap=0.225, align="start")
>>>panel.move_to(3.5, 1.875)
>>>scene.play(stagger(
>>>    title.animate.write().duration(0.8),
>>>    caption.animate.fade_in().duration(0.6),
>>>    each=0.12,
>>>))
>>>scene.play(stagger(
>>>    orbit.animate.create().duration(1.0),
>>>    each=0.15,
>>>))
>>>scene.play(stagger(
>>>    formula.animate.write().duration(0.8),
>>>    explanation.animate.fade_in().duration(0.6),
>>>    each=0.1,
>>>))
>>>scene.wait(0.8)
>>>scene.wait(0.8)
>>>scene.play([system.animate.shift_by(0.5, 0).duration(0.6).easing(Easing.SMOOTH)])
>>>scene.play([system.animate.shift_by(-0.5, 0).duration(0.6).easing(Easing.SMOOTH)])
>>>circle_center = (-4, 0)
>>>import math
>>>from gaanim import Axis
>>>axis = (
>>>  Axis.linear(0, 3 * math.pi)
>>>  .ticks(math.pi)
>>>  .numbers("pi", denominator=1)
>>>)
>>>timeline = scene.viz.number_line(axis, length=7.5)
>>>timeline.drawable().move_to(2.25, 0)
>>>import math
>>>from gaanim import computed
>>>radius = 1.5
>>>theta = scene.viz.parameter(0.0)
>>>sine_curve = timeline.function(
>>>  lambda value: math.sin(value),
>>>  normal_scale=radius,
>>>  reveal=theta,
>>>)
>>>sine_curve.stroke(PRIMARY, 0.04).no_fill()
>>>circle_ref = scene.geometry.polar_point(circle_center, radius, theta)
>>>circle_dot = scene.geometry.dot(0.125).fill(ACCENT).follow(circle_ref)
>>>wave_ref = timeline.point_ref(
>>>  theta,
>>>  normal_offset=computed(lambda angle: radius * math.sin(angle), inputs=[theta]),
>>>)
>>>wave_dot = scene.geometry.dot(0.1).fill(ACCENT).follow(wave_ref)
>>>radius_line = scene.geometry.tracking_line(circle_center, circle_ref)
>>>radius_line.stroke(MUTED, 0.025).no_fill()
>>>projection = scene.geometry.tracking_line(circle_ref, wave_ref)
>>>projection.stroke(ACCENT, 0.025).no_fill()
>>>scene.play([
>>>  circle_dot.animate.fade_in().duration(0.3),
>>>  radius_line.animate.fade_in().duration(0.3),
>>>  timeline.animate.create().duration(0.8),
>>>  sine_curve.animate.fade_in().duration(0.01),
>>>  wave_dot.animate.fade_in().duration(0.3),
>>>  projection.animate.fade_in().duration(0.3),
>>>])
>>>scene.play([
>>>  theta.animate.set(3 * math.pi).duration(8).easing(Easing.LINEAR),
>>>])
scene.render()
```

Para un video final no cambies el script; exporta desde la raíz del proyecto:

```bash
gaanim export . --output exports/movimiento-circular.mp4
```

MP4 y WebM necesitan FFmpeg. Para revisar rápidamente una animación en una web
o documento, WebP suele ser más liviano.

== Capturas reproducibles

El ejemplo final acepta el directorio que inyecta el comparador visual:

```python
# continue
import os

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    end = scene.cursor  # duración total de la timeline
    scene.snapshots(snapshot_dir, [0.0, end * 0.25, end * 0.5, end * 0.75, end])
else:
    scene.render()
```

La misma escena puede servir al editor y a pruebas visuales sin mantener dos
archivos distintos.

== Comprobar antes de exportar

Desde la raíz del proyecto:

```bash
gaanim check .
gaanim .
```

Revisa la escena completa y también instantes intermedios. Busca texto fuera
del área segura, objetos que aparezcan antes de su entrada, relaciones que se
rompan al hacer seek y pausas demasiado cortas para leer.

#idea[
Una buena animación no es la que contiene más métodos de la API. Es la que
mantiene una relación clara entre lo que se ve, el orden en que se revela y la
idea que debe comprender el espectador.
]

== Qué aprendiste realmente

El proyecto recorrió las capas fundamentales de Gaanim:

- `Scene` definió el espacio y la timeline.
- Los drawables expresaron geometría, texto y estilo.
- `play`, `wait`, duración y easing construyeron el ritmo.
- Layout organizó contenido que depende de su medida.
- Los updaters describieron movimiento continuo.
- `tracking_line`, `polar_point` y `follow` conectaron objetos en el mismo frame.
- Un `Parameter` compartido y `NumberLine.function(..., reveal=theta)`
  convirtieron el ángulo en una curva que se forma sin deriva.
- Render, export y snapshots produjeron salidas distintas desde la misma escena.

== Cómo continuar

No leas la referencia API de principio a fin. Úsala cuando tengas una pregunta
concreta. Para ampliar este proyecto, prueba en este orden:

1. Cambia el radio y la velocidad angular.
2. Añade una segunda curva con otra fase.
3. Sustituye colores locales por un `Theme`.
4. Adapta la composición a un viewport 9:16.
5. Convierte la explicación en segmentos de una presentación.

#checkpoint[
`gaanim check .` debe terminar sin errores y `gaanim .` debe reproducir
`main.py` completo. Conserva una copia antes de experimentar con extensiones.
]

== Después de la guía

La parte siguiente abre un taller de escenas con recetas de texto estructurado,
datos, Layout, reactividad, 3D, proyectos, presentaciones y regresión visual.
La referencia técnica de la API queda reservada para el apéndice final.
]
