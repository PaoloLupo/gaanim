# Tests visuales por ejemplo

Cada ejemplo guarda sus snapshots de regresión en una única carpeta global:

```text
tests/visual/<nombre-del-ejemplo>/
  baseline/  # PNGs aprobados y manifest.json: se versionan
  current/   # captura de la implementación actual: local
  report/    # visor egui, JSON y heatmaps: local
```

Por ejemplo, `examples/visual_diff_demo.py` usa automáticamente
`tests/visual/visual_diff_demo/`. No hace falta escribir las rutas de
`baseline`, `current` ni `output`.

## Flujo normal

Primero registra la implementación correcta como baseline:

```powershell
. .\.venv\Scripts\Activate.ps1
target/debug/gaanim.exe --diff --example examples/visual_diff_demo.py --bless
```

Después, en cada cambio, captura el ejemplo, compara contra ese baseline y abre
el visor egui:

```powershell
target/debug/gaanim.exe --diff --example examples/visual_diff_demo.py
```

El script debe llamar a `scene.snapshots(...)` cuando existe la variable
`GAANIM_SNAPSHOTS`, como hace `examples/visual_diff_demo.py`. El CLI define esa
variable y ejecuta la captura headless automáticamente.

## CI y tolerancias

```powershell
target/debug/gaanim.exe --diff --example examples/visual_diff_demo.py `
  --no-gui --pixel-threshold 4 --max-changed-ratio 0.0001
```

Usa `--no-capture` para comparar los PNG ya presentes en `current/`, y
`--tests-root <DIR>` sólo si necesitas cambiar la carpeta global por defecto.
El modo manual con `--baseline`, `--current` y `--output` sigue disponible.

El reporte JSON incluye el seek, porcentaje de píxeles modificados, error medio,
delta máximo y el rectángulo del cambio. El visor alterna baseline/actual/diff
con `1`, `2` y `3`.
