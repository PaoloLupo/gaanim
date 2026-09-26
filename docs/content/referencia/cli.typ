#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Línea de comandos",
  description: "El comando gaanim: previsualizar, crear proyectos, validar, exportar, presentar y comparar capturas",
  route: "/referencia/cli/",
  nav: "CLI",
)

= Línea de comandos

El ejecutable `gaanim` abre el editor, crea proyectos y exporta escenas. Todos
los comandos que reciben `<SCRIPT_O_PROYECTO>` aceptan la ruta de un script
Python o la carpeta de un proyecto: en una carpeta se ejecuta el `entry` de su
#link("/referencia/gaanim-toml/")[`gaanim.toml`].

```bash
gaanim                                   # abre el Inicio
gaanim mi-video                          # previsualiza con hot reload
gaanim init video mi-video               # crea un proyecto
gaanim check mi-video                    # valida sin abrir ventana
gaanim export mi-video --output exports/video.mp4
gaanim --present --monitor 1 mi-charla   # presenta a pantalla completa
gaanim --diff --example mi-video         # compara capturas con un baseline
gaanim --version
```

`gaanim --help` resume el uso y `gaanim <comando> --help` lista las opciones de
`init`, `check`, `export` y `--diff`. `--help` y `--version` funcionan aunque
no haya un Python compatible instalado; el resto de comandos necesita el
entorno de Python que Gaanim detecta junto al proyecto (ver
#link("/guias/proyectos/")[Proyectos]).

== Códigos de salida

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Código*], [*Significado*],
  [`0`], [El comando terminó bien.],
  [`1`], [`check` encontró errores (o advertencias con `--strict`), o la exportación falló.],
  [`2`], [Argumentos inválidos, proyecto o manifiesto inválido, entorno de Python no disponible, o un script que falla o no llama a `scene.render()`.],
)

= Previsualizar

```bash
gaanim [--present] [--monitor <ÍNDICE>] [--sections <LISTA>] [--from <NOMBRE>] <SCRIPT_O_PROYECTO>
```

Sin argumentos, `gaanim` abre el Inicio: crear un proyecto, abrir una carpeta o
volver a uno de los diez proyectos recientes. Con un script o proyecto abre el
editor, que vuelve a ejecutar el script al guardar (hot reload) y recarga la
escena cuando cambia un asset del proyecto.

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Opción*], [*Efecto*],
  [`--present`], [Abre la escena en modo presentación: pantalla completa sin bordes para la audiencia y Presenter View para quien presenta. Requiere `<SCRIPT_O_PROYECTO>`.],
  [`--monitor <ÍNDICE>`], [Monitor de la presentación, contado desde `0`. Sin él se usa el monitor principal. Solo es válido junto con `--present`.],
  [`--sections <LISTA>`], [Reproduce solo los segmentos indicados, separados por comas, por ejemplo `resultados,cierre`.],
  [`--from <NOMBRE>`], [Empieza en ese segmento y sigue hasta el final. Combinado con `--sections`, recorta la lista a partir de ese nombre.],
)

Cada nombre de `--sections` y `--from` puede ser el nombre completo de un
segmento, la clave de un `Section` (selecciona todos sus pasos) o el nombre de
un `SectionStep`. No distingue mayúsculas de minúsculas. Un nombre que no
selecciona nada muestra un error con los nombres disponibles y se reproduce la
escena completa. La escena se
construye entera igualmente, así que objetos, cámara y tema llegan al primer
segmento elegido en el mismo estado que en una reproducción completa; solo se
limitan la reproducción y la navegación entre pausas.

```bash
gaanim mi-charla --sections portada,resultados
gaanim --present --monitor 1 mi-charla --from resultados
```

Los atajos de teclado del modo presentación y de Presenter View están en
#link("/guias/presentaciones/")[Presentaciones].

= `gaanim init`

```bash
gaanim init <video|slides> [DIRECTORIO] [--force]
```

Crea un proyecto ejecutable. `video` genera una escena animada 16:9 y
`slides`, un starter de segmentos con notas y pausas para Presenter View. Sin
`DIRECTORIO` se usa `gaanim-video` o `gaanim-slides`.

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Archivo*], [*Contenido*],
  [`main.py`], [Escena inicial del tipo elegido.],
  [`gaanim.toml`], [Manifiesto del proyecto (ver #link("/referencia/gaanim-toml/")[`gaanim.toml`]).],
  [`pyproject.toml`], [Proyecto uv sin paquete propio (`requires-python = ">=3.14"`) que declara `gaanim` como dependencia.],
  [`.python-version`], [`3.14`.],
  [`assets/`, `exports/`], [Carpetas de recursos y de salidas, con un `.gitkeep`.],
  [`README.md`, `AGENTS.md`], [Instrucciones de uso y guía para asistentes de código.],
  [`.gitignore`], [Ignora exportaciones, capturas y vídeos generados.],
)

Después de escribir los archivos, `init` prepara `.venv` con uv e instala el
paquete de autoría de Gaanim; si no lo consigue, avisa y el proyecto sigue
siendo válido. Si algún archivo del scaffold ya existe, falla. `--force`
reescribe solo esos archivos y no borra nada de `assets/` ni de otras carpetas.

= `gaanim check`

```bash
gaanim check <SCRIPT_O_PROYECTO> [--strict]
```

Ejecuta el script sin abrir ventana y revisa la escena resultante. Imprime la
duración, el tamaño del marco y una línea por problema (`ERROR:` o `WARN:`), y
termina con `PASS` o `FAIL`.

- En toda escena: que la línea de tiempo dure algo, que no queden textos de
  plantilla que empiecen por `[` y, si no hay tema, que el fondo no sea blanco
  o casi blanco, porque el texto, las formas y los ejes sin color propio
  también son blancos y no se verían.
- Si la escena usa segmentos (una presentación): que exista al menos un
  segmento, que ninguno dure cero segundos, que cada uno tenga notas y que
  las pausas tengan nombre; avisa si el marco no es 16:9 o si la escena dura
  menos de un segundo.

Los errores devuelven el código `1`. `--strict` también devuelve `1` cuando
solo hay advertencias. Si el script lanza una excepción o no termina con
`scene.render()`, `check` lo informa y devuelve `2`.

```bash
gaanim check mi-charla --strict
```

= `gaanim export`

```bash
gaanim export <SCRIPT_O_PROYECTO> --output <ARCHIVO> [OPCIONES]
```

Renderiza la escena completa, o un tramo, a un archivo. El script no cambia:
la resolución y la calidad se eligen aquí, no en `Scene`. Las pausas
(`scene.stop()`) no detienen la exportación.

== Formatos

La extensión de `--output` elige el formato.

#table(
  columns: (auto, 1fr, auto),
  inset: 7pt,
  [*Extensión*], [*Formato*], [*Transparencia*],
  [`.mp4`], [H.264. Necesita FFmpeg.], [No],
  [`.webm`], [VP9. Necesita FFmpeg.], [Sí],
  [`.webp`], [WebP animado. Necesita FFmpeg.], [Sí],
  [`.gif`], [GIF. Necesita FFmpeg.], [No],
  [`.png`], [Secuencia de PNG, un archivo por fotograma. No necesita FFmpeg.], [Sí],
)

En una secuencia PNG, un patrón `%d` o `%0Nd` en el nombre se sustituye por el
número de fotograma desde 0 (`frames/f_%04d.png` escribe `f_0000.png`,
`f_0001.png`…). Sin patrón, el número se añade al nombre: `frame.png` escribe
`frame_00000.png`. El audio de la escena se mezcla en MP4 y WebM.

== Opciones

#table(
  columns: (auto, auto, 1fr),
  inset: 7pt,
  [*Opción*], [*Predeterminado*], [*Efecto*],
  [`-o`, `--output <ARCHIVO>`], [obligatorio], [Archivo de salida; su extensión elige el formato.],
  [`--quality <PRESET>`], [`standard`], [`draft`: 30 fps y codificación rápida. `standard`: 60 fps y equilibrio. `production`: 60 fps y la mejor compresión, más lenta.],
  [`--width <PX>`], [`1920`], [Ancho de salida en píxeles.],
  [`--height <PX>`], [`1080`], [Alto de salida en píxeles.],
  [`--fit <MODO>`], [`error`], [Qué hacer si la proporción de salida no coincide con el marco de la escena: `error` falla, `contain` añade bandas y `cover` recorta los bordes.],
  [`--encoder <CODIFICADOR>`], [`auto`], [Solo MP4: `auto`, `libx264`, `nvenc`, `amf`, `qsv` o `vaapi`.],
  [`--transparent`], [desactivado], [Conserva el canal alfa en WebM, WebP y PNG. MP4 y GIF lo rechazan.],
  [`--from <SEGUNDOS|MARCADOR>`], [`0`], [Inicio del tramo exportado.],
  [`--to <SEGUNDOS|MARCADOR>`], [fin de la escena], [Fin del tramo exportado.],
)

La composición se define en unidades lógicas, así que `--width` y `--height`
cambian la nitidez, no la disposición. Con una proporción distinta de la del
marco (por ejemplo 1080×1920 para una escena 16×9) hace falta `--fit contain`
o `--fit cover`.

Con `--encoder auto`, MP4 prueba NVENC, AMF y QSV con una codificación real y
usa el primero que funciona; si ninguno funciona, usa `libx264`. Un
codificador explícito no tiene alternativa: si falla, la exportación falla.
VAAPI nunca se prueba automáticamente, porque un driver defectuoso puede
bloquear la GPU.

`--transparent` solo tiene efecto si la escena tiene un fondo con alfa, por
ejemplo `Scene(background="#00000000")`.

== Tramos y marcadores

`--from` y `--to` aceptan segundos o el nombre de un
#link("/referencia/scene/#api-scene-marker")[`scene.marker`], y se pueden
combinar. Los nombres se resuelven después de ejecutar el script; uno que no
existe produce un error con los marcadores definidos. El audio se recorta al
mismo tramo, una secuencia PNG numera sus fotogramas desde 0 y un `--to`
posterior al final se limita a la duración de la escena. `--to` debe quedar
después de `--from`.

```bash
gaanim export mi-video --output exports/video.mp4 --quality production
gaanim export mi-video --output exports/vertical.mp4 --width 1080 --height 1920 --fit cover
gaanim export overlay.py --output overlay.webm --transparent
gaanim export mi-video --output exports/tramo.mp4 --from 12 --to 15
gaanim export mi-video --output exports/climax.mp4 --from climax --to fin
gaanim export mi-video --output frames/f_%04d.png --quality draft
```

= `gaanim --diff`

```bash
gaanim --diff --example <SCRIPT_O_PROYECTO> [OPCIONES]
gaanim --diff --baseline <DIR> --current <DIR> [OPCIONES]
```

Captura fotogramas exactos de una escena y los compara con un baseline
aprobado; guarda un informe JSON y HTML y abre un visor. Las capturas de
`--example escenas/intro.py` viven en `tests/visual/escenas/intro/`
(`baseline/`, `current/` y `report/`). El flujo completo está en
#link("/guias/capturas-y-comparacion/")[Capturas y comparación visual].

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Opción*], [*Efecto*],
  [`-e`, `--example <SCRIPT_O_PROYECTO>`], [Captura la escena y la compara con su baseline.],
  [`--bless`], [Guarda la captura como baseline y termina. Sobrescribe el baseline anterior.],
  [`--capture-only`], [Escribe solo `current/` y termina, sin comparar.],
  [`--no-capture`], [Compara los PNG que ya están en `current/`.],
  [`--capture-stops`], [Captura el fotograma de cada `scene.stop()` en lugar de los tiempos de `scene.snapshots`; el script no necesita cambios.],
  [`--stops <LISTA>`], [Con `--capture-stops`, solo esas pausas: números y rangos contados desde 1, como `3-7,12`.],
  [`--sections <LISTA>`, `--from <NOMBRE>`], [Con `--capture-stops`, solo las pausas de esos segmentos, con las mismas reglas que al previsualizar.],
  [`--tests-root <DIR>`], [Carpeta raíz de las capturas; por defecto `tests/visual`.],
  [`-b`, `--baseline <DIR>`, `-c`, `--current <DIR>`], [Modo manual: compara dos carpetas cualesquiera.],
  [`-o`, `--output <DIR>`], [Carpeta del informe.],
  [`--pixel-threshold <0..255>`], [Diferencia por canal que se ignora.],
  [`--max-changed-ratio <0..1>`], [Fracción de píxeles distintos que se tolera.],
  [`--no-gui`], [Genera el informe sin abrir el visor, por ejemplo en integración continua.],
)

Sin `--capture-stops`, el script elige los instantes: cuando existe la
variable de entorno `GAANIM_SNAPSHOTS`, debe llamar a
#link("/referencia/scene/#api-scene-snapshots")[`scene.snapshots`] con esa ruta.

```bash
gaanim --diff --example escenas/intro.py --bless
gaanim --diff --example escenas/intro.py --no-gui --pixel-threshold 4
gaanim --diff --example mi-charla --capture-stops --stops 3-7 --capture-only
```

= Variables de entorno

#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Variable*], [*Efecto*],
  [`GAANIM_SNAPSHOTS`], [La define `gaanim --diff` con la carpeta donde `scene.snapshots` debe capturar. No la definas a mano.],
  [`GAANIM_INCREMENTAL=0`], [Desactiva la recompilación incremental del hot reload: cada guardado recompila la escena entera.],
)
