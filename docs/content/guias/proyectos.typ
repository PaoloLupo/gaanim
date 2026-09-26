#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Proyectos y exportación",
  description: "Organiza escenas, recursos y salidas en un proyecto; compruébalo y expórtalo",
  route: "/guias/proyectos/",
)

En esta guía aprenderás a trabajar con un proyecto de Gaanim de principio a
fin: qué contiene, cómo carga sus recursos, qué pasa cuando guardas un
archivo, cómo comprobarlo sin abrir ventana y cómo exportarlo al formato que
necesites.

Si todavía no tienes Gaanim instalado ni un proyecto creado, empieza por
#link("/empezar/instalacion/")[Instalación]: allí están `gaanim init`, la
estructura que genera y cómo se prepara el entorno de Python.

= Por qué un proyecto

Gaanim puede abrir un script suelto (`gaanim escena.py`), pero un proyecto
mantiene juntos el código, los recursos y las exportaciones, y se abre igual
desde cualquier directorio. Hay dos tipos:

- `video`: una escena animada 16:9 pensada para exportarse.
- `slides`: segmentos con nombre, notas y paradas para presentar en vivo
  (ver #link("/guias/presentaciones/")[Presentaciones]).

`gaanim` sin argumentos abre el Inicio, desde donde puedes crear un proyecto,
abrir una carpeta o volver a uno de los diez proyectos recientes. Los scripts
sueltos no se añaden a esa lista.

= El manifiesto `gaanim.toml`

Cada proyecto tiene un `gaanim.toml` en su raíz:

```toml
name = "mi-charla"
kind = "slides"
entry = "main.py"
assets_dir = "assets"
output_dir = "exports"
```

- `entry` es el script que se ejecuta. Debe ser una ruta relativa que quede
  dentro del proyecto.
- `kind` solo acepta `video` o `slides`.
- `assets_dir` es la carpeta de recursos que carga la escena.
- `output_dir` es la carpeta sugerida para las exportaciones.

La #link("/referencia/gaanim-toml/")[referencia de `gaanim.toml`] describe
cada clave.

`--force` en `gaanim init` actualiza los archivos conocidos del proyecto sin
borrar tus recursos ni otras carpetas.

= Cargar los recursos del proyecto

Para que las rutas de imágenes, SVG, audio o modelos no dependan del
directorio desde el que se abre Gaanim, carga el manifiesto al principio de la
escena. Después, las rutas se resuelven dentro de `assets_dir`:

```python
from gaanim import Scene

scene = Scene(frame=(16, 9))
scene.assets.load_project("gaanim.toml")
logo = scene.media.svg("logo.svg")  # busca assets/logo.svg
scene.play([logo.animate.fade_in().duration(0.5)])
scene.render()
```

Si no usas manifiesto, `scene.assets.assets_dir("assets")` fija la carpeta de
recursos directamente. Consulta #link("/referencia/assets/")[Recursos] para
los formatos admitidos.

= Abrir, comprobar y presentar

Puedes pasar la carpeta del proyecto en lugar de la ruta de `main.py`:

```bash
gaanim mi-charla                      # vista previa con recarga al guardar
gaanim check mi-charla                # ejecuta la escena sin ventana
gaanim check mi-charla --strict       # falla también con avisos
gaanim --present --monitor 1 mi-charla
```

Dentro del proyecto, `.` es la carpeta actual: `gaanim .` o `gaanim check .`.

`gaanim check` ejecuta el script completo, calcula la duración y revisa los
segmentos, las notas y las paradas si las hay; en proyectos `slides` también
comprueba el formato 16:9 y los marcadores de plantilla sin rellenar. Úsalo
antes de exportar o en integración continua.

= Qué pasa al guardar

La vista previa vuelve a ejecutar la escena cada vez que guardas.

== Recarga incremental

Los segmentos anteriores al primero que cambió conservan su escena compilada
y solo se recompila desde ahí. El aviso de recarga lo indica, por ejemplo
`replay 0.12s (reused 37/40 segments)`. La primera edición en una zona nueva
recompila todo una vez; las siguientes en esa zona ya son incrementales.

Se recompila todo cuando:

- cambia algo global (tema, fuentes, tamaño del lienzo);
- un segmento posterior modifica objetos creados antes, por ejemplo al agrupar
  un logo que aparece desde el principio;
- desde cualquier segmento que use funciones de Python en tiempo de ejecución
  (animaciones propias, updaters, funciones reactivas o easings propios).

El resultado siempre es idéntico al de una recarga completa. Para forzar esta
última, inicia Gaanim con la variable de entorno `GAANIM_INCREMENTAL=0`.

== Recursos

Guardar cualquier archivo que no sea Python dentro del proyecto (imágenes,
SVG, Lottie, glTF, WGSL, fuentes, documentos Typst, datos) vacía las cachés de
recursos, vuelve a ejecutar el script y recompila todos los segmentos.

Se ignoran los archivos y carpetas ocultos (`.git`, `.venv`, archivos de
intercambio del editor), `venv`, `env`, `__pycache__`, `exports`,
`snapshots`, `target`, los temporales (`~`, `.swp`, `.tmp`, `.bak`) y los
`.lock`.

== Módulos de Python del proyecto

Puedes repartir el código en varios módulos. Antes de cada ejecución, Gaanim
descarta de `sys.modules` los módulos cuyo archivo está dentro del proyecto
(salvo en las carpetas ignoradas), así que guardar un `.py` importado por
`main.py` lo vuelve a importar. Los paquetes instalados y el código fuera del
proyecto siguen en caché hasta reiniciar Gaanim.

La raíz del proyecto, su carpeta `src/` y la carpeta del script están en
`sys.path`, y `__file__`, `sys.argv[0]` y `sys.path` usan rutas normales, sin
el prefijo `\\?\` de Windows.

= Exportar

Para exportar no cambias el script: se lo pides al ejecutable.

```bash
gaanim export mi-video --output mi-video/exports/final.mp4
```

La extensión de `--output` elige el formato:

#table(
  columns: 3,
  table.header[*Extensión*][*Formato*][*Necesita FFmpeg*],
  [`.mp4`], [video H.264, con el audio de la escena], [sí],
  [`.webm`], [video VP9, con audio; admite transparencia], [sí],
  [`.webp`], [WebP animado; admite transparencia], [sí],
  [`.gif`], [GIF animado], [sí],
  [`.png`], [secuencia de imágenes, una por fotograma; admite transparencia], [no],
)

En una secuencia PNG, un `%d` o `%0Nd` en el nombre se sustituye por el número
de fotograma (`frames/f_%04d.png` produce `f_0000.png`, `f_0001.png`…); si no,
el número se añade al final (`frame_00000.png`).

== Opciones

- `--quality draft` exporta rápido a 30 fps para revisar; `standard` (la
  predeterminada) usa 60 fps y `production`, la mejor codificación.
- `--width` y `--height` eligen la resolución en píxeles (1920 × 1080 por
  defecto). La escena no cambia: sus unidades lógicas se escalan.
- `--fit contain` o `--fit cover` permiten una salida con otra proporción que
  la escena: `contain` añade bandas y `cover` recorta. Sin `--fit`, una
  proporción distinta es un error.
- `--from` y `--to` exportan solo un tramo. Aceptan segundos o el nombre de un
  marcador creado con `scene.marker("nombre")`; el audio se recorta al tramo.
- `--transparent` conserva el canal alfa en WebM, WebP y PNG; la escena
  necesita un fondo transparente.
- `--encoder` elige el codificador de MP4: `auto` (predeterminado) prueba los
  de hardware y recurre a `libx264`; también acepta `libx264`, `nvenc`, `amf`,
  `qsv` y `vaapi`, que no recurren a otro si fallan.

`gaanim export --help` y la #link("/referencia/cli/")[referencia de la línea
de comandos] listan todas las opciones. Las paradas
(`scene.stop(...)`) no detienen la exportación: el resultado es un video
continuo.

= Convertir un script suelto en proyecto

Crea un proyecto con `gaanim init video mi-video`, reemplaza su `main.py` por
tu script, añade `scene.assets.load_project("gaanim.toml")` justo después de
crear la `Scene` y mueve los recursos a `assets/`.

Si algo falla al abrir o exportar, consulta
#link("/apendices/solucion-de-problemas/")[Solución de problemas].
