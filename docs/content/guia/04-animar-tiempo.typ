#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Animar el tiempo",
  description: "Timeline, entradas, paralelismo, duración y easing",
  route: "/guia/animar-tiempo/",
  updated: datetime.today().display(),
  code-langs: (),
)[

= De un fotograma a una secuencia

Hasta ahora `scene.render()` mostraba inmediatamente el estado construido. No
eliminaremos ese estado: añadiremos una timeline que explique cómo debe
revelarse.

Sustituye la última línea por:

```python
from gaanim import stagger
scene.play(stagger(
    title.animate.write().duration(0.8),
    caption.animate.fade_in().duration(0.6),
    each=0.12,
))

scene.play(stagger(
    orbit.animate.create().duration(1.0),
    radius.animate.create().duration(0.7),
    point.animate.fade_in().duration(0.35),
    each=0.15,
))

scene.wait(0.8)
scene.render()
```

== Qué devuelve una animación

`title.animate.write()` y `orbit.animate.create()` devuelven objetos `Anim`. Todavía no
modifican el tiempo. `scene.play([...])` programa esos descriptores en el cursor
actual de la timeline.

Las animaciones dentro de una misma lista forman un grupo. Las llamadas
distintas a `play` son secuenciales. `lag` retrasa progresivamente el inicio de
cada miembro del grupo.

== Duración y ritmo

Las duraciones están en segundos. No todo debe durar lo mismo. El título puede
necesitar tiempo para leerse, mientras que un punto pequeño puede entrar con un
fundido corto.

Para mover el sistema completo y devolverlo a su sitio:

```python
scene.play([system.animate.shift_by(40, 0).duration(0.6).easing(Easing.SMOOTH)])
scene.play([system.animate.shift_by(-40, 0).duration(0.6).easing(Easing.SMOOTH)])
```

`smooth` desacelera cerca de los extremos. Para una magnitud que representa
velocidad física constante usaremos `linear` más adelante.

#idea[
El easing también comunica. `smooth` se siente editorial; `linear` representa
mejor un movimiento uniforme; `spring` llama la atención y debe usarse con
intención.
]

== Entrada no significa construcción tardía

Todos los objetos se describen antes de la timeline. Las animaciones de entrada
controlan su aparición. Esto permite que Gaanim conozca la escena completa,
calcule layout y busque estados exactos en cualquier instante.

== Una primera narración

La secuencia ya cuenta algo:

1. El título presenta la idea.
2. La órbita define el espacio del movimiento.
3. El radio y el punto revelan la relación geométrica.
4. La pausa concede tiempo para observar.

#checkpoint[
Reproduce la escena varias veces. Ajusta una sola duración y observa cómo
cambia la lectura. El código debe terminar otra vez en `scene.render()`.
]

== Siguiente paso

Antes de poner el punto en movimiento añadiremos una explicación matemática y
aprenderemos cuándo usar Layout en lugar de coordenadas manuales.
]
