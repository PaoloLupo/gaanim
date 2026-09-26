#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Audio",
  description: "Pistas sincronizadas, narración grabada en el editor y mezcla en la exportación de video",
  route: "/referencia/audio/",
)

= Audio

Música, efectos y narración sincronizados con la línea de tiempo. Declaras una
pista con `scene.media.audio(...)` y la activas con `scene.play([pista])`. La
vista previa la reproduce siguiendo el reloj de la escena (pausa, seek y
velocidad incluidos) y la exportación a MP4 o WebM la mezcla con el video.

== Pistas de audio

Archivos de sonido que empiezan en el cursor donde los activas.

#api-entry(
  name: "MediaLibrary.audio",
  kind: "factory",
  params: ((name: "path", type: "str", default: none, desc: [Archivo de audio; una ruta relativa usa la carpeta de assets, igual que imágenes y SVG.]), (name: "duration", type: "float | None", default: "None", desc: [Recorta la pista y la hace participar en la duración del lote. Sin ella, la pista suena de fondo sin alargar la línea de tiempo.]), (name: "volume", type: "float", default: "1.0", desc: [Ganancia lineal.]), (name: "fade_in / fade_out", type: "float", default: "0.0", desc: [Fundidos de entrada y salida en segundos; el de salida es determinista cuando hay `duration`.])),
  desc: [Declara una pista validada. La declaración no cambia la línea de tiempo: `scene.play([pista])` fija su inicio en el cursor absoluto de esa llamada. Rutas o tiempos inválidos lanzan `ValueError`.],
)[
```python
from gaanim import Scene

scene = Scene()
scene.assets.assets_dir("assets")

music = scene.media.audio("music.ogg", volume=0.35)
pop = scene.media.audio("pop.wav", duration=0.4, volume=0.8, fade_in=0.02)
scene.play([music])
scene.wait(1.5)
scene.play([pop])

scene.render()
```

Una narración grabada con duración fija alarga el lote y termina con un fundido:

```python
>>>from gaanim import *
>>>scene = Scene()
>>>scene.assets.assets_dir("assets")
narration = scene.media.audio(
    "narration.m4a",
    duration=7.5,
    volume=0.9,
    fade_in=0.15,
    fade_out=0.25,
)
scene.play([narration])
```
]

#api-entry(
  name: "Audio",
  kind: "class",
  signature: "scene.media.audio(...) -> Audio",
  desc: [Declaración de audio ligada a la escena que la creó. Solo se usa pasándola a `scene.play`.],
  none,
)

La vista previa reproduce varias pistas a la vez y aplica `volume`, `fade_in` y
`fade_out`. MP4 usa AAC y WebM usa Opus. Las secuencias de imágenes, GIF y WebP
animado rechazan las pistas porque esos formatos no transportan audio.

== Audio de los videos

El audio embebido de un video sigue las mismas reglas que una pista.

`clip = scene.media.video(...)` solo declara el video y `scene.play([clip])`
activa juntos sus fotogramas y su audio; ambos se pausan, recorren y repiten con
la línea de tiempo. `audio=False` silencia el video y `volume` fija su ganancia.
Con `video.segment(start=..., end=...)`, cada fragmento genera una pista finita:
las pausas entre fragmentos son silenciosas aunque el último fotograma siga
visible, y `speed` conserva el tono. Consulta #link("/referencia/medios/")[Medios].

== Narración

Gaanim graba tu voz sin salir del editor y la sincroniza con la escena, sin
editor de video externo. Hay dos formas de trabajar:

- *Voz en off*: la voz marca el ritmo. Cada bloque `scene.voiceover(...)` tiene
  su propia toma y el script espera a marcas con nombre en lugar de llevar
  duraciones fijas. Puedes regrabar un bloque sin tocar los demás.
- *Toma en vivo*: presentas la escena hablando. Cada `scene.stop()` posterior a
  `scene.live_take(...)` se convierte en una pausa tan larga como la que hiciste
  al grabar.

Las tomas viven en `narration/` dentro de la carpeta de assets (o junto al
script si la escena no tiene assets): `narration/<clave>.wav` y, al lado,
`narration/<clave>.json` con las marcas. Al grabar o editar esos archivos, el
editor recarga la escena con los tiempos nuevos. `render()` y la exportación
alargan el timeline para que ninguna toma quede cortada, y MP4/WebM mezclan las
tomas como cualquier otra pista.

=== El guion

El texto que lees no tiene que vivir en el código. Escríbelo en
`narration/script.md` con cualquier editor: cada encabezado `## clave` contiene
el texto de `scene.voiceover("clave")`. Gaanim lo carga automáticamente; para
usar otro archivo, llama antes a `scene.narration_script("guion.md")`. Al
guardar el guion, el editor recarga la escena con las duraciones estimadas
nuevas.

```markdown
# Mi video sobre derivadas

## intro
Hoy vemos qué es una derivada: la pendiente de una curva en un punto.

### nota para mí: sonreír, sin prisa

## tangente
Movemos el punto por la recta y leemos su pendiente, que no cambia.
<!-- aquí respiro antes del cierre -->
```

Un `# título` cierra el bloque anterior; los encabezados `###` son notas para ti
que no se leen, y los comentarios HTML se ignoran. Las líneas seguidas se unen
y una línea en blanco separa párrafos. Una clave repetida o un `##` que no sea
una clave válida producen `ValueError`. El texto de un bloque se busca en este
orden: el argumento `text`, su sección del guion y las `notes` del segmento; con
un guion cargado, una clave sin sección emite un `UserWarning`. El botón
*Crear guion* / *Abrir guion* del panel crea el archivo o le añade las claves
que faltan, y lo abre en tu editor. En una toma en vivo, el teleprompter
muestra la sección con el nombre del segmento actual.

=== Grabar desde el editor

Abre el panel con el botón de micrófono de la barra de reproducción (o
*Más controles → Narración*). El panel lista el guion, la toma en vivo y cada
bloque de voz en off con su estado: *sin grabar* (duración estimada del texto)
o *grabada*, su nivel y de dónde sale su texto. El panel y el teleprompter se
pueden arrastrar desde su asa de puntos, y *Ocultar guion* reduce el
teleprompter a su línea de estado para ver la escena.

- *Grabar* posiciona la escena al inicio del bloque y abre el micrófono
  predeterminado durante una cuenta atrás de tres segundos que sirve de prueba
  de nivel (no se guarda). Después graba mientras la escena avanza, con el
  texto del bloque como teleprompter y la siguiente marca resaltada. Pulsa
  *Espacio* al decir cada marca, *Enter* para guardar y *Esc* para descartar.
- *Grabar toma en vivo* recarga la escena con sus paradas originales y graba
  mientras presentas. La reproducción se detiene en cada parada; *→* o
  *Espacio* continúa, *Enter* termina y *Esc* descarta.
- *Oír* reproduce el bloque desde su inicio.
- *Nivelar* lleva una toma ya grabada al nivel configurado.

Durante la grabación el audio del preview se silencia para que la toma anterior
no se cuele en el micrófono.

=== Nivel de la voz

Al grabar, el medidor del teleprompter muestra los picos del micrófono en dBFS.
La zona verde, entre -18 y -6 dBFS, es la ideal. Entre -6 y -3 dBFS el medidor
pasa a ámbar, y por encima de -3 dBFS el borde del teleprompter se pone rojo con
el aviso "¡Muy fuerte!": una palabra más fuerte podría saturar, y la saturación
no se corrige después. Si algún tramo satura, el aviso lo cuenta hasta el final
de la toma; si durante cuatro segundos nada llega a -30 dBFS, avisa de que la
voz está muy baja.

Cada toma nueva se nivela al guardarla a -16 LUFS integrados, el nivel
recomendado para voz en video online (YouTube reproduce a -14 y la televisión
usa -23). FFmpeg mide la toma y le aplica una ganancia constante, con un pico
real máximo de -1.5 dBTP y un filtro paso alto a 80 Hz. La grabación original
se conserva en `narration/.originals/`. En *Voz y grabación* puedes cambiar el
nivel final: el panel avisa si eliges más de -14 LUFS o menos de -20, o
desactivar la nivelación. Las tomas sin nivelar aparecen marcadas en el panel.

=== Whisper (opcional)

En lugar de pulsar Espacio, whisper.cpp puede detectar cuándo dices cada
palabra. Instala `whisper-cli` y un modelo `ggml-*.bin`, e indícalos en
*Whisper (opcional)* dentro del panel (vacío busca `whisper-cli` en el `PATH`).
*Transcribir* guarda en el JSON de la toma el tiempo de cada palabra; también
puede hacerse automáticamente al guardar cada toma. Requiere FFmpeg. Para que
una marca se resuelva por transcripción, su nombre debe ser la palabra o frase
que dices; se ignoran mayúsculas, tildes y puntuación.

=== Cómo se resuelven las marcas

Cada marca se busca, en este orden: la marca pulsada al grabar (o escrita a
mano en el JSON), la transcripción de Whisper y, por último, su posición
proporcional dentro del texto del bloque. Las marcas se buscan después de la
anterior, así que una palabra repetida corresponde a su siguiente aparición;
pedir otra vez el mismo nombre devuelve el mismo tiempo. Una marca que no se
encuentra emite un `UserWarning` y no espera.

Sin toma grabada, la duración se estima del texto a unas 150 palabras por
minuto, de modo que puedes componer y revisar la escena antes de grabar.

El JSON es editable: mover un número retima la escena sin tocar el código.

```json
{
  "version": 1,
  "markers": { "derivada": 2.4, "pendiente": 5.1 },
  "words": [{ "text": "Hoy", "start": 0.12, "end": 0.3 }],
  "holds": [],
  "loudness": -16.0
}
```

`markers` guarda segundos desde el inicio de la toma, `words` la transcripción,
`holds` los segundos de cada pausa de una toma en vivo y `loudness` el nivel en
LUFS al que se niveló.

=== API de narración

Los bloques de voz en off, sus marcas y la toma en vivo.

#api-entry(
  name: "Scene.voiceover",
  kind: "method",
  params: ((name: "key", type: "str", default: none, desc: [Nombre de la toma: letras, dígitos, `-`, `_` o `.`. Es el nombre de archivo dentro de `narration/`.]), (name: "text", type: "str | None", default: "None", desc: [Texto del bloque: teleprompter y estimación de tiempos. Sin él se usa la sección `## key` del guion y, si no existe, las `notes` del segmento activo.]), (name: "volume", type: "float", default: "1.0", desc: [Ganancia lineal de la toma.]),),
  desc: [Empieza un bloque en el cursor y devuelve un `Voiceover`, usable con `with`. Una toma grabada (`.wav`, `.flac`, `.mp3`, `.m4a`, `.aac`, `.ogg` u `.opus`) suena desde aquí y fija la duración del bloque; si no existe, la duración se estima del texto. Al salir del `with` se espera el resto de la toma. Claves inválidas o repetidas, volumen negativo o tomas ilegibles lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import Scene

scene = Scene()
scene.segment("derivada", notes="Hoy vemos la derivada y su pendiente")
titulo = scene.text("La derivada", role="title").move_to(0, 2.5)
curva = scene.geometry.circle(1.5)
with scene.voiceover("intro") as vo:
    scene.play([titulo.animate.write()])
    vo.wait_until("derivada")
    scene.play([curva.animate.create()], duration=vo.until("pendiente"))
print(f"grabada: {vo.recorded} · {vo.duration:.1f} s · termina en {vo.end:.1f} s")
scene.render()
```
]

#api-entry(
  name: "Voiceover.wait_until",
  kind: "method",
  params: ((name: "marker", type: "str", default: none, desc: [Nombre de la marca: palabra o frase del guion, o el nombre pulsado al grabar.]),),
  desc: [Avanza el cursor hasta la marca. Si la marca ya quedó atrás o no existe, no espera. Usarlo en un bloque cerrado lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Voiceover.until",
  kind: "method",
  desc: [Segundos desde el cursor hasta la marca, nunca negativos. Úsalo como `duration` para que una animación termine justo en la marca.],
)[
```python
>>>from gaanim import *
>>>scene = Scene()
>>>scene.assets.assets_dir("assets")
>>>logo = scene.media.svg("logo.svg").scale_to(0.025)
vo = scene.voiceover("cierre", text="Y eso es todo por hoy")
scene.play([logo.animate.fade_in()], duration=vo.until("todo"))
vo.finish()
```
]

#api-entry(
  name: "Voiceover.finish",
  kind: "method",
  desc: [Espera el resto de la toma y cierra el bloque; repetirlo no hace nada. Es lo que hace el `with` al salir.],
  none,
)

#api-entry(
  name: "Voiceover.key / text / start / duration / end / remaining / recorded",
  kind: "property",
  signature: "key: str · text: str | None · start: float · duration: float · end: float · remaining: float · recorded: bool",
  desc: [Nombre de la toma, texto del bloque, segundos absolutos de inicio y fin, duración (medida o estimada), segundos que faltan desde el cursor y si hay un archivo grabado (`False` mientras la duración sea estimada).],
  none,
)

#api-entry(
  name: "Scene.narration_script",
  kind: "method",
  signature: "narration_script(path) -> None",
  params: ((name: "path", type: "str", default: none, desc: [Archivo Markdown; una ruta relativa se busca en la carpeta de assets, o junto al script si la escena no tiene assets.]),),
  returns: (type: "None", desc: [Los bloques que empiezan después leen su texto de este guion.]),
  desc: [Sin esta llamada se carga `narration/script.md` si existe. Llámala antes de los bloques que la usan. Un archivo ilegible, un `##` que no es una clave válida o una clave repetida producen `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene()
>>>scene.assets.assets_dir("assets")
scene.narration_script("guion.md")
with scene.voiceover("intro") as vo:
    vo.wait_until("derivada")
```
]

#api-entry(
  name: "Scene.live_take",
  kind: "method",
  signature: "live_take(key=\"live\", *, volume=1.0) -> None",
  params: ((name: "key", type: "str", default: "\"live\"", desc: [Nombre de la toma dentro de `narration/`.]), (name: "volume", type: "float", default: "1.0", desc: [Ganancia lineal de la toma.]),),
  returns: (type: "None", desc: [Declara la única toma en vivo de la escena.]),
  desc: [Empieza la toma en el cursor. Con la toma grabada, suena desde aquí y cada `stop()` posterior espera lo que duró tu pausa, así que el timeline se reproduce de corrido en sincronía con tu voz. Sin toma, las paradas siguen siendo interactivas. Una segunda toma en vivo, una clave inválida o usada por una voz en off, o una toma ilegible producen `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene()
>>>scene.assets.assets_dir("assets")
scene.live_take("clase")
scene.segment("intro", notes="Presentamos el problema")
>>>titulo = scene.text("El problema", role="title")
scene.play([titulo.animate.write()])
scene.stop()
scene.segment("idea", notes="La idea clave es…")
>>>diagrama = scene.media.svg("architecture.svg").scale_to(0.02)
scene.play([diagrama.animate.fade_in()])
scene.stop()
scene.render()
```
]
