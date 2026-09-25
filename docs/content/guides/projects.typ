#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Proyectos",
  description: "Estructura, manifiesto y flujo con gaanim init / check / --diff",
  route: "/guides/projects/",
  code-langs: (),
)

Gaanim puede abrir un script Python suelto, pero para trabajos reales conviene usar un proyecto. Un proyecto mantiene código, assets y exportaciones juntos, y puede ejecutarse desde cualquier directorio. No necesitas tocar `PATH`/`VIRTUAL_ENV` manualmente: el launcher `gaanim` detecta el `.venv` cercano.

Ejecuta `gaanim` sin argumentos para abrir el Inicio. Desde allí puedes crear
un proyecto, abrir una carpeta válida o volver a uno de los diez proyectos
recientes. Los scripts Python sueltos no se añaden a esa lista.

= Crear un proyecto

```powershell
gaanim init video mi-video
gaanim init slides mi-charla
```

Cada comando crea un starter ejecutable:

```text
mi-video/
  gaanim.toml
  pyproject.toml
  .python-version # 3.14
  main.py
  README.md
  AGENTS.md
  assets/
  exports/
```

El `pyproject.toml` declara `gaanim` como dependencia del proyecto. `gaanim init`
también prepara el entorno: si falta `.venv`, ejecuta `uv venv --python 3.14` e
instala en él el wheel de autoría incluido. Comandos posteriores como `uv sync`
conservan a Gaanim como parte del entorno.

`AGENTS.md` orienta a agentes de código: apunta a la documentación Typst que
incluye el paquete instalado (`gaanim/_docs`, de la misma versión que la API),
a los stubs `.pyi` y a `gaanim check .`, porque Python sin la aplicación no
puede ejecutar escenas.

- `video` — escena animada 16:9 y flujo de exportación.
- `slides` — segmentos semánticos, notas y paradas para Presenter View.

`--force` actualiza únicamente los archivos conocidos del scaffold. No borra archivos propios dentro de `assets/` ni otras carpetas del proyecto.

```powershell
gaanim init slides mi-charla --force
```

= Trabajar con la carpeta

No hace falta escribir la ruta de `main.py`:

```powershell
gaanim mi-charla
gaanim check mi-charla
gaanim --present --monitor 1 mi-charla
gaanim --diff --example mi-charla --bless --no-gui
```

Dentro del proyecto también se puede usar `.`:

```powershell
cd mi-charla
gaanim .
gaanim check .
```

El visor conserva hot reload sobre el entry point resuelto. Las rutas de assets se resuelven respecto de `gaanim.toml`, por lo que no dependen del directorio desde el que se inició Gaanim.

El hot reload es incremental: al guardar, el script se vuelve a ejecutar, pero
los segmentos anteriores al primero que cambió conservan su escena compilada y
solo se recompila desde ahí. El aviso de recarga lo indica, por ejemplo
`replay 0.12s (reused 37/40 segments)`. La primera edición en una zona nueva
recompila todo una vez para ubicarla; las siguientes ediciones en esa zona ya
son incrementales. Se recompila todo cuando cambia algo global (tema, fuentes,
tamaño del lienzo), cuando un segmento posterior modifica objetos creados antes
—por ejemplo, agrupar el logo persistente— y desde cualquier segmento que use
callbacks de Python (animaciones personalizadas, updaters, funciones reactivas o
curvas de easing propias). El resultado siempre es idéntico al de una recarga
completa; para forzar esta última, inicia Gaanim con `GAANIM_INCREMENTAL=0`.

Los assets del proyecto también recargan la escena. Guardar cualquier archivo
que no sea Python dentro del proyecto (imágenes, SVG, Lottie, glTF, WGSL,
fuentes Typst, datos) vacía las cachés de imágenes, Lottie, glTF y Typst,
vuelve a ejecutar el script y recompila todos los segmentos, porque un segmento
cuyo código no cambió puede dibujar el archivo modificado. Se ignoran los
archivos y carpetas ocultos (`.git`, `.venv`, archivos de intercambio del
editor), `venv`, `env`, `__pycache__`, `exports`, `snapshots`, `target`, los
temporales (`~`, `.swp`, `.tmp`, `.bak`) y los `.lock`.

Los módulos Python del proyecto también recargan. Python se inicializa una sola
vez por sesión, pero antes de cada ejecución Gaanim descarga de `sys.modules`
todos los módulos cuyo archivo está dentro de la raíz del proyecto (salvo en
las carpetas ignoradas de arriba), así que guardar un `.py` importado por el
entry point vuelve a importarlo sin purgas manuales. Los paquetes instalados y
el código fuera de la raíz del proyecto siguen en caché hasta reiniciar Gaanim.
La raíz del proyecto, su carpeta `src/` y la del script están en `sys.path`, y `__file__`,
`sys.argv[0]` y `sys.path` usan rutas normales, sin el prefijo `\\?\` de Windows.

Puedes crear el `.venv` dentro del proyecto con `uv`:

```powershell
uv venv --python 3.14
.\.venv\Scripts\Activate.ps1
```

Sin activar, el launcher igual encuentra `mi-charla/.venv` por walk-up.

Si el proyecto no tiene `.venv` al abrirlo, Gaanim lo prepara igual que
`gaanim init` cuando encuentra uv. Sin uv, abre el proyecto con el Python 3.14
del sistema, sin aislamiento; si tampoco hay un Python compatible, el Inicio
muestra instrucciones copiables para instalar uv.

= Manifiesto

El scaffold genera:

```toml
name = "mi-charla"
kind = "slides"
entry = "main.py"
assets_dir = "assets"
output_dir = "exports"
```

`entry` debe ser una ruta relativa que permanezca dentro del proyecto. `kind`
solo acepta `video` o `slides`. La CLI usa `entry`; la escena carga `assets_dir`
mediante `scene.assets.load_project(...)`. `output_dir` es la carpeta convencional para
artefactos.

En `main.py`:

```python
from gaanim import Scene

scene = Scene(frame=(16, 9))
scene.assets.load_project("gaanim.toml")  # resuelve assets relativo al proyecto
# ... contenido ...
scene.render()
```

= Usar un script existente

Los scripts continúan siendo compatibles:

```powershell
gaanim examples/mi_escena.py
```

Para organizarlo como proyecto, crea la estructura descrita arriba, mueve el
script a `main.py`, añade `scene.assets.load_project(...)` después de construir `Scene`
y coloca los recursos en `assets/`.
