#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Slides",
  description: "Crea, previsualiza y presenta decks semánticos reutilizables",
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
from gaanim import Anchor, Scene

scene = Scene(1920, 1080, margin=72)
scene.canvas.set_theme("presentation")
scene.brand(footer="MI CHARLA", slide_numbers=True, rule=True)

cover = scene.segment("Portada", layout="cover", notes="Presenta el tema.")
cover.region("title").place(scene.title("Una idea clara"), Anchor.CENTER)
cover.region("subtitle").place(scene.subtitle("Slides reutilizables"), Anchor.CENTER)
scene.wait(0.5)
scene.stop("portada")

content = scene.segment("Contenido", layout="content", notes="Desarrolla la idea.")
content.region("title").place(scene.title("Contenido"), Anchor.LEFT)
content.region("content").place(
    scene.paragraph("Una sola idea por slide.", width=1000, font_size=42),
    Anchor.CENTER,
)
scene.wait(0.5)
scene.stop("mensaje")
scene.render()
```

`scene.brand(...)` configura logo, footer, numeración y regla para todo el deck.
Los layouts `cover`, `content`, `comparison` y `conclusion` aportan regiones,
pero siguen siendo APIs generales que puedes combinar con cualquier tema.

= Presentar y validar

```powershell
gaanim check mi-charla --strict
gaanim --present --monitor 1 mi-charla
```

Los índices de monitor empiezan en cero. Presenter View utiliza los nombres y
notas de `scene.segment(...)`, y solo espera input en los puntos marcados con
`scene.stop(...)`. La exportación ignora esas paradas y genera un video continuo.

= Entorno opcional con uv

El visor embebe Gaanim y puede usar un Python compatible del sistema. Si el
proyecto no contiene `.venv`, el Inicio muestra el runtime detectado y permite
abrir igualmente. Para aislar dependencias, copia y ejecuta fuera de Gaanim:

```powershell
cd mi-charla
uv venv --python 3.12
```

Gaanim nunca ejecuta esas instrucciones ni modifica el entorno por su cuenta.
