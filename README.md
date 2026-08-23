# Gaanim

Motor de animación vectorial 2D acelerado por GPU, escrito en Rust y diseñado
para crear escenas programáticas desde Python. El flujo actual ejecuta los
scripts Python dentro de la aplicación `gaanim`, que proporciona la ventana de
previsualización, hot reload y exportación.

Gaanim se distribuye en dos piezas complementarias:

- El ejecutable `gaanim`, que es el único runtime y proporciona ejecución,
  previsualización, hot reload, render y exportación.
- El wheel de autoría, que solo instala helpers Python, stubs y `py.typed` para
  el entorno del proyecto. No contiene renderer, extensión nativa ni un modo
  headless independiente. Requiere Python 3.12 o superior.

## Inicio rápido

Requisitos: Rust (toolchain estable), Python y [`just`](https://just.systems).
En Windows, ejecute PowerShell desde la raíz del repositorio:

```powershell
just bootstrap
just doctor
just run quickstart
```

En Linux y macOS los mismos comandos funcionan desde una terminal. El primer
build puede tardar porque compila Bevy, Vello y la aplicación nativa. `doctor`
comprueba el workspace y que la aplicación pueda iniciar; `run quickstart`
abre la escena de ejemplo en el visor.

El ejemplo ejecutado es [`examples/quickstart.py`](examples/quickstart.py):

```python
from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background=BLACK, margin=48)
circle = scene.circle(96).fill(BLUE).stroke(WHITE, 4)
title = scene.text("Hola, Gaanim", role="title").fill(GOLD).at(0, 180)

scene.play([circle.create().duration(0.8), title.write().duration(0.6)])
scene.play([circle.move(240, 0).duration(1.0).smooth()])
scene.render()
```

Para crear una escena nueva, guarde un script en `examples/` y ejecútelo con
`just run nombre_del_script` (sin `.py`). Durante la previsualización, guardar
el archivo recarga la escena.

## Plataformas y artefactos

| Plataforma | CI | Artefacto instalable | Estado declarado |
| --- | --- | --- | --- |
| Windows 10/11 x64 | Sí | Zip con launcher, core y wheel de autoría | Soportada en `0.1.x` |
| Ubuntu 24.04 x64 | Sí | Tarball con launcher, core y wheel de autoría | Soportada en `0.1.x` |
| macOS | No | No | Experimental, sin garantía de release |

El wheel `py3-none-any` es el mismo en todas las plataformas porque no contiene
runtime. Esta tabla se refiere al ejecutable, que es lo necesario para abrir o
exportar una escena.

En Ubuntu, descargue `gaanim-v<versión>-linux-x64.tar.gz`, extráigalo y copie
`gaanim` y `gaanim-core` juntos a una carpeta de `PATH`, por ejemplo
`~/.local/bin`. Requiere Python 3.12 y las bibliotecas de sistema de Ubuntu
24.04; FFmpeg sigue siendo opcional salvo para video y audio.

El binario de usuario también administra proyectos. `gaanim` sin argumentos
abre el Inicio con creación, apertura, diagnóstico de Python/uv y hasta diez
proyectos recientes. Los únicos scaffolds generales son:

```powershell
gaanim init video mi-video
gaanim init slides mi-charla
gaanim .
```

Cada `gaanim init` crea un proyecto uv mínimo con Python 3.14, declara `gaanim`
en las dependencias de `pyproject.toml`, deja `.python-version` y prepara
`.venv` con esa misma versión. Los proyectos sin `.venv` pueden usar un Python
3.14+ detectado en el sistema.

Las presentaciones usan el mismo concepto de segmento que los videos. Los
límites son continuos y solo `stop()` solicita input durante la reproducción:

```python
from gaanim import Anchor, Scene

scene = Scene(1280, 720)
intro = scene.segment("Introducción", notes="Presenta el objetivo", layout="cover")
intro.region("title").place(scene.text("Una idea clara", role="title"), Anchor.CENTER)
scene.play([scene.text("Resultado").write().duration(0.5)])
scene.stop("resultado")
```

La exportación ignora los stops. Para exportar un segmento concreto use
`gaanim export . --output intro.mp4`.

El editor usa un único playback flotante en lugar de una timeline detallada.
`Space` alterna play/pausa, las flechas navegan entre segmentos y `L` activa el
loop del segmento actual, salta a su inicio e ignora sus `stop()` hasta apagar
el loop. Los tiradores de la barra permiten refinar el rango dentro de ese
segmento. **Continuous** reproduce la escena completa sin detenerse en
`stop()`; es una preferencia temporal de la sesión y no cambia el script.

El playback reduce márgenes y controles según el ancho disponible; en ventanas
estrechas mueve velocidad, Continuous, fullscreen, Present, Export y Pin al
menú **More**. `F11` alterna fullscreen del editor en el monitor actual sin
cambiar el playback ni activar Presenter Mode. **Present** sigue siendo un modo
independiente, pensado para audiencia y con su propio dock seguro.

Presenter Mode siempre respeta los stops. Durante una presentación,
`Right`, `Space`, `Enter` o un clic en la pantalla de audiencia avanzan;
`Left`/`Backspace` retroceden, `O` abre el overview y `B`/`W` controlan el
blanking. Cerrar Presenter View mantiene la audiencia activa y `P` vuelve a
abrir el cockpit sin regenerar sus previews; `Esc` sale del modo presentación.
  El encabezado incluye un cronómetro de exposición reiniciable y la hora local.
  La pantalla fullscreen revela un dock compacto con Previous, Advance/Pause,
  inicio, fin y progreso al llevar el cursor a su zona inferior; se oculta al
  retirar el cursor o perder foco. Presenter View
  identifica el cue activo sin repetir el nombre del segmento y mantiene Up Next
  por encima de las notas con scroll independiente.

## 3D nativo e inspección

Gaanim incluye `cube`, `sphere`, `cylinder`, `cone` y `plane` como mallas PBR
animables, con `Material3D.matte`, `Material3D.metal` y
`Material3D.emissive`. `scene.lighting_3d("studio")` proporciona un único rig
de estudio. Consulta `examples/primitives_3d_demo.py` para una escena completa.

Las escenas 2D y 3D abren con el modo interactivo desactivado. Cuando el usuario
pulsa `I` o usa **Interactivo: ON/OFF** en Overlays (`O`), la cámara interactiva
comienza siempre como una copia fresca de `scene.camera` en el tiempo actual;
una inspección anterior nunca se reutiliza. La copia es independiente, por lo
que orbitar, desplazar o hacer dolly no altera el timeline, snapshots, Presenter
View ni la exportación. Con la interacción activa, `Num0` alterna **Free 3D** /
**Camera View**; `F` encuadra y `R` reinicia. El picking conserva internamente
la selección sin dibujar un bounding box sobre el objeto. El marco de salida
mantiene la resolución y relación de aspecto declaradas por la escena.

El snapping temporal del editor permanece desactivado mientras haya contenido
3D. Las escenas puramente 2D conservan el comportamiento habitual.

## Paquete Python local

El wheel instala helpers y stubs para autocompletado; el binding nativo y todo
el runtime viven exclusivamente dentro del ejecutable:

```powershell
just bootstrap
just python-develop
```

Para construir una wheel distribuible use `just wheel`; el resultado se escribe
en `target/wheels/`. Es un wheel universal `py3-none-any` sin renderer ni
extensión nativa. Ejecutar o importar escenas con Python plano produce un error
que dirige al usuario a la aplicación `gaanim`.

Esta separación es un contrato de producto, no una limitación temporal del
empaquetado: no se publicará un wheel autónomo. Los proyectos declaran el wheel
para obtener la experiencia de autoría, pero siempre se ejecutan con
`gaanim <script.py>` o `gaanim export ...`.

Después de `just python-develop`, ejecute `just validate-python-api` para
comprobar que el stub tipado público sigue coincidiendo con el módulo PyO3
embebido por el ejecutable.

## Exportar

Mantenga `scene.render()` al final del script y solicite la exportación al
ejecutable:

```powershell
gaanim export . --output output.mp4 --quality standard
gaanim export . --output overlay.webm --transparent
```

Se admiten MP4, WebM, WebP animado, GIF y secuencias PNG. La exportación de
video requiere FFmpeg disponible en `PATH`; si no está instalado, use primero
una secuencia PNG o instale FFmpeg según su plataforma.
`--transparent` está disponible para WebM, WebP y PNG; MP4 y GIF se rechazan
explícitamente porque no forman parte del contrato alpha de Gaanim.

También puede colocar un MP4 dentro de la escena. El clip es un `Drawable`,
responde al seek del editor y permite trim, loop, velocidad y audio embebido:

```python
clip = scene.video("assets/clip.mp4", width=720, loop=True, volume=0.8)
scene.wait(8)
```

Esta función requiere `ffmpeg` y `ffprobe` disponibles en `PATH`.

## Comandos de desarrollo

| Objetivo | Comando |
| --- | --- |
| Comprobar todo el workspace | `just check` |
| Ejecutar Clippy | `just clippy` |
| Verificar entorno y aplicación | `just doctor` |
| Compilar la aplicación | `just build` |
| Ejecutar un ejemplo | `just run quickstart` |
| Medir presupuestos del runtime | `just benchmark smoke` |
| Generar la documentación | `just docs` |

La API pública de Python comienza en `Scene`; `Canvas(...)` es un constructor
de compatibilidad deprecado. Consulte la documentación del sitio en
[`docs/`](docs/) y las escenas de referencia en [`examples/`](examples/).

## Estado

Gaanim está en fase alfa (`0.1.0`). La base de render, timeline, texto y
ecuaciones es funcional; la cobertura de API, pruebas de exportación y
capacidades multimedia continúan en desarrollo. El plan de evolución se
encuentra en [`engine_improvements.md`](engine_improvements.md).
