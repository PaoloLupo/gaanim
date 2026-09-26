#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Del círculo a la onda",
  description: "Una recta numerada, la curva seno y una proyección gobernadas por el mismo ángulo",
  route: "/tutorial/circulo-al-seno/",
)[

= Objetivo

Este capítulo completa la explicación. A la derecha del círculo aparecerá una
recta numerada de $0$ a $2 pi$. Mientras el punto gira, un segundo punto
avanza por la recta a la misma altura que el primero, unido a él por una línea
de proyección, y deja dibujada la onda seno.

No añadiremos otro movimiento: la recta, la onda y el segundo punto dependen
del mismo `theta` que ya mueve el círculo. La animación de `theta` del capítulo
anterior no cambia.

= Cambios

== Nuevos imports

```python
import math

from gaanim import WHITE, YELLOW, Axis, Color, Easing, Scene, computed, stagger
>>># Contexto mínimo para validar los fragmentos de esta página.
>>>PRIMARY = Color(96, 165, 250)
>>>ACCENT = YELLOW
>>>MUTED = Color(148, 163, 184)
>>>scene = Scene(frame=(16, 9))
>>>CENTER = (-4.5, -1.0)
>>>R = 1.5
>>>theta = scene.viz.parameter(0.0)
>>>point = scene.geometry.dot(0.125).follow(scene.geometry.polar_point(CENTER, R, theta))
```

`Axis` describe una escala numérica y `computed` crea valores calculados a
partir de otros.

== Una recta numerada con π

Añade una zona nueva, `# Recta y onda`, después de la zona `# Círculo`:

```python
# continue
# Recta y onda
axis = (
    Axis.linear(0, 2 * math.pi)
    .ticks(math.pi)
    .numbers("pi", denominator=1)
    .style(color=MUTED, number_color=MUTED)
)
number_line = scene.viz.number_line(axis, length=8)
number_line.drawable().move_to(2, CENTER[1])
```

`Axis.linear(0, 2 * math.pi)` va de $0$ a $2 pi$, la vuelta completa. `ticks`
pone una marca en cada múltiplo de π y `numbers("pi", denominator=1)` las
rotula como `0`, `π` y `2π`, sin escribir los textos a mano. `style` usa el
color secundario de la paleta.

`scene.viz.number_line` convierte la escala en una recta de 8 unidades de
largo. La movemos a la altura del centro del círculo, de `x = -2` a `x = 6`.

== La onda

```python
# continue
wave = number_line.function(math.sin, normal_scale=R, reveal=theta)
wave.stroke(PRIMARY, 0.04).no_fill()
```

`number_line.function` dibuja una función perpendicular a la recta: para cada
valor de la recta, la curva se separa de ella `math.sin(valor)` por
`normal_scale`. Con `normal_scale=R`, la onda alcanza la misma altura máxima
que el punto sobre el círculo.

`reveal=theta` muestra la curva solo hasta el valor actual de `theta`. Con
`theta = 0` no se ve nada; a medida que el ángulo crece, la onda se dibuja. Por
eso no necesita animación de entrada.

== El punto de la onda y la proyección

```python
# continue
height = computed(lambda angle: R * math.sin(angle), inputs=[theta])
wave_dot = scene.geometry.dot(0.1).fill(ACCENT)
wave_dot.follow(number_line.point_ref(theta, normal_offset=height))
projection = scene.geometry.line(point, wave_dot).stroke(ACCENT, 0.025)
```

`computed` crea un valor que Gaanim recalcula cuando cambian sus entradas:
`height` vale siempre `R * sin(theta)`, la altura del punto respecto al centro
del círculo.

`number_line.point_ref(theta, normal_offset=height)` es el lugar de la recta que
corresponde a `theta`, desplazado `height` en perpendicular. `wave_dot` lo
sigue, igual que `point` sigue a `circle_ref`.

`projection` une los dos puntos. Como ambos están siempre a la misma altura,
la línea es siempre horizontal: esa es la prueba visual de que la onda es la
altura del punto.

== La entrada de la recta

En la zona `# Línea de tiempo`, añade una entrada justo antes de la animación
de `theta`:

```python
# continue
scene.play(stagger(
    number_line.animate.create().duration(0.8),
    projection.animate.create().duration(0.4),
    wave_dot.animate.grow_from_center().duration(0.5).easing(Easing.SNAPPY),
    each=0.25,
))
```

Es la misma estructura que la entrada del círculo: primero la recta, después
la línea y al final el punto con rebote. La vuelta de `theta` empieza ahora en
el segundo 4.

#idea[
Un solo parámetro evita la deriva. Si el punto y la onda tuvieran cada uno su
propia velocidad, bastaría un pequeño error para que dejaran de coincidir. Aquí
el círculo, la onda y la proyección se calculan del mismo `theta`, así que
coinciden en cualquier instante.
]

= Archivo completo

```python
# output: preview.webp
# timeout: 300
import math

from gaanim import WHITE, YELLOW, Axis, Color, Easing, Scene, computed, stagger

# Paleta
BACKGROUND = Color(15, 23, 42)
PRIMARY = Color(96, 165, 250)
ACCENT = YELLOW
MUTED = Color(148, 163, 184)

scene = Scene(frame=(16, 9), background=BACKGROUND, margin=0.6)

CENTER = (-4.5, -1.0)
R = 1.5

# Texto
title = scene.text("Del círculo al seno", role="title")
title.fill(WHITE).move_to(0, 3.4)
caption = scene.text("Un punto que gira a radio constante", role="subtitle")
caption.fill(MUTED).move_to(0, 2.8)

formula = scene.text("$y = r sin(theta)$", role="subtitle").fill(WHITE)
explanation = scene.text("La altura del punto dibuja la onda.", role="body")
explanation.fill(MUTED)
panel = scene.layout.column([formula, explanation], gap=0.2, width=6.5)
panel.move_to(3.5, 1.6)

# Círculo
theta = scene.viz.parameter(0.0)
orbit = scene.geometry.circle(R).stroke(PRIMARY, 0.05).no_fill().move_to(*CENTER)
circle_ref = scene.geometry.polar_point(CENTER, R, theta)
point = scene.geometry.dot(0.125).fill(ACCENT).follow(circle_ref)
radius = scene.geometry.line(CENTER, point).stroke(MUTED, 0.025)
point.z_index(1)

# Recta y onda
axis = (
    Axis.linear(0, 2 * math.pi)
    .ticks(math.pi)
    .numbers("pi", denominator=1)
    .style(color=MUTED, number_color=MUTED)
)
number_line = scene.viz.number_line(axis, length=8)
number_line.drawable().move_to(2, CENTER[1])

wave = number_line.function(math.sin, normal_scale=R, reveal=theta)
wave.stroke(PRIMARY, 0.04).no_fill()

height = computed(lambda angle: R * math.sin(angle), inputs=[theta])
wave_dot = scene.geometry.dot(0.1).fill(ACCENT)
wave_dot.follow(number_line.point_ref(theta, normal_offset=height))
projection = scene.geometry.line(point, wave_dot).stroke(ACCENT, 0.025)

# Línea de tiempo
scene.play(stagger(
    title.animate.write().duration(0.8),
    caption.animate.fade_in().duration(0.5),
    each=0.5,
))
scene.play(stagger(
    orbit.animate.create().duration(0.8),
    radius.animate.create().duration(0.4),
    point.animate.grow_from_center().duration(0.5).easing(Easing.SNAPPY),
    each=0.25,
))
scene.play(stagger(
    formula.animate.write().duration(0.8),
    explanation.animate.write().duration(0.8),
    each=0.2,
))
scene.play(stagger(
    number_line.animate.create().duration(0.8),
    projection.animate.create().duration(0.4),
    wave_dot.animate.grow_from_center().duration(0.5).easing(Easing.SNAPPY),
    each=0.25,
))
scene.play([theta.animate.set(2 * math.pi).duration(4).easing(Easing.LINEAR)])
scene.wait(1)
scene.render()
```

#checkpoint[
La vista previa dura 9 segundos. Entre el segundo 3 y el 4 aparecen la recta,
la línea de proyección y el punto de la onda; la vuelta ocupa del 4 al 8. Usa
la barra de reproducción del editor para comprobar estos instantes:

- En el segundo 5 ($theta = pi\/2$), el punto está arriba del círculo y el de
  la onda en su cresta, a mitad de camino entre `0` y `π`.
- En el 6 ($theta = pi$), los dos puntos están a la altura de la recta y el de
  la onda coincide con la marca `π`.
- En el 7 ($theta = 3 pi\/2$), ambos están en su punto más bajo.

En todos los instantes, la proyección es horizontal.
]

En el último capítulo, #link("/tutorial/terminar-proyecto/")[Terminar el
proyecto], cerraremos la animación, la revisaremos y la exportaremos.
]
