#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry
#import "../../components/tutorial.typ": experimental

#show: docs-chapter.with(
  title: "Escena",
  description: "Scene, el lienzo lógico, la línea de tiempo, los segmentos, la cámara y la salida",
  route: "/referencia/scene/",
)

= Escena

`Scene` es el punto de entrada de toda animación: crea los objetos, programa
la línea de tiempo, organiza los segmentos y entrega el resultado al
ejecutable de Gaanim. Cada script crea una escena, la llena y termina con
`scene.render()`.

```python
from gaanim import BLUE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
dot = scene.geometry.circle(0.6).fill(BLUE)
scene.play([dot.animate.create().duration(0.8)])
scene.wait(0.5)
scene.render()
```

Esta página documenta la escena, su lienzo (`scene.canvas`), su cámara
(`scene.camera`) y las secciones. Los métodos comunes de los objetos están en
#link("/referencia/drawable/")[Drawable], las animaciones en
#link("/referencia/animations/")[Animaciones] y los temas en
#link("/referencia/themes/")[Temas y colores]. Para ejecutar, validar y
exportar un script, consulta la #link("/referencia/cli/")[línea de comandos].

== Constructor

#api-entry(
  name: "Scene",
  kind: "class",
  signature: "Scene(*, frame: tuple[float, float] = (16.0, 9.0), background: BackgroundLike | None = None, margin: float | None = None, theme: ThemeName | Theme | None = \"technical\", post: PostProcess | None = None)",
  params: (
    (name: "frame", type: "tuple[float, float]", default: "(16.0, 9.0)", desc: [Ancho y alto del marco lógico, centrado en el origen. Toda la geometría, los márgenes, los tamaños de texto y los trazos usan esta unidad.]),
    (name: "background", type: "BackgroundLike | None", default: "None", desc: [Color, `Brush` o `Background`. Tiene prioridad sobre el fondo del tema.]),
    (name: "margin", type: "float | None", default: "None", desc: [Margen uniforme en unidades lógicas; reduce el área segura.]),
    (name: "theme", type: "ThemeName | Theme | None", default: "\"technical\"", desc: [Nombre o alias de un tema incluido, o un `Theme`. `None` crea la escena sin tema.]),
    (name: "post", type: "PostProcess | None", default: "None", desc: [Postprocesado WGSL aplicado a todos los segmentos que no lo sustituyan.]),
  ),
  returns: (type: "Scene", desc: [Una escena vacía, lista para crear objetos.]),
  desc: [Los píxeles de salida no se eligen aquí sino al previsualizar o exportar, así que la composición es la misma a cualquier resolución. Un marco no finito o no positivo, WGSL inválido o un tema desconocido lanzan `ValueError`; un `theme` de otro tipo lanza `TypeError`. La antigua forma `Scene(width, height)` en píxeles ya no se acepta.],
)[
```python
from gaanim import Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.5, theme="presentation")
print(scene.canvas.frame_width, scene.canvas.safe_width)  # 16.0 15.0
```
]

Sin `theme`, la escena usa el tema `technical`: fondo gris casi negro
(`#121212`), texto y ejes claros y formas rellenas con el color de acento. Un
`background` explícito gana al fondo del tema, y los colores de cada objeto
ganan a los del tema. `theme=None` deja un lienzo sin tema: fondo blanco (si
no pasas `background`) y objetos sin estilo también blancos, así que tendrás
que darles color. Consulta
#link("/referencia/themes/#api-canvas-set-theme")[`Canvas.set_theme`].

== Capacidades

Las fábricas viven en handles que pertenecen a la escena. Un objeto de una
escena no puede usarse en otra: pasarlo lanza `ValueError`.

#api-entry(
  name: "Scene.geometry / text / layout / media / viz / slides / mechanics / sections / assets",
  kind: "property",
  signature: "scene.geometry -> Geometry · scene.text -> Typography · scene.layout -> LayoutBuilder · scene.media -> MediaLibrary · scene.viz -> Visualization · scene.slides -> SlideKit · scene.mechanics -> Mechanics · scene.sections -> SceneSections · scene.assets -> AssetManager",
  returns: (type: "handle", desc: [Un handle de solo lectura ligado a esta escena.]),
  desc: [Cada propiedad agrupa las fábricas de un área.],
)[
#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Propiedad*], [*Contenido*],
  [`scene.geometry`], [Primitivas, trayectorias, flechas, geometría reactiva y 3D. Ver #link("/referencia/geometria/")[Geometría].],
  [`scene.text`], [Texto, ecuaciones, Typst y código. Ver #link("/referencia/text/")[Texto].],
  [`scene.layout`], [Cajas, grids, estilos y zonas. Ver #link("/referencia/layout/")[Layout].],
  [`scene.media`], [Imágenes, SVG, vídeo, audio y Lottie. Ver #link("/referencia/medios/")[Medios] y #link("/referencia/audio/")[Audio].],
  [`scene.viz`], [Ejes, funciones, gráficas, parámetros y matrices. Ver #link("/referencia/visualization/")[Visualización].],
  [`scene.slides`], [Tarjetas, viñetas, tablas e identidad de presentación. Ver #link("/referencia/diapositivas/")[Diapositivas].],
  [`scene.mechanics`], [Muelles, cotas, fuerzas y apoyos. Ver #link("/referencia/mecanica/")[Mecánica].],
  [`scene.sections`], [Agenda y barra de progreso de secciones. Ver #link(<secciones>)[Secciones].],
  [`scene.assets`], [Carpeta de recursos, precarga y recarga. Ver #link("/referencia/assets/")[Recursos].],
)

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="technical")
shape = scene.geometry.circle(1)
title = scene.text("Resultado", role="title")
page = scene.layout.column([title, shape])
value = scene.viz.parameter(0.0)
```
]

#api-entry(
  name: "Scene.canvas",
  kind: "property",
  returns: (type: "Canvas", desc: [El lienzo lógico de la escena.]),
  desc: [Marco, área segura, márgenes, fondo, tema, fuentes y postprocesado. Los miembros de marco y área segura están abajo; los de estilo, en #link("/referencia/themes/")[Temas y colores].],
  none,
)

#api-entry(
  name: "Scene.camera",
  kind: "property",
  returns: (type: "Camera", desc: [La cámara que se ve en la previsualización y en la exportación.]),
  desc: [Consulta #link(<camara>)[Cámara].],
  none,
)

#api-entry(
  name: "Scene.time",
  kind: "property",
  returns: (type: "TimeInput", desc: [Los segundos absolutos de la línea de tiempo como entrada reactiva.]),
  desc: [Es la misma fuente que `scene.viz.time`. Pásala a `computed` o a un setter absoluto; sigue los seeks exactos y la exportación.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>dot = scene.geometry.dot(0.1)
dot.move_to(computed(lambda t: -4 + t, inputs=[scene.time]), 0)
scene.wait(3)
```
]

Las narraciones (`Scene.voiceover`, `Scene.narration_script` y
`Scene.live_take`) están en #link("/referencia/audio/")[Audio].

== Marco lógico y área segura

El marco es el rectángulo que se ve, en unidades lógicas y centrado en el
origen: con el predeterminado de 16 × 9, `x` va de `-8` a `8` e `y` de `-4.5`
a `4.5`. El área segura es el marco menos los márgenes; la usan el layout
(`within="safe"`) y las colocaciones en bordes y esquinas.

#api-entry(
  name: "Canvas.frame_width",
  kind: "property",
  returns: (type: "float", desc: [Ancho del marco lógico; `16.0` en una escena predeterminada.]),
  none,
)

#api-entry(
  name: "Canvas.frame_height",
  kind: "property",
  returns: (type: "float", desc: [Alto del marco lógico; `9.0` en una escena predeterminada.]),
  none,
)

#api-entry(
  name: "Canvas.aspect_ratio",
  kind: "property",
  returns: (type: "float", desc: [Ancho dividido entre alto del marco.]),
  none,
)

#api-entry(
  name: "Canvas.safe_width",
  kind: "property",
  returns: (type: "float", desc: [Ancho del marco menos los márgenes izquierdo y derecho.]),
  none,
)

#api-entry(
  name: "Canvas.safe_height",
  kind: "property",
  returns: (type: "float", desc: [Alto del marco menos los márgenes superior e inferior.]),
  none,
)

#api-entry(
  name: "Canvas.set_margin",
  kind: "method",
  params: ((name: "margin", type: "float", default: none, desc: [Margen igual en los cuatro lados, en unidades lógicas.]),),
  returns: (type: "None", desc: [Reemplaza el área segura actual.]),
  desc: [Equivale a `Scene(margin=...)` después de crear la escena.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.canvas.set_margin(0.4)
print(scene.canvas.safe_width, scene.canvas.safe_height)  # 15.2 8.2
```
]

#api-entry(
  name: "Canvas.set_safe_area",
  kind: "method",
  params: (
    (name: "top", type: "float", default: "0.0", desc: [Margen superior en unidades lógicas.]),
    (name: "right", type: "float", default: "0.0", desc: [Margen derecho.]),
    (name: "bottom", type: "float", default: "0.0", desc: [Margen inferior.]),
    (name: "left", type: "float", default: "0.0", desc: [Margen izquierdo.]),
  ),
  returns: (type: "None", desc: [Reemplaza el área segura actual.]),
  desc: [Útil cuando una marca o una plataforma reserva franjas propias, como la interfaz de un vídeo vertical.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.canvas.set_safe_area(top=0.5, bottom=1.0)
print(scene.canvas.safe_height)  # 7.5
```
]

#api-entry(
  name: "Canvas.set_preset",
  kind: "method",
  params: ((name: "name", type: "\"widescreen\" | \"vertical\" | \"square\"", default: none, desc: [Composición estándar.]),),
  returns: (type: "None", desc: [Reemplaza el marco lógico y el área segura.]),
  desc: [`"widescreen"` da un marco 16 × 9 con área segura 15 × 8, `"vertical"` un marco 9 × 16 con área segura 8 × 13.5 y `"square"` un marco 10 × 10 con área segura 9 × 9.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="technical")
scene.canvas.set_preset("vertical")
page = scene.layout.column([scene.text("Vídeo vertical", role="title")], within="safe")
```
]

== Línea de tiempo

Cada llamada a `play` o `wait` avanza el cursor de autoría. Las animaciones
de una misma lista empiezan juntas; llamadas sucesivas van una detrás de otra.
`scene.launch(...)` (o `play(..., advance=False)`) empieza en el cursor sin
moverlo, para una animación larga que sigue mientras programas lo demás.

#api-entry(
  name: "Scene.play",
  kind: "method",
  params: (
    (name: "items", type: "Playable | Sequence[Playable]", default: none, desc: [Un `Anim`, audio, vídeo, Lottie o `Composition`, o una lista que se reproduce en paralelo.]),
    (name: "duration", type: "float | None", default: "None", desc: [Duración para las animaciones que no fijan la suya.]),
    (name: "easing", type: "Easing | None", default: "None", desc: [Easing para las animaciones que no fijan el suyo.]),
    (name: "advance", type: "bool", default: "True", desc: [Con `False` el cursor no se mueve: el bloque empieza y sigue sonando bajo lo que programes después. Es lo mismo que `scene.launch`.]),
  ),
  returns: (type: "None", desc: [Avanza el cursor hasta el final del bloque, salvo con `advance=False`.]),
  desc: [Programa el bloque de forma atómica: un `Anim` ya usado, de otra escena, repetido o que escribe un canal ocupado en el mismo tramo lanza `ValueError` sin aplicar ningún cambio. `duration` y `easing` son valores por defecto; los que fija cada `Anim` o cada grupo tienen prioridad.],
)[
```python
from gaanim import BLUE, GOLD, Easing, Scene

scene = Scene(frame=(16, 9), theme="technical")
circle = scene.geometry.circle(0.8).fill(BLUE).move_to(-1.6, 0)
rect = scene.geometry.rect(1.8, 1.0).fill(GOLD).move_to(1.6, 0)
scene.play([circle.animate.create(), rect.animate.grow_from_center()], duration=0.8)
scene.play(circle.animate.shift_by(1.2, 0), easing=Easing.SNAPPY)
scene.render()
```
]

#api-entry(
  name: "Scene.launch",
  kind: "method",
  params: (
    (name: "items", type: "Playable | Sequence[Playable]", default: none, desc: [Lo mismo que acepta `play`.]),
    (name: "duration / easing", type: "float | Easing | None", default: "None", desc: [Valores por defecto, como en `play`.]),
  ),
  returns: (type: "None", desc: [No mueve el cursor.]),
  desc: [Empieza el bloque en el cursor y deja el cursor donde estaba: lo que programes después (esperas, otros `play`, cortes, segmentos) ocurre mientras el bloque sigue. Sirve para un giro que dura varios cortes o una música de fondo. Animar el mismo canal del mismo objeto antes de que termine lanza `ValueError`. El vídeo acaba en el cursor, así que lo que siga corriendo ahí se corta; `gaanim check` lo avisa. Los objetos de un segmento salen de pantalla al terminar este, salvo que `scene.persist` los mantenga.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
import math
rueda = scene.geometry.regular_polygon(6, 1.5).fill("#38bdf8")
puntos = [scene.geometry.dot(0.2).move_to(x, -3) for x in (-4, -2, 0, 2, 4)]
scene.launch(rueda.animate.rotate_by(math.tau).duration(5.0))
for punto in puntos:
    scene.play(punto.animate.fade_in().duration(1.0))
```
]

#api-entry(
  name: "Scene.tempo",
  kind: "method",
  params: (
    (name: "bpm", type: "float", default: none, desc: [Pulsos por minuto; positivo.]),
    (name: "offset", type: "float", default: "0.0", desc: [Segundo de la línea de tiempo donde cae el pulso 0, el primer tiempo de la música.]),
    (name: "beats_per_bar", type: "int", default: "4", desc: [Pulsos por compás.]),
  ),
  returns: (type: "None", desc: [Fija el tempo de la escena.]),
  desc: [Rejilla de tempo para cortar a tiempo con la música: `scene.beats(n)` convierte pulsos en segundos y `scene.wait_until(beat=n)` o `scene.wait_until(bar=n)` llevan el cursor a un pulso o al inicio de un compás. Si el cursor ya pasó ese pulso, `wait_until` lanza `ValueError` y dice cuánto se alargó el plano anterior, en vez de desfasarse en silencio. La barra de reproducción del editor dibuja una línea en cada compás. Valores no válidos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.tempo(128, offset=0.0)
logo = scene.geometry.star(5, 1.2, 0.5).fill("#fbbf24")
scene.play(logo.animate.grow_from_center().duration(scene.beats(2)))
scene.wait_until(bar=1)
scene.play(logo.animate.rotate_by(1.0).duration(scene.beats(4)))
scene.wait_until(bar=2)
```
]

#api-entry(
  name: "Scene.wait",
  kind: "method",
  params: ((name: "seconds", type: "float", default: none, desc: [Tiempo que se mantiene la imagen, en segundos.]),),
  returns: (type: "None", desc: [Avanza el cursor.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.wait(1.0)
```
]

#api-entry(
  name: "Scene.fade_out_all",
  kind: "method",
  params: ((name: "seconds", type: "float", default: none, desc: [Duración del fundido.]),),
  returns: (type: "None", desc: [Avanza el cursor `seconds`.]),
  desc: [Desvanece a la vez, con easing suave, todos los objetos del segmento activo. Es la forma rápida de cerrar una escena.],
)[
```python
from gaanim import Scene

scene = Scene(frame=(16, 9), theme="technical")
title = scene.text("Fin", role="title")
scene.play(title.animate.write())
scene.fade_out_all(0.6)
scene.render()
```
]

#api-entry(
  name: "Scene.cursor",
  kind: "property",
  returns: (type: "float", desc: [Posición del cursor de autoría, en segundos absolutos de la línea de tiempo.]),
  desc: [Cuenta a través de todos los segmentos y es de solo lectura.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>title = scene.text("Hola")
scene.play([title.animate.write().duration(0.8)])
reveal_time = scene.cursor  # 0.8
```
]

#api-entry(
  name: "Scene.marker",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Nombre único del instante; se recortan los espacios.]),),
  returns: (type: "None", desc: [No añade duración ni mueve el cursor.]),
  desc: [Da nombre al instante actual de la línea de tiempo global. Es solo un metadato: no pausa la reproducción. El editor lo dibuja como un triángulo sobre la barra de tiempo (al pasar el cursor muestra el nombre y un clic salta a él), y `gaanim export --from` / `--to` lo aceptan en lugar de segundos. Un nombre vacío, repetido o que se lee como número lanza `ValueError`. Para instantes dentro de una `Composition`, usa #link("/referencia/animations/#api-label")[`label`].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>intro = scene.text("Intro").animate.write()
>>>outro = scene.text("Fin").move_to(0, -1).animate.write()
scene.play(intro)
scene.marker("climax")
scene.play(outro)
scene.marker("fin")
```

```bash
gaanim export escena.py --output climax.mp4 --from climax --to fin
```
]

#api-entry(
  name: "Scene.markers",
  kind: "property",
  returns: (type: "list[SceneMarker]", desc: [Los marcadores creados hasta ahora, en orden temporal.]),
  desc: [Cada `SceneMarker` tiene `name`, `time` (segundos absolutos) y `segment` (el segmento activo al crearlo).],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.wait(1.5)
scene.marker("climax")
print([(m.name, m.time) for m in scene.markers])  # [('climax', 1.5)]
```
]

== Segmentos

Un segmento es la unidad estructural de la escena: agrupa objetos, define la
transición de entrada, las notas para Presenter View, la plantilla y un fondo
propio. Los límites entre segmentos son continuos; `stop()` marca dónde la
reproducción interactiva espera al presentador.

#api-entry(
  name: "Scene.segment",
  kind: "method",
  signature: "segment(name: str, transition: Transition | None = None, *, notes: str | None = None, template: Callable[..., Layout] | None = None, background: BackgroundLike | None = None, post: PostProcess | Literal[False] | None = None) -> Segment",
  params: (
    (name: "name", type: "str", default: none, desc: [Nombre no vacío, único en la escena sin distinguir mayúsculas.]),
    (name: "transition", type: "Transition | None", default: "None", desc: [Transición de entrada desde el segmento anterior. Ver #link("/referencia/animations/#api-transition-cross-fade")[Transiciones].]),
    (name: "notes", type: "str | None", default: "None", desc: [Notas del orador que muestra Presenter View.]),
    (name: "template", type: "Callable[..., Layout] | None", default: "None", desc: [Plantilla de Python que se rellena después con `Segment.bind`.]),
    (name: "background", type: "BackgroundLike | None", default: "None", desc: [Fondo solo mientras el segmento está activo; `None` usa el de la escena.]),
    (name: "post", type: "PostProcess | False | None", default: "None", desc: [Postprocesado del segmento: otro `PostProcess`, `False` para ninguno o `None` para heredar `scene.canvas.post`.]),
  ),
  returns: (type: "Segment", desc: [Handle que aceptan `link` y `bind`.]),
  desc: [Crea el segmento y lo activa. La primera llamada sustituye al segmento implícito inicial, que aún no tiene contenido. Un nombre vacío o repetido, o una transición en el primer segmento, lanzan `ValueError`; un `post` de otro tipo lanza `TypeError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="presentation")
intro = scene.segment("Introducción", notes="Presenta el objetivo.", template=title_slide)
intro.bind(title=scene.text("Una idea clara", role="title"))
scene.wait(0.5)
details = scene.segment(
    "Detalles",
    Transition.cross_fade(0.4),
    background=Brush.linear(["#172554", "#0f172a"], start=(-8, 0), end=(8, 0)),
)
scene.wait(0.5)
```
]

#api-entry(
  name: "Segment.bind",
  kind: "method",
  params: ((name: "slots", type: "**Any", default: "{}", desc: [Un argumento con nombre por hueco de la plantilla.]),),
  returns: (type: "Layout", desc: [El layout raíz del segmento.]),
  desc: [Rellena la plantilla del segmento. Un hueco obligatorio que falta o uno que no existe lanzan `TypeError`; un segmento sin plantilla lanza `ValueError`. Las plantillas incluidas (`title_slide`, `lecture`, `comparison`…) se importan desde `gaanim`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="presentation")
>>>baseline = scene.geometry.rect(3, 2).fill(GRAY)
>>>proposed = scene.geometry.rect(3, 2).fill(BLUE)
results = scene.segment("Resultados", template=comparison, notes="Compara los dos modelos.")
results.bind(title=scene.text("Resultados", role="title"), left=baseline, right=proposed)
scene.stop("comparación")
```
]

#api-entry(
  name: "Scene.link",
  kind: "method",
  params: (
    (name: "from_", type: "Segment", default: none, desc: [Segmento de origen.]),
    (name: "to", type: "Segment", default: none, desc: [Segmento de destino, posterior a `from_`.]),
    (name: "transition", type: "Transition", default: none, desc: [Transición que sustituye a la de entrada de `to`.]),
  ),
  returns: (type: "None", desc: [No mueve el cursor.]),
  desc: [Declara la transición entre dos segmentos ya creados. Hace falta para `Transition.morph`, cuyos pares incluyen objetos del segmento de destino. Un segmento de otra escena, o un `to` que no va después de `from_`, lanzan un error.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="technical")
overview = scene.segment("Resumen")
card = scene.geometry.rect(3, 2).fill(BLUE).move_to(-4, 0)
scene.wait(0.5)
detail = scene.segment("Detalle")
panel = scene.geometry.rect(12, 6).fill(BLUE)
scene.wait(0.5)
scene.link(overview, detail, Transition.morph(0.8, pairs=[(card, panel)]))
```
]

#api-entry(
  name: "Scene.stop",
  kind: "method",
  params: (
    (name: "name", type: "str | None", default: "None", desc: [Etiqueta de la pausa en Presenter View.]),
    (name: "loop", type: "Anim | list | None", default: "None", desc: [Animación ambiental, con la misma forma que `scene.play`, que se repite mientras la presentación descansa en la pausa.]),
    (name: "until", type: "Condition | None", default: "None", desc: [Condición del público con la que la pausa avanza sola al presentar. Ver #link("/guias/presentaciones/#avanzar-sola")[Avanzar por sí sola].]),
  ),
  returns: (type: "None", desc: [Sin `loop`, no añade duración ni cambia la imagen; con `loop`, avanza el cursor lo que dura el bucle.]),
  desc: [Pausa la reproducción interactiva cuando el cursor llega a este instante. En el límite de un segmento, el segmento saliente sigue visible hasta que se avanza, así que no hace falta un `wait()` final. La exportación, las capturas y los seeks ignoran las pausas. Después de un `live_take()` grabado, la pausa espera tanto como la pausa real del orador (ver #link("/referencia/audio/")[Audio]). Con `loop`, la animación se coloca justo después de la pausa y, en lugar de congelar la imagen, se repite mientras el orador habla: el siguiente paso sale del bucle y sigue desde su final, y retroceder lo salta. Al exportar se reproduce una vez. Para que la repetición no salte, haz que el bucle termine como empieza. Un nombre vacío, una segunda pausa en el mismo instante del segmento o un bucle sin duración lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>result = scene.text("$x = 2$")
>>>arrow = scene.geometry.arrow(-1, -2, 1, -2)
scene.play([result.animate.write().duration(0.6)])
scene.stop("resultado")
scene.wait(0.5)
# Las flechas siguen moviéndose mientras se explica la pausa.
scene.stop("dos-placas", loop=sequence(
    arrow.animate.shift_by(0.5, 0).duration(0.75),
    arrow.animate.shift_by(-0.5, 0).duration(0.75),
))
```
]

#api-entry(
  name: "Scene.rehearsal",
  kind: "method",
  params: (
    (name: "players", type: "int | Sequence[str]", default: "12", desc: [Cuántos jugadores (Ana, Beto, Caro…) o sus apodos, en orden de llegada; de 1 a 200.]),
    (name: "seed", type: "int", default: "0", desc: [Elige otro grupo: otros tiempos, respuestas y personajes.]),
    (name: "arrive", type: "float | None", default: "None", desc: [Segundos en que entran todos; por defecto, entre `scene.audience()` y la primera pausa de la sala.]),
    (name: "skill", type: "float", default: "0.6", desc: [Qué parte de las preguntas aciertan en promedio, de 0 a 1.]),
    (name: "speed", type: "float", default: "0.5", desc: [Qué tan pronto responden, de 0 (al final) a 1 (al instante).]),
  ),
  returns: (type: "None", desc: []),
  desc: [Describe el público inventado que juega la escena fuera de una presentación en vivo. En la previsualización, la exportación y las capturas sus jugadores entran a la sala, responden cada encuesta y cuestionario, el reloj baja y la clasificación suma sus puntos; las zonas vivas también lo usan. Las respuestas de una pregunta llegan entre que se abre y la pausa donde la presentación las esperaría. Los mismos argumentos dan siempre el mismo ensayo. Sin llamarlo ensayan 12 jugadores. Sin jugadores o con más de 200, un apodo vacío o repetido, `skill` o `speed` fuera de 0–1 o un `arrive` negativo lanzan `ValueError`. Ver #link("/guias/presentaciones/#ensayo")[Ensayo].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.rehearsal(24, seed=3, skill=0.7)
audience = scene.audience()
```
]

#api-entry(
  name: "Scene.poll",
  kind: "method",
  params: (
    (name: "question", type: "str", desc: [Pregunta que responde la audiencia.]),
    (name: "options", type: "Sequence[str]", desc: [Entre 2 y 6 respuestas distintas.]),
    (name: "rehearse", type: "Sequence[float] | None", default: "None", desc: [Un peso por respuesta para el voto del ensayo (`[1, 3]` hace la segunda tres veces más popular); sin él vota al azar.]),
  ),
  returns: (type: "Poll", desc: [Los datos de la encuesta; no dibuja nada.]),
  desc: [Abre una encuesta en el cursor. Recibe votos mientras una presentación está entre este punto y `poll.close()`, o el final del segmento. La escena decide cómo mostrarla con los datos de `Poll`: `qr(tamaño)` y `bar(respuesta, ...)` devuelven drawables; `votes(i)`, `share(i)` y `total()` devuelven `Parameter` que siguen los votos en vivo; `code` y `url` son el código de la sesión y la dirección de votación. Al presentar un paquete `.gaanim` solo las barras siguen los votos en vivo; lo que pase por un `computed` muestra lo que se grabó. Fuera de una presentación vota el ensayo (`scene.rehearsal`). Ver #link("/guias/presentaciones/#encuestas-a-la-audiencia")[Encuestas a la audiencia]. Una pregunta o respuesta vacía, una respuesta repetida, menos de 2 o más de 6 respuestas, o pesos de `rehearse` que no sean uno por respuesta, negativos o todos cero lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
poll = scene.poll("¿Qué crece más rápido?", ["x²", "2ˣ"], rehearse=[1, 2])
card = scene.geometry.rounded_rect(3.4, 3.4, 0.2).fill(WHITE).no_stroke().move_to(-4, 0)
qr = poll.qr(3.0).move_to(-4, 0)
bar = poll.bar(1, length=6).fill(GOLD).no_stroke().move_to(2, 0)
votes = scene.viz.readout(poll.votes(1), format=".0f").move_to(5.6, 0)
scene.stop()
```
]

#api-entry(
  name: "Scene.quiz",
  kind: "method",
  params: (
    (name: "question", type: "str", desc: [Pregunta del juego.]),
    (name: "options", type: "Sequence[str]", desc: [Entre 2 y 6 respuestas distintas.]),
    (name: "correct", type: "int", desc: [Índice de la respuesta correcta (0 para la primera).]),
    (name: "time", type: "int", default: "20", desc: [Segundos para responder, de 5 a 300, medidos por el reloj del relay.]),
    (name: "points", type: "int", default: "1000", desc: [Puntos máximos de una respuesta correcta, de 100 a 10000.]),
    (name: "rehearse", type: "float | Sequence[float] | None", default: "None", desc: [La parte del ensayo que acierta, de 0 a 1 (`0.3` para una pregunta difícil), o un peso por respuesta; sin él responden según su `skill`.]),
  ),
  returns: (type: "Poll", desc: [Los datos del cuestionario, como `scene.poll`.]),
  desc: [Abre un cuestionario al estilo Kahoot: una encuesta con respuesta correcta. Los teléfonos se unen al juego con un apodo, responden una sola vez antes de que se acabe el tiempo y una respuesta correcta gana `points × (1 − tiempo / time / 2)`. `quiz.reveal()` marca dónde la presentación revela la respuesta en los teléfonos, `quiz.remaining()` es la cuenta atrás en segundos y `scene.leaderboard` da la clasificación. Una parte de `rehearse` fuera de 0–1 lanza `ValueError`. Ver #link("/guias/presentaciones/#modo-competencia")[Modo competencia].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿Derivada de x²?", ["x", "2x", "x²/2"], correct=1, time=20, rehearse=0.6)
clock = scene.viz.readout(quiz.remaining(), format=".0f").move_to(-5, 3)
scene.stop()
quiz.reveal()
scene.stop()
```
]

#api-entry(
  name: "Scene.leaderboard",
  kind: "method",
  returns: (type: "Leaderboard", desc: [Los datos de la clasificación; no dibuja nada.]),
  desc: [La clasificación del juego: los jugadores de los cuestionarios de la escena, de mejor a peor. Cada método recibe un puesto (0 para el primero): `name(i, size=, weight=, font=, align=)` devuelve el apodo como texto en vivo, `points(i)` y `players()` devuelven `Parameter` y `bar(i, ...)` una barra relativa al primero. Con ellos diseñas una lista, un podio o lo que quieras. Todo sigue a los resultados en vivo también al presentar un `.gaanim`, que guarda los glifos de los apodos; fuera de una presentación ordena a los jugadores del ensayo por los puntos de sus respuestas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
board = scene.leaderboard()
for rank in range(2):
    board.name(rank, size=0.5).move_to(-3, 1 - rank)
    scene.viz.readout(board.points(rank), format=".0f").move_to(3, 1 - rank)
```
]

#api-entry(
  name: "Scene.character",
  kind: "method",
  params: (
    (name: "avatar", type: "Sequence[int] | None", default: "None", desc: [Las partes: cuerpo, color, ojos, boca y extra. Sin ellas se deducen de `name`.]),
    (name: "name", type: "str", default: "\"\"", desc: [El apodo, que también marca el ritmo con que parpadea.]),
    (name: "size", type: "float", default: "2.0", desc: [Alto en unidades, con sombreros y orejas.]),
  ),
  returns: (type: "Character", desc: [Un dibujable que respira, parpadea y hace expresiones.]),
  desc: [Un personaje como los que crea el público en su teléfono, y que se mueve igual que allí. `character.express("happy")` hace una expresión desde el cursor; con `loop=True` la repite hasta la siguiente, y `express()` vuelve a su cara. Expresiones: `happy`, `sad`, `hurt`, `winner` y `surprised`. Como todo depende del tiempo de la línea de tiempo, saltar y exportar dan el mismo resultado.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
hero = scene.character([1, 0, 4, 2, 1], name="Ana").move_to(-3, 0)
scene.wait(1)
hero.express("winner", loop=True)
scene.wait(2)
```
]

#api-entry(
  name: "Condition",
  kind: "class",
  desc: [Lo que el público debe hacer para que una pausa avance sola con `scene.stop(until=...)`. Se crea con `poll.answered(at_least=)` o `poll.answered(share=)`, `quiz.time_up()` y `audience.at_least(n)`, y se combina con `|` (cualquiera) y `&` (todas). Ver #link("/guias/presentaciones/#avanzar-sola")[Avanzar por sí sola].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
quiz = scene.quiz("¿2 + 2?", ["3", "4"], correct=1)
scene.stop(until=quiz.answered(share=0.8) | quiz.time_up())
```
]

#api-entry(
  name: "Scene.audience",
  kind: "method",
  returns: (type: "Audience", desc: [Los datos del público; no dibuja nada.]),
  desc: [El público del juego: los jugadores en el orden en que entraron, cada uno en un puesto (0 para el primero). `name(i, size=, weight=, font=, align=)` devuelve el apodo como texto en vivo; `count()`, `joined(i)` (1 si el puesto está ocupado, 0 si no) y `age(i)` (segundos desde que entró, hasta 60) devuelven `Parameter`; `qr(size)`, `url` y `code` sirven para invitar. Con ellos diseñas una sala de espera, una arena o lo que quieras. Una escena que lo usa pide el apodo en cuanto el teléfono abre la página. Fuera de una presentación entran los jugadores del ensayo, uno tras otro. Ver #link("/guias/presentaciones/#sala-de-espera")[Sala de espera].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
audience = scene.audience()
for slot in range(6):
    audience.name(slot, size=0.4).move_to((slot - 2.5) * 2, 0)
```
]

#api-entry(
  name: "Scene.stops",
  kind: "property",
  returns: (type: "list[SceneStop]", desc: [Las pausas creadas hasta ahora, en orden temporal.]),
  desc: [Cada `SceneStop` tiene `name` (o `None`), `time` en segundos absolutos y `segment`. Léela al final del script, antes de `render()`, para pedir una captura por pausa; `gaanim --diff --capture-stops` hace lo mismo sin cambiar el script.],
)[
```python
>>>import os
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [stop.time for stop in scene.stops])
```
]

Al empezar un segmento, los objetos de los anteriores dejan de verse. Estos
tres métodos cambian a qué segmento pertenece un objeto, en el cursor actual y
sin alterar su estado visual. Un objeto de otra escena lanza `ValueError`.

#api-entry(
  name: "Scene.reuse",
  kind: "method",
  params: (
    (name: "object", type: "Drawable", default: none, desc: [Primer objeto.]),
    (name: "others", type: "Drawable", default: "()", desc: [Más objetos de la misma escena.]),
  ),
  returns: (type: "None"),
  desc: [Adopta objetos en el segmento activo. Un objeto visible en el segmento anterior queda quieto durante la transición automática y luego pasa a ser contenido del segmento activo. Llamarlo después de `play()` o `wait()` actúa en ese instante.],
  none,
)

#api-entry(
  name: "Scene.persist",
  kind: "method",
  params: (
    (name: "object", type: "Drawable", default: none, desc: [Primer objeto.]),
    (name: "others", type: "Drawable", default: "()", desc: [Más objetos de la misma escena.]),
  ),
  returns: (type: "None"),
  desc: [Mantiene objetos visibles y animables en los segmentos siguientes, desde el cursor actual. Las transiciones automáticas (`cross_fade`, `slide`…) no los afectan. Un objeto invisible sigue invisible hasta que una animación de entrada cambia su opacidad.],
  none,
)

#api-entry(
  name: "Scene.release",
  kind: "method",
  params: (
    (name: "object", type: "Drawable", default: none, desc: [Primer objeto.]),
    (name: "others", type: "Drawable", default: "()", desc: [Más objetos de la misma escena.]),
  ),
  returns: (type: "None"),
  desc: [Termina la persistencia y devuelve los objetos al segmento activo. Al inicio de un segmento, el objeto queda quieto durante la transición de entrada y después pasa a ser local. Nunca oculta ni elimina el objeto: la siguiente transición lo trata como contenido saliente normal.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene, Transition

scene = Scene(frame=(16, 9), background="#0f172a")
title = scene.text("Contexto compartido", role="title").fill(GOLD).move_to(0, 0.875)
scene.play([title.animate.write().duration(0.5)])

scene.segment("contenido", Transition.cross_fade(0.35))
scene.reuse(title)
dot = scene.geometry.dot(0.225).fill(BLUE).move_to(0, -0.375)
scene.play([dot.animate.grow_from_center().duration(0.4)])
scene.persist(title)

scene.segment("cierre", Transition.slide(0.35, "left"))
scene.release(title)
scene.wait(0.5)
# output: preview.webp
scene.render()
```
]

La identidad de una presentación (logo, pie, número de diapositiva) se
configura una vez con `scene.slides.brand(...)`; consulta
#link("/guias/presentaciones/")[Presentaciones].

== Secciones <secciones>

`Section` agrupa pasos de contenido que se construyen con funciones de Python.
Cada paso abre su propio segmento; la función recibe la escena y crea
contenido y pausas sin volver a llamar a `segment`. Estas clases se importan
desde `gaanim`.

#api-entry(
  name: "Section",
  kind: "class",
  signature: "Section(key: str, steps: Sequence[SectionStep], *, title: str | None = None)",
  params: (
    (name: "key", type: "str", default: none, desc: [Identidad estable de la sección; única entre las secciones de una escena.]),
    (name: "steps", type: "Sequence[SectionStep]", default: none, desc: [Pasos en orden; se copian al crear la sección.]),
    (name: "title", type: "str | None", default: "None", desc: [Nombre que muestran la agenda y la barra de progreso; sin él se usa `key`.]),
  ),
  returns: (type: "Section", desc: [Tiene `key`, `title` y `steps`.]),
  desc: [Una clave, un título o una lista de pasos vacíos lanzan `ValueError`; un paso de otro tipo, `TypeError`. `section.build(scene, *, on_enter=None)` abre cada segmento, llama a `on_enter(scene, progress)` y ejecuta la función del paso; devuelve los `Segment` en orden y no inserta pausas. Reutiliza la misma instancia para repetir la sección: el progreso vuelve a empezar y los nombres de segmento incluyen clave, visita, número y nombre del paso. Las excepciones se propagan sin deshacer lo ya creado.],
)[
```python
from gaanim import Scene, Section, SectionStep

scene = Scene(frame=(16, 9), theme="technical")

def contexto(scene):
    title = scene.text("Contexto", size=0.5)
    scene.play(title.animate.write())
    scene.stop()

def propuesta(scene):
    title = scene.text("Propuesta", size=0.5)
    scene.play(title.animate.write())
    scene.stop()

section = Section("introduccion", [
    SectionStep(name="Contexto", build=contexto, notes="Presenta el problema."),
    SectionStep(name="Propuesta", build=propuesta),
])

def al_entrar(scene, progress):
    print(progress.key, progress.index, progress.total, progress.fraction)

segments = section.build(scene, on_enter=al_entrar)
scene.render()
```
]

#api-entry(
  name: "SectionStep",
  kind: "class",
  signature: "SectionStep(*, name: str, build: Callable[[Scene], None], transition: Transition | None = None, notes: str | None = None, template: Callable[..., Layout] | None = None, background: BackgroundLike | None = None)",
  params: (
    (name: "name", type: "str", default: none, desc: [Nombre del paso.]),
    (name: "build", type: "Callable[[Scene], None]", default: none, desc: [Función que crea el contenido del paso.]),
    (name: "transition, notes, template, background", type: "", default: "None", desc: [Igual que en `Scene.segment`.]),
  ),
  returns: (type: "SectionStep", desc: [Valor inmutable.]),
  desc: [Un nombre vacío lanza `ValueError`; una función o plantilla que no se puede llamar, `TypeError`.],
  none,
)

#api-entry(
  name: "SectionProgress",
  kind: "class",
  signature: "SectionProgress(key, step, index, total, visit, segment)",
  returns: (type: "SectionProgress", desc: [Contexto que recibe `on_enter`.]),
  desc: [`key` y `step` identifican la sección y el paso; `index` (desde 1) y `total` cuentan los pasos; `visit` (desde 1) cuenta las repeticiones de la sección; `segment` es el `Segment` recién abierto, útil para `bind`. `fraction` vale `index / total`. Las pausas dentro de un paso no avanzan el progreso.],
  none,
)

#api-entry(
  name: "Scene.sections",
  kind: "property",
  returns: (type: "SceneSections", desc: [Fábricas `agenda(...)` y `progress_rail(...)`.]),
  desc: [Construyen las dos piezas de navegación de una charla sobre una base neutra de colores del tema. Cada parte es un drawable normal que puedes reestilizar, animar u ocultar. Las secciones pueden ser instancias de `Section`, claves o pares `(clave, título)`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="technical")
>>>def _content(scene):
>>>    scene.play(scene.text("Contenido", size=0.5).animate.write())
>>>sections = [
>>>    Section("intro", [SectionStep(name="Contexto", build=_content)], title="Introducción"),
>>>    Section("metodo", [SectionStep(name="Modelo", build=_content)], title="Método"),
>>>]
agenda = scene.sections.agenda(sections, pitch=0.66)
agenda.root.move_to(-2, 1.4, Anchor.TOP_LEFT)
rail = scene.sections.progress_rail(sections, segmented=True, captions=True)
rail.root.move_to(0, -4.3).hud()
scene.persist(rail.root)

for section in sections:
    scene.play([agenda.animate.focus(section), rail.animate.enter(section)])
    section.build(scene, on_enter=lambda scene, p: scene.play(rail.animate.to(p)))
```
]

*Agenda.* `scene.sections.agenda(sections, current=None, *, direction="column",
pitch=0.5, styles=None, item=None, marker=None, marker_gap=0.3)` devuelve un
`Agenda`. Cada entrada está en uno de tres estados, `done`, `current` o
`upcoming`, y se coloca por su punto izquierdo-central a `pitch` unidades de la
anterior, en columna o en fila. Cada entrada guarda un drawable por estado y
solo muestra el suyo, así que un estado puede cambiar peso, color o contenido:
`focus` hace un fundido cruzado entre ellos. Por defecto `done` usa
`foreground`, `current` usa `accent` con peso 700 y `upcoming` usa `muted`;
`styles={"current": TextStyle(...)}` sustituye cualquiera de ellos e
`item=lambda scene, entry, state: ...` sustituye el texto por cualquier
drawable. `marker=lambda scene: ...` añade un indicador a `marker_gap` de la
entrada actual que se desliza con `focus`. Para anotar esas funciones,
`NavigationEntry` y `NavigationState` se importan desde `gaanim`.

- `agenda.item(key)` es el grupo de una entrada, `agenda.items("done")` las
  entradas en un estado y `agenda.variant(key, "current")` el drawable de ese
  estado.
- `agenda.focus(key)` y `agenda.advance(steps)` cambian el estado de
  inmediato; `agenda.animate.focus(key)` y `agenda.animate.advance()` devuelven
  composiciones para `scene.play`, con `easing=` opcional. Los destinos aceptan
  una clave, un índice desde 0, un `Section` o un `SectionProgress`.

*Barra de progreso.* `scene.sections.progress_rail(sections=None, *,
length=12.0, thickness=0.08, orientation="horizontal", segmented=False, ...)`
devuelve un `ProgressRail`. Una barra continua es una sola pista con una marca
en cada límite de sección; `segmented=True` da a cada sección su propia pista,
separadas por `gap`. `rail.animate.to(progress)` llena las secciones anteriores
a un `SectionProgress` y su parte de pasos; un número en `[0, 1]` fija la
fracción de toda la barra. `rail.animate.enter(section)` marca una sección como
actual sin llenar nada, como en una diapositiva divisoria. `captions=True`
nombra cada sección sobre su tramo y la colorea por estado. Las partes son
`track`, `fills`, `marks`, `captions` y `label`, y los argumentos `track=`,
`mark=` y `caption=` sustituyen sus formas. Las barras se llenan de izquierda a
derecha, o hacia arriba con `orientation="vertical"`.

== Salida

#api-entry(
  name: "Scene.render",
  kind: "method",
  returns: (type: "None", desc: [Entrega la línea de tiempo al ejecutable de Gaanim.]),
  desc: [Llámalo una vez, al final del script. Qué se hace con la escena (previsualizarla, validarla, exportarla o capturarla) lo decide el comando que ejecuta el script. Fuera de la aplicación de Gaanim, por ejemplo con `python main.py`, lanza `RuntimeError`.],
)[
```python
from gaanim import Scene

scene = Scene(frame=(16, 9))
scene.wait(0.5)
scene.render()
```
]

#api-entry(
  name: "Scene.snapshots",
  kind: "method",
  params: (
    (name: "directory", type: "str", default: none, desc: [La ruta que `gaanim --diff` pasa en `GAANIM_SNAPSHOTS`.]),
    (name: "times", type: "Sequence[float]", default: none, desc: [Instantes absolutos que se capturan.]),
  ),
  returns: (type: "int", desc: [Número de fotogramas capturados.]),
  desc: [Pide al ejecutable que capture seeks exactos para la comparación visual. Sin `gaanim --diff`, o con otra ruta, lanza `RuntimeError`. Consulta #link("/guias/capturas-y-comparacion/")[Capturas y comparación visual].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
import os

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0])
```
]

#api-entry(
  name: "Scene.thumbnail",
  kind: "method",
  params: (
    (name: "time", type: "float | None", default: "None", desc: [Instante de la línea de tiempo, en segundos; `None` toma el cursor.]),
  ),
  returns: (type: "None", desc: [Solo marca el instante.]),
  desc: [Elige el fotograma que un archivo `.gaanim` guarda como portada, la imagen que el explorador de archivos muestra tras `gaanim register`. Sin llamarlo, la portada es el fotograma de la primera pausa o, sin pausas, el del primer segmento que más muestra. Un `time` negativo o no finito lanza `ValueError`. Consulta #link("/guias/compartir/")[Compartir sin Python].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
titulo = scene.text("Resultados").scale_by(2)
scene.play(titulo.animate.write())
scene.thumbnail()
```
]

== Variación reproducible

#api-entry(
  name: "Scene.random",
  kind: "method",
  params: ((name: "seed", type: "int", default: "0", desc: [Semilla del generador.]),),
  returns: (type: "Random", desc: [Un generador con semilla.]),
  desc: [La misma semilla da los mismos valores en cualquier plataforma, así que la escena sale igual en la previsualización y en la exportación.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
rng = scene.random(seed=42)
stars = [scene.geometry.dot(0.04).move_to(rng.uniform(-7, 7), rng.uniform(-4, 4)) for _ in range(60)]
```
]

#api-entry(
  name: "Random.uniform",
  kind: "method",
  params: (
    (name: "low", type: "float", default: "0.0", desc: [Límite inferior, incluido.]),
    (name: "high", type: "float", default: "1.0", desc: [Límite superior, excluido.]),
  ),
  returns: (type: "float", desc: [Un número en `[low, high)`.]),
  desc: [`high < low` lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Random.gauss",
  kind: "method",
  params: (
    (name: "mean", type: "float", default: "0.0", desc: [Media.]),
    (name: "std", type: "float", default: "1.0", desc: [Desviación típica.]),
  ),
  returns: (type: "float", desc: [Un valor de una distribución normal.]),
  desc: [Una desviación negativa lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Random.integer",
  kind: "method",
  params: (
    (name: "low", type: "int", default: none, desc: [Límite inferior, incluido.]),
    (name: "high", type: "int", default: none, desc: [Límite superior, excluido.]),
  ),
  returns: (type: "int", desc: [Un entero en `[low, high)`.]),
  desc: [Un rango vacío lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Random.choice",
  kind: "method",
  params: ((name: "items", type: "Sequence[Any]", default: none, desc: [Elementos entre los que elegir.]),),
  returns: (type: "Any", desc: [Uno de los elementos.]),
  desc: [Una secuencia vacía lanza `IndexError`.],
  none,
)

#api-entry(
  name: "Random.shuffle",
  kind: "method",
  params: ((name: "items", type: "list[Any]", default: none, desc: [Lista que se baraja.]),),
  returns: (type: "None", desc: [Modifica la lista.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
rng = scene.random(seed=7)
colors = [BLUE, GOLD, CYAN]
rng.shuffle(colors)
size = rng.gauss(1.0, 0.1)
count = rng.integer(3, 8)
accent = rng.choice(colors)
```
]

#api-entry(
  name: "Random.seed",
  kind: "property",
  returns: (type: "int", desc: [La semilla con la que se creó el generador.]),
  none,
)

#api-entry(
  name: "Scene.noise",
  kind: "method",
  params: (
    (name: "frequency", type: "float", default: "1.0", desc: [Rapidez con que cambia el valor.]),
    (name: "amplitude", type: "float", default: "1.0", desc: [El valor queda en `[-amplitude, amplitude]` alrededor de `center`.]),
    (name: "octaves", type: "int", default: "1", desc: [Capas de detalle fino, de 1 a 8.]),
    (name: "seed", type: "int", default: "0", desc: [Semilla del ruido.]),
    (name: "center", type: "float", default: "0.0", desc: [Valor central.]),
  ),
  returns: (type: "Computed", desc: [Un escalar reactivo que depende del tiempo de la línea de tiempo.]),
  desc: [Ruido simplex fractal con semilla, evaluado de forma nativa en cada fotograma sin llamar a Python; la reproducción, los seeks y la exportación coinciden. Valores inválidos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>title = scene.text("Gaanim", role="title")
drift = scene.noise(frequency=0.6, amplitude=0.3, octaves=3, seed=5)
title.rotate_to(computed(lambda v: 0.1 * v, inputs=[drift]))
scene.wait(2)
```
]

== Cámara <camara>

`scene.camera` controla qué parte del mundo se ve. Sus métodos directos
aplican un corte en el cursor, sin avanzar el tiempo, y devuelven la cámara
para encadenar. Los mismos métodos bajo `scene.camera.animate` devuelven un
`Anim` que se programa con `scene.play` junto a cualquier otra animación y
admite `.duration()`, `.delay()` y `.easing()`.

```python
# show-code: true
from gaanim import BLUE, GOLD, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
circle = scene.geometry.circle(0.8).fill(BLUE).move_to(-4, 1)
square = scene.geometry.square(1.2).fill(GOLD).move_to(4, -1)
scene.play([scene.camera.animate.frame_to(circle, margin=0.6).duration(0.8)])
scene.play([scene.camera.animate.frame_to(square, margin=0.6).duration(0.8)])
scene.play([scene.camera.animate.reset().duration(0.6)])
# output: preview.webp
scene.render()
```

Los destinos de posición (`pan_to`, `follow`, `look_at`, `bind_*`) aceptan un
`Drawable`, un `AnchorPoint`, un `PointRef` o una tupla 2D o 3D. El zoom y la
rotación aceptan también `Parameter`, `Variable` y `Computed`. Las APIs de
cámara rechazan valores no finitos, zooms, campos de visión y planos de
recorte inválidos, poses de `look_at` degeneradas e influencias fuera de
`[0, 1]` con `ValueError`, sin recortar valores en silencio.

=== Poses guardadas

#api-entry(
  name: "Camera.state_2d",
  kind: "method",
  params: (
    (name: "center", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Punto que queda en el centro de la vista.]),
    (name: "zoom", type: "float", default: "1.0", desc: [Zoom ortográfico; más de 1 acerca.]),
    (name: "rotation", type: "float", default: "0.0", desc: [Giro de la vista en radianes.]),
  ),
  returns: (type: "CameraState", desc: [Una pose ortográfica reutilizable.]),
  desc: [Valida la pose sin avanzar la línea de tiempo. Un `CameraState` pertenece a la escena que lo creó y no depende del tamaño del lienzo ni de la ventana.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
detail = scene.camera.state_2d(center=(-1.6, 0.4), zoom=1.5)
scene.play([scene.camera.animate.to(detail).duration(0.8)])
```
]

#api-entry(
  name: "Camera.capture",
  kind: "method",
  returns: (type: "CameraState", desc: [La pose que la cámara tiene en el cursor.]),
  desc: [No tiene duración. Registra la pose escrita en el script antes de los bindings, los efectos temporales, el shake y la vista de inspección del editor, así que restaurarla da el mismo resultado al previsualizar, buscar y exportar.],
  none,
)

#api-entry(
  name: "Camera.save",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Nombre no vacío; reemplaza una pose guardada con el mismo nombre.]),),
  returns: (type: "CameraState", desc: [La pose capturada.]),
  desc: [Captura la pose actual y la guarda con un nombre para `restore`.],
  none,
)

#api-entry(
  name: "Camera.to",
  kind: "method",
  params: ((name: "state", type: "CameraState", default: none, desc: [Pose de esta escena.]),),
  returns: (type: "Camera", desc: [La misma cámara.]),
  desc: [Salta a la pose en el cursor. Una pose de otra escena lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Camera.restore",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Nombre usado en `save`.]),),
  returns: (type: "Camera", desc: [La misma cámara.]),
  desc: [Salta a una pose guardada. Un nombre desconocido lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.camera.save("general")
scene.camera.zoom_to(2.0)
scene.wait(0.5)
scene.camera.restore("general")
```
]

=== Encuadre

#api-entry(
  name: "Camera.pan_to",
  kind: "method",
  signature: "pan_to(x: float, y: float) -> Camera | pan_to(target: Endpoint) -> Camera",
  params: (
    (name: "x, y", type: "float", default: none, desc: [Punto que queda en el centro de la vista.]),
    (name: "target", type: "Endpoint", default: none, desc: [Objeto, punto de anclaje o tupla que se centra.]),
  ),
  returns: (type: "Camera", desc: [La misma cámara.]),
  none,
)

#api-entry(
  name: "Camera.zoom_to",
  kind: "method",
  params: ((name: "zoom", type: "ScalarSource", default: none, desc: [Zoom ortográfico; más de 1 acerca y menos de 1 aleja. Debe ser positivo.]),),
  returns: (type: "Camera", desc: [La misma cámara.]),
  none,
)

#api-entry(
  name: "Camera.frame_to",
  kind: "method",
  params: (
    (name: "targets", type: "Drawable | Sequence[Drawable]", default: none, desc: [Objetos que deben caber en la vista.]),
    (name: "margin", type: "float | tuple", default: "None", desc: [Margen alrededor: un valor, `(vertical, horizontal)` o cuatro lados en el orden de CSS.]),
    (name: "dynamic", type: "bool", default: "False", desc: [Recalcula la unión en cada fotograma, después de los updaters y del layout.]),
  ),
  returns: (type: "Camera", desc: [La misma cámara.]),
  desc: [Centra y ajusta el zoom para que los objetos quepan en la vista.],
  none,
)

#api-entry(
  name: "Camera.rotate_to",
  kind: "method",
  params: ((name: "angle", type: "ScalarSource", default: none, desc: [Giro de la vista en radianes.]),),
  returns: (type: "Camera", desc: [La misma cámara.]),
  none,
)

#api-entry(
  name: "Camera.orthographic",
  kind: "method",
  params: ((name: "zoom", type: "float", default: "1.0", desc: [Zoom positivo.]),),
  returns: (type: "Camera", desc: [La misma cámara.]),
  desc: [Selecciona la proyección ortográfica, la de una escena 2D.],
  none,
)

#api-entry(
  name: "Camera.reset",
  kind: "method",
  returns: (type: "Camera", desc: [La misma cámara.]),
  desc: [Restaura la pose 2D predeterminada, el vector vertical, el objetivo y la proyección ortográfica.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>circle = scene.geometry.circle(0.8).move_to(-3, 1)
scene.camera.pan_to(circle).zoom_to(1.5).rotate_to(0.1)
scene.wait(0.5)
scene.camera.frame_to(circle, margin=0.4)
scene.wait(0.5)
scene.camera.reset()
```
]

#api-entry(
  name: "Camera.animate",
  kind: "property",
  returns: (type: "CameraAnimation", desc: [El vocabulario animado de la cámara.]),
  desc: [Cada método devuelve un `Anim` nuevo; construirlo no cambia nada hasta pasarlo a `scene.play`.],
  none,
)

=== Movimientos animados

#api-entry(
  name: "CameraAnimation.to",
  kind: "method",
  params: ((name: "state", type: "CameraState", default: none, desc: [Pose de esta escena.]),),
  returns: (type: "Anim", desc: [Movimiento hasta la pose.]),
  desc: [Una pose de otra escena lanza `ValueError`.],
  none,
)

#api-entry(
  name: "CameraAnimation.restore",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Nombre usado en `Camera.save`.]),),
  returns: (type: "Anim", desc: [Movimiento hasta la pose guardada.]),
  desc: [Un nombre desconocido lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.camera.save("general")
scene.play([scene.camera.animate.zoom_to(2.0).duration(0.6)])
scene.play([scene.camera.animate.restore("general").duration(0.6)])
```
]

#api-entry(
  name: "CameraAnimation.pan_to",
  kind: "method",
  signature: "pan_to(x: float, y: float) -> Anim | pan_to(target: Endpoint) -> Anim",
  params: (
    (name: "x, y", type: "float", default: none, desc: [Punto que termina en el centro de la vista.]),
    (name: "target", type: "Endpoint", default: none, desc: [Objeto, punto de anclaje o tupla que se centra.]),
  ),
  returns: (type: "Anim", desc: [Desplazamiento de la vista.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.play([scene.camera.animate.pan_to(-1.6, 0.4).duration(0.8)])
```
]

#api-entry(
  name: "CameraAnimation.zoom_to",
  kind: "method",
  params: (
    (name: "zoom", type: "ScalarSource", default: none, desc: [Zoom final; más de 1 acerca.]),
    (name: "interpolation", type: "\"exponential\" | \"linear\"", default: "\"exponential\"", desc: [Cómo se interpola el zoom.]),
  ),
  returns: (type: "Anim", desc: [Zoom animado.]),
  desc: [Con `"exponential"` el zoom sigue `z0 * (z1 / z0) ** p`, donde `p` es el progreso con easing: el área visible cambia en la misma proporción en cada fotograma y un zoom de 8× se lee a velocidad constante. `"linear"` usa `z0 + (z1 - z0) * p`. Un zoom constante no positivo o una interpolación desconocida lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.play([scene.camera.animate.zoom_to(8.0).duration(1.5)])
scene.play([scene.camera.animate.zoom_to(1.0, interpolation="linear").duration(0.8)])
```
]

#api-entry(
  name: "CameraAnimation.frame_to",
  kind: "method",
  params: (
    (name: "targets", type: "Drawable | Sequence[Drawable]", default: none, desc: [Objetos que deben caber en la vista.]),
    (name: "margin", type: "float | tuple", default: "None", desc: [Igual que en `Camera.frame_to`.]),
    (name: "dynamic", type: "bool", default: "False", desc: [Sigue a los objetos si se mueven durante el movimiento.]),
    (name: "interpolation", type: "\"exponential\" | \"linear\"", default: "\"exponential\"", desc: [Con `"exponential"` el zoom es exponencial y el desplazamiento acompaña al cambio de ancho visible, así que la vista escala alrededor de un punto fijo y el contenido viaja en línea recta. `"linear"` interpola posición y zoom por separado.]),
  ),
  returns: (type: "Anim", desc: [Encuadre animado.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>circle = scene.geometry.circle(0.8).move_to(-1.6, 0)
>>>label = scene.text("Gaanim").move_to(0, 2.2)
scene.play([scene.camera.animate.frame_to([circle, label], margin=(0.32, 0.48), dynamic=True).duration(0.9)])
```
]

#api-entry(
  name: "CameraAnimation.rotate_to",
  kind: "method",
  params: ((name: "angle", type: "ScalarSource", default: none, desc: [Giro final de la vista en radianes.]),),
  returns: (type: "Anim", desc: [Giro animado.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.play([scene.camera.animate.rotate_to(0.15).duration(0.5)])
```
]

#api-entry(
  name: "CameraAnimation.follow",
  kind: "method",
  params: (
    (name: "target", type: "Endpoint", default: none, desc: [Objeto o punto que se sigue.]),
    (name: "offset", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Desplazamiento de la vista respecto del objetivo.]),
    (name: "offset_space", type: "\"world\" | \"local\"", default: "\"world\"", desc: [`"local"` gira el desplazamiento con el objetivo.]),
    (name: "lag", type: "float", default: "0.0", desc: [Retraso de seguimiento en segundos, determinista.]),
  ),
  returns: (type: "Anim", desc: [Seguimiento durante la duración del `Anim`.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>circle = scene.geometry.circle(0.5).move_to(-4, 0)
scene.play([
    circle.animate.move_to(4, 0).duration(2.0),
    scene.camera.animate.follow(circle.anchor_point(Anchor.TOP), offset=(0, 0.24), lag=0.2).duration(2.0),
])
```
]

#api-entry(
  name: "CameraAnimation.shake",
  kind: "method",
  params: (
    (name: "amplitude", type: "float | None", default: "0.4", desc: [Desplazamiento máximo en unidades de escena con trauma 1.]),
    (name: "frequency", type: "float | None", default: "12", desc: [Frecuencia del ruido en Hz.]),
    (name: "trauma", type: "float | None", default: "0.8", desc: [Intensidad inicial, en `[0, 1]`.]),
    (name: "decay", type: "float | None", default: "1.5", desc: [Trauma que se pierde por segundo.]),
    (name: "rotation", type: "float | None", default: "0.02", desc: [Giro máximo en radianes con trauma 1.]),
    (name: "seed", type: "int | None", default: "0", desc: [Semilla del ruido.]),
  ),
  returns: (type: "Anim", desc: [Sacudida que termina en reposo.]),
  desc: [Sigue el modelo de trauma: el desplazamiento es proporcional a `trauma ** 2`, así que un golpe leve apenas se nota y uno fuerte sacude mucho. Traslación y giro salen de ruido coherente con semilla. Dura `trauma / decay` segundos (un segundo con `decay=0`); un `.duration()` más corto también vuelve suavemente al reposo, y el easing no cambia el decaimiento. Se suma después de follow, encuadre y bindings, y es función pura del tiempo. Pasar solo `amplitude` (sin `trauma`, `decay`, `rotation` ni `seed`) mantiene la sacudida sinusoidal anterior: `amplitude` es el pico, `frequency` cuenta oscilaciones por clip (8 por defecto) y dura 0.5 s. Valores negativos o `trauma` mayor que 1 lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.play([scene.camera.animate.shake(trauma=0.8, decay=1.5, frequency=12, rotation=0.02, seed=0)])
scene.play([scene.camera.animate.shake(trauma=0.4, seed=3)])  # un golpe más leve
```
]

#api-entry(
  name: "CameraAnimation.orthographic",
  kind: "method",
  params: ((name: "zoom", type: "float", default: "1.0", desc: [Zoom positivo.]),),
  returns: (type: "Anim", desc: [Paso animado a la proyección ortográfica.]),
  none,
)

#api-entry(
  name: "CameraAnimation.reset",
  kind: "method",
  returns: (type: "Anim", desc: [Vuelta animada a la pose y proyección predeterminadas.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.play([scene.camera.animate.zoom_to(2.0).duration(0.5)])
scene.play([scene.camera.animate.reset().duration(0.5)])
```
]

=== Bindings persistentes

Un binding es una restricción de cámara que no se dibuja: desde que se crea
(salvo con `enabled=False`) escribe los canales que declara en cada fotograma,
también a través de segmentos. `influence` es un escalar en `[0, 1]` que mezcla
el binding con la pose anterior, y los bindings posteriores se componen sobre
los anteriores. `follow` y el encuadre dinámico actúan después, y el shake se
suma al final.

#api-entry(
  name: "Camera.bind_2d",
  kind: "method",
  params: (
    (name: "center", type: "Endpoint | None", default: "None", desc: [Punto que sigue el centro de la vista.]),
    (name: "zoom", type: "ScalarSource | None", default: "None", desc: [Zoom reactivo.]),
    (name: "rotation", type: "ScalarSource | None", default: "None", desc: [Giro reactivo en radianes.]),
    (name: "influence", type: "ScalarSource | None", default: "None", desc: [Peso en `[0, 1]`; sin él, 1.]),
    (name: "enabled", type: "bool", default: "True", desc: [Activo desde el cursor actual.]),
  ),
  returns: (type: "CameraConstraint", desc: [El binding, con `enable()` y `disable()`.]),
  desc: [Necesita `center`, `zoom` o `rotation`, y selecciona la proyección ortográfica.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
import math

theta = scene.viz.parameter(0.0)
focus = scene.geometry.point_ref(
    computed(lambda t: t * 2.25, inputs=[theta]),
    computed(lambda t: math.sin(t * 2), inputs=[theta]),
)
rig = scene.camera.bind_2d(center=focus, zoom=computed(lambda t: 1 + t * 0.3, inputs=[theta]))
scene.play([theta.animate.set(1.0).duration(2.0)])
rig.disable()
```
]

#api-entry(
  name: "CameraConstraint.enable",
  kind: "method",
  returns: (type: "None", desc: [Activa el binding en el cursor actual.]),
  none,
)

#api-entry(
  name: "CameraConstraint.disable",
  kind: "method",
  returns: (type: "None", desc: [Desactiva el binding en el cursor actual.]),
  desc: [Los cambios quedan registrados en la línea de tiempo: un seek anterior vuelve a verlo activo.],
  none,
)

=== Vistas de cámara

Una vista de cámara muestra dentro de una figura lo que ve una segunda
cámara. `Drawable.camera_view` (en #link("/referencia/drawable/")[Drawable])
convierte cualquier figura cerrada en pantalla; `scene.camera.inset` monta de
una vez la pantalla, el marco y los conectores. Ambos devuelven un
`CameraView`.

#api-entry(
  name: "Camera.inset",
  kind: "method",
  params: (
    (name: "target", type: "Endpoint", default: none, desc: [Qué ampliar: un punto, un objeto o un punto de anclaje. Con `follow=True`, cualquier extremo reactivo.]),
    (name: "zoom", type: "float | Parameter", default: "2.0", desc: [Aumento de la pantalla en reposo.]),
    (name: "at", type: "Anchor | tuple[float, float]", default: "Anchor.TOP_RIGHT", desc: [Esquina o borde del fotograma donde va la pantalla (`Anchor.CENTER` la centra), o punto donde se centra.]),
    (name: "size", type: "float | tuple[float, float] | None", default: "None", desc: [Ancho de la pantalla, o diámetro si es un círculo; por defecto, el 30 % del ancho de la escena (20 % para un círculo). `size=(ancho, alto)` fija ambos lados de un rectángulo.]),
    (name: "aspect", type: "float | None", default: "None", desc: [Proporción ancho / alto de un rectángulo y su marco; por defecto, la del fotograma. Úsala para encuadrar entero un panel alto o una barra de herramientas. Un círculo no admite `aspect`.]),
    (name: "shape", type: "str", default: "\"rounded\"", desc: [`"rect"`, `"rounded"` o `"circle"`.]),
    (name: "follow", type: "bool", default: "False", desc: [El marco sigue a `target` mientras se mueve.]),
    (name: "connectors", type: "bool", default: "True", desc: [Dos líneas que unen las esquinas enfrentadas del marco y la pantalla.]),
    (name: "color", type: "Color | None", default: "None", desc: [Trazo de la pantalla, el marco y los conectores; por defecto, el acento del tema.]),
    (name: "fixed", type: "bool", default: "False", desc: [Fija la pantalla a la imagen como un HUD, para que no se mueva con `scene.camera`.]),
    (name: "background / exclude / layers", type: "", default: none, desc: [Como en `Drawable.camera_view`. Los conectores nunca aparecen en la vista.]),
  ),
  returns: (type: "CameraView", desc: [La vista, con sus `connectors`.]),
  desc: [Muestra un detalle ampliado en una pantalla aparte. El marco sigue el zoom, así que siempre recuadra lo que la pantalla muestra en reposo. Lanza `ValueError` con un zoom o tamaño no positivo, un `shape` o `at` desconocido, o un `target` reactivo sin `follow`.],
)[
```python
# show-code: true
from gaanim import Anchor, CYAN, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
for i in range(24):
    scene.geometry.dot(0.05).fill(CYAN if i % 2 else GOLD).move_to(-6 + 0.25 * i, -1 + 0.15 * (i % 5))
view = scene.camera.inset((-4, -0.7), zoom=4, at=Anchor.TOP_RIGHT)
scene.play([view.animate.pop_out().duration(0.8)])
scene.play([view.animate.pan_to(-1.5, -0.7).duration(1.2), view.animate.zoom_to(8).duration(1.2)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "CameraView.screen",
  kind: "property",
  returns: (type: "Drawable", desc: [La figura que muestra la vista.]),
  none,
)

#api-entry(
  name: "CameraView.frame",
  kind: "property",
  returns: (type: "Drawable", desc: [El objeto que hace de cámara; se anima como cualquier otro.]),
  none,
)

#api-entry(
  name: "CameraView.connectors",
  kind: "property",
  returns: (type: "list[Drawable]", desc: [Las líneas de un inset; vacía en otras vistas.]),
  none,
)

#api-entry(
  name: "CameraView.zoom",
  kind: "property",
  returns: (type: "Parameter | Computed", desc: [El zoom como valor reactivo, para lecturas y vínculos.]),
  desc: [Una vista creada con un número devuelve un `Computed`; con un `Parameter` o `Computed`, lo devuelve tal cual. Si el tamaño del marco fija el zoom, lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>view = scene.camera.inset((0, 0), zoom=3)
label = scene.viz.readout(view.zoom, format=".1f", prefix="x")
```
]

#api-entry(
  name: "CameraView.pan_to",
  kind: "method",
  params: ((name: "x, y / target", type: "float / Endpoint", default: none, desc: [Coordenadas, un objeto o un punto de anclaje.]),),
  returns: (type: "CameraView", desc: [La misma vista.]),
  desc: [Apunta la cámara en el cursor actual: mueve el centro del marco.],
  none,
)

#api-entry(
  name: "CameraView.zoom_to",
  kind: "method",
  params: ((name: "zoom", type: "float", default: none, desc: [Aumento, mayor que cero.]),),
  returns: (type: "CameraView", desc: [La misma vista.]),
  desc: [Fija el zoom en el cursor actual. Si el tamaño del marco fija el zoom, escala el marco para alcanzarlo, medido con su `fit` sobre ambos tamaños en ese punto. Un zoom no positivo, o una vista que sigue un `Computed`, lanza `ValueError`.],
  none,
)

#api-entry(
  name: "CameraView.rotate_to",
  kind: "method",
  params: ((name: "radians", type: "float", default: none, desc: [Giro de la cámara; la vista gira en sentido contrario.]),),
  returns: (type: "CameraView", desc: [La misma vista.]),
  none,
)

#api-entry(
  name: "CameraView.follow",
  kind: "method",
  params: ((name: "target", type: "Endpoint", default: none, desc: [Qué seguir.]), (name: "offset", type: "tuple[float, float]", default: "(0, 0)", desc: [Desplazamiento en unidades de escena.])),
  returns: (type: "CameraView", desc: [La misma vista.]),
  desc: [Mantiene la cámara sobre `target` desde ese momento; `pan_to` deja de tener efecto.],
  none,
)

#api-entry(
  name: "CameraView.animate",
  kind: "property",
  returns: (type: "CameraViewAnimation", desc: [Proxy de animación; sus resultados se pasan a `scene.play`.]),
  none,
)

#api-entry(
  name: "CameraViewAnimation.zoom_to",
  kind: "method",
  params: ((name: "zoom", type: "float", default: none, desc: [Aumento final.]),),
  returns: (type: "Anim", desc: [La animación.]),
  desc: [El zoom propio de la vista cambia a velocidad percibida constante; un `Parameter` se anima de forma lineal y una vista con zoom de marco escala el marco.],
  none,
)

#api-entry(
  name: "CameraViewAnimation.pan_to / rotate_to",
  kind: "method",
  returns: (type: "Anim", desc: [La animación.]),
  desc: [Versiones animadas de `CameraView.pan_to` y `CameraView.rotate_to`.],
  none,
)

#api-entry(
  name: "CameraViewAnimation.pop_out",
  kind: "method",
  returns: (type: "Anim", desc: [La animación.]),
  desc: [La pantalla sale de la región que ve su cámara y crece hasta su sitio. Encogida sobre esa región muestra la escena a tamaño real, así que la vista brota de la escena sin saltos. Si es la primera animación de la pantalla, hace de entrada: hasta que empieza, la pantalla (y en un inset, el marco y los conectores) está oculta. Tras `pop_in`, la devuelve a donde estaba y los vuelve a mostrar.],
  none,
)

#api-entry(
  name: "CameraViewAnimation.pop_in",
  kind: "method",
  returns: (type: "Anim", desc: [La animación.]),
  desc: [Encoge la pantalla de vuelta a la región que ve su cámara y, al llegar, la oculta junto con el marco y los conectores de un inset, así que una capa de vista deja de verse sobre la escena. Es la salida simétrica de `pop_out`; el siguiente `pop_out` los vuelve a mostrar. En una vista de `camera_view`, el marco es tuyo y conserva su visibilidad.],
  none,
)

=== Cámara 3D

#experimental()

La cámara 3D usa coordenadas de mundo `(x, y, z)` y ángulos en radianes. Las
primitivas 3D están en #link("/referencia/geometria/")[Geometría]. Para una
introducción, consulta la guía #link("/guias/camara-y-3d/")[Cámara y 3D].

#api-entry(
  name: "Camera.state_3d",
  kind: "method",
  params: (
    (name: "eye", type: "tuple[float, float, float]", default: none, desc: [Posición de la cámara.]),
    (name: "target", type: "tuple[float, float, float]", default: none, desc: [Punto al que mira.]),
    (name: "up", type: "tuple[float, float, float]", default: "(0.0, 1.0, 0.0)", desc: [Dirección vertical del mundo.]),
    (name: "fov_y", type: "float", default: "0.785…", desc: [Campo de visión vertical en radianes (π/4).]),
    (name: "near, far", type: "float", default: "0.1, 1000.0", desc: [Planos de recorte, con `0 < near < far`.]),
  ),
  returns: (type: "CameraState", desc: [Una pose en perspectiva reutilizable.]),
  none,
)

#api-entry(
  name: "Camera.perspective",
  kind: "method",
  params: (
    (name: "fov_y", type: "float", default: none, desc: [Campo de visión vertical en radianes, en `(0, pi)`.]),
    (name: "near", type: "float", default: "0.1", desc: [Plano de recorte cercano, positivo.]),
    (name: "far", type: "float", default: "1000.0", desc: [Plano de recorte lejano, mayor que `near`.]),
  ),
  returns: (type: "Camera", desc: [La misma cámara.]),
  desc: [Cambia a proyección en perspectiva en el cursor.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.camera.perspective(fov_y=0.785, near=0.1, far=1000.0)
```
]

#api-entry(
  name: "Camera.look_at",
  kind: "method",
  params: (
    (name: "eye", type: "Endpoint", default: none, desc: [Posición de la cámara en el mundo.]),
    (name: "target", type: "Endpoint", default: none, desc: [Punto al que mira.]),
    (name: "up", type: "tuple[float, float, float] | None", default: "None", desc: [Dirección vertical; `(0, 1, 0)` si se omite.]),
  ),
  returns: (type: "Camera", desc: [La misma cámara.]),
  desc: [Coloca la cámara en `eye` mirando a `target`. Los extremos se resuelven después del layout reactivo; `eye` y `target` deben ser distintos y `up` no puede ser nulo ni paralelo a la dirección de la mirada.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.camera.look_at(eye=(7, 5, 6), target=(0, 0, 0))
```
]

#api-entry(
  name: "Camera.bind_3d",
  kind: "method",
  params: (
    (name: "eye", type: "Endpoint | None", default: "None", desc: [Posición reactiva de la cámara.]),
    (name: "target", type: "Endpoint | None", default: "None", desc: [Punto reactivo al que mira.]),
    (name: "fov_y", type: "ScalarSource | None", default: "None", desc: [Campo de visión reactivo en radianes.]),
    (name: "up", type: "tuple[float, float, float]", default: "(0.0, 1.0, 0.0)", desc: [Dirección vertical.]),
    (name: "influence", type: "ScalarSource | None", default: "None", desc: [Peso en `[0, 1]`.]),
    (name: "enabled", type: "bool", default: "True", desc: [Activo desde el cursor actual.]),
  ),
  returns: (type: "CameraConstraint", desc: [El binding.]),
  desc: [Necesita `eye`, `target` o `fov_y`, y selecciona la proyección en perspectiva.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
rig = scene.camera.bind_3d(eye=(6, 4, 8), target=(0, 0, 0), fov_y=0.8, influence=0.75)
scene.wait(1)
rig.disable()
```
]

#api-entry(
  name: "CameraAnimation.look_at",
  kind: "method",
  params: (
    (name: "eye", type: "Endpoint", default: none, desc: [Posición final de la cámara.]),
    (name: "target", type: "Endpoint", default: none, desc: [Punto al que mira al final.]),
    (name: "up", type: "tuple[float, float, float] | None", default: "None", desc: [Dirección vertical; `(0, 1, 0)` si se omite.]),
  ),
  returns: (type: "Anim", desc: [Movimiento animado de la cámara.]),
  none,
)

#api-entry(
  name: "CameraAnimation.orbit",
  kind: "method",
  params: (
    (name: "delta_yaw", type: "float", default: none, desc: [Giro horizontal alrededor del objetivo, en radianes.]),
    (name: "delta_pitch", type: "float", default: none, desc: [Giro vertical, en radianes.]),
  ),
  returns: (type: "Anim", desc: [Órbita alrededor del objetivo actual de `look_at`.]),
  desc: [Usa incrementos pequeños para un giro suave.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.camera.look_at(eye=(7, 5, 6), target=(0, 0, 0))
scene.play([scene.camera.animate.orbit(delta_yaw=0.5, delta_pitch=0.1).duration(1.0)])
```
]

#api-entry(
  name: "CameraAnimation.perspective",
  kind: "method",
  params: (
    (name: "fov_y", type: "float", default: none, desc: [Campo de visión vertical final, en radianes.]),
    (name: "near", type: "float", default: "0.1", desc: [Plano cercano.]),
    (name: "far", type: "float", default: "1000.0", desc: [Plano lejano.]),
  ),
  returns: (type: "Anim", desc: [Paso animado a la perspectiva.]),
  none,
)

#api-entry(
  name: "CameraAnimation.dolly",
  kind: "method",
  params: ((name: "factor", type: "float", default: none, desc: [Multiplicador positivo de la distancia al objetivo.]),),
  returns: (type: "Anim", desc: [Acercamiento o alejamiento.]),
  desc: [Un factor menor que 1 acerca la cámara y uno mayor la aleja.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.camera.look_at(eye=(7, 5, 6), target=(0, 0, 0))
scene.play([scene.camera.animate.dolly(factor=0.85).duration(0.6)])
```
]

=== Cámara de inspección del editor

El editor tiene una cámara de inspección separada de `scene.camera`, que
nunca cambia las capturas, Presenter View ni la exportación. Empieza
desactivada; cada vez que se activa con `I` (o con el botón *Interactivo* de
la barra de overlays, `O`) parte de una copia de la cámara de la escena en ese
instante.

- `Num0`: alterna entre *Free 3D* y *Camera View* (la cámara de la escena).
- En 3D, arrastrar con el botón derecho: orbitar; con el central o Mayús +
  izquierdo: desplazar. En 2D, arrastrar con cualquier botón desplaza la vista
  y la escena sigue al cursor; un clic sin arrastrar selecciona el objeto.
- Rueda: hacia arriba acerca y hacia abajo aleja. En 2D acerca hacia el punto
  bajo el cursor, que se queda quieto.
- `W`, `A`, `S`, `D`: desplazan la cámara. Las flechas siguen navegando entre
  paradas y escenas.
- Mientras la inspección está activa, un clic en la vista previa no avanza a la
  siguiente parada; `Enter` y las flechas sí. Arrastrar o usar la rueda sobre
  los paneles del editor no mueve la cámara.
- `F`: encuadra la selección, o toda la escena si no hay nada seleccionado.
- `R`: restablece y encuadra; `I`: activa o desactiva la inspección.

El marco de salida visible conserva el marco lógico y su proporción, y la
selección con el ratón se limita a ese marco. Mientras la escena tiene
contenido 3D, la barra de tiempo desactiva el ajuste magnético.

=== Overlays del editor

`O` muestra una barra con guías que se dibujan solo en el editor, nunca en las
capturas ni en la exportación. `O` o `Esc` la ocultan.

- *Límites* (`B`): el marco de salida y su tamaño en unidades lógicas.
- *Coordenadas* (`C`): los ejes con sus marcas y el punto bajo el cursor en
  unidades de la escena. Mantén Mayús para ajustarlo a la grilla y pulsa
  `Ctrl+C` (`Cmd+C` en macOS) para copiarlo como tupla, lista para pegar en
  `move_to((x, y))`. La etiqueta se oculta sobre los paneles del editor.
- *Grilla* (`G`): líneas cada 1, 2 o 5 unidades (o sus potencias de diez),
  según el zoom. Durante la inspección cubren toda la vista, no solo el marco.
  En 3D es una grilla sobre el plano XZ alrededor del punto que mira la cámara,
  con el eje X en rojo y el Z en azul.
- *Guías* (`M`): márgenes seguros de acción (90 %) y de títulos (80 %), y los
  tercios del marco.
- *Layout* (`K`): inspecciona las cajas como las herramientas de desarrollo
  de un navegador. Cada caja lleva un contorno discontinuo y las zonas del
  segmento actual un contorno turquesa con su nombre. Al pasar el cursor por
  una caja se colorean su padding (verde), las celdas de sus hijos (azul), sus
  márgenes (naranja) y los gaps entre ellos (morado), con una etiqueta de su
  tipo, tamaño, padding y gap en px de diseño. Sigue la caja en cada momento
  de la línea de tiempo, también mientras se reorganiza.
- El objeto seleccionado se enmarca con su centro y su tamaño en unidades.
- Con la inspección activa, la barra muestra el zoom de la vista (o *3D libre*)
  en azul cuando ya no coincide con la cámara de la escena, y un botón para
  restablecerla (`R`). El icono de teclado lista todos los atajos.

El editor recuerda qué overlays dejaste activados entre sesiones; el modo en
sí siempre empieza oculto.

== Recorte y máscaras

Cualquier objeto vectorial puede recortar a otro con una máscara viva que se
recalcula en cada fotograma: consulta
#link("/referencia/drawable/#api-drawable-clip")[`Drawable.clip`] y
#link("/referencia/drawable/#api-drawable-no-clip")[`Drawable.no_clip`].
