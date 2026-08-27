#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Objetos y estilo",
  description: "Cómo pensar en drawables, rellenos, trazos, grupos y jerarquía visual",
  route: "/guia/objetos-estilo/",
  updated: datetime.today().display(),
  code-langs: (),
)[

= Hacer que el fotograma comunique

Nuestro primer fotograma funciona, pero todos sus elementos compiten por igual.
Ahora definiremos una pequeña jerarquía visual y prepararemos el código para que
el sistema circular se pueda tratar como una unidad.

== Una paleta con propósito

Amplía los imports y define colores semánticos:

```python
from gaanim import BLUE, WHITE, YELLOW, Color, Scene

BACKGROUND = Color(15, 23, 42)
PRIMARY = BLUE
ACCENT = YELLOW
MUTED = Color(148, 163, 184)

scene = Scene(1280, 720, background=BACKGROUND, margin=48)
```

Los nombres dicen para qué sirve cada color. Si la identidad visual cambia,
solo modificamos la paleta.

== Relleno y trazo

Un drawable vectorial puede tener relleno, trazo o ambos. La órbita representa
una trayectoria, así que no necesita relleno. El punto necesita contraste y el
radio debe ser secundario:

```python
orbit = scene.geometry.circle(120).stroke(PRIMARY, 4).no_fill().move_to(-320, 0)
point = scene.geometry.dot(10).fill(ACCENT).move_to(-200, 0)
radius = scene.geometry.line(-320, 0, -200, 0).stroke(MUTED, 2)
```

El orden de creación también influye en la lectura cuando los objetos se
superponen: lo registrado después suele quedar visualmente por encima.

== Texto como objeto vectorial

`scene.text` no crea una etiqueta del sistema operativo; crea geometría
vectorial medible y animable. Los roles conectan el texto con la tipografía del
tema:

```python
title = scene.text("Movimiento circular", role="title")
title.fill(WHITE).move_to(0, 260)

caption = scene.text("Un punto, un radio constante", role="subtitle")
caption.fill(MUTED).move_to(0, 215)
```

== Agrupar sin perder las piezas

Agrupa la geometría del círculo:

```python
system = scene.geometry.group([orbit, radius, point])
```

`system` permite animar las tres piezas juntas. Los handles originales siguen
siendo útiles: podremos mover `point` y actualizar `radius` por separado.

#idea[
Un grupo expresa pertenencia visual. No lo uses solo para reducir líneas de
código. Agrupa objetos que el espectador debería reconocer como una unidad.
]

== Estado del proyecto

Hasta aquí, la parte central de `main.py` se lee así:

```python
title = scene.text("Movimiento circular", role="title").fill(WHITE).move_to(0, 260)
caption = scene.text("Un punto, un radio constante", role="subtitle").fill(MUTED).move_to(0, 215)

orbit = scene.geometry.circle(120).stroke(PRIMARY, 4).no_fill().move_to(-320, 0)
radius = scene.geometry.line(-320, 0, -200, 0).stroke(MUTED, 2)
point = scene.geometry.dot(10).fill(ACCENT).move_to(-200, 0)
system = scene.geometry.group([orbit, radius, point])

scene.render()
```

#checkpoint[
Comprueba que la órbita domina sobre el radio, que el punto es el foco y que el
subtítulo se lee como información secundaria. Todavía no debe moverse nada.
]

== Siguiente paso

Ya tenemos un fotograma diseñado. Ahora construiremos el tiempo: primero
aparecerá el texto, después se dibujará la órbita y finalmente entrará el punto.
]
