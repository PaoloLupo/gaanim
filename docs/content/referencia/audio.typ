#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Audio",
  description: "Pistas sincronizadas, efectos de sonido anclados, narración grabada en el editor y mezcla en la exportación de video",
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
  params: ((name: "path", type: "str", default: none, desc: [Archivo de audio; una ruta relativa usa la carpeta de assets, igual que imágenes y SVG.]), (name: "duration", type: "float | None", default: "None", desc: [Recorta la pista y la hace participar en la duración del lote. Sin ella, la pista suena de fondo sin alargar la línea de tiempo.]), (name: "end", type: "float | None", default: "None", desc: [Corta la pista en ese segundo del archivo *sin* alargar el lote: música de fondo que suena bajo las escenas siguientes y termina donde quieres. No se combina con `duration`.]), (name: "volume", type: "float", default: "1.0", desc: [Ganancia lineal.]), (name: "fade_in / fade_out", type: "float", default: "0.0", desc: [Fundidos de entrada y salida en segundos; `fade_out` necesita `duration` o `end` y termina justo en el corte.])),
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

Música de fondo que dura 30 s y se funde al final, mientras la escena sigue
programándose desde el mismo cursor:

```python
>>>from gaanim import *
>>>scene = Scene()
>>>scene.assets.assets_dir("assets")
musica = scene.media.audio("music.ogg", end=30.0, fade_out=1.5, volume=0.5)
scene.play([musica])  # no mueve el cursor: lo que sigue suena sobre la música
```
]

#api-entry(
  name: "Audio",
  kind: "class",
  signature: "scene.media.audio(...) -> Audio",
  desc: [Declaración de audio ligada a la escena que la creó. Solo se usa pasándola a `scene.play`.],
  none,
)

== Efectos de sonido anclados

Sonidos cortos que se colocan en la línea de tiempo sin `scene.play`. Usan el
mismo mezclador que `scene.media.audio`: suenan en la vista previa (con pausa,
seek y velocidad) y MP4/WebM los mezclan con el video. Un seek al segundo `t`
oye lo mismo que la reproducción continua, porque cada efecto tiene un inicio
absoluto fijado al componer la escena. Ninguno alarga la línea de tiempo ni
mueve el cursor. Hay tres formas de anclarlos:

- `scene.media.sfx(...)`: en un segundo absoluto.
- `anim.sound(...)`: al inicio de una animación. Si la animación se mueve (un
  `delay`, su lugar en una `sequence` o un `stagger`), el sonido se mueve con
  ella.
- `Transition.*(..., sound=...)`: al inicio de la transición, que es el inicio
  del segmento al que entra.

#api-entry(
  name: "MediaLibrary.sfx",
  kind: "method",
  params: ((name: "path", type: "str", default: none, desc: [Archivo de audio; una ruta relativa usa la carpeta de assets.]), (name: "at", type: "float | None", default: "None", desc: [Segundo absoluto de inicio; sin él, el cursor actual (`scene.cursor`).]), (name: "volume", type: "float", default: "1.0", desc: [Ganancia lineal.])),
  returns: (type: "None", desc: [El efecto queda programado de inmediato.]),
  desc: [Coloca un efecto que suena una vez, entero. Un archivo inexistente o un `at` o `volume` negativos o no finitos lanzan `ValueError`. Para que el sonido siga a una animación que puede moverse, usa `Anim.sound`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene()
>>>scene.assets.assets_dir("assets")
>>>title = scene.text("Hola", role="title")
scene.play(title.animate.fade_in())
scene.media.sfx("whoosh.wav", at=scene.cursor)
scene.media.sfx("pop.wav", at=0.25, volume=0.6)
scene.wait(1)
```
]

#api-entry(
  name: "Anim.sound",
  kind: "method",
  params: ((name: "path", type: "str", default: none, desc: [Archivo de audio; una ruta relativa usa la carpeta de assets.]), (name: "volume", type: "float", default: "1.0", desc: [Ganancia lineal.]), (name: "offset", type: "float", default: "0.0", desc: [Segundos desde el inicio de la animación hasta el sonido; negativo lo adelanta.])),
  returns: (type: "Anim", desc: [Una copia con el efecto anclado; una segunda llamada lo reemplaza.]),
  desc: [El sonido empieza cuando empieza la animación resuelta, incluidos su `delay` y su posición en composiciones, y suena una vez, entero, aunque la animación use `repeat`. Una ruta vacía, un `volume` negativo o no finito o un `offset` no finito lanzan `ValueError` al llamarlo; un archivo inexistente o un sonido que empezaría antes de 0 s lanzan `ValueError` desde `scene.play`, que entonces no programa nada.],
)[
```python
>>>from gaanim import *
>>>scene = Scene()
>>>scene.assets.assets_dir("assets")
title = scene.text("Efectos anclados", role="title")
dot = scene.geometry.circle(0.5).move_to(0, -2)
scene.play(title.animate.write().sound("typing.wav", volume=0.6))
# El pop suena 0.3 s después, con el círculo.
scene.play(dot.animate.grow_from_center().delay(0.3).sound("pop.wav"))
```
]

Una transición con sonido:

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.assets.assets_dir("assets")
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("detalle", Transition.slide(0.5, "left", sound="whoosh.wav"))
scene.wait(1)
```

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

== Audio que anima la escena

Los datos de una pista animan cualquier cosa. `level`, `band` y `pulse`
devuelven un `Computed` del tiempo, entre 0 y 1, que se usa donde se acepta
un número reactivo:
- `scale_to`, `opacity`, `move_to` y `rotate_to`;
- la cámara (`bind_2d`) y los uniforms de los shaders;
- `computed`, `scene.viz.readout` y `fill_level`;
- `Updater.rotate` y `Updater.wiggle`;
- los colores, con `Falloff.source` y `drive("fill", ...)`.

El archivo se analiza una sola vez: los WAV directamente y los demás formatos
con FFmpeg. Cada señal consulta ese análisis donde suena la pista, según su
inicio, así que un salto en la línea de tiempo o una exportación leen lo
mismo que la reproducción. Antes de que la pista suene y después de que
termine, las señales valen 0.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
musica = scene.media.audio("assets/ritmo.ogg")
bajo = musica.band(20, 150, smoothing=0.5)
golpe = musica.pulse(decay=0.12)

logo = scene.geometry.star(5, 1, 0.45).fill(GOLD)
logo.scale_to(computed(lambda b: 0.8 + 0.5 * b, inputs=[bajo]))
logo.add_updater(Updater.wiggle(position=computed(lambda p: 0.15 * p, inputs=[golpe])))
puntos = scene.geometry.group([scene.geometry.circle(0.2).move_to(x, -2) for x in range(-3, 4)])
puntos.drive("fill", Falloff.source(bajo).gradient(BLUE, GOLD))

scene.play(musica)
scene.wait(musica.duration)
scene.render()
```

`scene.play(musica)` sin `duration` no alarga el lote: `scene.wait(musica.duration)`
mantiene la escena mientras suena. El ejemplo completo,
#link("https://github.com/PaoloLupo/gaanim/blob/main/examples/audio_reactive.py")[`examples/audio_reactive.py`],
está en el repositorio de Gaanim; no viene con el paquete instalado.

#api-entry(
  name: "Audio.level",
  kind: "method",
  params: ((name: "smoothing", type: "float", default: "0.0", desc: [En `[0, 1)`: cada cuadro conserva esa parte del anterior, así que cerca de 1 cambia despacio.]),),
  returns: (type: "Computed", desc: [El volumen donde suena la pista: 0 en silencio, 1 en su parte fuerte.]),
  desc: [Envolvente de volumen (RMS), normalizada al percentil 99 del archivo para que unos pocos picos no aplasten el resto. Un `smoothing` fuera de rango lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Audio.band",
  kind: "method",
  params: ((name: "low / high", type: "float", default: none, desc: [Banda de frecuencias en Hz: `band(20, 150)` sigue el bajo y `band(2000, 8000)`, los platillos y las sibilantes.]), (name: "smoothing", type: "float", default: "0.0", desc: [Como en `level`.])),
  returns: (type: "Computed", desc: [La amplitud de la banda, de 0 a 1, cada banda respecto a su propia parte fuerte.]),
  desc: [El análisis llega hasta 11 kHz. Una banda con `low >= high`, negativa o por encima de ese rango lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Audio.pulse",
  kind: "method",
  params: ((name: "decay", type: "float", default: "0.15", desc: [Segundos en que el pulso cae a un tercio.]),),
  returns: (type: "Computed", desc: [1 en cada golpe y bajando hasta 0.]),
  desc: [La forma más directa de reaccionar al ritmo: `scale_to(computed(lambda p: 1 + 0.2 * p, inputs=[musica.pulse()]))`. Un golpe es un *onset*: donde empieza un sonido (un bombo, una nota, una sílaba). Un `decay` no positivo lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Audio.beats",
  kind: "method",
  returns: (type: "list[float]", desc: [Segundos desde el inicio de la pista en que empieza un sonido, en orden.]),
  desc: [Son onsets, no una rejilla de tempo; para una rejilla regular usa `scene.tempo`. Suma `musica.start` cuando la pista ya sonó para obtener segundos de la línea de tiempo, por ejemplo para colocar marcas o animaciones en cada golpe.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>musica = scene.media.audio("assets/ritmo.ogg")
scene.play(musica)
golpes = [musica.start + t for t in musica.beats()]
```
]

#api-entry(
  name: "Audio.start / duration",
  kind: "property",
  desc: [`start` es el segundo de la línea de tiempo en que la pista empezó a sonar por última vez, o `None` antes de reproducirla. `duration` son los segundos que suena: su `duration`, o el archivo entero.],
  none,
)

#api-entry(
  name: "Falloff.source",
  kind: "factory",
  params: ((name: "value", type: "Computed", default: none, desc: [Un `Computed` que depende solo del tiempo: una señal de audio, `scene.noise` o `scene.time`.]),),
  returns: (type: "Falloff", desc: [Su valor, el mismo para todos los miembros.]),
  desc: [Lleva las señales de audio a `drive`, que es como cambian los colores: `drive("fill", Falloff.source(bajo).gradient(BLUE, GOLD))`. Un `Computed` que lee un `Parameter` lanza `ValueError`. Ver #link("/referencia/animations/")[Animaciones].],
  none,
)

== Visualizar el audio

Para dibujar el sonido, una pista da una señal por banda de frecuencia
(`spectrum`) o por instante de su pasado reciente (`waveform`), y
`scene.viz.equalizer` las dibuja. Cada parte se elige o se reemplaza: la forma
de los elementos, su disposición y sus colores. Como son señales normales,
también sirven para dibujar a mano o en un shader.

#api-entry(
  name: "Audio.spectrum",
  kind: "method",
  params: (
    (name: "bands", type: "int", default: "32", desc: [Bandas, de 1 a 256.]),
    (name: "low / high", type: "float", default: "40 / None", desc: [Rango en Hz; `high` es por defecto la frecuencia más alta del análisis, unos 11 kHz.]),
    (name: "smoothing", type: "float", default: "0.0", desc: [Suavizado de cada banda, en `[0, 1)`.]),
  ),
  returns: (type: "list[Computed]", desc: [Una señal de 0 a 1 por banda, de la más grave a la más aguda.]),
  desc: [Reparte el rango en escala logarítmica, como el oído percibe el tono. Cada banda se mide contra su propio máximo, igual que `band`, así que los agudos también se mueven aunque suenen menos. Un rango inválido lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Audio.waveform",
  kind: "method",
  params: (
    (name: "points", type: "int", default: "64", desc: [Instantes, de 2 a 512.]),
    (name: "span", type: "float", default: "2.0", desc: [Segundos que abarca, hacia atrás desde el presente.]),
    (name: "low / high", type: "float | None", default: "None", desc: [Sigue esa banda en lugar del volumen; los dos o ninguno.]),
    (name: "smoothing", type: "float", default: "0.0", desc: [Suavizado, en `[0, 1)`.]),
  ),
  returns: (type: "list[Computed]", desc: [El volumen en `points` instantes de los últimos `span` segundos, el más antiguo primero.]),
  desc: [El punto `i` lee la pista `span * (1 - i / (points - 1))` segundos atrás, así que los valores avanzan hacia el primer punto mientras suena, como el trazo de una grabadora. Valores fuera de rango lanzan `ValueError`.],
  none,
)

#api-entry(
  name: "Visualization.equalizer",
  kind: "factory",
  params: (
    (name: "values", type: "Sequence[ScalarSource]", default: none, desc: [Señales de 0 a 1: `spectrum()`, `waveform()`, `Parameter`, `Computed` o números. Lo que sale de ese rango se recorta.]),
    (name: "shape", type: "str | Callable", default: "\"bar\"", desc: [`"bar"`, `"capsule"` (extremos redondos), `"dot"` (en la punta), `"line"` (una curva por las puntas) o `"area"` (esa curva rellena hasta la base). O una función `shape(scene, index, count)` que devuelve cualquier objeto, que se centra en su lugar, se gira a lo largo y se estira a su longitud.]),
    (name: "layout", type: "str | Callable", default: "\"row\"", desc: [`"row"` (crece hacia arriba desde la base de una caja de `width` × `height`), `"mirror"` (crece hacia los dos lados de una línea media) o `"radial"` (alrededor de un círculo de `radius`, desde `start_angle` en sentido horario, hacia fuera). O una función `layout(index, count)` que devuelve `(x, y, ángulo)`: dónde empieza cada elemento y en qué dirección crece, en radianes.]),
    (name: "fill", type: "Paint | list | FalloffColor | Callable", default: "None", desc: [Una pintura, una lista que se repite, una rampa `Falloff` sobre el grupo (`Falloff.index().gradient(...)`) o `fill(index, count)`.]),
    (name: "value_colors", type: "Sequence[ColorLike] | None", default: "None", desc: [Colorea cada elemento según su propio valor a lo largo de estos colores. Requiere señales que solo dependen del tiempo, como las de audio.]),
    (name: "center / width / height / radius", type: "—", default: "(0, 0) / 8 / 2 / 1.5", desc: [Dónde y de qué tamaño. Cada elemento mide de `min_length` (0.04) a `height`.]),
    (name: "gap / thickness", type: "float", default: "0.25 / None", desc: [Fracción de cada hueco que queda vacía, o un grosor fijo.]),
    (name: "stretch", type: "bool", default: "True", desc: [Con una forma propia, `False` la escala entera en lugar de estirarla, para iconos que deben conservar su proporción.]),
  ),
  returns: (type: "Drawable", desc: [Un grupo con un miembro por valor, en orden: objetos normales que aceptan cualquier estilo, efecto o animación.]),
  desc: [Una lista vacía, una forma o disposición desconocida o tamaños inválidos lanzan `ValueError`; una función que no devuelve lo esperado, `TypeError`.],
)[
```python
# show-code: true
import math
from gaanim import BLUE, CORAL, GOLD, Falloff, Scene
scene = Scene(frame=(16, 9), background="#0b1020")
music = scene.media.audio("assets/ritmo.ogg")
bars = scene.viz.equalizer(music.spectrum(32, smoothing=0.3), center=(-4, 1.5), width=6.5,
                           fill=Falloff.index().gradient(BLUE, GOLD))
ring = scene.viz.equalizer(music.spectrum(40), shape="dot", layout="radial", center=(4, 1.5),
                           radius=1.2, height=0.8, fill=[CORAL, GOLD])
trace = scene.viz.equalizer(music.waveform(96, span=2), shape="line", layout="mirror",
                            center=(0, -2.2), width=12, height=1.6)
scene.play(music)
scene.wait(music.duration)
scene.render()
```
]

Las mismas señales llegan a un shader como uniforms de `PostProcess.shader`.
Un shader no indexa campos, así que el ejemplo genera una función que elige
la banda:

```python
# show-code: true
from gaanim import PostProcess, Scene
scene = Scene(frame=(16, 9), background="#0b1020")
music = scene.media.audio("assets/ritmo.ogg")
bands = music.spectrum(16, smoothing=0.3)
pick = "\n".join(f"    if (i == {i}u) {{ return gaanim_uniforms.b{i}; }}" for i in range(16))
scene.canvas.post = PostProcess.shader(f"""
fn band(i: u32) -> f32 {{
{pick}
    return 0.0;
}}
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {{
    let column = min(u32(uv.x * 16.0), 15u);
    if (1.0 - uv.y < 0.3 * band(column)) {{
        return vec4<f32>(0.2, 0.8, 0.95, 1.0);
    }}
    return gaanim_scene(uv);
}}
""", uniforms={f"b{i}": band for i, band in enumerate(bands)})
scene.play(music)
scene.wait(music.duration)
scene.render()
```

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
>>>logo = scene.media.svg("logo.svg").scale_to(2.5)
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
>>>diagrama = scene.media.svg("architecture.svg").scale_to(2)
scene.play([diagrama.animate.fade_in()])
scene.stop()
scene.render()
```
]
