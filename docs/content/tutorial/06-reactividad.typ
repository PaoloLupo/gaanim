#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Dar vida a la escena",
  description: "Un parámetro animable que mueve el punto y arrastra al radio",
  route: "/tutorial/reactividad/",
)[

= Objetivo

Al terminar este capítulo, después de que se escriba el panel, el punto dará
una vuelta completa al círculo en sentido antihorario, a velocidad constante, y
el radio lo seguirá en todo momento.

No vamos a animar la posición del punto de un sitio a otro. Vamos a animar un
solo número, el ángulo `theta`, y a declarar que la posición del punto depende
de él. En el capítulo siguiente, la onda dependerá del mismo número.

= Cambios

== Un import de Python

Añade `import math` al principio del archivo: necesitaremos `math.pi`.

```python
import math

from gaanim import WHITE, YELLOW, Color, Easing, Scene, stagger
>>># Contexto mínimo para validar los fragmentos de esta página.
>>>PRIMARY = Color(96, 165, 250)
>>>ACCENT = YELLOW
>>>MUTED = Color(148, 163, 184)
>>>scene = Scene(frame=(16, 9))
>>>CENTER = (-4.5, -1.0)
>>>R = 1.5
```

== Un ángulo animable

Sustituye la zona `# Círculo` completa por esta versión:

```python
# continue
# Círculo
theta = scene.viz.parameter(0.0)
orbit = scene.geometry.circle(R).stroke(PRIMARY, 0.05).no_fill().move_to(*CENTER)
circle_ref = scene.geometry.polar_point(CENTER, R, theta)
point = scene.geometry.dot(0.125).fill(ACCENT).follow(circle_ref)
radius = scene.geometry.line(CENTER, point).stroke(MUTED, 0.025)
point.z_index(1)
```

Solo cambian tres cosas:

- `scene.viz.parameter(0.0)` crea `theta`, un `Parameter`: un número de la
  escena que empieza en `0` y que se puede animar como cualquier objeto.
- `scene.geometry.polar_point(CENTER, R, theta)` describe un punto a distancia
  `R` de `CENTER` con ángulo `theta`, en radianes, medido desde la derecha y en
  sentido antihorario. No dibuja nada: es una referencia que Gaanim recalcula
  cuando cambia `theta`.
- `point` ya no usa `move_to`, sino `follow(circle_ref)`: su centro queda pegado
  a esa referencia. Con `theta = 0`, el punto está donde estaba antes, en el
  borde derecho.

`radius` no cambia. En el capítulo 2 le dimos `point` como extremo, así que ya
sigue al punto allá donde vaya.

== Animar el ángulo

En la zona `# Línea de tiempo`, añade la vuelta completa antes de
`scene.wait(1)`:

```python
# continue
scene.play([theta.animate.set(2 * math.pi).duration(4).easing(Easing.LINEAR)])
```

`theta.animate.set(2 * math.pi)` lleva el ángulo de `0` a $2 pi$, una vuelta
completa, en 4 segundos. `scene.play` también acepta una lista de animaciones,
que empiezan a la vez; aquí solo hay una.

`Easing.LINEAR` hace que el ángulo avance siempre al mismo ritmo. Con el easing
por defecto, el punto arrancaría despacio y frenaría al final, y el movimiento
circular dejaría de ser uniforme.

#idea[
Anima el valor, no la forma. `theta` es la única fuente de verdad: el punto
depende de él y el radio depende del punto. Como todo se calcula a partir del
tiempo, puedes saltar a cualquier instante y la escena será exactamente la
misma que al reproducirla de corrido.
]

= Archivo completo

```python
# output: preview.webp
import math

from gaanim import WHITE, YELLOW, Color, Easing, Scene, stagger

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
scene.play([theta.animate.set(2 * math.pi).duration(4).easing(Easing.LINEAR)])
scene.wait(1)
scene.render()
```

#checkpoint[
La vista previa dura 8 segundos y la vuelta ocupa del segundo 3 al 7. Arrastra
la barra de reproducción del editor: en el segundo 4 el punto está arriba
($theta = pi\/2$), en el 5 a la izquierda, en el 6 abajo y en el 7 de vuelta
a la derecha. El radio une siempre el centro con el punto.
]

La reactividad tiene más herramientas: valores calculados, rastros y
updaters para simulaciones. Las encontrarás en la guía
#link("/guias/reactividad/")[Reactividad]. En el siguiente capítulo,
#link("/tutorial/circulo-al-seno/")[Del círculo a la onda], usaremos `theta`
para dibujar la curva seno.
]
