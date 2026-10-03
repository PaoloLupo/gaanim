#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Personajes con SVG por partes",
  description: "Dibuja un personaje en Inkscape con un id por parte y anímalo con part, with_pivot y look_at",
  route: "/guias/personajes/",
)

= Dibujar en un editor, animar por partes

Un personaje ilustrado se dibuja mejor en un editor vectorial que con curvas
escritas a mano. En esta guía lo dibujas en Inkscape (vale cualquier editor que
guarde SVG), le das un `id` a cada parte que quieras mover y lo animas en
Gaanim: inclinas la cabeza desde el cuello, levantas las cejas, saludas con un
brazo y haces que el otro apunte a un objeto que se mueve.

== Prepara el SVG

1. Dibuja cada parte móvil por separado: cabeza, cejas, brazos, piernas. Agrupa
  (`Ctrl+G`) lo que se mueve junto, como la cara con sus ojos.
2. Dale a cada parte un `id` en *Objeto › Propiedades del objeto*
  (`Ctrl+Shift+O`): escribe el nombre en *ID* y pulsa *Establecer*. Usa nombres
  cortos sin espacios (`cabeza`, `brazo-der`); distinguen mayúsculas.
3. Dibuja las extremidades en reposo y extendidas a lo largo del eje x, desde la
  articulación hacia fuera. Así un giro de 0 es la pose de reposo y
  `look_at` apunta la extremidad hacia su objetivo.
4. Si una parte tiene que girar entera hacia un objetivo con `look_at`, haz que
  sea un solo trazado (*Trazo › Unión*): en un grupo, `look_at` gira cada
  miembro por separado.
5. Guarda como *SVG plano*. Anota en píxeles los puntos de giro: el cuello, los
  hombros, las caderas.

El personaje de esta guía (`assets/personaje.svg`, 400 × 300 px) tiene las
partes `cabeza`, `cejas`, `ojos`, `nariz`, `boca`, `torso`, `piernas`,
`brazo-izq` y `brazo-der`; cada brazo es un solo trazado con su mano.

== De píxeles del SVG a la escena

Gaanim importa el SVG a 100 píxeles por unidad, con el centro del documento en
el origen y la y hacia arriba. Un punto `(px, py)` de un documento de
`ancho × alto` píxeles, escalado por `ESCALA`, queda en

```python
<<< ((px - ancho / 2) / 100 * ESCALA, (alto / 2 - py) / 100 * ESCALA)
```

Usa `with_pivot(0, 0)` antes de `scale_to`, para que el SVG escale desde el
centro del documento y la fórmula siga valiendo. Con ella fijas el pivote de
cada parte en su articulación.

== Animar por partes

`part(id)` devuelve cada parte como un `Drawable` propio: se anima, se colorea y
se mueve sin mover el resto. `with_pivot` fija el punto alrededor del que gira:

```python
# show-code: true
from gaanim import GOLD, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
ESCALA = 2.0
persona = scene.media.svg("assets/personaje.svg").with_pivot(0, 0).scale_to(ESCALA)


def punto(px, py):
    """Un punto del SVG, en píxeles, en coordenadas de la escena."""
    return ((px - 200) / 100 * ESCALA, (150 - py) / 100 * ESCALA)


cabeza = persona.part("cabeza").with_pivot(*punto(200, 140))  # el cuello
cejas = persona.part("cejas")
brazo_izq = persona.part("brazo-izq").with_pivot(*punto(164, 159))  # hombro izquierdo
brazo_der = persona.part("brazo-der").with_pivot(*punto(236, 159))  # hombro derecho

scene.play([persona.animate.fade_in().duration(0.4)])
# Inclina la cabeza, levanta las cejas y saluda.
scene.play([
    cabeza.animate.rotate_by(0.15).duration(0.4),
    cejas.animate.shift_by(0, 0.08).duration(0.4),
    brazo_izq.animate.rotate_by(-0.8).duration(0.4),
])
scene.play([
    cabeza.animate.rotate_by(-0.15).duration(0.4),
    cejas.animate.shift_by(0, -0.08).duration(0.4),
    brazo_izq.animate.rotate_by(0.8).duration(0.4),
])
# El brazo derecho sigue a la estrella mientras se mueve.
estrella = scene.geometry.star(5, 0.3, 0.12).fill(GOLD).move_to(5, 3)
brazo_der.look_at(estrella)
scene.play([estrella.animate.move_to(5, -2.5).duration(1.2)])
scene.render()
# output: preview.webp
```

`look_at` sigue al objetivo desde el cursor hasta `clear_drive()`; con
`offset=` (radianes) sirve también para partes dibujadas apuntando a otro lado,
como un brazo izquierdo extendido hacia la izquierda (`offset=math.pi`).

== Trazos que crecen con la cara

Las cejas, la nariz y la boca de este personaje son trazos. Un trazo mide su
ancho en unidades de escena, así que al agrandar la cabeza quedarían más finos
que el resto. `scale_stroke_with_object()` hace que crezcan con ella:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>persona = scene.media.svg("assets/personaje.svg").with_pivot(0, 0).scale_to(2)
cabeza = persona.part("cabeza").with_pivot(0, 0.2).scale_stroke_with_object()
scene.play([cabeza.animate.scale_to(1.18).duration(0.5)])
```

== Referencia

- #link("/referencia/medios/")[Medios]: `scene.media.svg`, `part` y `parts`.
- #link("/referencia/drawable/")[Drawable]: `with_pivot`, `look_at`,
  `scale_stroke_with_object` y las animaciones de `.animate`.
- #link("/guias/movimiento/")[Movimiento]: trayectorias, updaters y temblores
  para dar vida a un personaje quieto.
- #link("/referencia/layout/")[Layout]: `scene.layout.scatter` reparte
  fórmulas o etiquetas alrededor del personaje sin tapar sus partes
  (`avoid=[persona.part("cabeza"), ...]`).
