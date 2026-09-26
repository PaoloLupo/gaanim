#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Manifiesto gaanim.toml",
  description: "Campos, valores predeterminados y validación del manifiesto de proyecto",
  route: "/referencia/gaanim-toml/",
  nav: "gaanim.toml",
)

= Manifiesto `gaanim.toml`

Una carpeta es un proyecto de Gaanim cuando contiene un `gaanim.toml`. El
manifiesto dice qué script ejecutar y dónde están los recursos y las
salidas. `gaanim init` lo genera así:

```toml
name = "mi-charla"
kind = "slides"
entry = "main.py"
assets_dir = "assets"
output_dir = "exports"
```

== Campos

#table(
  columns: (auto, auto, auto, 1fr),
  inset: 7pt,
  [*Campo*], [*Tipo*], [*Predeterminado*], [*Significado*],
  [`kind`], [cadena], [obligatorio], [Tipo de proyecto: `"video"` o `"slides"`.],
  [`entry`], [ruta], [obligatorio], [Script que ejecutan `gaanim <carpeta>`, `check` y `export`.],
  [`name`], [cadena], [nombre de la carpeta], [Nombre que muestran el Inicio y el título de la ventana.],
  [`assets_dir`], [ruta], [`"assets"`], [Carpeta de recursos, relativa al manifiesto. La lee `scene.assets.load_project()`.],
  [`output_dir`], [ruta], [`"exports"`], [Carpeta donde el diálogo de exportación del editor propone guardar.],
)

Las claves desconocidas se ignoran.

=== `kind`

`"video"` y `"slides"` distinguen los dos tipos de proyecto en el Inicio y en
el starter de `gaanim init`. No cambian cómo se ejecuta ni cómo se exporta la
escena: `gaanim check` aplica las comprobaciones de presentación cuando la
escena usa segmentos, sea cual sea su `kind`. Los valores antiguos
`"presentation"` y `"thesis"` producen un error que pide cambiarlos por
`"slides"`; cualquier otro valor, un error con los tipos disponibles.

=== `entry`

Ruta relativa a la carpeta del proyecto. Debe apuntar a un archivo que exista
y no puede ser absoluta ni salir de la carpeta con `..`. Así, `gaanim .`,
`gaanim check .` y `gaanim export . --output …` no necesitan la ruta del script.

=== `assets_dir`

El editor y la CLI no aplican `assets_dir` por su cuenta: la escena lo carga al
llamar a `scene.assets.load_project()`, que busca el `gaanim.toml` junto al
script. Después, las rutas relativas de `scene.media.image`, `svg`, `lottie`,
`gltf` y demás se resuelven desde esa carpeta, sin depender del directorio de
trabajo desde el que se lanzó Gaanim.

```python
from gaanim import Scene

scene = Scene()
scene.assets.load_project()             # lee assets_dir de ./gaanim.toml
logo = scene.media.image("logo.webp")   # es assets/logo.webp
```

`load_project` lee solo una línea `assets_dir = "…"` con comillas dobles; si el
manifiesto no la tiene, lanza `ValueError` aunque la CLI use `"assets"` como
valor predeterminado. Escríbela siempre. Consulta
#link("/referencia/assets/#api-assetmanager-load-project")[`AssetManager.load_project`].

=== `output_dir`

Carpeta relativa al proyecto, o absoluta. El diálogo de exportación del editor
propone `<output_dir>/output.mp4`. `gaanim export` no la usa: escribe en la
ruta de `--output`, relativa al directorio actual.

== Cómo se encuentra el proyecto

- `gaanim <carpeta>` exige un `gaanim.toml` en esa carpeta.
- `gaanim <script.py>` ejecuta ese script y busca un `gaanim.toml` en su
  carpeta y en hasta cinco carpetas superiores para mostrar el nombre del
  proyecto y proponer la carpeta de exportación.

Al abrir una carpeta, un manifiesto que no es TOML válido, sin `kind` o
`entry`, o con valores inválidos detiene `gaanim`, `check` y `export` con un
mensaje que incluye la ruta del manifiesto y el código de salida `2`. Al abrir
un script, un manifiesto inválido en una carpeta superior se ignora.
