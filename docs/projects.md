# Proyectos de video y presentaciones

Gaanim puede abrir un script Python suelto, pero para trabajos reales conviene usar un
proyecto. Un proyecto mantiene código, assets y exportaciones juntos, y puede ejecutarse
desde cualquier directorio.

Si ejecutas el binario de desarrollo directamente en Windows, prepara primero la DLL de
Python para esa sesión:

```powershell
$pyBase = & .\.venv\Scripts\python.exe -c "import sys; print(sys.base_prefix)"
$env:PATH = "$pyBase;$env:PATH"
```

## Crear un proyecto

```powershell
gaanim init video mi-video
gaanim init presentation mi-charla
gaanim init thesis mi-tesis
```

Cada comando crea un starter ejecutable:

```text
mi-proyecto/
├── gaanim.toml
├── main.py
├── README.md
├── assets/
└── exports/
```

- `video` crea una escena animada 16:9 y un flujo de exportación.
- `presentation` crea slides semánticos, notas y pasos para Presenter View.
- `thesis` crea una defensa completa con el template institucional.

`--force` actualiza únicamente los archivos conocidos del scaffold. No borra archivos
propios dentro de `assets/` ni otras carpetas del proyecto.

## Trabajar con la carpeta

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

El visor conserva hot reload sobre el entry point resuelto. Las rutas de assets se
resuelven respecto de `gaanim.toml`, por lo que no dependen del directorio desde el que se
inició Gaanim.

## Manifiesto

El scaffold genera:

```toml
name = "mi-charla"
kind = "presentation"
entry = "main.py"
assets_dir = "assets"
output_dir = "exports"
```

`entry` debe ser una ruta relativa que permanezca dentro del proyecto. `kind` documenta
la intención (`video`, `presentation` o `thesis`). El CLI usa `entry`; la escena carga
`assets_dir` mediante `scene.load_project(...)`. `output_dir` define la carpeta
convencional para artefactos y será la base de futuros presets de exportación.

## Usar un script existente

Los scripts continúan siendo compatibles:

```powershell
gaanim examples/mi_escena.py
```

Para convertir uno en proyecto, crea la estructura anterior, mueve el script a `main.py`,
añade `scene.load_project(...)` después de construir `Scene` y coloca recursos en
`assets/`. No es obligatorio migrar hasta que necesites portabilidad, presentación,
validación o exportaciones organizadas.
