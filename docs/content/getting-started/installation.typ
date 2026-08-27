#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Instalación",
  description: "Instalación nativa para Windows y Ubuntu, y desarrollo local",
  route: "/getting-started/installation/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Usuario final — Windows

Gaanim `0.1.0` se distribuye como un zip de GitHub Releases. No necesitas `just`, ni compilar Rust, ni tocar `PATH`/`VIRTUAL_ENV` manualmente. El ejecutable detecta solo el Python.

== Requisitos <reqs-user>

- *Windows 10/11 x64*
- *Python >=3.12* — 3.12 es el mínimo, 3.13 y 3.14 también funcionan. Instala desde #link("https://www.python.org/downloads/")[python.org] o `winget install Python.Python.3.12` / `winget install Python.Python.3.14`.
- *uv recomendado* — #link("https://docs.astral.sh/uv/")[docs.astral.sh/uv] para crear entornos. También funciona `python -m venv`.
- *FFmpeg opcional* — necesario para exportar `mp4`/`webm` y para usar `scene.media.video()`. Si no está, exporta `png` y evita videos embebidos.

El binario es `gaanim.exe` (launcher, 300KB) + `gaanim-core.exe` (motor, ~140MB). El launcher no depende de `python3.dll`, por eso puede arrancar sin tener Python en `PATH` antes de ejecutarlo: detecta el venv, añade su directorio al `PATH` y luego lanza el core.

El zip también incluye un wheel universal `py3-none-any`. Ese wheel solo aporta
helpers, stubs y `py.typed` al entorno del proyecto: no contiene el binding
nativo, renderer ni exportador. Importarlo con Python plano falla de forma
intencional. Preview, validación y exportación siempre se ejecutan mediante
`gaanim`; no existe ni está previsto un runtime autónomo instalable con `pip`.

== Descargar y poner en PATH

1. Ve a *Releases* en GitHub y descarga `gaanim-v0.1.0-windows-x64.zip` (y su `.sha256` si quieres verificar).
2. Extrae a `C:\Tools\gaanim\` (o donde prefieras).
3. Añade esa carpeta a `PATH`: Panel de control #sym.arrow Sistema #sym.arrow Variables de entorno #sym.arrow `Path` #sym.arrow Nuevo #sym.arrow `C:\Tools\gaanim`.
4. Verifica sin activar nada:

```powershell
gaanim --help
gaanim check --help
```

Debe imprimir la ayuda sin error `python3.dll` ni pedir variables.

== Crear un proyecto con uv <uv>

Crea tus proyectos manualmente, donde quieras. Ejemplo:

```powershell
mkdir mi-proyecto; cd mi-proyecto
uv venv --python 3.12
.\.venv\Scripts\Activate.ps1   # opcional: activa el entorno
```

Alternativa sin `uv`:

```powershell
py -3.14 -m venv .venv
.\.venv\Scripts\Activate.ps1
```

Ahora crea el scaffold:

```powershell
gaanim init video mi-video      # video 16:9
gaanim init slides mi-charla    # slides semánticos
# o dentro de una carpeta vacía:
gaanim init video .
```

Cada `init` genera:

```text
mi-video/
  gaanim.toml   # name/kind/entry/assets_dir/output_dir
  pyproject.toml # proyecto uv --bare (Python >=3.14)
  .python-version # 3.14
  main.py       # Scene de ejemplo
  assets/       # imágenes, svg, fuentes
  exports/      # mp4/webm/png (gitignore)
  README.md
```

Cada scaffold crea el equivalente a `uv init --bare --python 3.14`, añade
`gaanim` a las dependencias de `pyproject.toml` y prepara `.venv` con Python
3.14. `--force` solo actualiza los archivos del scaffold, no borra tus assets.

== Usar el proyecto

Sin necesidad de escribir `main.py`:

```powershell
gaanim mi-video          # preview con hot reload (guardar recarga)
gaanim check mi-video    # preflight 16:9, notas, stops, placeholders
gaanim check mi-video --strict  # falla también con warnings
gaanim --present --monitor 1 mi-video  # presentación en proyector 1
```

Desde dentro:

```powershell
cd mi-video
gaanim .                 # equivale a gaanim main.py
```

Dentro de `main.py` la forma canónica es:

```python
from gaanim import Scene, BLACK

scene = Scene(1920, 1080, background=BLACK)
scene.assets.load_project("gaanim.toml")  # opcional: resuelve assets relativo al proyecto
circle = scene.geometry.circle(80).fill(BLUE)
scene.play([circle.animate.create().duration(1).spring()])
scene.render()  # para preview
# luego: gaanim export . --output exports/demo.mp4
```

== Cómo detecta Python sin variables

Cuando ejecutas `gaanim` desde cualquier terminal (activada o no), el launcher hace:

1. Si `VIRTUAL_ENV` existe (hiciste `Activate.ps1`), usa ese `.venv` y su `home` de `pyvenv.cfg`.
2. Si no, hace *walk-up* desde `script`, `cwd` y `exe` buscando `.venv/pyvenv.cfg`, `venv/pyvenv.cfg`, `env/pyvenv.cfg` hasta 4 niveles arriba. Así `gaanim .\mi-video` encuentra `.\mi-video\.venv` sin activar.
3. Si no hay venv cercano, prueba `py -3.14`, `py -3.13`, `py -3.12`, `py -3`, `python`, `where python` y lee `sys.base_prefix`.

Luego antepone `home` y `home\Scripts` al `PATH` del proceso hijo (`gaanim-core.exe`), que sí está linkeado a `python3.dll`/`python3xx.dll`. Por eso no necesitas ` $env:PATH = ...` manualmente.

Tip: `gaanim --help` también funciona sin proyecto, usando el fallback del sistema.

= Usuario final — Ubuntu 24.04 x64

Descarga `gaanim-v0.1.0-linux-x64.tar.gz` desde GitHub Releases. El paquete
contiene `gaanim`, `gaanim-core` y el mismo wheel universal de autoría que la
distribución Windows.

```bash
tar -xzf gaanim-v0.1.0-linux-x64.tar.gz
mkdir -p ~/.local/bin ~/.local/share/gaanim
install -m 755 gaanim gaanim-core ~/.local/bin/
install -m 644 gaanim-*-py3-none-any.whl ~/.local/share/gaanim/
gaanim --help
```

Requiere Python 3.12 y las bibliotecas base de Ubuntu 24.04. Instala FFmpeg para
exportar MP4/WebM o usar video embebido. Mantén ambos ejecutables en la misma
carpeta: el launcher localiza `gaanim-core` junto a sí mismo.

= Ejemplo completo — de cero a proyecto nuevo usando el PATH

Este es el flujo que yo uso en local y el que usará cualquier usuario con el zip en `PATH`. No requiere activar el venv para que `gaanim` funcione:

```powershell
# 1. Requisitos ya instalados: Python >=3.12, uv, Rust (solo si compilas)

# 2a. Si compilas en local (dev):
git clone https://github.com/<tu-org>/gaanim; cd gaanim
just build-release        # genera target/release/gaanim.exe + gaanim-core.exe
#   o descarga el release:
#   Expand-Archive gaanim-v0.1.0-windows-x64.zip -DestinationPath C:\Tools\gaanim

# 2b. Poner en PATH (una sola vez)
#    Copia los dos exe a una carpeta en PATH, por ejemplo:
Copy-Item target/release/gaanim.exe C:\Tools\gaanim\ -Force
Copy-Item target/release/gaanim-core.exe C:\Tools\gaanim\ -Force
#    Verifica que C:\Tools\gaanim está en $env:PATH:
$env:PATH -split ';' | Select-String gaanim
gaanim --help             # debe funcionar incluso sin .venv

# 3. Crear un proyecto nuevo en cualquier lugar, con su propio entorno
mkdir C:\proyectos\demo-hello; cd C:\proyectos\demo-hello
uv venv --python 3.12
# no hace falta Activate.ps1: el launcher encuentra .venv por walk-up
# El ejecutable aporta el runtime; el wheel del proyecto solo aporta autoría y tipos.

# 4. Generar el scaffold (yo lo hago así)
gaanim init video .       # crea gaanim.toml, main.py, assets/, exports/
# o: gaanim init slides mi-charla

# 5. Verificar y previsualizar
gaanim check .            # 2.2 seconds · 1920x1080 · PASS
gaanim .                  # abre preview con hot reload; guarda main.py y recarga

# 6. Con venv activado también funciona (misma detección vía VIRTUAL_ENV)
.\.venv\Scripts\Activate.ps1
gaanim --present --monitor 1 .  # presentación en proyector
deactivate
```

Notas de este flujo:

- El `gaanim.exe` que está en `PATH` es el *launcher* (sin `python3.dll`). Él encuentra `C:\proyectos\demo-hello\.venv\pyvenv.cfg` → `home = C:\...\Python314` → antepone `home` y `.venv\Scripts` al `PATH` del hijo `gaanim-core.exe`.
- Si mueves el proyecto, el walk-up sigue funcionando mientras `.venv` esté dentro o hasta 4 niveles arriba.
- Para otro proyecto no necesitas reinstalar `gaanim`: basta `uv venv` + `gaanim init` de nuevo. El binario en `PATH` es único y reutilizable.

= Desarrollo local — clonar y compilar

Para contribuir o compilar desde fuente:

== Requisitos dev

- Rust estable (`rustup`), Python >=3.12, `just` (#link("https://just.systems")[just.systems]), Git.
- En Windows, PowerShell. En Linux/macOS, bash.

== Preparación y compilación

```powershell
git clone https://github.com/<tu-org>/gaanim
cd gaanim
just bootstrap        # crea .venv e instala build + hatchling
just build            # debug: gaanim_launcher + gaanim_editor
just doctor           # check + compila y prueba gaanim --help via launcher
```

Comandos habituales:

```powershell
just check            # cargo check --workspace
just clippy           # clippy
cargo test --workspace
just run quickstart   # via launcher en Windows, directo en Unix
just python-develop   # instala el paquete de autoría editable
just wheel            # wheel universal en target/wheels/
just validate-python-api  # compara el stub con el módulo embebido
just docs             # compila el site Typst
```

Estructura relevante:

```text
crates/gaanim_launcher  # exe sin pyo3: detecta Python y lanza gaanim-core
crates/gaanim_editor    # lib + bin gaanim / gaanim-core (Bevy + PyO3 abi3-py312)
crates/gaanim_python    # módulo PyO3 embebido + paquete de autoría
target/debug/gaanim.exe       # launcher (Windows)
target/debug/gaanim-core.exe  # motor
target/wheels/gaanim-0.1.0-py3-none-any.whl
```

En dev, `cargo run -p gaanim_editor -- examples/quickstart.py` requiere `PATH` con Python. Por eso `just run` en Windows usa el launcher (`cargo run -p gaanim_launcher`). Para debug directo del core, el `just` anterior ya no inyecta `$env:PATH`.

== Versión local de distribución

```powershell
cargo build -p gaanim_launcher --release
cargo build -p gaanim_editor --release --bin gaanim-core
# zip:
# target/release/gaanim.exe + target/release/gaanim-core.exe + README.md -> dist/gaanim-v0.1.0-windows-x64.zip
```

El workflow `.github/workflows/release.yml` hace lo mismo en `windows-latest` al pushear `v0.1.0`.

== Solución de problemas

- `exit -1073741515 / python3.dll not found`: el launcher no encontró Python. Verifica `py --version` o crea `.venv` con `uv venv --python 3.12`. El core solo (`gaanim-core.exe`) siempre fallará sin launcher si `PATH` no contiene Python.
- `gaanim check: could not load project`: revisa `gaanim.toml` `entry` sea relativo y exista, y que `assets/` exista si `scene.assets.load_project` lo usa.
- `FFmpeg not found` al exportar `mp4`: instala FFmpeg y añádelo a `PATH`, o exporta `png`/`webp`.
- `just bootstrap` crea `.venv` con 3.14 pero el zip release exige >=3.12: es compatible, no necesitas recrear el venv.
