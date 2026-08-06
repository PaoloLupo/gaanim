#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Presentaciones de tesis",
  description: "Plantilla institucional, Presenter View y checklist de sustentación",
  route: "/guides/thesis/",
  updated: datetime.today().display(),
  code-langs: (),
)

Gaanim incluye un flujo completo para preparar y ejecutar una sustentación:

- plantilla de tesis 16:9 con nueve slides editables;
- layouts semánticos y revelados controlados por el expositor;
- Presenter View con notas, slide actual, siguiente parada y cronómetro;
- overview navegable por nombre para responder preguntas;
- hot reload durante la edición;
- exportación de respaldo a video;
- capturas y regresión visual determinista.

= 1. Crear la presentación

Con el release en `PATH` (ver #link("/getting-started/installation/")[Instalación]) y Python >=3.12 disponible, genera la presentación. No necesitas `$env:PATH` manual: el launcher detecta el `.venv` o el Python del sistema.

```powershell
gaanim init thesis mi_tesis
```

El comando no reemplaza archivos existentes. Para reemplazar de forma explícita:

```powershell
gaanim init thesis mi_tesis --force
```

Crea `mi_tesis/` con `main.py`, `gaanim.toml`, `assets/`, `exports/` y guía breve. `--force` actualiza solo los archivos del scaffold y conserva recursos propios. La plantilla funciona inmediatamente. La portada reproduce el sistema visual institucional con fondo `#1601FC`, texto blanco y Tw Cen MT. En Windows la fuente se detecta desde `C:\Windows\Fonts\TCM_____.TTF`; no se sustituye silenciosamente.

Todo el sistema visual se configura con un único objeto:

```python
from gaanim import Scene, ThesisTemplate

scene = Scene(1920, 1080, margin=72)
design = ThesisTemplate(
    scene,
    font_path="assets/TwCenMT.ttf",  # opcional en Windows
    logo="assets/logo_ucsp_blanco.svg",
    background="#1601FC",
)
design.cover(
    "TÍTULO DE LA TESIS\nEN DOS O TRES LÍNEAS",
    "NOMBRE DEL AUTOR  •  NOMBRE DEL COAUTOR",
    "AGOSTO 2026",
)
```

También puede definirse `GAANIM_TW_CEN_FONT` para una copia licenciada de la fuente y `GAANIM_THESIS_LOGO` para el logo blanco. Si no hay logo, aparece un marcador vectorial neutro. Busca los textos entre corchetes (`[...]`) y reemplázalos. Colores, títulos, tablas, gráficos, captions y paneles heredan el mismo tema.

Para dev desde fuente:

```powershell
cargo run -p gaanim_launcher -- init thesis mi_tesis
```

= 2. Previsualizar mientras editas

```powershell
gaanim mi_tesis
```

Guardar el archivo dispara hot reload sin reiniciar la ventana. En dev: `cargo run -p gaanim_launcher -- mi_tesis`.

= 3. Modelo de navegación

```python
slide = scene.slide(
    "Resultados",
    notes="Explica primero el hallazgo y después la métrica.",
    layout="two_columns",
)

title = slide.region("title").place(
    scene.title("Resultado principal"),
    Anchor.CENTER,
)

chart = scene.bar_chart([42, 61, 86], labels=["Inicial", "Piloto", "Final"])
scene.play([chart.fade_in().duration(0.5)])
slide.step("hallazgo")
```

El contenido estático aparece al entrar al slide. La misma pulsación que avanza desde la parada anterior inicia la primera animación del siguiente slide. Gaanim pausa en cada `slide.step()`.

Controles:

- Avanzar o continuar animación: `→`, `Enter`, `Espacio` o clic izquierdo
- Volver a la parada anterior: `←` o `Backspace`
- Inicio / final: `Home` / `End`
- Abrir overview en Presenter View: `O`
- Pantalla pública negra / restaurar: `B`
- Pantalla pública blanca / restaurar: `W`
- Salir de pantalla completa: `Esc`

= 4. Presentar con dos pantallas

Los índices de monitor empiezan en cero. Normalmente el proyector es `1`:

```powershell
gaanim --present --monitor 1 mi_tesis
```

La salida pública abre a pantalla completa y Presenter View queda en segunda ventana. Presenter View contiene:

- previsualizadores del slide anterior, actual y siguiente;
- vista inicial del siguiente slide, sin adelantar revelados;
- salto directo al anterior/siguiente haciendo clic en su previsualización;
- título y paso actuales;
- siguiente parada;
- notas del expositor;
- tiempo de la presentación;
- controles anterior, pausa y siguiente;
- overview con miniaturas renderizadas;
- búsqueda y salto directo por nombre de slide;
- acceso directo a cualquier paso nombrado desde la tarjeta.

Las miniaturas se generan de forma asíncrona en un mundo de render aislado. Abrir el overview no mueve el timeline público ni interrumpe animación. Al guardar, hot reload incrementa la revisión y las miniaturas se regeneran.

= 5. Validación visual

Antes de ensayar, ejecuta el preflight semántico:

```powershell
gaanim check mi_tesis
gaanim check mi_tesis --strict
```

Comprueba slides, duración, formato 16:9, notas, pausas nombradas y placeholders sin reemplazar. `--strict` falla también con advertencias, útil en CI.

La plantilla incluye nueve tiempos de captura representativos. Crea la primera línea base cuando apruebes el diseño:

```powershell
gaanim --diff --example mi_tesis --bless --no-gui
```

Después de cada cambio importante:

```powershell
gaanim --diff --example mi_tesis --no-gui
```

Un resultado sano termina con `0 changed, 0 missing`. Ver #link("/guides/visual-regression/")[Regresión visual] para tolerancias y `--pixel-threshold`.

= 6. Exportar un respaldo

La plantilla reconoce `GAANIM_EXPORT` (también `scene.export`):

```powershell
$env:GAANIM_EXPORT = "exports/mi_tesis_respaldo.mp4"
gaanim mi_tesis
Remove-Item Env:GAANIM_EXPORT
```

El respaldo se exporta a 60 FPS con calidad `production`. Requiere FFmpeg en `PATH`. La presentación interactiva y el video usan el mismo timeline.

También puedes exportar un slide específico por su nombre semántico:

```python
scene.export("resultados.mp4", slide="Resultados", quality="production")
```

Para un archivo por cada slide sin añadir otra función al API:

```python
scene.export(
    "slides/{index}-{slide}.mp4",
    slide="*",
    quality="production",
)
```

`{index}` produce `01`, `02`, etc.; `{slide}` usa nombre seguro para archivos. Gaanim crea el directorio padre y usa los límites exactos registrados por `scene.slide(...)`.

= 7. Checklist para el día de la sustentación

1. Compila en release: `cargo build -p gaanim_launcher --release; cargo build -p gaanim_editor --release --bin gaanim-core` o descarga el zip `gaanim-v0.1.0-windows-x64.zip`.
2. Ejecuta la regresión visual y confirma `0 changed, 0 missing`.
3. Exporta y reproduce el MP4 de respaldo.
4. Desactiva notificaciones y suspensión de Windows.
5. Conecta el proyector antes de iniciar Gaanim.
6. Comprueba el índice correcto con `--monitor`.
7. Recorre todos los slides una vez usando solo los controles de presentación.
8. Ejecuta `gaanim check mi_tesis --strict`.
9. Mantén la carpeta `mi_tesis`, el ejecutable y el MP4 en una misma copia de respaldo.

Ver también #link("/guides/projects/")[Proyectos] y #link("/getting-started/installation/")[Instalación].
