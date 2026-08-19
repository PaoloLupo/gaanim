#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Audio",
  description: "Pistas alineadas con la línea de tiempo y mezcladas en la exportación de video",
  route: "/api/audio/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Audio

Declara el audio directamente en la escena. Las rutas relativas usan
`scene.assets_dir(...)`, igual que las imágenes y los archivos SVG. Al exportar
MP4 o WebM, Gaanim envía las pistas a FFmpeg, las alinea con la línea de tiempo,
las mezcla y combina el resultado con el video renderizado.

```python
from gaanim import Scene

scene = Scene()
scene.assets_dir("assets")

scene.audio("music.ogg", volume=0.35)
scene.wait(1.5)
scene.audio("pop.wav", duration=0.4, volume=0.8, fade_in=0.02)

scene.export("lesson.mp4")
```

`start` es opcional. Si se omite, la fuente comienza en el cursor actual de la
línea de tiempo; usa `start=...` para situarla en un instante absoluto de la
escena. `duration` recorta la fuente y hace determinista el fundido de salida.

```python
scene.audio(
    "narration.m4a",
    start=3.0,
    duration=7.5,
    volume=0.9,
    fade_in=0.15,
    fade_out=0.25,
)
```

Por ahora, el audio solo está disponible durante la exportación: la vista
previa interactiva todavía no lo reproduce ni permite recorrerlo. MP4 usa AAC
y WebM usa Opus. Las secuencias de imágenes, GIF y WebP animado rechazan las
pistas porque esos formatos no transportan audio.

El audio embebido de `scene.video(...)` sí se reproduce en la vista previa:
se pausa, recorre y repite junto con el timeline. `audio=false` silencia ese
video y `volume` configura su ganancia. Esta reproducción interactiva no cambia
todavía el comportamiento de las pistas independientes declaradas con
`scene.audio(...)`.
