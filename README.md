# Gaanim

Motor de animación vectorial 2D acelerado por GPU, escrito en Rust y diseñado
para crear escenas programáticas desde Python. El flujo actual ejecuta los
scripts Python dentro de la aplicación `gaanim`, que proporciona la ventana de
previsualización, hot reload y exportación.

Gaanim ofrece dos modos de uso:

- La aplicación embebida, recomendada para previsualización y hot reload.
- El paquete Python local, útil para integrar la API y ejecutar exportaciones
  headless. Requiere Python 3.12 o superior.

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
title = scene.title("Hola, Gaanim").fill(GOLD).at(0, 180)

scene.play([circle.create().duration(0.8), title.write().duration(0.6)])
scene.play([circle.move(240, 0).duration(1.0).smooth()])
scene.render()
```

Para crear una escena nueva, guarde un script en `examples/` y ejecútelo con
`just run nombre_del_script` (sin `.py`). Durante la previsualización, guardar
el archivo recarga la escena.

El binario de usuario también administra proyectos. `gaanim` sin argumentos
abre el Inicio con creación, apertura, diagnóstico de Python/uv y hasta diez
proyectos recientes. Los únicos scaffolds generales son:

```powershell
gaanim init video mi-video
gaanim init slides mi-charla
gaanim .
```

Los proyectos sin `.venv` pueden usar un Python 3.12+ detectado en el sistema;
el Inicio muestra instrucciones copiables de uv, pero nunca las ejecuta.

Las presentaciones usan el mismo concepto de segmento que los videos. Los
límites son continuos y solo `stop()` solicita input durante la reproducción:

```python
from gaanim import Anchor, Scene

scene = Scene(1280, 720)
intro = scene.segment("Introducción", notes="Presenta el objetivo", layout="cover")
intro.region("title").place(scene.title("Una idea clara"), Anchor.CENTER)
scene.play([scene.text("Resultado").write().duration(0.5)])
scene.stop("resultado")
```

La exportación ignora los stops. Para exportar un segmento concreto use
`scene.export("intro.mp4", segment="Introducción")`.

## Paquete Python local

El binding también puede instalarse en el entorno virtual del repositorio:

```powershell
just bootstrap
just python-develop
.\.venv\Scripts\python -c "from gaanim import Scene; print(Scene)"
```

Para construir una wheel distribuible use `just wheel`; el resultado se escribe
en `target/wheels/`. El modo de paquete no incluye el visor interactivo:
`Scene.render()` sigue requiriendo ejecutar el script con la aplicación
`gaanim`.

Después de `just python-develop`, ejecute `just validate-python-api` para
comprobar que el stub tipado público sigue coincidiendo con la extensión nativa.

## Exportar

Reemplace `scene.render()` por una exportación cuyo formato se infiere de la
extensión:

```python
scene.export("output.mp4", fps=30)
```

Se admiten MP4, WebM, WebP animado, GIF y secuencias PNG. La exportación de
video requiere FFmpeg disponible en `PATH`; si no está instalado, use primero
una secuencia PNG o instale FFmpeg según su plataforma.

## Comandos de desarrollo

| Objetivo | Comando |
| --- | --- |
| Comprobar todo el workspace | `just check` |
| Ejecutar Clippy | `just clippy` |
| Verificar entorno y aplicación | `just doctor` |
| Compilar la aplicación | `just build` |
| Ejecutar un ejemplo | `just run quickstart` |
| Generar la documentación | `just docs` |

La API pública de Python comienza en `Scene`; `Canvas(...)` es un constructor
de compatibilidad deprecado. Consulte la documentación del sitio en
[`docs/`](docs/) y las escenas de referencia en [`examples/`](examples/).

## Estado

Gaanim está en fase alfa (`0.1.0`). La base de render, timeline, texto y
ecuaciones es funcional; la cobertura de API, pruebas de exportación y
capacidades multimedia continúan en desarrollo. El plan de evolución se
encuentra en [`engine_improvements.md`](engine_improvements.md).
