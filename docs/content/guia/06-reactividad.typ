#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Dar vida a la escena",
  description: "Updaters, geometría reactiva y movimiento circular uniforme",
  route: "/guia/reactividad/",
  updated: datetime.today().display(),
  code-langs: (),
)[

= El punto deja de ser una posición fija

Una animación tradicional conoce un estado inicial y uno final. Nuestro punto
debe recorrer una órbita continuamente. Para eso usaremos un `Updater`.

Amplía los imports:

```python
from gaanim import BLUE, WHITE, YELLOW, Color, Scene, Updater
```

Después de crear `point`, registra el movimiento:

```python
point.add_updater(
    Updater.orbit(cx=-320, cy=0, radius=120, speed=1.2)
)
```

`speed` es velocidad angular en radianes por segundo. Con `1.2`, una vuelta
completa tarda aproximadamente `2*pi/1.2` segundos.

== El radio también debe reaccionar

La línea estática del capítulo anterior ya no sirve: su extremo se quedaría en
la posición inicial. Sustituye su creación por:

```python
radius = scene.geometry.tracking_line((-320, 0), point)
radius.stroke(MUTED, 2).no_fill()
```

`tracking_line` resuelve sus extremos en el mismo frame. El origen permanece
fijo y el otro extremo sigue el handle `point`.

Como la geometría reactiva comienza oculta, revélala dentro de la timeline:

```python
scene.play([
    point.animate.fade_in().duration(0.3),
    radius.animate.fade_in().duration(0.3),
])
scene.wait(5.3)
point.remove_updater()
```

La espera es ahora parte activa de la escena: mientras el cursor avanza, el
updater cambia la posición en cada frame. `remove_updater()` congela el punto al
terminar la demostración.

#idea[
Un updater describe una regla, no una secuencia de posiciones. Gaanim puede
evaluar esa regla al reproducir, exportar o buscar un instante exacto.
]

== Separar construcción y comportamiento

El código se entiende mejor si conserva este orden:

1. Crea `point` y `radius`.
2. Registra la relación reactiva.
3. Programa su aparición.
4. Deja avanzar el tiempo.
5. Retira el updater cuando ya no sea necesario.

== Errores frecuentes

- Si el punto gira alrededor del lugar incorrecto, revisa `cx` y `cy`.
- Si no toca la órbita, el radio del updater y el del círculo no coinciden.
- Si la línea no aparece, falta su `fade_in` dentro de `scene.play`.
- Si el movimiento cambia entre seeks, evita callbacks con estado no
  determinista; los presets nativos son reproducibles.

#checkpoint[
El punto debe completar aproximadamente una vuelta y el radio debe conservar
siempre un extremo en el centro del círculo. Pausar el editor en distintos
instantes no debe romper la relación.
]

== Siguiente paso

Ya tenemos movimiento circular. Ahora proyectaremos la altura del punto sobre
un eje horizontal y conservaremos el recorrido de esa proyección.
]
