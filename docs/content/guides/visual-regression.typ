#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Regresión visual",
  description: "Snapshots deterministas por ejemplo y visor egui",
  route: "/guides/visual-regression/",
  updated: datetime.today().display(),
  code-langs: (),
)

Cada ejemplo guarda sus snapshots de regresión en una única carpeta global:

```text
tests/visual/<nombre-del-ejemplo>/
  baseline/  # PNGs aprobados y manifest.json: se versionan
  current/   # captura de la implementación actual: local
  report/    # visor egui, JSON y heatmaps: local
```

Por ejemplo, `examples/visual_diff_demo.py` usa automáticamente `tests/visual/visual_diff_demo/`. No hace falta escribir las rutas de `baseline`, `current` ni `output`.

= Flujo normal

Primero registra la implementación correcta como baseline:

```powershell
.\target\debug\gaanim.exe --diff --example examples/visual_diff_demo.py --bless
# o con el release:
gaanim --diff --example examples/visual_diff_demo.py --bless
```

Después, en cada cambio, captura el ejemplo, compara contra ese baseline y abre el visor egui:

```powershell
gaanim --diff --example examples/visual_diff_demo.py
```

El script debe llamar a `scene.snapshots(...)` cuando existe la variable `GAANIM_SNAPSHOTS`, como hace `examples/visual_diff_demo.py`. El CLI define esa variable y ejecuta la captura headless automáticamente:

```python
import os
if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.0, 0.5, 1.0])
```

= CI y tolerancias

```powershell
gaanim --diff --example examples/visual_diff_demo.py --no-gui --pixel-threshold 4 --max-changed-ratio 0.0001
```

- `--no-gui` — genera report sin abrir egui (para CI).
- `--no-capture` — compara los PNG ya presentes en `current/`.
- `--tests-root <DIR>` — cambia la carpeta global por defecto.
- `--pixel-threshold` / `--max-changed-ratio` — tolerancias.

El modo manual con `--baseline`, `--current` y `--output` sigue disponible para comparar carpetas arbitrarias:

```powershell
gaanim --diff --baseline tests/visual/a/baseline --current tests/visual/a/current --output tests/visual/a/report --no-gui
```

El reporte JSON incluye el seek, porcentaje de píxeles modificados, error medio, delta máximo y rectángulo del cambio. El visor alterna baseline/actual/diff con `1`, `2` y `3`.

En CI (`windows-latest`) el workflow `visual-regression` construye el snapshot runner y compara `transform_demo`/`image_demo`/`svg_demo`/`camera_demo`.
