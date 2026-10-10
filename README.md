<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/brand/gaanim-logo-dark.svg">
    <img src="docs/assets/brand/gaanim-logo.svg" alt="Gaanim" width="284" height="64">
  </picture>
</h1>

Motor de animación vectorial 2D acelerado por GPU, escrito en Rust y diseñado
para crear escenas programáticas desde Python. El flujo actual ejecuta los
scripts Python dentro de la aplicación `gaanim`, que proporciona la ventana de
previsualización, hot reload y exportación.

Gaanim se distribuye en dos piezas complementarias:

- El ejecutable `gaanim`, que es el único runtime y proporciona ejecución,
  previsualización, hot reload, render y exportación.
- El wheel de autoría, que solo instala helpers Python, stubs, `py.typed` y las
  fuentes Typst de la documentación (`gaanim/_docs`) para el entorno del proyecto. No contiene renderer, extensión nativa ni un modo
  headless independiente. Requiere Python 3.14 o superior.

La documentación (guía, recetas y referencia de la API) se publica en
<https://paololupo.github.io/gaanim/>. El workflow `Docs` la reconstruye en
cada push a `main`, renderizando las vistas previas de los ejemplos con el
runtime real.

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

scene = Scene(frame=(16, 9), background=BLACK, margin=0.6)
circle = scene.geometry.circle(1.2).fill(BLUE).stroke(WHITE, 0.05)
title = scene.text("Hola, Gaanim", role="title").fill(GOLD).move_to(0, 2.25)

scene.play([circle.animate.create().duration(0.8), title.animate.write().duration(0.6)])
scene.play([circle.animate.shift_by(3, 0).duration(1.0).smooth()])
scene.render()
```

Para crear una escena nueva, guarde un script en `examples/` y ejecútelo con
`just run nombre_del_script` (sin `.py`). Durante la previsualización, guardar
el archivo recarga la escena.

### Composición sin coordenadas

`scene.layout` compone la escena con cajas que funcionan como en CSS: padding,
gap, bordes, radio, sombra, filas, columnas y grids que miden el texto y
reparten el espacio. Los cambios de estructura se animan solos:

```python
from gaanim import BoxStyle, Scene

scene = Scene(theme="paper")
L = scene.layout
L.classes(pill=BoxStyle(padding=("6px", "16px"), radius="full", background="#4f46e5", color="white"))

card = L.box(
    L.box("Panel semanal", font_size="44px", weight=700),
    L.row(L.box("Visitas", class_="pill"), L.box("Ventas", class_="pill"), gap="10px"),
    padding="28px", gap="16px", radius="24px", background="white", shadow=True,
)
scene.play([card.animate.fade_in().duration(0.5)])
card[1].add(L.box("Nuevo", class_="pill", background="#16a34a"), duration=0.6)
scene.render()
```

La [guía de Layout](https://paololupo.github.io/gaanim/guias/layout/) cubre
grids, estilos, zonas y componentes; `examples/ui_*.py` construye un panel de
métricas, una app de chat y un rótulo de vídeo.

## Plataformas y artefactos

| Plataforma | CI | Artefacto instalable | Estado declarado |
| --- | --- | --- | --- |
| Windows 10/11 x64 | Sí | Zip con `gaanim.exe`, sus bibliotecas y wheel de autoría | Soportada en `0.2.x` |
| Ubuntu 24.04 x64 | Sí | Tarball con `gaanim`, sus bibliotecas y wheel de autoría | Soportada en `0.2.x` |
| macOS | No | No | Experimental, sin garantía de release |

El wheel `py3-none-any` es el mismo en todas las plataformas porque no contiene
runtime. Esta tabla se refiere al ejecutable, que es lo necesario para abrir o
exportar una escena.

Cada release se publica al subir un tag `v<versión>`. Su título y sus notas
salen de `.github/release-notes/v<versión>.md`: la primera línea,
`# Título`, nombra el release y el resto es el texto que se muestra. Sin ese
archivo el release se llama como el tag y lleva las notas que genera GitHub.

En Ubuntu, descargue `gaanim-v<versión>-linux-x64.tar.gz`, extráigalo completo
en una carpeta, por ejemplo `~/.local/lib/gaanim`, y enlace el ejecutable
`gaanim` desde una carpeta de `PATH` como `~/.local/bin`: el ejecutable busca
sus bibliotecas (`libgaanim_engine.so`, `libgaanim_python_plugin.so` y el `std`
de Rust) junto a él. Para ejecutar scripts requiere exactamente Python 3.14
(el soporte de Python de Linux enlaza `libpython3.14.so`; por ejemplo
`uv python install 3.14`) y las bibliotecas de
sistema de Ubuntu 24.04; FFmpeg sigue siendo opcional salvo para video y audio.
En Windows sirve cualquier Python 3.14 o superior.

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

Los proyectos pueden crecer con un layout `src/`: Gaanim añade automáticamente
`src` y la carpeta del entrypoint a la ruta de imports. Durante la
previsualización observa todos los archivos Python del proyecto y vuelve a
importar sus módulos al guardar, por lo que no hacen falta modificaciones
manuales de `sys.path` en `main.py`.

Las presentaciones usan el mismo concepto de segmento que los videos. Los
límites son continuos y solo `stop()` solicita input durante la reproducción:

```python
from gaanim import Scene, title_slide

scene = Scene()
intro = scene.segment("Introducción", notes="Presenta el objetivo", template=title_slide)
intro.bind(title=scene.text("Una idea clara", role="title"))
scene.stop("resultado")
```

La exportación ignora los stops. Para exportar un segmento concreto use
`gaanim export . --output intro.mp4`.

El editor usa un único playback flotante en lugar de una timeline detallada.
`Space` alterna play/pausa (nunca salta al siguiente stop), las flechas navegan
entre segmentos y `L` activa el
loop del segmento actual, salta a su inicio e ignora sus `stop()` hasta apagar
el loop. Los tiradores de la barra permiten refinar el rango dentro de ese
segmento. **Continuous** reproduce la escena completa sin detenerse en
`stop()`; es una preferencia temporal de la sesión y no cambia el script.

El playback reduce márgenes y controles según el ancho disponible; en ventanas
estrechas mueve velocidad, Continuous, fullscreen, Present, Export y Pin al
menú **More**. `F11` alterna fullscreen del editor en el monitor actual sin
cambiar el playback ni activar Presenter Mode, y `P` fija la ventana encima de
las demás (Pin). **Present** sigue siendo un modo
independiente, pensado para audiencia y con su propio dock seguro.

Presenter Mode siempre respeta los stops. Durante una presentación,
`Right`, `Enter` o un clic en la pantalla de audiencia avanzan; `Space` solo
pausa o reanuda la animación, sin saltar al siguiente step;
`Left`/`Backspace` retroceden, `O` abre el overview y `B`/`W` controlan el
blanking. Cerrar Presenter View mantiene la audiencia activa y `P` vuelve a
abrir el cockpit sin regenerar sus previews; `Esc` sale del modo presentación.
Presenter View muestra cada segmento como *slide* y cada stop como *step*: un
encabezado con estado, `Slide n of N`, hora, tiempo transcurrido y una barra
por diapositiva; la preview grande de lo que ve la audiencia con sus steps;
Up Next; notas con tamaño ajustable; y un dock con navegación, overview,
blanking y el progreso de las previews. Las previews se generan en segundo
plano, primero la diapositiva actual y la siguiente. La pantalla fullscreen
revela un dock compacto con Previous, Advance/Pause, inicio, fin y progreso al
llevar el cursor a su zona inferior; se oculta al retirar el cursor o perder
foco. Consulta `docs/content/guias/presentaciones.typ` para el detalle.

## 3D e inspección

> **Experimental.** La API 3D todavía tiene errores conocidos y puede cambiar
> entre versiones; queda mucho trabajo para estabilizarla.

Gaanim incluye `cube`, `sphere`, `cylinder`, `cone` y `plane` como mallas
animables, con `Material3D.matte`, `Material3D.metal` y
`Material3D.emissive`. `scene.geometry.lighting_3d("studio")` las ilumina con el
estilo de Gaanim: luz de cielo y suelo, una luz principal suave y un brillo leve
en los bordes, conservando el color de cada material. Consulta
`examples/primitives_3d_demo.py` para una escena completa.

Vello dibuja el 3D como el resto de la escena: proyecta las mallas y las líneas
3D con la cámara, las ordena de atrás hacia delante y calcula la luz en la CPU.
No hay sombras ni búfer de profundidad, la geometría que se cruza puede
ordenarse mal y el 3D queda debajo del contenido 2D. Gaanim ya no importa
glTF: `scene.media.gltf(...)` lanza `NotImplementedError`.

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

## Agentes de código

El wheel incluye `docs/content` como `gaanim/_docs`, así que cada proyecto tiene
la documentación de su versión instalada. `gaanim init` genera un `AGENTS.md`
que apunta a ella. Para Claude Code, el plugin `gaanim` añade la skill
`gaanim-docs`, que localiza, indexa y busca esas docs en cualquier proyecto:

```text
/plugin marketplace add PaoloLupo/gaanim
/plugin install gaanim@gaanim
```

## Exportar

Mantenga `scene.render()` al final del script y solicite la exportación al
ejecutable:

```powershell
gaanim export . --output output.mp4 --quality standard
gaanim export . --output output.mp4 --encoder nvenc
gaanim export . --output overlay.webm --transparent
```

Se admiten MP4, WebM, WebP animado, GIF y secuencias PNG. La exportación de
video requiere FFmpeg disponible en `PATH`; si no está instalado, use primero
una secuencia PNG o instale FFmpeg según su plataforma.
Para MP4, Gaanim prueba automáticamente NVENC, AMF y QSV y usa el primer encoder
hardware funcional. `libx264` queda como fallback cuando ninguno pasa el probe
real; el editor muestra el encoder efectivo durante la exportación. Para exigir
una implementación concreta, use `--encoder libx264|nvenc|amf|qsv|vaapi` o el
selector del editor. Una selección explícita nunca cae a otro encoder: si el
hardware o driver no funciona, la exportación falla. `--encoder auto` conserva
el probe y fallback anteriores. VAAPI es explícito y muestra una advertencia
porque un fallo del driver puede bloquear la GPU completa, fuera del aislamiento
que puede ofrecer Gaanim.
`--transparent` está disponible para WebM, WebP y PNG; MP4 y GIF se rechazan
explícitamente porque no forman parte del contrato alpha de Gaanim.

Para compartir una escena o presentación con alguien que no tiene Python,
grábela en un paquete `.gaanim`: un solo archivo con todos los fotogramas que
dibuja la escena (callbacks incluidos), sus segmentos, notas, pausas,
marcadores y audio. `gaanim` lo reproduce, lo presenta y lo exporta a vídeo sin
cargar Python, con los mismos píxeles que la exportación del script:

```powershell
gaanim export . --output charla.gaanim
gaanim --present charla.gaanim
gaanim export charla.gaanim --output charla.mp4
```

Consulte `docs/content/guias/compartir.typ`.

También puede colocar un MP4 dentro de la escena. El clip es un `Drawable`,
responde al seek del editor y permite trim, loop, velocidad y audio embebido:

```python
clip = scene.media.video("assets/clip.mp4", width=720, loop=True, volume=0.8)
scene.wait(8)
```

Esta función requiere `ffmpeg` y `ffprobe` disponibles en `PATH`.

Para narrar, el editor graba tu voz sin herramientas externas. Con voz en off,
la toma marca el ritmo y el script espera a marcas con nombre en lugar de
duraciones fijas; con `scene.live_take()`, presentas la escena hablando y cada
`scene.stop()` se convierte en la pausa que hiciste. Abre el panel con el botón
de micrófono de la barra de reproducción:

```python
with scene.voiceover("intro", text="Hoy vemos la derivada") as vo:
    scene.play([title.animate.write()])
    vo.wait_until("derivada")
```

El texto de cada bloque puede vivir fuera del código, en
`assets/narration/script.md` (una sección `## intro` por bloque). Las tomas se
guardan en `assets/narration/`, se nivelan a -16 LUFS para video y se mezclan
al exportar MP4 o WebM. Mientras grabas, el medidor marca la zona de nivel ideal
y avisa en rojo si te acercas a la saturación. Opcionalmente, whisper.cpp
detecta las marcas por palabra. Consulte `docs/content/referencia/audio.typ`.

## Comandos de desarrollo

| Objetivo | Comando |
| --- | --- |
| Comprobar todo el workspace | `just check` |
| Comprobar un crate y sus dependencias | `just check-package gaanim_math` |
| Probar un crate | `just test-package gaanim_math --lib` |
| Probar varios crates en una sola ejecución | `just dev test -p gaanim_math -p gaanim_scene --lib` |
| Ejecutar Clippy | `just clippy` |
| Verificar entorno y aplicación | `just doctor` |
| Compilar la aplicación | `just build` |
| Compilar el runtime con informe de tiempos | `just build-timings` |
| Ejecutar un ejemplo | `just run quickstart` |
| Medir presupuestos del runtime | `just benchmark smoke` |
| Generar la documentación | `just docs` |
| Compilar el reproductor web de `.gaanim` (experimental) | `just web` (servirlo: `just web-serve`) |

Durante la iteración, comprueba el crate afectado y conserva `target/`, el
toolchain y las opciones de compilación para aprovechar la caché. El perfil
`dev` conserva optimización de nivel 1 para Gaanim y nivel 3 para dependencias,
pero genera solo tablas de líneas para el workspace y omite los símbolos de
depuración de dependencias. Esto reduce el trabajo de compilación y enlace a
cambio de limitar la inspección de variables en el depurador. Para depurar con
información completa, cambia temporalmente ambos ajustes `debug` a `true`;
esto requiere recompilar los artefactos afectados.

Los comandos dev de `just` usan `scripts/dev.py` para activar `dev-dynamic`,
que habilita `bevy/dynamic_linking` en los crates seleccionados que usan Bevy.
Mantén ese modo en `check`, `test`, `build` y `run`: alternarlo con comandos
Cargo sin la feature genera variantes distintas de los artefactos. Para pasar
opciones de Cargo usa `just dev`, por ejemplo
`just dev check -p gaanim_scene --tests`. `just dev --dry-run test -p gaanim_math`
muestra el comando sin ejecutarlo. Las recetas de release y benchmarks no
activan enlace dinámico; evita `--all-features` en builds de distribución.

Para diagnosticar los FPS de la previsualización, abre un proyecto con
`GAANIM_FRAME_PROFILE=1`: el editor reproduce la línea de tiempo completa sin
detenerse en las paradas, escribe en stderr una línea por segundo con el coste
del seek, la compilación de fragmentos, el render (incluida la espera de
vsync), la resolución del preview y los objetos visibles y, al terminar, cierra
la ventana tras listar las ventanas más lentas. La previsualización baja su
resolución al reproducir si el render no llega a 60 fps y la recupera al
pausar; para medir siempre a resolución completa, añade
`GAANIM_PREVIEW_RESOLUTION=full`.

Los seeks parten del inicio del segmento en que caen: cuando la reproducción
entra en un segmento, o la segunda vez que un seek cae en él, el timeline
guarda el estado de su inicio (si ningún clip anterior sigue en curso ahí) y
los siguientes solo reaplican los clips desde ese punto. `GAANIM_SEGMENT_CHECKPOINTS=0` vuelve a reaplicar desde t=0 para
comparar; `GAANIM_CHECKPOINT_TIMINGS=1` informa cuánto tarda cada captura.

`just run` comprueba la vigencia de los binarios mediante Cargo antes de abrir
la escena. Para ejecutar un binario ya validado sin invocar un build, usa
`just dev-exec target/debug/gaanim.exe examples/quickstart.py` en Windows
(`target/debug/gaanim` en Unix). Ese comando prepara las rutas de las bibliotecas
compartidas de Bevy y Rust y conserva las de Python. No copies únicamente el
ejecutable dev para distribuirlo; utiliza `just build-dist`.

La verificación rápida de agentes prueba los crates con cambios Rust en una
sola invocación, sin añadir automáticamente otro `check` de todo el workspace.
El enlace dinámico reduce el trabajo de enlace, pero no elimina la compilación
de código modificado ni permite intercambiar los artefactos de `check`, `test`
y `build`. La primera activación de este modo necesita compilar la biblioteca
dinámica; los siguientes comandos reutilizan lo que Cargo considere vigente.

En Windows MSVC, configura LLD en `.cargo/config.toml`, conservando la selección
local del intérprete Python. Este archivo está excluido de Git, por lo que cada
checkout necesita su propia configuración:

```toml
[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"
```

Si el comando no está disponible, instala `cargo-binutils` y el componente
`llvm-tools-preview` de Rust. Las funciones de Bevy se activan en los crates
que las necesitan: matemáticas usa la base ECS, escena añade assets y render
(sin PBR ni glTF), el renderizador añade el pipeline 2D donde compone el
lienzo de Vello, multimedia añade audio y los hosts añaden ventanas nativas.
Bevy UI, sprites, texto y gizmos no se compilan.

`just build-timings` compila el runtime y genera
`target/cargo-timings/cargo-timing.html`. Para medir una iteración representativa,
úsalo después de un cambio habitual en Rust; una ejecución sin cambios mide
principalmente la comprobación de caché. La primera compilación tras modificar
perfiles o linker reconstruye los artefactos afectados. `just build-release`
usa el perfil `release` (LTO local por crate, 16 codegen units) para builds
optimizados y benchmarks locales; `just build-dist` usa el perfil `dist`
(ThinLTO entre crates, un codegen unit), más lento pero más pequeño, para los
binarios distribuidos.

La API pública de Python comienza en `Scene`, que conserva la orquestación y
expone las capacidades `geometry`, `text`, `layout`, `media`, `viz`, `slides`,
`mechanics` y `assets`; `Canvas(...)` es un constructor de compatibilidad
deprecado. La migración desde la superficie plana 0.1 está documentada en
[`docs/content/apendices/novedades.typ`](docs/content/apendices/novedades.typ).
Consulte también las escenas de referencia en [`examples/`](examples/).

## Estado

Gaanim está en fase alfa (`0.10.2`). La base de render, timeline, texto y
ecuaciones es funcional; la cobertura de API, pruebas de exportación y
capacidades multimedia continúan en desarrollo. El plan de evolución se
sigue en las [issues](https://github.com/PaoloLupo/gaanim/issues).

## Licencia

Copyright (c) 2026 Paolo Guillen Lupo.

Gaanim se distribuye bajo cualquiera de estas dos licencias, a tu elección:

- MIT ([`LICENSE-MIT`](LICENSE-MIT))
- Apache 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))

Salvo que indiques lo contrario, cualquier contribución que envíes para su
inclusión en Gaanim, tal como define la licencia Apache 2.0, se distribuye bajo
estas dos licencias, sin términos ni condiciones adicionales.

**Lo que creas es tuyo.** Los vídeos, imágenes, archivos `.gaanim` y demás
salidas que generes con Gaanim, y los scripts de tus escenas, te pertenecen y
no están sujetos a la licencia de Gaanim. Puedes usarlos, publicarlos y
venderlos libremente.

**Marca.** La licencia cubre el código, no el nombre «Gaanim» ni su logotipo.
Puedes usar el nombre para describir con veracidad que tu trabajo usa Gaanim o
deriva de él, pero una versión modificada o un fork no debe presentarse como
Gaanim oficial.

**Componentes de terceros.** Gaanim usa bibliotecas y fuentes de terceros con
sus propias licencias. Las descargas incluyen sus avisos en
`THIRD-PARTY-NOTICES.txt`, generado con `python scripts/third_party_licenses.py`.
Las fuentes de la documentación tienen los suyos en
[`docs/assets/fonts/`](docs/assets/fonts/README.md).
