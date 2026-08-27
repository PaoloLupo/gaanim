#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Presentaciones",
  description: "Crea, previsualiza y presenta diapositivas semánticas reutilizables",
  route: "/guides/slides/",
  updated: datetime.today().display(),
  code-langs: (),
)

Los proyectos `slides` son el punto de partida general para clases, charlas,
demos y presentaciones técnicas. La identidad visual pertenece al proyecto: el
starter no incluye colores, fuentes ni contenido de una institución concreta.

= Crear el proyecto

```powershell
gaanim init slides mi-charla
gaanim mi-charla
```

También puedes ejecutar `gaanim` sin argumentos, elegir *Nuevo proyecto* y
seleccionar *Slides*. El Inicio detecta Python, `.venv` y uv antes de abrir.

= Estructura semántica

```python
from gaanim import Scene, lecture, title_slide

scene = Scene(1920, 1080, margin=72)
scene.canvas.set_theme("presentation")
scene.slides.brand(footer="MI CHARLA", slide_numbers=True, rule=True)

cover = scene.segment("Portada", template=title_slide, notes="Presenta el tema.")
cover.bind(
    title=scene.text("Una idea clara", role="title"),
    subtitle=scene.text("Slides reutilizables", role="subtitle"),
)
scene.wait(0.5)
scene.stop("portada")

content = scene.segment("Contenido", template=lecture, notes="Desarrolla la idea.")
content.bind(
    title=scene.text("Contenido", role="title"),
    body=scene.text("Una sola idea por slide.", size=42),
)
scene.wait(0.5)
scene.stop("mensaje")
scene.render()
```

`scene.slides.brand(...)` configura logo, footer, numeración y regla para todo el deck.
Las plantillas `title_slide`, `lecture`, `comparison` y `credits` devuelven un
`Layout` raíz responsive y pueden reemplazarse por funciones Python propias.

= Presentar y validar

```powershell
gaanim check mi-charla --strict
gaanim --present --monitor 1 mi-charla
```

Los índices de monitor empiezan en cero. Presenter View utiliza los nombres y
notas de `scene.segment(...)`, y solo espera input en los puntos marcados con
`scene.stop(...)`. La exportación ignora esas paradas y genera un video continuo.

El cockpit muestra el cue actual, las notas y el siguiente stop. Usa `Right`,
`Space`, `Enter` o clic en la pantalla de audiencia para avanzar;
`Left`/`Backspace` para volver; `O` para buscar cues; y `B`/`W` para apagar la
audiencia en negro o blanco. Si cierras Presenter View, la presentación sigue
en fullscreen y `P` vuelve a abrir el cockpit. `Esc` cierra primero el overview
o blanking activo y después sale del modo presentación. Las previews se
conservan al reabrir Presenter View y se renderizan a una resolución adaptada al
tamaño y DPI de la ventana. El encabezado muestra un cronómetro reiniciable para
  medir la exposición y, en menor tamaño, la hora local.
  La pantalla fullscreen muestra un dock compacto de reproducción con navegación
  anterior, avance o pausa, inicio, fin y progreso al llevar el cursor a su zona
  inferior. El dock se oculta al retirar el cursor o cambiar el foco al cockpit.
  Sus
  botones y los atajos pasan por las mismas acciones, por lo que un clic en el
  dock no avanza dos veces. Presenter View titula el cue activo, evita repetir el
  nombre del segmento y mantiene Up Next por encima de las notas con scroll
  independiente.

La salida fullscreen, tanto en el editor como en Presenter Mode, ajusta el lienzo
completo al monitor sin deformarlo y rellena en negro cualquier franja exterior,
independientemente del fondo de la escena. En el fullscreen normal del editor,
`Esc` también restaura la ventana, igual que `F11`. Si el render asíncrono de
previews falla, Presenter View permite
reintentarlo; al terminar la charla muestra un estado final en vez de una preview
pendiente que nunca puede existir.

Para revisar una animación sin pausas en el editor, activa *Continuous* junto a
los controles de transporte. El toggle dura la sesión y sobrevive al hot reload,
pero Presenter Mode sigue respetando `scene.stop(...)`. Los seeks, snapshots y
la exportación también continúan ignorando stops como antes.

= Entorno opcional con uv

El visor embebe Gaanim y puede usar un Python compatible del sistema. Si el
proyecto no contiene `.venv`, el Inicio muestra el runtime detectado y permite
abrir igualmente. Para aislar dependencias, copia y ejecuta fuera de Gaanim:

```powershell
cd mi-charla
uv venv --python 3.12
```

Gaanim nunca ejecuta esas instrucciones ni modifica el entorno por su cuenta.
