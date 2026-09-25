#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Instalación",
  description: "Instala Gaanim en Windows o Ubuntu, crea un proyecto y resuelve los problemas habituales",
  route: "/empezar/instalacion/",
)

= En resumen

Si ya tienes Python 3.14 y #link("https://docs.astral.sh/uv/")[uv]:

1. Descarga el paquete de tu sistema desde la
   #link("https://github.com/PaoloLupo/gaanim/releases/latest")[última release].
2. Extráelo y pon la carpeta de los ejecutables en el `PATH`
   (#link(<instala-windows>)[Windows] o #link(<instala-ubuntu>)[Ubuntu]).
3. Abre una terminal nueva y crea un proyecto:

```bash
gaanim init video mi-video   # crea el proyecto y su entorno con uv
gaanim mi-video              # abre la vista previa
```

Si algo falla, ve a #link(<problemas>)[Solución de problemas]. El resto de la
página explica cada paso con detalle.

= Requisitos

#table(
  columns: 3,
  table.header[][*Windows*][*Ubuntu*],
  [Sistema], [Windows 10/11 x64], [Ubuntu 24.04 x64],
  [Python], [3.14 o posterior], [3.14 exactamente],
  [uv], [necesario], [necesario],
  [GPU], [controladores actuales (Vulkan o DirectX 12)], [Vulkan],
  [FFmpeg], [opcional], [opcional],
)

- *Python* ejecuta tus escenas. Instálalo desde
  #link("https://www.python.org/downloads/")[python.org], con
  `winget install Python.Python.3.14` (Windows) o, en cualquier sistema, con
  `uv python install 3.14`. En Ubuntu tiene que ser 3.14 porque el motor
  enlaza `libpython3.14.so`.
- *#link("https://docs.astral.sh/uv/")[uv]* crea el entorno de cada proyecto.
- *FFmpeg* solo hace falta para exportar video (MP4, WebM, WebP animado y GIF)
  y para `scene.media.video()`. Sin él tienes la vista previa completa y puedes
  exportar secuencias PNG.

macOS todavía no tiene paquete. Para compilar Gaanim desde el código, sigue el
#link("https://github.com/PaoloLupo/gaanim#readme")[README del repositorio].

= Descarga

Cada versión se publica en las
#link("https://github.com/PaoloLupo/gaanim/releases/latest")[releases de GitHub]
con un paquete por sistema y su suma SHA-256. No necesitas Rust ni compilar nada.

#table(
  columns: 3,
  table.header[*Sistema*][*Archivo*][*Contenido*],
  [Windows 10/11 x64], [`gaanim-v<versión>-windows-x64.zip`], [`gaanim.exe`, `gaanim-core.exe`, wheel de autoría],
  [Ubuntu 24.04 x64], [`gaanim-v<versión>-linux-x64.tar.gz`], [`gaanim`, `gaanim-core`, wheel de autoría],
)

- `gaanim` es un lanzador ligero: busca un Python compatible y arranca
  `gaanim-core`.
- `gaanim-core` es el motor: vista previa, renderer y exportador. Deja siempre
  los dos ejecutables en la misma carpeta.
- El wheel `gaanim-<versión>-py3-none-any.whl` es el paquete de autoría: tipos,
  stubs y ayudas para que tu editor autocomplete la API. No contiene el
  renderer, así que `python main.py` falla a propósito: las escenas siempre se
  ejecutan con `gaanim`.

== Verifica la descarga

Descarga también el archivo `.sha256` de tu paquete y compara las sumas:

```powershell
# Windows (PowerShell): los dos valores deben coincidir
Get-FileHash .\gaanim-v*-windows-x64.zip -Algorithm SHA256
Get-Content .\gaanim-v*-windows-x64.zip.sha256
```

```bash
# Ubuntu: imprime "OK" si coincide
sha256sum -c gaanim-v*-linux-x64.tar.gz.sha256
```

= Instala en Windows <instala-windows>

1. Extrae el zip en una carpeta, por ejemplo `C:\Tools\gaanim`. El wheel debe
   quedar junto a los ejecutables: ahí lo busca `gaanim`.
2. Añade esa carpeta al `PATH` de tu usuario, desde _Configuración_ #sym.arrow
   _Variables de entorno_ #sym.arrow `Path`, o con PowerShell:

```powershell
Expand-Archive .\gaanim-v*-windows-x64.zip -DestinationPath C:\Tools\gaanim
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
[Environment]::SetEnvironmentVariable("Path", "$userPath;C:\Tools\gaanim", "User")
```

3. Abre una terminal nueva para que tome el `PATH` actualizado.

`gaanim.exe` arranca aunque Python no esté en el `PATH`: solo lo busca cuando
abres una escena o un proyecto.

= Instala en Ubuntu <instala-ubuntu>

Copia los ejecutables a `~/.local/bin` y el wheel a `~/.local/share/gaanim`,
donde el lanzador lo busca:

```bash
tar -xzf gaanim-v*-linux-x64.tar.gz
install -Dm755 -t ~/.local/bin gaanim gaanim-core
install -Dm644 -t ~/.local/share/gaanim gaanim-*-py3-none-any.whl
```

Ubuntu añade `~/.local/bin` al `PATH` si la carpeta existe al iniciar sesión.
Si acabas de crearla y `gaanim` no aparece, cierra la sesión y vuelve a entrar.

= Comprueba la instalación

```bash
gaanim --version   # imprime la versión instalada
gaanim --help      # lista los comandos
```

Ninguno de los dos necesita Python. Si funcionan, los ejecutables están bien
instalados; el siguiente paso comprueba Python y uv.

= Crea un proyecto

```bash
gaanim init video mi-video      # video 16:9
gaanim init slides mi-charla    # presentación con pasos
gaanim init video .             # en la carpeta actual
```

`init` genera la estructura del proyecto y, con uv, un entorno `.venv` con
Python 3.14 y el wheel de autoría:

```text
mi-video/
  gaanim.toml       # nombre, tipo, archivo de entrada y carpetas
  main.py           # escena de ejemplo: edítala
  assets/           # imágenes, SVG, fuentes, audio
  exports/          # destino sugerido para las exportaciones
  pyproject.toml    # proyecto uv con Python 3.14
  .python-version
  README.md
  AGENTS.md         # instrucciones para asistentes de código
  .gitignore
  .venv/            # entorno creado con uv
```

Si la carpeta ya contiene archivos del proyecto, `init` se detiene para no
pisarlos. Con `--force` reescribe esos archivos (incluido `main.py`) sin tocar
tus assets.

También puedes crear y abrir proyectos sin terminal: `gaanim` sin argumentos
abre el Inicio, con los proyectos recientes y un diagnóstico de Python y uv.

== Usa el proyecto

Estos son los comandos del día a día. La
#link("/empezar/primera-animacion/")[primera animación] los usa paso a paso.

```bash
gaanim mi-video                                     # vista previa; guardar recarga
gaanim check mi-video                               # ejecuta la escena sin ventana
gaanim check mi-video --strict                      # falla también con avisos
gaanim export mi-video --output mi-video/exports/demo.mp4
gaanim --present --monitor 1 mi-charla              # presenta en el monitor 1
```

Dentro de la carpeta del proyecto, `gaanim .` abre su archivo de entrada.
También puedes abrir un script suelto con `gaanim escena.py`.

Tu editor autocompleta la API si usa el intérprete de `mi-video/.venv`.

= Cómo encuentra Python

No hace falta activar el entorno. Al abrir una escena, el lanzador usa el
primer intérprete que encuentra:

1. El entorno activo (`VIRTUAL_ENV`).
2. Una carpeta `.venv`, `venv` o `env` junto al proyecto o al directorio
   actual, subiendo hasta cuatro niveles.
3. El Python del sistema: `py -3.14`, `python` o `python3` en Windows;
   `python3.14`, `python3` o `python` en Ubuntu.
4. El Python 3.14 que haya instalado uv.

Los pasos 1 y 2 no siguen buscando si encuentran otra versión: un entorno con
Python 3.12 produce un error aunque tengas 3.14 instalado.

= Actualiza

Descarga la release nueva y reemplaza los dos ejecutables y el wheel. No hace
falta recrear los proyectos: al abrir uno, Gaanim reinstala su paquete de
autoría si no coincide con la versión del ejecutable.

= Solución de problemas <problemas>

- *`Python >=3.14 was not found` o `Python 3.14 was not found`*: instala
  Python 3.14 (`uv python install 3.14`) y vuelve a intentarlo.
- *`Gaanim requires Python … but found Python 3.x`*: el entorno activo o el
  `.venv` del proyecto usa otra versión. Desactívalo, o recrea el entorno con
  `uv venv --python 3.14`.
- *`python3.dll` no encontrado o salida `-1073741515` (Windows)*: ejecutaste
  `gaanim-core` directamente. Usa siempre `gaanim`, que prepara Python antes de
  arrancar el motor.
- *`authoring environment not ready`*: falta uv, o el wheel no está junto a
  `gaanim.exe` (Windows) ni en `~/.local/share/gaanim` (Ubuntu). La vista
  previa funciona igual, pero el editor no autocompleta.
- *`gaanim check: could not load project`*: revisa que `entry` en
  `gaanim.toml` sea una ruta relativa a un archivo que exista.
- *La exportación dice que `ffmpeg` no está en el `PATH`* (`was not found on
  PATH`): instala FFmpeg y añádelo al `PATH`, o exporta una secuencia PNG
  (`--output frames/frame.png`).
