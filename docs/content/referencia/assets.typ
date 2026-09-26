#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry
#import "../../components/tutorial.typ": experimental

#show: docs-chapter.with(
  title: "Recursos",
  description: "Carpeta de recursos, precarga y recarga, y qué admiten los importadores de SVG, Lottie y glTF",
  route: "/referencia/assets/",
)

= Recursos

Imágenes, SVG, vídeo, audio, Lottie, glTF, WGSL y archivos Typst se cargan
desde rutas locales. Las fábricas que los crean están en
#link("/referencia/medios/")[Medios] y #link("/referencia/audio/")[Audio]; esta
página explica cómo se resuelven las rutas, cómo precargar y recargar
archivos, y qué partes de cada formato se importan.

```python
from gaanim import Scene

scene = Scene(frame=(16, 9), background="#0f172a")
scene.assets.load_project()          # usa assets_dir de ./gaanim.toml

logo = scene.media.svg("logo.svg")
cover = scene.media.image("cover.png")
pulse = scene.media.lottie("pulse.json")
```

Una ruta relativa se resuelve desde la carpeta de recursos de la escena; sin
ella, desde el directorio de trabajo del proceso. Una ruta absoluta se usa tal
cual. Todas las fábricas usan archivos locales: no descargan URLs ni aceptan
bytes o arrays.

== Carpeta de recursos

#api-entry(
  name: "AssetManager.load_project",
  kind: "method",
  params: ((name: "path", type: "str | None", default: "None", desc: [Ruta del manifiesto; sin ella, el `gaanim.toml` que está junto al script que llama.]),),
  returns: (type: "None", desc: [Fija la carpeta de recursos.]),
  desc: [Lee la clave `assets_dir` del manifiesto (`"assets"` si falta, como en la CLI) y la resuelve respecto de la carpeta del manifiesto, así que funciona igual se lance Gaanim desde donde se lance. Una ruta explícita relativa, en cambio, se resuelve desde el directorio de trabajo del proceso; sin argumento se usa el `gaanim.toml` junto al script. Es la forma recomendada en un proyecto. Un manifiesto ilegible lanza `RuntimeError`; un `assets_dir` que no es una cadena entre comillas o que apunta a una carpeta inexistente, `ValueError`. Consulta #link("/referencia/gaanim-toml/")[`gaanim.toml`].],
)[
```python
>>>from gaanim import *
scene = Scene(frame=(16, 9), background="#0f172a")
scene.assets.load_project()
scene.assets.load_project("gaanim.toml")   # ruta explícita
```
]

#api-entry(
  name: "AssetManager.assets_dir",
  kind: "method",
  params: ((name: "path", type: "str", default: none, desc: [Carpeta existente.]),),
  returns: (type: "None", desc: [Fija la carpeta de recursos.]),
  desc: [Una ruta relativa se resuelve desde el directorio de trabajo del proceso, no desde el script. Para que el script no dependa de dónde se lanza, usa `load_project()` o construye la ruta desde `__file__`. Una carpeta inexistente lanza `ValueError`.],
)[
```python
>>>from gaanim import *
from pathlib import Path

scene = Scene(frame=(16, 9), background="#0f172a")
scene.assets.assets_dir(str(Path(__file__).parent / "assets"))
hero = scene.media.image("cover.png", width=8)
```
]

#api-entry(
  name: "AssetManager.preload",
  kind: "method",
  params: ((name: "paths", type: "Sequence[str]", default: none, desc: [Rutas que se validan, relativas a la carpeta de recursos.]),),
  returns: (type: "None", desc: [Valida y guarda en caché los archivos.]),
  desc: [Comprueba archivos ráster, SVG, Lottie JSON, `.lottie` y glTF antes de reproducir la escena. Las imágenes se decodifican en la misma caché que usa `scene.media.image`, y las composiciones Lottie en la de `scene.media.lottie`; de un paquete dotLottie se precarga su animación o máquina de estados inicial. Un archivo que no se puede resolver o decodificar lanza `RuntimeError` con su ruta.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.assets.load_project()
scene.assets.preload(["logo.svg", "cover.png", "pulse.json", "button.lottie"])
```
]

#api-entry(
  name: "AssetManager.reload_assets",
  kind: "method",
  returns: (type: "None", desc: [Vacía las cachés de archivos.]),
  desc: [Vacía las cachés de imágenes ráster, Lottie JSON, paquetes dotLottie y glTF. Los objetos ya creados conservan sus recursos; las cargas siguientes leen los archivos del disco. En el editor no hace falta: guardar cualquier archivo del proyecto vacía las cachés y recarga la escena. Los SVG se vuelven a leer cada vez que `scene.media.svg(...)` crea un objeto.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.assets.load_project()
scene.assets.reload_assets()
cover = scene.media.image("cover.png")
```
]

Para crear un proyecto con esta estructura, usa
#link("/referencia/cli/")[`gaanim init`].

== Lottie JSON y dotLottie

`scene.media.lottie` acepta archivos Lottie JSON y paquetes `.lottie` v1 y v2,
y los dibuja como vectores con Velato dentro de la misma escena. La
reproducción sigue el tiempo absoluto de la escena, también al saltar hacia
atrás. Las opciones de la fábrica y los métodos `set_theme`, `set_input`,
`fire_event`, `warnings` y los identificadores del paquete están en
#link("/referencia/medios/#api-medialibrary-lottie")[Medios].

#api-entry(
  name: "Lottie.source_width",
  kind: "property",
  returns: (type: "int", desc: [Ancho de la composición original, en sus unidades.]),
  none,
)

#api-entry(
  name: "Lottie.source_height",
  kind: "property",
  returns: (type: "int", desc: [Alto de la composición original.]),
  none,
)

#api-entry(
  name: "Lottie.frame_rate",
  kind: "property",
  returns: (type: "float", desc: [Fotogramas por segundo de la composición.]),
  none,
)

#api-entry(
  name: "Lottie.source_duration",
  kind: "property",
  returns: (type: "float", desc: [Duración de la composición, en segundos.]),
)[
```python
# hide-code
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
clip = scene.media.lottie("assets/pulse.json")
print(clip.source_width, clip.source_height, clip.frame_rate, clip.source_duration)
```
]

*Qué se importa.* Formas vectoriales, transformaciones, rellenos, trazos y
máscaras. Las capas sólidas se dibujan con su color y tamaño, también dentro
de precomposiciones. Las imágenes externas (como las que exporta Cavalry) se
resuelven respecto del archivo JSON, incluida su carpeta `images/`, y
conservan el orden de capa, las transformaciones, la opacidad, el tiempo local
de la precomposición y las máscaras simples. Los gradientes lineales y
radiales conservan sus paradas de color y de opacidad, estáticas o animadas.
Una transformación que omite posición o escala usa traslación cero y escala
del 100 %, y las posiciones X/Y separadas se reproducen. El texto, las
imágenes incrustadas y los efectos pueden omitirse o aproximarse: consulta
`clip.warnings`. Si Velato encuentra algo que no puede convertir con
seguridad, la carga lanza un error en lugar de cerrar el proceso.

*Paquetes `.lottie`.* Se leen en memoria, incluidas sus imágenes PNG, JPEG y
WebP; no se extraen al disco ni se buscan recursos fuera del ZIP. La caché
comparte recursos entre clips, pero cada clip tiene sus propias entradas y
órdenes. `animation_id` y `state_machine_id` son excluyentes; sin ninguno se
usa el contenido inicial del manifiesto o, si no hay, la primera animación.
Los controles de reproducción del manifiesto v1 no se aplican: se usan los de
Gaanim.

*Máquinas de estados.* Activar una máquina no alarga la escena: usa
`scene.wait` u otras animaciones. El estado controla velocidad, dirección,
bucles, segmento por marcador y fondo, así que `offset`, `duration`, `loop` y
`speed` deben quedarse en sus valores predeterminados. Un estado final detiene
las transiciones y un marcador ausente usa la animación completa. Se admiten
`PlaybackState`, `GlobalState`, transiciones inmediatas y condiciones
numéricas, booleanas, de cadena y de evento; los temas admiten valores
estáticos de color, escalar, posición, vector y gradiente. No se admiten
acciones, interacciones de ratón, entradas automáticas reservadas,
transiciones suavizadas, temas con keyframes o expresiones, ni sustituciones
temáticas de imágenes o texto: seleccionar un tema o una máquina que los use
lanza `ValueError`, y el resto del paquete sigue disponible. Los ciclos de
transiciones instantáneas también se rechazan.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), background="#0f172a")
clip = scene.media.lottie("assets/button.lottie", state_machine_id="main", width=4)
clip.set_input("active", False)
scene.play([clip])
scene.wait(1)
clip.set_input("active", True).set_theme("gold")
scene.wait(1)
clip.fire_event("reset").set_theme(None)
scene.wait(1)
```

`write` y `create` presentan el primer fotograma trazando los contornos en
paralelo y fundiendo después los rellenos y las imágenes; no inician la
animación interna. Encadena el clip después para reproducirlo:

```python
from gaanim import Scene, sequence

scene = Scene(frame=(16, 9), background="#0f172a")
clip = scene.media.lottie("assets/pulse.json")
scene.play(sequence(clip.animate.create().duration(1.0), clip))
scene.render()
```

== Modelos 3D glTF

#experimental()

`scene.media.gltf(path, *, scene=None)` importa archivos glTF 2.0 locales
(`.gltf` o `.glb`); la fábrica y las acciones de Blender están en
#link("/referencia/medios/#api-medialibrary-gltf")[Medios].

*Qué se importa.* Unidades, orientación, jerarquía de nodos, materiales PBR
metallic-roughness, normales, UV, texturas, skins, huesos y morph targets. Una
unidad glTF es una unidad de mundo de Gaanim: los modelos no se centran ni se
escalan solos. Las cámaras y luces del archivo se descartan en favor de la
cámara de Gaanim y su iluminación neutra. Las extensiones no admitidas y los
buffers o texturas externos que faltan producen un error con la ruta del
archivo.

*Nodos.* `model.part("Robot/Rig/Arm")` selecciona un nodo. El nombre corto
solo vale si es único; una ruta jerárquica resuelve los nombres repetidos, y
las rutas completas duplicadas reciben el sufijo estable `#<índice>`. Los
errores de búsqueda listan los candidatos y `model.parts()` los enumera. Cada
nodo tiene un contenedor propio: sus transformaciones se componen sobre la
de Blender y sobre la animación esquelética o de morph, sin sustituirlas.
Los materiales se copian por instancia, así que animar la opacidad de un
modelo no cambia otra importación del mismo archivo. Los metadatos se guardan
en caché por ruta y fecha de modificación.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), background="#0f172a")
model = scene.media.gltf("assets/robot.glb")
arm = model.part("Robot/Rig/Arm")
print(model.parts())       # selectores estables
print(model.animations())  # acciones de Blender
```

== SVG

`scene.media.svg(path)` conserva el documento como geometría vectorial: una
jerarquía real de caminos y grupos a la que se llega con
#link("/referencia/medios/#api-drawable-part")[`part(id)`]. Por eso un SVG
sirve también como máscara o como operando de operaciones booleanas. Se
importa a una unidad de escena por píxel del documento, centrado en el origen:
dale tamaño con `scale_to(ancho / ancho_en_px)` (consulta
#link("/referencia/medios/#api-medialibrary-svg")[`MediaLibrary.svg`]).

*Qué se importa.* Grupos anidados, CSS, transformaciones, `viewBox` y
`<use>`; rellenos y trazos sólidos, lineales y radiales con sus modos de
extensión; geometría `clipPath`, sin rasterizar el documento; texto convertido
en contornos con las fuentes instaladas; y los filtros `feGaussianBlur` y
`feDropShadow`, como efectos vectoriales de Gaanim.

*Qué no.* Patrones de pintura, máscaras `<mask>` de alfa o luminancia,
imágenes ráster incrustadas y grafos de filtros arbitrarios. Para una máscara,
usa una silueta vectorial explícita. Para que el texto salga igual en
cualquier equipo, instala la fuente en cada máquina que renderiza o convierte
el texto en caminos en el SVG de origen.

Los identificadores de las partes distinguen mayúsculas; un identificador
repetido en el archivo falla al importar y uno desconocido lanza `KeyError`
con los nombres disponibles. El grosor aplicado al objeto raíz con
`stroke(...)` está en unidades lógicas de escena y no se reduce al escalar el
SVG para encajarlo en el marco.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), background="#0f172a")
diagram = scene.media.svg("assets/architecture.svg")
scene.play(diagram.part("database").animate.indicate().duration(0.6))
```
