#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Primera escena",
  description: "El lienzo, las coordenadas y el primer fotograma del proyecto",
  route: "/guia/primera-escena/",
  updated: datetime.today().display(),
  code-langs: (),
)[

= Del lienzo vacío al círculo unitario

Abre `main.py` y reemplaza su contenido. Nuestro primer objetivo es deliberadamente
modesto: un círculo, un punto y un título. Todavía no habrá movimiento.

```python
from gaanim import BLUE, WHITE, YELLOW, Scene

scene = Scene(1280, 720, background="#0f172a", margin=48)

title = scene.text("Movimiento circular", role="title")
title.fill(WHITE).at(0, 260)

orbit = scene.geometry.circle(120)
orbit.stroke(BLUE, 4).no_fill().at(-320, 0)

point = scene.geometry.dot(10)
point.fill(YELLOW).at(-200, 0)

scene.render()
```

Guarda el archivo y vuelve a la ventana de Gaanim.

== El viewport

`Scene(1280, 720, ...)` crea un lienzo horizontal. El origen `(0, 0)` está en
el centro. X crece hacia la derecha e Y hacia arriba. Por eso el título usa
`y=260` y el sistema circular aparece a la izquierda con `x=-320`.

Gaanim trabaja en unidades del lienzo, que en este caso se pueden interpretar
como píxeles. El radio `120` ocupa 120 unidades desde el centro del círculo
hasta su borde.

== Handles y estado inicial

Las variables `title`, `orbit` y `point` no contienen píxeles. Son handles con
los que seguimos describiendo un objeto registrado en `scene`.

Separamos algunas llamadas en dos líneas para ver la intención:

```python
orbit = scene.geometry.circle(120)
orbit.stroke(BLUE, 4).no_fill().at(-320, 0)
```

La misma construcción podría escribirse como una sola cadena. Ambas formas
producen el mismo estado inicial.

#idea[
Usa nombres que expliquen el papel del objeto, no su forma. `orbit` comunica
más que `blue_circle`; después podremos cambiar su color sin volver falso el
nombre de la variable.
]

== Relación geométrica

El centro de la órbita es `(-320, 0)` y su radio es `120`. El punto inicial se
coloca en `(-200, 0)`: exactamente 120 unidades a la derecha. Esta relación
será importante cuando el punto empiece a girar.

Podemos hacer visible el radio:

```python
radius = scene.geometry.line(-320, 0, -200, 0).stroke(WHITE, 2)
```

Añade esa línea antes de `scene.render()`.

#checkpoint[
La escena debe mostrar el título, un círculo a la izquierda, un punto amarillo
en su borde derecho y una línea blanca desde el centro hasta el punto.
]

== Lo que acabamos de aprender

Ya sabes crear una escena, leer su sistema de coordenadas, conservar handles y
construir una relación geométrica con medidas coherentes. En el siguiente
capítulo convertiremos este boceto en una composición visual consistente.
]
