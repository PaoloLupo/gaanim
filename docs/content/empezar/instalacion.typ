#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Instalación",
  description: "Instala Gaanim desde las releases de GitHub en Windows y Ubuntu",
  route: "/empezar/instalacion/",
)

= Descarga

Cada versión de Gaanim se publica en las
#link("https://github.com/PaoloLupo/gaanim/releases/latest")[releases de GitHub]
con un paquete por sistema y su suma SHA-256. No necesitas Rust, `just` ni
compilar nada.

#table(
  columns: 3,
  table.header[*Sistema*][*Archivo*][*Contenido*],
  [Windows 10/11 x64], [`gaanim-v<versión>-windows-x64.zip`], [`gaanim.exe`, `gaanim-core.exe`, wheel de autoría],
  [Ubuntu 24.04 x64], [`gaanim-v<versión>-linux-x64.tar.gz`], [`gaanim`, `gaanim-core`, wheel de autoría],
)

macOS todavía no tiene paquete.

El paquete trae dos ejecutables. `gaanim` es un lanzador ligero: encuentra un
Python compatible y arranca `gaanim-core`, el motor, que contiene la vista
previa, el renderer y el exportador. Deja siempre los dos en la misma carpeta.

El wheel `py3-none-any` es el paquete de autoría: helpers, stubs y `py.typed`
para que tu editor autocomplete la API. No contiene el renderer; importarlo con
Python plano falla a propósito. Las escenas siempre se ejecutan con `gaanim`.

== Verifica la descarga

Descarga también el archivo `.sha256` y compara:

```powershell
# Windows (PowerShell)
Get-FileHash .\gaanim-v*-windows-x64.zip -Algorithm SHA256
Get-Content .\gaanim-v*-windows-x64.zip.sha256
```

```bash
# Ubuntu
sha256sum -c gaanim-v*-linux-x64.tar.gz.sha256
```

= Windows 10/11

== Requisitos <reqs-windows>

- *Python 3.14 o posterior*: desde #link("https://www.python.org/downloads/")[python.org],
  con `winget install Python.Python.3.14` o con `uv python install 3.14`.
- *#link("https://docs.astral.sh/uv/")[uv]*: `gaanim init` lo usa para crear el
  entorno de cada proyecto.
- Una GPU con controladores actualizados (Vulkan o DirectX 12).
- *FFmpeg*, opcional: necesario para exportar `mp4`/`webm` y para
  `scene.media.video()`. Sin él puedes exportar `webp`, `gif` o `png`.

== Instala

1. Extrae el zip en una carpeta, por ejemplo `C:\Tools\gaanim`.
2. Añade esa carpeta al `PATH` de tu usuario, desde _Configuración_ #sym.arrow
   _Variables de entorno_ #sym.arrow `Path`, o con PowerShell:

```powershell
Expand-Archive .\gaanim-v*-windows-x64.zip -DestinationPath C:\Tools\gaanim
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
[Environment]::SetEnvironmentVariable("Path", "$userPath;C:\Tools\gaanim", "User")
```

3. Abre una terminal nueva y comprueba:

```powershell
gaanim --version
gaanim --help
```

`gaanim.exe` no depende de `python3.dll`, así que arranca aunque Python no esté
en el `PATH`; busca el intérprete al abrir una escena.

= Ubuntu 24.04

== Requisitos <reqs-ubuntu>

- *Python 3.14 exactamente*: `gaanim-core` enlaza `libpython3.14.so`. Con uv:
  `uv python install 3.14`.
- *#link("https://docs.astral.sh/uv/")[uv]* para los entornos de proyecto.
- Las bibliotecas de sistema de Ubuntu 24.04 y una GPU con Vulkan.
- *FFmpeg*, opcional: `sudo apt install ffmpeg` para video y audio.

== Instala

Copia los ejecutables a `~/.local/bin` y el wheel a `~/.local/share/gaanim`,
donde el lanzador lo busca:

```bash
tar -xzf gaanim-v*-linux-x64.tar.gz
install -Dm755 -t ~/.local/bin gaanim gaanim-core
install -Dm644 -t ~/.local/share/gaanim gaanim-*-py3-none-any.whl
gaanim --version
```

Ubuntu ya incluye `~/.local/bin` en el `PATH` cuando la carpeta existe al
iniciar sesión; si `gaanim` no aparece, abre una sesión nueva.

= Crea un proyecto

`gaanim` sin argumentos abre el Inicio: crea o abre proyectos, revisa el
diagnóstico de Python y uv y muestra los recientes. Desde la terminal:

```powershell
gaanim init video mi-video      # video 16:9
gaanim init slides mi-charla    # presentación con pasos
gaanim init video .             # en la carpeta actual, si está vacía
```

Cada `init` genera:

```text
mi-video/
  gaanim.toml       # nombre, tipo, entrada y carpetas del proyecto
  pyproject.toml    # proyecto uv con Python 3.14
  .python-version
  main.py           # escena de ejemplo
  assets/           # imágenes, SVG, fuentes
  exports/          # videos exportados
  README.md
  AGENTS.md         # guía para asistentes de código
  .gitignore
  .venv/            # entorno creado con uv, con el wheel de autoría
```

`--force` solo actualiza los archivos del scaffold y no borra tus assets.

== Usa el proyecto

```powershell
gaanim mi-video                            # vista previa; guardar recarga
gaanim check mi-video                      # revisa la escena sin abrir ventana
gaanim check mi-video --strict             # falla también con avisos
gaanim export mi-video --output mi-video/exports/demo.mp4
gaanim --present --monitor 1 mi-video      # presentación en el monitor 1
```

Dentro de la carpeta del proyecto, `gaanim .` equivale a abrir su `main.py`.
`gaanim check` ejecuta el script: lo que imprimas con `print()` aparece antes
del informe.

Un `main.py` mínimo:

```python
from gaanim import BLACK, BLUE, Easing, Scene

scene = Scene(frame=(16, 9), background=BLACK)
circle = scene.geometry.circle(1).fill(BLUE)
scene.play([circle.animate.create().duration(1).easing(Easing.SMOOTH)])
scene.render()
```

= Cómo encuentra Python

No necesitas activar el entorno ni tocar variables. Al abrir una escena, el
lanzador usa el primer intérprete que encuentre:

1. El entorno activo (`VIRTUAL_ENV`).
2. Una carpeta `.venv`, `venv` o `env` junto al proyecto o al directorio
   actual, subiendo hasta cuatro niveles.
3. El Python del sistema: `py -3.14`, `python` o `python3` en Windows;
   `python3.14`, `python3` o `python` en Ubuntu.
4. Un Python 3.14 instalado con uv.

Después prepara las rutas de ese Python para `gaanim-core` y lo arranca.

= Actualiza

Descarga la release nueva y reemplaza los dos ejecutables y el wheel. Al abrir
un proyecto, Gaanim reinstala su paquete de autoría si la versión no coincide
con la del ejecutable, así que no hace falta recrear los entornos.

= Solución de problemas

- *`python3.dll` no encontrado o salida `-1073741515`*: el lanzador no encontró
  Python 3.14. Comprueba `py -3.14 --version` o crea el entorno con
  `uv venv --python 3.14`. Ejecuta siempre `gaanim`, no `gaanim-core` directamente.
- *`authoring environment not ready`*: falta uv o el wheel no está junto al
  ejecutable (Windows) ni en `~/.local/share/gaanim` (Ubuntu).
- *`gaanim check: could not load project`*: revisa que `entry` en `gaanim.toml`
  sea una ruta relativa que exista.
- *`FFmpeg not found` al exportar `mp4`*: instala FFmpeg y añádelo al `PATH`, o
  exporta `webp`, `gif` o `png`.

Para compilar Gaanim desde el código y contribuir, sigue el
#link("https://github.com/PaoloLupo/gaanim#readme")[README del repositorio].
