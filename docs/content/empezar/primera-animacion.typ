#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Tu primera animación",
  description: "Construye un punto que recorre un círculo, previsualízalo y expórtalo a video",
  route: "/empezar/primera-animacion/",
  nav: "Primera animación",
)

= Qué vas a construir

Un punto amarillo que da vueltas sobre un círculo, con un título que se escribe
mientras gira. Son unas diez líneas de Python y aprenderás el ciclo completo:
escribir la escena, verla y exportarla a video.

Necesitas Gaanim instalado (ver #link("/empezar/instalacion/")[Instalación]).
Crea un proyecto y abre su `main.py` en tu editor:

```bash
gaanim init video mi-orbita
```

Borra el contenido de `main.py`; lo escribiremos por partes.

= Paso 1: la escena y los objetos

```python
from gaanim import BLUE, WHITE, YELLOW, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")

orbit = scene.geometry.circle(2).stroke(BLUE, 0.05).no_fill()
point = scene.geometry.dot(0.15).fill(YELLOW).move_to(2, 0)

scene.render()
```

- `Scene(frame=(16, 9), ...)` crea un lienzo de 16 × 9 unidades con el origen
  en el centro: X va de `-8` a `8` e Y de `-4.5` a `4.5`, hacia arriba. Son
  unidades lógicas, no píxeles; la resolución se elige al exportar.
- `scene.geometry.circle(2)` crea un círculo de radio 2 centrado en el origen, y
  `scene.geometry.dot(0.15)` un punto pequeño. Cada fábrica devuelve un
  _handle_, una referencia al objeto con la que seguimos describiéndolo.
- `stroke`, `no_fill`, `fill` y `move_to` se encadenan: cada uno cambia el
  objeto y devuelve el handle. El punto empieza en `(2, 0)`, justo sobre el
  borde derecho del círculo.
- `scene.render()` va siempre al final. Entrega la escena a Gaanim, que decide
  si la muestra o la exporta.

Todavía no hay movimiento: la escena dura cero segundos.

= Paso 2: haz que el punto gire

Añade una línea antes de `scene.render()`:

```python
# continue
scene.play(point.animate.move_along(orbit).duration(2).easing(Easing.LINEAR))
scene.render()
# output: preview.webp
```

- `point.animate` abre la descripción de una animación del punto;
  `move_along(orbit)` le pide recorrer el contorno del círculo.
- `duration(2)` fija la vuelta en 2 segundos, y `easing(Easing.LINEAR)`
  mantiene la velocidad constante. Sin él, el punto arrancaría y frenaría
  suavemente.
- Nada se mueve hasta que `scene.play(...)` coloca esa animación en la línea
  de tiempo. La escena ahora dura 2 segundos.

El círculo y el punto no tienen animación de entrada, así que están visibles
desde el primer fotograma.

= Paso 3: un título y otra vuelta

```python
# continue
title = scene.text("Movimiento circular", role="title").fill(WHITE).move_to(0, 3.25)

scene.play([
    title.animate.write().duration(1),
    point.animate.move_along(orbit).duration(2).easing(Easing.LINEAR),
])
scene.wait(0.5)
scene.render()
# output: preview.webp
```

- `scene.text(..., role="title")` crea un texto con el estilo de título.
  Como su entrada es `write()`, permanece oculto hasta que empieza a escribirse.
- Las animaciones de una misma lista empiezan a la vez: el título se escribe
  mientras el punto da la segunda vuelta. Cada `play` empieza cuando termina el
  anterior y dura lo que su animación más larga, aquí 2 segundos.
- `scene.wait(0.5)` deja medio segundo quieto el último fotograma.

= Previsualiza

Guarda el archivo y abre la vista previa desde la carpeta del proyecto:

```bash
cd mi-orbita
gaanim .
```

Se abre la ventana de Gaanim con la línea de tiempo. Deja la ventana abierta
mientras editas: cada vez que guardas `main.py`, la escena se recarga y
conserva el instante en que estabas. Prueba a cambiar `duration(2)` por
`duration(1)` y guarda.

Para comprobar el script sin abrir ventana, por ejemplo antes de exportar, usa
`gaanim check .`: ejecuta la escena e informa de errores y avisos.

= Exporta

Para exportar no cambias el script; se lo pides al ejecutable:

```bash
gaanim export . --output exports/orbita.mp4
```

La extensión de `--output` elige el formato:

#table(
  columns: 3,
  table.header[*Extensión*][*Formato*][*Necesita FFmpeg*],
  [`.mp4`], [video H.264, con audio si la escena lo tiene], [sí],
  [`.webm`], [video VP9; admite fondo transparente], [sí],
  [`.webp`], [WebP animado, útil para webs y documentación], [sí],
  [`.gif`], [GIF animado], [sí],
  [`.png`], [secuencia de imágenes, una por fotograma], [no],
)

Opciones útiles:

- `--quality draft` exporta rápido a 30 fps para revisar; `standard` (la
  predeterminada) usa 60 fps y `production`, la mejor codificación.
- `--width` y `--height` cambian la resolución (1920 × 1080 por defecto). La
  escena no cambia: sus unidades lógicas se escalan a los píxeles que pidas.
- `--from` y `--to` exportan solo un tramo, en segundos.

`gaanim export --help` lista todas las opciones.

= El archivo completo

```python
from gaanim import BLUE, WHITE, YELLOW, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")

orbit = scene.geometry.circle(2).stroke(BLUE, 0.05).no_fill()
point = scene.geometry.dot(0.15).fill(YELLOW).move_to(2, 0)
scene.play(point.animate.move_along(orbit).duration(2).easing(Easing.LINEAR))

title = scene.text("Movimiento circular", role="title").fill(WHITE).move_to(0, 3.25)
scene.play([
    title.animate.write().duration(1),
    point.animate.move_along(orbit).duration(2).easing(Easing.LINEAR),
])
scene.wait(0.5)

scene.render()
```

= Siguientes pasos

- #link("/empezar/como-piensa-gaanim/")[Cómo piensa Gaanim] explica las ideas
  que acabas de usar: la escena, los handles, la línea de tiempo, los estilos y
  la salida.
- El #link("/tutorial/antes-de-empezar/")[tutorial] parte de un círculo y un
  punto como estos y los convierte, lección a lección, en una explicación
  completa del círculo al seno.
