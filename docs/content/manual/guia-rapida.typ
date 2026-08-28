#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Guía rápida",
  description: "Construye y exporta una primera animación de movimiento circular",
  route: "/manual/guia-rapida/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Objetivo

Crearemos una escena pequeña con un círculo, un punto y un título. Al terminar
podrás previsualizarla y exportarla. Guarda el código como `main.py` dentro de
un proyecto creado con `gaanim init video mi-movimiento`.

== Crea la escena

```python
from gaanim import BLUE, WHITE, YELLOW, Scene, stagger

scene = Scene(frame=(16, 9), background="#0f172a")
```

El ancho y el alto definen el viewport. El color de fondo acepta una cadena
CSS o un objeto `Color`.

== Añade los objetos

```python
title = scene.text("Movimiento circular", role="title").fill(WHITE).move_to(0, 250)
orbit = scene.geometry.circle(140).stroke(BLUE, 4).no_fill()
point = scene.geometry.dot(12).fill(YELLOW).move_to(140, 0)
```

Cada fábrica devuelve un objeto fluido. `fill`, `stroke`, `no_fill` y `at`
modifican su especificación y devuelven el mismo objeto para seguir
encadenando llamadas.

== Programa la entrada

```python
scene.play(stagger(
    title.animate.write().duration(0.8),
    orbit.animate.create().duration(1.0),
    point.animate.fade_in().duration(0.4),
    each=0.12,
))
scene.wait(0.5)
```

Las animaciones de una lista comienzan como grupo. `stagger(..., each=...)`
escalona sus inicios; `duration` controla la duración individual.

== Previsualiza o exporta

Durante el trabajo usa:

```python
scene.render()
```

Ejecuta `gaanim .` desde la carpeta del proyecto. Para producir un video no
cambies el script; usa el ejecutable:

```powershell
gaanim export . --output exports/movimiento-circular.mp4 --quality standard
```

La exportación MP4 necesita FFmpeg. WebP animado es una alternativa útil para
previews de documentación.

== Qué aprendiste

Ya conoces el flujo completo: crear una escena, añadir objetos, describir
animaciones, controlar el tiempo y producir una salida. En
#link("/manual/escena/")[Partes de una escena] veremos qué responsabilidad tiene
cada pieza y cómo organizar escenas mayores.
