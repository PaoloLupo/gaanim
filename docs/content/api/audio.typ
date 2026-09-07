#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Audio",
  description: "Pistas sincronizadas en preview y mezcladas en la exportación de video",
  route: "/api/audio/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Audio

Declara primero el audio y actívalo explícitamente con `scene.play([audio])`.
Las rutas relativas usan
`scene.assets.assets_dir(...)`, igual que las imágenes y los archivos SVG. Al exportar
MP4 o WebM, Gaanim envía las pistas a FFmpeg, las alinea con la línea de tiempo,
las mezcla y combina el resultado con el video renderizado. En la vista previa,
las mismas pistas siguen el reloj del timeline.

```python
from gaanim import Scene

scene = Scene()
scene.assets.assets_dir("assets")

music = scene.media.audio("music.ogg", volume=0.35)
pop = scene.media.audio("pop.wav", duration=0.4, volume=0.8, fade_in=0.02)
scene.play([music])
scene.wait(1.5)
scene.play([pop])

# output: lesson.mp4
scene.render()
```

La declaración no modifica el timeline. `scene.play(...)` fija el inicio en su
cursor absoluto. Una pista con `duration` participa en la duración del batch;
sin `duration`, comienza como fondo sin alargar el timeline. `duration` también
recorta la fuente y hace determinista el fundido de salida.

```python
narration = scene.media.audio(
    "narration.m4a",
    duration=7.5,
    volume=0.9,
    fade_in=0.15,
    fade_out=0.25,
)
scene.play([narration])
```

La vista previa reproduce varias pistas a la vez y mantiene su posición al
pausar, recorrer el timeline o cambiar la velocidad. `volume`, `fade_in` y
`fade_out` se aplican también durante esa reproducción. MP4 usa AAC y WebM usa
Opus. Las secuencias de imágenes, GIF y WebP animado rechazan las pistas porque
esos formatos no transportan audio.

El video sigue el mismo modelo: `clip = scene.media.video(...)` solo declara el
drawable y `scene.play([clip])` activa juntos sus frames y su audio embebido.
Ambos se pausan, recorren y repiten junto con el timeline. `audio=false`
silencia ese video y `volume` configura su ganancia.


== Audio de fragmentos de video

`video.segment(start=..., end=..., speed=None, audio=None, volume=None)` usa
la misma selección temporal para imagen y audio. Las opciones omitidas heredan
las del video. Cada fragmento genera una pista finita; las pausas entre fragmentos
son silenciosas aunque el último fotograma siga visible. `speed` conserva el tono,
como en la reproducción de video existente. `audio=False` silencia ese fragmento.
