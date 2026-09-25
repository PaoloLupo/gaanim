#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Capturas y comparación visual",
  description: "Fotogramas exactos de tu escena, comparados contra una versión aprobada",
  route: "/guias/capturas-y-comparacion/",
)

`gaanim --diff` captura fotogramas exactos de una escena y los compara con una
versión que ya aprobaste. Sirve para comprobar que un cambio en el código no
alteró lo que se ve. Las capturas de cada escena viven en una carpeta propia
dentro del proyecto:

```text
tests/visual/<nombre-de-la-escena>/
  baseline/  # PNGs aprobados y manifest.json: se versionan
  current/   # captura de la implementación actual: local
  report/    # visor egui, JSON y heatmaps: local
```

Por ejemplo, `escenas/intro.py` usa automáticamente `tests/visual/escenas/intro/`. No hace falta escribir las rutas de `baseline`, `current` ni `output`.

= Flujo normal

Primero registra la implementación correcta como baseline:

```bash
gaanim --diff --example escenas/intro.py --bless
```

Después, en cada cambio, captura el ejemplo, compara contra ese baseline y abre el visor egui:

```bash
gaanim --diff --example escenas/intro.py
```

El script debe llamar a `scene.snapshots(...)` cuando existe la variable `GAANIM_SNAPSHOTS`. El CLI define esa variable y ejecuta la captura sin abrir ventana:

```python
>>>from gaanim import Scene
>>>scene = Scene(frame=(16, 9))
import os
if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.0, 0.5, 1.0])
```

== Capturar cada pausa de una presentación

Para revisar una presentación, `--capture-stops` captura el fotograma que muestra el presentador en cada `scene.stop()`, sin tocar el script: no hace falta llamar a `scene.snapshots` ni leer `GAANIM_SNAPSHOTS`. El script solo debe terminar con `scene.render()`.

```bash
gaanim --diff --example mi-charla --capture-stops --capture-only
gaanim --diff --example mi-charla --capture-stops --stops 12,30 --capture-only
```

- Cada pausa se captura en su instante exacto: las animaciones anteriores ya terminaron y una pausa al final de un segmento conserva ese segmento en pantalla, sin desfases manuales.
- Los archivos se llaman `stop_0001.png`, `stop_0002.png`, … según la numeración global (desde 1) de las pausas, así que el baseline y la captura actual se emparejan por pausa aunque cambien los tiempos.
- `--stops` acepta números y rangos (`12,30`, `3-7`); los números siguen siendo los de la numeración completa.
- `--sections resultados,cierre` y `--from resultados` capturan solo las pausas de esos segmentos, claves de `Section` o nombres de `SectionStep` (ver #link("/guias/presentaciones/")[Presentaciones]); se combinan con `--stops` y conservan la numeración global.
- Además de `manifest.json`, escribe `stops.json` con número, tiempo, segmento, nombre de la pausa y archivo de cada captura.
- Funciona con `--bless`, `--capture-only` y `--no-gui`; no con `--no-capture`.
- Solo compara si el baseline tiene su propio `stops.json`. Si no hay baseline, o es un baseline de `scene.snapshots`, captura, avisa de que no hay nada que comparar y termina con éxito. `--bless` con `--capture-stops` reemplaza el `manifest.json` del baseline, así que no lo uses sobre un baseline de `scene.snapshots` que quieras conservar.

Si prefieres elegir los tiempos desde el script, `scene.stops` devuelve las pausas con su tiempo absoluto y `scene.cursor` el instante actual de autoría:

```python
>>>import os
>>>from gaanim import Scene
>>>scene = Scene(frame=(16, 9))
if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [stop.time for stop in scene.stops])
```

= Automatizar y tolerar diferencias

```bash
gaanim --diff --example escenas/intro.py --no-gui --pixel-threshold 4 --max-changed-ratio 0.0001
```

- `--no-gui` — genera el informe sin abrir el visor (útil en integración continua).
- `--no-capture` — compara los PNG ya presentes en `current/`.
- `--capture-only` — escribe `current/` (o `--current <DIR>`) y termina sin
  comparar ni modificar un baseline; sirve para diagnóstico y benchmarks.
- `--capture-stops` / `--stops <LISTA>` — captura cada `scene.stop()` en lugar
  de `scene.snapshots` (ver arriba).
- `--tests-root <DIR>` — cambia la carpeta global por defecto.
- `--pixel-threshold` / `--max-changed-ratio` — tolerancias.

El modo manual con `--baseline`, `--current` y `--output` sigue disponible para comparar carpetas arbitrarias:

```bash
gaanim --diff --baseline tests/visual/a/baseline --current tests/visual/a/current --output tests/visual/a/report --no-gui
```

El reporte JSON incluye el seek, porcentaje de píxeles modificados, error medio, delta máximo y rectángulo del cambio. El visor alterna baseline/actual/diff con `1`, `2` y `3`.
