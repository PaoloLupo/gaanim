#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Primera escena",
  description: "El lienzo, las coordenadas y el primer fotograma del proyecto",
  route: "/guia/primera-escena/",
  code-langs: (),
)[

= Del lienzo vacío al círculo unitario

Abre `main.py` y reemplaza su contenido. Nuestro primer objetivo es deliberadamente
modesto: un círculo, un punto y un título. Todavía no habrá movimiento.

```python
from gaanim import BLUE, WHITE, YELLOW, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.6)

title = scene.text("Movimiento circular", role="title")
title.fill(WHITE).move_to(0, 3.25)

orbit = scene.geometry.circle(1.5)
orbit.stroke(BLUE, 0.05).no_fill().move_to(-4, 0)

point = scene.geometry.dot(0.125)
point.fill(YELLOW).move_to(-2.5, 0)

scene.render()
```

Guarda el archivo y vuelve a la ventana de Gaanim.

== El viewport

`Scene(frame=(16, 9), ...)` crea un lienzo horizontal de 16 unidades de ancho y
9 de alto. El origen `(0, 0)` está en el centro, X crece hacia la derecha e Y
hacia arriba, así que X va de `-8` a `8` e Y de `-4.5` a `4.5`. Por eso el
título usa `y=3.25`, cerca del borde superior, y el sistema circular aparece a
la izquierda con `x=-4`.

Estas unidades son lógicas: no dependen de la resolución. La misma escena se
exporta a 1280×720 o a 3840×2160 sin cambiar el código; los píxeles se eligen al
exportar. El radio `1.5` ocupa 1.5 unidades desde el centro del círculo hasta su
borde, y los grosores de trazo y los tamaños de texto usan la misma unidad.

== Handles y estado inicial

Las variables `title`, `orbit` y `point` no contienen coordenadas ni imágenes.
Son handles con los que seguimos describiendo un objeto registrado en `scene`.

Separamos algunas llamadas en dos líneas para ver la intención:

```python
orbit = scene.geometry.circle(1.5)
orbit.stroke(BLUE, 0.05).no_fill().move_to(-4, 0)
```

La misma construcción podría escribirse como una sola cadena. Ambas formas
producen el mismo estado inicial.

#idea[
Usa nombres que expliquen el papel del objeto, no su forma. `orbit` comunica
más que `blue_circle`; después podremos cambiar su color sin volver falso el
nombre de la variable.
]

== Relación geométrica

El centro de la órbita es `(-4, 0)` y su radio es `1.5`. El punto inicial se
coloca en `(-2.5, 0)`: exactamente 1.5 unidades a la derecha. Esta relación
será importante cuando el punto empiece a girar.

Podemos hacer visible el radio:

```python
radius = scene.geometry.line(-4, 0, -2.5, 0).stroke(WHITE, 0.025)
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
