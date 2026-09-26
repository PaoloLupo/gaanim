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
  [`scene.layout`], [Filas, columnas, grids y regiones. Ver #link("/referencia/layout/")[Layout].],
  [`scene.media`], [Imágenes, SVG, vídeo, audio, Lottie y glTF. Ver #link("/referencia/medios/")[Medios] y #link("/referencia/audio/")[Audio].],
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

#api-entry(
  name: "Scene.play",
  kind: "method",
  params: (
    (name: "items", type: "Playable | Sequence[Playable]", default: none, desc: [Un `Anim`, audio, vídeo, Lottie o `Composition`, o una lista que se reproduce en paralelo.]),
    (name: "duration", type: "float | None", default: "None", desc: [Duración para las animaciones que no fijan la suya.]),
    (name: "easing", type: "Easing | None", default: "None", desc: [Easing para las animaciones que no fijan el suyo.]),
  ),
  returns: (type: "None", desc: [Avanza el cursor hasta el final del bloque.]),
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
  params: ((name: "name", type: "str | None", default: "None", desc: [Etiqueta de la pausa en Presenter View.]),),
  returns: (type: "None", desc: [No añade duración ni cambia la imagen.]),
  desc: [Pausa la reproducción interactiva cuando el cursor llega a este instante. En el límite de un segmento, el segmento saliente sigue visible hasta que se avanza, así que no hace falta un `wait()` final. La exportación, las capturas y los seeks ignoran las pausas. Después de un `live_take()` grabado, la pausa espera tanto como la pausa real del orador (ver #link("/referencia/audio/")[Audio]). Un nombre vacío o una segunda pausa en el mismo instante del segmento lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>result = scene.text("$x = 2$")
scene.play([result.animate.write().duration(0.6)])
scene.stop("resultado")
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

=== Cámara 3D

#experimental()

La cámara 3D usa coordenadas de mundo `(x, y, z)` y ángulos en radianes. Las
primitivas 3D están en #link("/referencia/geometria/")[Geometría] y los modelos
glTF en #link("/referencia/medios/#api-medialibrary-gltf")[Medios]. Para una
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
desactivada; cada vez que se activa con `I` (o con el indicador
*Interactive* del panel de overlays, `O`) parte de una copia de la cámara de
la escena en ese instante.

- `Num0`: alterna entre *Free 3D* y *Camera View* (la cámara de la escena).
- Arrastrar con el botón derecho: orbitar; con el central o Mayús + izquierdo:
  desplazar; rueda: acercar o alejar.
- `F`: encuadra la selección, o toda la escena si no hay nada seleccionado.
- `R`: restablece y encuadra; `I`: activa o desactiva la inspección.

El marco de salida visible conserva el marco lógico y su proporción, y la
selección con el ratón se limita a ese marco. Mientras la escena tiene
contenido 3D, la barra de tiempo desactiva el ajuste magnético.

== Recorte y máscaras

Cualquier objeto vectorial puede recortar a otro con una máscara viva que se
recalcula en cada fotograma: consulta
#link("/referencia/drawable/#api-drawable-clip")[`Drawable.clip`] y
#link("/referencia/drawable/#api-drawable-no-clip")[`Drawable.no_clip`].
