#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Cómo piensa Gaanim",
  description: "El modelo mental de una escena: coordenadas, handles, línea de tiempo, estilos, layout, recursos y salida",
  route: "/empezar/como-piensa-gaanim/",
  nav: "Cómo piensa Gaanim",
)

= Un script describe, no dibuja

Un script de Gaanim no pinta fotogramas. Describe una escena: qué objetos hay,
cómo se ven y cuándo cambian. Al final, `scene.render()` entrega esa
descripción al ejecutable `gaanim`, que la muestra en la vista previa, la
comprueba o la exporta.

Como la escena es una descripción y no un video grabado, Gaanim puede saltar a
cualquier instante, recargarla al guardar el archivo y exportar exactamente lo
mismo que ves en la vista previa.

Esta página recorre las piezas de ese modelo. Cada ejemplo es un archivo
completo que puedes ejecutar con `gaanim`. Si todavía no has creado ninguna
escena, empieza por #link("/empezar/primera-animacion/")[Tu primera animación].

= La escena y sus coordenadas

`Scene` es el punto de partida: fija el tamaño del lienzo, el fondo, el tema y
la línea de tiempo.

```python
from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")

x_axis = scene.geometry.arrow(-7.5, 0, 7.5, 0).fill(BLUE)
y_axis = scene.geometry.arrow(0, -4, 0, 4).fill(BLUE)
origin = scene.text("(0, 0)", role="label").fill(WHITE).move_to(0.7, -0.4)
target = scene.text("(5, 2.5)", role="label").fill(GOLD).move_to(5, 3.1)

point = scene.geometry.dot(0.15).fill(GOLD)
scene.play(point.animate.move_to(5, 2.5).duration(1))
scene.render()
# output: preview.webp
```

- El lienzo mide 16 × 9 *unidades lógicas*. El origen `(0, 0)` está en el
  centro, X crece hacia la derecha (de `-8` a `8`) e Y hacia arriba (de `-4.5`
  a `4.5`).
- Posiciones, radios, grosores de trazo y tamaños de texto usan esa misma
  unidad. Los píxeles solo aparecen al exportar, así que la misma escena sale
  igual a 1280 × 720 que a 3840 × 2160.
- Un objeto nuevo aparece en el origen hasta que lo mueves.
- Los tiempos se expresan en segundos y los ángulos, en radianes.

`frame` admite otras proporciones, por ejemplo `(9, 16)` para video vertical.
`margin` define un área segura que usan el layout y las plantillas.

= Fábricas y handles

Los objetos se crean con _fábricas_ agrupadas por tema dentro de la escena:

- `scene.geometry`: círculos, rectángulos, líneas, flechas, arcos, grupos…
- `scene.text`: texto, ecuaciones con Typst y código.
- `scene.layout`: filas, columnas, grids y tarjetas.
- `scene.media`: imágenes, SVG, video, audio y Lottie.
- `scene.viz`: ejes, gráficas de funciones y datos.

Cada fábrica registra el objeto en la escena y devuelve un _handle_: una
referencia con la que sigues describiéndolo. Los métodos de estilo y posición
(`fill`, `stroke`, `move_to`, `rotate_by`…) son _setters fluidos_: cambian el
objeto y devuelven un handle, así que puedes encadenarlos o escribirlos en
líneas separadas.

```python
from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")

orbit = scene.geometry.circle(2).stroke(BLUE, 0.05).no_fill()
marker = scene.geometry.square(0.3).fill(GOLD).move_to(2, 0).rotate_by(0.785)

label = scene.text("r = 2", role="label")
label.fill(WHITE)
label.move_to(1, 0.3)

system = scene.geometry.group([orbit, marker, label]).move_to(-3, 0)
scene.render()
```

Un grupo transforma varias piezas como una unidad, y cada handle sigue sirviendo
para cambiar su pieza por separado. Pon a los handles nombres que digan su
papel (`orbit`, `label`), no su forma (`c1`, `t2`).

= Construcción y línea de tiempo

Una escena tiene dos fases que conviene distinguir:

1. *Construcción*: todo lo que escribes antes del primer `play` o `wait`
   define el estado inicial.
2. *Línea de tiempo*: cada `play` y cada `wait` avanza un cursor. Lo que
   programas después ocurre en ese instante.

`objeto.animate` no cambia nada: devuelve la descripción de una animación
(un `Anim`) que puedes configurar con `duration`, `easing` o `delay`.
`scene.play(...)` la coloca en la línea de tiempo, en la posición del cursor.

```python
from gaanim import BLUE, GOLD, RED, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
circle = scene.geometry.circle(1).fill(BLUE).move_to(-2, 0)
square = scene.geometry.square(1.5).fill(GOLD).move_to(2, 0)

scene.play([
    circle.animate.fade_in().duration(1),
    square.animate.fade_in().duration(2),
])
print(scene.cursor)  # el play dura lo que su animación más larga

scene.play(circle.animate.shift_by(0, 1).duration(0.5))
scene.wait(0.5)
print(scene.cursor)

square.fill(RED)  # después del tiempo 3.0: es un corte instantáneo
scene.wait(1)
scene.render()
```

- Las animaciones de una misma lista empiezan juntas. Llamadas a `play`
  consecutivas van una detrás de otra.
- `scene.wait(segundos)` avanza el cursor sin animar nada.
- Un objeto con animación de entrada (`create`, `write`, `fade_in`…) permanece
  oculto hasta que esta empieza. Uno sin entrada está visible desde el
  principio.
- Un setter fluido llamado cuando el cursor ya avanzó, como `square.fill(RED)`,
  no reescribe el estado inicial: produce un cambio instantáneo en ese punto de
  la línea de tiempo. Para que el cambio sea gradual, usa `.animate`:
  `square.animate.fill(RED)`.

== Grupos de animaciones y easing

Para coordinar varias animaciones, `scene.play` acepta composiciones:
`stagger(...)` escalona los inicios, `sequence(...)` encadena y `parallel(...)`
agrupa. El _easing_ decide cómo avanza cada animación en su duración.

```python
from gaanim import GOLD, WHITE, Easing, Scene, stagger

scene = Scene(frame=(16, 9), background="#0f172a")
easings = {"LINEAR": Easing.LINEAR, "SMOOTH": Easing.SMOOTH, "BOUNCY": Easing.BOUNCY}

dots = []
for i, name in enumerate(easings):
    y = 1.5 - 1.5 * i
    scene.text(name, role="label").fill(WHITE).move_to(-6, y)
    dots.append(scene.geometry.dot(0.25).fill(GOLD).move_to(-4, y))

scene.play(stagger(*[dot.animate.fade_in().duration(0.4) for dot in dots], each=0.1))
scene.play([
    dot.animate.shift_by(8, 0).duration(1.5).easing(easing)
    for dot, easing in zip(dots, easings.values())
])
scene.render()
# output: preview.webp
```

- `stagger(a, b, c, each=0.1)` arranca cada animación 0.1 s después de la
  anterior.
- `Easing.LINEAR` mantiene la velocidad; `Easing.SMOOTH` acelera y frena, y es
  el valor por defecto de la mayoría de las animaciones; `Easing.BOUNCY` es un
  muelle que se pasa del destino y rebota.
- Elige el easing por lo que comunica: velocidad constante para un proceso
  físico uniforme, suavizado para un cambio de estado, muelle para algo que
  llega con energía.

La #link("/referencia/animations/")[referencia de animaciones] lista todas las
animaciones, los easings y las composiciones.

= Estilos y temas

Hay dos maneras de dar estilo:

- *Estilo local*: los setters fluidos (`fill`, `stroke`, `opacity`) sobre un
  objeto concreto. Es lo más directo para una excepción.
- *Tema*: un `Theme` reúne colores con nombre, tipografía y reglas por tipo de
  objeto o por clase. Cambiar el tema cambia toda la escena.

```python
from gaanim import Scene, Style, Theme

theme = Theme(
    "technical",
    colors={"brand": "#38bdf8"},
    styles={".destacado": Style(fill="brand")},
)
scene = Scene(theme=theme)

normal = scene.geometry.circle(1).move_to(-3, 0)
featured = scene.geometry.circle(1).style_class("destacado")
explicit = scene.geometry.circle(1).style_class("destacado").fill("#f97316").move_to(3, 0)
scene.render()
```

El primero toma el estilo del tema `technical`; el segundo, el de la clase
`.destacado`; el tercero, el color explícito, porque un setter fluido siempre
gana. Los roles de texto (`title`, `subtitle`, `body`, `label`…) también
vienen del tema, así que `scene.text(..., role="title")` tiene el aspecto de
título sin fijar tamaño ni fuente.

`Scene(theme="paper")` usa un tema incluido tal cual; `Theme.schemes()` lista
los disponibles. Los detalles están en
#link("/referencia/themes/")[Temas y colores].

= Coordenadas o layout

Para colocar objetos tienes dos herramientas, cada una para un tipo de pieza:

- *Coordenadas* (`move_to`, `shift_by`) cuando la posición tiene significado:
  el punto de una gráfica, el centro de una órbita.
- *Layout* cuando lo que importa es la composición: un título arriba, una fila
  de pasos, columnas, tarjetas. El layout mide el contenido y se reajusta si
  cambia un texto.

```python
from gaanim import Direction, Scene

scene = Scene(frame=(16, 9), theme="technical")

title = scene.text("Tres pasos", role="title").to_edge(Direction.UP, buff=0.6)
steps = scene.layout.row(
    [scene.text(word, role="body") for word in ("Crear", "Animar", "Exportar")],
    gap=1.2,
).move_to(0, 1)

orbit = scene.geometry.circle(1).move_to(0, -2)
radius = scene.text("r = 1", role="caption").next_to(orbit, Direction.RIGHT, spacing=0.3)
scene.render()
```

`to_edge` y `next_to` colocan un objeto respecto al borde del lienzo o a otro
objeto. `scene.layout.row` es un contenedor: decide dónde va cada hijo, y
mover un hijo por su cuenta produce un `LayoutOwnershipError`. Mueve el
contenedor entero, como hace `.move_to(0, 1)`.

La guía de #link("/guias/layout/")[Layout] cubre filas, columnas, grids,
regiones y anclas.

= Recursos

Imágenes, SVG, audio, video y fuentes viven en la carpeta `assets/` del
proyecto. `scene.assets.load_project()` lee el `gaanim.toml` que está junto al
script y resuelve las rutas relativas dentro de esa carpeta:

```python
from gaanim import WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
scene.assets.load_project()

cover = scene.media.image("cover.png", width=3).move_to(0, 0.5)
caption = scene.text("assets/cover.png", role="caption").fill(WHITE).move_to(0, -1.5)
scene.render()
```

Así el proyecto funciona igual lo abras desde donde lo abras. `width` usa
unidades de la escena, como todo lo demás. Consulta
#link("/referencia/assets/")[Recursos] para la precarga y los formatos, y
#link("/referencia/audio/")[Audio] para sonido y narración.

= La salida

El script no decide qué se hace con la escena; lo decide el comando con el que
lo abres:

#table(
  columns: 2,
  table.header[*Comando*][*Qué hace*],
  [`gaanim .`], [Abre la vista previa y recarga al guardar.],
  [`gaanim check .`], [Ejecuta la escena sin ventana e informa de errores y avisos.],
  [`gaanim export . --output video.mp4`], [Renderiza la línea de tiempo a un archivo.],
  [`gaanim --present .`], [Presenta la escena y se detiene en cada `scene.stop()`.],
)

Por eso `scene.render()` va siempre al final del script: es el momento en que
la descripción está completa. La exportación y sus formatos se explican en
#link("/empezar/primera-animacion/")[Tu primera animación]; la organización de
proyectos, en #link("/guias/proyectos/")[Proyectos], y las
presentaciones, en #link("/guias/presentaciones/")[Presentaciones].

= Dónde seguir

- El #link("/tutorial/antes-de-empezar/")[tutorial] aplica estas ideas a un
  proyecto completo, lección a lección.
- La #link("/referencia/")[referencia] documenta cada fábrica, setter y
  animación: #link("/referencia/scene/")[Scene],
  #link("/referencia/geometria/")[geometría] y
  #link("/referencia/text/")[texto].
