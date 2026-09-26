#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Capturas y comparación visual",
  description: "Fotogramas exactos de tu escena, comparados contra una versión aprobada",
  route: "/guias/capturas-y-comparacion/",
)

En esta guía aprenderás a comprobar que un cambio en el código no alteró lo
que se ve. `gaanim --diff` captura fotogramas exactos de una escena, los
compara con una versión que ya aprobaste y te muestra las diferencias en un
visor.

= Dónde se guardan las capturas

Cada escena tiene su propia carpeta dentro del proyecto:

```text
tests/visual/<nombre-de-la-escena>/
  baseline/  # capturas aprobadas y manifest.json: guárdalas en git
  current/   # captura de la versión actual: local
  report/    # visor, informe JSON y mapas de calor: local
```

Por ejemplo, `escenas/intro.py` usa automáticamente
`tests/visual/escenas/intro/`. No hace falta escribir las rutas de
`baseline`, `current` ni `report`.

= Flujo normal

+ Elige los instantes que quieres capturar. El script llama a
  `scene.snapshots(...)` cuando existe la variable de entorno
  `GAANIM_SNAPSHOTS`; `gaanim --diff` la define y ejecuta la captura sin abrir
  ventana:

  ```python
  >>>from gaanim import Scene
  >>>scene = Scene(frame=(16, 9))
  import os
  if os.environ.get("GAANIM_SNAPSHOTS"):
      scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.0, 0.5, 1.0])
  ```

+ Cuando la escena se vea como quieres, apruébala. `--bless` guarda la captura
  como versión aprobada (`baseline/`):

  ```bash
  gaanim --diff --example escenas/intro.py --bless
  ```

+ Después de cada cambio, captura otra vez, compara con la versión aprobada y
  abre el visor:

  ```bash
  gaanim --diff --example escenas/intro.py
  ```

`--bless` sobrescribe la versión aprobada: úsalo solo cuando hayas revisado y
aceptado el cambio visual.

Si prefieres capturar las pausas del script, `scene.stops` devuelve cada
`scene.stop()` con su instante y `scene.cursor` da el instante actual de la
escena:

```python
>>>import os
>>>from gaanim import Scene
>>>scene = Scene(frame=(16, 9))
if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [stop.time for stop in scene.stops])
```

= Capturar cada pausa de una presentación

Para revisar una presentación, `--capture-stops` captura el fotograma que se ve
en cada `scene.stop()` sin tocar el script: no hace falta llamar a
`scene.snapshots` ni leer `GAANIM_SNAPSHOTS`. El script solo debe terminar con
`scene.render()`.

```bash
gaanim --diff --example mi-charla --capture-stops --capture-only
gaanim --diff --example mi-charla --capture-stops --stops 12,30 --capture-only
```

- Cada pausa se captura en su instante exacto: las animaciones anteriores ya
  terminaron, y una pausa al final de un segmento conserva ese segmento en
  pantalla.
- Los archivos se llaman `stop_0001.png`, `stop_0002.png`… según la numeración
  global de las pausas (desde 1), así que la versión aprobada y la actual se
  emparejan por pausa aunque cambien los tiempos.
- `--stops` acepta números y rangos (`12,30`, `3-7`) de esa misma numeración.
- `--sections resultados,cierre` y `--from resultados` capturan solo las
  pausas de esos segmentos, claves de `Section` o nombres de `SectionStep`
  (ver #link("/guias/presentaciones/")[Presentaciones]); se combinan con
  `--stops` y conservan la numeración global.
- Además de `manifest.json`, escribe `stops.json` con el número, el instante,
  el segmento, el nombre de la pausa y el archivo de cada captura.
- Funciona con `--bless`, `--capture-only` y `--no-gui`, pero no con
  `--no-capture`.
- Solo compara si la versión aprobada tiene su propio `stops.json`. Si no hay
  versión aprobada, o se hizo con `scene.snapshots`, captura, avisa de que no
  hay nada que comparar y termina con éxito. `--bless` con `--capture-stops`
  reemplaza el `manifest.json` aprobado: no lo uses sobre una versión de
  `scene.snapshots` que quieras conservar.

= Automatizar y tolerar diferencias

```bash
gaanim --diff --example escenas/intro.py --no-gui --pixel-threshold 4 --max-changed-ratio 0.0001
```

- `--no-gui` genera el informe sin abrir el visor, útil en integración
  continua.
- `--no-capture` compara los PNG que ya están en `current/`.
- `--capture-only` escribe `current/` (o la carpeta de `--current <DIR>`) y
  termina sin comparar ni modificar la versión aprobada.
- `--capture-stops` y `--stops <LISTA>` capturan cada `scene.stop()` en lugar
  de usar `scene.snapshots` (ver arriba).
- `--tests-root <DIR>` cambia la carpeta raíz, que por defecto es
  `tests/visual`.
- `--pixel-threshold` y `--max-changed-ratio` fijan cuánto puede cambiar cada
  píxel y qué proporción de píxeles puede cambiar sin que cuente como
  diferencia.

Para comparar dos carpetas cualesquiera, indica las rutas a mano con
`--baseline`, `--current` y `--output`:

```bash
gaanim --diff --baseline capturas/antes --current capturas/despues --output capturas/informe --no-gui
```

= Leer el resultado

El informe JSON incluye, para cada captura, el instante, el porcentaje de
píxeles modificados, el error medio, la diferencia máxima y el rectángulo que
contiene el cambio. En el visor, las teclas `1`, `2` y `3` alternan entre la
versión aprobada, la actual y el mapa de diferencias.
